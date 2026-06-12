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
  version: "0.101.0"
```

If present, Swiss ensures this Nushell version is installed
(`cargo install nu@<version> --all-features`) and symlinks the nu binaries to
`/usr/local/bin` on Unix. If absent, Swiss does not manage Nushell.

### `rust`

```yaml
rust:
  toolchain: "nightly"
  components:
    - rust-analyzer
  installer:
    linux: ['curl https://sh.rustup.rs -sSf | sh -s -- -y']
```

`components` become `rustup component add <name>` steps. `toolchain` and
`installer` are informational: they document how to bootstrap Rust per OS
(Swiss itself is a Rust binary, so the real first bootstrap is a prebuilt
release or an existing toolchain).

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
    env_modules: [path]         # generated into ~/.config/swiss/env/<m>.nu
    conf_modules: [zoxide]      # generated into ~/.config/swiss/conf/<m>.nu
    user_modules_dir: ~/.swiss  # extra user modules aggregated by `swiss init`
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

Modes:

- `managed-loader` (Nushell only): Swiss generates
  `~/.config/swiss/{env.nu,conf.nu}` plus the dynamic files and per-module files,
  then patches `$nu.env-path` / `$nu.config-path` once.
- `snippet`: Swiss generates one init file under `~/.config/swiss`
  (`init.zsh` / `init.bash`) containing every module as a bounded block, and
  patches a single `# swiss begin: init` block into `target` that sources it.
- `profile`: like snippet, but the default target is PowerShell's `$PROFILE`
  and the generated file is `init.ps1`.
- `print`: never written during apply; print with `swiss init --shell <name>`.

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
    package: starship           # informational
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

For the Nushell managed loader, `env_modules` render env vars + init, and
`conf_modules` render aliases (plus init for pure-init modules).

All generated blocks are idempotent: re-applying replaces the existing block in
place and leaves the rest of the file untouched.
