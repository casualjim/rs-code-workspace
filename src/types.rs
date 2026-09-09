use std::fmt;
use std::str::FromStr;

use crate::Error;
use crate::paths::WorkspacePath;

/// Canonical ecosystem detected for a workspace or package.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Ecosystem {
  /// No ecosystem signal was detected.
  #[default]
  Unknown,
  /// Rust ecosystem.
  Rust,
  /// Go ecosystem.
  Go,
  /// Node.js ecosystem.
  Node,
  /// Python ecosystem.
  Python,
  /// Ruby ecosystem.
  Ruby,
  /// PHP ecosystem.
  PHP,
  /// Java ecosystem.
  Java,
  /// Gradle ecosystem.
  Gradle,
  /// Maven ecosystem.
  Maven,
  /// Elixir ecosystem.
  Elixir,
  /// Deno ecosystem.
  Deno,
  /// Haskell ecosystem.
  Haskell,
  /// Scala ecosystem.
  Scala,
  /// Dart ecosystem.
  Dart,
  /// Bazel ecosystem.
  Bazel,
  /// .NET ecosystem.
  DotNet,
  /// Swift ecosystem.
  Swift,
}

impl Ecosystem {
  /// Return the canonical lowercase ecosystem label.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Unknown => "unknown",
      Self::Rust => "rust",
      Self::Go => "go",
      Self::Node => "node",
      Self::Python => "python",
      Self::Ruby => "ruby",
      Self::PHP => "php",
      Self::Java => "java",
      Self::Gradle => "gradle",
      Self::Maven => "maven",
      Self::Elixir => "elixir",
      Self::Deno => "deno",
      Self::Haskell => "haskell",
      Self::Scala => "scala",
      Self::Dart => "dart",
      Self::Bazel => "bazel",
      Self::DotNet => "dotnet",
      Self::Swift => "swift",
    }
  }

  /// Return the user-facing stack label for this ecosystem when one exists.
  #[must_use]
  pub const fn stack_name(self) -> Option<&'static str> {
    match self {
      Self::Unknown => None,
      Self::Rust => Some("Rust"),
      Self::Go => Some("Go"),
      Self::Node => Some("Node.js"),
      Self::Python => Some("Python"),
      Self::Ruby => Some("Ruby"),
      Self::PHP => Some("PHP"),
      Self::Java => Some("Java"),
      Self::Gradle => Some("Gradle"),
      Self::Maven => Some("Maven"),
      Self::Elixir => Some("Elixir"),
      Self::Deno => Some("Deno"),
      Self::Haskell => Some("Haskell"),
      Self::Scala => Some("Scala"),
      Self::Dart => Some("Dart"),
      Self::Bazel => Some("Bazel"),
      Self::DotNet => Some("DotNet"),
      Self::Swift => Some("Swift"),
    }
  }
}

impl fmt::Display for Ecosystem {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl AsRef<str> for Ecosystem {
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

impl FromStr for Ecosystem {
  type Err = Error;

  /// Parse a canonical ecosystem label.
  ///
  /// # Arguments
  /// - `value`: Lowercase ecosystem label such as `rust` or `dotnet`.
  ///
  /// # Errors
  /// Returns an error when the label is not recognized.
  fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
      "unknown" => Ok(Self::Unknown),
      "rust" => Ok(Self::Rust),
      "go" => Ok(Self::Go),
      "node" | "node.js" | "nodejs" => Ok(Self::Node),
      "python" => Ok(Self::Python),
      "ruby" => Ok(Self::Ruby),
      "php" => Ok(Self::PHP),
      "java" => Ok(Self::Java),
      "gradle" => Ok(Self::Gradle),
      "maven" => Ok(Self::Maven),
      "elixir" => Ok(Self::Elixir),
      "deno" => Ok(Self::Deno),
      "haskell" => Ok(Self::Haskell),
      "scala" => Ok(Self::Scala),
      "dart" => Ok(Self::Dart),
      "bazel" => Ok(Self::Bazel),
      "dotnet" | ".net" | "csharp" | "c#" => Ok(Self::DotNet),
      "swift" => Ok(Self::Swift),
      other => Err(Error::UnsupportedEcosystem {
        value: other.to_string(),
      }),
    }
  }
}

impl TryFrom<&str> for Ecosystem {
  type Error = Error;

  fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
    value.parse()
  }
}

/// A workspace package with a normalized path and ecosystem classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePackage {
  /// Package name used for display and reporting.
  pub name: String,
  /// Package path relative to the workspace root.
  pub path: WorkspacePath,
  /// Canonical ecosystem for the package.
  pub ecosystem: Ecosystem,
}

