use crate::commands::{self, NuShell};
use crate::expand::expand_path;
use crate::parser::GitConfig;
use crate::persistence::{SwissCache, FINGERPRINT_KEY, NU_CONF_LOADER, NU_ENV_LOADER};
use crate::plan::{git_clone_args, Plan, Step};
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

    pub fn execute(&mut self, plan: &Plan, fingerprint: Option<&str>) -> Result<()> {
        let total = plan.steps.len();
        for (index, step) in plan.steps.iter().enumerate() {
            log::info!("[{}/{}] {}", index + 1, total, step);
            self.execute_step(step)
                .with_context(|| format!("Step {}/{} failed: {}", index + 1, total, step))?;
        }
        if let Some(fingerprint) = fingerprint {
            self.cache.set(FINGERPRINT_KEY, fingerprint);
        }
        Ok(())
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
            } => write_file(path, content, *overwrite, *append_if_missing, *backup),
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
) -> Result<()> {
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

fn configure_nu() -> Result<()> {
    let env_path = NuShell::get_env_value("$nu.env-path")?;
    let conf_path = NuShell::get_env_value("$nu.config-path")?;

    let mut env_data = std::fs::read_to_string(&env_path)?;
    if !env_data.contains(NU_ENV_LOADER) {
        env_data.push_str(
            format!(
                "\n# Swiss environment loader\n{}\n# Swiss environment loader end line\n",
                NU_ENV_LOADER
            )
            .as_str(),
        );
        std::fs::write(&env_path, env_data)?;
        log::debug!("Added {} to {}", NU_ENV_LOADER, env_path);
    }

    let conf_data = std::fs::read_to_string(&conf_path)?;
    if !conf_data.contains(NU_CONF_LOADER) {
        let mut conf_data = conf_data;
        conf_data.push_str(
            format!(
                "\n# Swiss configuration loader\n{}\n# Swiss configuration loader end line\n",
                NU_CONF_LOADER
            )
            .as_str(),
        );
        std::fs::write(&conf_path, conf_data)?;
        log::debug!("Added {} to {}", NU_CONF_LOADER, conf_path);
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

    #[test]
    fn write_file_respects_overwrite_flag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        write_file(&path, b"first", false, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");

        // Existing file is preserved without overwrite.
        write_file(&path, b"second", false, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");

        write_file(&path, b"second", true, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn write_file_appends_only_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env.nu");

        write_file(&path, b"source swiss\n", false, true, false).unwrap();
        write_file(&path, b"source swiss\n", false, true, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "source swiss\n");

        write_file(&path, b"other line\n", false, true, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "source swiss\nother line\n"
        );
    }

    #[test]
    fn write_file_creates_backup_before_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");

        write_file(&path, b"original", false, false, false).unwrap();
        write_file(&path, b"updated", true, false, true).unwrap();

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

        write_file(&path, b"data", false, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "data");
    }
}
