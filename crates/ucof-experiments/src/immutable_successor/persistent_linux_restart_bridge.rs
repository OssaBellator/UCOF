use crate::private_cleanup_restart_bridge::{
    prepared_cleanup_disposition_from_inventory, PreparedCleanupRestartDisposition,
};

fn scan_linux_pinned_prepared_cleanup_restart(
    staging_directory: &File,
    expected_name: &OsStr,
    expected_identity: LinuxRestartExpectedIdentity,
    max_entries: usize,
    max_metadata_bytes: u64,
    max_identity_bytes: u64,
) -> Result<(PreparedCleanupRestartDisposition, usize, u64, bool, usize), &'static str> {
    let (observation, entries, metadata_bytes, truncated, unreadable) =
        scan_linux_pinned_restart_inventory(
            staging_directory,
            expected_name,
            expected_identity,
            max_entries,
            max_metadata_bytes,
            max_identity_bytes,
        )?;
    Ok((
        prepared_cleanup_disposition_from_inventory(observation),
        entries,
        metadata_bytes,
        truncated,
        unreadable,
    ))
}

#[cfg(test)]
mod persistent_linux_restart_bridge_tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    fn test_root(label: &str) -> PathBuf {
        let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ucof-restart-bridge-{label}-{}-{id}",
            std::process::id()
        ))
    }

    fn private_directory(path: &Path) {
        fs::create_dir_all(path).expect("create private directory");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("private permissions");
    }

    fn open_private_directory(path: &Path) -> File {
        linux_open_directory(path, true).expect("open private directory")
    }

    fn expected_identity(
        directory: &File,
        name: &OsStr,
    ) -> LinuxRestartExpectedIdentity {
        let file = linux_open_relative_readonly(directory, name)
            .expect("open expected")
            .expect("expected exists");
        linux_restart_expected_identity(&file).expect("expected identity")
    }

    fn disposition(
        directory: &File,
        expected_name: &OsStr,
        identity: LinuxRestartExpectedIdentity,
        max_entries: usize,
        max_metadata_bytes: u64,
        max_identity_bytes: u64,
    ) -> (PreparedCleanupRestartDisposition, usize, u64, bool, usize) {
        scan_linux_pinned_prepared_cleanup_restart(
            directory,
            expected_name,
            identity,
            max_entries,
            max_metadata_bytes,
            max_identity_bytes,
        )
        .expect("restart disposition")
    }

    #[test]
    fn exact_confirmed_identity_becomes_retry_exact_cleanup() {
        let root = test_root("retry");
        let staging = root.join("staging");
        private_directory(&staging);
        let expected_name = OsStr::new("stage.tmp");
        fs::write(staging.join(expected_name), b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        let report = disposition(&directory, expected_name, identity, 32, 4096, 4096);
        assert_eq!(
            report.0,
            PreparedCleanupRestartDisposition::RetryExactCleanup
        );
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn renamed_confirmed_identity_becomes_resolve_renamed_private() {
        let root = test_root("renamed");
        let staging = root.join("staging");
        private_directory(&staging);
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::rename(&expected_path, staging.join("renamed.tmp")).expect("rename expected");
        let report = disposition(&directory, expected_name, identity, 32, 4096, 4096);
        assert_eq!(
            report.0,
            PreparedCleanupRestartDisposition::ResolveRenamedPrivate
        );
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn complete_confirmed_absence_becomes_sync_directory_then_finalize() {
        let root = test_root("absent");
        let staging = root.join("staging");
        private_directory(&staging);
        fs::write(staging.join("other.tmp"), b"other").expect("write other");
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::remove_file(expected_path).expect("unlink expected");
        let report = disposition(&directory, expected_name, identity, 32, 4096, 4096);
        assert_eq!(
            report.0,
            PreparedCleanupRestartDisposition::SyncDirectoryThenFinalize
        );
        assert!(!report.3);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn expected_name_replacement_becomes_retain_indeterminate() {
        let root = test_root("replacement");
        let staging = root.join("staging");
        private_directory(&staging);
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"original").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::rename(&expected_path, staging.join("moved-original.tmp")).expect("move expected");
        fs::write(&expected_path, b"replacement").expect("write replacement");
        let report = disposition(&directory, expected_name, identity, 32, 4096, 4096);
        assert_eq!(
            report.0,
            PreparedCleanupRestartDisposition::RetainIndeterminate
        );
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn truncated_inventory_becomes_retain_indeterminate() {
        let root = test_root("truncated");
        let staging = root.join("staging");
        private_directory(&staging);
        for index in 0..8 {
            fs::write(staging.join(format!("other-{index}.tmp")), b"other").expect("write other");
        }
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::remove_file(expected_path).expect("unlink expected");
        let report = disposition(&directory, expected_name, identity, 1, 4096, 4096);
        assert_eq!(
            report.0,
            PreparedCleanupRestartDisposition::RetainIndeterminate
        );
        assert!(report.3);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn unreadable_or_unverified_child_becomes_retain_indeterminate() {
        let root = test_root("unreadable");
        let staging = root.join("staging");
        private_directory(&staging);
        fs::write(staging.join("target.tmp"), b"target").expect("write target");
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::remove_file(expected_path).expect("unlink expected");
        symlink("target.tmp", staging.join("link.tmp")).expect("create symlink");
        let report = disposition(&directory, expected_name, identity, 32, 4096, 4096);
        assert_eq!(
            report.0,
            PreparedCleanupRestartDisposition::RetainIndeterminate
        );
        assert_eq!(report.4, 1);
        fs::remove_dir_all(root).expect("cleanup root");
    }
}
