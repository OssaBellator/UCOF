use std::collections::BTreeMap;

use ucof_experiments::immutable_successor::{
    build_genesis, rewrite_all, rewrite_selected, rewrite_source_all, rewrite_source_selected,
    semantic_compact, semantic_compact_source, DependencyResolution, ImmutableCompactionLimits,
    ImmutableDependencyResolver, ImmutableLimits, ImmutableObjectInput, ImmutableReadAt,
    ImmutableSourceError, ImmutableSourceLimits, UnknownDependencyPolicy,
};

#[derive(Debug)]
struct TracingSource {
    data: Vec<u8>,
    read_operations: usize,
    bytes_read: usize,
    largest_request: usize,
}

impl TracingSource {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            read_operations: 0,
            bytes_read: 0,
            largest_request: 0,
        }
    }
}

impl ImmutableReadAt for TracingSource {
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
        let source = self
            .data
            .get(start..end)
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(source);
        self.read_operations += 1;
        self.bytes_read += buffer.len();
        self.largest_request = self.largest_request.max(buffer.len());
        Ok(())
    }
}

#[derive(Default)]
struct Resolver {
    dependencies: BTreeMap<u64, DependencyResolution>,
}

impl Resolver {
    fn with(mut self, object_id: u64, dependencies: DependencyResolution) -> Self {
        self.dependencies.insert(object_id, dependencies);
        self
    }
}

impl ImmutableDependencyResolver for Resolver {
    fn dependencies(
        &mut self,
        object_id: u64,
        _kind: u16,
        _payload: &[u8],
    ) -> Result<DependencyResolution, &'static str> {
        Ok(self
            .dependencies
            .get(&object_id)
            .cloned()
            .unwrap_or(DependencyResolution::Known(Vec::new())))
    }
}

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::try_from(1 + object_id % 5).expect("kind"),
        format!("payload:{object_id}").into_bytes(),
    )
}

fn source_limits() -> ImmutableSourceLimits {
    ImmutableSourceLimits {
        max_total_bytes_read: 32 * 1024 * 1024,
        max_read_operations: 100_000,
        max_read_request_bytes: 4 * 1024,
        hash_block_bytes: 4 * 1024,
        ..ImmutableSourceLimits::default()
    }
}

#[test]
fn source_rewrite_all_matches_slice_rewrite_without_whole_file_requests() {
    let format = ImmutableLimits::default();
    let objects: Vec<_> = (1_u64..=400).map(object).collect();
    let bytes = build_genesis(&objects, format).expect("genesis");
    let expected = rewrite_all(&bytes, format).expect("slice rewrite");
    let source_len = bytes.len();
    let mut source = TracingSource::new(bytes);

    let actual = rewrite_source_all(&mut source, source_limits()).expect("source rewrite");
    assert_eq!(actual.rewrite, expected);
    assert_eq!(actual.stats.bytes_read, source.bytes_read as u64);
    assert!(source.read_operations > 1);
    assert!(source.largest_request <= 4 * 1024);
    assert!(source.largest_request < source_len);
}

#[test]
fn source_selected_rewrite_matches_slice_semantics() {
    let format = ImmutableLimits::default();
    let objects: Vec<_> = (1_u64..=400).map(object).collect();
    let bytes = build_genesis(&objects, format).expect("genesis");
    let selected = [1_u64, 200, 400];
    let expected = rewrite_selected(&bytes, &selected, format).expect("slice rewrite");
    let mut source = TracingSource::new(bytes);

    let actual = rewrite_source_selected(&mut source, &selected, source_limits())
        .expect("source selected rewrite");
    assert_eq!(actual.rewrite, expected);
    assert!(actual.stats.read_operations > 0);
}

#[test]
fn source_semantic_compaction_matches_slice_result() {
    let format = ImmutableLimits::default();
    let objects: Vec<_> = (1_u64..=6).map(object).collect();
    let bytes = build_genesis(&objects, format).expect("genesis");
    let mut resolver = Resolver::default()
        .with(1, DependencyResolution::Known(vec![2, 3]))
        .with(2, DependencyResolution::Known(vec![4]));
    let expected = semantic_compact(
        &bytes,
        &[1],
        &mut resolver,
        UnknownDependencyPolicy::Reject,
        ImmutableCompactionLimits::default(),
        format,
    )
    .expect("slice semantic compaction");

    let mut source = TracingSource::new(bytes);
    let mut resolver = Resolver::default()
        .with(1, DependencyResolution::Known(vec![2, 3]))
        .with(2, DependencyResolution::Known(vec![4]));
    let actual = semantic_compact_source(
        &mut source,
        &[1],
        &mut resolver,
        UnknownDependencyPolicy::Reject,
        ImmutableCompactionLimits::default(),
        source_limits(),
    )
    .expect("source semantic compaction");

    assert_eq!(actual.compaction, expected);
    assert!(actual.stats.bytes_read > 0);
    assert!(source.largest_request <= 4 * 1024);
}

#[test]
fn source_rewrite_uses_one_cumulative_read_budget() {
    let format = ImmutableLimits::default();
    let bytes =
        build_genesis(&(1_u64..=20).map(object).collect::<Vec<_>>(), format).expect("genesis");
    let mut source = TracingSource::new(bytes);
    let limits = ImmutableSourceLimits {
        max_total_bytes_read: 1,
        max_read_operations: 1,
        max_read_request_bytes: 1,
        hash_block_bytes: 1,
        ..source_limits()
    };
    assert!(matches!(
        rewrite_source_all(&mut source, limits),
        Err(ImmutableSourceError::Limit("read bytes"))
            | Err(ImmutableSourceError::Limit("read operations"))
    ));
}
