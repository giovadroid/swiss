# Swiss manifest reference

A manifest is a YAML file passed to `swiss plan/apply/setup/doctor` with
`--manifest <path>`. All sections are optional; an empty manifest is a valid
(empty) plan. Validate any manifest with `swiss doctor -m <path>`.

## Composition

### `includes`

```yaml
includes:
  - ./base.yaml
  - ./services/docker.yaml
```

- Paths are resolved relative to the file declaring them.
- Includes are loaded first, in order; the including file is merged last and wins.
- Merge rules:
  - **maps** merge per key, recursively;
  - **lists** concatenate, skipping exact duplicates;
  - **scalars** are overridden by the later value;
  - a **null** overlay value keeps the existing value (so `ripgrep:`-style empty
    keys never erase earlier data).
- Circular includes and missing include files are errors.

### `profiles`

```yaml
profiles:
  workstation:
    dependencies:
      cargo:
        ripgrep:
  service-host:
    package_manager:
      linux:
        apt:
          packages: [docker.io]
```

- The top level of the manifest is the base; selected profiles overlay it in the
  order given: `swiss apply -m boot.yaml --profile base --profile devops`.
- Multiple profiles are allowed. Unknown profile names fail, listing the
  available ones.
- Profile bodies use the same schema (and merge rules) as the top level.

## Bootstrap sections

### `nushell`

```yaml
nushell:
  version: "0.101.0"   # optional: pin a version; omit for the latest
```

Nushell is just another shell Swiss can install. Swiss installs it when either
this section is present **or** `shells.nushell` is enabled: it runs
`cargo binstall -y nu[@<version>]` and symlinks the nu binaries to
`/usr/local/bin` on Unix. The `nushell` section only exists to pin a version
explicitly; an empty `nushell: {}` or an enabled `shells.nushell` gets the
latest release. If neither is present, Swiss does not manage Nushell.

### `rust`

```yaml
rust:
  toolchain: "nightly"
  components:
    - rust-analyzer
  installer:
    linux: ['curl https://sh.rustup.rs -sSf | sh -s -- -y']
```

When anything in the manifest needs cargo (cargo dependencies, the `nushell`
section, `toolchain` or `components`) and cargo is not installed, Swiss first
runs `installer` for the current OS — or the official rustup one-liner when no
installer is declared. Plain installer entries run through `sh` (PowerShell on
Windows), never Nushell, since nu may not exist yet. Then `toolchain` becomes
`rustup toolchain install <name>` and `components` become
`rustup component add [--toolchain <name>] <component>` steps.

### `package_manager`

```yaml
package_manager:
  linux:
    apt:
      update_index: true      # run `apt-get update` first
      packages: [git, curl]
  macos:
    brew:
      packages: [cmake]
  windows:
    scoop:
      packages: [libssl-dev]
```

Only the section matching the current OS runs. apt steps run through `sudo` and
are marked as requiring admin. `swiss doctor` reports whether the manager binary
is available.

### `dependencies.cargo`

```yaml
dependencies:
  cargo:
    cargo-binstall:            # bootstraps the rest; always installed first
    ripgrep:                   # empty value = defaults
    zoxide:
      version: "0.9.4"         # pinned; skipped when cache matches
      args: ["--locked"]       # extra args for cargo binstall
      alias:
        cdi: "__zoxide_zi"     # recorded and loaded into Nushell aliases
      commands:                # post-install commands
        - "tldr --update"
      windows: false           # OS gates (default true)
```

Packages install via `cargo binstall -y <name>[@version]`. Pinned versions
already recorded in the cache are skipped; `*` (default) always reinstalls.

When `cargo-binstall` itself is missing from the host, Swiss bootstraps it
with plain `cargo install --locked cargo-binstall` (honouring a pinned
version) before any binstall step — it never tries to install binstall with
binstall, and it does this even when the manifest does not list
`cargo-binstall` explicitly. Once the binary exists, a `cargo-binstall` entry
self-updates through binstall as usual.

### `dependencies.customs`

```yaml
dependencies:
  customs:
    helix:
      git:
        repo: "https://github.com/helix-editor/helix"
        branch: master
        depth: 1
        recursive: false
      install: ["cargo install --path helix-term"]
      update:  ["cargo install --path helix-term"]   # used on re-runs
      post-install:                                   # first install only, per OS
        linux: ["ln -s ..."]                          # macOS falls back to linux
      linux: true                                     # OS gates
```

Each custom dependency gets a working directory under
`~/.config/swiss/crates/<name>`. With `git`, Swiss clones on first run and
`git pull --ff-only` afterwards. `install` runs on first install; `update`
(falling back to `install`) on later runs.

### Command entries

Anywhere a command list appears (`commands`, `install`, `update`,
`post-install`...), entries are either plain strings or detailed maps:

```yaml
install:
  - "plain string"                 # runs through Nushell: nu -c "..."
  - run: ./configure && make install
    shell: sh                      # nu | sh | bash | zsh | pwsh | cmd
    requires_admin: true           # marks the plan as needing admin
```

### `files`

