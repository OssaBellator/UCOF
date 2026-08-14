use crate::private_cleanup_restart_inventory::{
    classify_external_restart_inventory, InventoryObservation,
};

fn linux_restart_artifact_identity(file: &File) -> Result<[u8; 32], &'static str> {
    let metadata = file
        .metadata()
        .map_err(|_| "restart artifact identity metadata")?;
    let mut hasher = Sha256::new();
    hasher.update(b"UCOF-TEST-PINNED-ARTIFACT-IDENTITY\0");
    hasher.update(metadata.dev().to_le_bytes());
    hasher.update(metadata.ino().to_le_bytes());
    Ok(hasher.finalize().into())
}

fn linux_restart_metadata_charge(name: &OsStr) -> u64 {
    u64::try_from(name.as_bytes().len())
        .ok()
        .and_then(|name_bytes| 64u64.checked_add(name_bytes))
        .unwrap_or(u64::MAX)
}

fn scan_linux_pinned_restart_inventory(
    staging_directory: &File,
    expected_name: &OsStr,
    expected_identity: [u8; 32],
    max_entries: usize,
    max_metadata_bytes: u64,
) -> Result<(InventoryObservation, usize, u64, bool, usize), &'static str> {
    linux_verify_procfd_directory(staging_directory)?;
    let entries = fs::read_dir(linux_procfd_directory(staging_directory))
        .map_err(|_| "restart staging directory scan")?;
    let classified = entries.map(|entry| match entry {
        Ok(entry) => {
            let name = entry.file_name();
            let is_expected_name = name == expected_name;
            let charged_bytes = linux_restart_metadata_charge(&name);
            let identity = match linux_open_relative_readonly(staging_directory, &name) {
                Ok(Some(file)) => linux_restart_artifact_identity(&file).ok(),
                Ok(None) | Err(_) => None,
            };
            (is_expected_name, identity, charged_bytes)
        }
        Err(_) => (false, None, 64),
    });
    classify_external_restart_inventory(
        classified,
        expected_identity,
        max_entries,
        max_metadata_bytes,
    )
    .map_err(|_| "restart inventory classification")
}

#[cfg(test)]
mod persistent_linux_restart_inventory_tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    fn test_root(label: &str) -> PathBuf {
        let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ucof-restart-inventory-{label}-{}-{id}",
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

    fn expected_identity(directory: &File, name: &OsStr) -> [u8; 32] {
        let file = linux_open_relative_readonly(directory, name)
            .expect("open expected")
            .expect("expected exists");
        linux_restart_artifact_identity(&file).expect("expected identity")
    }

    fn scan(
        directory: &File,
        expected_name: &OsStr,
        expected_identity: [u8; 32],
        max_entries: usize,
        max_metadata_bytes: u64,
    ) -> (InventoryObservation, usize, u64, bool, usize) {
        scan_linux_pinned_restart_inventory(
            directory,
            expected_name,
            expected_identity,
            max_entries,
            max_metadata_bytes,
        )
        .expect("scan")
    }

    #[test]
    fn pinned_scan_finds_exact_expected_identity() {
        let root = test_root("exact");
        let staging = root.join("staging");
        private_directory(&staging);
        let expected_name = OsStr::new("stage.tmp");
        fs::write(staging.join(expected_name), b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        let report = scan(&directory, expected_name, identity, 32, 4096);
        assert_eq!(report.0, InventoryObservation::ExactIdentity);
        assert_eq!(report.1, 1);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn pinned_scan_finds_expected_identity_after_rename() {
        let root = test_root("renamed");
        let staging = root.join("staging");
        private_directory(&staging);
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::rename(&expected_path, staging.join("renamed.tmp")).expect("rename expected");
        let report = scan(&directory, expected_name, identity, 32, 4096);
        assert_eq!(
            report.0,
            InventoryObservation::MissingMatchingIdentityElsewhere
        );
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn complete_pinned_scan_can_prove_absence_after_unlink() {
        let root = test_root("absent");
        let staging = root.join("staging");
        private_directory(&staging);
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"private").expect("write expected");
        fs::write(staging.join("other.tmp"), b"other").expect("write other");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::remove_file(expected_path).expect("unlink expected");
        let report = scan(&directory, expected_name, identity, 32, 4096);
        assert_eq!(
            report.0,
            InventoryObservation::MissingNoMatchingIdentityCompleteScan
        );
        assert!(!report.3);
        assert_eq!(report.4, 0);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn conflicting_expected_name_identity_is_detected_even_when_original_was_moved() {
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
        let report = scan(&directory, expected_name, identity, 32, 4096);
        assert_eq!(report.0, InventoryObservation::DifferentIdentity);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn replacing_original_staging_path_cannot_redirect_pinned_inventory() {
        let root = test_root("directory-replaced");
        let staging = root.join("staging");
        let moved = root.join("staging-moved");
        private_directory(&staging);
        let expected_name = OsStr::new("stage.tmp");
        fs::write(staging.join(expected_name), b"original").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::rename(&staging, &moved).expect("move staging");
        private_directory(&staging);
        fs::write(staging.join(expected_name), b"replacement").expect("write replacement");
        let report = scan(&directory, expected_name, identity, 32, 4096);
        assert_eq!(report.0, InventoryObservation::ExactIdentity);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn entry_bound_prevents_false_absence_on_real_directory() {
        let root = test_root("entry-bound");
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
        let report = scan(&directory, expected_name, identity, 1, 4096);
        assert_eq!(report.0, InventoryObservation::MissingScanTruncated);
        assert!(report.3);
        assert_eq!(report.1, 1);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn metadata_byte_bound_prevents_false_absence_on_real_directory() {
        let root = test_root("byte-bound");
        let staging = root.join("staging");
        private_directory(&staging);
        fs::write(staging.join("other.tmp"), b"other").expect("write other");
        let expected_name = OsStr::new("stage.tmp");
        let expected_path = staging.join(expected_name);
        fs::write(&expected_path, b"private").expect("write expected");
        let directory = open_private_directory(&staging);
        let identity = expected_identity(&directory, expected_name);
        fs::remove_file(expected_path).expect("unlink expected");
        let report = scan(&directory, expected_name, identity, 32, 1);
        assert_eq!(report.0, InventoryObservation::MissingScanTruncated);
        assert!(report.3);
        assert_eq!(report.1, 0);
        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn nofollow_symlink_child_prevents_false_absence() {
        let root = test_root("symlink");
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
        let report = scan(&directory, expected_name, identity, 32, 4096);
        assert_eq!(report.0, InventoryObservation::MissingScanTruncated);
        assert_eq!(report.4, 1);
        fs::remove_dir_all(root).expect("cleanup root");
    }
}
