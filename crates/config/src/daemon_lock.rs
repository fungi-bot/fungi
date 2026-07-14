use anyhow::{Context, Result};
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
};

pub const DAEMON_LOCK_FILE: &str = "daemon.lock";

pub fn daemon_lock_path(fungi_dir: &Path) -> PathBuf {
    fungi_dir.join(DAEMON_LOCK_FILE)
}

#[cfg(target_os = "android")]
fn try_lock(file: &File) -> std::result::Result<(), TryLockError> {
    use rustix::fs::{FlockOperation, flock};

    match flock(file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(()),
        Err(error) if error == rustix::io::Errno::WOULDBLOCK => Err(TryLockError::WouldBlock),
        Err(error) => Err(TryLockError::Error(error.into())),
    }
}

#[cfg(not(target_os = "android"))]
fn try_lock(file: &File) -> std::result::Result<(), TryLockError> {
    file.try_lock()
}

pub struct DaemonInstanceLock {
    _file: File,
}

impl DaemonInstanceLock {
    pub fn acquire(fungi_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(fungi_dir).with_context(|| {
            format!("Failed to create Fungi directory: {}", fungi_dir.display())
        })?;
        let path = daemon_lock_path(fungi_dir);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .with_context(|| format!("Failed to open daemon lock file: {}", path.display()))?;

        match try_lock(&file) {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => anyhow::bail!(
                "Fungi daemon is already running for {}",
                fungi_dir.display()
            ),
            Err(TryLockError::Error(error)) => Err(error).with_context(|| {
                format!("Failed to lock daemon instance file: {}", path.display())
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_daemon_lock_can_be_held_for_a_fungi_dir() {
        let dir = tempfile::tempdir().unwrap();
        let first = DaemonInstanceLock::acquire(dir.path()).unwrap();

        let error = DaemonInstanceLock::acquire(dir.path())
            .err()
            .expect("second daemon lock should fail");

        assert!(error.to_string().contains("already running"));
        drop(first);
        DaemonInstanceLock::acquire(dir.path()).unwrap();
    }

    #[test]
    fn stale_lock_file_does_not_prevent_a_new_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let lock = DaemonInstanceLock::acquire(dir.path()).unwrap();
        drop(lock);

        assert!(daemon_lock_path(dir.path()).exists());
        DaemonInstanceLock::acquire(dir.path()).unwrap();
    }
}
