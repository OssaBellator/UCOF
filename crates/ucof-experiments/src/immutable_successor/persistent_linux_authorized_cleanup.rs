mod persistent_linux_authorized_cleanup_tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[repr(u8)]
    enum CleanupAuthority {
        ResumeOrDiscardPrivate = 1,
        ResolvePublication = 2,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct CleanupClaims {
        operation_id: [u8; 16],
        generation: u64,
        authority: CleanupAuthority,
        artifact_identity: [u8; 32],
        private_bytes: u64,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SealedCleanup {
        claims: CleanupClaims,
        tag: [u8; 32],
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum CleanupError {
        AuthenticationFailed,
        ResolvePublication,
        JournalChanged,
        ArtifactChanged,
        StagedNameIndeterminate,
        Backend(&'static str),
    }

    struct TestCleanupAuth {
        key: [u8; 32],
    }

    impl TestCleanupAuth {
        fn tag(&self, claims: CleanupClaims) -> [u8; 32] {
            let mut hasher = Sha256::new();
            hasher.update(b"UCOF-TEST-PINNED-LINUX-CLEANUP\0");
            hasher.update(self.key);
            hasher.update(claims.operation_id);
            hasher.update(claims.generation.to_le_bytes());
            hasher.update([claims.authority as u8]);
            hasher.update(claims.artifact_identity);
            hasher.update(claims.private_bytes.to_le_bytes());
            hasher.finalize().into()
        }

        fn seal(&self, claims: CleanupClaims) -> SealedCleanup {
            SealedCleanup {
                claims,
                tag: self.tag(claims),
            }
        }

        fn open(&self, sealed: SealedCleanup) -> Result<CleanupClaims, CleanupError> {
            if sealed.tag != self.tag(sealed.claims) {
                return Err(CleanupError::AuthenticationFailed);
            }
            Ok(sealed.claims)
        }
    }

    fn test_auth() -> TestCleanupAuth {
        TestCleanupAuth { key: [0x91; 32] }
    }

    fn test_root(label: &str) -> PathBuf {
        let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ucof-pinned-cleanup-{label}-{}-{id}",
            std::process::id()
        ))
    }

    fn create_private_directory(path: &Path) {
        fs::create_dir_all(path).expect("create private directory");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("private permissions");
    }

    fn pinned_snapshot(
        backend: &PersistentLinuxDescriptorStagingBackend,
    ) -> Result<([u8; 32], u64), CleanupError> {
        let staged = backend
            .staged_file
            .as_ref()
            .ok_or(CleanupError::ArtifactChanged)?;
        let metadata = staged
            .metadata()
            .map_err(|_| CleanupError::ArtifactChanged)?;
        let mut hasher = Sha256::new();
        hasher.update(b"UCOF-TEST-PINNED-ARTIFACT-IDENTITY\0");
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
        Ok((hasher.finalize().into(), metadata.len()))
    }

    fn strict_name_preflight(
        backend: &PersistentLinuxDescriptorStagingBackend,
    ) -> Result<(), CleanupError> {
        let staging = backend
            .staging_directory
            .as_ref()
            .ok_or(CleanupError::ArtifactChanged)?;
        let staged = backend
            .staged_file
            .as_ref()
            .ok_or(CleanupError::ArtifactChanged)?;
        match linux_open_relative_readonly(staging, &backend.staged_name)
            .map_err(CleanupError::Backend)?
        {
            Some(named) => {
                if !linux_same_file(staged, &named).map_err(CleanupError::Backend)? {
                    return Err(CleanupError::ArtifactChanged);
                }
                Ok(())
            }
            None => {
                let metadata = staged
                    .metadata()
                    .map_err(|_| CleanupError::ArtifactChanged)?;
                if metadata.nlink() == 0 {
                    Ok(())
                } else {
                    Err(CleanupError::StagedNameIndeterminate)
                }
            }
        }
    }

    fn plan_cleanup(
        backend: &PersistentLinuxDescriptorStagingBackend,
        operation_id: [u8; 16],
        generation: u64,
        authority: CleanupAuthority,
        auth: &TestCleanupAuth,
    ) -> Result<SealedCleanup, CleanupError> {
        if authority == CleanupAuthority::ResolvePublication {
            return Err(CleanupError::ResolvePublication);
        }
        let (artifact_identity, private_bytes) = pinned_snapshot(backend)?;
        Ok(auth.seal(CleanupClaims {
            operation_id,
            generation,
            authority,
            artifact_identity,
            private_bytes,
        }))
    }

    fn execute_cleanup(
        backend: &mut PersistentLinuxDescriptorStagingBackend,
        sealed: SealedCleanup,
        current_operation_id: [u8; 16],
        current_generation: u64,
        current_authority: CleanupAuthority,
        auth: &TestCleanupAuth,
    ) -> Result<(), CleanupError> {
        let claims = auth.open(sealed)?;
        if current_authority == CleanupAuthority::ResolvePublication {
            return Err(CleanupError::ResolvePublication);
        }
        if current_operation_id != claims.operation_id
            || current_generation != claims.generation
            || current_authority != claims.authority
        {
            return Err(CleanupError::JournalChanged);
        }
        let (artifact_identity, private_bytes) = pinned_snapshot(backend)?;
        if artifact_identity != claims.artifact_identity || private_bytes != claims.private_bytes {
            return Err(CleanupError::ArtifactChanged);
        }
        strict_name_preflight(backend)?;
        backend.abort_private().map_err(CleanupError::Backend)
    }

    fn staged_backend(
        label: &str,
        ownership_token: [u8; 32],
        bytes: &[u8],
    ) -> (
        PathBuf,
        PathBuf,
        PathBuf,
        PersistentLinuxDescriptorStagingBackend,
    ) {
        let root = test_root(label);
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        create_private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        let mut backend =
            PersistentLinuxDescriptorStagingBackend::new(&staging, &destination, ownership_token)
                .expect("backend");
        backend
            .begin_private(u64::try_from(bytes.len()).expect("length"))
            .expect("begin private");
        std::io::Write::write_all(&mut backend, bytes).expect("write staged bytes");
        std::io::Write::flush(&mut backend).expect("flush staged bytes");
        (root, staging, destination, backend)
    }

    #[test]
    fn authorized_cleanup_removes_exact_pinned_private_name() {
        let (root, staging, _, mut backend) =
            staged_backend("success", [1; 32], b"private-stage-bytes");
        let staged_name = backend.staged_name.clone();
        let staged_path = staging.join(&staged_name);
        let operation_id = [0x41; 16];
        let token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        execute_cleanup(
            &mut backend,
            token,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("execute");
        assert!(!staged_path.exists());
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn generation_or_resolve_publication_change_is_non_destructive() {
        let (root, staging, _, mut backend) =
            staged_backend("journal-change", [2; 32], b"private-stage-bytes");
        let staged_path = staging.join(&backend.staged_name);
        let operation_id = [0x42; 16];
        let token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        assert_eq!(
            execute_cleanup(
                &mut backend,
                token,
                operation_id,
                8,
                CleanupAuthority::ResumeOrDiscardPrivate,
                &test_auth(),
            )
            .expect_err("generation change"),
            CleanupError::JournalChanged
        );
        assert!(staged_path.exists());
        assert_eq!(
            execute_cleanup(
                &mut backend,
                token,
                operation_id,
                7,
                CleanupAuthority::ResolvePublication,
                &test_auth(),
            )
            .expect_err("resolve publication"),
            CleanupError::ResolvePublication
        );
        assert!(staged_path.exists());
        drop(backend);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn observed_staged_name_replacement_is_rejected_before_unlink() {
        let (root, staging, _, mut backend) =
            staged_backend("name-replaced", [3; 32], b"original-private-bytes");
        let staged_path = staging.join(&backend.staged_name);
        let moved_path = staging.join("moved-original.tmp");
        let operation_id = [0x43; 16];
        let token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        fs::rename(&staged_path, &moved_path).expect("move original name");
        fs::write(&staged_path, b"replacement").expect("replacement");
        assert_eq!(
            execute_cleanup(
                &mut backend,
                token,
                operation_id,
                7,
                CleanupAuthority::ResumeOrDiscardPrivate,
                &test_auth(),
            )
            .expect_err("name replacement"),
            CleanupError::ArtifactChanged
        );
        assert_eq!(fs::read(&staged_path).expect("replacement"), b"replacement");
        assert_eq!(
            fs::read(&moved_path).expect("original"),
            b"original-private-bytes"
        );
        drop(backend);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn missing_name_with_live_link_is_indeterminate_not_cleanup_success() {
        let (root, staging, _, mut backend) =
            staged_backend("renamed", [4; 32], b"private-stage-bytes");
        let staged_path = staging.join(&backend.staged_name);
        let moved_path = staging.join("renamed-private.tmp");
        let operation_id = [0x44; 16];
        let token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        fs::rename(&staged_path, &moved_path).expect("rename staged name");
        assert_eq!(
            execute_cleanup(
                &mut backend,
                token,
                operation_id,
                7,
                CleanupAuthority::ResumeOrDiscardPrivate,
                &test_auth(),
            )
            .expect_err("renamed live link"),
            CleanupError::StagedNameIndeterminate
        );
        assert_eq!(
            fs::read(&moved_path).expect("renamed private"),
            b"private-stage-bytes"
        );
        drop(backend);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn already_unlinked_open_inode_can_retire_without_false_indeterminate() {
        let (root, staging, _, mut backend) =
            staged_backend("already-unlinked", [5; 32], b"private-stage-bytes");
        let staged_path = staging.join(&backend.staged_name);
        let operation_id = [0x45; 16];
        let token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        fs::remove_file(&staged_path).expect("unlink externally");
        assert_eq!(
            backend
                .staged_file
                .as_ref()
                .expect("open staged file")
                .metadata()
                .expect("metadata")
                .nlink(),
            0
        );
        execute_cleanup(
            &mut backend,
            token,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("retire unlinked inode");
        assert!(!staged_path.exists());
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn original_staging_path_replacement_cannot_redirect_authorized_cleanup() {
        let (root, staging, _, mut backend) =
            staged_backend("directory-replaced", [6; 32], b"private-stage-bytes");
        let staged_name = backend.staged_name.clone();
        let moved_staging = root.join("staging-moved");
        let operation_id = [0x46; 16];
        let token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        fs::rename(&staging, &moved_staging).expect("move staging directory");
        create_private_directory(&staging);
        let replacement_marker = staging.join("replacement-marker");
        fs::write(&replacement_marker, b"replacement-directory").expect("replacement marker");
        execute_cleanup(
            &mut backend,
            token,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("execute through pinned directory");
        assert!(!moved_staging.join(&staged_name).exists());
        assert_eq!(
            fs::read(&replacement_marker).expect("replacement marker"),
            b"replacement-directory"
        );
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn validly_sealed_foreign_artifact_identity_is_rejected_before_backend_mutation() {
        let (root, staging, _, mut backend) =
            staged_backend("foreign-identity", [7; 32], b"private-stage-bytes");
        let staged_path = staging.join(&backend.staged_name);
        let operation_id = [0x47; 16];
        let token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        let mut foreign_claims = token.claims;
        foreign_claims.artifact_identity[0] ^= 1;
        let foreign = test_auth().seal(foreign_claims);
        assert_eq!(
            execute_cleanup(
                &mut backend,
                foreign,
                operation_id,
                7,
                CleanupAuthority::ResumeOrDiscardPrivate,
                &test_auth(),
            )
            .expect_err("foreign identity"),
            CleanupError::ArtifactChanged
        );
        assert!(staged_path.exists());
        drop(backend);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn tampered_cleanup_authorization_is_rejected_before_backend_mutation() {
        let (root, staging, _, mut backend) =
            staged_backend("tampered-token", [8; 32], b"private-stage-bytes");
        let staged_path = staging.join(&backend.staged_name);
        let operation_id = [0x48; 16];
        let mut token = plan_cleanup(
            &backend,
            operation_id,
            7,
            CleanupAuthority::ResumeOrDiscardPrivate,
            &test_auth(),
        )
        .expect("plan");
        token.claims.generation += 1;
        assert_eq!(
            execute_cleanup(
                &mut backend,
                token,
                operation_id,
                7,
                CleanupAuthority::ResumeOrDiscardPrivate,
                &test_auth(),
            )
            .expect_err("tampered token"),
            CleanupError::AuthenticationFailed
        );
        assert!(staged_path.exists());
        drop(backend);
        fs::remove_dir_all(root).expect("cleanup root");
    }
}
