use std::collections::{BTreeSet, HashSet};
use std::fs::{read_dir, read_to_string};
use std::path::Path;

use dotnet_lens::search;
use relative_path::RelativePath;

use crate::Result;
use toml_edit::DocumentMut;

use crate::packages::{default_package, package_name_for_dir, push_workspace_package};
use crate::parsers::{
  parse_cabal_packages, parse_cargo_dependencies, parse_elixir_apps_path,
  parse_gemfile_dependencies, parse_go_mod_dependencies, parse_gradle_dependencies,
  parse_gradle_includes, parse_json_dependency_sections, parse_maven_dependencies,
  parse_maven_modules, parse_requirements_dependencies, parse_sbt_modules, parse_sln_projects,
  strip_json_comments, toml_array_strings,
};
use crate::paths::WorkspacePath;
use crate::stack::stack_marker_ecosystem;
use crate::types::{Ecosystem, WorkspaceConfig, WorkspaceDependency, WorkspacePackage};

pub(super) fn detect_ecosystem_configs(
  dir: &Path,
  files: &[&Path],
) -> Result<Vec<WorkspaceConfig>> {
  let mut configs = Vec::new();
  if let Some(config) = detect_rust_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_node_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_go_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_bazel_workspace(dir, files) {
    configs.push(config);
  }
  if let Some(config) = detect_java_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_elixir_workspace(dir, files)? {
    configs.push(config);
  }
  if let Some(config) = detect_deno_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_haskell_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_dart_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_python_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_ruby_workspace(dir, files)? {
    configs.push(config);
  }
  if let Some(config) = detect_php_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_scala_workspace(dir)? {
    configs.push(config);
  }
  if let Some(config) = detect_dotnet_workspace(dir, files)? {
    configs.push(config);
  }

  Ok(configs)
}

fn dependency_records(
  dir: &Path,
  manifest_path: &Path,
  package_name: &str,
  ecosystem: Ecosystem,
  dependencies: Vec<(String, String)>,
) -> Vec<WorkspaceDependency> {
  let relative = manifest_path.strip_prefix(dir).unwrap_or(manifest_path);
  let Ok(manifest_path) = WorkspacePath::from_relative_path(relative) else {
    return Vec::new();
  };
  dependencies
    .into_iter()
    .map(|(name, group)| {
      WorkspaceDependency::new(name, group, manifest_path.clone(), package_name, ecosystem)
    })
    .collect()
}

fn root_package_name(dir: &Path) -> String {
  package_name_for_dir(dir, "")
}

fn cargo_dependencies_for_manifest(
  dir: &Path,
  manifest_path: &Path,
  package_name: &str,
) -> Result<Vec<WorkspaceDependency>> {
  let content = read_to_string(manifest_path)?;
  let doc = content.parse::<DocumentMut>()?;
  Ok(dependency_records(
    dir,
    manifest_path,
    package_name,
    Ecosystem::Rust,
    parse_cargo_dependencies(&doc),
  ))
}

fn node_dependencies_for_manifest(
  dir: &Path,
  manifest_path: &Path,
  package_name: &str,
) -> Result<Vec<WorkspaceDependency>> {
  let content = read_to_string(manifest_path)?;
  let json = serde_json::from_str::<serde_json::Value>(&content)?;
  Ok(dependency_records(
    dir,
    manifest_path,
    package_name,
    Ecosystem::Node,
    parse_json_dependency_sections(
      &json,
      &[
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
      ],
    ),
  ))
}

