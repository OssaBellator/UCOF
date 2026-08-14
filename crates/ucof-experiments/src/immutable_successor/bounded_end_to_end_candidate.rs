#[cfg(test)]
mod bounded_end_to_end_candidate_tests {
    use super::*;
    use crate::bounded_spill_sort::{
        bounded_spill_sort_to, BoundedSpillRecord, BoundedSpillSortLimits, BoundedSpillSortReport,
    };
    use std::fs::{self, File, OpenOptions};
    use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    mod group_iter {
        include!("../canonical_group_iter_candidate.rs");
    }
    use group_iter::CanonicalGroupSizesIter;

    include!("bounded_end_to_end_candidate/stage.rs");
    include!("bounded_end_to_end_candidate/writer.rs");
    include!("bounded_end_to_end_candidate/quota.rs");
    include!("bounded_end_to_end_candidate/tests.rs");
}
