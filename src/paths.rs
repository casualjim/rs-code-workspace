//! Normalized workspace-relative path helpers.
//!
//! [`WorkspacePath`] is the crate-owned path type used by discovery, target
//! parsing, and package ownership APIs. It trims empty inputs to the workspace
//! root, normalizes relative segments, and rejects paths that escape the
//! workspace boundary.

use std::{borrow::Borrow, path::Path, str::FromStr};

use relative_path::RelativePathBuf;

use crate::{Error, Result};

/// Normalized workspace-relative path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkspacePath(RelativePathBuf);

impl std::hash::Hash for WorkspacePath {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.as_str().hash(state);
  }
}

impl WorkspacePath {
  /// Create an empty workspace path.
  #[must_use]
  pub fn empty() -> Self {
    Self(RelativePathBuf::new())
  }

  /// Borrow the normalized path as a string slice.
  #[must_use]
  pub fn as_str(&self) -> &str {
    self.0.as_str()
  }

  /// Return whether the path is empty.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.0.as_str().is_empty()
  }

  /// Return whether this path is equal to or nested under `base`.
  #[must_use]
  pub fn starts_with(&self, base: &Self) -> bool {
    if base.is_empty() {
      return true;
    }

    let self_path = Path::new(self.as_str());
    let base_path = Path::new(base.as_str());
    self_path == base_path || self_path.starts_with(base_path)
  }

  /// Build a workspace path from a relative filesystem path.
  ///
  /// # Arguments
  /// - `path`: Relative filesystem path to normalize.
  ///
  /// # Errors
  /// Returns an error when the path cannot be normalized or escapes the workspace.
  pub fn from_relative_path(path: &Path) -> Result<Self> {
    if path.as_os_str().is_empty() {
      return Ok(Self::empty());
    }
    let relative = RelativePathBuf::from_path(path)?.normalize();
    Self::from_relative_buf(&relative)
  }

  /// Parse a workspace path from a string.
  ///
  /// # Arguments
  /// - `value`: Relative path text to normalize.
  ///
  /// # Errors
  /// Returns an error when the input is absolute, invalid, or escapes the workspace.
  pub fn parse(value: &str) -> Result<Self> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "." {
      return Ok(Self::empty());
    }
    let input = Path::new(trimmed);
    if input.is_absolute() {
      return Err(Error::AbsolutePathNotAllowed);
    }
    let relative = RelativePathBuf::from_path(input)?.normalize();
    Self::from_relative_buf(&relative)
  }

  /// Build a workspace path from an already-relative buffer.
  ///
  /// # Arguments
  /// - `relative`: Relative path buffer to normalize.
  ///
  /// # Errors
  /// Returns an error when the path escapes the workspace.
  pub fn from_relative_buf(relative: &RelativePathBuf) -> Result<Self> {
    let normalized = reject_outside_workspace(relative)?;
    Ok(Self(normalized))
  }
}

impl std::fmt::Display for WorkspacePath {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(self.0.as_str())
  }
}

impl AsRef<str> for WorkspacePath {
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

impl Borrow<str> for WorkspacePath {
  fn borrow(&self) -> &str {
    self.as_str()
  }
}

impl FromStr for WorkspacePath {
  type Err = Error;

  fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
    Self::parse(s)
  }
}

impl TryFrom<&str> for WorkspacePath {
  type Error = Error;

  fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
    value.parse()
  }
}

fn reject_outside_workspace(relative: &RelativePathBuf) -> Result<RelativePathBuf> {
  let normalized = relative.normalize();
  let value = normalized.as_str();
  if value == ".." || value.starts_with("../") {
    return Err(Error::PathOutsideWorkspace);
  }
  Ok(normalized)
}

#[cfg(test)]
mod tests {
  use super::*;

  use pretty_assertions::assert_eq;

  #[test]
  fn workspace_path_rejects_absolute_parse() {
    let _ =
      WorkspacePath::parse("/tmp/file.txt").expect_err("expected absolute path to be rejected");
  }

  #[test]
  fn workspace_path_rejects_outside_dotdot() {
    let _ =
      WorkspacePath::parse("../outside.txt").expect_err("expected outside path to be rejected");
  }

  #[test]
  fn workspace_path_normalizes_dot_segments() {
    let normalized = WorkspacePath::parse("a/./b/../b").unwrap();
    assert_eq!(normalized.as_str(), "a/b");
  }

  #[test]
  fn workspace_path_empty_is_empty() {
    let normalized = WorkspacePath::parse("   ").unwrap();
    assert!(normalized.is_empty());

    let normalized = WorkspacePath::parse(".").unwrap();
    assert!(normalized.is_empty());
  }

  #[test]
  fn workspace_path_starts_with_ancestry() {
    let base = WorkspacePath::parse("packages/app").unwrap();
    let child = WorkspacePath::parse("packages/app/src/lib.rs").unwrap();
    let sibling = WorkspacePath::parse("packages/application/src/lib.rs").unwrap();

    assert!(child.starts_with(&base));
    assert!(!sibling.starts_with(&base));
    assert!(base.starts_with(&base));
  }
}
