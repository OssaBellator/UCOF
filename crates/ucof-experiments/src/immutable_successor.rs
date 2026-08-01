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
include!("immutable_successor/persistent_multi_put.rs");
include!("immutable_successor/rewrite.rs");

#[allow(clippy::len_without_is_empty)]
mod source_api {
    use super::*;

    include!("immutable_successor/source.rs");
    include!("immutable_successor/source_full.rs");
    include!("immutable_successor/source_inventory_conversion.rs");

    #[allow(clippy::too_many_arguments)]
    mod source_to_sink_api {
        use super::*;

        include!("immutable_successor/source_to_sink.rs");
    }

    pub use source_to_sink_api::*;

    include!("immutable_successor/source_inventory.rs");
}

pub use source_api::*;

/// Convenience methods completing the synchronous random-access source contract.
pub trait ImmutableReadAtExt: ImmutableReadAt {
    /// Returns whether the current stable source view has zero bytes.
    fn is_empty(&mut self) -> Result<bool, ImmutableSourceError> {
        Ok(self.len()? == 0)
    }
}

impl<T: ImmutableReadAt + ?Sized> ImmutableReadAtExt for T {}