fn read_node_root_manifest(
  dir: &Path,
  packages: &mut Vec<WorkspacePackage>,
  package_json: &Path,
) -> Result<Vec<WorkspaceDependency>> {
  let content = read_to_string(package_json)?;
  let json = serde_json::from_str::<serde_json::Value>(&content)?;
  let dependencies = dependency_records(
    dir,
    package_json,
    &root_package_name(dir),
    Ecosystem::Node,
    parse_json_dependency_sections(
      &json,
      &[
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
      ],
    ),
  );
  if packages.is_empty()
    && let Some(workspaces) = json.get("workspaces")
  {
    let workspace_list = match workspaces {
      serde_json::Value::Array(arr) => arr.clone(),
      serde_json::Value::Object(obj) => obj
        .get("packages")
        .and_then(|packages| packages.as_array())
        .cloned()
        .unwrap_or_default(),
      _ => Vec::new(),
    };

    for ws in workspace_list {
      if let Some(ws_str) = ws.as_str() {
        push_workspace_package(packages, dir, ws_str, Ecosystem::Node);
      }
    }
  }
  Ok(dependencies)
}

fn detect_rust_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let cargo_toml = dir.join("Cargo.toml");
  if !cargo_toml.exists() {
    return Ok(None);
  }

  let content = read_to_string(&cargo_toml)?;
  let doc = content.parse::<DocumentMut>()?;
  let workspace = doc.get("workspace");
  let mut packages = Vec::new();
  if workspace.is_some() {
    let members = toml_array_strings(workspace.and_then(|item| item.get("members")));
    let excludes = toml_array_strings(workspace.and_then(|item| item.get("exclude")));
    let exclude_set: HashSet<String> = excludes.into_iter().collect();
    for member in members {
      let member = member.trim().to_string();
      if member.is_empty() || exclude_set.contains(&member) {
        continue;
      }
      push_workspace_package(&mut packages, dir, &member, Ecosystem::Rust);
    }
  }
  if packages.is_empty() {
    packages.push(default_package(dir, Ecosystem::Rust));
  }
  let package_name = root_package_name(dir);
  let mut dependencies = dependency_records(
    dir,
    &cargo_toml,
    &package_name,
    Ecosystem::Rust,
    parse_cargo_dependencies(&doc),
  );
  for package in &packages {
    let manifest_path = dir.join(package.path.as_str()).join("Cargo.toml");
    if manifest_path == cargo_toml || !manifest_path.exists() {
      continue;
    }
    dependencies.extend(cargo_dependencies_for_manifest(
      dir,
      &manifest_path,
      &package.name,
    )?);
  }

  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::Rust,
  }))
}

fn detect_node_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let mut packages = Vec::new();
  let mut dependencies = Vec::new();
  let package_json = dir.join("package.json");

  let pnpm_workspace = dir.join("pnpm-workspace.yaml");
  if pnpm_workspace.exists() {
    let content = read_to_string(&pnpm_workspace)?;
    let doc = serde_yaml::from_str::<serde_yaml::Value>(&content)?;
    if let Some(list) = doc.get("packages").and_then(|value| value.as_sequence()) {
      for entry in list {
        if let Some(pkg) = entry.as_str() {
          push_workspace_package(&mut packages, dir, pkg, Ecosystem::Node);
        }
      }
    }
  }

  if package_json.exists() {
    dependencies.extend(read_node_root_manifest(dir, &mut packages, &package_json)?);
  }

  if packages.is_empty() {
    let lerna_json = dir.join("lerna.json");
    if lerna_json.exists() {
      let content = read_to_string(&lerna_json)?;
      let json = serde_json::from_str::<serde_json::Value>(&content)?;
      if let Some(list) = json.get("packages").and_then(|value| value.as_array()) {
        for entry in list {
          if let Some(path) = entry.as_str() {
            push_workspace_package(&mut packages, dir, path, Ecosystem::Node);
          }
        }
      }
    }
  }

  if packages.is_empty() {
    let turbo = dir.join("turbo.json");
    let lerna = dir.join("lerna.json");
    let nx = dir.join("nx.json");
    if turbo.exists() || lerna.exists() || nx.exists() {
      for entry in read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join("package.json").exists() {
          let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
          if name != "node_modules" {
            let Ok(pkg_path) = WorkspacePath::parse(&name) else {
              continue;
            };
            packages.push(WorkspacePackage {
              name: name.clone(),
              path: pkg_path,
              ecosystem: Ecosystem::Node,
            });
          }
        }
      }
    }
  }

  if packages.is_empty() && package_json.exists() {
    packages.push(default_package(dir, Ecosystem::Node));
  }
  if packages.is_empty() {
    return Ok(None);
  }
  for package in &packages {
    let manifest_path = dir.join(package.path.as_str()).join("package.json");
    if manifest_path == package_json || !manifest_path.exists() {
      continue;
    }
    dependencies.extend(node_dependencies_for_manifest(
      dir,
      &manifest_path,
      &package.name,
    )?);
  }

  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::Node,
  }))
}

