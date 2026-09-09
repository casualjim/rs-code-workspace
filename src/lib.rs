//! Workspace discovery, normalization, and caller-facing target parsing for multi-ecosystem repositories.
//!
//! Use [`Workspace::discover`] to walk upward from a starting path until a
//! supported workspace marker is found, or [`Workspace::from_root`] when the
//! caller already knows the project root. The supported discovery markers are
//! documented in the root-detection module comments.
//!
//! [`WorkspacePath`] is the single normalized workspace-relative path type.
//! [`Workspace::normalize_path`], [`Workspace::package_for_file`], and
//! [`Workspace::package_for_path_str`] keep path ownership checks centralized in
//! this crate instead of scattering raw-string heuristics through callers.
//!
//! [`Workspace::parse_target`] parses caller-facing target strings such as
//! `path#L12` and `path:symbol` into a [`WorkspaceTarget`] plus typed
//! [`WorkspaceTargetWarning`] values.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Workspace-relative path helpers.
pub mod paths;
/// Stack detection and ecosystem inference helpers.
pub mod stack;

mod detector;
mod error;
mod packages;
mod parsers;
mod root;
mod types;

pub use error::Error;

/// Public result alias for workspace detection and normalization.
pub type Result<T> = std::result::Result<T, Error>;

pub use paths::WorkspacePath;
pub use stack::{
  DetectedStack, detect_stack_from_compose, detect_stack_from_path, infer_ecosystem_from_stack,
  stack_name_for_ecosystem,
};
pub use types::{
  Ecosystem, WorkspaceConfig, WorkspaceDependency, WorkspaceKind, WorkspacePackage,
  WorkspaceTarget, WorkspaceTargetWarning,
};

const DEPENDENCY_MANIFEST_NAMES: &[&str] = &[
  "cargo.toml",
  "package.json",
  "go.mod",
  "requirements.txt",
  "gemfile",
  "build.gradle",
  "build.gradle.kts",
  "pom.xml",
  "composer.json",
];

/// Return true when a workspace-relative path names a supported dependency manifest.
#[must_use]
pub fn is_dependency_manifest_path(path: &str) -> bool {
  let lower = path.to_ascii_lowercase();
  let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());
  DEPENDENCY_MANIFEST_NAMES.contains(&file_name)
}

/// Resolve the workspace root from an explicit root or by walking up from a start path.
///
/// # Arguments
/// - `explicit_root`: Optional caller-provided workspace root.
/// - `start`: Path to start walking from when no explicit root is provided.
///
/// # Errors
/// Returns an error when no workspace marker can be found while walking upward.
pub fn resolve_root(explicit_root: Option<&Path>, start: &Path) -> Result<PathBuf> {
  if let Some(root) = explicit_root {
    return Ok(root.to_path_buf());
  }

  let start_dir = if start.is_dir() {
    start
  } else {
    start.parent().unwrap_or(start)
  };

  let Some(root) = root::find_workspace_root(start_dir) else {
    return Err(Error::NoWorkspaceMarker {
      start: start_dir.display().to_string(),
    });
  };

  Ok(root)
}

fn detect(
  workspace_root: &Path,
  files: &[&Path],
) -> Result<(
  Vec<WorkspacePackage>,
  BTreeSet<Ecosystem>,
  Vec<WorkspaceDependency>,
)> {
  let mut ecosystems = BTreeSet::new();
  let mut packages = Vec::new();
  let mut dependencies = Vec::new();
  let mut seen: std::collections::HashSet<(Ecosystem, WorkspacePath)> =
    std::collections::HashSet::new();

  for config in detector::detect_ecosystem_configs(workspace_root, files)? {
    ecosystems.insert(config.ecosystem);
    dependencies.extend(config.dependencies);
    for package in config.packages {
      if !seen.insert((package.ecosystem, package.path.clone())) {
        continue;
      }
      packages.push(package);
    }
  }

  let (detected_packages, detected_ecosystems) =
    detector::detect_workspace_packages_from_files(workspace_root, files);
  ecosystems.extend(detected_ecosystems);
  for package in detected_packages {
    if !seen.insert((package.ecosystem, package.path.clone())) {
      continue;
    }
    packages.push(package);
  }

  let mut stack = DetectedStack::new();
  for file in files {
    stack::detect_stack_from_path(file, &mut stack);
  }
  let ecosystem = stack::infer_ecosystem_from_stack(&stack);
  if packages.is_empty() {
    ecosystems.insert(ecosystem);
    packages.push(packages::default_package(workspace_root, ecosystem));
  }

  Ok((packages, ecosystems, dependencies))
}

