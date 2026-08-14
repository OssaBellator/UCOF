#[test]
fn prepared_phase_resolves_metadata_before_canonical_emission() {
    const OBJECTS: u64 = 401;
    let original: Vec<_> = (1..=OBJECTS).rev().map(TinySource::new).collect();
    let limits = ImmutableLimits::default();
    let spill = spill_limits(17, 3);

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        options(),
        limits,
    )
    .expect("baseline writer");

    let directory = TestDirectory::new("prepared-seam");
    let mut sources = original;
    let output = Vec::<u8>::new();
    let preflight = prepare_bounded_preflight(
        &directory.0,
        &mut sources,
        options(),
        limits,
        spill,
    )
    .expect("prepared metadata");

    assert!(output.is_empty());
    assert_eq!(preflight.object_count, usize::try_from(OBJECTS).unwrap());
    assert_eq!(preflight.expected_bytes, baseline.len());
    assert_eq!(preflight.descriptor_spill.output_records, OBJECTS);

    let mut actual = output;
    let evidence = write_prepared_bounded_candidate(
        &mut actual,
        &mut sources,
        &directory.0,
        options(),
        limits,
        preflight,
    )
    .expect("prepared emission");

    assert_eq!(actual, baseline);
    assert_eq!(evidence.output, baseline_report);
    directory.assert_empty();
}
