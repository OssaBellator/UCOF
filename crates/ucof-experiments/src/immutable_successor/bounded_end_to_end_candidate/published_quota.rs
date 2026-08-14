#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublishedPrivateStoragePlan {
    working: PrivateStoragePlan,
    output_bytes: u64,
    output_plus_working_bytes: u64,
    required_bytes: u64,
}

fn expected_canonical_output_bytes<S: ImmutableStreamingPayloadSource>(
    sources: &[S],
    limits: ImmutableLimits,
) -> CandidateResult<u64> {
    if sources.is_empty() || sources.len() > limits.max_objects {
        return Err("object count limit".into());
    }
    let mut object_bytes = 0usize;
    for source in sources {
        if source.object_id() == 0 || source.kind() == 0 {
            return Err("invalid object input".into());
        }
        let logical_len =
            usize::try_from(source.logical_len()).map_err(|_| "object size".to_owned())?;
        object_bytes = object_bytes
            .checked_add(
                OBJECT_HEADER_LEN
                    .checked_add(logical_len)
                    .ok_or_else(|| "object size".to_owned())?,
            )
            .ok_or_else(|| "output size".to_owned())?;
    }
    let (pages, _) =
        streaming_tree_shape(sources.len(), limits).map_err(|error| error.to_string())?;
    let page_bytes = pages
        .checked_mul(PAGE_SIZE)
        .ok_or_else(|| "page output size".to_owned())?;
    let bytes = FILE_HEADER_LEN
        .checked_add(object_bytes)
        .and_then(|value| value.checked_add(page_bytes))
        .and_then(|value| value.checked_add(SNAPSHOT_LEN))
        .and_then(|value| value.checked_add(FOOTER_LEN))
        .ok_or_else(|| "output size".to_owned())?;
    if bytes > limits.max_output_bytes {
        return Err("output limit".into());
    }
    if bytes > limits.max_file_bytes {
        return Err("file size limit".into());
    }
    u64::try_from(bytes).map_err(|_| "output size conversion".to_owned())
}

fn published_private_storage_plan<S: ImmutableStreamingPayloadSource>(
    sources: &[S],
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
) -> CandidateResult<PublishedPrivateStoragePlan> {
    let working = private_storage_plan(sources.len(), spill_limits)?;
    let output_bytes = expected_canonical_output_bytes(sources, limits)?;
    let post_preflight_working = working
        .descriptor_plus_locator_bytes
        .max(working.locator_plus_leaf_ref_bytes)
        .max(working.max_adjacent_page_ref_bytes);
    let output_plus_working_bytes = output_bytes
        .checked_add(post_preflight_working)
        .ok_or_else(|| "published private storage overflow".to_owned())?;
    let required_bytes = working
        .sorter_plus_descriptor_bytes
        .max(output_plus_working_bytes);
    Ok(PublishedPrivateStoragePlan {
        working,
        output_bytes,
        output_plus_working_bytes,
        required_bytes,
    })
}

fn enforce_published_private_storage_limit<S: ImmutableStreamingPayloadSource>(
    sources: &[S],
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
) -> CandidateResult<PublishedPrivateStoragePlan> {
    let plan = published_private_storage_plan(sources, limits, spill_limits)?;
    if plan.required_bytes > max_private_storage_bytes {
        return Err("published private storage limit".into());
    }
    Ok(plan)
}
