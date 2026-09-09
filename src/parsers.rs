use std::path::Path;

use relative_path::RelativePathBuf;
use toml_edit::Item;
use tracing::warn;

use crate::packages::push_workspace_package;
use crate::types::{Ecosystem, WorkspacePackage};

pub(super) fn parse_gradle_includes(content: &str) -> Vec<String> {
  let mut modules = Vec::new();
  for line in content.lines() {
    let line = line.split("//").next().unwrap_or("").trim();
    if !line.contains("include") {
      continue;
    }
    for token in extract_quoted_strings(line) {
      let trimmed = token.trim().trim_start_matches(':');
      if trimmed.is_empty() {
        continue;
      }
      modules.push(trimmed.replace(':', "/"));
    }
  }
  modules
}

pub(super) fn parse_maven_modules(content: &str) -> Vec<String> {
  let mut modules = Vec::new();
  let mut rest = content;
  while let Some(start) = rest.find("<module>") {
    let after = &rest[start + "<module>".len()..];
    if let Some(end) = after.find("</module>") {
      let value = after[..end].trim();
      if !value.is_empty() {
        match RelativePathBuf::from_path(Path::new(value)) {
          Ok(path) => {
            let normalized = path.normalize().to_string();
            if !normalized.is_empty() {
              modules.push(normalized);
            }
          }
          Err(err) => {
            warn!(error = %err, value, "maven module path is not relative; ignoring");
          }
        }
      }
      rest = &after[end + "</module>".len()..];
    } else {
      break;
    }
  }
  modules
}

pub(super) fn parse_elixir_apps_path(content: &str) -> Option<String> {
  for line in content.lines() {
    let line = line.trim();
    if !line.contains("apps_path") {
      continue;
    }
    if let Some(start) = line.find('"')
      && let Some(end) = line[start + 1..].find('"')
    {
      let value = &line[start + 1..start + 1 + end];
      return Some(value.trim().to_string());
    }
    if let Some(start) = line.find('\'')
      && let Some(end) = line[start + 1..].find('\'')
    {
      let value = &line[start + 1..start + 1 + end];
      return Some(value.trim().to_string());
    }
  }
  None
}

pub(super) fn strip_json_comments(content: &str) -> String {
  let mut out = String::new();
  let mut chars = content.chars().peekable();
  while let Some(ch) = chars.next() {
    if ch == '"' {
      out.push(ch);
      let mut escaped = false;
      for next in chars.by_ref() {
        out.push(next);
        if escaped {
          escaped = false;
          continue;
        }
        if next == '\\' {
          escaped = true;
          continue;
        }
        if next == '"' {
          break;
        }
      }
      continue;
    }
    if ch == '/' {
      if let Some('/') = chars.peek().copied() {
        chars.next();
        for next in chars.by_ref() {
          if next == '\n' {
            out.push('\n');
            break;
          }
        }
        continue;
      }
      if let Some('*') = chars.peek().copied() {
        chars.next();
        while let Some(next) = chars.next() {
          if next == '*'
            && let Some('/') = chars.peek().copied()
          {
            chars.next();
            break;
          }
        }
        continue;
      }
    }
    out.push(ch);
  }
  out
}

pub(super) fn parse_cabal_packages(content: &str, repo_root: &Path) -> Vec<WorkspacePackage> {
  let mut packages = Vec::new();
  let mut in_packages = false;
  for line in content.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with("--") || trimmed.starts_with('#') {
      continue;
    }
    if trimmed.starts_with("packages:") {
      in_packages = true;
      let rest = trimmed.trim_start_matches("packages:").trim();
      if !rest.is_empty() {
        for entry in rest.split_whitespace() {
          push_workspace_package(&mut packages, repo_root, entry, Ecosystem::Haskell);
        }
      }
      continue;
    }
    if in_packages {
      if trimmed.is_empty() {
        continue;
      }
      if trimmed.contains(':') {
        in_packages = false;
        continue;
      }
      for entry in trimmed.split_whitespace() {
        push_workspace_package(&mut packages, repo_root, entry, Ecosystem::Haskell);
      }
    }
  }
  packages
}

pub(super) fn parse_sbt_modules(content: &str) -> Vec<String> {
  let mut modules = Vec::new();
  for line in content.lines() {
    let trimmed = line.trim();
    if !trimmed.contains("lazy val") {
      continue;
    }
    if let Some(start) = trimmed.find("lazy val") {
      let rest = trimmed[start + "lazy val".len()..].trim();
      if let Some(name) = rest.split_whitespace().next()
        && !name.is_empty()
      {
        modules.push(name.to_string());
      }
    }
  }
  modules
}

