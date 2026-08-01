#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSelectedHistoryToSinkError {
    Source(ImmutableSourceError),
    Rewrite(ImmutableSelectedSourceToSinkError),
    SequenceNotFound(u64),
    VersionChanged,
}

impl fmt::Display for ImmutableSelectedHistoryToSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => {
                write!(formatter, "selected historical source validation failed: {error}")
            }
            Self::Rewrite(error) => write!(formatter, "selected historical rewrite failed: {error}"),
            Self::SequenceNotFound(sequence) => {
                write!(formatter, "historical sequence {sequence} was not found")
            }
            Self::VersionChanged => {
                write!(formatter, "source version changed during selected historical rewrite")
            }
        }
    }
}

impl Error for ImmutableSelectedHistoryToSinkError {}

impl From<ImmutableSourceError> for ImmutableSelectedHistoryToSinkError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

impl From<ImmutableSelectedSourceToSinkError> for ImmutableSelectedHistoryToSinkError {
    fn from(error: ImmutableSelectedSourceToSinkError) -> Self {
        Self::Rewrite(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSelectedHistoryToSinkReport {
    pub history: ImmutableHistoryReport,
    pub history_stats: ImmutableSourceStats,
    pub selected_prefix_len: u64,
    pub output: ImmutableSelectedSourceToSinkReport,
    pub cumulative_source_stats: ImmutableSourceStats,
}

/// Revalidates one complete linked history, selects one exact sequence, and streams only the
/// caller-selected active objects from that historical prefix into canonical genesis output.
///
/// History validation, strict selected-prefix inventory, and selected payload emission share one
/// cumulative source budget and one non-ABA version. Complete strict inventory still authenticates
/// every active object in the selected state; only the second payload pass and output are filtered.
pub fn rewrite_selected_versioned_source_sequence_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    sequence: u64,
    selected_object_ids: &[u64],
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<ImmutableSelectedHistoryToSinkReport, ImmutableSelectedHistoryToSinkError> {
    let expected_version = source.strong_version()?;
    let history_report = validate_source_history(source, source_limits)?;
    if source.strong_version()? != expected_version {
        return Err(ImmutableSelectedHistoryToSinkError::VersionChanged);
    }

    let selected = history_report
        .history
        .entries
        .iter()
        .find(|entry| entry.report.sequence == sequence)
        .cloned()
        .ok_or(ImmutableSelectedHistoryToSinkError::SequenceNotFound(sequence))?;
    let selected_prefix_len = selected
        .footer_offset
        .checked_add(u64::try_from(FOOTER_LEN).expect("footer length"))
        .ok_or(ImmutableSourceError::Limit("historical prefix"))?;
    let remaining = remaining_source_limits(source_limits, history_report.stats)?;

    let mut prefix = VersionLockedPrefix {
        source,
        length: selected_prefix_len,
        expected_version,
        version_changed: false,
    };
    let rewrite_result = rewrite_selected_versioned_source_to(
        writer,
        &mut prefix,
        selected_object_ids,
        remaining,
        options,
    );
    let version_changed = prefix.version_changed;
    let output = match rewrite_result {
        Ok(report) => report,
        Err(_) if version_changed => {
            return Err(ImmutableSelectedHistoryToSinkError::VersionChanged)
        }
        Err(error) => return Err(ImmutableSelectedHistoryToSinkError::Rewrite(error)),
    };
    if output.output.source != selected.report || output.output.source_version != expected_version {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "selected historical source",
        ))
        .into());
    }

    let mut cumulative_source_stats = history_report.stats;
    add_source_stats(
        &mut cumulative_source_stats,
        output.output.cumulative_source_stats,
    )?;
    if cumulative_source_stats.bytes_read > source_limits.max_total_bytes_read
        || cumulative_source_stats.read_operations > source_limits.max_read_operations
    {
        return Err(ImmutableSourceError::Limit("historical source budget").into());
    }

    Ok(ImmutableSelectedHistoryToSinkReport {
        history: history_report.history,
        history_stats: history_report.stats,
        selected_prefix_len,
        output,
        cumulative_source_stats,
    })
}

