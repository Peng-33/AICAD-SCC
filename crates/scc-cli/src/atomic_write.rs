use scc_core::error::{ErrorStage, SccError, StructuredError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Options controlling how atomic writes behave when the target path already
/// exists.
#[derive(Clone, Copy, Debug, Default)]
pub struct WriteOptions {
    /// When `true`, an existing target file may be replaced.
    /// When `false` (the default), attempting to overwrite returns an error.
    pub force: bool,
}

/// Prepares a single atomic write to `target`.
///
/// The content is first written to a temporary file in the same directory as
/// `target`, flushed, and synced. Call [`AtomicWriter::commit`] to publish the
/// file atomically.
#[derive(Debug)]
pub struct AtomicWriter {
    temp: tempfile::NamedTempFile,
}

enum CommitFailure {
    BeforePublish(SccError),
    AfterPublish(SccError),
}

impl CommitFailure {
    fn into_error(self) -> SccError {
        match self {
            CommitFailure::BeforePublish(error) | CommitFailure::AfterPublish(error) => error,
        }
    }
}

impl AtomicWriter {
    /// Stage `content` for writing to `target`.
    ///
    /// Existing target contents are not loaded during staging. Pair commits
    /// capture only the first target immediately before publishing it.
    pub fn prepare(target: &Path, content: &[u8]) -> Result<Self, SccError> {
        let parent = parent_dir(target);
        let mut temp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| SccError::Io(wrap_io_error(parent, e)))?;

        if let Err(e) = temp.write_all(content) {
            return Err(SccError::Io(wrap_io_error(temp.path(), e)));
        }
        if let Err(e) = temp.flush() {
            return Err(SccError::Io(wrap_io_error(temp.path(), e)));
        }
        if let Err(e) = temp.as_file().sync_all() {
            return Err(SccError::Io(wrap_io_error(temp.path(), e)));
        }

        Ok(AtomicWriter { temp })
    }

    /// Atomically publish the staged content to the target path.
    ///
    /// - If `options.force` is `false` and the target exists, the operation
    ///   fails without overwriting. `tempfile`'s `persist_noclobber` is used
    ///   to make this check as atomic as the underlying OS allows.
    /// - If `options.force` is `true` and the target exists, the target is
    ///   replaced atomically on Unix; on Windows `tempfile` performs the
    ///   platform-specific replacement.
    ///
    pub fn commit(self, target: &Path, options: WriteOptions) -> Result<(), SccError> {
        self.commit_inner(target, options)
            .map_err(CommitFailure::into_error)
    }

    fn commit_inner(self, target: &Path, options: WriteOptions) -> Result<(), CommitFailure> {
        if !options.force && target.exists() {
            return Err(CommitFailure::BeforePublish(SccError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{} already exists; use --force to overwrite",
                    target.display()
                ),
            ))));
        }

        let result = if options.force {
            self.temp.persist(target)
        } else {
            self.temp.persist_noclobber(target)
        };

        match result {
            Ok(_) => {
                sync_parent_dir(target).map_err(|error| CommitFailure::AfterPublish(
                    SccError::Structured(Box::new(StructuredError::new(
                        "SCC-CLI-003",
                        ErrorStage::Cli,
                        format!(
                            "output {} was published but parent-directory durability sync failed: {}",
                            target.display(),
                            error
                        ),
                    )))
                ))?;
                Ok(())
            }
            Err(e) => Err(CommitFailure::BeforePublish(SccError::Structured(
                Box::new(StructuredError::new(
                    "SCC-CLI-003",
                    ErrorStage::Cli,
                    format!("failed to write output {}: {}", target.display(), e),
                )),
            ))),
        }
    }
}

/// A pair of atomic writes that should be committed together.
///
/// The implementation prepares both temporary files first, then commits them
/// in order. If the second commit fails, the first file is restored to its
/// original contents if it previously existed. This provides a *best-effort*
/// two-phase commit across two independent files; it is not a strict
/// distributed transaction.
#[derive(Debug)]
pub struct AtomicPair {
    first_target: PathBuf,
    first: AtomicWriter,
    first_backup: Option<Vec<u8>>,
    second_target: PathBuf,
    second: AtomicWriter,
    second_backup: Option<Vec<u8>>,
}

