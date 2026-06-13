use crate::commands::{self, NuShell};
use crate::expand::expand_path;
use crate::parser::GitConfig;
use crate::persistence::{SwissCache, FINGERPRINT_KEY};
use crate::plan::{git_clone_args, Plan, Step};
use crate::report::{self, Outcome, Reporter};
use crate::shellgen::upsert_block;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub struct Executor {
    cache: SwissCache,
}

impl Executor {
    pub fn new(cache: SwissCache) -> Self {
        Self { cache }
    }

    /// Runs every step, never aborting on the first failure: each is reported
    /// with a check or a cross, and the failures are tallied in the returned
    /// outcome. The manifest fingerprint is recorded only on a fully clean run.
    pub fn execute(&mut self, plan: &Plan, fingerprint: Option<&str>) -> Outcome {
        let total = plan
            .steps
            .iter()
            .filter(|step| !step.is_bookkeeping())
            .count();
        let mut reporter = Reporter::new(total);

        // Cache-only bookkeeping (RecordDep/RecordAliases) is skipped after its
        // preceding action failed, so a failed install never records the tool
        // as present.
        let mut last_action_failed = false;
        for step in &plan.steps {
            if step.is_bookkeeping() {
                if last_action_failed {
                    report::file_log(&format!("skip (previous step failed): {}", step));
                    continue;
                }
                if let Err(error) = self.execute_step(step) {
                    report::file_log(&format!("bookkeeping step failed: {}: {:#}", step, error));
                }
                continue;
            }

            let label = step.to_string();
            let ok = reporter.step(&label, || self.execute_step(step));
            last_action_failed = !ok;
        }

        let outcome = reporter.into_outcome();
        if outcome.succeeded() {
            if let Some(fingerprint) = fingerprint {
                self.cache.set(FINGERPRINT_KEY, fingerprint);
            }
        }
        outcome
    }

    fn execute_step(&mut self, step: &Step) -> Result<()> {
        match step {
            Step::Command {
                program, args, cwd, ..
            } => {
                commands::run_command(program, args, cwd.as_deref().and_then(Path::to_str))?;
                Ok(())
            }
            Step::ShellCommand {
                shell,
                command,
                cwd,
                ..
            } => run_shell_command(shell, command, cwd.as_deref()),
            Step::EnsureDir { path } => ensure_dir(path),
            Step::WriteFile {
                path,
                content,
                overwrite,
                append_if_missing,
                backup,
                requires_admin,
            } => write_file(
                path,
                content,
                *overwrite,
                *append_if_missing,
                *backup,
                *requires_admin,
            ),
            Step::GitSync {
                repo,
                branch,
                depth,
                recursive,
                dest,
            } => git_sync(
                &GitConfig {
                    repo: repo.clone(),
                    branch: branch.clone(),
                    depth: *depth,
                    recursive: *recursive,
                },
                dest,
            ),
            Step::PatchBlock {
                target,
                block,
                content,
            } => patch_block(target, block, content),
            Step::ConfigureNu => configure_nu(),
            Step::LinkNuBinaries => NuShell::link(),
            Step::RecordDep { dep } => {
                self.cache.set_dep(dep.name(), dep);
                Ok(())
            }
            Step::RecordAliases { aliases } => {
                self.cache.set_aliases(aliases);
                Ok(())
            }
        }
    }
}

fn run_shell_command(shell: &str, command: &str, cwd: Option<&Path>) -> Result<()> {
    let cwd = cwd.and_then(Path::to_str);
    match shell {
        "nu" | "nushell" => {
            NuShell::run(&["-c", command], cwd)?;
        }
        "sh" | "bash" | "zsh" => {
            commands::run_command(shell, ["-c", command], cwd)?;
        }
        "pwsh" | "powershell" => {
            commands::run_command(shell, ["-NoProfile", "-Command", command], cwd)?;
        }
        "cmd" => {
            commands::run_command("cmd", ["/C", command], cwd)?;
        }
        other => bail!("Unsupported command shell '{}'", other),
    }
    Ok(())
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        log::debug!("Creating directory: {}", path.display());
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn write_file(
    path: &Path,
    content: &[u8],
    overwrite: bool,
    append_if_missing: bool,
    backup: bool,
    requires_admin: bool,
) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    if requires_admin && !is_admin().unwrap_or(false) {
        return write_file_elevated(path, content, overwrite, append_if_missing, backup);
    }
    #[cfg(target_os = "windows")]
    let _ = requires_admin;

    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }

    if append_if_missing {
        let existing = if path.exists() {
            std::fs::read_to_string(path)?
        } else {
            String::new()
        };
        let addition = String::from_utf8_lossy(content);
        if existing.contains(addition.as_ref()) {
            log::debug!("Skipping append, content present: {}", path.display());
            return Ok(());
        }
        let mut updated = existing;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(&addition);
        std::fs::write(path, updated)?;
        return Ok(());
    }

    if path.exists() || path.is_symlink() {
        if !overwrite {
            log::debug!("Skipping existing file: {}", path.display());
            return Ok(());
        }
        if backup {
            let mut file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            file_name.push_str(".bak");
            let backup_path = path.with_file_name(file_name);
            std::fs::copy(path, &backup_path)
                .with_context(|| format!("Failed to back up {}", path.display()))?;
        }
        std::fs::remove_file(path)?;
    }

    log::debug!("Writing file: {}", path.display());
    std::fs::write(path, content)?;
    Ok(())
}