fn detect_go_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let go_work = dir.join("go.work");
  let go_mod = dir.join("go.mod");
  if !go_work.exists() && !go_mod.exists() {
    return Ok(None);
  }

  let content = if go_work.exists() {
    read_to_string(&go_work)?
  } else {
    String::new()
  };
  let mut packages = Vec::new();

  for line in content.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with("use") && !trimmed.starts_with("use (") {
      let path = trimmed.strip_prefix("use").unwrap_or("").trim();
      if !path.is_empty() {
        let pkg_path = dir.join(path);
        if pkg_path.exists() {
          let name = pkg_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string();
          let Ok(pkg_rel) = WorkspacePath::parse(path) else {
            continue;
          };
          packages.push(WorkspacePackage {
            name,
            path: pkg_rel,
            ecosystem: Ecosystem::Go,
          });
        }
      }
    }
  }

  if let Some(use_start) = content.find("use (") {
    let rest = &content[use_start + 5..];
    if let Some(use_end) = rest.find(')') {
      let use_block = &rest[..use_end];
      for line in use_block.lines() {
        let path = line.trim();
        if !path.is_empty() {
          let pkg_path = dir.join(path);
          if pkg_path.exists() {
            let name = pkg_path
              .file_name()
              .and_then(|name| name.to_str())
              .unwrap_or(path)
              .to_string();
            let Ok(pkg_rel) = WorkspacePath::parse(path) else {
              continue;
            };
            packages.push(WorkspacePackage {
              name,
              path: pkg_rel,
              ecosystem: Ecosystem::Go,
            });
          }
        }
      }
    }
  }

  if packages.is_empty() {
    packages.push(default_package(dir, Ecosystem::Go));
  }
  let dependencies = if go_mod.exists() {
    dependency_records(
      dir,
      &go_mod,
      &root_package_name(dir),
      Ecosystem::Go,
      parse_go_mod_dependencies(&read_to_string(&go_mod)?),
    )
  } else {
    Vec::new()
  };

  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::Go,
  }))
}

fn detect_bazel_workspace(dir: &Path, files: &[&Path]) -> Option<WorkspaceConfig> {
  let workspace_root = dir.join("WORKSPACE").exists()
    || dir.join("WORKSPACE.bazel").exists()
    || dir.join("MODULE.bazel").exists();
  if !workspace_root {
    return None;
  }

  let mut packages = Vec::new();
  let mut seen: HashSet<WorkspacePath> = HashSet::new();
  for file in files {
    let file_name = file
      .file_name()
      .and_then(|name| name.to_str())
      .unwrap_or("");
    if file_name != "BUILD" && file_name != "BUILD.bazel" {
      continue;
    }
    let dir_path = file.parent().unwrap_or(Path::new("."));
    let Ok(dir_path) = WorkspacePath::from_relative_path(dir_path) else {
      continue;
    };
    if !seen.insert(dir_path.clone()) {
      continue;
    }
    let name = package_name_for_dir(dir, dir_path.as_str());
    packages.push(WorkspacePackage {
      name,
      path: dir_path,
      ecosystem: Ecosystem::Bazel,
    });
  }

  if packages.is_empty() {
    packages.push(default_package(dir, Ecosystem::Bazel));
  }

  Some(WorkspaceConfig {
    packages,
    dependencies: Vec::new(),
    ecosystem: Ecosystem::Bazel,
  })
}

