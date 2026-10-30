//! Root and package discovery against real repositories and on-disk fixtures.

use std::env;
use std::fs::{create_dir_all, remove_dir_all, write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use pretty_assertions::assert_eq;

use crate::scan::Scan;
use crate::{DiscoveryWarning, Error, Vcs, Workspace};

struct TempDir {
  path: PathBuf,
}

impl TempDir {
  fn new(prefix: &str) -> Self {
    let unique = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("system clock before unix epoch")
      .as_nanos();
    let path = env::temp_dir().join(format!(
      "code-workspace-vcs-{prefix}-{}-{unique}",
      std::process::id()
    ));
    create_dir_all(&path).expect("failed to create temp dir");
    Self { path }
  }

  fn path(&self) -> &Path {
    &self.path
  }
}

impl Drop for TempDir {
  fn drop(&mut self) {
    let _ = remove_dir_all(&self.path);
  }
}

fn run(dir: &Path, program: &str, args: &[&str]) {
  let status = Command::new(program)
    .args(args)
    .current_dir(dir)
    .status()
    .unwrap_or_else(|err| panic!("failed to launch {program}: {err}"));
  assert!(status.success(), "{program} {args:?} failed with {status}");
}

fn write_file(path: &Path, contents: &str) {
  create_dir_all(path.parent().expect("fixture path has a parent")).expect("failed to create dir");
  write(path, contents).expect("failed to write fixture file");
}

fn git(dir: &Path, args: &[&str]) {
  let mut full = vec!["-c", "user.name=test", "-c", "user.email=test@example.com"];
  full.extend_from_slice(args);
  run(dir, "git", &full);
}

fn jj(dir: &Path, args: &[&str]) {
  let mut full = vec![
    "--config",
    "user.name=test",
    "--config",
    "user.email=test@example.com",
  ];
  full.extend_from_slice(args);
  run(dir, "jj", &full);
}

fn canonical(path: &Path) -> PathBuf {
  path.canonicalize().expect("failed to canonicalize")
}

#[test]
fn vcs_root_wins_over_nested_cargo_workspace() {
  let temp = TempDir::new("nested-cargo");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  git(&repo, &["init"]);
  let nested = repo.join("tools");
  create_dir_all(nested.join("src")).expect("failed to create nested dir");
  write(nested.join("Cargo.toml"), "[workspace]\nmembers = []\n")
    .expect("failed to write manifest");

  let workspace = Workspace::discover(nested.join("src")).expect("discovery should succeed");

  assert_eq!(workspace.root(), repo.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&repo).as_path())
  );
}

#[test]
fn git_worktree_links_back_to_main_repository() {
  let temp = TempDir::new("git-worktree");
  let main = temp.path().join("main");
  create_dir_all(&main).expect("failed to create main dir");
  git(&main, &["init"]);
  git(&main, &["commit", "--allow-empty", "-m", "init"]);
  git(&main, &["worktree", "add", "../linked"]);
  let linked = temp.path().join("linked");

  let workspace = Workspace::discover(&linked).expect("discovery should succeed");

  assert_eq!(workspace.root(), linked.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&main).as_path())
  );
}

#[test]
fn jj_repository_without_git_directory_is_found() {
  let temp = TempDir::new("jj-plain");
  let main = temp.path().join("main");
  jj(temp.path(), &["git", "init", "--no-colocate", "main"]);
  create_dir_all(main.join("src")).expect("failed to create src dir");

  let workspace = Workspace::discover(main.join("src")).expect("discovery should succeed");

  assert!(!main.join(".git").exists());
  assert_eq!(workspace.root(), main.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&main).as_path())
  );
}

#[test]
fn secondary_jj_workspace_links_back_to_main_repository() {
  let temp = TempDir::new("jj-workspace");
  let main = temp.path().join("main");
  jj(temp.path(), &["git", "init", "--no-colocate", "main"]);
  jj(&main, &["workspace", "add", "--no-colocate", "../second"]);
  let second = temp.path().join("second");

  let workspace = Workspace::discover(&second).expect("discovery should succeed");

  assert_eq!(workspace.root(), second.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&main).as_path())
  );
}

