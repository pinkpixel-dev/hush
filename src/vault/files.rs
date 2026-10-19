//! Rules for the vault folder: real files and folders only (no symlinks),
//! owner-only permissions, and writes that survive a crash or power cut.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Fails if `path` is a symlink. A missing path is fine.
pub fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => bail!(
            "{} is a symlink. hush only uses real files and folders for the vault, \
             so point HUSH_DIR at the real location",
            path.display()
        ),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("couldn't check {}", path.display())),
    }
}

/// Checks an existing vault folder: a real folder that only its owner can use.
/// A missing folder is fine, callers report that themselves.
pub fn check_private_dir(dir: &Path) -> Result<()> {
    reject_symlink(dir)?;
    let meta = match fs::metadata(dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("couldn't check {}", dir.display())),
    };
    if !meta.is_dir() {
        bail!("{} isn't a folder", dir.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            bail!(
                "{} is open to other users (mode {mode:o}). Run `chmod 700 {}` to fix it",
                dir.display(),
                dir.display()
            );
        }
    }
    Ok(())
}

/// Creates the vault folder (or tightens an existing one) with `0700` permissions.
pub fn create_private_dir(dir: &Path) -> Result<()> {
    reject_symlink(dir)?;
    fs::create_dir_all(dir).with_context(|| format!("couldn't create {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes to a temp file and renames it over the target, so a crash mid-write
/// never leaves a half-written vault behind. The temp name includes the pid so
/// two processes never write into the same temp file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = write_and_rename(&tmp, path, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_and_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    // A crashed hush that had the same pid can leave a stale temp file. Clearing
    // it lets `create_new` refuse anything else, including a planted symlink.
    match fs::remove_file(tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("couldn't clear {}", tmp.display())),
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(tmp).with_context(|| format!("couldn't write {}", tmp.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(tmp, path).with_context(|| format!("couldn't replace {}", path.display()))?;
    sync_parent(path)
}

/// The rename lives in the folder's entries, so the folder needs its own fsync
/// for the new file to survive a power cut.
fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .with_context(|| format!("couldn't sync {}", dir.display()))?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    #[test]
    fn write_atomic_never_follows_a_planted_temp_symlink() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("vault.age");
        let victim = dir.path().join("victim");
        fs::write(&victim, b"untouched").unwrap();
        let tmp = target.with_extension(format!("{}.tmp", std::process::id()));
        symlink(&victim, &tmp).unwrap();

        write_atomic(&target, b"new").unwrap();

        assert_eq!(fs::read(&victim).unwrap(), b"untouched");
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!tmp.exists());
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn private_dir_rules() {
        let root = tempfile::TempDir::new().unwrap();
        let dir = root.path().join("vault");
        assert!(check_private_dir(&dir).is_ok(), "a missing folder is the caller's problem");

        create_private_dir(&dir).unwrap();
        check_private_dir(&dir).unwrap();

        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        let err = check_private_dir(&dir).unwrap_err().to_string();
        assert!(err.contains("chmod 700"), "{err}");

        let link = root.path().join("link");
        symlink(&dir, &link).unwrap();
        assert!(check_private_dir(&link).unwrap_err().to_string().contains("symlink"));
        assert!(create_private_dir(&link).is_err());
    }
}
