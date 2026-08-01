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
            *dependency == 0 || index > 0 && dependencies[index - 1] >= *dependency
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
    use sha2::{Digest, Sha256};

    const REFERENCE_KIND: u16 = 100;
    const LEAF_KIND: u16 = 101;

    fn resolver() -> CanonicalReferenceListResolver {
        CanonicalReferenceListResolver::new(REFERENCE_KIND, LEAF_KIND, 8)
            .expect("valid resolver")
    }

    fn reference_object(object_id: u64, dependencies: &[u64]) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            REFERENCE_KIND,
            encode_canonical_reference_list(dependencies, 8).expect("reference payload"),
        )
    }

    fn leaf_object(object_id: u64) -> ImmutableObjectInput {
        ImmutableObjectInput::new(object_id, LEAF_KIND, Vec::new())
    }

    fn decode_sha256(value: &str) -> [u8; 32] {
        assert_eq!(value.len(), 64);
        let mut decoded = [0_u8; 32];
        for (index, output) in decoded.iter_mut().enumerate() {
            let start = index * 2;
            *output = u8::from_str_radix(&value[start..start + 2], 16).expect("SHA-256 hex");
        }
        decoded
    }

    struct RewriteRecipe<'a> {
        name: &'a str,
        roots: &'a [u64],
        objects: Vec<ImmutableObjectInput>,
        selected_roots: &'a [u64],
        retained: &'a [u64],
        discarded: &'a [u64],
        edges_visited: usize,
        maximum_depth: usize,
        decoded_bytes: usize,
        sha256: &'a str,
        root_level: u8,
        page_count: usize,
        object_count: usize,
    }

    fn assert_rewrite_recipe(recipe: RewriteRecipe<'_>) -> [u8; 32] {
        let limits = ImmutableLimits::default();
        let source = build_genesis(&recipe.objects, limits).expect("recipe source");
        let result = semantic_compact(
            &source,
            recipe.roots,
            &mut resolver(),
            UnknownDependencyPolicy::Reject,
            ImmutableCompactionLimits::default(),
            limits,
        )
        .expect("profile rewrite recipe");

        assert_eq!(result.selected_roots, recipe.selected_roots, "{} roots", recipe.name);
        assert_eq!(
            result.retained_object_ids, recipe.retained,
            "{} retained",
            recipe.name
        );
        assert_eq!(
            result.discarded_object_ids, recipe.discarded,
            "{} discarded",
            recipe.name
        );
        assert_eq!(
            result.edges_visited, recipe.edges_visited,
            "{} edges",
            recipe.name
        );
        assert_eq!(
            result.maximum_depth, recipe.maximum_depth,
            "{} depth",
            recipe.name
        );
        assert_eq!(
            result.rewrite.bytes.len(), recipe.decoded_bytes,
            "{} bytes",
            recipe.name
        );
        assert_eq!(
            result.rewrite.output.root_level, recipe.root_level,
            "{} root level",
            recipe.name
        );
        assert_eq!(
            result.rewrite.output.page_count, recipe.page_count,
            "{} pages",
            recipe.name
        );
        assert_eq!(
            result.rewrite.output.object_count, recipe.object_count,
            "{} objects",
            recipe.name
        );
        let digest: [u8; 32] = Sha256::digest(&result.rewrite.bytes).into();
        assert_eq!(digest, decode_sha256(recipe.sha256), "{} digest", recipe.name);
        digest
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
            reference_object(1, &[2, 3]),
            leaf_object(2),
            reference_object(3, &[4]),
            leaf_object(4),
            leaf_object(5),
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

    #[test]
    fn rust_matches_independent_reference_profile_rewrite_bytes() {
        let recipes = [
            RewriteRecipe {
                name: "single-root-chain",
                roots: &[1],
                objects: vec![
                    reference_object(1, &[2, 3]),
                    leaf_object(2),
                    reference_object(3, &[4]),
                    leaf_object(4),
                    leaf_object(5),
                ],
                selected_roots: &[1],
                retained: &[1, 2, 3, 4],
                discarded: &[5],
                edges_visited: 3,
                maximum_depth: 2,
                decoded_bytes: 16_904,
                sha256: "87bc7de2d5e2afb51e765bf4694cf3a9a178a605d0f032b8157a4bc1bfc7040e",
                root_level: 0,
                page_count: 1,
                object_count: 4,
            },
            RewriteRecipe {
                name: "two-roots-shared-dependency",
                roots: &[50, 10],
                objects: vec![
                    reference_object(10, &[20, 30]),
                    reference_object(20, &[40]),
                    leaf_object(30),
                    leaf_object(40),
                    reference_object(50, &[40]),
                    leaf_object(60),
                ],
                selected_roots: &[10, 50],
                retained: &[10, 20, 30, 40, 50],
                discarded: &[60],
                edges_visited: 4,
                maximum_depth: 2,
                decoded_bytes: 16_968,
                sha256: "aee59b41b6a7bf135fc1d741256ea7e6ef121ffb1897a39b646ca2f66b73b715",
                root_level: 0,
                page_count: 1,
                object_count: 5,
            },
            RewriteRecipe {
                name: "empty-reference-root",
                roots: &[7],
                objects: vec![reference_object(7, &[]), leaf_object(8)],
                selected_roots: &[7],
                retained: &[7],
                discarded: &[8],
                edges_visited: 0,
                maximum_depth: 0,
                decoded_bytes: 16_728,
                sha256: "65654ef62675f12db1ed3fde4304eaa1344e2cc25bbb5a2718c51ba3779c5e43",
                root_level: 0,
                page_count: 1,
                object_count: 1,
            },
        ];

        let mut aggregate = Sha256::new();
        for recipe in recipes {
            aggregate.update(recipe.name.as_bytes());
            aggregate.update(assert_rewrite_recipe(recipe));
        }
        let aggregate: [u8; 32] = aggregate.finalize().into();
        assert_eq!(
            aggregate,
            decode_sha256("fe20a891b04b90b6df1870e6652eec5d6ddfa91ebc8370f4d6dfa70881a27c84")
        );
    }
}
