# code-workspace

`code-workspace` is a Rust library for finding the workspace root of a repository, detecting the packages and dependency manifests inside it, and normalizing paths relative to that root. It works across build ecosystems — Rust, Node, Go, Python, Ruby, PHP, Java, Gradle, Maven, Elixir, Deno, Haskell, Scala, Dart, Bazel, .NET, and Swift — so tools like editors, linters, and AI agents can reason about "the workspace" without hard-coding conventions for a single ecosystem.

## Features

- **Root discovery** — walk upward from any path until a workspace marker is found: `.git`, `Cargo.toml` with a `[workspace]` table, `package.json` with `workspaces` (or `pnpm-workspace.yaml`), `go.work`, Gradle settings files, Maven `pom.xml` with `<modules>`, `build.sbt`, Deno workspace files, `pyproject.toml` with `tool.uv.workspace`, Bazel markers (`WORKSPACE`, `WORKSPACE.bazel`, `MODULE.bazel`), and .NET solutions or projects (`.sln`, `.slnx`, `.csproj`, `.fsproj`, `.vbproj`). Best-effort markers such as `melos.yaml`, `cabal.project`, `stack.yaml`, `turbo.json`, `lerna.json`, `nx.json`, `pubspec.yaml` with `workspace`, and `mix.exs` umbrella apps are also recognized.
- **Package + dependency detection** — enumerate `WorkspacePackage`s and `WorkspaceDependency` declarations discovered from workspace-owned manifests across ecosystems.
- **Path normalization** — `WorkspacePath` is the single normalized workspace-relative path type; absolute paths inside the root are re-based, paths outside the root are rejected with a typed error.
- **Target parsing** — parse caller-facing strings like `path#L12` (line anchor) and `path:symbol` (symbol hint) into a `WorkspaceTarget` with typed warnings for invalid or unsupported anchors.
- **Stack detection** — `DetectedStack` helpers (`detect_stack_from_path`, `detect_stack_from_compose`) identify container/compose stacks and infer the dominant `Ecosystem`.

## Quick start

```rust
use code_workspace::Workspace;

let ws = Workspace::discover("crates/some-package/src")?;

let root = ws.root();                    // absolute path to the workspace root
let packages = ws.packages();            // discovered workspace packages
let normalized = ws.normalize_path("crates/some-package/src/lib.rs")?;  // "crates/some-package/src/lib.rs"
let owner = ws.package_for_path_str("crates/some-package/src/lib.rs"); // Some("some-package")
let (target, warnings) = ws.parse_target("crates/some-package/src/lib.rs#L3");
assert_eq!(target.unwrap().line_anchor, Some(3));
assert!(warnings.is_empty());
```

## API overview

| Type / function | Purpose |
|---|---|
| `Workspace` | Resolved workspace model; entry points `discover` / `from_root` / `new`. Accessors: `root`, `has_git`, `packages`, `dependencies`, `ecosystem_label`, `kind`, `normalize_path`, `display_path`, `parse_target`, `package_for_file`, `package_for_path_str`, `ecosystem_stack_names`, `is_workspace_file`. |
| `WorkspacePath` | Normalized workspace-relative path (`as_str`, `parse`, `starts_with`, `from_relative_path`, …). |
| `Ecosystem` | Canonical ecosystem enum: Rust, Go, Node, Python, Ruby, PHP, Java, Gradle, Maven, Elixir, Deno, Haskell, Scala, Dart, Bazel, DotNet, Swift, Unknown. |
| `WorkspaceKind` | Topology classification: `SingleLanguageRepo`, `Monorepo`, `PolyglotRepo`, `PolyglotMonorepo` with `label()`. |
| `WorkspacePackage` | Discovered package: name, normalized path, ecosystem. |
| `WorkspaceDependency` | Dependency declaration discovered from a workspace-owned manifest. |
| `WorkspaceConfig` | Per-ecosystem config aggregation (`new`, `with_dependencies`). |
| `WorkspaceTarget` / `WorkspaceTargetWarning` | Parsed target with `path`, `line_anchor`, `symbol_hint`; typed warnings: `EmptyPath`, `OutsideWorkspace`, `InvalidLineAnchor`, `UnsupportedAnchor`. |
| `stack::DetectedStack` | Ordered set of detected stack labels with `infer_ecosystem()`. |
| `stack::detect_stack_from_path` | Record stack names implied by a path's files. |
| `stack::detect_stack_from_compose` | Record stack names implied by a compose file relative to the workspace root. |
| `stack::infer_ecosystem_from_stack` | Infer the dominant `Ecosystem` from a `DetectedStack`. |
| `stack::stack_name_for_ecosystem` | Return the user-facing stack label for an ecosystem. |
| `resolve_root` | Resolve workspace root from an explicit root or by walking up from a start path. |
| `is_dependency_manifest_path` | True when a workspace-relative path names a supported dependency manifest (`Cargo.toml`, `package.json`, `go.mod`, `requirements.txt`, `Gemfile`, `build.gradle`, `pom.xml`, `composer.json`, …). |
| `Error` / `Result` | Typed error enum with `discovery_hint()`; public result alias. |

## License

MIT. See [LICENSE](LICENSE).