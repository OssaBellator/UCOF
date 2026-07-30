//! Hardened bounded recovery over random-access EXP-0002 sources.
//!
//! Candidate footer magic has no authority. Every reported prefix is validated
//! by the full exact-end source validator. Scan work and all candidate reads,
//! including failed candidates, are independently bounded and reported.

use crate::exp0002::{Exp0002Error, FOOTER_LEN, PAGE_SIZE};
use crate::exp0002_source::{
    Exp0002ReadAt, Exp0002SourceError, Exp0002SourceLimits, Exp0002SourceStats,
};
use crate::exp0002_source_strict::validate_strict_at;
use std::io;

const FOOTER_MAGIC: &[u8; 8] = b"UCOF2END";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002SourceRecoveryLimits {
    pub candidate: Exp0002SourceLimits,
    pub max_scan_bytes: u64,
    pub max_scan_read_operations: u64,
    pub max_magic_matches: usize,
    pub max_candidate_validations: usize,
    pub max_results: usize,
    pub max_total_candidate_bytes_read: u64,
}

impl Default for Exp0002SourceRecoveryLimits {
    fn default() -> Self {
        Self {
            candidate: Exp0002SourceLimits::default(),
            max_scan_bytes: 16 * 1024 * 1024,
            max_scan_read_operations: 4096,
            max_magic_matches: 4096,
            max_candidate_validations: 1024,
            max_results: 64,
            max_total_candidate_bytes_read: 64 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredExp0002SourcePrefix {
    pub prefix_len: u64,
    pub footer_offset: u64,
    pub sequence: u64,
    pub previous_footer_offset: u64,
    pub parent_snapshot_digest: [u8; 32],
    pub snapshot_digest: [u8; 32],
    pub commit_digest: [u8; 32],
    pub roots: Vec<u64>,
    pub validation_stats: Exp0002SourceStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002SourceRecoveryReport {
    pub file_len: u64,
    pub scan_start: u64,
    pub scan_bytes_read: u64,
    pub scan_read_operations: u64,
    pub magic_matches: usize,
    pub candidates_validated: usize,
    pub total_candidate_bytes_read: u64,
    pub results: Vec<RecoveredExp0002SourcePrefix>,
}

pub fn scan_valid_prefixes_at<S: Exp0002ReadAt>(
    source: &mut S,
    limits: &Exp0002SourceRecoveryLimits,
) -> Result<Exp0002SourceRecoveryReport, Exp0002SourceError> {
    validate_configuration(limits)?;
    let file_len = source
        .len()
        .map_err(|_| Exp0002SourceError::Io("recovery source length"))?;
    let scan_len = file_len.min(limits.max_scan_bytes);
    let scan_start = file_len
        .checked_sub(scan_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    let scan_len_usize = usize::try_from(scan_len).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
    let mut scan = vec![0_u8; scan_len_usize];
    let mut scan_cursor = 0_usize;
    let mut scan_read_operations = 0_u64;
    while scan_cursor < scan.len() {
        let take = (scan.len() - scan_cursor).min(limits.candidate.max_read_request_bytes);
        let read_offset = scan_start
            .checked_add(u64::try_from(scan_cursor).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        source
            .read_exact_at(read_offset, &mut scan[scan_cursor..scan_cursor + take])
            .map_err(|_| Exp0002SourceError::Io("recovery scan"))?;
        scan_cursor += take;
        scan_read_operations = scan_read_operations
            .checked_add(1)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if scan_read_operations > limits.max_scan_read_operations {
            return Err(Exp0002Error::ResourceLimit("recovery scan reads").into());
        }
    }

    let mut report = Exp0002SourceRecoveryReport {
        file_len,
        scan_start,
        scan_bytes_read: scan_len,
        scan_read_operations,
        magic_matches: 0,
        candidates_validated: 0,
        total_candidate_bytes_read: 0,
        results: Vec::new(),
    };
    if scan.len() < FOOTER_MAGIC.len() {
        return Ok(report);
    }

    let mut positions = Vec::new();
    for index in 0..=scan.len() - FOOTER_MAGIC.len() {
        if &scan[index..index + FOOTER_MAGIC.len()] == FOOTER_MAGIC {
            report.magic_matches = report
                .magic_matches
                .checked_add(1)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            if report.magic_matches > limits.max_magic_matches {
                return Err(Exp0002Error::ResourceLimit("recovery magic matches").into());
            }
            positions.push(index);
        }
    }

    for index in positions.into_iter().rev() {
        if report.candidates_validated >= limits.max_candidate_validations {
            return Err(Exp0002Error::ResourceLimit("recovery candidates").into());
        }
        let footer_offset = scan_start
            .checked_add(u64::try_from(index).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let prefix_len = footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if prefix_len > file_len {
            continue;
        }

        report.candidates_validated += 1;
        let (validation, candidate_bytes_read) = {
            let mut prefix = CountingPrefixSource {
                source: &mut *source,
                len: prefix_len,
                bytes_read: 0,
            };
            let validation = validate_strict_at(&mut prefix, &limits.candidate);
            (validation, prefix.bytes_read)
        };
        report.total_candidate_bytes_read = report
            .total_candidate_bytes_read
            .checked_add(candidate_bytes_read)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if report.total_candidate_bytes_read > limits.max_total_candidate_bytes_read {
            return Err(Exp0002Error::ResourceLimit("recovery validation bytes").into());
        }

        if let Ok(verified) = validation {
            if report.results.len() >= limits.max_results {
                return Err(Exp0002Error::ResourceLimit("recovery results").into());
            }
            report.results.push(RecoveredExp0002SourcePrefix {
                prefix_len,
                footer_offset: verified.footer_offset,
                sequence: verified.footer.sequence,
                previous_footer_offset: verified.footer.previous_footer_offset,
                parent_snapshot_digest: verified.snapshot.parent_snapshot_digest,
                snapshot_digest: verified.footer.snapshot_digest,
                commit_digest: verified.footer.commit_digest,
                roots: verified.snapshot.roots.clone(),
                validation_stats: verified.stats,
            });
        }
    }
    Ok(report)
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

fn validate_configuration(limits: &Exp0002SourceRecoveryLimits) -> Result<(), Exp0002SourceError> {
    if limits.max_scan_bytes == 0
        || limits.max_scan_read_operations == 0
        || limits.max_magic_matches == 0
        || limits.max_candidate_validations == 0
        || limits.max_results == 0
        || limits.candidate.max_read_request_bytes < PAGE_SIZE
        || limits.candidate.hash_block_bytes == 0
        || limits.candidate.hash_block_bytes > limits.candidate.max_read_request_bytes
    {
        return Err(Exp0002Error::ResourceLimit("recovery configuration").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exp0002::{build_append, build_genesis, FileHeader, ObjectInput, ValidationLimits};
    use crate::exp0002_source::Exp0002SliceSource;

    fn header() -> FileHeader {
        FileHeader {
            file_id: *b"recover-src-id01",
            creation_nonce: *b"recover-nonce002",
        }
    }

    fn object(id: u64, payload: &[u8], is_root: bool) -> ObjectInput {
        ObjectInput {
            object_id: id,
            kind: 1,
            payload: payload.to_vec(),
            is_root,
        }
    }

    #[test]
    fn interrupted_append_recovers_only_strict_prefixes() {
        let genesis = build_genesis(
            header(),
            vec![object(1, b"one", true), object(2, b"two", false)],
        )
        .expect("genesis");
        let append = build_append(
            &genesis,
            vec![object(3, b"three", false)],
            vec![1, 3],
            &ValidationLimits::default(),
        )
        .expect("append");
        let interrupted = &append[..append.len() - FOOTER_LEN / 2];
        let mut source = Exp0002SliceSource::new(interrupted);
        let report = scan_valid_prefixes_at(&mut source, &Exp0002SourceRecoveryLimits::default())
            .expect("recovery");
        let recovered = report
            .results
            .iter()
            .find(|candidate| candidate.prefix_len == u64::try_from(genesis.len()).expect("len"))
            .expect("genesis prefix");
        assert_eq!(recovered.sequence, 0);
        assert_eq!(recovered.roots, vec![1]);
        assert_eq!(recovered.parent_snapshot_digest, [0_u8; 32]);
        assert!(report
            .results
            .iter()
            .all(|candidate| candidate.sequence == 0));
    }

    #[test]
    fn failed_candidate_reads_are_charged() {
        let mut bytes = build_genesis(header(), vec![object(1, b"root", true)]).expect("genesis");
        let mut fake_footer = [0_u8; FOOTER_LEN];
        fake_footer[..FOOTER_MAGIC.len()].copy_from_slice(FOOTER_MAGIC);
        bytes.extend_from_slice(&fake_footer);
        let mut source = Exp0002SliceSource::new(&bytes);
        let error = scan_valid_prefixes_at(
            &mut source,
            &Exp0002SourceRecoveryLimits {
                max_total_candidate_bytes_read: 1,
                ..Exp0002SourceRecoveryLimits::default()
            },
        )
        .expect_err("failed candidate work must be charged");
        assert_eq!(
            error,
            Exp0002SourceError::Format(Exp0002Error::ResourceLimit("recovery validation bytes"))
        );
    }

    #[test]
    fn scan_requests_are_chunked_and_counted() {
        let genesis = build_genesis(header(), vec![object(1, b"root", true)]).expect("genesis");
        let mut bytes = genesis.clone();
        bytes.resize(genesis.len() + PAGE_SIZE * 3, 0);
        let mut source = RequestBoundSource {
            bytes: &bytes,
            maximum: PAGE_SIZE,
        };
        let report = scan_valid_prefixes_at(
            &mut source,
            &Exp0002SourceRecoveryLimits {
                candidate: Exp0002SourceLimits {
                    max_read_request_bytes: PAGE_SIZE,
                    hash_block_bytes: PAGE_SIZE,
                    ..Exp0002SourceLimits::default()
                },
                max_scan_bytes: u64::try_from(bytes.len()).expect("len"),
                ..Exp0002SourceRecoveryLimits::default()
            },
        )
        .expect("chunked scan");
        assert!(report.scan_read_operations >= 4);
        assert!(report
            .results
            .iter()
            .any(|candidate| candidate.prefix_len == u64::try_from(genesis.len()).expect("len")));
    }

    #[test]
    fn magic_storm_fails_under_configured_limit() {
        let mut bytes = build_genesis(header(), vec![object(1, b"root", true)]).expect("genesis");
        for _ in 0..32 {
            bytes.extend_from_slice(FOOTER_MAGIC);
        }
        let mut source = Exp0002SliceSource::new(&bytes);
        let error = scan_valid_prefixes_at(
            &mut source,
            &Exp0002SourceRecoveryLimits {
                max_magic_matches: 4,
                ..Exp0002SourceRecoveryLimits::default()
            },
        )
        .expect_err("storm");
        assert_eq!(
            error,
            Exp0002SourceError::Format(Exp0002Error::ResourceLimit("recovery magic matches"))
        );
    }

    struct RequestBoundSource<'a> {
        bytes: &'a [u8],
        maximum: usize,
    }

    impl Exp0002ReadAt for RequestBoundSource<'_> {
        fn len(&mut self) -> io::Result<u64> {
            u64::try_from(self.bytes.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "source length exceeds u64")
            })
        }

        fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
            if buffer.len() > self.maximum {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "request exceeded bound",
                ));
            }
            let start = usize::try_from(offset)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset"))?;
            let end = start
                .checked_add(buffer.len())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "range"))?;
            let source = self
                .bytes
                .get(start..end)
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated"))?;
            buffer.copy_from_slice(source);
            Ok(())
        }
    }
}