pub(super) fn parse_sln_projects(content: &str) -> Vec<String> {
  let mut projects = Vec::new();
  for line in content.lines() {
    let line = line.trim();
    if !line.starts_with("Project(") {
      continue;
    }
    let Some(rest) = line.split('=').nth(1) else {
      continue;
    };
    let mut parts = rest.split(',').map(str::trim);
    let _name = parts.next();
    let path = parts.next();
    if let Some(path) = path {
      let raw = path.trim_matches('"').trim();
      if raw.is_empty() {
        continue;
      }
      match RelativePathBuf::from_path(Path::new(raw)) {
        Ok(path) => {
          let normalized = path.normalize().to_string();
          if !normalized.is_empty() {
            projects.push(normalized);
          }
        }
        Err(err) => {
          warn!(error = %err, raw, "sln project path is not relative; ignoring");
        }
      }
    }
  }
  projects
}

fn extract_quoted_strings(line: &str) -> Vec<String> {
  let mut values = Vec::new();
  let mut chars = line.chars().peekable();
  while let Some(ch) = chars.next() {
    if ch == '"' || ch == '\'' {
      let quote = ch;
      let mut value = String::new();
      for next in chars.by_ref() {
        if next == quote {
          break;
        }
        value.push(next);
      }
      if !value.is_empty() {
        values.push(value);
      }
    }
  }
  values
}

pub(super) fn toml_array_strings(item: Option<&Item>) -> Vec<String> {
  let Some(item) = item else {
    return Vec::new();
  };
  let Some(value) = item.as_value() else {
    return Vec::new();
  };
  let Some(array) = value.as_array() else {
    return Vec::new();
  };
  array
    .iter()
    .filter_map(|entry| entry.as_str().map(std::string::ToString::to_string))
    .collect()
}

pub(super) fn parse_cargo_dependencies(doc: &toml_edit::DocumentMut) -> Vec<(String, String)> {
  let mut dependencies = Vec::new();
  push_toml_dependency_table(
    &mut dependencies,
    doc.as_item().get("dependencies"),
    "dependencies",
  );
  push_toml_dependency_table(
    &mut dependencies,
    doc.as_item().get("dev-dependencies"),
    "dev-dependencies",
  );
  push_toml_dependency_table(
    &mut dependencies,
    doc.as_item().get("build-dependencies"),
    "build-dependencies",
  );
  push_toml_dependency_table(
    &mut dependencies,
    doc
      .as_item()
      .get("workspace")
      .and_then(|item| item.get("dependencies")),
    "workspace.dependencies",
  );
  if let Some(targets) = doc
    .as_item()
    .get("target")
    .and_then(|item| item.as_table_like())
  {
    for target_name in targets
      .iter()
      .map(|(name, _)| name.to_string())
      .collect::<Vec<_>>()
    {
      let Some(target) = targets.get(&target_name) else {
        continue;
      };
      push_toml_dependency_table(
        &mut dependencies,
        target.get("dependencies"),
        &format!("target.{target_name}.dependencies"),
      );
      push_toml_dependency_table(
        &mut dependencies,
        target.get("dev-dependencies"),
        &format!("target.{target_name}.dev-dependencies"),
      );
      push_toml_dependency_table(
        &mut dependencies,
        target.get("build-dependencies"),
        &format!("target.{target_name}.build-dependencies"),
      );
    }
  }
  dedupe_dependencies(dependencies)
}

pub(super) fn parse_json_dependency_sections(
  json: &serde_json::Value,
  sections: &[&str],
) -> Vec<(String, String)> {
  let mut dependencies = Vec::new();
  for section in sections {
    let Some(object) = json.get(*section).and_then(|value| value.as_object()) else {
      continue;
    };
    for name in object.keys() {
      push_dependency_name(&mut dependencies, name, section);
    }
  }
  dedupe_dependencies(dependencies)
}

pub(super) fn parse_go_mod_dependencies(content: &str) -> Vec<(String, String)> {
  let mut dependencies = Vec::new();
  let mut in_require_block = false;
  for raw_line in content.lines() {
    let line = raw_line.split("//").next().unwrap_or("").trim();
    if line.is_empty() {
      continue;
    }
    if line == ")" && in_require_block {
      in_require_block = false;
      continue;
    }
    if line == "require (" {
      in_require_block = true;
      continue;
    }
    if let Some(rest) = line.strip_prefix("require ") {
      push_go_dependency(&mut dependencies, rest);
      continue;
    }
    if in_require_block {
      push_go_dependency(&mut dependencies, line);
    }
  }
  dedupe_dependencies(dependencies)
}

