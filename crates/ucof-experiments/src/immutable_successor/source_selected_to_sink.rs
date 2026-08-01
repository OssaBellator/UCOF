fn selected_source_inventory(
    inventory: ImmutableSourceActiveInventory,
    selected_ids: &[u64],
    limits: ImmutableLimits,
) -> Result<ImmutableSourceActiveInventory, ImmutableSourceToSinkError> {
    if selected_ids.is_empty() || selected_ids.len() > limits.max_objects {
        return Err(ImmutableError::Invalid("source selection").into());
    }
    allocation_check::<u64>(selected_ids.len(), limits)?;
    let mut selected = selected_ids.to_vec();
    selected.sort_unstable();
    if selected.first() == Some(&0) || selected.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ImmutableError::Invalid("source selection").into());
    }

    allocation_check::<ImmutableSourceActiveObject>(selected.len(), limits)?;
    let mut objects = Vec::with_capacity(selected.len());
    let mut inventory_index = 0_usize;
    for selected_id in selected {
        while inventory_index < inventory.objects.len()
            && inventory.objects[inventory_index].object_id < selected_id
        {
            inventory_index += 1;
        }
        let object = inventory
            .objects
            .get(inventory_index)
            .filter(|object| object.object_id == selected_id)
            .ok_or(ImmutableError::Invalid("source selection"))?;
        objects.push(object.clone());
        inventory_index += 1;
    }

    Ok(ImmutableSourceActiveInventory {
        report: inventory.report,
        version: inventory.version,
        objects,
        stats: inventory.stats,
    })
}

fn rewrite_selected_inventory_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    inventory: ImmutableSourceActiveInventory,
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<ImmutableSourceToSinkReport, ImmutableSourceToSinkError> {
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

    Ok(ImmutableSourceToSinkReport {
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
    })
}

/// Strictly inventories one stable bounded source and streams exactly the selected active objects
/// into canonical genesis output.
///
/// Selection is canonicalized by object identifier and must be non-empty, duplicate-free, and fully
/// present in the authenticated active inventory. Complete strict inventory validation occurs before
/// output. Only selected payloads are reread for emission, but inventory validation may read all
/// active payloads. Source version, cumulative source budgets, digest equality, and bounded sink
/// behavior match `rewrite_versioned_source_to`.
pub fn rewrite_versioned_source_selected_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    selected_ids: &[u64],
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<ImmutableSourceToSinkReport, ImmutableSourceToSinkError> {
    let inventory = inventory_source_at(source, source_limits)?;
    let inventory = selected_source_inventory(inventory, selected_ids, source_limits.format)?;
    rewrite_selected_inventory_to(writer, source, inventory, source_limits, options)
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
            u16::try_from(1 + object_id % 31).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("seed"); payload_len],
        )
    }

    #[test]
    fn selected_versioned_source_matches_owned_selection() {
        let format = ImmutableLimits::default();
        let data = build_genesis(
            &[object(1, 11), object(2, 4_096), object(3, 17), object(4, 19)],
            format,
        )
        .expect("genesis");
        let expected = rewrite_selected(&data, &[4, 1, 3], format).expect("owned selection");
        let mut source = VersionedMemorySource {
            data,
            version: [43; 32],
            largest_request: 0,
        };
        let mut actual = Vec::new();
        let report = rewrite_versioned_source_selected_to(
            &mut actual,
            &mut source,
            &[4, 1, 3],
            ImmutableSourceLimits {
                format,
                max_total_bytes_read: 16 * 1024 * 1024,
                max_read_operations: 1_000_000,
                max_read_request_bytes: 29,
                hash_block_bytes: 23,
            },
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 31,
                },
                max_source_read_bytes: 13,
            },
        )
        .expect("selected versioned source");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.output.report, expected.output);
        assert_eq!(report.output.locator_entries, 3);
        assert_eq!(
            report.cumulative_source_stats.bytes_read - report.inventory_stats.bytes_read,
            11 + 17 + 19
        );
        assert!(report.largest_payload_read_request <= 13);
        assert!(source.largest_request <= 29);
    }

    #[test]
    fn missing_or_duplicate_selection_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let data = build_genesis(&[object(1, 8), object(2, 9)], format).expect("genesis");
        for selected in [vec![1, 3], vec![1, 1], Vec::new()] {
            let mut source = VersionedMemorySource {
                data: data.clone(),
                version: [47; 32],
                largest_request: 0,
            };
            let mut sink = Vec::new();
            assert!(rewrite_versioned_source_selected_to(
                &mut sink,
                &mut source,
                &selected,
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions::default(),
            )
            .is_err());
            assert!(sink.is_empty());
        }
    }
}
