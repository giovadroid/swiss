use crate::parser::{EnvValue, ShellMode, ShellModuleSpec, ShellTargetConfig, SwissConfig};
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

    /// Generated init file under `~/.config/swiss`, printed by
    /// `swiss init --shell <name>` and sourced from the startup file.
    pub fn init_file_name(&self) -> Option<&'static str> {
        match self {
            ShellKind::Nushell => None,
            ShellKind::Zsh => Some("init.zsh"),
            ShellKind::Bash => Some("init.bash"),
            ShellKind::Pwsh => Some("init.ps1"),
        }
    }

    /// The single line registered in the startup file: it sources the
    /// generated init file when present, so removing Swiss never breaks the
    /// shell.
    pub fn registration_line(&self) -> Option<String> {
        let file = self.init_file_name()?;
        Some(match self {
            ShellKind::Zsh | ShellKind::Bash => format!(
                "[ -f \"$HOME/.config/swiss/{file}\" ] && . \"$HOME/.config/swiss/{file}\""
            ),
            ShellKind::Pwsh => format!(
                "if (Test-Path \"$HOME/.config/swiss/{file}\") {{ . \"$HOME/.config/swiss/{file}\" }}"
            ),
            ShellKind::Nushell => unreachable!(),
        })
    }
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

/// Splits a nushell module in its env part (env vars + init) and conf part
/// (aliases). Used by the managed loader.
fn render_nu_module_part(spec: &ShellModuleSpec, env_part: bool) -> Option<String> {
    let filtered = if env_part {
        ShellModuleSpec {
            aliases: Default::default(),
            ..spec.clone()
        }
    } else {
        ShellModuleSpec {
            env: Default::default(),
            init: spec
                .init
                .iter()
                .filter(|_| spec.env.is_empty() && spec.aliases.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ..spec.clone()
        }
    };
    render_module(ShellKind::Nushell, &filtered)
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

        match mode {
            ShellMode::Print => continue,
            ShellMode::ManagedLoader => {
                if kind != ShellKind::Nushell {
                    bail!(
                        "Shell '{}' does not support managed-loader mode (only nushell)",
                        shell_name
                    );
                }
                steps.extend(managed_loader_steps_for(config, target, context)?);
            }
            ShellMode::Snippet | ShellMode::Profile => {
                steps.extend(snippet_steps(config, kind, target, context)?);
            }
        }
    }

    Ok(steps)
}

/// Renders the full generated init file for a snippet/profile shell: all
/// module bodies as bounded blocks under a header.
pub fn init_file_content(
    config: &SwissConfig,
    kind: ShellKind,
    modules: &[String],
) -> Result<String> {
    let mut content = String::from("# Generated by swiss - do not edit, re-run `swiss setup`\n");
    for (module_name, spec) in resolve_modules(config, modules, kind.name())? {
        if let Some(body) = render_module(kind, spec) {
            content.push('\n');
            content.push_str(&render_block(module_name, &body));
        }
    }
    Ok(content)
}

/// Snippet/profile shells get one generated init file plus a single bounded
/// registration block in the startup file that sources it.
fn snippet_steps(
    config: &SwissConfig,
    kind: ShellKind,
    target: &ShellTargetConfig,
    context: &PlanContext,
) -> Result<Vec<Step>> {
    let target_file = target
        .target
        .clone()
        .or_else(|| kind.default_target().map(str::to_owned))
        .ok_or_else(|| anyhow::anyhow!("Shell '{}' needs an explicit 'target'", kind.name()))?;
    let init_file = kind
        .init_file_name()
        .ok_or_else(|| anyhow::anyhow!("Shell '{}' has no init file", kind.name()))?;
    let registration = kind
        .registration_line()
        .expect("snippet shells always have a registration line");

    Ok(vec![
        Step::WriteFile {
            path: context.home.join(".config/swiss").join(init_file),
            content: init_file_content(config, kind, &target.modules)?.into_bytes(),
            overwrite: true,
            append_if_missing: false,
            backup: false,
        },
        Step::PatchBlock {
            target: target_file,
            block: REGISTRATION_BLOCK.to_owned(),
            content: registration,
        },
    ])
}