fn detect_java_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  if let Some(config) = detect_gradle_workspace(dir)? {
    return Ok(Some(config));
  }
  if let Some(config) = detect_maven_workspace(dir)? {
    return Ok(Some(config));
  }
  Ok(None)
}

fn detect_gradle_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let settings_files = ["settings.gradle", "settings.gradle.kts"];
  let mut settings_path = None;
  for name in settings_files {
    let path = dir.join(name);
    if path.exists() {
      settings_path = Some(path);
      break;
    }
  }
  let build_files = ["build.gradle", "build.gradle.kts"];
  let build_path = build_files
    .iter()
    .map(|name| dir.join(name))
    .find(|path| path.exists());
  if settings_path.is_none() && build_path.is_none() {
    return Ok(None);
  }
  let mut packages = Vec::new();
  if let Some(settings_path) = settings_path {
    let content = read_to_string(&settings_path)?;
    let modules = parse_gradle_includes(&content);
    for module in modules {
      push_workspace_package(&mut packages, dir, &module, Ecosystem::Gradle);
    }
  }
  if packages.is_empty() {
    packages.push(default_package(dir, Ecosystem::Gradle));
  }
  let dependencies = if let Some(build_path) = build_path {
    dependency_records(
      dir,
      &build_path,
      &root_package_name(dir),
      Ecosystem::Gradle,
      parse_gradle_dependencies(&read_to_string(&build_path)?),
    )
  } else {
    Vec::new()
  };
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::Gradle,
  }))
}

fn detect_maven_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let pom = dir.join("pom.xml");
  if !pom.exists() {
    return Ok(None);
  }
  let content = read_to_string(&pom)?;
  let modules = parse_maven_modules(&content);
  let mut packages = Vec::new();
  for module in modules {
    push_workspace_package(&mut packages, dir, &module, Ecosystem::Maven);
  }
  if packages.is_empty() {
    packages.push(default_package(dir, Ecosystem::Maven));
  }
  let dependencies = dependency_records(
    dir,
    &pom,
    &root_package_name(dir),
    Ecosystem::Maven,
    parse_maven_dependencies(&content),
  );
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::Maven,
  }))
}

fn detect_elixir_workspace(dir: &Path, files: &[&Path]) -> Result<Option<WorkspaceConfig>> {
  let mix_exs = dir.join("mix.exs");
  if !mix_exs.exists() {
    return Ok(None);
  }
  let content = read_to_string(&mix_exs)?;
  let Some(apps_path) = parse_elixir_apps_path(&content) else {
    return Ok(None);
  };
  let mut packages = Vec::new();
  let mut seen: HashSet<WorkspacePath> = HashSet::new();
  for file in files {
    if file.file_name().and_then(|n| n.to_str()) != Some("mix.exs") {
      continue;
    }
    let parent = file.parent().unwrap_or(Path::new("."));
    let Ok(parent_path) = WorkspacePath::from_relative_path(parent) else {
      continue;
    };
    if !parent_path.as_str().starts_with(&apps_path) {
      continue;
    }
    if !seen.insert(parent_path.clone()) {
      continue;
    }
    let name = package_name_for_dir(dir, parent_path.as_str());
    packages.push(WorkspacePackage {
      name,
      path: parent_path,
      ecosystem: Ecosystem::Elixir,
    });
  }
  if packages.is_empty() {
    return Ok(None);
  }
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies: Vec::new(),
    ecosystem: Ecosystem::Elixir,
  }))
}

