#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSelectedSourceToSinkError {
    Rewrite(ImmutableSourceToSinkError),
    EmptySelection,
    DuplicateObject(u64),
    MissingObject(u64),
}

impl fmt::Display for ImmutableSelectedSourceToSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rewrite(error) => write!(formatter, "selected source rewrite failed: {error}"),
            Self::EmptySelection => write!(formatter, "selected source rewrite requires an object"),
            Self::DuplicateObject(object_id) => {
                write!(formatter, "selected source object {object_id} is duplicated")
            }
            Self::MissingObject(object_id) => {
                write!(formatter, "selected source object {object_id} was not found")
            }
        }
    }
}

impl Error for ImmutableSelectedSourceToSinkError {}

impl From<ImmutableSourceToSinkError> for ImmutableSelectedSourceToSinkError {
    fn from(error: ImmutableSourceToSinkError) -> Self {
        Self::Rewrite(error)
    }
}

impl From<ImmutableSourceInventoryError> for ImmutableSelectedSourceToSinkError {
    fn from(error: ImmutableSourceInventoryError) -> Self {
        Self::Rewrite(error.into())
    }
}

impl From<ImmutableSourceError> for ImmutableSelectedSourceToSinkError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Rewrite(error.into())
    }
}

impl From<ImmutableStreamingWriteError> for ImmutableSelectedSourceToSinkError {
    fn from(error: ImmutableStreamingWriteError) -> Self {
        Self::Rewrite(error.into())
    }
}

impl From<ImmutableError> for ImmutableSelectedSourceToSinkError {
    fn from(error: ImmutableError) -> Self {
        Self::Rewrite(error.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSelectedSourceToSinkReport {
    pub selected_object_ids: Vec<u64>,
    pub output: ImmutableSourceToSinkReport,
}

fn selected_source_inventory(
    mut inventory: ImmutableSourceActiveInventory,
    selected_object_ids: &[u64],
    limits: ImmutableLimits,
) -> Result<(Vec<u64>, ImmutableSourceActiveInventory), ImmutableSelectedSourceToSinkError> {
    if selected_object_ids.is_empty() {
        return Err(ImmutableSelectedSourceToSinkError::EmptySelection);
    }
    if selected_object_ids.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count").into());
    }
    allocation_check::<u64>(selected_object_ids.len(), limits)?;
    let mut selected = selected_object_ids.to_vec();
    selected.sort_unstable();
    if let Some(pair) = selected.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(ImmutableSelectedSourceToSinkError::DuplicateObject(pair[0]));
    }

    allocation_check::<ImmutableSourceActiveObject>(selected.len(), limits)?;
    let mut objects = Vec::with_capacity(selected.len());
    for object_id in &selected {
        let index = inventory
            .objects
            .binary_search_by_key(object_id, |object| object.object_id)
            .map_err(|_| ImmutableSelectedSourceToSinkError::MissingObject(*object_id))?;
        objects.push(inventory.objects[index].clone());
    }
    inventory.objects = objects;
    Ok((selected, inventory))
}

/// Strictly inventories one stable bounded source and streams the caller-selected active objects
/// into canonical genesis output.
///
/// Complete strict inventory still authenticates every active source object. Selection removes
/// unselected payloads from the second emission pass and from output; it does not remove the initial
/// validation cost. Selection is canonicalized by object identifier and duplicate or missing objects
/// fail before the first output byte.
pub fn rewrite_selected_versioned_source_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    selected_object_ids: &[u64],
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<ImmutableSelectedSourceToSinkReport, ImmutableSelectedSourceToSinkError> {
    let inventory = inventory_source_at(source, source_limits)?;
    let (selected_object_ids, inventory) =
        selected_source_inventory(inventory, selected_object_ids, source_limits.format)?;
    let preflight = preflight_source_to_sink(&inventory, source_limits, options)?;
    let source_report = inventory.report.clone();
    let source_version = inventory.version;
    let inventory_stats = inventory.stats;
    let objects = inventory.objects;

    let mut payload_version_checks = 0_u64;
    check_rewrite_source_version(source, source_version, &mut payload_version_checks)?;
    let mut sink = StreamingSink::new(writer, options.output.max_write_request_bytes)?;
    let mut header = [0_u8; FILE_HEADER_LEN];
    header[..8].copy_from_slice(FILE_MAGIC);
    sink.write_commit_bytes(&header)?;

    let mut buffer = vec![0_u8; preflight.read_chunk];
    let mut locators = Vec::with_capacity(objects.len());
    let mut cumulative_source_stats = inventory_stats;
    cumulative_source_stats.largest_allocation = cumulative_source_stats
        .largest_allocation
        .max(buffer.len());
    let mut largest_payload_read_request = 0_usize;
    for object in &objects {
        locators.push(write_inventory_object(
            &mut sink,
            source,
            object,
            source_version,
            &mut buffer,
            &mut cumulative_source_stats,
            &mut payload_version_checks,
            &mut largest_payload_read_request,
        )?);
    }
    check_rewrite_source_version(source, source_version, &mut payload_version_checks)?;

    if cumulative_source_stats.bytes_read
        != inventory_stats
            .bytes_read
            .checked_add(preflight.payload_bytes)
            .ok_or(ImmutableSourceError::Limit("total bytes"))?
        || cumulative_source_stats.read_operations
            != inventory_stats
                .read_operations
                .checked_add(preflight.payload_read_operations)
                .ok_or(ImmutableSourceError::Limit("read operations"))?
    {
        return Err(ImmutableError::Invalid("source budget accounting").into());
    }

    let (root, page_count) = write_streaming_tree(&mut sink, &locators, source_limits.format)?;
    if page_count != preflight.expected_pages || root.level != preflight.expected_root_level {
        return Err(ImmutableError::Invalid("streaming tree shape").into());
    }
    let mut report = write_streaming_publication(&mut sink, &root, page_count)?;
    report.object_count = locators.len();
    if sink.offset != preflight.expected_bytes {
        return Err(ImmutableError::Invalid("streaming output length").into());
    }

    Ok(ImmutableSelectedSourceToSinkReport {
        selected_object_ids,
        output: ImmutableSourceToSinkReport {
            source: source_report,
            output: ImmutableStreamingWriteReport {
                report,
                bytes_written: sink.offset,
                largest_write_request: sink.largest_write_request,
                locator_entries: locators.len(),
            },
            source_version,
            inventory_stats,
            cumulative_source_stats,
            payload_version_checks,
            largest_payload_read_request,
        },
    })
}

#[cfg(test)]
mod selected_source_to_sink_tests {
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

