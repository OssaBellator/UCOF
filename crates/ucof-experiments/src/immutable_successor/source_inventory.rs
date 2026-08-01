/// Random-access source whose strong version identifies one immutable view without ABA reuse.
///
/// Implementations must bind `len` and every successful `read_exact_at` call to the returned version
/// for the lifetime of one assurance operation. Returning to an older token for different bytes is
/// forbidden. The inventory operation checks the version before and after complete strict
/// validation and rejects mixed views.
pub trait ImmutableVersionedReadAt: ImmutableReadAt {
    fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSourceInventoryError {
    Source(ImmutableSourceError),
    VersionChanged,
}

impl fmt::Display for ImmutableSourceInventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "{error}"),
            Self::VersionChanged => write!(formatter, "source version changed during inventory"),
        }
    }
}

impl Error for ImmutableSourceInventoryError {}

impl From<ImmutableSourceError> for ImmutableSourceInventoryError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceActiveObject {
    pub object_id: u64,
    pub kind: u16,
    pub record_offset: u64,
    pub record_len: u64,
    pub logical_len: u64,
    pub object_digest: [u8; 32],
}

impl ImmutableSourceActiveObject {
    pub fn payload_offset(&self) -> Result<u64, ImmutableSourceError> {
        self.record_offset
            .checked_add(
                u64::try_from(OBJECT_HEADER_LEN)
                    .map_err(|_| ImmutableSourceError::Limit("object header"))?,
            )
            .ok_or(ImmutableSourceError::Limit("payload offset"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceActiveInventory {
    pub report: ImmutableReport,
    pub version: [u8; 32],
    pub objects: Vec<ImmutableSourceActiveObject>,
    pub stats: ImmutableSourceStats,
}

/// Strictly validates the exact-end active snapshot through one strongly versioned bounded source
/// and returns authenticated active object descriptors without copying payload bytes.
pub fn inventory_source_at<S: ImmutableVersionedReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceActiveInventory, ImmutableSourceInventoryError> {
    let version = source.strong_version()?;
    let mut reader = SourceReader::new(source, limits)?;
    let envelope = read_lookup_envelope(&mut reader)?;
    let footer_raw = reader.read_vec(envelope.footer_offset, FOOTER_LEN, "footer")?;
    let footer = parse_footer(&footer_raw, 0)?;
    let commit_start = if footer.previous_footer_offset == ABSENT_OFFSET {
        0
    } else {
        usize_from_u64(footer.previous_footer_offset, "previous footer")?
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "previous footer",
            )))?
    };

    let mut visited = HashSet::new();
    let mut stack = vec![envelope.root.clone()];
    let mut locators = Vec::new();
    let mut known_ranges = vec![
        (envelope.snapshot_offset, envelope.footer_offset),
        (envelope.footer_offset, reader.length),
    ];
    while let Some(reference) = stack.pop() {
        read_full_page(
            &mut reader,
            &reference,
            &envelope,
            &mut visited,
            &mut stack,
            &mut locators,
            &mut known_ranges,
        )?;
    }

    let current_pages = visited
        .iter()
        .filter(|offset| **offset >= commit_start)
        .count();
    if footer.page_count_current != u64_from_usize(current_pages)? {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page count",
        ))
        .into());
    }
    locators.sort_by_key(|locator| locator.object_id);
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object order",
        ))
        .into());
    }

    allocation_check::<(usize, usize)>(locators.len(), reader.limits.format)?;
    allocation_check::<ImmutableSourceActiveObject>(locators.len(), reader.limits.format)?;
    let mut object_ranges = Vec::with_capacity(locators.len());
    for locator in &locators {
        let offset = usize_from_u64(locator.record_offset, "object range")?;
        let length = usize_from_u64(locator.record_len, "object range")?;
        let end = offset
            .checked_add(length)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "object range",
            )))?;
        object_ranges.push((offset, end));
    }
    object_ranges.sort_unstable();
    if object_ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object overlap",
        ))
        .into());
    }
    for locator in &locators {
        let result = validate_lookup_object(&mut reader, locator, &envelope, &known_ranges)?;
        if !matches!(result, ImmutableLookupResult::Found { .. }) {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid("object")).into());
        }
    }

    let final_version = reader.source.strong_version()?;
    if final_version != version {
        return Err(ImmutableSourceInventoryError::VersionChanged);
    }

    let report = ImmutableReport {
        sequence: envelope.sequence,
        object_count: locators.len(),
        page_count: visited.len(),
        root_level: envelope.root.level,
        snapshot_digest: envelope.snapshot_digest,
        commit_digest: envelope.commit_digest,
    };
    let objects = locators
        .into_iter()
        .map(|locator| ImmutableSourceActiveObject {
            object_id: locator.object_id,
            kind: locator.kind,
            record_offset: locator.record_offset,
            record_len: locator.record_len,
            logical_len: locator.logical_len,
            object_digest: locator.digest,
        })
        .collect();
    Ok(ImmutableSourceActiveInventory {
        report,
        version,
        objects,
        stats: reader.stats,
    })
}

#[cfg(test)]
mod source_inventory_tests {
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
                version: [7; 32],
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

    fn object(object_id: u64, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 17).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("seed"); payload_len],
        )
    }

    #[test]
    fn inventory_returns_strict_active_descriptors_under_budgets() {
        let format = ImmutableLimits::default();
        let inputs: Vec<_> = (1..=400_u64).map(|id| object(id, 257)).collect();
        let genesis = build_genesis(&inputs, format).expect("genesis");
        let data = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(200, 88, b"active replacement".to_vec()),
            format,
        )
        .expect("replacement");
        let mut source = VersionedMemorySource::new(data);
        let inventory = inventory_source_at(
            &mut source,
            ImmutableSourceLimits {
                max_read_request_bytes: 97,
                hash_block_bytes: 89,
                ..ImmutableSourceLimits::default()
            },
        )
        .expect("active inventory");
        assert_eq!(inventory.report.sequence, 1);
        assert_eq!(inventory.report.object_count, 400);
        assert_eq!(inventory.objects.len(), 400);
        assert_eq!(inventory.objects[0].object_id, 1);
        assert_eq!(inventory.objects[199].object_id, 200);
        assert_eq!(inventory.objects[199].kind, 88);
        assert_eq!(inventory.objects[199].logical_len, 18);
        assert_eq!(inventory.version, [7; 32]);
        assert!(inventory.stats.read_operations > 0);
        assert!(inventory.stats.bytes_hashed > 0);
        assert!(source.largest_request <= 97);
        assert_eq!(
            inventory.objects[199].payload_offset().expect("payload offset"),
            inventory.objects[199].record_offset
                + u64::try_from(OBJECT_HEADER_LEN).expect("header")
        );
    }

    #[test]
    fn version_change_during_validation_is_terminal() {
        let format = ImmutableLimits::default();
        let data = build_genesis(&[object(1, 257)], format).expect("genesis");
        let mut source = VersionedMemorySource::new(data);
        source.mutate_after_read = Some(2);
        assert_eq!(
            inventory_source_at(
                &mut source,
                ImmutableSourceLimits {
                    max_read_request_bytes: 31,
                    hash_block_bytes: 29,
                    ..ImmutableSourceLimits::default()
                },
            ),
            Err(ImmutableSourceInventoryError::VersionChanged)
        );
    }
}