fn detect_deno_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let candidates = ["deno.json", "deno.jsonc"];
  let mut config_path = None;
  for name in candidates {
    let path = dir.join(name);
    if path.exists() {
      config_path = Some(path);
      break;
    }
  }
  let Some(config_path) = config_path else {
    return Ok(None);
  };
  let content = read_to_string(&config_path)?;
  let normalized = if config_path
    .extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonc"))
  {
    strip_json_comments(&content)
  } else {
    content
  };
  let Ok(json) = serde_json::from_str::<serde_json::Value>(&normalized) else {
    return Ok(None);
  };
  let Some(list) = json.get("workspace").and_then(|value| value.as_array()) else {
    return Ok(None);
  };
  let mut packages = Vec::new();
  for entry in list {
    if let Some(path) = entry.as_str() {
      push_workspace_package(&mut packages, dir, path, Ecosystem::Deno);
    }
  }
  if packages.is_empty() {
    return Ok(None);
  }
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies: Vec::new(),
    ecosystem: Ecosystem::Deno,
  }))
}

fn detect_haskell_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let cabal_project = dir.join("cabal.project");
  if !cabal_project.exists() {
    return Ok(None);
  }
  let content = read_to_string(&cabal_project)?;
  let packages = parse_cabal_packages(&content, dir);
  if packages.is_empty() {
    return Ok(None);
  }
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies: Vec::new(),
    ecosystem: Ecosystem::Haskell,
  }))
}

fn detect_dart_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  if let Some(config) = detect_yaml_workspace(dir, "melos.yaml", "packages", Ecosystem::Dart)? {
    return Ok(Some(config));
  }
  if let Some(config) = detect_yaml_workspace(dir, "pubspec.yaml", "workspace", Ecosystem::Dart)? {
    return Ok(Some(config));
  }
  Ok(None)
}

fn detect_python_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let pyproject = dir.join("pyproject.toml");
  let requirements = dir.join("requirements.txt");
  if !pyproject.exists() && !requirements.exists() {
    return Ok(None);
  }
  let mut packages = Vec::new();
  if pyproject.exists() {
    let content = read_to_string(&pyproject)?;
    let doc = content.parse::<DocumentMut>()?;
    let tool = doc.get("tool");
    let uv_workspace = tool
      .and_then(|item| item.get("uv"))
      .and_then(|item| item.get("workspace"));
    let members = toml_array_strings(uv_workspace.and_then(|item| item.get("members")));
    let excludes = toml_array_strings(uv_workspace.and_then(|item| item.get("exclude")));
    let exclude_set: HashSet<String> = excludes.into_iter().collect();
    for member in members {
      if exclude_set.contains(&member) {
        continue;
      }
      push_workspace_package(&mut packages, dir, &member, Ecosystem::Python);
    }
  }
  if packages.is_empty() {
    packages.push(default_package(dir, Ecosystem::Python));
  }
  let dependencies = if requirements.exists() {
    dependency_records(
      dir,
      &requirements,
      &root_package_name(dir),
      Ecosystem::Python,
      parse_requirements_dependencies(&read_to_string(&requirements)?),
    )
  } else {
    Vec::new()
  };
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::Python,
  }))
}

fn detect_ruby_workspace(dir: &Path, files: &[&Path]) -> Result<Option<WorkspaceConfig>> {
  let gemfile = dir.join("Gemfile");
  let mut packages = Vec::new();
  let mut seen: HashSet<WorkspacePath> = HashSet::new();
  for file in files {
    if !file
      .extension()
      .and_then(|ext| ext.to_str())
      .is_some_and(|ext| ext.eq_ignore_ascii_case("gemspec"))
    {
      continue;
    }
    let parent = file.parent().unwrap_or(Path::new("."));
    let Ok(parent_path) = WorkspacePath::from_relative_path(parent) else {
      continue;
    };
    if !seen.insert(parent_path.clone()) {
      continue;
    }
    let name = package_name_for_dir(dir, parent_path.as_str());
    packages.push(WorkspacePackage {
      name,
      path: parent_path,
      ecosystem: Ecosystem::Ruby,
    });
  }
  if packages.is_empty() && gemfile.exists() {
    packages.push(default_package(dir, Ecosystem::Ruby));
  }
  if packages.is_empty() {
    return Ok(None);
  }
  let dependencies = if gemfile.exists() {
    dependency_records(
      dir,
      &gemfile,
      &root_package_name(dir),
      Ecosystem::Ruby,
      parse_gemfile_dependencies(&read_to_string(&gemfile)?),
    )
  } else {
    Vec::new()
  };
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::Ruby,
  }))
}