#[test]
fn colocated_secondary_jj_workspace_links_back_to_main_repository() {
  let temp = TempDir::new("jj-colocated");
  let main = temp.path().join("main");
  jj(temp.path(), &["git", "init", "--colocate", "main"]);
  jj(&main, &["workspace", "add", "--colocate", "../second"]);
  let second = temp.path().join("second");

  let workspace = Workspace::discover(&second).expect("discovery should succeed");

  assert!(second.join(".git").is_file());
  assert_eq!(workspace.root(), second.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&main).as_path())
  );
}

#[test]
fn malformed_git_pointer_file_is_an_error() {
  let temp = TempDir::new("bad-pointer");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  write(repo.join(".git"), "not a pointer\n").expect("failed to write pointer file");

  let err = Workspace::discover(&repo).expect_err("malformed pointer should fail");

  assert!(
    matches!(err, Error::InvalidVcsPointer { ref path } if path == &repo.join(".git")),
    "unexpected error: {err:?}"
  );
}

#[test]
fn vcs_kind_distinguishes_git_jujutsu_and_colocated() {
  let temp = TempDir::new("vcs-kind");
  let git_repo = temp.path().join("git");
  create_dir_all(&git_repo).expect("failed to create git dir");
  git(&git_repo, &["init"]);
  jj(temp.path(), &["git", "init", "--no-colocate", "plain-jj"]);
  jj(temp.path(), &["git", "init", "--colocate", "colocated"]);
  let go_only = temp.path().join("go-only");
  write_file(&go_only.join("go.work"), "go 1.22\n");

  let kind = |path: &Path| Workspace::from_root(path).expect("from_root").vcs();

  assert_eq!(kind(&git_repo), Some(Vcs::Git));
  assert_eq!(kind(&temp.path().join("plain-jj")), Some(Vcs::Jujutsu));
  assert_eq!(
    kind(&temp.path().join("colocated")),
    Some(Vcs::JujutsuColocated)
  );
  assert_eq!(kind(&go_only), None);
}

#[test]
fn from_root_on_linked_worktree_reports_main_repository() {
  let temp = TempDir::new("from-root-linked");
  let main = temp.path().join("main");
  create_dir_all(&main).expect("failed to create main dir");
  git(&main, &["init"]);
  git(&main, &["commit", "--allow-empty", "-m", "init"]);
  git(&main, &["worktree", "add", "../linked"]);
  let linked = temp.path().join("linked");

  let workspace = Workspace::from_root(&linked).expect("from_root should succeed");

  assert_eq!(workspace.root(), linked.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&main).as_path())
  );
}

#[test]
fn worktree_of_bare_repository_links_to_the_bare_directory() {
  let temp = TempDir::new("bare-worktree");
  let seed = temp.path().join("seed");
  create_dir_all(&seed).expect("failed to create seed dir");
  git(&seed, &["init"]);
  git(&seed, &["commit", "--allow-empty", "-m", "init"]);
  git(temp.path(), &["clone", "--bare", "seed", "bare.git"]);
  let bare = temp.path().join("bare.git");
  git(&bare, &["worktree", "add", "../linked"]);
  let linked = temp.path().join("linked");

  let workspace = Workspace::discover(&linked).expect("discovery should succeed");

  assert_eq!(workspace.root(), linked.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&bare).as_path())
  );
}

#[test]
fn submodule_is_its_own_repository() {
  let temp = TempDir::new("submodule");
  let sub = temp.path().join("sub");
  create_dir_all(&sub).expect("failed to create sub dir");
  git(&sub, &["init"]);
  git(&sub, &["commit", "--allow-empty", "-m", "init"]);
  let main = temp.path().join("main");
  create_dir_all(&main).expect("failed to create main dir");
  git(&main, &["init"]);
  let sub_url = sub.to_str().expect("temp path is not utf-8");
  git(
    &main,
    &[
      "-c",
      "protocol.file.allow=always",
      "submodule",
      "add",
      sub_url,
      "sub",
    ],
  );
  let checkout = main.join("sub");

  let workspace = Workspace::discover(&checkout).expect("discovery should succeed");

  assert!(checkout.join(".git").is_file());
  assert_eq!(workspace.root(), checkout.as_path());
  assert_eq!(
    workspace.repository_root(),
    Some(canonical(&checkout).as_path())
  );
}

