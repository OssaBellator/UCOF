//! Stable-view adapter for remote or mutable EXP-0002 range sources.
//!
//! The adapter does not define how a storage system produces version evidence.
//! Callers must map a strong immutable version identifier, such as an object
//! generation or strong ETag plus object identity, into a 32-byte token. The
//! same token is required before and after every length or range read.

use crate::exp0002_source::Exp0002ReadAt;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Exp0002SourceVersion(pub [u8; 32]);

pub trait Exp0002VersionedReadAt: Exp0002ReadAt {
    fn version_token(&mut self) -> io::Result<Exp0002SourceVersion>;
}

#[derive(Debug)]
pub struct Exp0002StableSource<S> {
    inner: S,
    expected: Exp0002SourceVersion,
}

impl<S: Exp0002VersionedReadAt> Exp0002StableSource<S> {
    pub fn new(mut inner: S) -> io::Result<Self> {
        let expected = inner.version_token()?;
        Ok(Self { inner, expected })
    }

    #[must_use]
    pub const fn expected_version(&self) -> Exp0002SourceVersion {
        self.expected
    }

    #[must_use]
    pub fn into_inner(self) -> S {
        self.inner
    }

    fn ensure_stable(&mut self) -> io::Result<()> {
        let actual = self.inner.version_token()?;
        if actual == self.expected {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "EXP-0002 source version changed during one operation",
            ))
        }
    }
}

impl<S: Exp0002VersionedReadAt> Exp0002ReadAt for Exp0002StableSource<S> {
    fn len(&mut self) -> io::Result<u64> {
        self.ensure_stable()?;
        let length = self.inner.len()?;
        self.ensure_stable()?;
        Ok(length)
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        self.ensure_stable()?;
        self.inner.read_exact_at(offset, buffer)?;
        self.ensure_stable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exp0002::{build_genesis, FileHeader, ObjectInput};
    use crate::exp0002_source::Exp0002SourceLimits;
    use crate::validate_strict_at;

    struct VersionedBytes {
        bytes: Vec<u8>,
        version: Exp0002SourceVersion,
        reads: usize,
        mutate_after_read: Option<usize>,
    }

    impl Exp0002ReadAt for VersionedBytes {
        fn len(&mut self) -> io::Result<u64> {
            u64::try_from(self.bytes.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "source length exceeds u64")
            })
        }

        fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
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
            self.reads += 1;
            if self.mutate_after_read == Some(self.reads) {
                self.version.0[0] ^= 1;
            }
            Ok(())
        }
    }

    impl Exp0002VersionedReadAt for VersionedBytes {
        fn version_token(&mut self) -> io::Result<Exp0002SourceVersion> {
            Ok(self.version)
        }
    }

    fn vector() -> Vec<u8> {
        build_genesis(
            FileHeader {
                file_id: *b"stable-source-id",
                creation_nonce: *b"stable-nonce-002",
            },
            vec![ObjectInput {
                object_id: 1,
                kind: 1,
                payload: b"root".to_vec(),
                is_root: true,
            }],
        )
        .expect("vector")
    }

    #[test]
    fn stable_version_allows_full_validation() {
        let source = VersionedBytes {
            bytes: vector(),
            version: Exp0002SourceVersion([7; 32]),
            reads: 0,
            mutate_after_read: None,
        };
        let mut stable = Exp0002StableSource::new(source).expect("stable source");
        validate_strict_at(&mut stable, &Exp0002SourceLimits::default())
            .expect("strict validation");
    }

    #[test]
    fn changed_version_fails_during_validation() {
        let source = VersionedBytes {
            bytes: vector(),
            version: Exp0002SourceVersion([7; 32]),
            reads: 0,
            mutate_after_read: Some(3),
        };
        let mut stable = Exp0002StableSource::new(source).expect("stable source");
        assert!(validate_strict_at(&mut stable, &Exp0002SourceLimits::default()).is_err());
    }

    #[test]
    fn changed_version_fails_even_when_returned_bytes_are_unchanged() {
        let mut source = VersionedBytes {
            bytes: vector(),
            version: Exp0002SourceVersion([9; 32]),
            reads: 0,
            mutate_after_read: None,
        };
        let mut stable = Exp0002StableSource::new(source).expect("stable source");
        stable.inner.version.0[31] ^= 1;
        let mut byte = [0_u8; 1];
        assert!(stable.read_exact_at(0, &mut byte).is_err());
        source = stable.into_inner();
        assert_eq!(source.reads, 0);
    }
}