fn detect_php_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let composer = dir.join("composer.json");
  if !composer.exists() {
    return Ok(None);
  }
  let content = read_to_string(&composer)?;
  let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
    return Ok(None);
  };
  let mut packages = Vec::new();
  if let Some(repos) = json.get("repositories").and_then(|value| value.as_array()) {
    for repo in repos {
      let repo_type = repo.get("type").and_then(|value| value.as_str());
      let repo_url = repo.get("url").and_then(|value| value.as_str());
      if repo_type == Some("path")
        && let Some(path) = repo_url
      {
        push_workspace_package(&mut packages, dir, path, Ecosystem::PHP);
      }
    }
  }
  if packages.is_empty() {
    packages.push(default_package(dir, Ecosystem::PHP));
  }
  let dependencies = dependency_records(
    dir,
    &composer,
    &root_package_name(dir),
    Ecosystem::PHP,
    parse_json_dependency_sections(&json, &["require", "require-dev"]),
  );
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies,
    ecosystem: Ecosystem::PHP,
  }))
}

fn detect_scala_workspace(dir: &Path) -> Result<Option<WorkspaceConfig>> {
  let build_sbt = dir.join("build.sbt");
  if !build_sbt.exists() {
    return Ok(None);
  }
  let content = read_to_string(&build_sbt)?;
  let modules = parse_sbt_modules(&content);
  if modules.is_empty() {
    return Ok(None);
  }
  let mut packages = Vec::new();
  for module in modules {
    push_workspace_package(&mut packages, dir, &module, Ecosystem::Scala);
  }
  if packages.is_empty() {
    return Ok(None);
  }
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies: Vec::new(),
    ecosystem: Ecosystem::Scala,
  }))
}

fn detect_dotnet_workspace(dir: &Path, files: &[&Path]) -> Result<Option<WorkspaceConfig>> {
  let mut has_marker = false;
  let mut solution_path = None;
  for file in files {
    if file
      .extension()
      .and_then(|ext| ext.to_str())
      .is_some_and(|ext| ext.eq_ignore_ascii_case("sln") || ext.eq_ignore_ascii_case("slnx"))
    {
      has_marker = true;
      if solution_path.is_none() {
        let path = Path::new(file);
        solution_path = Some(dir.join(path));
      }
    } else if file
      .extension()
      .and_then(|ext| ext.to_str())
      .is_some_and(|ext| {
        ext.eq_ignore_ascii_case("csproj")
          || ext.eq_ignore_ascii_case("fsproj")
          || ext.eq_ignore_ascii_case("vbproj")
      })
    {
      has_marker = true;
    }
    if has_marker && solution_path.is_some() {
      break;
    }
  }

  if !has_marker && let Ok(entries) = read_dir(dir) {
    for entry in entries.flatten() {
      let path = entry.path();
      if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
          ext.eq_ignore_ascii_case("sln")
            || ext.eq_ignore_ascii_case("slnx")
            || ext.eq_ignore_ascii_case("csproj")
            || ext.eq_ignore_ascii_case("fsproj")
            || ext.eq_ignore_ascii_case("vbproj")
        })
      {
        has_marker = true;
        if solution_path.is_none()
          && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("sln"))
        {
          solution_path = Some(path);
        }
        break;
      }
    }
  }

  if !has_marker {
    return Ok(None);
  }

  let mut packages = Vec::new();
  let mut seen: HashSet<WorkspacePath> = HashSet::new();

  if let Some(solution_path) = solution_path
    && let Ok(content) = read_to_string(&solution_path)
  {
    let sln_dir = solution_path.parent().unwrap_or(dir);
    for project in parse_sln_projects(&content) {
      let project_path = sln_dir.join(&project);
      push_dotnet_project(&mut packages, &mut seen, dir, &project_path);
    }
  }

  if packages.is_empty() {
    let dir_buf = dir.to_path_buf();
    let projects = search::search_projects(&dir_buf)?;
    for project in projects {
      push_dotnet_project(&mut packages, &mut seen, dir, &project);
    }
  }

  if packages.is_empty() {
    return Ok(None);
  }

  Ok(Some(WorkspaceConfig {
    packages,
    dependencies: Vec::new(),
    ecosystem: Ecosystem::DotNet,
  }))
}