/// Write a file at a privileged destination (e.g. `/etc`) by staging the
/// content in a user-writable temp file and installing it through `sudo`.
#[cfg(not(target_os = "windows"))]
fn write_file_elevated(
    path: &Path,
    content: &[u8],
    overwrite: bool,
    append_if_missing: bool,
    backup: bool,
) -> Result<()> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 destination path: {}", path.display()))?;

    if let Some(parent) = path.parent() {
        if let Some(parent) = parent.to_str() {
            sudo(&["mkdir", "-p", parent])?;
        }
    }

    if append_if_missing {
        let existing = sudo_read(path_str)?.unwrap_or_default();
        if contains_slice(&existing, content) {
            log::debug!("Skipping append, content present: {}", path.display());
            return Ok(());
        }
        let mut merged = existing;
        if !merged.is_empty() && merged.last() != Some(&b'\n') {
            merged.push(b'\n');
        }
        merged.extend_from_slice(content);
        return sudo_install(&merged, path_str);
    }

    let exists = sudo_test(&["-e", path_str])? || sudo_test(&["-L", path_str])?;
    if exists {
        if !overwrite {
            log::debug!("Skipping existing file: {}", path.display());
            return Ok(());
        }
        if backup {
            sudo(&["cp", "-p", path_str, &format!("{}.bak", path_str)])
                .with_context(|| format!("Failed to back up {}", path.display()))?;
        }
        sudo(&["rm", "-f", path_str])?;
    }

    log::debug!("Writing file (elevated): {}", path.display());
    sudo_install(content, path_str)
}

/// Stage `content` to a temp file and `sudo cp` it onto `dest`.
#[cfg(not(target_os = "windows"))]
fn sudo_install(content: &[u8], dest: &str) -> Result<()> {
    let staged = stage_temp(content)?;
    let result = staged
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 temp path"))
        .and_then(|src| sudo(&["cp", src, dest]));
    let _ = std::fs::remove_file(&staged);
    result
}

/// Write `content` to a uniquely named file under the system temp directory.
#[cfg(not(target_os = "windows"))]
fn stage_temp(content: &[u8]) -> Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let mut staged = std::env::temp_dir();
    staged.push(format!("swiss-stage-{}-{}", std::process::id(), nanos));
    std::fs::write(&staged, content)
        .with_context(|| format!("Failed to stage temp file {}", staged.display()))?;
    Ok(staged)
}

