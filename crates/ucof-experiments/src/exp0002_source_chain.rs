//! Verified previous-footer chain enumeration over bounded random-access sources.
//!
//! Every ancestor is validated as an exact-end prefix. Stored previous-footer
//! locators are traversal hints only; the parent prefix must independently pass
//! full validation and match the child's parent digest and sequence relation.

use crate::exp0002::{Exp0002Error, ABSENT_OFFSET, FOOTER_LEN};
use crate::exp0002_source::{Exp0002ReadAt, Exp0002SourceError, Exp0002SourceLimits};
use crate::exp0002_source_recovery::RecoveredExp0002SourcePrefix;
use crate::validate_strict_at;
use std::collections::BTreeSet;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002SourceChainLimits {
    pub candidate: Exp0002SourceLimits,
    pub max_chain_depth: usize,
    pub max_total_bytes_read: u64,
}

impl Default for Exp0002SourceChainLimits {
    fn default() -> Self {
        Self {
            candidate: Exp0002SourceLimits::default(),
            max_chain_depth: 1024,
            max_total_bytes_read: 64 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002SourceChainReport {
    pub file_len: u64,
    pub total_bytes_read: u64,
    /// Entries are ordered from the exact-end active commit toward genesis.
    pub commits: Vec<RecoveredExp0002SourcePrefix>,
}

pub fn enumerate_previous_chain_at<S: Exp0002ReadAt>(
    source: &mut S,
    limits: &Exp0002SourceChainLimits,
) -> Result<Exp0002SourceChainReport, Exp0002SourceError> {
    if limits.max_chain_depth == 0 || limits.max_total_bytes_read == 0 {
        return Err(Exp0002Error::ResourceLimit("source chain configuration").into());
    }
    let file_len = source
        .len()
        .map_err(|_| Exp0002SourceError::Io("source chain length"))?;
    let mut prefix_len = file_len;
    let mut total_bytes_read = 0_u64;
    let mut commits: Vec<RecoveredExp0002SourcePrefix> = Vec::new();
    let mut seen_footers = BTreeSet::new();

    loop {
        if commits.len() >= limits.max_chain_depth {
            return Err(Exp0002Error::ResourceLimit("source chain depth").into());
        }
        let (validation, bytes_read) = {
            let mut prefix = CountingPrefixSource {
                source: &mut *source,
                len: prefix_len,
                bytes_read: 0,
            };
            let validation = validate_strict_at(&mut prefix, &limits.candidate);
            (validation, prefix.bytes_read)
        };
        total_bytes_read = total_bytes_read
            .checked_add(bytes_read)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if total_bytes_read > limits.max_total_bytes_read {
            return Err(Exp0002Error::ResourceLimit("source chain bytes").into());
        }
        let verified = validation?;
        if !seen_footers.insert(verified.footer_offset) {
            return Err(Exp0002Error::InvalidPreviousFooter.into());
        }

        let current = RecoveredExp0002SourcePrefix {
            prefix_len,
            footer_offset: verified.footer_offset,
            sequence: verified.footer.sequence,
            previous_footer_offset: verified.footer.previous_footer_offset,
            parent_snapshot_digest: verified.snapshot.parent_snapshot_digest,
            snapshot_digest: verified.footer.snapshot_digest,
            commit_digest: verified.footer.commit_digest,
            roots: verified.snapshot.roots.clone(),
            validation_stats: verified.stats,
        };

        if let Some(child) = commits.last() {
            if child.previous_footer_offset != current.footer_offset
                || child.parent_snapshot_digest != current.snapshot_digest
                || current.sequence.checked_add(1) != Some(child.sequence)
            {
                return Err(Exp0002Error::InvalidParent.into());
            }
        }

        let previous_footer_offset = current.previous_footer_offset;
        commits.push(current);
        if previous_footer_offset == ABSENT_OFFSET {
            break;
        }
        if previous_footer_offset >= commits.last().expect("current commit").footer_offset {
            return Err(Exp0002Error::InvalidPreviousFooter.into());
        }
        prefix_len = previous_footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
    }

    Ok(Exp0002SourceChainReport {
        file_len,
        total_bytes_read,
        commits,
    })
}

struct CountingPrefixSource<'a, S> {
    source: &'a mut S,
    len: u64,
    bytes_read: u64,
}

impl<S: Exp0002ReadAt> Exp0002ReadAt for CountingPrefixSource<'_, S> {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let length = u64::try_from(buffer.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "range length exceeds u64"))?;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "prefix range overflow"))?;
        if end > self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "prefix range is truncated",
            ));
        }
        self.bytes_read = self
            .bytes_read
            .checked_add(length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "read counter overflow"))?;
        self.source.read_exact_at(offset, buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exp0002::{build_append, build_genesis, FileHeader, ObjectInput, ValidationLimits};
    use crate::exp0002_source::Exp0002SliceSource;

    fn object(id: u64, payload: &[u8], is_root: bool) -> ObjectInput {
        ObjectInput {
            object_id: id,
            kind: 1,
            payload: payload.to_vec(),
            is_root,
        }
    }

    fn history() -> Vec<u8> {
        let genesis = build_genesis(
            FileHeader {
                file_id: *b"source-chain-id!",
                creation_nonce: *b"source-chain-002",
            },
            vec![object(1, b"one", true)],
        )
        .expect("genesis");
        let first = build_append(
            &genesis,
            vec![object(2, b"two", false)],
            vec![1, 2],
            &ValidationLimits::default(),
        )
        .expect("first append");
        build_append(
            &first,
            vec![object(3, b"three", false)],
            vec![1, 3],
            &ValidationLimits::default(),
        )
        .expect("second append")
    }

    #[test]
    fn enumerates_exact_verified_chain_to_genesis() {
        let bytes = history();
        let mut source = Exp0002SliceSource::new(&bytes);
        let report = enumerate_previous_chain_at(&mut source, &Exp0002SourceChainLimits::default())
            .expect("chain");
        assert_eq!(report.commits.len(), 3);
        assert_eq!(
            report
                .commits
                .iter()
                .map(|commit| commit.sequence)
                .collect::<Vec<_>>(),
            vec![2, 1, 0]
        );
        assert_eq!(report.commits[0].roots, vec![1, 3]);
        assert_eq!(report.commits[1].roots, vec![1, 2]);
        assert_eq!(report.commits[2].roots, vec![1]);
        assert_eq!(report.commits[2].parent_snapshot_digest, [0_u8; 32]);
        assert!(report.total_bytes_read > 0);
    }

    #[test]
    fn chain_depth_limit_fails_closed() {
        let bytes = history();
        let mut source = Exp0002SliceSource::new(&bytes);
        let error = enumerate_previous_chain_at(
            &mut source,
            &Exp0002SourceChainLimits {
                max_chain_depth: 2,
                ..Exp0002SourceChainLimits::default()
            },
        )
        .expect_err("depth");
        assert_eq!(
            error,
            Exp0002SourceError::Format(Exp0002Error::ResourceLimit("source chain depth"))
        );
    }

    #[test]
    fn cumulative_chain_read_budget_is_enforced() {
        let bytes = history();
        let mut source = Exp0002SliceSource::new(&bytes);
        let error = enumerate_previous_chain_at(
            &mut source,
            &Exp0002SourceChainLimits {
                max_total_bytes_read: 1,
                ..Exp0002SourceChainLimits::default()
            },
        )
        .expect_err("bytes");
        assert_eq!(
            error,
            Exp0002SourceError::Format(Exp0002Error::ResourceLimit("source chain bytes"))
        );
    }
}
