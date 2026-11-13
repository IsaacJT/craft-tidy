# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project overview

craft-tidy is a linter for Snapcraft projects (`snapcraft.yaml`), written in
Rust and distributed as a strictly confined snap. It is licensed
`GPL-3.0-only`.

The project is in early development: the crate currently contains only a
placeholder `main.rs` and no lint rules.

## Repository layout

- `craft-tidy/` - the Rust crate (all application code lives here)
  - `Cargo.toml` - crate manifest
  - `src/main.rs` - binary entry point
- `snap/snapcraft.yaml` - snap packaging (base `core26`, `rust` plugin,
  part source `craft-tidy/`)
- `README.md` - user-facing introduction
- `CONTRIBUTING.md` - contributor workflow and conventions
- `LICENSE` - GPL-3.0 text

## Commands

Run from `craft-tidy/`:

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Run from the repository root to build the snap:

```sh
snapcraft pack
```

Before finishing a change, ensure `cargo fmt`, `cargo clippy` (no warnings)
and `cargo test` all pass.

## Conventions

- Rust edition 2024; follow `rustfmt` defaults.
- Every source file begins with `// SPDX-License-Identifier: GPL-3.0-only`.
- Prefer `Result`-based error handling over panics in non-test code.
- Keep dependencies minimal; do not add crates without a clear need.
- Add tests alongside any new lint rule or bug fix.
- Keep `version` in `craft-tidy/Cargo.toml` and `snap/snapcraft.yaml` in sync.
- If the app needs new system access, add the appropriate interface plug in
  `snap/snapcraft.yaml` and document it in `README.md`.
- Do not commit build artifacts (`target/`, `*.snap`, `parts/`, `stage/`,
  `prime/`).

## Keeping documentation current

`README.md`, `CONTRIBUTING.md` and `AGENTS.md` must be updated in the same
change whenever features, CLI usage, lint rules, repository layout, build
steps, dependencies or conventions change. In particular:

- `README.md`: features, installation, usage, status, layout table.
- `CONTRIBUTING.md`: prerequisites, build/test commands, workflow.
- `AGENTS.md`: overview, layout, commands, conventions.
