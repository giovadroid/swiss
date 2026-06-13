use crate::parser::{EnvValue, Os, ShellMode, ShellModuleSpec, ShellTargetConfig, SwissConfig};
use crate::plan::{PlanContext, Step};
use anyhow::{bail, Result};
use std::fmt::Write as _;

pub const BLOCK_BEGIN: &str = "# swiss begin:";
pub const BLOCK_END: &str = "# swiss end:";

/// Name of the single bounded block Swiss maintains in startup files.
pub const REGISTRATION_BLOCK: &str = "init";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Nushell,
    Zsh,
    Bash,
    Pwsh,
}

impl ShellKind {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "nushell" | "nu" => Some(ShellKind::Nushell),
            "zsh" => Some(ShellKind::Zsh),
            "bash" => Some(ShellKind::Bash),
            "pwsh" | "powershell" => Some(ShellKind::Pwsh),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ShellKind::Nushell => "nushell",
            ShellKind::Zsh => "zsh",
            ShellKind::Bash => "bash",
            ShellKind::Pwsh => "pwsh",
        }
    }

    fn default_mode(&self) -> ShellMode {
        match self {
            ShellKind::Nushell => ShellMode::ManagedLoader,
            ShellKind::Pwsh => ShellMode::Profile,
            _ => ShellMode::Snippet,
        }
    }

    pub fn default_target(&self) -> Option<&'static str> {
        match self {
            ShellKind::Zsh => Some("~/.zshrc"),
            ShellKind::Bash => Some("~/.bashrc"),
            ShellKind::Pwsh => Some("$PROFILE"),
            ShellKind::Nushell => None,
        }
    }

    /// The block registered in the startup file: it puts cargo bin on PATH
    /// and evaluates `swiss init` output, guarded so that removing Swiss
    /// never breaks the shell. Nushell cannot evaluate dynamic strings, so it
    /// uses the env/config hooks instead (see `nu_env_hook`).
    pub fn registration_line(&self) -> Option<String> {
        Some(match self {
            ShellKind::Zsh | ShellKind::Bash => format!(
                "export PATH=\"$HOME/.cargo/bin:$PATH\"\n\
                 command -v swiss >/dev/null 2>&1 && eval \"$(swiss init --shell {})\"",
                self.name()
            ),
            ShellKind::Pwsh => {
                "$env:PATH = \"$HOME/.cargo/bin\" + [IO.Path]::PathSeparator + $env:PATH\n\
                 if (Get-Command swiss -ErrorAction SilentlyContinue) { swiss init --shell pwsh | Out-String | Invoke-Expression }"
                    .to_owned()
            }
            ShellKind::Nushell => return None,
        })
    }
}

/// Nushell init file (within `$HOME`) regenerated at every shell startup by
/// the env hook and sourced by the config hook. Nushell parses `source`
/// targets before running, hence the on-disk indirection instead of `eval`.
pub const NU_INIT_RELATIVE: &str = ".config/swiss/init.nu";

/// Block patched into `$nu.env-path`: env.nu runs before config.nu, so the
/// init file always exists (and is fresh) by the time it gets sourced.
pub fn nu_env_hook() -> String {
    "$env.PATH = ($env.PATH | prepend ($nu.home-path | path join \".cargo\" \"bin\"))\n\
     try { ^swiss init --shell nushell | save -f ($nu.home-path | path join \".config\" \"swiss\" \"init.nu\") }"
        .to_owned()
}

/// Block patched into `$nu.config-path`.
pub fn nu_config_hook() -> String {
    format!("source ~/{}", NU_INIT_RELATIVE)
}

/// Wraps content in a bounded, idempotent block.
pub fn render_block(name: &str, content: &str) -> String {
    format!(
        "{} {}\n{}\n{} {}\n",
        BLOCK_BEGIN,
        name,
        content.trim_end(),
        BLOCK_END,
        name
    )
}

/// Replaces an existing bounded block or appends a new one. Re-applying the
/// same content is a no-op, so generated files stay stable.
pub fn upsert_block(existing: &str, name: &str, content: &str) -> String {
    let begin_marker = format!("{} {}", BLOCK_BEGIN, name);
    let end_marker = format!("{} {}", BLOCK_END, name);
    let block = render_block(name, content);

    if let Some(begin) = existing.find(&begin_marker) {
        if let Some(end_start) = existing[begin..].find(&end_marker) {
            let end = begin
                + end_start
                + existing[begin + end_start..]
                    .find('\n')
                    .map(|offset| end_start + offset + 1 - end_start)
                    .unwrap_or(existing.len() - begin - end_start);
            let mut output = String::with_capacity(existing.len() + block.len());
            output.push_str(&existing[..begin]);
            output.push_str(&block);
            output.push_str(&existing[end..]);
            return output;
        }
    }

    let mut output = existing.to_owned();
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(&block);
    output
}