/// Nushell managed-loader steps; also used by `swiss setup` to register a
/// minimal loader when the manifest does not configure Nushell integration.
pub fn managed_loader_steps_for(
    config: &SwissConfig,
    target: &ShellTargetConfig,
    context: &PlanContext,
) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    let swiss_home = context.home.join(".config/swiss");
    let user_dir = target
        .user_modules_dir
        .as_deref()
        .map(|dir| crate::expand::expand_path(dir, &context.home))
        .unwrap_or_else(|| context.home.join(".swiss"));

    for dir in [
        swiss_home.join("env"),
        swiss_home.join("conf"),
        user_dir.join("env"),
        user_dir.join("conf"),
    ] {
        steps.push(Step::EnsureDir { path: dir });
    }

    // Generated module files, aggregated later by `swiss init`.
    for (module_name, spec) in resolve_modules(config, &target.env_modules, "nushell")? {
        if let Some(content) = render_nu_module_part(spec, true) {
            steps.push(Step::WriteFile {
                path: swiss_home.join("env").join(format!("{}.nu", module_name)),
                content: format!(
                    "# Generated by swiss: module '{}'\n{}",
                    module_name, content
                )
                .into_bytes(),
                overwrite: true,
                append_if_missing: false,
                backup: false,
            });
        }
    }
    for (module_name, spec) in resolve_modules(config, &target.conf_modules, "nushell")? {
        if let Some(content) = render_nu_module_part(spec, false) {
            steps.push(Step::WriteFile {
                path: swiss_home.join("conf").join(format!("{}.nu", module_name)),
                content: format!(
                    "# Generated by swiss: module '{}'\n{}",
                    module_name, content
                )
                .into_bytes(),
                overwrite: true,
                append_if_missing: false,
                backup: false,
            });
        }
    }

    // Empty dynamic files so the loaders never fail to source them.
    for name in ["env.dyn.nu", "conf.dyn.nu", "aliases.dyn.nu"] {
        steps.push(Step::WriteFile {
            path: swiss_home.join(name),
            content: Vec::new(),
            overwrite: false,
            append_if_missing: false,
            backup: false,
        });
    }

    steps.push(Step::WriteFile {
        path: swiss_home.join("env.nu"),
        content: nu_env_loader_content().into_bytes(),
        overwrite: true,
        append_if_missing: false,
        backup: false,
    });
    steps.push(Step::WriteFile {
        path: swiss_home.join("conf.nu"),
        content: nu_conf_loader_content().into_bytes(),
        overwrite: true,
        append_if_missing: false,
        backup: false,
    });

    steps.push(Step::ConfigureNu);
    Ok(steps)
}

/// The env loader must put `~/.cargo/bin` on PATH *before* calling `swiss`,
/// otherwise a fresh shell cannot find the binary.
pub fn nu_env_loader_content() -> String {
    let cargo_path = if cfg!(target_os = "windows") {
        "$env.Path = ($env.Path | prepend ~/.cargo/bin)\n$env.PATH = $env.Path"
    } else {
        "$env.PATH = ($env.PATH | prepend ~/.cargo/bin)"
    };
    format!(
        "# Generated by swiss\n{}\n\nswiss init | from yaml | load-env\n\nsource ~/.config/swiss/env.dyn.nu;\n",
        cargo_path
    )
}

