#[cfg(test)]
mod internal_donor_policy_tests {
    use super::*;

    fn two_donor_internal_fixture(
        limits: ImmutableLimits,
    ) -> Result<(Vec<u8>, u64), ImmutableError> {
        let internal_sizes = [
            INTERNAL_MIN_OCCUPANCY + 1,
            INTERNAL_MIN_OCCUPANCY,
            INTERNAL_FANOUT,
        ];
        assert_eq!(internal_sizes, [129, 128, 255]);
        let leaf_count: usize = internal_sizes.iter().sum();
        assert_eq!(leaf_count, 512);
        let object_count = leaf_count
            .checked_mul(LEAF_MIN_OCCUPANCY)
            .ok_or(ImmutableError::Limit("object count"))?;
        assert_eq!(object_count, 47_616);

        let mut output = vec![0_u8; FILE_HEADER_LEN];
        output[..8].copy_from_slice(FILE_MAGIC);
        let mut locators = Vec::with_capacity(object_count);
        allocation_check::<Locator>(object_count, limits)?;
        for object_id in 1..=u64::try_from(object_count)
            .map_err(|_| ImmutableError::Limit("object count"))?
        {
            locators.push(append_object(
                &mut output,
                &ImmutableObjectInput::new(object_id, 1, vec![object_id as u8]),
                limits,
            )?);
        }

        let mut leaves = Vec::with_capacity(leaf_count);
        allocation_check::<PageRef>(leaf_count, limits)?;
        for chunk in locators.chunks(LEAF_MIN_OCCUPANCY) {
            assert_eq!(chunk.len(), LEAF_MIN_OCCUPANCY);
            leaves.push(append_page(&mut output, &encode_leaf(chunk)?, limits)?);
        }
        assert_eq!(leaves.len(), leaf_count);

        let mut internals = Vec::with_capacity(internal_sizes.len());
        let mut start = 0_usize;
        for size in internal_sizes {
            let end = start
                .checked_add(size)
                .ok_or(ImmutableError::Limit("page count"))?;
            internals.push(append_page(
                &mut output,
                &encode_internal(&leaves[start..end], 1)?,
                limits,
            )?);
            start = end;
        }
        assert_eq!(start, leaves.len());
        let root = append_page(&mut output, &encode_internal(&internals, 2)?, limits)?;
        let page_count = leaves
            .len()
            .checked_add(internals.len())
            .and_then(|value| value.checked_add(1))
            .ok_or(ImmutableError::Limit("page count"))?;
        assert_eq!(page_count, 516);
        publish(
            &mut output,
            0,
            &root,
            [0_u8; 32],
            ABSENT_OFFSET,
            page_count,
            limits,
        )?;
        validate_canonical_occupancy(&output, limits)?;

        let target_leaf_index = internal_sizes[0]
            .checked_add(internal_sizes[1])
            .and_then(|value| value.checked_sub(1))
            .ok_or(ImmutableError::Limit("page count"))?;
        let target_object = target_leaf_index
            .checked_add(1)
            .and_then(|value| value.checked_mul(LEAF_MIN_OCCUPANCY))
            .ok_or(ImmutableError::Limit("object count"))?;
        assert_eq!(target_object, 23_901);
        Ok((
            output,
            u64::try_from(target_object)
                .map_err(|_| ImmutableError::Limit("object count"))?,
        ))
    }

    fn active_root_child_occupancies(
        data: &[u8],
        limits: ImmutableLimits,
    ) -> Result<Vec<usize>, ImmutableError> {
        let previous = validate_canonical_internal(data, limits)?;
        let footer = parse_footer(data, previous.footer_offset)?;
        let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
        let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
        let root = root_reference(data, snapshot, limits)?;
        let PendingDeletionNode::Internal { children, .. } =
            load_deletion_node(data, &root, limits)?
        else {
            return Err(ImmutableError::Invalid("internal donor test root"));
        };
        children
            .iter()
            .map(|child| experimental_deletion_node_occupancy(data, child, limits))
            .collect()
    }