/// `~/x` becomes `$HOME/x`; both POSIX shells and PowerShell expand `$HOME`.
fn home_path_entry(entry: &str) -> String {
    if let Some(rest) = entry.strip_prefix("~/") {
        format!("$HOME/{}", rest)
    } else {
        entry.to_owned()
    }
}

/// Renders the body of one module for a POSIX-ish shell or PowerShell.
/// Returns `None` when the module contributes nothing for that shell.
pub fn render_module(kind: ShellKind, spec: &ShellModuleSpec) -> Option<String> {
    let mut body = String::new();

    for (name, value) in &spec.env {
        match (kind, value) {
            (ShellKind::Zsh | ShellKind::Bash, EnvValue::Plain(plain)) => {
                let _ = writeln!(body, "export {}=\"{}\"", name, plain);
            }
            (ShellKind::Zsh | ShellKind::Bash, EnvValue::PathOps { prepend, append }) => {
                if !prepend.is_empty() {
                    let joined: Vec<String> =
                        prepend.iter().map(|entry| home_path_entry(entry)).collect();
                    let _ = writeln!(body, "export {}=\"{}:${}\"", name, joined.join(":"), name);
                }
                if !append.is_empty() {
                    let joined: Vec<String> =
                        append.iter().map(|entry| home_path_entry(entry)).collect();
                    let _ = writeln!(body, "export {}=\"${}:{}\"", name, name, joined.join(":"));
                }
            }
            (ShellKind::Pwsh, EnvValue::Plain(plain)) => {
                let _ = writeln!(body, "$env:{} = \"{}\"", name, plain);
            }
            (ShellKind::Pwsh, EnvValue::PathOps { prepend, append }) => {
                for entry in prepend.iter().rev() {
                    let _ = writeln!(
                        body,
                        "$env:{} = \"{}\" + [IO.Path]::PathSeparator + $env:{}",
                        name,
                        home_path_entry(entry),
                        name
                    );
                }
                for entry in append {
                    let _ = writeln!(
                        body,
                        "$env:{} = $env:{} + [IO.Path]::PathSeparator + \"{}\"",
                        name,
                        name,
                        home_path_entry(entry)
                    );
                }
            }
            (ShellKind::Nushell, EnvValue::Plain(plain)) => {
                let _ = writeln!(body, "$env.{} = \"{}\"", name, plain);
            }
            (ShellKind::Nushell, EnvValue::PathOps { prepend, append }) => {
                if !prepend.is_empty() {
                    let entries: Vec<String> = prepend
                        .iter()
                        .map(|entry| format!("\"{}\"", entry))
                        .collect();
                    let _ = writeln!(
                        body,
                        "$env.{} = ($env.{} | prepend [{}])",
                        name,
                        name,
                        entries.join(", ")
                    );
                }
                if !append.is_empty() {
                    let entries: Vec<String> = append
                        .iter()
                        .map(|entry| format!("\"{}\"", entry))
                        .collect();
                    let _ = writeln!(
                        body,
                        "$env.{} = ($env.{} | append [{}])",
                        name,
                        name,
                        entries.join(", ")
                    );
                }
            }
        }
    }

    for (alias, command) in &spec.aliases {
        match kind {
            ShellKind::Zsh | ShellKind::Bash => {
                let _ = writeln!(body, "alias {}='{}'", alias, command);
            }
            ShellKind::Pwsh => {
                let _ = writeln!(body, "function {} {{ {} @args }}", alias, command);
            }
            ShellKind::Nushell => {
                let _ = writeln!(body, "alias {} = {}", alias, command);
            }
        }
    }

    if let Some(init) = spec.init.get(kind.name()) {
        let _ = writeln!(body, "{}", init.trim_end());
    }

    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

fn resolve_modules<'a>(
    config: &'a SwissConfig,
    names: &'a [String],
    shell: &str,
) -> Result<Vec<(&'a String, &'a ShellModuleSpec)>> {
    let mut resolved = Vec::new();
    for name in names {
        match config.shell_modules.get(name) {
            Some(spec) => resolved.push((name, spec)),
            None => bail!(
                "Shell '{}' references unknown shell module '{}'",
                shell,
                name
            ),
        }
    }
    Ok(resolved)
}

