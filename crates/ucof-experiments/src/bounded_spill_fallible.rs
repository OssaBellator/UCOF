//! Fallible-input adapter for the bounded spill sorter.
//!
//! The underlying sorter accepts infallible records and rejects every payload whose size differs
//! from the configured non-zero `record_bytes` during intake, before final sorted output can begin.
//! This adapter uses one zero-length record as a private impossible sentinel after an input error,
//! then restores the original input error after the sorter has performed its normal spill cleanup.

use crate::bounded_spill_sort::{
    bounded_spill_sort_to, BoundedSpillRecord, BoundedSpillSortError, BoundedSpillSortLimits,
    BoundedSpillSortReport,
};
use std::path::Path;

#[derive(Debug)]
pub enum BoundedSpillInputError<E> {
    Sort(BoundedSpillSortError),
    Input(E),
}

impl<E: std::fmt::Display> std::fmt::Display for BoundedSpillInputError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sort(error) => write!(formatter, "{error}"),
            Self::Input(error) => write!(formatter, "spill-sort input failed: {error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for BoundedSpillInputError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sort(error) => Some(error),
            Self::Input(error) => Some(error),
        }
    }
}

struct FallibleSpillRecords<'a, I, E> {
    inner: I,
    input_error: &'a mut Option<E>,
    failed: bool,
}

impl<I, E> Iterator for FallibleSpillRecords<'_, I, E>
where
    I: Iterator<Item = Result<BoundedSpillRecord, E>>,
{
    type Item = BoundedSpillRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        match self.inner.next()? {
            Ok(record) => Some(record),
            Err(error) => {
                *self.input_error = Some(error);
                self.failed = true;
                Some(BoundedSpillRecord::new(0, Vec::new()))
            }
        }
    }
}

/// Sorts fallibly acquired records while preserving the underlying sorter's cleanup semantics.
///
/// An input error is guaranteed to stop the sorter during record intake: `BoundedSpillSortLimits`
/// requires `record_bytes > 0`, while the private error sentinel has a zero-length payload. Therefore
/// no final sorted payload byte can be emitted after an input error. Existing initial runs are still
/// owned by the sorter workspace and are removed by its ordinary error cleanup path.
pub fn bounded_spill_sort_fallible_to<I, W, E>(
    directory: &Path,
    records: I,
    output: &mut W,
    limits: BoundedSpillSortLimits,
) -> Result<BoundedSpillSortReport, BoundedSpillInputError<E>>
where
    I: IntoIterator<Item = Result<BoundedSpillRecord, E>>,
    W: std::io::Write,
{
    let mut input_error = None;
    let adapter = FallibleSpillRecords {
        inner: records.into_iter(),
        input_error: &mut input_error,
        failed: false,
    };
    let result = bounded_spill_sort_to(directory, adapter, output, limits);
    if let Some(error) = input_error {
        return Err(BoundedSpillInputError::Input(error));
    }
    result.map_err(BoundedSpillInputError::Sort)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ucof-fallible-spill-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn limits() -> BoundedSpillSortLimits {
        BoundedSpillSortLimits {
            record_bytes: 8,
            run_records: 2,
            max_records: 16,
            max_initial_runs: 8,
            max_open_inputs: 2,
            max_merge_passes: 8,
            max_live_spill_bytes: 16 * 1024,
            max_spill_bytes_written: 64 * 1024,
            max_merge_bytes_read: 64 * 1024,
            max_merge_bytes_written: 64 * 1024,
        }
    }

    #[test]
    fn input_error_after_completed_run_writes_no_output_and_cleans_spill() {
        let directory = TestDirectory::new();
        let records = vec![
            Ok(BoundedSpillRecord::new(2, 2u64.to_le_bytes().to_vec())),
            Ok(BoundedSpillRecord::new(1, 1u64.to_le_bytes().to_vec())),
            Err("source metadata"),
            Ok(BoundedSpillRecord::new(3, 3u64.to_le_bytes().to_vec())),
        ];
        let mut output = Vec::new();
        let error = bounded_spill_sort_fallible_to(&directory.0, records, &mut output, limits())
            .expect_err("input failure must stop sort");
        assert!(matches!(
            error,
            BoundedSpillInputError::Input("source metadata")
        ));
        assert!(output.is_empty());
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }

    #[test]
    fn successful_input_preserves_sort_behavior() {
        let directory = TestDirectory::new();
        let records: Vec<Result<_, &'static str>> = [3u64, 1, 2]
            .into_iter()
            .map(|key| Ok(BoundedSpillRecord::new(key, key.to_le_bytes().to_vec())))
            .collect();
        let mut output = Vec::new();
        let report = bounded_spill_sort_fallible_to(&directory.0, records, &mut output, limits())
            .expect("fallible adapter success");
        assert_eq!(report.output_records, 3);
        assert_eq!(
            output,
            [1u64, 2, 3]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect::<Vec<_>>()
        );
    }
}