    #[test]
    fn fuller_policy_avoids_an_internal_donor_cliff_with_equal_immediate_reward() {
        let limits = ImmutableLimits::default();
        let (fixture, object_id) = two_donor_internal_fixture(limits).expect("valid fixture");
        let original =
            validate_canonical_internal(&fixture, limits).expect("strict valid fixture report");
        assert_eq!(original.public.root_level, 2);
        assert_eq!(original.public.page_count, 516);
        assert_eq!(original.public.object_count, 47_616);
        assert_eq!(
            active_root_child_occupancies(&fixture, limits).expect("root occupancies"),
            vec![129, 128, 255]
        );

        let left_path = inspect_persistent_delete_repair_path_experimental(
            &fixture,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        )
        .expect("left-first repair path");
        let fuller_path = inspect_persistent_delete_repair_path_experimental(
            &fixture,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
        )
        .expect("fuller repair path");
        assert_eq!(left_path.levels.len(), 2);
        assert_eq!(fuller_path.levels.len(), 2);
        assert_eq!(left_path.levels[0], fuller_path.levels[0]);
        assert!(left_path.levels[0].would_merge);

        let left_internal = &left_path.levels[1];
        let fuller_internal = &fuller_path.levels[1];
        for event in [left_internal, fuller_internal] {
            assert_eq!(event.level, 1);
            assert!(event.triggered_by_child_removal);
            assert_eq!(event.target_occupancy_before, INTERNAL_MIN_OCCUPANCY);
            assert_eq!(
                event.target_occupancy_after_local_change,
                INTERNAL_MIN_OCCUPANCY - 1
            );
            assert_eq!(event.left_occupancy, Some(INTERNAL_MIN_OCCUPANCY + 1));
            assert_eq!(event.right_occupancy, Some(INTERNAL_FANOUT));
            assert!(event.would_underflow);
            assert!(!event.would_merge);
        }
        assert_eq!(
            left_internal.selected_donor_direction,
            Some(ExperimentalDeleteBorrowDirection::Left)
        );
        assert_eq!(
            left_internal.selected_donor_occupancy,
            Some(INTERNAL_MIN_OCCUPANCY + 1)
        );
        assert!(left_internal.donor_cliff);
        assert!(left_internal.strictly_fuller_eligible_alternative);

        assert_eq!(
            fuller_internal.selected_donor_direction,
            Some(ExperimentalDeleteBorrowDirection::Right)
        );
        assert_eq!(
            fuller_internal.selected_donor_occupancy,
            Some(INTERNAL_FANOUT)
        );
        assert!(!fuller_internal.donor_cliff);
        assert!(!fuller_internal.strictly_fuller_eligible_alternative);

        let left_result = append_persistent_delete_experimental(
            &fixture,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        )
        .expect("left-first persistent delete");
        let fuller_result = append_persistent_delete_experimental(
            &fixture,
            object_id,
            limits,
            ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
        )
        .expect("fuller persistent delete");

        for result in [&left_result, &fuller_result] {
            assert_eq!(result.report.root_level, 2);
            assert_eq!(result.report.object_count, 47_615);
            assert_eq!(result.report.page_count, 515);
            assert_eq!(result.pages_written, 4);
            assert_eq!(result.pages_reused, 511);
            assert_eq!(
                validate_canonical_occupancy(&result.bytes, limits).expect("canonical result"),
                result.report
            );
        }
        assert_ne!(left_result.bytes, fuller_result.bytes);
        assert_ne!(
            left_result.report.snapshot_digest,
            fuller_result.report.snapshot_digest
        );

        let left_report =
            validate_canonical_internal(&left_result.bytes, limits).expect("left report");
        let fuller_report =
            validate_canonical_internal(&fuller_result.bytes, limits).expect("fuller report");
        assert_eq!(left_report.locators, fuller_report.locators);

        let left_occupancies =
            active_root_child_occupancies(&left_result.bytes, limits).expect("left occupancies");
        let fuller_occupancies =
            active_root_child_occupancies(&fuller_result.bytes, limits).expect("fuller occupancies");
        assert_eq!(left_occupancies, vec![128, 128, 255]);
        assert_eq!(fuller_occupancies, vec![129, 128, 254]);
        assert_eq!(
            left_occupancies
                .iter()
                .filter(|occupancy| **occupancy == INTERNAL_MIN_OCCUPANCY)
                .count(),
            2
        );
        assert_eq!(
            fuller_occupancies
                .iter()
                .filter(|occupancy| **occupancy == INTERNAL_MIN_OCCUPANCY)
                .count(),
            1
        );
    }
}