/// System package that provides a shell, per OS. `None` means Swiss cannot
/// install it automatically there.
fn shell_system_package(kind: ShellKind, os: Os) -> Option<(&'static str, bool)> {
    // (package name, needs --cask for brew)
    match (kind, os) {
        (ShellKind::Zsh, Os::Linux) | (ShellKind::Zsh, Os::Macos) => Some(("zsh", false)),
        (ShellKind::Bash, Os::Linux) | (ShellKind::Bash, Os::Macos) => Some(("bash", false)),
        (ShellKind::Pwsh, Os::Macos) => Some(("powershell", true)),
        _ => None,
    }
}

/// Installs the shells the manifest enables but the host is missing, before
/// any step configures them. Nushell is handled by the `nushell` section; here
/// it only fails fast when neither that section nor an installed nu exist.
pub fn shell_package_steps(config: &SwissConfig, context: &PlanContext) -> Result<Vec<Step>> {
    let mut steps = Vec::new();

    for (shell_name, target) in &config.shells {
        if !target.enabled {
            continue;
        }
        let Some(kind) = ShellKind::from_name(shell_name) else {
            continue; // shell_steps reports the unsupported name
        };
        if target.mode.unwrap_or_else(|| kind.default_mode()) == ShellMode::Print {
            continue;
        }

        if kind == ShellKind::Nushell {
            // Installed like any other dependency, through the cargo phase.
            continue;
        }

        if context.available_shells.contains(kind.name()) {
            continue;
        }

        match (shell_system_package(kind, context.os), context.os) {
            (Some((package, _)), Os::Linux) => steps.push(Step::Command {
                program: "sudo".to_owned(),
                args: vec![
                    "apt-get".to_owned(),
                    "install".to_owned(),
                    "-y".to_owned(),
                    package.to_owned(),
                ],
                cwd: None,
                requires_admin: true,
            }),
            (Some((package, cask)), Os::Macos) => {
                let mut args = vec!["install".to_owned()];
                if cask {
                    args.push("--cask".to_owned());
                }
                args.push(package.to_owned());
                steps.push(Step::Command {
                    program: "brew".to_owned(),
                    args,
                    cwd: None,
                    requires_admin: false,
                });
            }
            _ => log::warn!(
                "Shell '{}' is enabled in the manifest but not installed, and Swiss \
                 cannot install it automatically on {}; install it manually",
                kind.name(),
                context.os.name()
            ),
        }
    }

    Ok(steps)
}

/// Generates the plan steps for every enabled shell in the manifest.
pub fn shell_steps(config: &SwissConfig, context: &PlanContext) -> Result<Vec<Step>> {
    let mut steps = Vec::new();

    for (shell_name, target) in &config.shells {
        if !target.enabled {
            continue;
        }
        let Some(kind) = ShellKind::from_name(shell_name) else {
            bail!("Unsupported shell '{}' in manifest", shell_name);
        };
        let mode = target.mode.unwrap_or_else(|| kind.default_mode());

        // Validate module references at plan time so a broken manifest fails
        // here, never at shell startup.
        resolve_modules(config, &target.modules, shell_name)?;

        match mode {
            ShellMode::Print => continue,
            ShellMode::ManagedLoader => {
                if kind != ShellKind::Nushell {
                    bail!(
                        "Shell '{}' does not support managed-loader mode (only nushell)",
                        shell_name
                    );
                }
                steps.extend(nushell_registration_steps(context));
            }
            ShellMode::Snippet | ShellMode::Profile => {
                if kind == ShellKind::Nushell {
                    bail!("Shell 'nushell' uses managed-loader mode (it cannot eval snippets)");
                }
                steps.push(registration_step(kind, target)?);
            }
        }
    }

    Ok(steps)
}

/// Snippet/profile shells get one bounded registration block in the startup
/// file; the actual init code is rendered live by `swiss init --shell <name>`.
fn registration_step(kind: ShellKind, target: &ShellTargetConfig) -> Result<Step> {
    let target_file = target
        .target
        .clone()
        .or_else(|| kind.default_target().map(str::to_owned))
        .ok_or_else(|| anyhow::anyhow!("Shell '{}' needs an explicit 'target'", kind.name()))?;
    Ok(Step::PatchBlock {
        target: target_file,
        block: REGISTRATION_BLOCK.to_owned(),
        content: kind
            .registration_line()
            .expect("snippet shells always have a registration line"),
    })
}

