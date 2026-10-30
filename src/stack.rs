//! Stack marker detection and ecosystem inference.
//!
//! This module maps repository file names, source extensions, and Docker Compose
//! service hints to stable stack labels. The labels feed workspace ecosystem
//! inference through a deterministic priority order in [`DetectedStack::infer_ecosystem`].
//!
//! Docker Compose inspection is strict: read, YAML parse, and missing `services`
//! failures are returned as structured workspace errors rather than silently
//! weakening stack detection.

use std::collections::BTreeSet;
use std::fs::read_to_string;
use std::path::Path;

use crate::Error;

use crate::types::Ecosystem;

/// Set of stack labels detected from workspace markers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DetectedStack(BTreeSet<String>);

impl DetectedStack {
  /// Create an empty detected stack.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Insert a detected stack label.
  ///
  /// # Arguments
  /// - `value`: Stack label to insert.
  pub fn insert(&mut self, value: impl Into<String>) {
    self.0.insert(value.into());
  }

  /// Return whether the stack contains the provided label.
  ///
  /// # Arguments
  /// - `value`: Stack label to check.
  #[must_use]
  pub fn contains(&self, value: &str) -> bool {
    self.0.contains(value)
  }

  /// Return whether the stack is empty.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// Iterate over the detected labels.
  pub fn iter(&self) -> impl Iterator<Item = &str> + '_ {
    self.0.iter().map(String::as_str)
  }

  /// Infer a canonical ecosystem from the detected stack labels.
  #[must_use]
  pub fn infer_ecosystem(&self) -> Ecosystem {
    if self.contains("Rust") {
      return Ecosystem::Rust;
    }
    if self.contains("Go") {
      return Ecosystem::Go;
    }
    if self.contains("Deno") {
      return Ecosystem::Deno;
    }
    if self.contains("Node.js") || self.contains("JavaScript") || self.contains("TypeScript") {
      return Ecosystem::Node;
    }
    if self.contains("Python") {
      return Ecosystem::Python;
    }
    if self.contains("Ruby") {
      return Ecosystem::Ruby;
    }
    if self.contains("PHP") {
      return Ecosystem::PHP;
    }
    if self.contains("Gradle") {
      return Ecosystem::Gradle;
    }
    if self.contains("Maven") {
      return Ecosystem::Maven;
    }
    if self.contains("Java") {
      return Ecosystem::Java;
    }
    if self.contains("Elixir") {
      return Ecosystem::Elixir;
    }
    if self.contains("Haskell") {
      return Ecosystem::Haskell;
    }
    if self.contains("Scala") {
      return Ecosystem::Scala;
    }
    if self.contains("Flutter") || self.contains("Dart") || self.contains("Melos") {
      return Ecosystem::Dart;
    }
    if self.contains("Bazel") {
      return Ecosystem::Bazel;
    }
    if self.contains("DotNet") {
      return Ecosystem::DotNet;
    }
    if self.contains("Swift") {
      return Ecosystem::Swift;
    }
    Ecosystem::Unknown
  }
}