impl WorkspacePackage {
  /// Create a workspace package record.
  ///
  /// # Arguments
  /// - `name`: Package display name.
  /// - `path`: Package path relative to the workspace root.
  /// - `ecosystem`: Canonical ecosystem classification.
  pub fn new(name: impl Into<String>, path: WorkspacePath, ecosystem: Ecosystem) -> Self {
    Self {
      name: name.into(),
      path,
      ecosystem,
    }
  }
}

/// A dependency declaration discovered from a workspace-owned manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDependency {
  /// Dependency package or module name as declared by the manifest.
  pub name: String,
  /// Manifest dependency group or section, such as `dependencies` or `require-dev`.
  pub group: String,
  /// Manifest path relative to the workspace root.
  pub manifest_path: WorkspacePath,
  /// Package that owns the manifest.
  pub package_name: String,
  /// Canonical ecosystem for the dependency declaration.
  pub ecosystem: Ecosystem,
}

impl WorkspaceDependency {
  /// Create a workspace dependency declaration record.
  ///
  /// # Arguments
  /// - `name`: Dependency package or module name.
  /// - `group`: Dependency group or section.
  /// - `manifest_path`: Manifest path relative to the workspace root.
  /// - `package_name`: Package that owns the manifest.
  /// - `ecosystem`: Canonical ecosystem classification.
  #[must_use]
  pub fn new(
    name: impl Into<String>,
    group: impl Into<String>,
    manifest_path: WorkspacePath,
    package_name: impl Into<String>,
    ecosystem: Ecosystem,
  ) -> Self {
    Self {
      name: name.into(),
      group: group.into(),
      manifest_path,
      package_name: package_name.into(),
      ecosystem,
    }
  }
}

/// Workspace-level configuration synthesized by the detector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceConfig {
  /// Packages discovered for the workspace.
  pub packages: Vec<WorkspacePackage>,
  /// Dependency declarations discovered from workspace-owned manifests.
  pub dependencies: Vec<WorkspaceDependency>,
  /// Canonical ecosystem detected for the workspace.
  pub ecosystem: Ecosystem,
}

impl WorkspaceConfig {
  /// Create a workspace configuration.
  ///
  /// # Arguments
  /// - `packages`: Packages discovered for the workspace.
  /// - `ecosystem`: Canonical workspace ecosystem classification.
  #[must_use]
  pub fn new(packages: Vec<WorkspacePackage>, ecosystem: Ecosystem) -> Self {
    Self {
      packages,
      dependencies: Vec::new(),
      ecosystem,
    }
  }

  /// Create a workspace configuration with dependency metadata.
  ///
  /// # Arguments
  /// - `packages`: Packages discovered for the workspace.
  /// - `dependencies`: Dependency declarations discovered from workspace manifests.
  /// - `ecosystem`: Canonical workspace ecosystem classification.
  #[must_use]
  pub fn with_dependencies(
    packages: Vec<WorkspacePackage>,
    dependencies: Vec<WorkspaceDependency>,
    ecosystem: Ecosystem,
  ) -> Self {
    Self {
      packages,
      dependencies,
      ecosystem,
    }
  }
}

/// Coarse workspace topology classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceKind {
  /// One package in one ecosystem.
  SingleLanguageRepo,
  /// Multiple packages in one ecosystem.
  Monorepo,
  /// One package spanning multiple ecosystems.
  PolyglotRepo,
  /// Multiple packages spanning multiple ecosystems.
  PolyglotMonorepo,
}

impl WorkspaceKind {
  /// Return the stable label for this workspace kind.
  #[must_use]
  pub fn label(&self) -> &'static str {
    match self {
      WorkspaceKind::SingleLanguageRepo => "single-language",
      WorkspaceKind::Monorepo => "monorepo",
      WorkspaceKind::PolyglotRepo => "polyglot",
      WorkspaceKind::PolyglotMonorepo => "polyglot-monorepo",
    }
  }
}

/// A parsed workspace target with optional anchor metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceTarget {
  /// Normalized workspace-relative path for the target.
  pub path: WorkspacePath,
  /// Optional one-based line anchor extracted from `#L<n>`.
  pub line_anchor: Option<usize>,
  /// Optional unresolved symbol hint extracted from `:symbol`.
  pub symbol_hint: Option<String>,
}

/// A typed warning produced while parsing a workspace target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceTargetWarning {
  /// The caller did not provide a usable path.
  EmptyPath,
  /// The provided path escapes the workspace root.
  OutsideWorkspace,
  /// The caller provided a malformed `#L<n>` anchor.
  InvalidLineAnchor,
  /// The caller provided an unsupported `#...` anchor.
  UnsupportedAnchor,
}

impl fmt::Display for WorkspaceTargetWarning {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::EmptyPath => f.write_str("empty path"),
      Self::OutsideWorkspace => f.write_str("path outside workspace"),
      Self::InvalidLineAnchor => f.write_str("invalid line anchor"),
      Self::UnsupportedAnchor => f.write_str("unsupported anchor"),
    }
  }
}
