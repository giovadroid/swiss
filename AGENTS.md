# AGENTS.md — working on swiss

Swiss is a manifest-driven bootstrap engine in Rust. It turns YAML manifests
into an explicit execution plan (packages, cargo tools, git repos, files,
shell integration) and runs it. **The binary ships no embedded environment** —
everything comes from manifest files.

## Build, test, validate

```bash
cargo build
cargo test --all --locked
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
```

All four must pass before any commit. Clippy runs with `-D warnings`: no dead
code, no unused imports. Toolchain is pinned in `rust-toolchain.toml`.

Quick manual smoke (read-only, never mutates the system):

```bash
cargo run -- plan --manifest examples/workstation.yaml
cargo run -- doctor --manifest examples/bootstrap.yaml --profile service-host
cargo run -- setup --manifest /tmp/boot.yaml --template dev-shell --dry-run
```

## Architecture (data flows left to right)

```
manifest.yaml ──> config_loader ──> SwissConfig ──> plan ──> Plan ──> executor
                  (includes,        (parser.rs)    (pure!)  (Steps)  (side effects)
                   profiles)                          │
                                                      └─ shellgen (shell init/blocks)
```

| Module | Role |
|--------|------|
| `src/cli.rs` | clap definitions. Commands: `init`, `plan`, `apply`, `setup`, `update`, `status`, `doctor`, `clean-cache` |
| `src/config_loader.rs` | Loads YAML from path; resolves `includes` (deep merge, cycle detection) and `profiles` overlays; computes the manifest fingerprint |
| `src/parser.rs` | The manifest data model (`SwissConfig`) and the `Os` enum |
| `src/plan.rs` | `Step` enum + `build_plan()`. **Pure**: only reads `files[].source` templates, never touches the system |
| `src/executor.rs` | Runs `Step`s: commands, file writes (overwrite/append/backup), git sync, bounded-block patches |
| `src/shellgen.rs` | Shell integration generators (nushell/zsh/bash/pwsh), bounded blocks, init files, the nu managed loader |
| `src/doctor.rs` | Installation checks + manifest lints (`Finding::Error/Warning`) |
| `src/loader.rs` | `swiss init` for Nushell: aggregates `~/.config/swiss/{env,conf}` + `~/.swiss/{env,conf}` into `*.dyn.nu` |
| `src/persistence.rs` | rkyv cache at `~/.config/swiss/.cache`: deps, aliases, fingerprint, registered manifest |
| `src/embedded.rs` | Embeds `examples/*.yaml` **only** for `setup --template` and tests |
| `src/expand.rs` | `~` and `$VAR` expansion |

Docs: `README.md` (user guide), `docs/manifest.md` (schema), `docs/examples.md`.
Examples and file templates: `examples/`, `examples/templates/`.

## Invariants — do not break these

1. **Plan generation is pure.** `build_plan()` and everything in `shellgen.rs`
   must be testable with a fake `PlanContext` (os/home/cache injected). System
   probes (nu version, $SHELL, real home) happen only in `main.rs` /
   `executor.rs`. Tests must never need `sudo`, `git`, `rustup` or `nu`.
2. **No implicit manifests.** Never re-introduce an embedded default that gets
   applied without `--manifest`.
3. **Generated shell content lives in bounded blocks** (`# swiss begin: <name>`
   / `# swiss end: <name>`) and is idempotent: re-applying replaces in place
   (`shellgen::upsert_block`). Startup files get exactly one Swiss block (the
   `init` registration); everything else goes into generated files under
   `~/.config/swiss/`.
4. **Destructive file writes are opt-in** (`overwrite`, `--force-files`),
   never the default.
5. **Plain manifest commands run via Nushell** (`nu -c`). The detailed form
   `{run, shell, requires_admin}` selects other shells. Admin-needing steps
   must be flagged so `Plan::requires_admin()` stays truthful.
6. **Cross-OS logic takes `Os` as a parameter** (no `cfg!`-only branching in
   plan code) so Linux CI can test macOS/Windows plans.

## Conventions

- Errors: `anyhow` with `.context()` carrying the file path or step that
  failed; user-facing errors must say what to do next.
- Tests live in `#[cfg(test)] mod tests` per file; use `tempfile` for any
  filesystem test and `indoc!` for YAML fixtures. CLI behavior is tested with
  `Cli::try_parse_from`.
- The cache schema (`persistence.rs`) is rkyv-serialized; invalid/old caches
  must keep falling back to `SwissCache::default()` instead of erroring.
- New manifest fields: add to `parser.rs` with `#[serde(default)]` (old
  manifests must keep parsing), document in `docs/manifest.md`, and cover with
  a parser test and a doctor lint if misuse is plausible.

## CLI semantics (keep these distinctions)

- `plan` — read-only, prints steps.
- `apply` — bootstrap only: executes the plan, registers the manifest for
  `update`. Warns (does not stop) on missing admin rights.
- `setup` — `apply` + full integration: `--template` materializes a starter
  manifest, and the init hook is registered in the detected (or `--shell`)
  shell when the manifest itself does not cover it. Fails early without admin
  rights. Manifests that mention a shell (even `enabled: false`) are the
  source of truth — setup never overrides them.
- `update` — re-applies the registered manifest (path + profiles from cache).
- `init --shell <name>` — startup hook: nushell prints env YAML (and rebuilds
  the dyn loaders); zsh/bash/pwsh print the generated init file.
- `doctor [-m <path>]` — installation checks, plus manifest validation when a
  manifest is given.

## Roadmap and open decisions

See `.ai/bootstrap-roadmap.md` for the backlog (retry, URL manifests, more
package managers) and the product decisions still owned by Jaesbit.