/// Nushell registration: a placeholder init file (so `source` never fails)
/// plus the env/config hooks that regenerate and load it at startup. Also
/// used by `swiss setup` when the manifest does not mention nushell.
pub fn nushell_registration_steps(context: &PlanContext) -> Vec<Step> {
    vec![
        Step::WriteFile {
            path: context.home.join(NU_INIT_RELATIVE),
            content: b"# Generated by swiss at shell startup\n".to_vec(),
            overwrite: false,
            append_if_missing: false,
            backup: false,
            requires_admin: false,
        },
        Step::ConfigureNu,
    ]
}

/// Full init script for a shell, rendered at `swiss init` time: SWISS
/// metadata, every module as a bounded block, and the aliases recorded in the
/// cache by installed dependencies.
pub fn render_init_script(
    config: &SwissConfig,
    kind: ShellKind,
    modules: &[String],
    cached_aliases: &std::collections::BTreeMap<String, String>,
) -> Result<String> {
    let mut content = format!("# Generated by swiss init --shell {}\n", kind.name());
    let version_line = match kind {
        ShellKind::Nushell => format!("$env.SWISS_VERSION = \"{}\"", crate::persistence::VERSION),
        ShellKind::Pwsh => format!("$env:SWISS_VERSION = \"{}\"", crate::persistence::VERSION),
        _ => format!("export SWISS_VERSION=\"{}\"", crate::persistence::VERSION),
    };
    content.push_str(&version_line);
    content.push('\n');

    for (module_name, spec) in resolve_modules(config, modules, kind.name())? {
        if let Some(body) = render_module(kind, spec) {
            content.push('\n');
            content.push_str(&render_block(module_name, &body));
        }
    }

    if !cached_aliases.is_empty() {
        let spec = ShellModuleSpec {
            aliases: cached_aliases.clone(),
            ..ShellModuleSpec::default()
        };
        if let Some(body) = render_module(kind, &spec) {
            content.push('\n');
            content.push_str(&render_block("swiss-aliases", &body));
        }
    }

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::SwissCache;
    use indoc::indoc;
    use std::path::PathBuf;

    fn shell_config() -> SwissConfig {
        SwissConfig::parse(indoc! {r#"
            shells:
              nushell:
                mode: managed-loader
                modules: [path, starship, zoxide]
              zsh:
                mode: snippet
                modules: [path, starship, aliases]
              bash:
                enabled: false
                modules: [path]
              pwsh:
                mode: profile
                modules: [path]
            shell_modules:
              path:
                env:
                  PATH:
                    prepend: ["~/.cargo/bin", "~/.local/bin"]
              starship:
                package: starship
                init:
                  nushell: "mkdir ~/.cache/starship; starship init nu | save -f ~/.cache/starship/init.nu"
                  zsh: eval "$(starship init zsh)"
              zoxide:
                aliases:
                  cdi: "__zoxide_zi"
              aliases:
                aliases:
                  ll: "ls -la"
        "#})
        .unwrap()
    }

    fn test_context() -> PlanContext {
        PlanContext {
            os: crate::parser::Os::Linux,
            home: PathBuf::from("/home/tester"),
            base_dir: PathBuf::from("/manifests"),
            cache: SwissCache::default(),
            nu_installed: false,
            nu_version: None,
            cargo_installed: true,
            binstall_installed: true,
            available_shells: ["zsh", "bash", "pwsh"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }

    #[test]
    fn upsert_block_appends_then_replaces_idempotently() {
        let first = upsert_block("# my zshrc\n", "starship", "eval one");
        assert!(first.contains("# swiss begin: starship"));
        assert!(first.contains("eval one"));
        assert!(first.starts_with("# my zshrc\n"));

        let second = upsert_block(&first, "starship", "eval two");
        assert!(second.contains("eval two"));
        assert!(!second.contains("eval one"));
        assert_eq!(second.matches("# swiss begin: starship").count(), 1);

        // Re-applying identical content changes nothing.
        let third = upsert_block(&second, "starship", "eval two");
        assert_eq!(second, third);
    }

    #[test]
    fn upsert_block_keeps_other_blocks_untouched() {
        let mut content = upsert_block("", "path", "export PATH=x");
        content = upsert_block(&content, "starship", "eval s");
        let updated = upsert_block(&content, "path", "export PATH=y");
        assert!(updated.contains("export PATH=y"));
        assert!(updated.contains("eval s"));
        assert!(!updated.contains("export PATH=x"));
    }

    #[test]
    fn zsh_module_renders_path_aliases_and_init() {
        let config = shell_config();
        let path = render_module(ShellKind::Zsh, &config.shell_modules["path"]).unwrap();
        assert_eq!(
            path.trim(),
            "export PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:$PATH\""
        );

        let starship = render_module(ShellKind::Zsh, &config.shell_modules["starship"]).unwrap();
        assert_eq!(starship.trim(), "eval \"$(starship init zsh)\"");

        let aliases = render_module(ShellKind::Zsh, &config.shell_modules["aliases"]).unwrap();
        assert_eq!(aliases.trim(), "alias ll='ls -la'");
    }

    #[test]
    fn pwsh_module_renders_env_and_functions() {
        let config = shell_config();
        let path = render_module(ShellKind::Pwsh, &config.shell_modules["path"]).unwrap();
        assert!(path
            .contains("$env:PATH = \"$HOME/.cargo/bin\" + [IO.Path]::PathSeparator + $env:PATH"));

        let aliases = render_module(ShellKind::Pwsh, &config.shell_modules["aliases"]).unwrap();
        assert_eq!(aliases.trim(), "function ll { ls -la @args }");
    }

    #[test]
    fn nushell_module_renders_path_pipeline() {
        let config = shell_config();
        let path = render_module(ShellKind::Nushell, &config.shell_modules["path"]).unwrap();
        assert_eq!(
            path.trim(),
            "$env.PATH = ($env.PATH | prepend [\"~/.cargo/bin\", \"~/.local/bin\"])"
        );
    }

    #[test]
    fn snippet_shells_get_one_registration_block_only() {
        let config = shell_config();
        let steps = shell_steps(&config, &test_context()).unwrap();

        // bash is disabled: no patch against ~/.bashrc
        assert!(!steps.iter().any(|step| matches!(
            step,
            Step::PatchBlock { target, .. } if target == "~/.bashrc"
        )));

        // No init files are written anymore: content is rendered live by
        // `swiss init`, only the registration block lands on disk.
        assert!(!steps.iter().any(|step| matches!(
            step,
            Step::WriteFile { path, .. }
                if path.extension().map(|ext| ext != "nu").unwrap_or(true)
        )));

        // zsh: a single bounded block in ~/.zshrc that evals `swiss init`.
        let zshrc_patches: Vec<&Step> = steps
            .iter()
            .filter(|step| matches!(step, Step::PatchBlock { target, .. } if target == "~/.zshrc"))
            .collect();
        assert_eq!(zshrc_patches.len(), 1);
        match zshrc_patches[0] {
            Step::PatchBlock { block, content, .. } => {
                assert_eq!(block, REGISTRATION_BLOCK);
                assert!(content.contains("eval \"$(swiss init --shell zsh)\""));
                // cargo bin first, so a fresh shell finds the swiss binary.
                assert!(content.find(".cargo/bin").unwrap() < content.find("swiss init").unwrap());
            }
            _ => unreachable!(),
        }

        // pwsh defaults to $PROFILE.
        assert!(steps.iter().any(|step| matches!(
            step,
            Step::PatchBlock { target, content, .. }
                if target == "$PROFILE" && content.contains("swiss init --shell pwsh")
        )));
    }

    #[test]
    fn nushell_registration_writes_placeholder_and_configures_nu() {
        let config = shell_config();
        let steps = shell_steps(&config, &test_context()).unwrap();

        // Placeholder init.nu so config.nu's `source` never fails, even
        // before the first startup regenerates it.
        assert!(steps.iter().any(|step| matches!(
            step,
            Step::WriteFile { path, overwrite: false, .. }
                if path.ends_with(".config/swiss/init.nu")
        )));
        assert!(steps.contains(&Step::ConfigureNu));

        // The env hook regenerates the init file before config.nu sources it.
        let env_hook = nu_env_hook();
        assert!(env_hook.find(".cargo").unwrap() < env_hook.find("swiss init").unwrap());
        assert!(env_hook.contains("swiss init --shell nushell"));
        assert!(nu_config_hook().contains("source ~/.config/swiss/init.nu"));
    }

    #[test]
    fn nushell_snippet_mode_is_rejected() {
        let config = SwissConfig::parse(indoc! {"
            shells:
              nushell:
                mode: snippet
        "})
        .unwrap();
        let error = shell_steps(&config, &test_context()).unwrap_err();
        assert!(error.to_string().contains("managed-loader"));
    }

    #[test]
    fn render_init_script_includes_modules_and_cached_aliases() {
        let config = shell_config();
        let aliases: std::collections::BTreeMap<String, String> =
            [("cdi".to_owned(), "__zoxide_zi".to_owned())]
                .into_iter()
                .collect();

        let script = render_init_script(
            &config,
            ShellKind::Zsh,
            &["path".to_owned(), "starship".to_owned()],
            &aliases,
        )
        .unwrap();
        assert!(script.starts_with("# Generated by swiss init --shell zsh"));
        assert!(script.contains("export SWISS_VERSION="));
        assert!(script.contains("export PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:$PATH\""));
        assert!(script.contains("eval \"$(starship init zsh)\""));
        assert!(script.contains("# swiss begin: swiss-aliases"));
        assert!(script.contains("alias cdi='__zoxide_zi'"));

        // Same data renders with nushell syntax for nu.
        let script =
            render_init_script(&config, ShellKind::Nushell, &["path".to_owned()], &aliases)
                .unwrap();
        assert!(script.contains("$env.SWISS_VERSION = "));
        assert!(script.contains("$env.PATH = ($env.PATH | prepend"));
        assert!(script.contains("alias cdi = __zoxide_zi"));

        // No registered manifest at all still yields a valid script.
        let script = render_init_script(
            &SwissConfig::default(),
            ShellKind::Bash,
            &[],
            &Default::default(),
        )
        .unwrap();
        assert!(script.contains("export SWISS_VERSION="));
    }

    #[test]
    fn service_host_without_shells_generates_no_shell_steps() {
        let config = SwissConfig::parse("dependencies:\n  cargo:\n    ripgrep:\n").unwrap();
        let steps = shell_steps(&config, &test_context()).unwrap();
        assert!(steps.is_empty());
    }

    #[test]
    fn unknown_module_reference_fails() {
        let config = SwissConfig::parse(indoc! {"
            shells:
              zsh:
                modules: [ghost]
        "})
        .unwrap();
        let error = shell_steps(&config, &test_context()).unwrap_err();
        assert!(error.to_string().contains("ghost"));
    }

    #[test]
    fn missing_enabled_zsh_is_installed_before_configuration() {
        let config = shell_config();
        let mut context = test_context();
        context.nu_installed = true;
        context.available_shells.clear();

        let steps = shell_package_steps(&config, &context).unwrap();
        // zsh and pwsh are enabled; bash is disabled. On Linux only zsh has an
        // automatic install mapping (pwsh just warns).
        assert_eq!(
            steps,
            vec![Step::Command {
                program: "sudo".to_owned(),
                args: vec![
                    "apt-get".to_owned(),
                    "install".to_owned(),
                    "-y".to_owned(),
                    "zsh".to_owned(),
                ],
                cwd: None,
                requires_admin: true,
            }]
        );
    }

    #[test]
    fn present_shells_are_not_reinstalled() {
        let config = shell_config();
        let mut context = test_context();
        context.nu_installed = true;
        let steps = shell_package_steps(&config, &context).unwrap();
        assert!(steps.is_empty());
    }

    #[test]
    fn nushell_never_installs_through_the_system_package_manager() {
        // nu is handled by the cargo/binstall phase, not by apt/brew.
        let config = SwissConfig::parse(indoc! {"
            shells:
              nushell:
                mode: managed-loader
        "})
        .unwrap();
        assert!(shell_package_steps(&config, &test_context())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn registration_lines_are_safe_when_swiss_is_missing() {
        assert!(ShellKind::Zsh
            .registration_line()
            .unwrap()
            .contains("command -v swiss >/dev/null"));
        assert!(ShellKind::Pwsh
            .registration_line()
            .unwrap()
            .contains("Get-Command swiss -ErrorAction SilentlyContinue"));
        assert!(ShellKind::Nushell.registration_line().is_none());
        assert!(nu_env_hook().contains("try {"));
    }
}