/// Resolved workspace information for a repository.
#[derive(Clone, Debug)]
pub struct Workspace {
  root: PathBuf,
  has_git: bool,
  packages: Vec<WorkspacePackage>,
  dependencies: Vec<WorkspaceDependency>,
  ecosystems: BTreeSet<Ecosystem>,
  kind: WorkspaceKind,
}

impl Workspace {
  /// Create a workspace model from an explicit root or by discovering one.
  ///
  /// # Arguments
  /// - `explicit_root`: Optional caller-provided workspace root.
  /// - `start`: Path to start walking from when no explicit root is provided.
  ///
  /// # Errors
  /// Returns an error when workspace root discovery fails.
  pub fn new(explicit_root: Option<&Path>, start: &Path) -> Result<Self> {
    let root = resolve_root(explicit_root, start)?;
    let has_git = root.join(".git").exists();

    let (packages, ecosystems, dependencies) = detect(&root, &[])?;
    let package_count = packages.len();
    let ecosystem_count = ecosystems.len();
    let is_mono = package_count > 1;
    let is_poly = ecosystem_count > 1;
    let kind = match (is_mono, is_poly) {
      (false, false) => WorkspaceKind::SingleLanguageRepo,
      (true, false) => WorkspaceKind::Monorepo,
      (false, true) => WorkspaceKind::PolyglotRepo,
      (true, true) => WorkspaceKind::PolyglotMonorepo,
    };

    Ok(Self {
      root,
      has_git,
      packages,
      dependencies,
      ecosystems,
      kind,
    })
  }

  /// Discover a workspace starting from `start`.
  ///
  /// # Errors
  /// Returns an error when workspace root discovery fails.
  pub fn discover(start: impl AsRef<Path>) -> Result<Self> {
    Self::new(None, start.as_ref())
  }

  /// Build a workspace from an explicit root path.
  ///
  /// # Errors
  /// Returns an error when workspace construction fails.
  pub fn from_root(root: impl AsRef<Path>) -> Result<Self> {
    Self::new(Some(root.as_ref()), root.as_ref())
  }

  /// Return the resolved workspace root.
  #[must_use]
  pub fn root(&self) -> &Path {
    &self.root
  }

  /// Return whether the workspace contains a Git checkout.
  #[must_use]
  pub fn has_git(&self) -> bool {
    self.has_git
  }

  /// Return the discovered workspace packages.
  #[must_use]
  pub fn packages(&self) -> &[WorkspacePackage] {
    &self.packages
  }

  /// Return the dependency declarations discovered from workspace-owned manifests.
  #[must_use]
  pub fn dependencies(&self) -> &[WorkspaceDependency] {
    &self.dependencies
  }

  /// Return a stable ecosystem label for the workspace.
  #[must_use]
  pub fn ecosystem_label(&self) -> String {
    packages::ecosystem_label(&self.ecosystems)
  }

  /// Return the workspace topology classification.
  #[must_use]
  pub fn kind(&self) -> WorkspaceKind {
    self.kind
  }

  /// Normalize a raw path into a workspace-relative path.
  ///
  /// # Arguments
  /// - `raw`: Raw path string to normalize.
  ///
  /// # Errors
  /// Returns an error when the path is absolute or lies outside the workspace.
  pub fn normalize_path(&self, raw: &str) -> Result<WorkspacePath> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "." {
      return Ok(WorkspacePath::empty());
    }

    let input = Path::new(trimmed);
    if input.is_absolute() {
      let Ok(relative) = input.strip_prefix(&self.root) else {
        return Err(Error::PathOutsideWorkspace);
      };
      return WorkspacePath::from_relative_path(relative);
    }