/// Detect stack labels from a file path and merge them into the provided set.
///
/// # Arguments
/// - `path`: File path to inspect for stack markers.
/// - `stack`: Stack labels accumulated so far.
pub fn detect_stack_from_path(path: &Path, stack: &mut DetectedStack) {
  let file_name = path
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();
  let extension = path
    .extension()
    .and_then(|ext| ext.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();

  apply_file_name_stack(&file_name, stack);
  apply_extension_stack(&extension, stack);
}

fn apply_file_name_stack(file_name: &str, stack: &mut DetectedStack) {
  match file_name {
    "cargo.toml" | "cargo.lock" => insert_stack(stack, "Rust"),
    "package.json" | "pnpm-lock.yaml" | "yarn.lock" | "bun.lockb" | "bun.lock"
    | "package-lock.json" => insert_stack(stack, "Node.js"),
    "pyproject.toml"
    | "requirements.txt"
    | "requirements-dev.txt"
    | "pipfile"
    | "poetry.lock"
    | "setup.py"
    | "setup.cfg" => {
      insert_stack(stack, "Python");
    }
    "go.mod" | "go.sum" | "go.work" => insert_stack(stack, "Go"),
    "gemfile" | "gemfile.lock" => insert_stack(stack, "Ruby"),
    "composer.json" | "composer.lock" => insert_stack(stack, "PHP"),
    "pom.xml" | "mvnw" | "mvnw.cmd" => {
      insert_stack(stack, "Maven");
      insert_stack(stack, "Java");
    }
    "build.gradle"
    | "build.gradle.kts"
    | "settings.gradle"
    | "settings.gradle.kts"
    | "gradle.properties"
    | "gradlew"
    | "gradlew.bat" => {
      insert_stack(stack, "Gradle");
      insert_stack(stack, "Java");
    }
    "mix.exs" | "mix.lock" => insert_stack(stack, "Elixir"),
    "melos.yaml" => insert_stack(stack, "Melos"),
    "pubspec.yaml" => {
      insert_stack(stack, "Dart");
      insert_stack(stack, "Flutter");
    }
    "dockerfile" | "dockerfile.dev" => insert_stack(stack, "Docker"),
    "deno.json" | "deno.jsonc" => insert_stack(stack, "Deno"),
    "cabal.project" | "stack.yaml" => insert_stack(stack, "Haskell"),
    "build.sbt" => insert_stack(stack, "Scala"),
    "package.swift" => insert_stack(stack, "Swift"),
    "workspace" | "workspace.bazel" | "module.bazel" | "build" | "build.bazel" => {
      insert_stack(stack, "Bazel");
    }
    _ => {}
  }

  if is_dotnet_file(file_name) {
    insert_stack(stack, "DotNet");
  }
}

fn apply_extension_stack(extension: &str, stack: &mut DetectedStack) {
  match extension {
    "rs" => insert_stack(stack, "Rust"),
    "py" => insert_stack(stack, "Python"),
    "js" | "jsx" => insert_stack(stack, "JavaScript"),
    "ts" | "tsx" => insert_stack(stack, "TypeScript"),
    "go" => insert_stack(stack, "Go"),
    "rb" => insert_stack(stack, "Ruby"),
    "php" => insert_stack(stack, "PHP"),
    "java" => insert_stack(stack, "Java"),
    "kt" | "kts" => insert_stack(stack, "Kotlin"),
    "cs" | "fs" | "vb" => insert_stack(stack, "DotNet"),
    "swift" => insert_stack(stack, "Swift"),
    "scala" => insert_stack(stack, "Scala"),
    "c" | "h" => insert_stack(stack, "C"),
    "cc" | "cpp" | "cxx" | "hpp" | "hh" => insert_stack(stack, "C++"),
    "sql" => insert_stack(stack, "SQL"),
    "tf" | "tfvars" => insert_stack(stack, "Terraform"),
    _ => {}
  }
}

fn is_dotnet_file(file_name: &str) -> bool {
  has_case_insensitive_extension(file_name, "sln")
    || has_case_insensitive_extension(file_name, "slnx")
    || has_case_insensitive_extension(file_name, "csproj")
    || has_case_insensitive_extension(file_name, "fsproj")
    || has_case_insensitive_extension(file_name, "vbproj")
}

fn insert_stack(stack: &mut DetectedStack, value: &str) {
  stack.insert(value.to_string());
}

fn has_case_insensitive_extension(file_name: &str, expected: &str) -> bool {
  Path::new(file_name)
    .extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| ext.eq_ignore_ascii_case(expected))
}

/// Return the preferred stack name for a canonical ecosystem.
#[must_use]
pub fn stack_name_for_ecosystem(ecosystem: Ecosystem) -> Option<&'static str> {
  ecosystem.stack_name()
}

/// Infer the canonical ecosystem from detected stack labels.
#[must_use]
pub fn infer_ecosystem_from_stack(stack: &DetectedStack) -> Ecosystem {
  stack.infer_ecosystem()
}

/// Map a lowercase file name to the ecosystem it marks, if any.
pub(super) fn stack_marker_ecosystem(file_name: &str) -> Option<Ecosystem> {
  if is_dotnet_file(file_name) {
    return Some(Ecosystem::DotNet);
  }
  if has_case_insensitive_extension(file_name, "gemspec") {
    return Some(Ecosystem::Ruby);
  }
  match file_name {
    "cargo.toml" | "cargo.lock" => Some(Ecosystem::Rust),
    "package.json"
    | "pnpm-lock.yaml"
    | "yarn.lock"
    | "bun.lockb"
    | "bun.lock"
    | "package-lock.json"
    | "pnpm-workspace.yaml"
    | "nx.json"
    | "turbo.json"
    | "lerna.json" => Some(Ecosystem::Node),
    "go.mod" | "go.sum" | "go.work" => Some(Ecosystem::Go),
    "pyproject.toml"
    | "requirements.txt"
    | "requirements-dev.txt"
    | "pipfile"
    | "poetry.lock"
    | "setup.py"
    | "setup.cfg" => Some(Ecosystem::Python),
    "gemfile" | "gemfile.lock" => Some(Ecosystem::Ruby),
    "composer.json" | "composer.lock" => Some(Ecosystem::PHP),
    "pom.xml" | "mvnw" | "mvnw.cmd" => Some(Ecosystem::Maven),
    "build.gradle"
    | "build.gradle.kts"
    | "settings.gradle"
    | "settings.gradle.kts"
    | "gradle.properties"
    | "gradlew"
    | "gradlew.bat" => Some(Ecosystem::Gradle),
    "mix.exs" | "mix.lock" => Some(Ecosystem::Elixir),
    "deno.json" | "deno.jsonc" => Some(Ecosystem::Deno),
    "cabal.project" | "stack.yaml" => Some(Ecosystem::Haskell),
    "build.sbt" => Some(Ecosystem::Scala),
    "pubspec.yaml" | "melos.yaml" => Some(Ecosystem::Dart),
    "package.swift" => Some(Ecosystem::Swift),
    "workspace" | "workspace.bazel" | "module.bazel" | "build" | "build.bazel" => {
      Some(Ecosystem::Bazel)
    }
    _ => None,
  }
}

