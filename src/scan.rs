//! Bounded repository scan that feeds nested project and file-driven detection.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use ignore::types::{Types, TypesBuilder};
use ignore::{Error as WalkError, WalkBuilder};

use crate::root::is_project_file;
use crate::types::DiscoveryWarning;

/// Directories never entered, on top of ignore-file rules and hidden entries.
const SKIPPED_DIRS: &[&str] = &[
  "node_modules",
  "target",
  "vendor",
  "dist",
  "build",
  "__pycache__",
  "venv",
];

/// Globs for the files discovery looks at: project and workspace markers, lock files,
/// container files, and the source extensions used for stack inference.
///
/// Keep in step with the tables in `stack.rs` and `root.rs`. The walker applies the
/// filter after `.gitignore`, so ignored files stay hidden. Matching is case-sensitive.
const SCAN_GLOBS: &[&str] = &[
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "package-lock.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "yarn.lock",
  "bun.lock",
  "bun.lockb",
  "nx.json",
  "turbo.json",
  "lerna.json",
  "go.mod",
  "go.sum",
  "go.work",
  "pyproject.toml",
  "requirements.txt",
  "requirements-dev.txt",
  "Pipfile",
  "poetry.lock",
  "setup.py",
  "setup.cfg",
  "Gemfile",
  "Gemfile.lock",
  "*.gemspec",
  "composer.json",
  "composer.lock",
  "pom.xml",
  "mvnw",
  "mvnw.cmd",
  "build.gradle",
  "build.gradle.kts",
  "settings.gradle",
  "settings.gradle.kts",
  "gradle.properties",
  "gradlew",
  "gradlew.bat",
  "build.sbt",
  "mix.exs",
  "mix.lock",
  "deno.json",
  "deno.jsonc",
  "cabal.project",
  "stack.yaml",
  "pubspec.yaml",
  "melos.yaml",
  "Package.swift",
  "WORKSPACE",
  "WORKSPACE.bazel",
  "MODULE.bazel",
  "BUILD",
  "BUILD.bazel",
  "Dockerfile",
  "Dockerfile.dev",
  "*.sln",
  "*.slnx",
  "*.csproj",
  "*.fsproj",
  "*.vbproj",
  "*.rs",
  "*.py",
  "*.js",
  "*.jsx",
  "*.ts",
  "*.tsx",
  "*.go",
  "*.rb",
  "*.php",
  "*.java",
  "*.kt",
  "*.kts",
  "*.cs",
  "*.fs",
  "*.vb",
  "*.swift",
  "*.scala",
  "*.c",
  "*.h",
  "*.cc",
  "*.cpp",
  "*.cxx",
  "*.hpp",
  "*.hh",
  "*.sql",
  "*.tf",
  "*.tfvars",
];

static SCAN_TYPES: LazyLock<Types> = LazyLock::new(|| {
  let mut builder = TypesBuilder::new();
  for glob in SCAN_GLOBS {
    builder
      .add("scan", glob)
      .expect("scan globs are constant and valid");
  }
  builder
    .select("scan")
    .build()
    .expect("scan globs are constant and valid")
});

/// Files and project directories found below a workspace root.
pub(crate) struct Scan {
  /// Files relative to the root, sorted.
  files: Vec<PathBuf>,
  /// Directories relative to the root, excluding the root itself, that hold a project or workspace file.
  project_dirs: Vec<PathBuf>,
  /// Problems that made the scan skip a path.
  pub(crate) warnings: Vec<DiscoveryWarning>,
}

impl Scan {
  /// Scan `root`, honoring `.gitignore` files whether or not a git repository is present.
  pub(crate) fn run(root: &Path) -> Self {
    let walker = WalkBuilder::new(root)
      .require_git(false)
      .git_global(false)
      .parents(false)
      .ignore(false)
      .types(SCAN_TYPES.clone())
      .filter_entry(|entry| {
        entry.depth() == 0
          || !entry.file_type().is_some_and(|kind| kind.is_dir())
          || !entry
            .file_name()
            .to_str()
            .is_some_and(|name| SKIPPED_DIRS.contains(&name))
      })
      .build();

    let mut files = Vec::new();
    let mut project_dirs = BTreeSet::new();
    let mut warnings = Vec::new();
    for result in walker {
      let entry = match result {
        Ok(entry) => entry,
        Err(err) => {
          warnings.push(warning_for(&err, root));
          continue;
        }
      };
      if !entry.file_type().is_some_and(|kind| kind.is_file()) {
        continue;
      }
      let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
      let is_project = entry
        .file_name()
        .to_str()
        .is_some_and(|name| is_project_file(&name.to_ascii_lowercase()));
      if is_project
        && let Some(dir) = relative.parent()
        && !dir.as_os_str().is_empty()
      {
        project_dirs.insert(dir.to_path_buf());
      }
      files.push(relative.to_path_buf());
    }
    files.sort();

    Self {
      files,
      project_dirs: project_dirs.into_iter().collect(),
      warnings,
    }
  }

  /// Directories below the root that hold a project or workspace file.
  pub(crate) fn project_dirs(&self) -> &[PathBuf] {
    &self.project_dirs
  }

  /// Files at or below `dir` (relative to the root), expressed relative to `dir`.
  pub(crate) fn files_below(&self, dir: &Path) -> Vec<&Path> {
    let start = self.files.partition_point(|file| file.as_path() < dir);
    self.files[start..]
      .iter()
      .take_while(|file| file.starts_with(dir))
      .map(|file| file.strip_prefix(dir).unwrap_or(file))
      .collect()
  }
}

fn warning_for(err: &WalkError, path: &Path) -> DiscoveryWarning {
  match err {
    WalkError::WithPath { path, err } => warning_for(err, path),
    WalkError::WithDepth { err, .. } | WalkError::WithLineNumber { err, .. } => {
      warning_for(err, path)
    }
    WalkError::Loop { child, .. } => DiscoveryWarning::FilesystemLoop {
      path: child.clone(),
    },
    WalkError::Io(io) => DiscoveryWarning::UnreadablePath {
      path: path.to_path_buf(),
      kind: io.kind(),
    },
    _ => DiscoveryWarning::InvalidIgnoreRule {
      path: path.to_path_buf(),
    },
  }
}
