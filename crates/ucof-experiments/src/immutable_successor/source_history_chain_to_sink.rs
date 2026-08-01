use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableHistoryChainStreamingOptions {
    pub max_write_request_bytes: usize,
}

impl Default for ImmutableHistoryChainStreamingOptions {
    fn default() -> Self {
        Self {
            max_write_request_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableHistoryChainStreamingError {
    Source(ImmutableSourceError),
    VersionChanged,
    Io(std::io::ErrorKind),
}

impl std::fmt::Display for ImmutableHistoryChainStreamingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "selected history source failed: {error}"),
            Self::VersionChanged => write!(formatter, "selected history source version changed"),
            Self::Io(kind) => write!(formatter, "selected history sink failed: {kind:?}"),
        }
    }
}

impl std::error::Error for ImmutableHistoryChainStreamingError {}

impl From<ImmutableSourceError> for ImmutableHistoryChainStreamingError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableHistoryChainStreamingReport {
    pub retained: Vec<ImmutableRetainedHistoryEntry>,
    pub source_stats: ImmutableSourceStats,
    pub source_version: [u8; 32],
    pub bytes_written: u64,
    pub largest_write_request: usize,
    pub output_allocation_bytes: usize,
    pub byte_scoped_signatures_preserved: bool,
}

/// Reissues selected linked-history states chronologically and copies the complete new history chain
/// through bounded sequential writes under one strong non-ABA source version.
///
/// Complete linked-history validation, selected-prefix rereads, chronological transition planning,
/// output construction, output validation, version recheck, and write-size validation finish before
/// the first sink write. The source selection is canonicalized chronologically by
/// `rewrite_source_selected_history`. Sink failure after output begins is terminal and returns no
/// success report.
pub fn rewrite_versioned_source_selected_history_to<
    W: std::io::Write,
    S: ImmutableVersionedReadAt,
>(
    writer: &mut W,
    source: &mut S,
    selected_sequences: &[u64],
    source_limits: ImmutableSourceLimits,
    options: ImmutableHistoryChainStreamingOptions,
) -> Result<ImmutableHistoryChainStreamingReport, ImmutableHistoryChainStreamingError> {
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableSourceError::Limit("write request").into());
    }
    let source_version = source.strong_version()?;
    let rewrite = rewrite_source_selected_history(source, selected_sequences, source_limits)?;
    if source.strong_version()? != source_version {
        return Err(ImmutableHistoryChainStreamingError::VersionChanged);
    }
    let bytes_written = u64::try_from(rewrite.bytes.len())
        .map_err(|_| ImmutableSourceError::Limit("output bytes"))?;
    let mut largest_write_request = 0_usize;
    for chunk in rewrite.bytes.chunks(options.max_write_request_bytes) {
        largest_write_request = largest_write_request.max(chunk.len());
        writer
            .write_all(chunk)
            .map_err(|error| ImmutableHistoryChainStreamingError::Io(error.kind()))?;
    }
    Ok(ImmutableHistoryChainStreamingReport {
        retained: rewrite.retained,
        source_stats: rewrite.stats,
        source_version,
        bytes_written,
        largest_write_request,
        output_allocation_bytes: rewrite.bytes.len(),
        byte_scoped_signatures_preserved: rewrite.byte_scoped_signatures_preserved,
    })
}

