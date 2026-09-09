//! Workspace-root discovery markers used by [`crate::Workspace::discover`].
//!
//! Discovery walks upward until it finds one of the supported markers below:
//! - `.git`
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

use std::fs::{read_dir, read_to_string};
use std::path::{Path, PathBuf};

use toml_edit::DocumentMut;

use crate::parsers::{parse_elixir_apps_path, strip_json_comments};

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

pub(super) fn find_workspace_root(start: &Path) -> Option<PathBuf> {
  let mut current = start.to_path_buf();
  loop {
    if has_workspace_marker(&current) {
      return Some(current);
    }
    if !current.pop() {
      return None;
    }
  }
}

pub(super) fn has_workspace_marker(dir: &Path) -> bool {
  has_git_marker(dir)
    || has_cargo_workspace(dir)
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

fn has_git_marker(dir: &Path) -> bool {
  dir.join(".git").exists()
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
