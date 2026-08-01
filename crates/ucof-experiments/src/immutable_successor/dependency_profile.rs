/// Non-normative dependency-profile resolver for a canonical list of object identifiers.
///
/// Reference objects use this payload:
///
/// - bytes `0..4`: little-endian dependency count;
/// - bytes `4..8`: zero;
/// - then exactly `count` strictly increasing, non-zero little-endian `u64` identifiers.
///
/// Objects with `leaf_kind` have no dependencies. Every other kind is reported as unknown rather
/// than guessed. The profile is research evidence and does not allocate a normative kind value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalReferenceListResolver {
    reference_kind: u16,
    leaf_kind: u16,
    max_dependencies_per_object: usize,
}

impl CanonicalReferenceListResolver {
    pub fn new(
        reference_kind: u16,
        leaf_kind: u16,
        max_dependencies_per_object: usize,
    ) -> Result<Self, &'static str> {
        if reference_kind == 0
            || leaf_kind == 0
            || reference_kind == leaf_kind
            || max_dependencies_per_object == 0
        {
            return Err("reference profile configuration");
        }
        Ok(Self {
            reference_kind,
            leaf_kind,
            max_dependencies_per_object,
        })
    }
}

impl ImmutableDependencyResolver for CanonicalReferenceListResolver {
    fn dependencies(
        &mut self,
        _object_id: u64,
        kind: u16,
        payload: &[u8],
    ) -> Result<DependencyResolution, &'static str> {
        if kind == self.leaf_kind {
            if payload.is_empty() {
                return Ok(DependencyResolution::Known(Vec::new()));
            }
            return Err("leaf payload");
        }
        if kind != self.reference_kind {
            return Ok(DependencyResolution::Unknown);
        }
        if payload.len() < 8 || payload[4..8].iter().any(|byte| *byte != 0) {
            return Err("reference header");
        }
        let count = usize::try_from(u32::from_le_bytes(
            payload[0..4]
                .try_into()
                .map_err(|_| "reference header")?,
        ))
        .map_err(|_| "reference count")?;
        if count > self.max_dependencies_per_object {
            return Err("reference count");
        }
        let expected = 8_usize
            .checked_add(count.checked_mul(8).ok_or("reference length")?)
            .ok_or("reference length")?;
        if payload.len() != expected {
            return Err("reference length");
        }

        let mut dependencies = Vec::with_capacity(count);
        let mut previous = None;
        for index in 0..count {
            let start = 8 + index * 8;
            let dependency = u64::from_le_bytes(
                payload[start..start + 8]
                    .try_into()
                    .map_err(|_| "reference entry")?,
            );
            if dependency == 0 || previous.is_some_and(|value| value >= dependency) {
                return Err("reference order");
            }
            previous = Some(dependency);
            dependencies.push(dependency);
        }
        Ok(DependencyResolution::Known(dependencies))
    }
}

/// Encodes the canonical research reference-list payload used by
/// [`CanonicalReferenceListResolver`].
pub fn encode_canonical_reference_list(
    dependencies: &[u64],
    max_dependencies_per_object: usize,
) -> Result<Vec<u8>, &'static str> {
    if dependencies.len() > max_dependencies_per_object {
        return Err("reference count");
    }
    if dependencies
        .iter()
        .enumerate()
        .any(|(index, dependency)| {
            *dependency == 0
                || index > 0 && dependencies[index - 1] >= *dependency
        })
    {
        return Err("reference order");
    }
    let count = u32::try_from(dependencies.len()).map_err(|_| "reference count")?;
    let capacity = 8_usize
        .checked_add(
            dependencies
                .len()
                .checked_mul(8)
                .ok_or("reference length")?,
        )
        .ok_or("reference length")?;
    let mut payload = Vec::with_capacity(capacity);
    payload.extend_from_slice(&count.to_le_bytes());
    payload.extend_from_slice(&[0_u8; 4]);
    for dependency in dependencies {
        payload.extend_from_slice(&dependency.to_le_bytes());
    }
    Ok(payload)
}

#[cfg(test)]
mod dependency_profile_tests {
    use super::*;

    const REFERENCE_KIND: u16 = 100;
    const LEAF_KIND: u16 = 101;

    fn resolver() -> CanonicalReferenceListResolver {
        CanonicalReferenceListResolver::new(REFERENCE_KIND, LEAF_KIND, 8)
            .expect("valid resolver")
    }

    #[test]
    fn canonical_reference_payload_round_trips() {
        let payload = encode_canonical_reference_list(&[2, 7, 9], 8).expect("payload");
        assert_eq!(
            resolver()
                .dependencies(1, REFERENCE_KIND, &payload)
                .expect("dependencies"),
            DependencyResolution::Known(vec![2, 7, 9])
        );
        assert_eq!(
            resolver()
                .dependencies(2, LEAF_KIND, &[])
                .expect("leaf"),
            DependencyResolution::Known(Vec::new())
        );
        assert_eq!(
            resolver()
                .dependencies(3, 999, &[])
                .expect("unknown"),
            DependencyResolution::Unknown
        );
    }

    #[test]
    fn malformed_payloads_fail_closed() {
        let mut reserved = encode_canonical_reference_list(&[2], 8).expect("payload");
        reserved[4] = 1;
        assert_eq!(
            resolver().dependencies(1, REFERENCE_KIND, &reserved),
            Err("reference header")
        );
        assert_eq!(
            resolver().dependencies(1, REFERENCE_KIND, &[1, 0, 0, 0, 0, 0, 0, 0]),
            Err("reference length")
        );
        assert_eq!(
            encode_canonical_reference_list(&[2, 2], 8),
            Err("reference order")
        );
        assert_eq!(
            encode_canonical_reference_list(&[0], 8),
            Err("reference order")
        );
        assert_eq!(
            resolver().dependencies(1, LEAF_KIND, &[1]),
            Err("leaf payload")
        );
    }

    #[test]
    fn profile_drives_dependency_complete_compaction() {
        let limits = ImmutableLimits::default();
        let objects = vec![
            ImmutableObjectInput::new(
                1,
                REFERENCE_KIND,
                encode_canonical_reference_list(&[2, 3], 8).expect("root references"),
            ),
            ImmutableObjectInput::new(2, LEAF_KIND, Vec::new()),
            ImmutableObjectInput::new(
                3,
                REFERENCE_KIND,
                encode_canonical_reference_list(&[4], 8).expect("child references"),
            ),
            ImmutableObjectInput::new(4, LEAF_KIND, Vec::new()),
            ImmutableObjectInput::new(5, LEAF_KIND, Vec::new()),
        ];
        let source = build_genesis(&objects, limits).expect("source");
        let result = semantic_compact(
            &source,
            &[1],
            &mut resolver(),
            UnknownDependencyPolicy::Reject,
            ImmutableCompactionLimits::default(),
            limits,
        )
        .expect("profile compaction");
        assert_eq!(result.retained_object_ids, vec![1, 2, 3, 4]);
        assert_eq!(result.discarded_object_ids, vec![5]);
        assert_eq!(result.edges_visited, 3);
        assert_eq!(result.maximum_depth, 2);
        assert_eq!(result.rewrite.output.object_count, 4);
    }
}
