#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableHistoryToSinkError {
    Source(ImmutableSourceError),
    Rewrite(ImmutableSourceToSinkError),
    SequenceNotFound(u64),
    VersionChanged,
}

impl fmt::Display for ImmutableHistoryToSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "historical source validation failed: {error}"),
            Self::Rewrite(error) => write!(formatter, "historical source rewrite failed: {error}"),
            Self::SequenceNotFound(sequence) => {
                write!(formatter, "historical sequence {sequence} was not found")
            }
            Self::VersionChanged => write!(formatter, "source version changed during historical rewrite"),
        }
    }
}

impl Error for ImmutableHistoryToSinkError {}

impl From<ImmutableSourceError> for ImmutableHistoryToSinkError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableHistoryToSinkReport {
    pub history: ImmutableHistoryReport,
    pub history_stats: ImmutableSourceStats,
    pub selected_prefix_len: u64,
    pub output: ImmutableSourceToSinkReport,
    pub cumulative_source_stats: ImmutableSourceStats,
}

struct VersionLockedPrefix<'a, S> {
    source: &'a mut S,
    length: u64,
    expected_version: [u8; 32],
    version_changed: bool,
}

impl<S: ImmutableReadAt> ImmutableReadAt for VersionLockedPrefix<'_, S> {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        Ok(self.length)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        let length = u64::try_from(buffer.len())
            .map_err(|_| ImmutableSourceError::Limit("read bytes"))?;
        if offset
            .checked_add(length)
            .is_none_or(|end| end > self.length)
        {
            return Err(ImmutableSourceError::Io("historical prefix range"));
        }
        self.source.read_exact_at(offset, buffer)
    }
}

impl<S: ImmutableVersionedReadAt> ImmutableVersionedReadAt for VersionLockedPrefix<'_, S> {
    fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError> {
        let actual = self.source.strong_version()?;
        if actual != self.expected_version {
            self.version_changed = true;
            return Err(ImmutableSourceError::Io("source version changed"));
        }
        Ok(actual)
    }
}

/// Revalidates the complete linked history of one strongly versioned bounded source, selects one
/// exact historical sequence, and streams that prefix's active state into canonical genesis output.
///
/// Full history validation and selected-prefix output share the caller's cumulative source budgets.
/// The source version is fixed before history validation and enforced by the selected prefix during
/// strict inventory and payload emission. The selected output is a new genesis file; historical,
/// offset, commit, extension, provenance, and signature identity are not preserved.
pub fn rewrite_versioned_source_sequence_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    sequence: u64,
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<ImmutableHistoryToSinkReport, ImmutableHistoryToSinkError> {
    let expected_version = source.strong_version()?;
    let history_report = validate_source_history(source, source_limits)?;
    if source.strong_version()? != expected_version {
        return Err(ImmutableHistoryToSinkError::VersionChanged);
    }

    let selected = history_report
        .history
        .entries
        .iter()
        .find(|entry| entry.report.sequence == sequence)
        .cloned()
        .ok_or(ImmutableHistoryToSinkError::SequenceNotFound(sequence))?;
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
    let rewrite_result = rewrite_versioned_source_to(writer, &mut prefix, remaining, options);
    let version_changed = prefix.version_changed;
    let output = match rewrite_result {
        Ok(report) => report,
        Err(_) if version_changed => return Err(ImmutableHistoryToSinkError::VersionChanged),
        Err(error) => return Err(ImmutableHistoryToSinkError::Rewrite(error)),
    };
    if output.source != selected.report || output.source_version != expected_version {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "selected historical source",
        ))
        .into());
    }

    let mut cumulative_source_stats = history_report.stats;
    add_source_stats(&mut cumulative_source_stats, output.cumulative_source_stats)?;
    if cumulative_source_stats.bytes_read > source_limits.max_total_bytes_read
        || cumulative_source_stats.read_operations > source_limits.max_read_operations
    {
        return Err(ImmutableSourceError::Limit("historical source budget").into());
    }

    Ok(ImmutableHistoryToSinkReport {
        history: history_report.history,
        history_stats: history_report.stats,
        selected_prefix_len,
        output,
        cumulative_source_stats,
    })
}

#[cfg(test)]
mod source_history_to_sink_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct VersionedMemorySource {
        data: Vec<u8>,
        version: [u8; 32],
        reads: u64,
        mutate_after_read: Option<u64>,
        largest_request: usize,
    }

    impl VersionedMemorySource {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                version: [29; 32],
                reads: 0,
                mutate_after_read: None,
                largest_request: 0,
            }
        }
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
                object(2, b"two-at-zero"),
                object(3, b"three-at-zero"),
            ],
            format,
        )
        .expect("genesis");
        let sequence_one = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(2, 77, b"two-at-one".to_vec()),
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
    fn selected_historical_sequence_matches_owned_prefix_rewrite() {
        let format = ImmutableLimits::default();
        let data = three_commit_source(format);
        let history = validate_history(&data, format).expect("slice history");
        for sequence in [0_u64, 1, 2] {
            let entry = history
                .entries
                .iter()
                .find(|entry| entry.report.sequence == sequence)
                .expect("historical entry");
            let prefix_len = entry.footer_offset + FOOTER_LEN as u64;
            let expected = rewrite_all(
                &data[..usize::try_from(prefix_len).expect("prefix")],
                format,
            )
            .expect("owned prefix rewrite");
            let mut source = VersionedMemorySource::new(data.clone());
            let mut actual = Vec::new();
            let report = rewrite_versioned_source_sequence_to(
                &mut actual,
                &mut source,
                sequence,
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
                    max_source_read_bytes: 17,
                },
            )
            .expect("historical source rewrite");
            assert_eq!(actual, expected.bytes);
            assert_eq!(report.output.source, expected.source);
            assert_eq!(report.output.output.report, expected.output);
            assert_eq!(report.selected_prefix_len, prefix_len);
            assert_eq!(report.output.source_version, [29; 32]);
            assert!(source.largest_request <= 31);
            assert!(report.output.largest_payload_read_request <= 17);
            assert!(report.output.output.largest_write_request <= 37);
        }
    }

    #[test]
    fn missing_sequence_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let mut source = VersionedMemorySource::new(three_commit_source(format));
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_versioned_source_sequence_to(
                &mut sink,
                &mut source,
                99,
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            ),
            Err(ImmutableHistoryToSinkError::SequenceNotFound(99))
        );
        assert!(sink.is_empty());
    }

    #[test]
    fn version_change_during_history_validation_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let mut source = VersionedMemorySource::new(three_commit_source(format));
        source.mutate_after_read = Some(2);
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_versioned_source_sequence_to(
                &mut sink,
                &mut source,
                1,
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            ),
            Err(ImmutableHistoryToSinkError::VersionChanged)
        );
        assert!(sink.is_empty());
    }
}
