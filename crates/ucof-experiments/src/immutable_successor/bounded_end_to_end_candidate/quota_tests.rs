#[test]
fn private_storage_plan_accounts_for_every_overlap_window() {
    let mut spill = spill_limits(31, 4);
    spill.max_live_spill_bytes = 100_000;
    let plan = private_storage_plan(2_003, spill).expect("private storage plan");

    assert_eq!(plan.descriptor_bytes, 2_003 * 64);
    assert_eq!(plan.locator_bytes, 2_003 * 72);
    assert_eq!(plan.leaf_ref_bytes, 11 * 64);
    assert_eq!(plan.sorter_plus_descriptor_bytes, 100_000 + 2_003 * 64);
    assert_eq!(plan.descriptor_plus_locator_bytes, 2_003 * (64 + 72));
    assert_eq!(plan.locator_plus_leaf_ref_bytes, 2_003 * 72 + 11 * 64);
    assert_eq!(plan.max_adjacent_page_ref_bytes, 12 * 64);
    assert_eq!(plan.required_bytes, plan.descriptor_plus_locator_bytes);
}

#[test]
fn exact_private_storage_quota_succeeds_and_one_byte_short_fails_before_io() {
    const OBJECTS: u64 = 401;
    let spill = spill_limits(17, 3);
    let plan = private_storage_plan(usize::try_from(OBJECTS).unwrap(), spill)
        .expect("private storage plan");
    let original: Vec<_> = (1..=OBJECTS).rev().map(TinySource::new).collect();

    let short_directory = TestDirectory::new("quota-short");
    let mut short_sources = original.clone();
    let mut short_output = Vec::new();
    let error = write_genesis_sources_with_private_quota_candidate(
        &mut short_output,
        &mut short_sources,
        &short_directory.0,
        options(),
        ImmutableLimits::default(),
        spill,
        plan.required_bytes - 1,
    )
    .expect_err("one byte short must fail");
    assert!(error.contains("private storage limit"));
    assert!(short_output.is_empty());
    short_directory.assert_empty();

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        options(),
        ImmutableLimits::default(),
    )
    .expect("baseline writer");

    let exact_directory = TestDirectory::new("quota-exact");
    let mut exact_sources = original;
    let mut actual = Vec::new();
    let (actual_plan, evidence) = write_genesis_sources_with_private_quota_candidate(
        &mut actual,
        &mut exact_sources,
        &exact_directory.0,
        options(),
        ImmutableLimits::default(),
        spill,
        plan.required_bytes,
    )
    .expect("exact quota must succeed");

    assert_eq!(actual_plan, plan);
    assert_eq!(actual, baseline);
    assert_eq!(evidence.output, baseline_report);
    assert!(evidence.peak_live_retained_stage_bytes <= plan.required_bytes);
    assert!(evidence.descriptor_spill.peak_live_spill_bytes <= spill.max_live_spill_bytes);
    exact_directory.assert_empty();
}
