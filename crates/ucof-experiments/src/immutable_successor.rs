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
