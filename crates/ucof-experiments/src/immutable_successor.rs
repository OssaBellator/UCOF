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
include!("immutable_successor/rewrite.rs");
