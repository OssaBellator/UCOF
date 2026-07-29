use crate::{CborValue, Error};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordKind {
    Opaque = 1,
    Manifest = 2,
    Directory = 3,
}

impl TryFrom<u16> for RecordKind {
    type Error = Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Opaque),
            2 => Ok(Self::Manifest),
            3 => Ok(Self::Directory),
            other => Err(Error::UnsupportedRecordKind(other)),
        }
    }
}

impl From<RecordKind> for u16 {
    fn from(value: RecordKind) -> Self {
        value as Self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub id: u64,
    pub kind: RecordKind,
    pub offset: u64,
    pub stored_len: u64,
    pub logical_len: u64,
}

impl DirectoryEntry {
    pub(crate) fn to_cbor(&self) -> CborValue {
        CborValue::Map(vec![
            (text("id"), CborValue::Unsigned(self.id)),
            (text("kind"), CborValue::Unsigned(u64::from(u16::from(self.kind)))),
            (text("offset"), CborValue::Unsigned(self.offset)),
            (text("stored_len"), CborValue::Unsigned(self.stored_len)),
            (text("logical_len"), CborValue::Unsigned(self.logical_len)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub roots: Vec<u64>,
    pub required_capabilities: Vec<u64>,
    pub optional_capabilities: Vec<u64>,
}

impl Manifest {
    #[must_use]
    pub fn new(roots: Vec<u64>) -> Self {
        Self {
            roots,
            required_capabilities: Vec::new(),
            optional_capabilities: Vec::new(),
        }
    }

    pub fn validate_shape(&self) -> Result<(), Error> {
        unique_nonzero(&self.roots, "manifest roots")?;
        unique(&self.required_capabilities, "required capabilities")?;
        unique(&self.optional_capabilities, "optional capabilities")?;
        Ok(())
    }

    pub(crate) fn to_cbor(&self) -> CborValue {
        CborValue::Map(vec![
            (
                text("roots"),
                CborValue::Array(
                    self.roots
                        .iter()
                        .copied()
                        .map(CborValue::Unsigned)
                        .collect(),
                ),
            ),
            (
                text("required"),
                CborValue::Array(
                    self.required_capabilities
                        .iter()
                        .copied()
                        .map(CborValue::Unsigned)
                        .collect(),
                ),
            ),
            (
                text("optional"),
                CborValue::Array(
                    self.optional_capabilities
                        .iter()
                        .copied()
                        .map(CborValue::Unsigned)
                        .collect(),
                ),
            ),
        ])
    }
}

pub(crate) fn text(value: &str) -> CborValue {
    CborValue::Text(value.to_owned())
}

fn unique(values: &[u64], context: &'static str) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(*value) {
            return Err(Error::InvalidMetadataSchema(context));
        }
    }
    Ok(())
}

fn unique_nonzero(values: &[u64], context: &'static str) -> Result<(), Error> {
    if values.contains(&0) {
        return Err(Error::InvalidMetadataSchema(context));
    }
    unique(values, context)
}