impl AtomicPair {
    pub fn prepare(
        first_target: &Path,
        first_content: &[u8],
        second_target: &Path,
        second_content: &[u8],
    ) -> Result<Self, SccError> {
        if first_target == second_target {
            return Err(SccError::Structured(Box::new(StructuredError::new(
                "SCC-CLI-004",
                ErrorStage::Cli,
                "first and second output paths must not be the same",
            ))));
        }

        Ok(AtomicPair {
            first_target: first_target.to_path_buf(),
            first: AtomicWriter::prepare(first_target, first_content)?,
            first_backup: capture_backup(first_target)?,
            second_target: second_target.to_path_buf(),
            second: AtomicWriter::prepare(second_target, second_content)?,
            second_backup: capture_backup(second_target)?,
        })
    }

    /// Commit both pending writes.
    ///
    /// If the first commit succeeds and the second fails, the first file is
    /// restored to its original contents (or removed if it did not previously
    /// exist). The returned error explains that a rollback was attempted.
    pub fn commit(self, options: WriteOptions) -> Result<(), SccError> {
        // Refuse the whole pair before publishing either file. The individual
        // commits still repeat this check to close races with other writers.
        if !options.force {
            for target in [&self.first_target, &self.second_target] {
                if target.exists() {
                    return Err(SccError::Io(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "{} already exists; use --force to overwrite",
                            target.display()
                        ),
                    )));
                }
            }
        }

        match self.first.commit_inner(&self.first_target, options) {
            Ok(()) => {}
            Err(CommitFailure::BeforePublish(error)) => {
                return Err(SccError::Structured(Box::new(StructuredError::new(
                    "SCC-CLI-003",
                    ErrorStage::Cli,
                    format!(
                        "failed to write first output {}: {}",
                        self.first_target.display(),
                        error
                    ),
                ))));
            }
            Err(CommitFailure::AfterPublish(error)) => {
                let rollback = restore_target(&self.first_target, self.first_backup.as_deref());
                return Err(SccError::Structured(Box::new(StructuredError::new(
                    "SCC-CLI-003",
                    ErrorStage::Cli,
                    format!(
                        "first output {} was published but durability sync failed: {}. rollback: {:?}",
                        self.first_target.display(), error, rollback
                    ),
                ))));
            }
        }

        match self.second.commit_inner(&self.second_target, options) {
            Ok(()) => Ok(()),
            Err(failure) => {
                let (error, second_rollback) = match failure {
                    CommitFailure::BeforePublish(error) => (error, None),
                    CommitFailure::AfterPublish(error) => (
                        error,
                        Some(restore_target(
                            &self.second_target,
                            self.second_backup.as_deref(),
                        )),
                    ),
                };
                let first_rollback =
                    restore_target(&self.first_target, self.first_backup.as_deref());

                Err(SccError::Structured(Box::new(StructuredError::new(
                    "SCC-CLI-003",
                    ErrorStage::Cli,
                    format!(
                        "failed to write second output {}: {}. first rollback for {}: {:?}; second rollback after publish: {:?}",
                        self.second_target.display(),
                        error,
                        self.first_target.display(),
                        first_rollback,
                        second_rollback
                    ),
                ))))
            }
        }
    }
}

/// Convenience helper that prepares and commits a single atomic write.
pub fn atomic_write(path: &Path, content: &[u8], options: WriteOptions) -> Result<(), SccError> {
    AtomicWriter::prepare(path, content)?.commit(path, options)
}

fn wrap_io_error(path: &Path, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{}: {}", path.display(), e))
}

fn capture_backup(path: &Path) -> Result<Option<Vec<u8>>, SccError> {
    if path.exists() {
        std::fs::read(path)
            .map(Some)
            .map_err(|error| SccError::Io(wrap_io_error(path, error)))
    } else {
        Ok(None)
    }
}

