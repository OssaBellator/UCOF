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
include!("immutable_successor/streaming_genesis.rs");
include!("immutable_successor/source_streaming_genesis.rs");
include!("immutable_successor/active_file_streaming.rs");
include!("immutable_successor/batch.rs");
include!("immutable_successor/persistent_batch.rs");
include!("immutable_successor/persistent_insert.rs");
include!("immutable_successor/persistent_delete.rs");
include!("immutable_successor/delete_frontier_inspect.rs");
include!("immutable_successor/delete_repair_path_inspect.rs");
#[cfg(test)]
include!("immutable_successor/delete_repair_path_policy_tests.rs");
include!("immutable_successor/persistent_multi_put.rs");
include!("immutable_successor/persistent_mixed.rs");
include!("immutable_successor/persistent_mixed_streaming.rs");
include!("immutable_successor/persistent_replacement_streaming.rs");
include!("immutable_successor/persistent_insert_streaming.rs");
include!("immutable_successor/persistent_delete_streaming.rs");
include!("immutable_successor/persistent_multi_put_streaming.rs");
include!("immutable_successor/persistent_streaming_dispatch.rs");
include!("immutable_successor/rewrite.rs");
include!("immutable_successor/freshness.rs");

#[allow(clippy::len_without_is_empty)]
mod source_api {
    use super::*;

    include!("immutable_successor/source.rs");
    include!("immutable_successor/source_full.rs");
    include!("immutable_successor/source_history_inventory.rs");
    include!("immutable_successor/source_history_rewrite.rs");
    include!("immutable_successor/source_inventory_conversion.rs");

    #[allow(clippy::too_many_arguments)]
    mod source_to_sink_api {
        use super::*;

        include!("immutable_successor/source_to_sink.rs");
        include!("immutable_successor/source_selected_to_sink.rs");
    }

    pub use source_to_sink_api::*;

    mod source_history_to_sink_api {
        use super::*;

        include!("immutable_successor/source_history_to_sink.rs");
        include!("immutable_successor/source_history_selected_to_sink.rs");
        include!("immutable_successor/source_history_chain_to_sink.rs");
        include!("immutable_successor/source_history_chain_owned_cap.rs");
    }

    pub use source_history_to_sink_api::*;

    include!("immutable_successor/source_inventory.rs");

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
            include!("immutable_successor/persistent_source_mixed.rs");
        }

        pub use persistent_source_deletion_api::*;
    }

    pub use persistent_source_replacement_api::*;
}

pub use source_api::*;

mod conditional_source_api {
    use super::*;

    include!("immutable_successor/conditional_source.rs");
    include!("immutable_successor/conditional_retry.rs");
    include!("immutable_successor/conditional_backoff.rs");
    include!("immutable_successor/conditional_http.rs");
    include!("immutable_successor/conditional_wait.rs");
    include!("immutable_successor/conditional_authentication.rs");
    #[cfg(feature = "http-reqwest")]
    include!("immutable_successor/conditional_reqwest.rs");
    #[cfg(feature = "http-reqwest")]
    include!("immutable_successor/conditional_async_retry.rs");
    #[cfg(feature = "http-reqwest")]
    include!("immutable_successor/conditional_async_authentication.rs");
}

pub use conditional_source_api::*;

include!("immutable_successor/persistent_source_mixed_regression.rs");
include!("immutable_successor/persistent_source_copy.rs");
include!("immutable_successor/persistent_versioned_source_copy.rs");
include!("immutable_successor/persistent_staged_publication.rs");

#[cfg(unix)]
mod persistent_unix_api {
    use super::*;

    include!("immutable_successor/persistent_unix_staging.rs");
    include!("immutable_successor/persistent_unix_directory_pinning.rs");
}

#[cfg(unix)]
pub use persistent_unix_api::*;

/// Convenience methods completing the synchronous random-access source contract.
pub trait ImmutableReadAtExt: ImmutableReadAt {
    /// Returns whether the current stable source view has zero bytes.
    fn is_empty(&mut self) -> Result<bool, ImmutableSourceError> {
        Ok(self.len()? == 0)
    }
}

impl<T: ImmutableReadAt + ?Sized> ImmutableReadAtExt for T {}