#[test]
fn enclosing_workspace_that_lists_the_inner_directory_wins_without_vcs() {
  let temp = TempDir::new("outer-lists-inner");
  let outer = temp.path().join("outer");
  write_file(
    &outer.join("Cargo.toml"),
    "[workspace]\nmembers = [\"crates/inner\"]\n",
  );
  write_file(&outer.join("crates/inner/Cargo.toml"), "[workspace]\n");
  create_dir_all(outer.join("crates/inner/src")).expect("failed to create src dir");

  let workspace =
    Workspace::discover(outer.join("crates/inner/src")).expect("discovery should succeed");

  assert_eq!(workspace.root(), outer.as_path());
}

#[test]
fn enclosing_workspace_that_does_not_list_the_inner_directory_is_ignored() {
  let temp = TempDir::new("outer-ignores-inner");
  let outer = temp.path().join("outer");
  write_file(
    &outer.join("Cargo.toml"),
    "[workspace]\nmembers = [\"other\"]\n",
  );
  write_file(
    &outer.join("other/Cargo.toml"),
    "[package]\nname = \"other\"\n",
  );
  let inner = outer.join("tools/inner");
  write_file(&inner.join("Cargo.toml"), "[workspace]\n");

  let workspace = Workspace::discover(&inner).expect("discovery should succeed");

  assert_eq!(workspace.root(), inner.as_path());
}

#[test]
fn vcs_root_reports_packages_of_nested_language_workspaces() {
  let temp = TempDir::new("nested-packages");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  git(&repo, &["init"]);
  write_file(
    &repo.join("web/package.json"),
    "{\"name\":\"web\",\"workspaces\":[\"packages/*\"]}",
  );
  write_file(
    &repo.join("web/packages/ui/package.json"),
    "{\"name\":\"ui\"}",
  );
  write_file(
    &repo.join("tools/Cargo.toml"),
    "[workspace]\nmembers = [\"a\"]\n",
  );
  write_file(
    &repo.join("tools/a/Cargo.toml"),
    "[package]\nname = \"a\"\n",
  );

  let workspace = Workspace::discover(&repo).expect("discovery should succeed");

  let mut paths: Vec<&str> = workspace
    .packages()
    .iter()
    .map(|package| package.path.as_str())
    .collect();
  paths.sort_unstable();
  assert_eq!(paths, vec!["tools/a", "web/packages/ui"]);
}

fn package_paths(workspace: &Workspace) -> Vec<&str> {
  let mut paths: Vec<&str> = workspace
    .packages()
    .iter()
    .map(|package| package.path.as_str())
    .collect();
  paths.sort_unstable();
  paths
}

#[test]
fn project_file_is_the_root_of_last_resort_without_vcs_or_workspace_marker() {
  let temp = TempDir::new("go-mod-only");
  let module = temp.path().join("module");
  write_file(&module.join("go.mod"), "module example.com/m\n\ngo 1.22\n");
  write_file(&module.join("cmd/main.go"), "package main\n");

  let workspace = Workspace::discover(module.join("cmd")).expect("discovery should succeed");

  assert_eq!(workspace.root(), module.as_path());
  assert_eq!(workspace.vcs(), None);
}

#[test]
fn workspace_marker_beats_the_nearest_project_file() {
  let temp = TempDir::new("marker-beats-project");
  let outer = temp.path().join("outer");
  write_file(
    &outer.join("Cargo.toml"),
    "[workspace]\nmembers = [\"crates/a\"]\n",
  );
  write_file(
    &outer.join("crates/a/Cargo.toml"),
    "[package]\nname = \"a\"\n",
  );

  let workspace = Workspace::discover(outer.join("crates/a")).expect("discovery should succeed");

  assert_eq!(workspace.root(), outer.as_path());
}

