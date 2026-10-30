//! Workspace-root discovery markers used by [`crate::Workspace::discover`].
//!
//! Discovery walks upward until it finds one of the supported markers below:
//! - `.git` or `.jj` (a VCS marker beats every marker below)
//! - `Cargo.toml` with a `[workspace]` table
//! - `package.json` with a `workspaces` field, or `pnpm-workspace.yaml`
//! - `go.work`
//! - Gradle settings files (`settings.gradle`, `settings.gradle.kts`)
//! - Maven `pom.xml` files that declare `<modules>`
//! - `build.sbt`
//! - Deno `deno.json` / `deno.jsonc` files with a top-level `workspace`
//! - Python `pyproject.toml` files with `tool.uv.workspace`
//! - Bazel markers: `WORKSPACE`, `WORKSPACE.bazel`, or `MODULE.bazel`
//! - .NET solutions or projects (`.sln`, `.slnx`, `.csproj`, `.fsproj`, `.vbproj`)
//!
//! The detector also recognizes a few best-effort ecosystem markers such as
//! `melos.yaml`, `cabal.project`, `stack.yaml`, `turbo.json`, `lerna.json`,
//! `nx.json`, `pubspec.yaml` with `workspace`, and `mix.exs` files that declare
//! umbrella apps. These heuristics are intentionally simple and only answer
//! whether a directory should be treated as a workspace root.

use std::ffi::OsStr;
use std::fs::{read_dir, read_to_string};
use std::path::{Path, PathBuf};

use toml_edit::DocumentMut;

use crate::detector::detect_ecosystem_configs;
use crate::parsers::{parse_elixir_apps_path, strip_json_comments};
use crate::paths::WorkspacePath;
use crate::types::DiscoveryWarning;
use crate::vcs::Vcs;
use crate::{DEPENDENCY_MANIFEST_NAMES, Error, Result};
const SIMPLE_MARKERS: &[&str] = &[
  "pnpm-workspace.yaml",
  "melos.yaml",
  "cabal.project",
  "stack.yaml",
  "go.work",
  "settings.gradle",
  "settings.gradle.kts",
  "build.sbt",
  "turbo.json",
  "lerna.json",
  "nx.json",
];
const BAZEL_MARKERS: &[&str] = &["WORKSPACE", "WORKSPACE.bazel", "MODULE.bazel"];

/// Lowercase file names, beyond [`SIMPLE_MARKERS`] and the dependency manifests, that mark a project.
const PROJECT_FILES: &[&str] = &[
  "pyproject.toml",
  "pubspec.yaml",
  "mix.exs",
  "workspace",
  "workspace.bazel",
  "module.bazel",
  "deno.json",
  "deno.jsonc",
];
const PROJECT_EXTENSIONS: &[&str] = &["sln", "slnx", "csproj", "fsproj", "vbproj", "gemspec"];

/// Return true when a lowercase file name marks its directory as a project or workspace.
pub(super) fn is_project_file(lower_name: &str) -> bool {
  DEPENDENCY_MANIFEST_NAMES.contains(&lower_name)
    || SIMPLE_MARKERS.contains(&lower_name)
    || PROJECT_FILES.contains(&lower_name)
    || Path::new(lower_name)
      .extension()
      .and_then(OsStr::to_str)
      .is_some_and(|ext| PROJECT_EXTENSIONS.contains(&ext))
}

/// Resolve the workspace root and the problems met while doing so.
///
/// The nearest VCS marker wins. Without one, the nearest language workspace
/// marker is the candidate, and discovery climbs to every enclosing marker
/// whose workspace members include it. With no workspace marker at all, the
/// nearest directory holding a project file is the root.
pub(super) fn find_workspace_root(start: &Path) -> Option<(PathBuf, Vec<DiscoveryWarning>)> {
  if let Some(dir) = start.ancestors().find(|dir| Vcs::detect(dir).is_some()) {
    return Some((dir.to_path_buf(), Vec::new()));
  }

  let mut warnings = Vec::new();
  let mut markers = start.ancestors().filter(|dir| has_ecosystem_marker(dir));
  let Some(mut root) = markers.next() else {
    return start
      .ancestors()
      .find(|dir| has_project_file(dir))
      .map(|dir| (dir.to_path_buf(), warnings));
  };
  for outer in markers {
    match lists_member(outer, root) {
      Ok(true) => root = outer,
      Ok(false) => {}
      Err(err) => warnings.push(DiscoveryWarning::for_project(outer, &err)),
    }
  }
  Some((root.to_path_buf(), warnings))
}