#[cfg(test)]
mod selected_history_to_sink_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct VersionedMemorySource {
        data: Vec<u8>,
        version: [u8; 32],
        reads: u64,
        mutate_after_read: Option<u64>,
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
            if self.mutate_after_read == Some(self.reads) {
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

    fn object(object_id: u64, payload: &[u8]) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 23).expect("kind"),
            payload.to_vec(),
        )
    }

    fn three_commit_source(format: ImmutableLimits) -> Vec<u8> {
        let genesis = build_genesis(
            &[
                object(1, b"one-at-zero"),
                object(2, b"two-at-zero-with-a-large-orphan-payload"),
                object(3, b"three-at-zero"),
                object(4, b"four-at-zero"),
            ],
            format,
        )
        .expect("genesis");
        let sequence_one = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(3, 77, b"three-at-one".to_vec()),
            format,
        )
        .expect("sequence one");
        append_replacement(
            &sequence_one,
            &ImmutableObjectInput::new(1, 78, b"one-at-two".to_vec()),
            format,
        )
        .expect("sequence two")
    }

    #[test]
    fn selected_historical_objects_match_owned_prefix_selection() {
        let format = ImmutableLimits::default();
        let data = three_commit_source(format);
        let history = validate_history(&data, format).expect("slice history");
        let entry = history
            .entries
            .iter()
            .find(|entry| entry.report.sequence == 1)
            .expect("sequence one");
        let prefix_len = entry.footer_offset + FOOTER_LEN as u64;
        let prefix = &data[..usize::try_from(prefix_len).expect("prefix")];
        let expected = rewrite_selected(prefix, &[1, 3, 4], format).expect("owned selection");

        let mut source = VersionedMemorySource {
            data,
            version: [53; 32],
            reads: 0,
            mutate_after_read: None,
            largest_request: 0,
        };
        let mut actual = Vec::new();
        let report = rewrite_selected_versioned_source_sequence_to(
            &mut actual,
            &mut source,
            1,
            &[4, 1, 3],
            ImmutableSourceLimits {
                format,
                max_total_bytes_read: 64 * 1024 * 1024,
                max_read_operations: 1_000_000,
                max_read_request_bytes: 31,
                hash_block_bytes: 29,
            },
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 37,
                },
                max_source_read_bytes: 7,
            },
        )
        .expect("selected history rewrite");

        assert_eq!(actual, expected.bytes);
        assert_eq!(report.selected_prefix_len, prefix_len);
        assert_eq!(report.output.selected_object_ids, vec![1, 3, 4]);
        assert_eq!(report.output.output.source, entry.report);
        assert_eq!(report.output.output.output.report, expected.output);
        assert_eq!(
            report.output.output.cumulative_source_stats.bytes_read
                - report.output.output.inventory_stats.bytes_read,
            u64::try_from(b"one-at-zero".len() + b"three-at-one".len() + b"four-at-zero".len())
                .expect("payload bytes")
        );
        assert!(source.largest_request <= 31);
    }

    #[test]
    fn invalid_historical_selection_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let data = three_commit_source(format);
        let mut source = VersionedMemorySource {
            data,
            version: [59; 32],
            reads: 0,
            mutate_after_read: None,
            largest_request: 0,
        };
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_selected_versioned_source_sequence_to(
                &mut sink,
                &mut source,
                1,
                &[99],
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            ),
            Err(ImmutableSelectedHistoryToSinkError::Rewrite(
                ImmutableSelectedSourceToSinkError::MissingObject(99)
            ))
        );
        assert!(sink.is_empty());
    }

    #[test]
    fn version_change_during_history_validation_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let mut source = VersionedMemorySource {
            data: three_commit_source(format),
            version: [61; 32],
            reads: 0,
            mutate_after_read: Some(2),
            largest_request: 0,
        };
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_selected_versioned_source_sequence_to(
                &mut sink,
                &mut source,
                1,
                &[1],
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            ),
            Err(ImmutableSelectedHistoryToSinkError::VersionChanged)
        );
        assert!(sink.is_empty());
    }
}
