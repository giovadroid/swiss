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
| `swiss init [--shell <name>]` | Shell startup hook: renders the live init script for the shell (detected from `$SHELL`, or `--shell nushell\|zsh\|bash\|pwsh`) from the registered manifest and cached state |
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

nushell:                  # optional: pin a version (omit for latest)
  version: "0.101.0"       # nu is just another shell Swiss can install

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

Shell integration is optional and module-driven. Swiss never writes generated
init files during apply: it patches a **single** bounded block into your shell's
startup file that calls `swiss init --shell <name>` at every startup. That
command renders the init script live from the registered manifest plus the
dynamic state (cached aliases, `SWISS_VERSION`), so editing modules takes effect
on the next shell start with no re-apply. The block prepends `~/.cargo/bin` to
`PATH` and is guarded with `command -v swiss`, so removing the binary never
breaks your shell. The rendered script wraps each module in bounded blocks:

```text
# swiss begin: starship
eval "$(starship init zsh)"
# swiss end: starship
```

Modes:

- `managed-loader` (Nushell): nu can't `eval` a dynamic string, so Swiss patches
  `$nu.env-path` to regenerate `~/.config/swiss/init.nu` from `swiss init` at
  startup and `$nu.config-path` to source it. Drop extra `*.nu` files into
  `~/.swiss/env` / `~/.swiss/conf` and they're appended to the script verbatim.
- `snippet` (zsh/bash): patches `~/.zshrc` / `~/.bashrc` with
  `eval "$(swiss init --shell <name>)"`.
- `profile` (PowerShell): same idea against `$PROFILE`, piped into
  `Invoke-Expression`.
- `print`: nothing is patched; run `swiss init --shell zsh` (or bash/pwsh/nushell)
  and source/eval the output yourself.

Nushell is installed like any other tool (via `cargo binstall`) when a `nushell`
section is present or `shells.nushell` is enabled; zsh/bash/pwsh are installed
from the system package manager when missing. Swiss bootstraps Rust/cargo itself
when a manifest needs it, so it runs on a host with nothing pre-installed.

Service hosts can omit the `shells` section entirely: no shell files are touched.
`swiss setup` additionally registers the init hook for your current shell
(detected from `$SHELL`, override with `--shell`) when the manifest does not
mention it; a manifest that declares a shell — even disabled — is always the
source of truth.

## Notes

- Plain string commands in manifests run through the **portable system shell**
  (`sh` on Unix, `powershell` on Windows) — not Nushell, which may not be
  installed yet. Use the detailed form to pick a specific shell:

  ```yaml
  commands:
    - run: ls | first              # needs shell: nu
      shell: nu
    - run: ./configure && make install
      shell: sh
      requires_admin: true
  ```

- `swiss plan` is read-only; `apply`/`setup` print the plan and ask for
  confirmation (skip with `--yes`).
- **Output & logs**: `apply`/`setup` show a live progress view — a spinner per
  step that resolves to `✓` or `✗` — and **do not stop at the first failure**:
  every step runs, failures are tallied in a final summary, and the command
  exits non-zero if any failed (so the rest of the plan still gets applied). A
  failed install never records its tool as present, so re-running retries only
  what's missing. Each run writes a full log (command output + diagnostics) to
  `~/.config/swiss/logs/run-<id>.log` for later analysis. Pass `-v`/`--verbose`
  to also stream command output and debug logs to the terminal as they happen.
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