#[cfg(not(target_os = "windows"))]
fn sudo(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("sudo")
        .args(args)
        .status()
        .with_context(|| format!("Failed to run: sudo {}", args.join(" ")))?;
    if !status.success() {
        bail!("Command failed: sudo {}", args.join(" "));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn sudo_test(args: &[&str]) -> Result<bool> {
    let status = std::process::Command::new("sudo")
        .arg("test")
        .args(args)
        .status()
        .with_context(|| format!("Failed to run: sudo test {}", args.join(" ")))?;
    Ok(status.success())
}

/// Read a privileged file's bytes via `sudo cat`, or `None` if it is absent.
#[cfg(not(target_os = "windows"))]
fn sudo_read(path: &str) -> Result<Option<Vec<u8>>> {
    if !sudo_test(&["-e", path])? {
        return Ok(None);
    }
    let output = std::process::Command::new("sudo")
        .args(["cat", path])
        .output()
        .with_context(|| format!("Failed to read {} as root", path))?;
    if !output.status.success() {
        bail!("Failed to read {} as root", path);
    }
    Ok(Some(output.stdout))
}

#[cfg(not(target_os = "windows"))]
fn contains_slice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn git_sync(git: &GitConfig, dest: &Path) -> Result<()> {
    if dest.join(".git").exists() {
        commands::run_command("git", ["pull", "--ff-only"], dest.to_str())?;
        if git.recursive {
            commands::run_command(
                "git",
                ["submodule", "update", "--init", "--recursive"],
                dest.to_str(),
            )?;
        }
        return Ok(());
    }

    if dest.exists() {
        bail!(
            "Custom dependency path exists but is not a git repository: {}",
            dest.display()
        );
    }

    if let Some(parent) = dest.parent() {
        ensure_dir(parent)?;
    }
    let args = git_clone_args(git, dest);
    commands::run_command("git", &args, dest.parent().and_then(Path::to_str))?;
    Ok(())
}

fn resolve_patch_target(target: &str) -> Result<PathBuf> {
    if target == "$PROFILE" {
        let profile = commands::run_command(
            "pwsh",
            ["-NoProfile", "-Command", "Write-Output $PROFILE"],
            None,
        )
        .context("Failed to resolve $PROFILE through pwsh")?;
        let profile = profile.trim().trim_matches('\r');
        if profile.is_empty() {
            bail!("pwsh returned an empty $PROFILE");
        }
        return Ok(PathBuf::from(profile));
    }
    let home = home::home_dir().context("Unable to obtain home directory")?;
    Ok(expand_path(target, &home))
}

fn patch_block(target: &str, block: &str, content: &str) -> Result<()> {
    let path = resolve_patch_target(target)?;
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let existing = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let updated = upsert_block(&existing, block, content);
    if updated != existing {
        std::fs::write(&path, updated)?;
        log::debug!("Patched block '{}' in {}", block, path.display());
    }
    Ok(())
}

/// Patches the bounded Swiss blocks into Nushell's env/config files: the env
/// hook regenerates `init.nu` at startup, the config hook sources it.
fn configure_nu() -> Result<()> {
    let env_path = NuShell::get_env_value("$nu.env-path")?;
    let conf_path = NuShell::get_env_value("$nu.config-path")?;

    patch_nu_file(env_path.trim(), &crate::shellgen::nu_env_hook())?;
    patch_nu_file(conf_path.trim(), &crate::shellgen::nu_config_hook())?;
    Ok(())
}

fn patch_nu_file(path: &str, content: &str) -> Result<()> {
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let updated = upsert_block(&existing, crate::shellgen::REGISTRATION_BLOCK, content);
    if updated != existing {
        std::fs::write(path, updated)?;
        log::debug!("Patched Swiss block in {}", path.display());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn is_admin() -> Result<bool> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)",
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().eq_ignore_ascii_case("true"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn is_admin() -> Result<bool> {
    let output = std::process::Command::new("id").arg("-u").output()?;
    let uid = String::from_utf8_lossy(&output.stdout);
    Ok(uid.trim() == "0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{Dependency, DependencyStatus, DependencyType};

    fn command_step(program: &str) -> Step {
        Step::Command {
            program: program.to_owned(),
            args: vec![],
            cwd: None,
            requires_admin: false,
        }
    }

    #[test]
    fn execute_runs_every_step_and_tallies_failures() {
        let mut executor = Executor::new(SwissCache::default());
        let plan = Plan {
            steps: vec![
                command_step("false"), // exits non-zero
                command_step("true"),  // must still run after the failure
            ],
        };

        let outcome = executor.execute(&plan, None);
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.completed, 1);
        assert!(!outcome.succeeded());
    }

    #[test]
    fn record_steps_are_skipped_after_a_failed_action() {
        let mut executor = Executor::new(SwissCache::default());
        let plan = Plan {
            steps: vec![
                command_step("false"),
                Step::RecordDep {
                    dep: Dependency::new(
                        "ghost".to_owned(),
                        None,
                        DependencyStatus::UpToDate,
                        DependencyType::Cargo,
                    ),
                },
            ],
        };

        let outcome = executor.execute(&plan, None);
        assert_eq!(outcome.failed, 1);
        // The install failed, so the dependency is never recorded as present.
        assert!(executor.cache.get_dep("ghost").is_none());
    }

    #[test]
    fn write_file_respects_overwrite_flag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        write_file(&path, b"first", false, false, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");

        // Existing file is preserved without overwrite.
        write_file(&path, b"second", false, false, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");

        write_file(&path, b"second", true, false, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn write_file_appends_only_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env.nu");

        write_file(&path, b"source swiss\n", false, true, false, false).unwrap();
        write_file(&path, b"source swiss\n", false, true, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "source swiss\n");

        write_file(&path, b"other line\n", false, true, false, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "source swiss\nother line\n"
        );
    }

    #[test]
    fn write_file_creates_backup_before_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");

        write_file(&path, b"original", false, false, false, false).unwrap();
        write_file(&path, b"updated", true, false, true, false).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "updated");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("settings.toml.bak")).unwrap(),
            "original"
        );
    }

    #[test]
    fn write_file_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/nested/file.txt");

        write_file(&path, b"data", false, false, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "data");
    }
}
