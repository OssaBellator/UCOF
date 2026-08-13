use crate::bounded_source_descriptor::{
    BoundedSourceDescriptor, BOUNDED_SOURCE_DESCRIPTOR_BYTES,
};
use crate::bounded_source_descriptor_parse::parse_bounded_source_descriptor;
use crate::bounded_spill_fallible::{bounded_spill_sort_fallible_to, BoundedSpillInputError};
use crate::bounded_spill_sort::{
    BoundedSpillRecord, BoundedSpillSortError, BoundedSpillSortLimits, BoundedSpillSortReport,
};
use std::cell::Cell;
use std::io::Write;
use std::path::Path;

#[derive(Debug)]
pub enum BoundedSourceDescriptorError<InputError, VisitError> {
    Input(InputError),
    Sort(BoundedSpillSortError),
    Invalid(&'static str),
    Visit(VisitError),
}

struct DescriptorVisitor<'a, F, VisitError> {
    visit: &'a mut F,
    pending: [u8; BOUNDED_SOURCE_DESCRIPTOR_BYTES],
    pending_len: usize,
    visit_error: Option<VisitError>,
    invalid: Option<&'static str>,
}

impl<F, VisitError> Write for DescriptorVisitor<'_, F, VisitError>
where
    F: FnMut(BoundedSourceDescriptor) -> Result<(), VisitError>,
{
    fn write(&mut self, mut bytes: &[u8]) -> std::io::Result<usize> {
        let original = bytes.len();
        while !bytes.is_empty() {
            let take = (BOUNDED_SOURCE_DESCRIPTOR_BYTES - self.pending_len).min(bytes.len());
            self.pending[self.pending_len..self.pending_len + take].copy_from_slice(&bytes[..take]);
            self.pending_len += take;
            bytes = &bytes[take..];
            if self.pending_len == BOUNDED_SOURCE_DESCRIPTOR_BYTES {
                let descriptor = match parse_bounded_source_descriptor(&self.pending) {
                    Ok(descriptor) => descriptor,
                    Err(label) => {
                        self.invalid = Some(label);
                        return Err(std::io::Error::other("invalid sorted source descriptor"));
                    }
                };
                if let Err(error) = (self.visit)(descriptor) {
                    self.visit_error = Some(error);
                    return Err(std::io::Error::other("source descriptor visitor failed"));
                }
                self.pending_len = 0;
            }
        }
        Ok(original)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn visit_bounded_source_descriptors<I, InputError, VisitError, F>(
    directory: &Path,
    descriptors: I,
    limits: BoundedSpillSortLimits,
    mut visit: F,
) -> Result<BoundedSpillSortReport, BoundedSourceDescriptorError<InputError, VisitError>>
where
    I: IntoIterator<Item = Result<BoundedSourceDescriptor, InputError>>,
    F: FnMut(BoundedSourceDescriptor) -> Result<(), VisitError>,
{
    if limits.record_bytes != BOUNDED_SOURCE_DESCRIPTOR_BYTES {
        return Err(BoundedSourceDescriptorError::Invalid(
            "spill record byte configuration",
        ));
    }
    let invalid = Cell::new(None);
    let records = descriptors.into_iter().map(|result| {
        result.map(|descriptor| match descriptor.encode() {
            Ok(bytes) => BoundedSpillRecord::new(descriptor.object_id, bytes.to_vec()),
            Err(label) => {
                invalid.set(Some(label));
                BoundedSpillRecord::new(0, Vec::new())
            }
        })
    });
    let mut writer = DescriptorVisitor {
        visit: &mut visit,
        pending: [0u8; BOUNDED_SOURCE_DESCRIPTOR_BYTES],
        pending_len: 0,
        visit_error: None,
        invalid: None,
    };
    let result = bounded_spill_sort_fallible_to(directory, records, &mut writer, limits);
    if let Some(label) = invalid.get().or(writer.invalid) {
        return Err(BoundedSourceDescriptorError::Invalid(label));
    }
    if let Some(error) = writer.visit_error {
        return Err(BoundedSourceDescriptorError::Visit(error));
    }
    let report = match result {
        Ok(report) => report,
        Err(BoundedSpillInputError::Input(error)) => {
            return Err(BoundedSourceDescriptorError::Input(error));
        }
        Err(BoundedSpillInputError::Sort(error)) => {
            return Err(BoundedSourceDescriptorError::Sort(error));
        }
    };
    if writer.pending_len != 0 {
        return Err(BoundedSourceDescriptorError::Invalid(
            "sorted descriptor framing",
        ));
    }
    Ok(report)
}
