#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableRetainedHistoryEntry {
    pub source: ImmutableReport,
    pub output: ImmutableReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceHistoryRewriteReport {
    pub bytes: Vec<u8>,
    pub retained: Vec<ImmutableRetainedHistoryEntry>,
    pub stats: ImmutableSourceStats,
    pub byte_scoped_signatures_preserved: bool,
}

fn history_selection_error() -> ImmutableSourceError {
    ImmutableSourceError::Format(ImmutableError::Invalid("history selection"))
}

fn source_snapshot_inputs<S: ImmutableReadAt>(
    source: &mut S,
    prefix_len: u64,
    expected: &ImmutableReport,
    limits: ImmutableSourceLimits,
    total_stats: &mut ImmutableSourceStats,
) -> Result<Vec<ImmutableObjectInput>, ImmutableSourceError> {
    let call_limits = remaining_source_limits(limits, *total_stats)?;
    let mut prefix = PrefixSource {
        inner: source,
        length: prefix_len,
        limits: call_limits,
        stats: ImmutableSourceStats::default(),
    };
    let inventory = validated_source_inventory(&mut prefix, call_limits)?;
    if &inventory.report != expected {
        return Err(ImmutableSourceError::Io("source changed"));
    }

    allocation_check::<ImmutableObjectInput>(inventory.locators.len(), limits.format)?;
    let mut snapshot_stats = inventory.stats;
    let mut inputs = Vec::with_capacity(inventory.locators.len());
    for locator in &inventory.locators {
        inputs.push(source_input_from_locator(
            &mut prefix,
            locator,
            call_limits,
            &mut snapshot_stats,
        )?);
    }
    add_source_stats(total_stats, snapshot_stats)?;
    Ok(inputs)
}

fn history_transition_operations(
    previous: &[ImmutableObjectInput],
    next: &[ImmutableObjectInput],
    limits: ImmutableLimits,
) -> Result<Vec<ImmutableBatchOperation>, ImmutableSourceError> {
    let capacity = previous
        .len()
        .checked_add(next.len())
        .ok_or(ImmutableSourceError::Format(ImmutableError::Limit(
            "object count",
        )))?;
    allocation_check::<ImmutableBatchOperation>(capacity, limits)?;
    let mut operations = Vec::with_capacity(capacity);
    let mut previous_index = 0_usize;
    let mut next_index = 0_usize;

    while previous_index < previous.len() || next_index < next.len() {
        match (previous.get(previous_index), next.get(next_index)) {
            (Some(old), Some(new)) if old.object_id < new.object_id => {
                operations.push(ImmutableBatchOperation::Delete(old.object_id));
                previous_index += 1;
            }
            (Some(old), Some(new)) if old.object_id == new.object_id => {
                if old.kind != new.kind || old.payload != new.payload {
                    operations.push(ImmutableBatchOperation::Put(new.clone()));
                }
                previous_index += 1;
                next_index += 1;
            }
            (Some(_), Some(new)) => {
                operations.push(ImmutableBatchOperation::Put(new.clone()));
                next_index += 1;
            }
            (Some(old), None) => {
                operations.push(ImmutableBatchOperation::Delete(old.object_id));
                previous_index += 1;
            }
            (None, Some(new)) => {
                operations.push(ImmutableBatchOperation::Put(new.clone()));
                next_index += 1;
            }
            (None, None) => break,
        }
    }

    if operations.is_empty() {
        let first = next.first().ok_or_else(history_selection_error)?;
        operations.push(ImmutableBatchOperation::Put(first.clone()));
    }
    Ok(operations)
}

/// Rewrites explicitly selected verified source snapshots into a new chronological history chain.
///
/// Selected source sequences are canonicalized in ascending order. The oldest selected snapshot
/// becomes output sequence zero; each later selected state becomes one subsequent commit. Original
/// sequence numbers and byte identities are reported as mappings rather than copied into the new
/// chain. When two selected snapshots have identical active semantic state, the writer reissues one
/// unchanged object so the selected snapshot boundary remains represented.
pub fn rewrite_source_selected_history<S: ImmutableReadAt>(
    source: &mut S,
    selected_sequences: &[u64],
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceHistoryRewriteReport, ImmutableSourceError> {
    if selected_sequences.is_empty()
        || selected_sequences.len() > limits.format.max_history_entries
    {
        return Err(history_selection_error());
    }
    allocation_check::<u64>(selected_sequences.len(), limits.format)?;
    let mut selected = selected_sequences.to_vec();
    selected.sort_unstable();
    if selected.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(history_selection_error());
    }

    let history = validate_source_history(source, limits)?;
    let mut stats = history.stats;
    allocation_check::<ImmutableRetainedHistoryEntry>(selected.len(), limits.format)?;
    let mut retained = Vec::with_capacity(selected.len());
    let mut output = Vec::new();
    let mut previous_inputs: Option<Vec<ImmutableObjectInput>> = None;

    for source_sequence in selected {
        let entry = history
            .history
            .entries
            .iter()
            .find(|entry| entry.report.sequence == source_sequence)
            .ok_or_else(history_selection_error)?;
        let prefix_len = entry
            .footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).expect("footer length"))
            .ok_or_else(history_selection_error)?;
        let inputs = source_snapshot_inputs(
            source,
            prefix_len,
            &entry.report,
            limits,
            &mut stats,
        )?;

        output = if let Some(previous) = &previous_inputs {
            let operations = history_transition_operations(previous, &inputs, limits.format)?;
            append_batch(&output, &operations, limits.format)?
        } else {
            build_genesis(&inputs, limits.format)?
        };
        let output_report = validate(&output, limits.format)?;
        retained.push(ImmutableRetainedHistoryEntry {
            source: entry.report.clone(),
            output: output_report,
        });
        previous_inputs = Some(inputs);
    }

    let output_history = validate_history(&output, limits.format)?;
    if output_history.entries.len() != retained.len() {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "history rewrite",
        )));
    }
    Ok(ImmutableSourceHistoryRewriteReport {
        bytes: output,
        retained,
        stats,
        byte_scoped_signatures_preserved: false,
    })
}