```yaml
files:
  - source: ./templates/starship.toml   # relative to the manifest file
    dest: ~/.config/starship.toml       # supports ~ and $VARS
    overwrite: false                    # default: keep existing files
    backup: true                        # copy to <name>.bak before overwriting
  - content: |
      source ~/.config/swiss/env.nu
    dest: ~/.config/nushell/env.nu
    append_if_missing: true             # append only if not already present
  - source: ./templates/nu/macos.nu
    dest: ~/.swiss/env/macos.nu
    os: [macos]                         # restrict to specific OSes
  - source: ./templates/daemon.conf
    dest: /etc/myservice/daemon.conf
    admin: true                         # install with elevated privileges (sudo)
```

Exactly one of `source` / `content` must be set. Template contents are resolved
at plan time, so `swiss plan` shows real byte counts.

`admin: true` writes the destination with elevated privileges — use it for
paths outside the user's home such as `/etc`. On Unix, when the process is not
already root, Swiss stages the content in a temp file and installs it through
`sudo` (honouring `overwrite`, `append_if_missing` and `backup`); the plan
marks these steps with `[admin]`.

### `env`

```yaml
env:
  GITHUB_TOKEN:
    from_env: GITHUB_TOKEN
    required: true          # plan fails when unset
  RUST_LOG:
    default: info           # exported during apply when unset
```

Secret references only — values never live in the manifest.

## Shell integration

### `shells`

```yaml
shells:
  nushell:
    enabled: true
    mode: managed-loader        # default for nushell
    modules: [path]             # rendered into the init script by `swiss init`
  zsh:
    enabled: true
    mode: snippet               # default for zsh/bash
    target: ~/.zshrc            # default per shell
    modules: [path, starship]
  bash:
    enabled: false
  pwsh:
    mode: profile               # patches $PROFILE (resolved through pwsh)
    modules: [path]
```

Swiss does **not** write generated init files during apply. Instead it patches
a single bounded `# swiss begin: init` block into the shell's startup file that
calls `swiss init --shell <name>` at every startup; that command renders the
script live from the registered manifest plus the dynamic state (cached
aliases, `SWISS_VERSION`). Editing the manifest's modules takes effect on the
next shell start, with no re-apply needed — and removing the `swiss` binary
degrades gracefully (the block is guarded with `command -v swiss`).

Enabled shells (any mode except `print`) are installed before they are
configured. zsh/bash/pwsh come from the system package manager when missing
(`apt-get install zsh`, `brew install zsh`, `brew install --cask powershell`…);
Nushell rides the cargo/binstall phase (see the `nushell` section). Where no
automatic install exists (e.g. pwsh on Linux, zsh on Windows) Swiss warns and
still registers the — harmless — startup hook.

Modes:

- `managed-loader` (Nushell only): Nushell cannot `eval` a dynamic string, so
  Swiss patches `$nu.env-path` to regenerate `~/.config/swiss/init.nu` from
  `swiss init --shell nushell` at startup and `$nu.config-path` to source it.
- `snippet` (zsh/bash): patches `target` to `eval "$(swiss init --shell <name>)"`.
- `profile`: like snippet, but the default target is PowerShell's `$PROFILE`
  and it pipes `swiss init --shell pwsh` into `Invoke-Expression`.
- `print`: nothing is patched during apply; run `swiss init --shell <name>`
  yourself and source/eval the output.

In every mode the registration block prepends `~/.cargo/bin` to `PATH` first,
so a freshly bootstrapped shell can find the `swiss` binary.

For Nushell, drop any `*.nu` file into `~/.swiss/env` or `~/.swiss/conf` (e.g.
via a `files` entry) and `swiss init --shell nushell` appends it verbatim after
the declarative modules — an escape hatch for snippets that don't fit
`shell_modules`.

### `shell_modules`

```yaml
shell_modules:
  path:
    env:
      PATH:
        prepend: ["~/.cargo/bin", "~/.local/bin"]
      EDITOR: hx                # plain value
  aliases:
    aliases:
      ll: "ls -la"
  starship:
    package: starship           # doctor warns when nothing installs it
    init:                       # per-shell init line, emitted verbatim
      nushell: "starship init nu | save -f ~/.cache/starship/init.nu"
      zsh: eval "$(starship init zsh)"
      bash: eval "$(starship init bash)"
      pwsh: Invoke-Expression (&starship init powershell)
```

Rendering per shell:

| Spec | zsh/bash | pwsh | nushell |
|------|----------|------|---------|
| `env` plain | `export K="V"` | `$env:K = "V"` | `$env.K = "V"` |
| `env` prepend/append | `export PATH="a:b:$PATH"` | `[IO.Path]::PathSeparator` chains | `$env.PATH = ($env.PATH \| prepend [...])` |
| `aliases` | `alias k='v'` | `function k { v @args }` | `alias k = v` |
| `init.<shell>` | verbatim | verbatim | verbatim |

Every module in a shell's `modules` list renders as a bounded `# swiss begin:
<module>` block, in list order, followed by a `swiss-aliases` block holding the
aliases recorded in the cache by installed cargo dependencies. Order matters:
list a base framework (e.g. oh-my-zsh) before the prompt/tool inits that should
override it.

All blocks (both the startup registration block and the per-module blocks in
the rendered script) are idempotent: re-running replaces the existing block in
place and leaves the rest untouched.
