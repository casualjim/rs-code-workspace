//! Version control detection and linked-checkout resolution.

use std::ffi::OsStr;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Version control system that owns a checkout.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Vcs {
  /// A Git checkout without Jujutsu metadata.
  Git,
  /// A Jujutsu checkout without a `.git` entry.
  Jujutsu,
  /// A Jujutsu checkout that also carries a `.git` entry.
  JujutsuColocated,
}

impl Vcs {
  /// Detect the VCS marker directly inside `dir`.
  pub(crate) fn detect(dir: &Path) -> Option<Self> {
    match (dir.join(".jj").exists(), dir.join(".git").exists()) {
      (true, true) => Some(Self::JujutsuColocated),
      (true, false) => Some(Self::Jujutsu),
      (false, true) => Some(Self::Git),
      (false, false) => None,
    }
  }

  /// Resolve the canonical root of the main repository that owns the checkout at `checkout`.
  ///
  /// A linked git worktree resolves through its `.git` file (`gitdir:` then
  /// `commondir`), a secondary jj workspace through its `.jj/repo` pointer. A
  /// git directory without a `commondir` (plain repository, submodule) is its
  /// own repository.
  pub(crate) fn repository_root(self, checkout: &Path) -> Result<PathBuf> {
    match self {
      Self::Git => git_repository_root(checkout),
      Self::Jujutsu | Self::JujutsuColocated => jj_repository_root(checkout),
    }
  }
}

fn jj_repository_root(checkout: &Path) -> Result<PathBuf> {
  let pointer = checkout.join(".jj").join("repo");
  if pointer.is_dir() {
    return Ok(checkout.canonicalize()?);
  }
  let target = read_to_string(&pointer)?;
  let repo_dir = checkout.join(".jj").join(target.trim()).canonicalize()?;
  // The pointer targets `<main>/.jj/repo`.
  repo_dir
    .ancestors()
    .nth(2)
    .map(Path::to_path_buf)
    .ok_or(Error::InvalidVcsPointer { path: pointer })
}

fn git_repository_root(checkout: &Path) -> Result<PathBuf> {
  let dot_git = checkout.join(".git");
  if dot_git.is_dir() {
    return Ok(checkout.canonicalize()?);
  }
  let contents = read_to_string(&dot_git)?;
  let Some(git_dir) = contents
    .lines()
    .next()
    .and_then(|line| line.strip_prefix("gitdir:"))
  else {
    return Err(Error::InvalidVcsPointer { path: dot_git });
  };
  let git_dir = checkout.join(git_dir.trim());
  if !git_dir.exists() {
    return Err(Error::InvalidVcsPointer { path: dot_git });
  }
  let common_dir_file = git_dir.join("commondir");
  if !common_dir_file.exists() {
    return Ok(checkout.canonicalize()?);
  }
  let common_dir = git_dir
    .join(read_to_string(&common_dir_file)?.trim())
    .canonicalize()?;
  // A non-bare main repository keeps its common dir at `<main>/.git`.
  if common_dir.file_name() == Some(OsStr::new(".git"))
    && let Some(main_root) = common_dir.parent()
  {
    return Ok(main_root.to_path_buf());
  }
  Ok(common_dir)
}