#[test]
fn vcs_root_reports_standalone_nested_projects() {
  let temp = TempDir::new("standalone-projects");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  git(&repo, &["init"]);
  write_file(
    &repo.join("services/api/go.mod"),
    "module example.com/api\n\ngo 1.22\n",
  );
  write_file(&repo.join("web/package.json"), "{\"name\":\"web\"}");

  let workspace = Workspace::discover(&repo).expect("discovery should succeed");

  assert_eq!(package_paths(&workspace), vec!["services/api", "web"]);
  assert!(workspace.warnings().is_empty());
}

#[test]
fn elixir_umbrella_apps_are_found_from_the_scanned_files() {
  let temp = TempDir::new("elixir-umbrella");
  let umbrella = temp.path().join("umbrella");
  write_file(
    &umbrella.join("mix.exs"),
    "defmodule U.MixProject do\n  def project, do: [apps_path: \"apps\"]\nend\n",
  );
  write_file(
    &umbrella.join("apps/a/mix.exs"),
    "defmodule A.MixProject do\nend\n",
  );
  write_file(
    &umbrella.join("apps/b/mix.exs"),
    "defmodule B.MixProject do\nend\n",
  );

  let workspace = Workspace::discover(umbrella.join("apps/a")).expect("discovery should succeed");

  assert_eq!(workspace.root(), umbrella.as_path());
  assert_eq!(package_paths(&workspace), vec!["apps/a", "apps/b"]);
}

#[test]
fn bazel_packages_come_from_build_files() {
  let temp = TempDir::new("bazel-build-files");
  let root = temp.path().join("bazel");
  write_file(&root.join("WORKSPACE"), "workspace(name = \"w\")\n");
  write_file(&root.join("app/BUILD"), "# app\n");
  write_file(&root.join("lib/BUILD.bazel"), "# lib\n");

  let workspace = Workspace::discover(root.join("app")).expect("discovery should succeed");

  assert_eq!(workspace.root(), root.as_path());
  assert_eq!(package_paths(&workspace), vec!["app", "lib"]);
}

#[test]
fn ruby_packages_come_from_gemspec_files() {
  let temp = TempDir::new("ruby-gemspecs");
  let root = temp.path().join("ruby");
  write_file(&root.join("Gemfile"), "source \"https://rubygems.org\"\n");
  write_file(
    &root.join("gems/foo/foo.gemspec"),
    "Gem::Specification.new\n",
  );

  let workspace = Workspace::discover(&root).expect("discovery should succeed");

  assert_eq!(package_paths(&workspace), vec!["gems/foo"]);
}

#[test]
fn gitignored_and_vendor_directories_are_not_scanned() {
  let temp = TempDir::new("ignored-dirs");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  git(&repo, &["init"]);
  write_file(&repo.join(".gitignore"), "scratch/\n");
  write_file(&repo.join("kept/go.mod"), "module example.com/kept\n");
  write_file(&repo.join("scratch/go.mod"), "module example.com/scratch\n");
  write_file(&repo.join("vendor/dep/go.mod"), "module example.com/dep\n");

  let workspace = Workspace::discover(&repo).expect("discovery should succeed");

  assert_eq!(package_paths(&workspace), vec!["kept"]);
}

#[test]
fn gitignore_is_honored_in_a_jj_repository_without_a_git_directory() {
  let temp = TempDir::new("ignored-jj");
  let repo = temp.path().join("repo");
  jj(temp.path(), &["git", "init", "--no-colocate", "repo"]);
  write_file(&repo.join(".gitignore"), "scratch/\n");
  write_file(&repo.join("kept/go.mod"), "module example.com/kept\n");
  write_file(&repo.join("scratch/go.mod"), "module example.com/scratch\n");

  let workspace = Workspace::discover(&repo).expect("discovery should succeed");

  assert_eq!(package_paths(&workspace), vec!["kept"]);
}

