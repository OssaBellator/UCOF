/// Revalidates one strongly versioned linked history, selects one exact sequence, and streams only
/// the requested active objects from that historical state into canonical genesis output.
///
/// Complete history and selected-prefix inventory validation occur before output. Selection is
/// canonicalized by identifier and must be non-empty, duplicate-free, and present in the selected
/// historical state. Only selected payloads are reread for emission, although strict inventory
/// validation may read every active payload in the selected prefix.
pub fn rewrite_versioned_source_sequence_selected_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    sequence: u64,
    selected_ids: &[u64],
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
    let rewrite_result = rewrite_versioned_source_selected_to(
        writer,
        &mut prefix,
        selected_ids,
        remaining,
        options,
    );
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
mod source_history_selected_to_sink_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct VersionedMemorySource {
        data: Vec<u8>,
        version: [u8; 32],
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
            self.largest_request = self.largest_request.max(buffer.len());
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
            u16::try_from(1 + object_id % 31).expect("kind"),
            payload.to_vec(),
        )
    }

    fn two_commit_source(format: ImmutableLimits) -> Vec<u8> {
        let genesis = build_genesis(
            &[
                object(1, b"one-at-zero"),
                object(2, &[2; 4_096]),
                object(3, b"three-at-zero"),
                object(4, b"four-at-zero"),
            ],
            format,
        )
        .expect("genesis");
        append_replacement(
            &genesis,
            &ImmutableObjectInput::new(3, 77, b"three-at-one".to_vec()),
            format,
        )
        .expect("sequence one")
    }

    #[test]
    fn selected_historical_objects_match_owned_prefix_selection() {
        let format = ImmutableLimits::default();
        let data = two_commit_source(format);
        let history = validate_history(&data, format).expect("history");
        let entry = history
            .entries
            .iter()
            .find(|entry| entry.report.sequence == 0)
            .expect("sequence zero");
        let prefix_len = entry.footer_offset + FOOTER_LEN as u64;
        let prefix = &data[..usize::try_from(prefix_len).expect("prefix")];
        let expected = rewrite_selected(prefix, &[4, 1, 3], format).expect("owned selection");

        let mut source = VersionedMemorySource {
            data,
            version: [53; 32],
            largest_request: 0,
        };
        let mut actual = Vec::new();
        let report = rewrite_versioned_source_sequence_selected_to(
            &mut actual,
            &mut source,
            0,
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
                max_source_read_bytes: 17,
            },
        )
        .expect("selected historical rewrite");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.output.output.report, expected.output);
        assert_eq!(report.output.output.locator_entries, 3);
        assert_eq!(
            report.output.cumulative_source_stats.bytes_read
                - report.output.inventory_stats.bytes_read,
            11 + 13 + 12
        );
        assert!(report.output.largest_payload_read_request <= 17);
        assert!(source.largest_request <= 31);
    }

    #[test]
    fn invalid_historical_selection_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let data = two_commit_source(format);
        for selected in [vec![1, 9], vec![1, 1], Vec::new()] {
            let mut source = VersionedMemorySource {
                data: data.clone(),
                version: [59; 32],
                largest_request: 0,
            };
            let mut sink = Vec::new();
            assert!(rewrite_versioned_source_sequence_selected_to(
                &mut sink,
                &mut source,
                1,
                &selected,
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            )
            .is_err());
            assert!(sink.is_empty());
        }
    }
}