    fn object(object_id: u64, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 29).expect("kind"),
            vec![u8::try_from(object_id).expect("seed"); payload_len],
        )
    }

    #[test]
    fn selected_source_output_matches_owned_selection_and_reads_selected_payload_twice() {
        let format = ImmutableLimits::default();
        let data = build_genesis(
            &[object(1, 11), object(2, 2_047), object(3, 17), object(4, 19)],
            format,
        )
        .expect("genesis");
        let expected = rewrite_selected(&data, &[1, 3], format).expect("selected rewrite");
        let mut source = VersionedMemorySource {
            data,
            version: [43; 32],
            largest_request: 0,
        };
        let mut actual = Vec::new();
        let report = rewrite_selected_versioned_source_to(
            &mut actual,
            &mut source,
            &[3, 1],
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
        .expect("selected source rewrite");

        assert_eq!(actual, expected.bytes);
        assert_eq!(report.selected_object_ids, vec![1, 3]);
        assert_eq!(report.output.source.object_count, 4);
        assert_eq!(report.output.output.report, expected.output);
        assert_eq!(
            report.output.cumulative_source_stats.bytes_read
                - report.output.inventory_stats.bytes_read,
            28
        );
        assert!(report.output.largest_payload_read_request <= 7);
        assert!(report.output.output.largest_write_request <= 37);
        assert!(source.largest_request <= 31);
    }

    #[test]
    fn invalid_selection_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let data = build_genesis(&[object(1, 8), object(2, 9)], format).expect("genesis");
        for (selected, expected) in [
            (
                Vec::new(),
                ImmutableSelectedSourceToSinkError::EmptySelection,
            ),
            (
                vec![1, 1],
                ImmutableSelectedSourceToSinkError::DuplicateObject(1),
            ),
            (
                vec![3],
                ImmutableSelectedSourceToSinkError::MissingObject(3),
            ),
        ] {
            let mut source = VersionedMemorySource {
                data: data.clone(),
                version: [47; 32],
                largest_request: 0,
            };
            let mut sink = Vec::new();
            assert_eq!(
                rewrite_selected_versioned_source_to(
                    &mut sink,
                    &mut source,
                    &selected,
                    ImmutableSourceLimits::default(),
                    ImmutableSourceStreamingWriteOptions::default(),
                ),
                Err(expected)
            );
            assert!(sink.is_empty());
        }
    }
}