fn lists_member(outer: &Path, inner: &Path) -> Result<bool> {
  let relative = inner
    .strip_prefix(outer)
    .map_err(|_| Error::PathOutsideWorkspace)?;
  let relative = WorkspacePath::from_relative_path(relative)?;
  Ok(
    detect_ecosystem_configs(outer, &[])?
      .iter()
      .flat_map(|config| &config.packages)
      .any(|package| package.path == relative),
  )
}

fn has_project_file(dir: &Path) -> bool {
  read_dir(dir).is_ok_and(|entries| {
    entries.flatten().any(|entry| {
      entry
        .file_name()
        .to_str()
        .is_some_and(|name| is_project_file(&name.to_ascii_lowercase()))
    })
  })
}

fn has_ecosystem_marker(dir: &Path) -> bool {
  has_cargo_workspace(dir)
    || has_simple_marker(dir)
    || has_maven_modules(dir)
    || has_package_json_workspaces(dir)
    || has_deno_workspace(dir)
    || has_pyproject_workspace(dir)
    || has_pubspec_workspace(dir)
    || has_mix_workspace(dir)
    || has_bazel_workspace(dir)
    || has_dotnet_workspace(dir)
}

fn has_cargo_workspace(dir: &Path) -> bool {
  let cargo_toml = dir.join("Cargo.toml");
  if !cargo_toml.exists() {
    return false;
  }
  let Ok(content) = read_to_string(&cargo_toml) else {
    return false;
  };
  if let Ok(doc) = content.parse::<DocumentMut>()
    && doc.get("workspace").is_some()
  {
    return true;
  }
  content.contains("[workspace]")
}

fn has_simple_marker(dir: &Path) -> bool {
  SIMPLE_MARKERS.iter().any(|name| dir.join(name).exists())
}

fn has_maven_modules(dir: &Path) -> bool {
  let pom = dir.join("pom.xml");
  pom.exists() && read_to_string(&pom).is_ok_and(|content| content.contains("<modules>"))
}

fn has_package_json_workspaces(dir: &Path) -> bool {
  let package_json = dir.join("package.json");
  if !package_json.exists() {
    return false;
  }
  let Ok(content) = read_to_string(&package_json) else {
    return false;
  };
  let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
    return false;
  };
  json.get("workspaces").is_some()
}

fn has_deno_workspace(dir: &Path) -> bool {
  for name in ["deno.json", "deno.jsonc"] {
    let path = dir.join(name);
    if !path.exists() {
      continue;
    }
    let Ok(content) = read_to_string(&path) else {
      continue;
    };
    let normalized = if name.ends_with("jsonc") {
      strip_json_comments(&content)
    } else {
      content
    };
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&normalized)
      && json.get("workspace").is_some()
    {
      return true;
    }
  }
  false
}

fn has_pyproject_workspace(dir: &Path) -> bool {
  let pyproject = dir.join("pyproject.toml");
  if !pyproject.exists() {
    return false;
  }
  let Ok(content) = read_to_string(&pyproject) else {
    return false;
  };
  let Ok(doc) = content.parse::<DocumentMut>() else {
    return false;
  };
  doc
    .get("tool")
    .and_then(|item| item.get("uv"))
    .and_then(|item| item.get("workspace"))
    .is_some()
}

fn has_pubspec_workspace(dir: &Path) -> bool {
  let pubspec = dir.join("pubspec.yaml");
  if !pubspec.exists() {
    return false;
  }
  let Ok(content) = read_to_string(&pubspec) else {
    return false;
  };
  let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
    return false;
  };
  doc.get("workspace").is_some()
}

fn has_mix_workspace(dir: &Path) -> bool {
  let mix_exs = dir.join("mix.exs");
  if !mix_exs.exists() {
    return false;
  }
  let Ok(content) = read_to_string(&mix_exs) else {
    return false;
  };
  parse_elixir_apps_path(&content).is_some()
}

fn has_bazel_workspace(dir: &Path) -> bool {
  BAZEL_MARKERS.iter().any(|name| dir.join(name).exists())
}

fn has_dotnet_workspace(dir: &Path) -> bool {
  let Ok(entries) = read_dir(dir) else {
    return false;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if matches!(
      path.extension().and_then(|ext| ext.to_str()),
      Some("sln" | "slnx" | "csproj" | "fsproj" | "vbproj")
    ) {
      return true;
    }
  }
  false
}
