//! Non-normative immutable-page successor byte experiment.
//!
//! This module has no compatibility promise and does not allocate a new UCOF
//! epoch. Strict validation is exact-end and never invokes recovery.

include!("immutable_successor/part1.rs");
include!("immutable_successor/part2.rs");
include!("immutable_successor/part3.rs");
include!("immutable_successor/history.rs");
include!("immutable_successor/part4.rs");
include!("immutable_successor/part5.rs");
include!("immutable_successor/batch.rs");
include!("immutable_successor/rewrite.rs");
include!("immutable_successor/semantic_compaction.rs");

#[allow(clippy::len_without_is_empty)]
mod source_api {
    use super::*;

    include!("immutable_successor/source.rs");
    include!("immutable_successor/source_full.rs");
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
