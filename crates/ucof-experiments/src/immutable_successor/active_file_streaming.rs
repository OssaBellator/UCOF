#[derive(Clone, Debug)]
pub struct ImmutableActiveFilePayload<'a> {
    data: &'a [u8],
    object_id: u64,
    kind: u16,
    payload_offset: usize,
    payload_len: usize,
    version: [u8; 32],
    largest_read_request: usize,
}

impl ImmutableActiveFilePayload<'_> {
    #[must_use]
    pub fn largest_read_request(&self) -> usize {
        self.largest_read_request
    }
}

impl ImmutableStreamingPayloadSource for ImmutableActiveFilePayload<'_> {
    fn object_id(&self) -> u64 {
        self.object_id
    }

    fn kind(&self) -> u16 {
        self.kind
    }

    fn logical_len(&self) -> u64 {
        u64::try_from(self.payload_len).expect("validated payload length fits u64")
    }

    fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
        Ok(self.version)
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), &'static str> {
        let relative = usize::try_from(offset).map_err(|_| "payload offset")?;
        let start = self
            .payload_offset
            .checked_add(relative)
            .ok_or("payload range")?;
        let end = start.checked_add(buffer.len()).ok_or("payload range")?;
        let payload_end = self
            .payload_offset
            .checked_add(self.payload_len)
            .ok_or("payload range")?;
        if end > payload_end {
            return Err("payload range");
        }
        buffer.copy_from_slice(self.data.get(start..end).ok_or("payload range")?);
        self.largest_read_request = self.largest_read_request.max(buffer.len());
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableActiveFileStreamingRewriteReport {
    pub source: ImmutableReport,
    pub output: ImmutableSourceStreamingWriteReport,
    pub largest_payload_read_request: usize,
}

/// Strictly validates a complete active file and exposes its active object records as immutable,
/// versioned payload sources without cloning payload bytes.
pub fn active_file_payload_sources(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<(ImmutableReport, Vec<ImmutableActiveFilePayload<'_>>), ImmutableError> {
    let internal = validate_canonical_internal(data, limits)?;
    allocation_check::<ImmutableActiveFilePayload<'_>>(internal.locators.len(), limits)?;
    let mut sources = Vec::with_capacity(internal.locators.len());
    for locator in internal.locators {
        let record_offset = usize_from_u64(locator.record_offset, "active payload record")?;
        let record_len = usize_from_u64(locator.record_len, "active payload record")?;
        if record_len < OBJECT_HEADER_LEN {
            return Err(ImmutableError::Invalid("active payload record"));
        }
        let payload_offset = record_offset
            .checked_add(OBJECT_HEADER_LEN)
            .ok_or(ImmutableError::Invalid("active payload range"))?;
        let payload_len = usize_from_u64(locator.logical_len, "active payload length")?;
        let expected_end = record_offset
            .checked_add(record_len)
            .ok_or(ImmutableError::Invalid("active payload range"))?;
        if payload_offset
            .checked_add(payload_len)
            .is_none_or(|end| end != expected_end || end > data.len())
        {
            return Err(ImmutableError::Invalid("active payload range"));
        }
        sources.push(ImmutableActiveFilePayload {
            data,
            object_id: locator.object_id,
            kind: locator.kind,
            payload_offset,
            payload_len,
            version: locator.digest,
            largest_read_request: 0,
        });
    }
    Ok((internal.public, sources))
}

/// Reissues the active state of a strictly valid file as canonical genesis bytes through a bounded
/// sequential sink.
///
/// Historical commits and inactive object records are not copied. Active payloads remain borrowed
/// from the source file and are read through one bounded reusable buffer. Atomic output visibility
/// still requires private staging because sink or source failure after writing begins is terminal.
pub fn rewrite_active_file_to<W: Write>(
    writer: &mut W,
    data: &[u8],
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
) -> Result<ImmutableActiveFileStreamingRewriteReport, ImmutableSourceStreamingWriteError> {
    let (source, mut payloads) = active_file_payload_sources(data, limits)?;
    let output = write_genesis_sources_to(writer, &mut payloads, options, limits)?;
    if output.output.report.object_count != source.object_count {
        return Err(ImmutableError::Invalid("active streaming object count").into());
    }
    let largest_payload_read_request = payloads
        .iter()
        .map(ImmutableActiveFilePayload::largest_read_request)
        .max()
        .unwrap_or(0);
    Ok(ImmutableActiveFileStreamingRewriteReport {
        source,
        output,
        largest_payload_read_request,
    })
}

#[cfg(test)]
mod active_file_streaming_tests {
    use super::*;

    fn object(object_id: u64, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 23).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("seed"); payload_len],
        )
    }

    #[test]
    fn active_file_rewrite_matches_slice_rewrite_without_payload_clones() {
        let limits = ImmutableLimits::default();
        let inputs: Vec<_> = (1..=400_u64).map(|id| object(id, 257)).collect();
        let genesis = build_genesis(&inputs, limits).expect("genesis");
        let source = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(200, 88, b"active replacement".to_vec()),
            limits,
        )
        .expect("replacement append");
        let expected = rewrite_all(&source, limits).expect("slice rewrite");

        let mut actual = Vec::new();
        let report = rewrite_active_file_to(
            &mut actual,
            &source,
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 113,
                },
                max_source_read_bytes: 31,
            },
            limits,
        )
        .expect("streaming active rewrite");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.source, expected.source);
        assert_eq!(report.output.output.report, expected.output);
        assert_eq!(report.largest_payload_read_request, 31);
        assert_eq!(report.output.largest_source_buffer, 31);
        assert!(report.output.output.largest_write_request <= 113);
    }

    #[test]
    fn invalid_source_fails_before_sink_output() {
        let limits = ImmutableLimits::default();
        let mut source = build_genesis(&[object(1, 64)], limits).expect("genesis");
        source[FILE_HEADER_LEN + OBJECT_HEADER_LEN] ^= 1;
        let mut sink = Vec::new();
        assert!(rewrite_active_file_to(
            &mut sink,
            &source,
            ImmutableSourceStreamingWriteOptions::default(),
            limits,
        )
        .is_err());
        assert!(sink.is_empty());
    }

    #[test]
    fn historical_inactive_payloads_are_not_streamed() {
        let limits = ImmutableLimits::default();
        let genesis = build_genesis(&[object(1, 4_096), object(2, 17)], limits).expect("genesis");
        let source = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(1, 7, b"small-active".to_vec()),
            limits,
        )
        .expect("replacement");
        let mut sink = Vec::new();
        let report = rewrite_active_file_to(
            &mut sink,
            &source,
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions::default(),
                max_source_read_bytes: 64,
            },
            limits,
        )
        .expect("active rewrite");
        assert_eq!(report.output.source_bytes_read, 12 + 17);
        assert_eq!(report.output.output.report.object_count, 2);
    }
}
