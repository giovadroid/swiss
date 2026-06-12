# SWISS

A generic, file-driven bootstrapper for workstations and service hosts, built in Rust.

Swiss installs packages, tools, repositories, files and shell integration from explicit
YAML manifests. The binary ships only the engine: there is no embedded default
environment. What gets installed and how your shells are configured is entirely
defined by the manifest files you provide.

Three separable layers:

1. **Bootstrap engine** — the `swiss` binary: plans and executes manifests.
2. **Manifests** — declarative YAML files with includes, profiles and overlays
   (see [docs/manifest.md](docs/manifest.md)).
3. **Shell integration modules** — optional per-shell modules that generate or patch
   startup files for Nushell, zsh, bash and PowerShell.

## Install

```bash
git clone https://github.com/giovadroid/swiss.git
cd swiss
cargo install --path .
```

## Quick start

Generate a starter manifest, review the plan, apply it:

```bash
swiss init-config --template dev-shell --output bootstrap.yaml
swiss plan  --manifest bootstrap.yaml
swiss apply --manifest bootstrap.yaml
```

Or start from one of the [examples](docs/examples.md):

```bash
swiss plan  --manifest examples/workstation.yaml
swiss setup --manifest examples/workstation.yaml
swiss apply --manifest examples/bootstrap.yaml --profile service-host --yes
```

`swiss setup` and `swiss apply` always require an explicit `--manifest`; Swiss never
applies anything implicitly.

## Commands

| Command | Description |
|---------|-------------|
| `swiss plan -m <path> [-p <profile>...]` | Show the execution plan without changing anything |
| `swiss apply -m <path> [-p <profile>...] [--yes] [--dry-run]` | Execute the plan (asks for confirmation unless `--yes`) |
| `swiss setup -m <path> ...` | Like `apply`, plus first-run checks (fails early if admin rights are needed) |
| `swiss files -m <path> [--force]` | Apply only the file/directory/shell-patch steps, no installs |
| `swiss shell plan\|apply\|print -m <path> --shell <name>` | Shell integration for one shell (`nushell`, `zsh`, `bash`, `pwsh`) |
| `swiss status` | Cached dependency state and last applied manifest fingerprint |
| `swiss doctor` | Check that nu/cargo/rustup/git and the OS package manager are available |
| `swiss clean-cache` | Delete the cache so the next apply re-runs everything |
| `swiss init-config --template <name> --output <path>` | Write a starter manifest (`workstation`, `service-host`, `dev-shell`) |
| `swiss init` | Internal: prints the Swiss environment for the Nushell loader |

## Manifest overview

```yaml
includes:                 # compose multiple files (relative to this one)
  - ./base.yaml

nushell:                  # optional: manage the Nushell version
  version: "0.101.0"

package_manager:          # OS packages (apt / brew / scoop)
  linux:
    apt:
      update_index: true
      packages: [git, curl]

dependencies:
  cargo:                  # cargo packages, installed via cargo-binstall
    ripgrep:
    zoxide:
      args: ["--locked"]
      alias: {cdi: "__zoxide_zi"}
  customs:                # git-sourced or scripted tools with install/update hooks
    helix:
      git: {repo: "https://github.com/helix-editor/helix", branch: master}
      install: ["cargo install --path helix-term"]

files:                    # files/templates to write, with overwrite/append modes
  - source: ./templates/starship.toml
    dest: ~/.config/starship.toml
    overwrite: false

shells:                   # optional shell integration, per shell
  nushell: {mode: managed-loader}
  zsh: {mode: snippet, target: ~/.zshrc, modules: [path, starship]}

shell_modules:            # reusable named units shared by all shells
  path:
    env: {PATH: {prepend: ["~/.cargo/bin"]}}
  starship:
    init: {zsh: 'eval "$(starship init zsh)"'}

profiles:                 # named overlays selected with --profile
  service-host:
    package_manager:
      linux: {apt: {packages: [docker.io]}}
```

Full schema reference: [docs/manifest.md](docs/manifest.md).

## Shell integration

Shell integration is optional and module-driven. Generated content is always wrapped
in bounded, idempotent blocks:

```text
# swiss begin: starship
eval "$(starship init zsh)"
# swiss end: starship
```

Modes:

- `managed-loader` (Nushell): Swiss owns `~/.config/swiss` and generates the loader
  files (`env.nu`, `conf.nu`, dynamic aggregation via `swiss init`), patching
  `$nu.env-path` / `$nu.config-path` once with stable source lines. User modules in
  `~/.swiss/env` and `~/.swiss/conf` are aggregated at shell startup.
- `snippet` (zsh/bash): Swiss patches bounded blocks into `~/.zshrc` / `~/.bashrc`.
- `profile` (PowerShell): same as snippet, against `$PROFILE`.
- `print`: nothing is written; use `swiss shell print` and source it yourself.

Service hosts can omit the `shells` section entirely: no shell files are touched.

## Notes

- Plain string commands in manifests run through **Nushell** (`nu -c`). Use the
  detailed form to pick another shell:

  ```yaml
  commands:
    - run: ./configure && make install
      shell: sh
      requires_admin: true
  ```

- `swiss plan` is read-only; `apply` prints the plan and asks for confirmation
  (skip with `--yes`).
- The cache (`~/.config/swiss/.cache`) records installed dependencies, aliases and
  the manifest fingerprint; pinned cargo versions are skipped when already
  installed. `swiss clean-cache` resets it.
- Secrets are never stored in manifests: use the `env` section to require or
  default environment variables.

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked
```
