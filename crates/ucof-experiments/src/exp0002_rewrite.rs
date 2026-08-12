//! Repair-to-new-file and explicit object-selection rewrite experiments.
//!
//! These operations accept only a strictly verified source snapshot. They
//! publish a new genesis commit and distinguish structural snapshot identity
//! from file-instance commit identity as required by ADR-0011.

use crate::exp0002::{
    build_genesis, validate_strict, Exp0002Error, FileHeader, ObjectInput, ValidationLimits,
    VerifiedExp0002, OBJECT_HEADER_LEN,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteLimits {
    pub validation: ValidationLimits,
    pub max_objects_copied: usize,
    pub max_payload_bytes_copied: u64,
    pub max_output_bytes: u64,
}

impl Default for RewriteLimits {
    fn default() -> Self {
        Self {
            validation: ValidationLimits::default(),
            max_objects_copied: 10_000_000,
            max_payload_bytes_copied: 16 * 1024 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteReport {
    pub output: Vec<u8>,
    pub source_snapshot_digest: [u8; 32],
    pub output_snapshot_digest: [u8; 32],
    pub source_commit_digest: [u8; 32],
    pub output_commit_digest: [u8; 32],
    pub snapshot_digest_preserved: bool,
    pub commit_digest_preserved: bool,
    pub byte_scoped_signatures_preserved: bool,
    pub source_object_count: usize,
    pub output_object_count: usize,
    pub payload_bytes_copied: u64,
}

pub fn repair_all_to_new_file(
    source: &[u8],
    output_header: FileHeader,
    limits: &RewriteLimits,
) -> Result<RewriteReport, Exp0002Error> {
    let verified = validate_strict(source, &limits.validation)?;
    let retained: Vec<u64> = verified
        .objects
        .iter()
        .map(|entry| entry.object_id)
        .collect();
    rewrite_verified(
        source,
        &verified,
        output_header,
        &retained,
        &verified.snapshot.roots,
        limits,
    )
}

pub fn rewrite_selected_to_new_file(
    source: &[u8],
    output_header: FileHeader,
    retained_object_ids: &[u64],
    output_roots: &[u64],
    limits: &RewriteLimits,
) -> Result<RewriteReport, Exp0002Error> {
    let verified = validate_strict(source, &limits.validation)?;
    validate_canonical_ids(retained_object_ids, "retained object identifiers")?;
    validate_canonical_ids(output_roots, "output roots")?;
    if retained_object_ids.is_empty() {
        return Err(Exp0002Error::EmptyObjectSet);
    }
    if output_roots.is_empty() {
        return Err(Exp0002Error::NoRootObjects);
    }
    let retained: BTreeSet<u64> = retained_object_ids.iter().copied().collect();
    if output_roots.iter().any(|root| !retained.contains(root)) {
        return Err(Exp0002Error::InvalidRoot);
    }
    let available: BTreeSet<u64> = verified
        .objects
        .iter()
        .map(|entry| entry.object_id)
        .collect();
    if retained_object_ids
        .iter()
        .any(|object_id| !available.contains(object_id))
    {
        return Err(Exp0002Error::InvalidObjectId);
    }
    rewrite_verified(
        source,
        &verified,
        output_header,
        retained_object_ids,
        output_roots,
        limits,
    )
}

fn rewrite_verified(
    source: &[u8],
    verified: &VerifiedExp0002,
    output_header: FileHeader,
    retained_object_ids: &[u64],
    output_roots: &[u64],
    limits: &RewriteLimits,
) -> Result<RewriteReport, Exp0002Error> {
    if retained_object_ids.len() > limits.max_objects_copied {
        return Err(Exp0002Error::ResourceLimit("rewrite objects"));
    }
    let retained: BTreeSet<u64> = retained_object_ids.iter().copied().collect();
    let roots: BTreeSet<u64> = output_roots.iter().copied().collect();
    let mut payload_bytes_copied = 0_u64;
    let mut inputs = Vec::with_capacity(retained_object_ids.len());

    for entry in verified
        .objects
        .iter()
        .filter(|entry| retained.contains(&entry.object_id))
    {
        let payload_len = entry
            .record_len
            .checked_sub(OBJECT_HEADER_LEN as u64)
            .ok_or(Exp0002Error::InvalidLength("object record"))?;
        payload_bytes_copied = payload_bytes_copied
            .checked_add(payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if payload_bytes_copied > limits.max_payload_bytes_copied {
            return Err(Exp0002Error::ResourceLimit("rewrite payload bytes"));
        }
        let payload_start = entry
            .record_offset
            .checked_add(OBJECT_HEADER_LEN as u64)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let payload_end = payload_start
            .checked_add(payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let start = usize::try_from(payload_start).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let end = usize::try_from(payload_end).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let payload = source
            .get(start..end)
            .ok_or(Exp0002Error::Truncated)?
            .to_vec();
        inputs.push(ObjectInput {
            object_id: entry.object_id,
            kind: entry.kind,
            payload,
            is_root: roots.contains(&entry.object_id),
        });
    }
    if inputs.len() != retained_object_ids.len() {
        return Err(Exp0002Error::InvalidObjectId);
    }

    let output = build_genesis(output_header, inputs)?;
    let output_len = u64::try_from(output.len()).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
    if output_len > limits.max_output_bytes {
        return Err(Exp0002Error::ResourceLimit("rewrite output bytes"));
    }
    let output_verified = validate_strict(&output, &limits.validation)?;
    let source_snapshot_digest = verified.footer.snapshot_digest;
    let output_snapshot_digest = output_verified.footer.snapshot_digest;
    let source_commit_digest = verified.footer.commit_digest;
    let output_commit_digest = output_verified.footer.commit_digest;
    Ok(RewriteReport {
        output,
        source_snapshot_digest,
        output_snapshot_digest,
        source_commit_digest,
        output_commit_digest,
        snapshot_digest_preserved: source_snapshot_digest == output_snapshot_digest,
        commit_digest_preserved: source_commit_digest == output_commit_digest,
        byte_scoped_signatures_preserved: false,
        source_object_count: verified.objects.len(),
        output_object_count: output_verified.objects.len(),
        payload_bytes_copied,
    })
}

fn validate_canonical_ids(values: &[u64], name: &'static str) -> Result<(), Exp0002Error> {
    if values.first() == Some(&0) || values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(match name {
            "output roots" => Exp0002Error::InvalidRoot,
            _ => Exp0002Error::InvalidObjectId,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exp0002::{build_append, build_genesis, ObjectInput};

    fn header(tag: u8) -> FileHeader {
        FileHeader {
            file_id: [tag; 16],
            creation_nonce: [tag.wrapping_add(1); 16],
        }
    }

    fn object(id: u64, payload: &[u8], root: bool) -> ObjectInput {
        ObjectInput {
            object_id: id,
            kind: 1,
            payload: payload.to_vec(),
            is_root: root,
        }
    }

    #[test]
    fn deterministic_genesis_repair_preserves_snapshot_but_not_commit_identity() {
        let source = build_genesis(
            header(1),
            vec![object(1, b"one", true), object(2, b"two", false)],
        )
        .expect("source");
        let report =
            repair_all_to_new_file(&source, header(9), &RewriteLimits::default()).expect("repair");
        assert!(report.snapshot_digest_preserved);
        assert!(!report.commit_digest_preserved);
        assert!(!report.byte_scoped_signatures_preserved);
        assert_eq!(report.source_object_count, 2);
        assert_eq!(report.output_object_count, 2);
        assert_eq!(report.payload_bytes_copied, 6);
    }

    #[test]
    fn append_repair_becomes_new_genesis_and_changes_both_identities() {
        let genesis = build_genesis(header(1), vec![object(1, b"one", true)]).expect("genesis");
        let source = build_append(
            &genesis,
            vec![object(2, b"two", false)],
            vec![1, 2],
            &ValidationLimits::default(),
        )
        .expect("append");
        let report =
            repair_all_to_new_file(&source, header(7), &RewriteLimits::default()).expect("repair");
        assert!(!report.snapshot_digest_preserved);
        assert!(!report.commit_digest_preserved);
        let output = validate_strict(&report.output, &ValidationLimits::default()).expect("output");
        assert_eq!(output.snapshot.sequence, 0);
        assert_eq!(output.snapshot.roots, vec![1, 2]);
        assert_eq!(output.objects.len(), 2);
    }

    #[test]
    fn explicit_selection_removes_unretained_objects() {
        let source = build_genesis(
            header(1),
            vec![
                object(1, b"one", true),
                object(2, b"two", false),
                object(3, b"orphan", false),
            ],
        )
        .expect("source");
        let report = rewrite_selected_to_new_file(
            &source,
            header(3),
            &[1, 2],
            &[1],
            &RewriteLimits::default(),
        )
        .expect("rewrite");
        assert_eq!(report.source_object_count, 3);
        assert_eq!(report.output_object_count, 2);
        assert!(!report.snapshot_digest_preserved);
        let output = validate_strict(&report.output, &ValidationLimits::default()).expect("output");
        assert_eq!(
            output
                .objects
                .iter()
                .map(|entry| entry.object_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn output_roots_must_be_retained() {
        let source = build_genesis(
            header(1),
            vec![object(1, b"one", true), object(2, b"two", false)],
        )
        .expect("source");
        assert_eq!(
            rewrite_selected_to_new_file(&source, header(2), &[2], &[1], &RewriteLimits::default()),
            Err(Exp0002Error::InvalidRoot)
        );
    }

    #[test]
    fn damaged_source_cannot_be_repaired() {
        let mut source = build_genesis(header(1), vec![object(1, b"one", true)]).expect("source");
        source[64 + OBJECT_HEADER_LEN] ^= 1;
        assert!(repair_all_to_new_file(&source, header(2), &RewriteLimits::default()).is_err());
    }

    #[test]
    fn rewrite_limits_apply_before_unbounded_copying() {
        let source = build_genesis(
            header(1),
            vec![object(1, b"one", true), object(2, b"two", false)],
        )
        .expect("source");
        assert_eq!(
            repair_all_to_new_file(
                &source,
                header(2),
                &RewriteLimits {
                    max_objects_copied: 1,
                    ..RewriteLimits::default()
                }
            ),
            Err(Exp0002Error::ResourceLimit("rewrite objects"))
        );
        assert_eq!(
            repair_all_to_new_file(
                &source,
                header(2),
                &RewriteLimits {
                    max_payload_bytes_copied: 5,
                    ..RewriteLimits::default()
                }
            ),
            Err(Exp0002Error::ResourceLimit("rewrite payload bytes"))
        );
    }
}
