//! Workspace package expansion helpers.
//!
//! The detector keeps wildcard support intentionally small and stable:
//! - negated patterns beginning with `!` are ignored
//! - leading `./` prefixes are normalized away
//! - patterns without `*` are treated as normalized relative package paths
//! - a single `*` expands exactly one directory layer under the base prefix
//! - this is not full glob support, and malformed patterns are ignored on a
//!   best-effort basis during internal detection

use std::collections::BTreeSet;
use std::fs::read_dir;
use std::path::{Path, PathBuf};

use crate::paths::WorkspacePath;
use crate::types::{Ecosystem, WorkspacePackage};

pub(super) fn push_workspace_package(
  packages: &mut Vec<WorkspacePackage>,
  workspace_root: &Path,
  pattern: &str,
  ecosystem: Ecosystem,
) {
  for path in expand_workspace_pattern(workspace_root, pattern) {
    let pkg_path = workspace_root.join(path.as_str());
    if !pkg_path.exists() {
      continue;
    }
    let name = pkg_path
      .file_name()
      .and_then(|name| name.to_str())
      .unwrap_or(path.as_str())
      .to_string();
    packages.push(WorkspacePackage::new(name, path, ecosystem));
  }
}

fn expand_workspace_pattern(workspace_root: &Path, pattern: &str) -> Vec<WorkspacePath> {
  let mut pattern = pattern.trim();
  if pattern.starts_with('!') {
    return Vec::new();
  }
  if let Some(rest) = pattern.strip_prefix("./") {
    pattern = rest;
  }
  if pattern.is_empty() || pattern == "." {
    return vec![WorkspacePath::empty()];
  }
  if !pattern.contains('*') {
    return match WorkspacePath::parse(pattern) {
      Ok(path) => vec![path],
      Err(err) => {
        tracing::warn!(error = %err, pattern, "workspace pattern invalid (ignored)");
        Vec::new()
      }
    };
  }

  let wildcard_index = pattern.find('*').unwrap_or(pattern.len());
  let prefix = pattern[..wildcard_index].trim_end_matches(['/', '\\']);
  let suffix = pattern[wildcard_index..].trim_start_matches('*');
  let base_dir = if prefix.is_empty() {
    workspace_root.to_path_buf()
  } else {
    workspace_root.join(prefix)
  };
  let mut expanded = Vec::new();
  let Ok(entries) = read_dir(&base_dir) else {
    return expanded;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if !path.is_dir() {
      continue;
    }
    let dir_name = path
      .file_name()
      .and_then(|name| name.to_str())
      .unwrap_or("");
    if dir_name.is_empty() {
      continue;
    }
    let mut candidate = PathBuf::new();
    if !prefix.is_empty() {
      candidate.push(prefix);
    }
    candidate.push(dir_name);
    let suffix = suffix.trim_start_matches(['/', '\\']);
    if !suffix.is_empty() {
      candidate.push(suffix);
    }

    if let Ok(path) = WorkspacePath::from_relative_path(&candidate) {
      expanded.push(path);
    }
  }
  expanded
}

pub(super) fn package_name_for_dir(workspace_root: &Path, dir: &str) -> String {
  if dir == "." || dir.is_empty() {
    return workspace_root
      .file_name()
      .and_then(|name| name.to_str())
      .unwrap_or("workspace")
      .to_string();
  }
  Path::new(dir)
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or(dir)
    .to_string()
}

pub(super) fn default_package(workspace_root: &Path, ecosystem: Ecosystem) -> WorkspacePackage {
  let name = workspace_root
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("workspace")
    .to_string();
  WorkspacePackage::new(name, WorkspacePath::empty(), ecosystem)
}

pub(super) fn ecosystem_label(ecosystems: &BTreeSet<Ecosystem>) -> String {
  if ecosystems.len() == 1 {
    ecosystems.iter().next().map_or_else(
      || "unknown".to_string(),
      |ecosystem| ecosystem.as_str().to_string(),
    )
  } else {
    "multi".to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use std::env;
  use std::fs::{create_dir_all, remove_dir_all};
  use std::time::{SystemTime, UNIX_EPOCH};

  use pretty_assertions::assert_eq;

  struct TempWorkspaceDir {
    path: PathBuf,
  }

  impl TempWorkspaceDir {
    fn new(prefix: &str) -> Self {
      let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
      let path = env::temp_dir().join(format!(
        "code-workspace-packages-{prefix}-{}-{unique}",
        std::process::id()
      ));
      create_dir_all(&path).expect("failed to create temp workspace dir");
      Self { path }
    }

    fn path(&self) -> &Path {
      &self.path
    }
  }

  impl Drop for TempWorkspaceDir {
    fn drop(&mut self) {
      let _ = remove_dir_all(&self.path);
    }
  }

  #[test]
  fn wildcard_expansion_ignores_negation_and_strips_dot_prefix() {
    let temp = TempWorkspaceDir::new("dot-prefix");
    let root = temp.path().join("repo");
    create_dir_all(root.join("packages/app")).expect("failed to create app dir");
    create_dir_all(root.join("packages/lib")).expect("failed to create lib dir");

    assert!(expand_workspace_pattern(&root, "!packages/*").is_empty());

    let mut expanded = expand_workspace_pattern(&root, "./packages/*")
      .into_iter()
      .map(|path| path.to_string())
      .collect::<Vec<_>>();
    expanded.sort();

    assert_eq!(expanded, vec!["packages/app", "packages/lib"]);
  }

  #[test]
  fn wildcard_expansion_stays_single_directory_layer() {
    let temp = TempWorkspaceDir::new("single-layer");
    let root = temp.path().join("repo");
    create_dir_all(root.join("packages/app/src")).expect("failed to create app src dir");
    create_dir_all(root.join("packages/lib/src")).expect("failed to create lib src dir");
    create_dir_all(root.join("packages/app/nested/src")).expect("failed to create nested src dir");

    let mut expanded = expand_workspace_pattern(&root, "packages/*/src")
      .into_iter()
      .map(|path| path.to_string())
      .collect::<Vec<_>>();
    expanded.sort();

    assert_eq!(expanded, vec!["packages/app/src", "packages/lib/src"]);
  }
}