fn restore_target(path: &Path, backup: Option<&[u8]>) -> Result<(), String> {
    match backup {
        Some(bytes) => atomic_write(path, bytes, WriteOptions { force: true })
            .map_err(|error| format!("atomic restore failed: {error}")),
        None if path.exists() => std::fs::remove_file(path)
            .map_err(|error| format!("remove failed: {error}"))
            .and_then(|_| {
                sync_parent_dir(path).map_err(|error| format!("directory sync failed: {error}"))
            }),
        None => Ok(()),
    }
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn sync_parent_dir(path: &Path) -> io::Result<()> {
    let parent = parent_dir(path);

    #[cfg(unix)]
    {
        let dir = std::fs::File::open(parent).map_err(|e| wrap_io_error(parent, e))?;
        dir.sync_all().map_err(|e| wrap_io_error(parent, e))?;
    }

    #[cfg(windows)]
    {
        // Windows does not provide a portable, stable API for syncing a
        // directory via std. The temporary file itself has already been synced.
        let _ = parent;
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = parent;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "scc-atomic-write-test-{}-{}",
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_creates_target() {
        let dir = tmp_dir();
        let target = dir.join("target.txt");

        atomic_write(&target, b"hello", WriteOptions::default()).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
        assert_no_temp_left(&dir);
    }

    #[test]
    fn atomic_write_refuses_overwrite_without_force() {
        let dir = tmp_dir();
        let target = dir.join("target.txt");
        fs::write(&target, "old").unwrap();

        let err = atomic_write(&target, b"new", WriteOptions::default()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("already exists"));
        assert!(msg.contains("--force"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert_no_temp_left(&dir);
    }

    #[test]
    fn atomic_write_allows_overwrite_with_force() {
        let dir = tmp_dir();
        let target = dir.join("target.txt");
        fs::write(&target, "old").unwrap();

        atomic_write(&target, b"new", WriteOptions { force: true }).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_no_temp_left(&dir);
    }

    #[test]
    fn atomic_pair_commits_both() {
        let dir = tmp_dir();
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");

        let pair = AtomicPair::prepare(&first, b"a", &second, b"b").unwrap();
        pair.commit(WriteOptions::default()).unwrap();

        assert_eq!(fs::read_to_string(&first).unwrap(), "a");
        assert_eq!(fs::read_to_string(&second).unwrap(), "b");
        assert_no_temp_left(&dir);
    }

    #[test]
    fn atomic_pair_preflight_preserves_both_existing_targets() {
        let dir = tmp_dir();
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");
        fs::write(&first, "original-first").unwrap();
        fs::write(&second, "original-second").unwrap();

        let pair = AtomicPair::prepare(&first, b"new-first", &second, b"new-second").unwrap();
        let err = pair.commit(WriteOptions::default()).unwrap_err();
        let msg = format!("{err}");

        // Without --force, the pair is rejected before either target changes.
        assert!(msg.contains("already exists"));
        // First file should be rolled back to its original contents.
        assert_eq!(fs::read_to_string(&first).unwrap(), "original-first");
        assert_eq!(fs::read_to_string(&second).unwrap(), "original-second");
        assert_no_temp_left(&dir);
    }

    #[test]
    fn atomic_pair_restores_first_when_second_publish_fails() {
        let dir = tmp_dir();
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");
        fs::write(&first, "original-first").unwrap();

        let pair = AtomicPair::prepare(&first, b"new-first", &second, b"new-second").unwrap();
        // A directory cannot be replaced by the staged file on either Unix or
        // Windows, so this deterministically fails only after the first file
        // has been published.
        fs::create_dir(&second).unwrap();

        let error = pair.commit(WriteOptions { force: true }).unwrap_err();
        assert!(format!("{error}").contains("first rollback"));
        assert_eq!(fs::read_to_string(&first).unwrap(), "original-first");
        assert!(second.is_dir());
        assert_no_temp_left(&dir);
    }

    #[test]
    fn atomic_pair_rejects_identical_paths() {
        let dir = tmp_dir();
        let path = dir.join("same.txt");

        let err = AtomicPair::prepare(&path, b"a", &path, b"b").unwrap_err();
        assert!(format!("{err}").contains("must not be the same"));
    }

    #[test]
    fn bare_relative_target_uses_current_directory() {
        assert_eq!(parent_dir(Path::new("target.txt")), Path::new("."));
    }

    fn assert_no_temp_left(dir: &Path) {
        let left: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(".tmp"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(left.is_empty(), "temporary files left behind: {:?}", left);
    }
}