pub(super) fn parse_requirements_dependencies(content: &str) -> Vec<(String, String)> {
  let mut dependencies = Vec::new();
  for raw_line in content.lines() {
    let line = raw_line.split('#').next().unwrap_or("").trim();
    if line.is_empty() || line.starts_with('-') {
      continue;
    }
    let name = line
      .split(['=', '<', '>', '~', '!', '[', ';', ' '])
      .next()
      .unwrap_or("")
      .trim();
    push_dependency_name(&mut dependencies, name, "requirements");
  }
  dedupe_dependencies(dependencies)
}

pub(super) fn parse_gemfile_dependencies(content: &str) -> Vec<(String, String)> {
  let mut dependencies = Vec::new();
  for raw_line in content.lines() {
    let line = raw_line.split('#').next().unwrap_or("").trim();
    if !line.starts_with("gem ") {
      continue;
    }
    if let Some(name) = extract_quoted_strings(line).into_iter().next() {
      push_dependency_name(&mut dependencies, &name, "gem");
    }
  }
  dedupe_dependencies(dependencies)
}

pub(super) fn parse_gradle_dependencies(content: &str) -> Vec<(String, String)> {
  let mut dependencies = Vec::new();
  let mut in_dependencies_block = false;
  for raw_line in content.lines() {
    let line = raw_line.split("//").next().unwrap_or("").trim();
    if line.is_empty() {
      continue;
    }
    if line.starts_with("dependencies") && line.contains('{') {
      in_dependencies_block = true;
      continue;
    }
    if in_dependencies_block && line.starts_with('}') {
      in_dependencies_block = false;
      continue;
    }
    if !in_dependencies_block
      && !line.contains("implementation")
      && !line.contains("api")
      && !line.contains("testImplementation")
    {
      continue;
    }
    let Some(group) = line.split_whitespace().next() else {
      continue;
    };
    if group.is_empty() || group == "dependencies" || group == "}" {
      continue;
    }
    if let Some(quoted) = extract_quoted_strings(line).into_iter().next() {
      let name = gradle_dependency_name(&quoted);
      push_dependency_name(&mut dependencies, &name, group);
    }
  }
  dedupe_dependencies(dependencies)
}

pub(super) fn parse_maven_dependencies(content: &str) -> Vec<(String, String)> {
  let mut dependencies = Vec::new();
  let mut rest = content;
  while let Some(start) = rest.find("<dependency>") {
    let after = &rest[start + "<dependency>".len()..];
    let Some(end) = after.find("</dependency>") else {
      break;
    };
    let block = &after[..end];
    if let Some(artifact_id) = xml_tag_text(block, "artifactId") {
      let group = xml_tag_text(block, "scope").unwrap_or_else(|| "compile".to_string());
      push_dependency_name(&mut dependencies, &artifact_id, &group);
    }
    rest = &after[end + "</dependency>".len()..];
  }
  dedupe_dependencies(dependencies)
}

fn push_toml_dependency_table(
  dependencies: &mut Vec<(String, String)>,
  item: Option<&Item>,
  group: &str,
) {
  let Some(table) = item.and_then(|item| item.as_table_like()) else {
    return;
  };
  for name in table
    .iter()
    .map(|(name, _)| name.to_string())
    .collect::<Vec<_>>()
  {
    push_dependency_name(dependencies, &name, group);
  }
}

fn push_go_dependency(dependencies: &mut Vec<(String, String)>, line: &str) {
  let name = line.split_whitespace().next().unwrap_or("").trim();
  push_dependency_name(dependencies, name, "require");
}

fn push_dependency_name(dependencies: &mut Vec<(String, String)>, name: &str, group: &str) {
  let trimmed = name.trim();
  if trimmed.is_empty() {
    return;
  }
  dependencies.push((trimmed.to_string(), group.to_string()));
}

fn gradle_dependency_name(value: &str) -> String {
  let parts = value.split(':').collect::<Vec<_>>();
  if parts.len() >= 2 {
    return parts[1].trim().to_string();
  }
  value.trim().to_string()
}

fn xml_tag_text(content: &str, tag: &str) -> Option<String> {
  let start_tag = format!("<{tag}>");
  let end_tag = format!("</{tag}>");
  let start = content.find(&start_tag)?;
  let after = &content[start + start_tag.len()..];
  let end = after.find(&end_tag)?;
  let value = after[..end].trim();
  if value.is_empty() {
    None
  } else {
    Some(value.to_string())
  }
}

fn dedupe_dependencies(dependencies: Vec<(String, String)>) -> Vec<(String, String)> {
  let mut seen = std::collections::BTreeSet::new();
  let mut deduped = Vec::new();
  for dependency in dependencies {
    if seen.insert(dependency.clone()) {
      deduped.push(dependency);
    }
  }
  deduped
}
