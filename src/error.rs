//! Error types for workspace detection and path normalization.

use std::path::PathBuf;

use thiserror::Error;

/// Error type for workspace detection and normalization.
#[derive(Debug, Error)]
pub enum Error {
  /// No workspace marker was found while walking upward from the given start path.
  #[error("no workspace marker found while walking up from {start}")]
  NoWorkspaceMarker {
    /// Path from which discovery started.
    start: String,
  },

  /// The caller passed an absolute path where a relative path was required.
  #[error("absolute path not allowed for WorkspacePath (parse relative first)")]
  AbsolutePathNotAllowed,

  /// The path escaped the workspace root.
  #[error("path outside workspace")]
  PathOutsideWorkspace,

  /// A raw ecosystem label was not recognized.
  #[error("unsupported ecosystem: {value}")]
  UnsupportedEcosystem {
    /// Raw ecosystem label that could not be parsed.
    value: String,
  },

  /// A filesystem or parsing operation failed.
  #[error(transparent)]
  Io(#[from] std::io::Error),

  /// A JSON parsing operation failed.
  #[error(transparent)]
  Json(#[from] serde_json::Error),

  /// A YAML parsing operation failed.
  #[error(transparent)]
  Yaml(#[from] serde_yaml::Error),

  /// A Docker Compose file could not be read during stack detection.
  #[error("failed to read Docker Compose file {path}")]
  ComposeRead {
    /// Compose file path that could not be read.
    path: PathBuf,
    /// Underlying filesystem error.
    #[source]
    source: std::io::Error,
  },

  /// A Docker Compose file could not be parsed during stack detection.
  #[error("failed to parse Docker Compose file {path}")]
  ComposeParse {
    /// Compose file path that could not be parsed.
    path: PathBuf,
    /// Underlying YAML parser error.
    #[source]
    source: serde_yaml::Error,
  },

  /// A Docker Compose file did not contain a services mapping.
  #[error("Docker Compose file {path} does not contain a services mapping")]
  ComposeMissingServices {
    /// Compose file path missing the services mapping.
    path: PathBuf,
  },

  /// A TOML parsing operation failed.
  #[error(transparent)]
  Toml(#[from] toml_edit::TomlError),

  /// A relative path conversion failed.
  #[error(transparent)]
  RelativePath(#[from] relative_path::FromPathError),

  /// A .NET project parse operation failed.
  #[error(transparent)]
  Dotnet(#[from] dotnet_lens::parser::ParseError),

  /// A path could not be resolved from the filesystem.
  #[error("failed to resolve path: {path}")]
  PathResolution {
    /// Path that could not be resolved.
    path: PathBuf,
  },

  /// A `.git` file or Jujutsu repository pointer did not have the expected format.
  #[error("invalid VCS pointer file: {path}")]
  InvalidVcsPointer {
    /// Pointer file that could not be parsed.
    path: PathBuf,
  },
}

impl Error {
  /// Return a stable discovery hint for caller-facing UX when one exists.
  #[must_use]
  pub fn discovery_hint(&self) -> Option<&'static str> {
    match self {
      Self::NoWorkspaceMarker { .. } => {
        Some("run the command from inside a supported workspace or pass an explicit project root")
      }
      _ => None,
    }
  }
}
