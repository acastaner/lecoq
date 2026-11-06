//! Copy to / Move to on the real filesystem (§8.2, issue #53).
//!
//! Unlike the trash, the clipboard or the dialogs, the file transfer is a plain function, not an
//! adapter trait: there is nothing OS-specific to fake. The one step that can't be reproduced with
//! temp folders on a single filesystem — a cross-device move, where the rename fails and the move
//! falls back to copy-then-delete — is reached by injecting the rename step
//! ([`move_to_with`]), so it has a unit test of its own.
//!
//! Both operations put the file in a Target folder under its own name. A name already taken in the
//! Target folder is refused (never overwritten); a Move into the file's own folder is a no-op.

use std::io;
use std::path::{Path, PathBuf};

/// What a Copy to / Move to did (issue #53): enough for the status bar to say what happened.
#[derive(Debug, PartialEq, Eq)]
pub enum Transfer {
    /// The file is now at this path (its name in the Target folder).
    Done(PathBuf),
    /// A file of that name is already in the Target folder (stories 27, 28): nothing was done.
    Exists,
    /// A Move into the file's own folder (story 29): the file is already there, nothing to do and
    /// nothing to say.
    AlreadyThere,
}

/// Copy `source` into `target_dir` under its own name (story 2). A name already taken there is
/// refused ([`Transfer::Exists`], stories 27–28), including the file's own folder.
pub fn copy_to(source: &Path, target_dir: &Path) -> Result<Transfer, String> {
    let target = target_path(source, target_dir)?;
    if target.exists() {
        return Ok(Transfer::Exists);
    }
    std::fs::copy(source, &target).map_err(|e| e.to_string())?;
    Ok(Transfer::Done(target))
}

/// Move `source` into `target_dir` under its own name (story 1). Its own folder is a no-op
/// ([`Transfer::AlreadyThere`], story 29); a name already taken there is refused
/// ([`Transfer::Exists`], story 27). A cross-device move — the rename fails — falls back to
/// copy-then-delete (story 30); if the delete then fails the copy is kept, the original left in
/// place, and a move error returned (story 31).
pub fn move_to(source: &Path, target_dir: &Path) -> Result<Transfer, String> {
    move_to_with(source, target_dir, |from, to| std::fs::rename(from, to))
}

/// [`move_to`] with the rename step injected, so the cross-device fallback can be unit-tested
/// without a second filesystem.
pub fn move_to_with(
    source: &Path,
    target_dir: &Path,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<Transfer, String> {
    let target = target_path(source, target_dir)?;
    // The file's own folder: it is already where it is being moved to.
    if same_file(source, &target) {
        return Ok(Transfer::AlreadyThere);
    }
    if target.exists() {
        return Ok(Transfer::Exists);
    }
    if rename(source, &target).is_ok() {
        return Ok(Transfer::Done(target));
    }
    // The rename failed (a different filesystem, say): copy, then remove the original. A failed
    // copy is a move failure with nothing changed; a failed delete leaves the copy in place and is
    // still reported as a move failure, so no data is lost (story 31).
    std::fs::copy(source, &target).map_err(|e| e.to_string())?;
    std::fs::remove_file(source).map_err(|e| e.to_string())?;
    Ok(Transfer::Done(target))
}

/// The path the file would take in `target_dir`: the Target folder joined with the file's name.
fn target_path(source: &Path, target_dir: &Path) -> Result<PathBuf, String> {
    let name = source
        .file_name()
        .ok_or_else(|| "the file has no name".to_owned())?;
    Ok(target_dir.join(name))
}

/// Do `a` and `b` name the same file? Compared by canonical path so a Target folder spelled
/// differently (a symlink, a trailing slash) still counts as the file's own folder.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .ok()
            .is_some_and(|a| std::fs::canonicalize(b).ok() == Some(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seam 2 (spec §9): a cross-device move. The rename is injected to always fail, so the move
    /// must fall back to copy-then-delete — the one path temp folders on a single filesystem can't
    /// reproduce. The injected error stands in for `EXDEV`.
    #[test]
    fn a_cross_device_move_falls_back_to_copy_then_delete() {
        let dir = tempfile::tempdir().unwrap();
        let from_dir = dir.path().join("from");
        let to_dir = dir.path().join("to");
        std::fs::create_dir_all(&from_dir).unwrap();
        std::fs::create_dir_all(&to_dir).unwrap();
        let source = from_dir.join("a.jpg");
        std::fs::write(&source, b"pixels").unwrap();

        let outcome = move_to_with(&source, &to_dir, |_, _| {
            Err(io::Error::from(io::ErrorKind::CrossesDevices))
        })
        .expect("the move falls back to a copy");

        assert_eq!(outcome, Transfer::Done(to_dir.join("a.jpg")));
        assert_eq!(std::fs::read(to_dir.join("a.jpg")).unwrap(), b"pixels");
        assert!(!source.exists(), "the original was removed after the copy");
    }

    /// When the cross-device copy succeeds but the original can't be removed, the copy is kept, the
    /// original is left in place, and the move is reported as a failure (story 31): no data is lost.
    #[test]
    fn a_cross_device_move_whose_delete_fails_keeps_the_copy_and_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let from_dir = dir.path().join("from");
        let to_dir = dir.path().join("to");
        std::fs::create_dir_all(&from_dir).unwrap();
        std::fs::create_dir_all(&to_dir).unwrap();
        let source = from_dir.join("a.jpg");
        std::fs::write(&source, b"pixels").unwrap();
        // Removing a file needs write permission on its folder, not the file: a read-only source
        // folder makes the delete fail while the read for the copy still works. (The gate runs as
        // a non-root user, so the mode is enforced.)
        set_readonly(&from_dir, true);

        let result = move_to_with(&source, &to_dir, |_, _| {
            Err(io::Error::from(io::ErrorKind::CrossesDevices))
        });

        set_readonly(&from_dir, false); // so the temp dir can be cleaned up
        assert!(
            result.is_err(),
            "a kept copy with a left-behind original is a move failure"
        );
        assert_eq!(
            std::fs::read(to_dir.join("a.jpg")).unwrap(),
            b"pixels",
            "the copy is kept"
        );
        assert!(source.exists(), "the original is left in place");
    }

    fn set_readonly(dir: &Path, readonly: bool) {
        let mut perms = std::fs::metadata(dir).unwrap().permissions();
        perms.set_readonly(readonly);
        std::fs::set_permissions(dir, perms).unwrap();
    }
}