    WorkspacePath::parse(trimmed)
  }

  /// Render a raw path string relative to the workspace.
  ///
  /// # Arguments
  /// - `raw`: Raw path string to render.
  #[must_use]
  pub fn display_path(&self, raw: &str) -> String {
    self
      .normalize_path(raw)
      .map_or_else(|_| raw.to_string(), |path| path.to_string())
  }

  /// Parse a raw target into a normalized path plus optional anchor hints.
  ///
  /// The parser accepts `path#L12` line anchors and `path:symbol` hints.
  /// Invalid or unsupported `#...` anchors become typed warnings. Empty and
  /// outside-workspace paths are rejected with typed warnings and no target.
  #[must_use]
  pub fn parse_target(&self, raw: &str) -> (Option<WorkspaceTarget>, Vec<WorkspaceTargetWarning>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
      return (None, vec![WorkspaceTargetWarning::EmptyPath]);
    }

    let (mut path_part, anchor_part) = trimmed.split_once('#').unwrap_or((trimmed, ""));
    let mut warnings = Vec::new();
    let mut line_anchor = None;
    if !anchor_part.trim().is_empty() {
      let anchor_trimmed = anchor_part.trim();
      if let Some(line) = anchor_trimmed
        .strip_prefix('L')
        .or_else(|| anchor_trimmed.strip_prefix('l'))
      {
        match line.parse::<usize>() {
          Ok(parsed) if parsed > 0 => line_anchor = Some(parsed),
          _ => warnings.push(WorkspaceTargetWarning::InvalidLineAnchor),
        }
      } else {
        warnings.push(WorkspaceTargetWarning::UnsupportedAnchor);
      }
    }

    let mut symbol_hint = None;
    if let Some((path, raw_symbol_hint)) = path_part.trim().rsplit_once(':') {
      let raw_symbol_hint = raw_symbol_hint.trim();
      if !raw_symbol_hint.is_empty()
        && !raw_symbol_hint.contains('/')
        && !raw_symbol_hint.contains('\\')
      {
        path_part = path;
        symbol_hint = Some(raw_symbol_hint.to_string());
      }
    }

    match self.normalize_path(path_part) {
      Ok(path) if !path.is_empty() => (
        Some(WorkspaceTarget {
          path,
          line_anchor,
          symbol_hint,
        }),
        warnings,
      ),
      Ok(_) => {
        warnings.push(WorkspaceTargetWarning::EmptyPath);
        (None, warnings)
      }
      Err(_) => {
        warnings.push(WorkspaceTargetWarning::OutsideWorkspace);
        (None, warnings)
      }
    }
  }

  /// Return the package name that owns a file, if one is known.
  ///
  /// # Arguments
  /// - `file`: Workspace-relative file path.
  #[must_use]
  pub fn package_for_file(&self, file: &WorkspacePath) -> Option<&str> {
    let mut best: Option<&types::WorkspacePackage> = None;
    for package in &self.packages {
      if package.path.is_empty() {
        best = best.or(Some(package));
        continue;
      }
      if !file.starts_with(&package.path) {
        continue;
      }
      let pkg_str = package.path.as_str();
      match best {
        Some(current) if current.path.as_str().len() >= pkg_str.len() => {}
        _ => best = Some(package),
      }
    }
    best.map(|pkg| pkg.name.as_str())
  }

  /// Return the package name that owns a raw file path, if one is known.
  #[must_use]
  pub fn package_for_path_str(&self, file: &str) -> Option<&str> {
    let file = self.normalize_path(file).ok()?;
    if file.is_empty() {
      return None;
    }
    self.package_for_file(&file)
  }

  /// Return the canonical stack names for the detected ecosystems.
  pub fn ecosystem_stack_names(&self) -> impl Iterator<Item = &'static str> + '_ {
    self
      .ecosystems
      .iter()
      .filter_map(|ecosystem| stack_name_for_ecosystem(*ecosystem))
  }

  /// Return whether the provided path lies under the workspace root.
  ///
  /// # Arguments
  /// - `path`: Path to check.
  #[must_use]
  pub fn is_workspace_file(&self, path: &Path) -> bool {
    path.starts_with(&self.root)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use std::env;
  use std::fs::{create_dir_all, remove_dir_all, write};
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
        "code-workspace-{prefix}-{}-{unique}",
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

  fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
      create_dir_all(parent).expect("failed to create parent dir");
    }
    write(path, contents).expect("failed to write fixture file");
  }

  fn workspace_with_packages(root: &Path, packages: Vec<WorkspacePackage>) -> Workspace {
    Workspace {
      root: root.to_path_buf(),
      has_git: root.join(".git").exists(),
      packages,
      dependencies: Vec::new(),
      ecosystems: BTreeSet::new(),
      kind: WorkspaceKind::Monorepo,
    }
  }

  #[test]
  fn discover_resolves_cargo_workspace_root_from_nested_path() {
    let temp = TempWorkspaceDir::new("cargo-discover");
    let root = temp.path().join("repo");
    write_file(
      &root.join("Cargo.toml"),
      "[workspace]\nmembers = [\"crates/app\"]\n",
    );
    create_dir_all(root.join("crates/app/src")).expect("failed to create nested cargo dir");

    let workspace = Workspace::discover(root.join("crates/app/src/lib.rs"))
      .expect("expected cargo workspace discovery to succeed");

    assert_eq!(workspace.root(), root.as_path());
  }

  #[test]
  fn discover_resolves_non_git_workspace_root_from_supported_marker() {
    let temp = TempWorkspaceDir::new("go-discover");
    let root = temp.path().join("repo");
    write_file(&root.join("go.work"), "go 1.22\nuse ./services/api\n");
    create_dir_all(root.join("services/api")).expect("failed to create go workspace dir");

    let workspace = Workspace::discover(root.join("services/api/main.go"))
      .expect("expected go workspace discovery to succeed");

    assert_eq!(workspace.root(), root.as_path());
    assert!(!workspace.has_git());
  }

  #[test]
  fn from_root_preserves_explicit_root() {
    let temp = TempWorkspaceDir::new("from-root");
    let root = temp.path().join("explicit-root");
    create_dir_all(&root).expect("failed to create explicit root");

    let workspace = Workspace::from_root(&root).expect("expected explicit root to succeed");

    assert_eq!(workspace.root(), root.as_path());
  }

  #[test]
  fn package_for_path_str_matches_package_for_file() {
    let temp = TempWorkspaceDir::new("package-owner");
    let root = temp.path().join("repo");
    create_dir_all(&root).expect("failed to create repo dir");
    let workspace = workspace_with_packages(
      &root,
      vec![
        WorkspacePackage::new("workspace", WorkspacePath::empty(), Ecosystem::Rust),
        WorkspacePackage::new(
          "app",
          WorkspacePath::parse("packages/app").expect("valid package path"),
          Ecosystem::Rust,
        ),
        WorkspacePackage::new(
          "nested",
          WorkspacePath::parse("packages/app/nested").expect("valid nested path"),
          Ecosystem::Rust,
        ),
      ],
    );
    let file =
      WorkspacePath::parse("packages/app/nested/src/lib.rs").expect("valid workspace file path");

    assert_eq!(workspace.package_for_file(&file), Some("nested"));
    assert_eq!(
      workspace.package_for_path_str("./packages/app/nested/src/lib.rs"),
      Some("nested")
    );
  }

  #[test]
  fn parse_target_parses_line_anchor() {
    let temp = TempWorkspaceDir::new("parse-line");
    let root = temp.path().join("repo");
    create_dir_all(&root).expect("failed to create repo dir");
    let workspace = workspace_with_packages(&root, Vec::new());

    let (target, warnings) = workspace.parse_target("src/lib.rs#L12");

    assert_eq!(warnings, Vec::<WorkspaceTargetWarning>::new());
    assert_eq!(
      target,
      Some(WorkspaceTarget {
        path: WorkspacePath::parse("src/lib.rs").expect("valid path"),
        line_anchor: Some(12),
        symbol_hint: None,
      })
    );
  }

  #[test]
  fn parse_target_parses_symbol_hint() {
    let temp = TempWorkspaceDir::new("parse-symbol");
    let root = temp.path().join("repo");
    create_dir_all(&root).expect("failed to create repo dir");
    let workspace = workspace_with_packages(&root, Vec::new());

    let (target, warnings) = workspace.parse_target("src/lib.rs:important_symbol");

    assert_eq!(warnings, Vec::<WorkspaceTargetWarning>::new());
    assert_eq!(
      target,
      Some(WorkspaceTarget {
        path: WorkspacePath::parse("src/lib.rs").expect("valid path"),
        line_anchor: None,
        symbol_hint: Some("important_symbol".to_string()),
      })
    );
  }

  #[test]
  fn parse_target_returns_typed_anchor_warnings() {
    let temp = TempWorkspaceDir::new("parse-warning");
    let root = temp.path().join("repo");
    create_dir_all(&root).expect("failed to create repo dir");
    let workspace = workspace_with_packages(&root, Vec::new());

    let (invalid_target, invalid_warnings) = workspace.parse_target("src/lib.rs#Lx");
    let (unsupported_target, unsupported_warnings) = workspace.parse_target("src/lib.rs#symbol");

    assert_eq!(
      invalid_target,
      Some(WorkspaceTarget {
        path: WorkspacePath::parse("src/lib.rs").expect("valid path"),
        line_anchor: None,
        symbol_hint: None,
      })
    );
    assert_eq!(
      invalid_warnings,
      vec![WorkspaceTargetWarning::InvalidLineAnchor]
    );
    assert_eq!(
      unsupported_target,
      Some(WorkspaceTarget {
        path: WorkspacePath::parse("src/lib.rs").expect("valid path"),
        line_anchor: None,
        symbol_hint: None,
      })
    );
    assert_eq!(
      unsupported_warnings,
      vec![WorkspaceTargetWarning::UnsupportedAnchor]
    );
  }

  #[test]
  fn parse_target_rejects_outside_workspace_targets() {
    let temp = TempWorkspaceDir::new("parse-outside");
    let root = temp.path().join("repo");
    create_dir_all(&root).expect("failed to create repo dir");
    let workspace = workspace_with_packages(&root, Vec::new());

    let (target, warnings) = workspace.parse_target("../outside.rs");

    assert_eq!(target, None);
    assert_eq!(warnings, vec![WorkspaceTargetWarning::OutsideWorkspace]);
  }

  #[test]
  fn workspace_dependencies_cover_rust_node_go_and_python_manifests() {
    let temp = TempWorkspaceDir::new("dependencies-main");

    let rust_root = temp.path().join("rust-repo");
    write_file(
      &rust_root.join("Cargo.toml"),
      "[package]\nname = \"rust-repo\"\n[dependencies]\nserde = \"1\"\n[dev-dependencies]\npretty_assertions = \"1\"\n[build-dependencies]\ncc = \"1\"\n",
    );
    let rust = Workspace::from_root(&rust_root).expect("expected rust workspace");
    assert_dependency(
      &rust,
      "serde",
      "dependencies",
      "Cargo.toml",
      Ecosystem::Rust,
    );
    assert_dependency(
      &rust,
      "pretty_assertions",
      "dev-dependencies",
      "Cargo.toml",
      Ecosystem::Rust,
    );
    assert_dependency(
      &rust,
      "cc",
      "build-dependencies",
      "Cargo.toml",
      Ecosystem::Rust,
    );

    let node_root = temp.path().join("node-repo");
    write_file(
      &node_root.join("package.json"),
      r#"{"dependencies":{"express":"^5"},"devDependencies":{"vitest":"^3"},"peerDependencies":{"react":"^19"}}"#,
    );
    let node = Workspace::from_root(&node_root).expect("expected node workspace");
    assert_dependency(
      &node,
      "express",
      "dependencies",
      "package.json",
      Ecosystem::Node,
    );
    assert_dependency(
      &node,
      "vitest",
      "devDependencies",
      "package.json",
      Ecosystem::Node,
    );
    assert_dependency(
      &node,
      "react",
      "peerDependencies",
      "package.json",
      Ecosystem::Node,
    );

    let go_root = temp.path().join("go-repo");
    write_file(
      &go_root.join("go.mod"),
      "module example.com/app\nrequire (\n  github.com/gin-gonic/gin v1.10.0\n)\n",
    );
    let go = Workspace::from_root(&go_root).expect("expected go workspace");
    assert_dependency(
      &go,
      "github.com/gin-gonic/gin",
      "require",
      "go.mod",
      Ecosystem::Go,
    );

    let python_root = temp.path().join("python-repo");
    write_file(
      &python_root.join("requirements.txt"),
      "requests==2.32.0\nuvicorn[standard]>=0.30\n",
    );
    let python = Workspace::from_root(&python_root).expect("expected python workspace");
    assert_dependency(
      &python,
      "requests",
      "requirements",
      "requirements.txt",
      Ecosystem::Python,
    );
    assert_dependency(
      &python,
      "uvicorn",
      "requirements",
      "requirements.txt",
      Ecosystem::Python,
    );
  }

  #[test]
  fn workspace_dependencies_cover_ruby_gradle_maven_and_composer_manifests() {
    let temp = TempWorkspaceDir::new("dependencies-extra");

    let ruby_root = temp.path().join("ruby-repo");
    write_file(
      &ruby_root.join("Gemfile"),
      "source 'https://rubygems.org'\ngem 'rails'\ngem \"puma\"\n",
    );
    let ruby = Workspace::from_root(&ruby_root).expect("expected ruby workspace");
    assert_dependency(&ruby, "rails", "gem", "Gemfile", Ecosystem::Ruby);
    assert_dependency(&ruby, "puma", "gem", "Gemfile", Ecosystem::Ruby);

    let gradle_root = temp.path().join("gradle-repo");
    write_file(
      &gradle_root.join("build.gradle"),
      "dependencies {\n  implementation 'org.springframework.boot:spring-boot-starter-web:3.3.0'\n  testImplementation \"org.junit.jupiter:junit-jupiter:5.10.0\"\n}\n",
    );
    let gradle = Workspace::from_root(&gradle_root).expect("expected gradle workspace");
    assert_dependency(
      &gradle,
      "spring-boot-starter-web",
      "implementation",
      "build.gradle",
      Ecosystem::Gradle,
    );
    assert_dependency(
      &gradle,
      "junit-jupiter",
      "testImplementation",
      "build.gradle",
      Ecosystem::Gradle,
    );

    let maven_root = temp.path().join("maven-repo");
    write_file(
      &maven_root.join("pom.xml"),
      "<project><dependencies><dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId></dependency></dependencies></project>",
    );
    let maven = Workspace::from_root(&maven_root).expect("expected maven workspace");
    assert_dependency(&maven, "slf4j-api", "compile", "pom.xml", Ecosystem::Maven);

    let php_root = temp.path().join("php-repo");
    write_file(
      &php_root.join("composer.json"),
      r#"{"require":{"monolog/monolog":"^3"},"require-dev":{"phpunit/phpunit":"^11"}}"#,
    );
    let php = Workspace::from_root(&php_root).expect("expected php workspace");
    assert_dependency(
      &php,
      "monolog/monolog",
      "require",
      "composer.json",
      Ecosystem::PHP,
    );
    assert_dependency(
      &php,
      "phpunit/phpunit",
      "require-dev",
      "composer.json",
      Ecosystem::PHP,
    );
  }

  fn assert_dependency(
    workspace: &Workspace,
    name: &str,
    group: &str,
    manifest_path: &str,
    ecosystem: Ecosystem,
  ) {
    assert!(
      workspace.dependencies().iter().any(|dependency| {
        dependency.name == name
          && dependency.group == group
          && dependency.manifest_path.as_str() == manifest_path
          && dependency.package_name
            == workspace
              .root()
              .file_name()
              .and_then(|value| value.to_str())
              .unwrap_or("workspace")
          && dependency.ecosystem == ecosystem
      }),
      "missing dependency {name} in group {group} from {manifest_path}"
    );
  }
}