#[cfg(test)]
mod source_history_chain_to_sink_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct VersionedMemorySource {
        data: Vec<u8>,
        version: [u8; 32],
        reads: u64,
        mutate_after: Option<u64>,
        largest_request: usize,
    }

    impl ImmutableReadAt for VersionedMemorySource {
        fn len(&mut self) -> Result<u64, ImmutableSourceError> {
            u64::try_from(self.data.len()).map_err(|_| ImmutableSourceError::Limit("length"))
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<(), ImmutableSourceError> {
            let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
            let end = start
                .checked_add(buffer.len())
                .ok_or(ImmutableSourceError::Io("range"))?;
            buffer.copy_from_slice(
                self.data
                    .get(start..end)
                    .ok_or(ImmutableSourceError::Io("range"))?,
            );
            self.reads += 1;
            self.largest_request = self.largest_request.max(buffer.len());
            if self.mutate_after == Some(self.reads) {
                self.version[0] ^= 1;
            }
            Ok(())
        }
    }

    impl ImmutableVersionedReadAt for VersionedMemorySource {
        fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError> {
            Ok(self.version)
        }
    }

    fn object(object_id: u64, seed: u8) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::from(1 + seed % 31),
            vec![seed; 1 + usize::from(seed % 32)],
        )
    }

    fn three_commit_source(format: ImmutableLimits) -> Vec<u8> {
        let genesis = build_genesis(&[object(1, 11), object(2, 12), object(3, 13)], format)
            .expect("genesis");
        let sequence_one = append_replacement(&genesis, &object(2, 21), format)
            .expect("sequence one");
        append_batch(
            &sequence_one,
            &[
                ImmutableBatchOperation::Delete(1),
                ImmutableBatchOperation::Put(object(4, 31)),
            ],
            format,
        )
        .expect("sequence two")
    }

    #[test]
    fn versioned_selected_history_matches_owned_rewrite_and_bounds_writes() {
        let format = ImmutableLimits::default();
        let data = three_commit_source(format);
        let limits = ImmutableSourceLimits {
            format,
            max_total_bytes_read: 64 * 1024 * 1024,
            max_read_operations: 1_000_000,
            max_read_request_bytes: 29,
            hash_block_bytes: 23,
        };
        let mut expected_source = VersionedMemorySource {
            data: data.clone(),
            version: [89; 32],
            reads: 0,
            mutate_after: None,
            largest_request: 0,
        };
        let expected = rewrite_source_selected_history(&mut expected_source, &[2, 0], limits)
            .expect("owned history rewrite");

        let mut source = VersionedMemorySource {
            data,
            version: [89; 32],
            reads: 0,
            mutate_after: None,
            largest_request: 0,
        };
        let mut actual = Vec::new();
        let report = rewrite_versioned_source_selected_history_to(
            &mut actual,
            &mut source,
            &[2, 0],
            limits,
            ImmutableHistoryChainStreamingOptions {
                max_write_request_bytes: 31,
            },
        )
        .expect("versioned history chain");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.retained, expected.retained);
        assert_eq!(report.source_stats, expected.stats);
        assert_eq!(report.bytes_written, actual.len() as u64);
        assert_eq!(report.output_allocation_bytes, actual.len());
        assert!(report.largest_write_request <= 31);
        assert!(source.largest_request <= 29);
        assert_eq!(
            validate_history(&actual, format)
                .expect("output history")
                .entries
                .len(),
            2
        );
    }

    #[test]
    fn invalid_selection_and_version_change_leave_sink_untouched() {
        let format = ImmutableLimits::default();
        let data = three_commit_source(format);
        let limits = ImmutableSourceLimits {
            format,
            max_total_bytes_read: 64 * 1024 * 1024,
            max_read_operations: 1_000_000,
            max_read_request_bytes: 31,
            hash_block_bytes: 29,
        };

        let mut missing = VersionedMemorySource {
            data: data.clone(),
            version: [97; 32],
            reads: 0,
            mutate_after: None,
            largest_request: 0,
        };
        let mut sink = Vec::new();
        assert!(rewrite_versioned_source_selected_history_to(
            &mut sink,
            &mut missing,
            &[9],
            limits,
            ImmutableHistoryChainStreamingOptions::default(),
        )
        .is_err());
        assert!(sink.is_empty());

        let mut unstable = VersionedMemorySource {
            data,
            version: [101; 32],
            reads: 0,
            mutate_after: Some(2),
            largest_request: 0,
        };
        assert_eq!(
            rewrite_versioned_source_selected_history_to(
                &mut sink,
                &mut unstable,
                &[0, 2],
                limits,
                ImmutableHistoryChainStreamingOptions::default(),
            ),
            Err(ImmutableHistoryChainStreamingError::VersionChanged)
        );
        assert!(sink.is_empty());
    }
}
