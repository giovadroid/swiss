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

One command bootstraps a fresh machine: it materializes a starter manifest,
applies it and hooks the Swiss init into your shell:

```bash
swiss setup --manifest bootstrap.yaml --template dev-shell
```

Review before touching anything, and keep everything fresh later:

```bash
swiss plan   --manifest bootstrap.yaml   # read-only
swiss doctor --manifest bootstrap.yaml   # validate tools + manifest
swiss update                             # re-apply the registered manifest
```

Or start from one of the [examples](docs/examples.md):

```bash
swiss setup --manifest examples/workstation.yaml
swiss apply --manifest examples/bootstrap.yaml --profile service-host --yes
```

`swiss setup` and `swiss apply` always require an explicit `--manifest`; Swiss never
applies anything implicitly.

## Commands

| Command | Description |
|---------|-------------|
| `swiss setup -m <path> [--template <name>] [--shell <name>] [-p <profile>...] [--yes]` | Full setup: apply the manifest, register it as active and hook the shell init (detected or `--shell`). `--template` writes a starter manifest (`workstation`, `service-host`, `dev-shell`) when the file does not exist |
| `swiss apply -m <path> [-p <profile>...] [--yes] [--dry-run] [--force-files]` | Bootstrap only: execute the plan, no shell registration |
| `swiss plan -m <path> [-p <profile>...]` | Show the execution plan without changing anything |
| `swiss update [--yes]` | Re-apply the manifest registered by setup/apply to update everything bootstrapped |
| `swiss init [--shell <name>]` | Shell startup hook: Nushell env (default) or the generated zsh/bash/pwsh init code |
| `swiss doctor [-m <path>] [-p <profile>...]` | Diagnose the installation and, optionally, validate a manifest |
| `swiss status` | Registered manifest, cached dependency state and fingerprint |
| `swiss clean-cache` | Delete the cache so the next apply re-runs everything |

`setup` vs `apply`: use `apply` to just bootstrap a host (CI, servers); use
`setup` to leave a machine fully integrated (manifest registered for
`swiss update`, shell init wired into your startup files).

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
- `snippet` (zsh/bash): Swiss generates one init file
  (`~/.config/swiss/init.zsh` / `init.bash`) holding all module blocks, and
  patches a **single** bounded block into `~/.zshrc` / `~/.bashrc` that sources
  it. Your rc file stays clean; modules change only the generated file.
- `profile` (PowerShell): same as snippet, against `$PROFILE` with `init.ps1`.
- `print`: nothing is written; run `swiss init --shell zsh` (or bash/pwsh) to
  print the generated init code and source/eval it yourself.

Service hosts can omit the `shells` section entirely: no shell files are touched.
`swiss setup` additionally registers the init hook for your current shell
(detected from `$SHELL`, override with `--shell`) when the manifest does not
mention it; a manifest that declares a shell — even disabled — is always the
source of truth.

## Notes

- Plain string commands in manifests run through **Nushell** (`nu -c`). Use the
  detailed form to pick another shell:

  ```yaml
  commands:
    - run: ./configure && make install
      shell: sh
      requires_admin: true
  ```

- `swiss plan` is read-only; `apply`/`setup` print the plan and ask for
  confirmation (skip with `--yes`).
- The cache (`~/.config/swiss/.cache`) records installed dependencies, aliases,
  the manifest fingerprint and the registered manifest used by `swiss update`;
  pinned cargo versions are skipped when already installed. `swiss clean-cache`
  resets it.
- Secrets are never stored in manifests: use the `env` section to require or
  default environment variables.

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked
```

## License

Swiss is released under the [GNU General Public License v3.0](LICENSE).
