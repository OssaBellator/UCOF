#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableHistoryChainOwnedOutputOptions {
    pub streaming: ImmutableHistoryChainStreamingOptions,
    pub max_owned_output_bytes: usize,
}

impl Default for ImmutableHistoryChainOwnedOutputOptions {
    fn default() -> Self {
        Self {
            streaming: ImmutableHistoryChainStreamingOptions::default(),
            max_owned_output_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Reissues a selected history chain with an explicit cap on the complete owned output allocation.
///
/// The existing selected-history rewriter still owns the complete output before the first sink
/// write. This wrapper narrows `max_output_bytes` only for output construction, so a chain exceeding
/// the caller's owned-output budget fails before any sink write. It does not provide constant-memory
/// history output or reduce the complete source-history validation passes.
pub fn rewrite_versioned_source_selected_history_to_with_owned_output_cap<
    W: std::io::Write,
    S: ImmutableVersionedReadAt,
>(
    writer: &mut W,
    source: &mut S,
    selected_sequences: &[u64],
    mut source_limits: ImmutableSourceLimits,
    options: ImmutableHistoryChainOwnedOutputOptions,
) -> Result<ImmutableHistoryChainStreamingReport, ImmutableHistoryChainStreamingError> {
    if options.max_owned_output_bytes == 0 {
        return Err(ImmutableSourceError::Limit("owned output").into());
    }
    source_limits.format.max_output_bytes = source_limits
        .format
        .max_output_bytes
        .min(options.max_owned_output_bytes);
    rewrite_versioned_source_selected_history_to(
        writer,
        source,
        selected_sequences,
        source_limits,
        options.streaming,
    )
}

#[cfg(test)]
mod source_history_chain_owned_cap_tests {
    use super::*;

    #[derive(Clone)]
    struct VersionedMemorySource {
        bytes: Vec<u8>,
        version: [u8; 32],
    }

    impl ImmutableReadAt for VersionedMemorySource {
        fn len(&mut self) -> Result<u64, ImmutableSourceError> {
            u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
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
                self.bytes
                    .get(start..end)
                    .ok_or(ImmutableSourceError::Io("range"))?,
            );
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
            u16::from(seed % 31 + 1),
            vec![seed; 1 + usize::from(seed % 32)],
        )
    }

    fn history(format: ImmutableLimits) -> Vec<u8> {
        let genesis = build_genesis(&[object(1, 11), object(2, 12)], format).expect("genesis");
        append_replacement(&genesis, &object(2, 21), format).expect("replacement")
    }

    fn limits(format: ImmutableLimits) -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            format,
            max_total_bytes_read: 64 * 1024 * 1024,
            max_read_operations: 1_000_000,
            max_read_request_bytes: 37,
            hash_block_bytes: 31,
        }
    }

    #[test]
    fn exact_owned_output_cap_preserves_bytes_and_report() {
        let format = ImmutableLimits::default();
        let bytes = history(format);
        let mut expected_source = VersionedMemorySource {
            bytes: bytes.clone(),
            version: [31; 32],
        };
        let expected = rewrite_source_selected_history(
            &mut expected_source,
            &[0, 1],
            limits(format),
        )
        .expect("owned history");

        let mut source = VersionedMemorySource {
            bytes,
            version: [31; 32],
        };
        let mut output = Vec::new();
        let report = rewrite_versioned_source_selected_history_to_with_owned_output_cap(
            &mut output,
            &mut source,
            &[1, 0],
            limits(format),
            ImmutableHistoryChainOwnedOutputOptions {
                streaming: ImmutableHistoryChainStreamingOptions {
                    max_write_request_bytes: 17,
                },
                max_owned_output_bytes: expected.bytes.len(),
            },
        )
        .expect("capped history");

        assert_eq!(output, expected.bytes);
        assert_eq!(report.retained, expected.retained);
        assert_eq!(report.output_allocation_bytes, output.len());
        assert!(report.largest_write_request <= 17);
    }

    #[test]
    fn undersized_owned_output_cap_fails_before_sink_write() {
        let format = ImmutableLimits::default();
        let bytes = history(format);
        let mut expected_source = VersionedMemorySource {
            bytes: bytes.clone(),
            version: [37; 32],
        };
        let expected = rewrite_source_selected_history(
            &mut expected_source,
            &[0, 1],
            limits(format),
        )
        .expect("owned history");

        let mut source = VersionedMemorySource {
            bytes,
            version: [37; 32],
        };
        let mut sink = Vec::new();
        assert!(rewrite_versioned_source_selected_history_to_with_owned_output_cap(
            &mut sink,
            &mut source,
            &[0, 1],
            limits(format),
            ImmutableHistoryChainOwnedOutputOptions {
                streaming: ImmutableHistoryChainStreamingOptions::default(),
                max_owned_output_bytes: expected.bytes.len() - 1,
            },
        )
        .is_err());
        assert!(sink.is_empty());
    }
}
