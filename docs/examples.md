# Swiss examples

All examples live in [`examples/`](../examples) and are plain manifests — Swiss
embeds them only for `swiss init-config`, never applies them implicitly.

## `examples/workstation.yaml`

The former embedded Swiss default environment: full Nushell-centric terminal
stack (cargo CLI tools, helix/wezterm from git, starship, apt/brew/scoop
packages) plus tool configuration written from
[`examples/templates/`](../examples/templates).

```bash
swiss plan  --manifest examples/workstation.yaml
swiss setup --manifest examples/workstation.yaml
```

Note: its `files` entries reference `./templates/*` relative to the manifest, so
run it from a checkout (or copy the templates next to your manifest).

## `examples/service-host.yaml`

Headless server bootstrap: apt packages (docker, git, curl), a couple of cargo
tools, **no shell integration**. Includes `monitoring` and `backup` profiles.

```bash
swiss apply --manifest examples/service-host.yaml --yes
swiss apply --manifest examples/service-host.yaml --profile monitoring --yes
```

## `examples/dev-shell.yaml`

Minimal developer shell: ripgrep/bat/zoxide/starship plus zsh (and optional
bash) snippets. The default `swiss init-config` template.

```bash
swiss init-config --template dev-shell --output bootstrap.yaml
swiss shell print --manifest bootstrap.yaml --shell zsh
```

## `examples/bootstrap.yaml` + `examples/base.yaml`

Composition demo: a shared base included by a root manifest with `workstation`
and `service-host` profiles.

```bash
swiss plan --manifest examples/bootstrap.yaml --profile workstation
swiss plan --manifest examples/bootstrap.yaml --profile service-host
```

## Templates

[`examples/templates/`](../examples/templates) holds the file templates used by
the workstation example:

- `starship.toml`, `helix.toml`, `zellij.yaml` — tool configuration.
- `nu/*.nu` — rich Nushell modules (fnm, starship prompt, zoxide, print helper,
  macOS PATH fixes) written into `~/.swiss/{env,conf}` and aggregated by the
  managed loader at shell startup.