#[test]
fn malformed_nested_manifest_keeps_its_package_and_warns() {
  let temp = TempDir::new("bad-nested-manifest");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  git(&repo, &["init"]);
  write_file(&repo.join("good/package.json"), "{\"name\":\"good\"}");
  write_file(&repo.join("bad/package.json"), "{not json");

  let workspace = Workspace::discover(&repo).expect("discovery should succeed");

  assert_eq!(package_paths(&workspace), vec!["bad", "good"]);
  assert!(workspace.dependencies().is_empty());
  assert_eq!(
    workspace.warnings(),
    [DiscoveryWarning::InvalidProject {
      path: repo.join("bad")
    }]
  );
}

#[cfg(unix)]
#[test]
fn unreadable_directory_is_skipped_with_a_warning() {
  use std::fs::{Permissions, set_permissions};
  use std::os::unix::fs::PermissionsExt;

  struct Unlock(PathBuf);
  impl Drop for Unlock {
    fn drop(&mut self) {
      let _ = set_permissions(&self.0, Permissions::from_mode(0o755));
    }
  }

  let temp = TempDir::new("unreadable-dir");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  git(&repo, &["init"]);
  write_file(&repo.join("good/package.json"), "{\"name\":\"good\"}");
  let locked = repo.join("locked");
  write_file(&locked.join("package.json"), "{\"name\":\"locked\"}");
  let _unlock = Unlock(locked.clone());
  set_permissions(&locked, Permissions::from_mode(0o000)).expect("failed to lock directory");

  let workspace = Workspace::discover(&repo).expect("discovery should succeed");

  assert_eq!(package_paths(&workspace), vec!["good"]);
  assert_eq!(
    workspace.warnings(),
    [DiscoveryWarning::UnreadablePath {
      path: locked,
      kind: std::io::ErrorKind::PermissionDenied
    }]
  );
}

#[cfg(unix)]
#[test]
fn symlink_cycles_are_not_followed() {
  let temp = TempDir::new("symlink-cycle");
  let repo = temp.path().join("repo");
  create_dir_all(&repo).expect("failed to create repo dir");
  git(&repo, &["init"]);
  write_file(&repo.join("pkg/go.mod"), "module example.com/pkg\n");
  std::os::unix::fs::symlink("..", repo.join("pkg/back")).expect("failed to create symlink");

  let workspace = Workspace::discover(&repo).expect("discovery should succeed");

  assert_eq!(package_paths(&workspace), vec!["pkg"]);
  assert!(workspace.warnings().is_empty());
}

#[test]
fn worktree_whose_main_repository_is_gone_is_an_error() {
  let temp = TempDir::new("orphaned-worktree");
  let main = temp.path().join("main");
  create_dir_all(&main).expect("failed to create main dir");
  git(&main, &["init"]);
  git(&main, &["commit", "--allow-empty", "-m", "init"]);
  git(&main, &["worktree", "add", "../linked"]);
  let linked = temp.path().join("linked");
  remove_dir_all(&main).expect("failed to remove main repository");

  let err = Workspace::discover(&linked).expect_err("orphaned worktree should fail");

  assert!(
    matches!(err, Error::InvalidVcsPointer { ref path } if path == &linked.join(".git")),
    "unexpected error: {err:?}"
  );
}

#[test]
fn scan_yields_only_files_discovery_uses_and_honors_gitignore() {
  let temp = TempDir::new("scan-filter");
  let root = temp.path().join("repo");
  create_dir_all(&root).expect("failed to create repo dir");
  git(&root, &["init"]);
  write_file(&root.join(".gitignore"), "scratch/\n");
  write_file(&root.join("Cargo.toml"), "[package]\nname = \"a\"\n");
  write_file(&root.join("Gemfile"), "source \"https://rubygems.org\"\n");
  write_file(&root.join("src/main.rs"), "fn main() {}\n");
  write_file(&root.join("README.md"), "# readme\n");
  write_file(&root.join("notes.txt"), "notes\n");
  write_file(
    &root.join("scratch/Cargo.toml"),
    "[package]\nname = \"s\"\n",
  );

  let scan = Scan::run(&root);

  assert_eq!(
    scan.files_below(Path::new("")),
    [
      Path::new("Cargo.toml"),
      Path::new("Gemfile"),
      Path::new("src/main.rs")
    ]
  );
  assert!(scan.warnings.is_empty());
}
