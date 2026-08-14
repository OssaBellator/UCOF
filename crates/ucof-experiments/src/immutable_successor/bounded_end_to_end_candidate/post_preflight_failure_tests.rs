#[derive(Clone, Debug)]
struct ChangingVersionSource {
    version_calls: u8,
}

impl ChangingVersionSource {
    fn new() -> Self {
        Self { version_calls: 0 }
    }
}

impl ImmutableStreamingPayloadSource for ChangingVersionSource {
    fn object_id(&self) -> u64 {
        1
    }

    fn kind(&self) -> u16 {
        1
    }

    fn logical_len(&self) -> u64 {
        1
    }

    fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
        self.version_calls = self
            .version_calls
            .checked_add(1)
            .ok_or("version call overflow")?;
        if self.version_calls <= 2 {
            Ok([7; 32])
        } else {
            Ok([8; 32])
        }
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), &'static str> {
        if offset != 0 || buffer.len() != 1 {
            return Err("changing source range");
        }
        buffer[0] = 0x5a;
        Ok(())
    }
}

#[test]
fn post_payload_version_change_returns_no_report_and_retires_private_working_state() {
    let directory = TestDirectory::new("post-payload-version");
    let spill = spill_limits(1, 2);
    let plan = private_storage_plan(1, spill).expect("private storage plan");
    let mut sources = [ChangingVersionSource::new()];
    let mut output = Vec::new();

    let error = write_genesis_sources_with_private_quota_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        options(),
        ImmutableLimits::default(),
        spill,
        plan.required_bytes,
    )
    .expect_err("post-payload version change must be terminal");

    assert!(error.contains("version"));
    assert_eq!(sources[0].version_calls, 3);
    assert_eq!(output.len(), FILE_HEADER_LEN + OBJECT_HEADER_LEN + 1);
    assert_eq!(&output[..8], FILE_MAGIC);
    assert_eq!(&output[FILE_HEADER_LEN..FILE_HEADER_LEN + 8], OBJECT_MAGIC);
    directory.assert_empty();
}