/// Detect stack labels from a Docker Compose file and merge them into the provided set.
///
/// # Arguments
/// - `workspace_root`: Root directory used to resolve relative compose files.
/// - `file_path`: Compose file path to inspect.
/// - `stack`: Stack labels accumulated so far.
///
/// # Errors
/// Returns an error when a Compose file cannot be read, parsed as YAML, or
/// interpreted as a Compose document with a `services` mapping.
pub fn detect_stack_from_compose(
  workspace_root: &Path,
  file_path: &str,
  stack: &mut DetectedStack,
) -> crate::Result<()> {
  let file_name = Path::new(file_path)
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();
  if !matches_compose_file(&file_name) {
    return Ok(());
  }

  stack.insert("Docker".to_string());
  stack.insert("Docker Compose".to_string());

  let path = Path::new(file_path);
  let absolute = if path.is_absolute() {
    path.to_path_buf()
  } else {
    workspace_root.join(file_path)
  };
  let content = read_to_string(&absolute).map_err(|source| Error::ComposeRead {
    path: absolute.clone(),
    source,
  })?;
  let doc =
    serde_yaml::from_str::<serde_yaml::Value>(&content).map_err(|source| Error::ComposeParse {
      path: absolute.clone(),
      source,
    })?;
  let Some(services) = doc.get("services").and_then(|value| value.as_mapping()) else {
    return Err(Error::ComposeMissingServices { path: absolute });
  };

  for (name, service) in services {
    if let Some(name) = name.as_str() {
      detect_dependency_from_token(name, stack);
    }
    if let Some(image) = service.get("image").and_then(|value| value.as_str()) {
      detect_dependency_from_token(image, stack);
    }
  }

  Ok(())
}

fn matches_compose_file(file_name: &str) -> bool {
  matches!(
    file_name,
    "docker-compose.yml"
      | "docker-compose.yaml"
      | "docker-compose.override.yml"
      | "docker-compose.override.yaml"
      | "compose.yml"
      | "compose.yaml"
  )
}

fn detect_dependency_from_token(token: &str, stack: &mut DetectedStack) {
  let lower = token.to_ascii_lowercase();
  if lower.contains("postgres") || lower.contains("timescale") {
    stack.insert("Postgres".to_string());
  }
  if lower.contains("mysql") || lower.contains("mariadb") {
    stack.insert("MySQL".to_string());
  }
  if lower.contains("redis") {
    stack.insert("Redis".to_string());
  }
  if lower.contains("mongo") {
    stack.insert("MongoDB".to_string());
  }
  if lower.contains("kafka") {
    stack.insert("Kafka".to_string());
  }
  if lower.contains("zookeeper") {
    stack.insert("Zookeeper".to_string());
  }
  if lower.contains("rabbitmq") {
    stack.insert("RabbitMQ".to_string());
  }
  if lower.contains("nats") {
    stack.insert("NATS".to_string());
  }
  if lower.contains("elasticsearch") {
    stack.insert("Elasticsearch".to_string());
  }
  if lower.contains("opensearch") {
    stack.insert("OpenSearch".to_string());
  }
  if lower.contains("prometheus") {
    stack.insert("Prometheus".to_string());
  }
  if lower.contains("grafana") {
    stack.insert("Grafana".to_string());
  }
  if lower.contains("jaeger") {
    stack.insert("Jaeger".to_string());
  }
  if lower.contains("minio") {
    stack.insert("MinIO".to_string());
  }
  if lower.contains("clickhouse") {
    stack.insert("ClickHouse".to_string());
  }
}
