use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::ReleaseAction;
use crate::ui;
use crate::version::{self, BumpKind};
use semver::Version;

type SemverResult = Result<semver::Version>;

fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("finding git root")?;
    if !output.status.success() {
        bail!("not inside a git repository");
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(root))
}

fn version_file() -> Result<PathBuf> {
    let root = repo_root()?;
    let candidate = root.join("VERSION");
    if candidate.exists() {
        return Ok(candidate);
    }
    bail!("VERSION file not found");
}

fn sync_cargo_toml(root: &Path, new_ver: &Version) -> Result<()> {
    let cargo_path = root.join("Cargo.toml");
    if !cargo_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&cargo_path);
    let content = content.context("reading Cargo.toml")?;
    let mut in_package = false;
    let mut replaced = false;
    let new_content: String = content
        .lines()
        .map(|line| {
            if line.trim() == "[package]" {
                in_package = true;
            } else if line.starts_with('[') {
                in_package = false;
            }

            let is_version_line = line.trim_start().starts_with("version");
            if in_package && !replaced && is_version_line {
                if let Some(eq_pos) = line.find('=') {
                    replaced = true;
                    return format!("{}= \"{new_ver}\"", &line[..eq_pos]);
                }
            }

            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let content_has_newline = content.ends_with('\n');
    let output_has_newline = new_content.ends_with('\n');
    let needs_trailing_newline = content_has_newline && !output_has_newline;
    let new_content = if needs_trailing_newline {
        format!("{new_content}\n")
    } else {
        new_content
    };

    let write = std::fs::write(&cargo_path, new_content);
    write.context("writing Cargo.toml")?;
    Ok(())
}

fn action_to_bump(action: &ReleaseAction) -> (Option<BumpKind>, Option<&str>) {
    match action {
        ReleaseAction::Patch => (Some(BumpKind::Patch), None),
        ReleaseAction::Minor => (Some(BumpKind::Minor), None),
        ReleaseAction::Major => (Some(BumpKind::Major), None),
        ReleaseAction::Set { version } => (None, Some(version.as_str())),
    }
}

fn next_version(current: &Version, action: &ReleaseAction) -> SemverResult {
    let (bump_kind, explicit) = action_to_bump(action);

    let new_ver = if let Some(kind) = bump_kind {
        version::bump(current, kind)
    } else {
        let raw = explicit.context("set requires a version string")?;
        let sem = version::extract_semver(raw);
        let sem = sem.context("invalid semver in provided version")?;
        semver::Version::parse(&sem)?
    };

    Ok(new_ver)
}

fn write_version_files(
    root: &Path,
    version_path: &Path,
    current: &Version,
    new_ver: &Version,
) -> Result<()> {
    version::write_to_file(version_path, new_ver)?;
    sync_cargo_toml(root, new_ver)?;
    ui::ok(&format!("VERSION: {current} -> {new_ver}"));
    Ok(())
}

fn git_cmd(args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .status()
        .with_context(|| format!("git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}

fn git_output(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ensure_clean_working_tree() -> Result<()> {
    let dirty = git_output(&["status", "--porcelain"])?;
    if !dirty.is_empty() {
        bail!("working tree is not clean - commit or stash first");
    }
    Ok(())
}

fn current_branch() -> Result<String> {
    git_output(&["rev-parse", "--abbrev-ref", "HEAD"])
}

fn ensure_on_main() -> Result<()> {
    let branch = current_branch()?;
    if branch != "main" {
        bail!("release must start from main (current: {branch})");
    }
    Ok(())
}

fn ensure_main_in_sync_with_origin() -> Result<()> {
    git_cmd(&["fetch", "origin", "main", "--tags"])?;

    let head = git_output(&["rev-parse", "HEAD"])?;
    let remote = git_output(&["rev-parse", "refs/remotes/origin/main"])?;

    if head != remote {
        bail!("local main is not up to date with origin/main");
    }
    Ok(())
}

fn ensure_gh_available() -> Result<()> {
    let status = Command::new("gh")
        .arg("--version")
        .status()
        .context("checking gh availability")?;
    if !status.success() {
        bail!("gh CLI is required to open the release PR");
    }

    let auth = Command::new("gh")
        .args(["auth", "status"])
        .status()
        .context("checking gh authentication")?;
    if !auth.success() {
        bail!("gh CLI is not authenticated; run `gh auth login`");
    }
    Ok(())
}

fn ensure_ref_does_not_exist(ref_name: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", ref_name])
        .status()
        .with_context(|| format!("checking {ref_name}"))?;
    if status.success() {
        bail!("git ref already exists: {ref_name}");
    }
    Ok(())
}

fn ensure_remote_ref_does_not_exist(kind: &str, name: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["ls-remote", "--exit-code", kind, "origin", name])
        .output()
        .with_context(|| format!("checking remote {kind} {name}"))?;

    if output.status.success() {
        bail!("remote git ref already exists: {name}");
    }
    Ok(())
}

fn ensure_release_refs_available(tag: &str, branch: &str) -> Result<()> {
    ensure_ref_does_not_exist(&format!("refs/tags/{tag}"))?;
    ensure_ref_does_not_exist(&format!("refs/heads/{branch}"))?;
    ensure_remote_ref_does_not_exist("--tags", tag)?;
    ensure_remote_ref_does_not_exist("--heads", branch)?;
    Ok(())
}

fn ensure_release_preconditions() -> Result<()> {
    ensure_clean_working_tree()?;
    ensure_on_main()?;
    ensure_main_in_sync_with_origin()?;
    ensure_gh_available()?;
    Ok(())
}

pub fn run(action: &ReleaseAction) -> Result<()> {
    ensure_release_preconditions()?;

    let root = repo_root()?;
    let version_path = version_file()?;
    let current = version::read_from_file(&version_path)?;
    let new_ver = next_version(&current, action)?;
    let tag = format!("v{new_ver}");
    let branch = format!("release/{tag}");

    ensure_release_refs_available(&tag, &branch)?;
    write_version_files(&root, &version_path, &current, &new_ver)?;

    git_cmd(&["checkout", "-b", &branch])?;
    git_cmd(&["add", "VERSION", "Cargo.toml"])?;

    let msg = format!("release: {tag}");
    git_cmd(&["commit", "-m", &msg])?;
    git_cmd(&["push", "-u", "origin", &branch])?;

    let pr_body = format!(
        "Bump version to {new_ver}.\n\n{}",
        "Merging triggers the verified release workflow.",
    );
    let pr = Command::new("gh")
        .args([
            "pr",
            "create",
            "--title",
            &format!("release: {tag}"),
            "--body",
            &pr_body,
            "--base",
            "main",
            "--head",
            &branch,
        ])
        .output()
        .context("gh pr create")?;
    if !pr.status.success() {
        let stderr = String::from_utf8_lossy(&pr.stderr);
        bail!("gh pr create failed: {stderr}");
    }

    let pr_url = String::from_utf8_lossy(&pr.stdout).trim().to_string();
    ui::ok(&format!("opened release PR: {pr_url}"));
    Ok(())
}

pub fn run_publish(_action: &ReleaseAction) -> Result<()> {
    bail!(concat!(
        "release-publish is deprecated; use `just release <bump>` ",
        "and merge the generated PR so GitHub Actions tags, releases, ",
        "and publishes"
    ))
}