pub(super) fn detect_workspace_packages_from_files(
  workspace_root: &Path,
  files: &[&Path],
) -> (Vec<WorkspacePackage>, BTreeSet<Ecosystem>) {
  let mut packages = Vec::new();
  let mut ecosystems = BTreeSet::new();
  let mut seen: HashSet<(Ecosystem, WorkspacePath)> = HashSet::new();

  for file in files {
    let file_name = file
      .file_name()
      .and_then(|name| name.to_str())
      .unwrap_or("")
      .to_ascii_lowercase();
    let Some(ecosystem) = stack_marker_ecosystem(&file_name) else {
      continue;
    };

    ecosystems.insert(ecosystem);

    let dir_rel = RelativePath::from_path(file).unwrap_or(RelativePath::new(""));
    let Ok(dir_path) = WorkspacePath::parse(dir_rel.as_str()) else {
      continue;
    };
    if !seen.insert((ecosystem, dir_path.clone())) {
      continue;
    }
    let name = package_name_for_dir(workspace_root, dir_path.as_str());
    packages.push(WorkspacePackage {
      name,
      path: dir_path,
      ecosystem,
    });
  }

  (packages, ecosystems)
}

fn detect_yaml_workspace(
  dir: &Path,
  file_name: &str,
  key: &str,
  ecosystem: Ecosystem,
) -> Result<Option<WorkspaceConfig>> {
  let path = dir.join(file_name);
  if !path.exists() {
    return Ok(None);
  }
  let content = read_to_string(&path)?;
  let doc = serde_yaml::from_str::<serde_yaml::Value>(&content)?;
  let mut packages = Vec::new();
  if let Some(list) = doc.get(key).and_then(|value| value.as_sequence()) {
    for entry in list {
      if let Some(path) = entry.as_str() {
        push_workspace_package(&mut packages, dir, path, ecosystem);
      }
    }
  }
  if packages.is_empty() {
    return Ok(None);
  }
  Ok(Some(WorkspaceConfig {
    packages,
    dependencies: Vec::new(),
    ecosystem,
  }))
}

fn push_dotnet_project(
  packages: &mut Vec<WorkspacePackage>,
  seen: &mut HashSet<WorkspacePath>,
  root: &Path,
  project_path: &Path,
) {
  let parent = project_path.parent().unwrap_or(root);
  let relative = parent.strip_prefix(root).unwrap_or(parent);
  let Ok(rel) = WorkspacePath::from_relative_path(relative) else {
    return;
  };
  if !seen.insert(rel.clone()) {
    return;
  }
  let name = project_path
    .file_stem()
    .and_then(|value| value.to_str())
    .map_or_else(|| package_name_for_dir(root, rel.as_str()), str::to_string);
  packages.push(WorkspacePackage {
    name,
    path: rel,
    ecosystem: Ecosystem::DotNet,
  });
}