pub fn nu_conf_loader_content() -> String {
    "# Generated by swiss\nsource ~/.config/swiss/conf.dyn.nu;\nsource ~/.config/swiss/aliases.dyn.nu;\n"
        .to_owned()
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
                env_modules: [path, starship]
                conf_modules: [zoxide]
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
    fn snippet_shells_get_one_init_file_and_one_registration_block() {
        let config = shell_config();
        let steps = shell_steps(&config, &test_context()).unwrap();

        // bash is disabled: nothing generated and no patch against ~/.bashrc
        assert!(!steps.iter().any(|step| matches!(
            step,
            Step::PatchBlock { target, .. } if target == "~/.bashrc"
        )));
        assert!(!steps.iter().any(|step| matches!(
            step,
            Step::WriteFile { path, .. } if path.ends_with("init.bash")
        )));

        // zsh: generated init file holding all module blocks
        let init_zsh = steps
            .iter()
            .find_map(|step| match step {
                Step::WriteFile { path, content, .. }
                    if path.ends_with(".config/swiss/init.zsh") =>
                {
                    Some(String::from_utf8(content.clone()).unwrap())
                }
                _ => None,
            })
            .expect("init.zsh must be generated");
        assert!(init_zsh.contains("# swiss begin: path"));
        assert!(init_zsh.contains("export PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:$PATH\""));
        assert!(init_zsh.contains("eval \"$(starship init zsh)\""));

        // ...plus a single bounded block in ~/.zshrc sourcing it
        let zshrc_patches: Vec<&Step> = steps
            .iter()
            .filter(|step| matches!(step, Step::PatchBlock { target, .. } if target == "~/.zshrc"))
            .collect();
        assert_eq!(zshrc_patches.len(), 1);
        match zshrc_patches[0] {
            Step::PatchBlock { block, content, .. } => {
                assert_eq!(block, REGISTRATION_BLOCK);
                assert!(content.contains(". \"$HOME/.config/swiss/init.zsh\""));
            }
            _ => unreachable!(),
        }

        // pwsh defaults to $PROFILE and gets init.ps1
        assert!(steps.iter().any(|step| matches!(
            step,
            Step::PatchBlock { target, .. } if target == "$PROFILE"
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            Step::WriteFile { path, .. } if path.ends_with(".config/swiss/init.ps1")
        )));
    }

    #[test]
    fn managed_loader_generates_loader_and_module_files() {
        let config = shell_config();
        let steps = shell_steps(&config, &test_context()).unwrap();

        let written: Vec<&PathBuf> = steps
            .iter()
            .filter_map(|step| match step {
                Step::WriteFile { path, .. } => Some(path),
                _ => None,
            })
            .collect();

        let expect = |suffix: &str| {
            assert!(
                written.iter().any(|path| path.ends_with(suffix)),
                "missing generated file: {} (got {:?})",
                suffix,
                written
            )
        };
        expect(".config/swiss/env.nu");
        expect(".config/swiss/conf.nu");
        expect(".config/swiss/env.dyn.nu");
        expect(".config/swiss/env/path.nu");
        expect(".config/swiss/env/starship.nu");
        expect(".config/swiss/conf/zoxide.nu");

        assert!(steps.contains(&Step::ConfigureNu));

        // The env loader prepends cargo bin before calling swiss.
        let env_loader = steps
            .iter()
            .find_map(|step| match step {
                Step::WriteFile { path, content, .. } if path.ends_with(".config/swiss/env.nu") => {
                    Some(String::from_utf8(content.clone()).unwrap())
                }
                _ => None,
            })
            .unwrap();
        let cargo_index = env_loader.find(".cargo/bin").unwrap();
        let init_index = env_loader.find("swiss init").unwrap();
        assert!(cargo_index < init_index);
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
    fn init_file_content_renders_bounded_blocks() {
        let config = shell_config();
        let output = init_file_content(
            &config,
            ShellKind::Zsh,
            &["path".to_owned(), "starship".to_owned()],
        )
        .unwrap();
        assert!(output.starts_with("# Generated by swiss"));
        assert!(output.contains("# swiss begin: path"));
        assert!(output.contains("# swiss end: path"));
        assert!(output.contains("# swiss begin: starship"));
    }

    #[test]
    fn registration_lines_are_safe_when_file_is_missing() {
        assert!(ShellKind::Zsh
            .registration_line()
            .unwrap()
            .starts_with("[ -f"));
        assert!(ShellKind::Pwsh
            .registration_line()
            .unwrap()
            .contains("Test-Path"));
        assert!(ShellKind::Nushell.registration_line().is_none());
    }
}
