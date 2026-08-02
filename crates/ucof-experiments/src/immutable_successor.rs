//! Non-normative immutable-page successor byte experiment.
//!
//! This module has no compatibility promise and does not allocate a new UCOF
//! epoch. Strict validation is exact-end and never invokes recovery.

include!("immutable_successor/part1.rs");
include!("immutable_successor/part2.rs");
include!("immutable_successor/part3.rs");
include!("immutable_successor/history.rs");
include!("immutable_successor/part4.rs");
include!("immutable_successor/occupancy.rs");
include!("immutable_successor/part5.rs");
include!("immutable_successor/batch.rs");
include!("immutable_successor/persistent_batch.rs");
include!("immutable_successor/persistent_insert.rs");
include!("immutable_successor/persistent_delete.rs");
include!("immutable_successor/persistent_multi_put.rs");
include!("immutable_successor/persistent_mixed.rs");
include!("immutable_successor/persistent_mixed_streaming.rs");
include!("immutable_successor/persistent_replacement_streaming.rs");
include!("immutable_successor/persistent_insert_streaming.rs");
include!("immutable_successor/persistent_delete_streaming.rs");
include!("immutable_successor/persistent_multi_put_streaming.rs");
include!("immutable_successor/persistent_streaming_dispatch.rs");
include!("immutable_successor/rewrite.rs");

#[allow(clippy::len_without_is_empty)]
mod source_api {
    use super::*;

    include!("immutable_successor/source.rs");
    include!("immutable_successor/source_full.rs");

    mod persistent_source_replacement_api {
        #![allow(clippy::type_complexity)]

        use super::*;

        include!("immutable_successor/persistent_source_replacement.rs");
        include!("immutable_successor/persistent_source_insertion.rs");
        include!("immutable_successor/persistent_source_insertion_error.rs");

        mod persistent_source_deletion_api {
            use super::*;

            fn persistent_source_canonical_envelope<S: ImmutableReadAt>(
                source: &mut S,
                limits: ImmutableSourceLimits,
                expected: &ImmutableReport,
            ) -> Result<(LookupEnvelope, ImmutableSourceStats), ImmutableSourceError> {
                let (mut envelope, mut stats) =
                    super::persistent_source_canonical_envelope(source, limits, expected)?;
                let extra_limits = remaining_source_limits(limits, stats)?;
                let mut reader = SourceReader::new(source, extra_limits)?;
                let page =
                    reader.read_vec(envelope.root.offset, PAGE_SIZE, "deletion root page")?;
                reader.stats.bytes_hashed = reader
                    .stats
                    .bytes_hashed
                    .checked_add(
                        u64::try_from(page.len())
                            .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
                    )
                    .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
                if digest(&[PAGE_DOMAIN, &page]) != envelope.root.digest
                    || &page[..8] != PAGE_MAGIC
                    || page[9] != envelope.root.level
                {
                    return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                        "deletion root page",
                    )));
                }
                envelope.root.range = Some((
                    u64_at(&page, 20, "deletion root page")?,
                    u64_at(&page, 28, "deletion root page")?,
                ));
                add_source_stats(&mut stats, reader.stats)?;
                Ok((envelope, stats))
            }

            include!("immutable_successor/persistent_source_deletion.rs");
            include!("immutable_successor/persistent_source_multi_put.rs");
        }

        pub use persistent_source_deletion_api::*;
    }

    pub use persistent_source_replacement_api::*;
}

pub use source_api::*;

include!("immutable_successor/persistent_source_copy.rs");
include!("immutable_successor/persistent_versioned_source_copy.rs");

/// Convenience methods completing the synchronous random-access source contract.
pub trait ImmutableReadAtExt: ImmutableReadAt {
    /// Returns whether the current stable source view has zero bytes.
    fn is_empty(&mut self) -> Result<bool, ImmutableSourceError> {
        Ok(self.len()? == 0)
    }
}

impl<T: ImmutableReadAt + ?Sized> ImmutableReadAtExt for T {}
