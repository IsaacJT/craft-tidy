# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project overview

craft-tidy is a linter for Snapcraft projects (`snapcraft.yaml`), written in
Rust and distributed as a strictly confined snap. It is licensed
`GPL-3.0-only`.

The project is in early development: the CLI, the configuration system and
the check framework exist, but the check registry is empty (no lint checks
are implemented yet).

## Repository layout

- `craft-tidy/` - the Rust crate (all application code lives here)
  - `Cargo.toml` - crate manifest (`rust-version = "1.89"`)
  - `src/main.rs` - thin binary: parses `Cli`, calls `app::run`, maps the
    result to an exit code
  - `src/lib.rs` - module declarations
  - `src/app.rs` - program flow, `resolve_settings()` (config layering);
    `run()` uses the real registry, `run_with_checks()` takes any checks
  - `src/cli.rs` - clap derive `Cli`; `Cli::settings()` turns flags into a
    `Settings` layer
  - `src/config.rs` - `Settings`, `CheckOptions`, `OptionValue`, `.craft-tidy`
    parsing/serialization and upward discovery
  - `src/glob.rs` - `GlobList`, clang-tidy style check globs
  - `src/project.rs` - locating/loading `snapcraft.yaml`
  - `src/checks/mod.rs` - `Check` trait, `Diagnostic`, `registry()`,
    `enabled()`, `run()`
  - `src/error.rs` - crate `Error` enum
  - `tests/` - the whole test suite (see [Testing](#testing))
- `examples/.craft-tidy` - commented example configuration, verified by
  `craft-tidy/tests/example.rs`
- `snap/snapcraft.yaml` - snap packaging (base `core26`, `rust` plugin,
  part source `craft-tidy/`, `home` plug)
- `README.md` - user-facing introduction, usage and configuration reference
- `CONTRIBUTING.md` - contributor workflow, code structure, how to add checks
  and settings
- `LICENSE` - GPL-3.0 text

## Configuration model

- Configuration is either CLI flags or a YAML file named `.craft-tidy`,
  discovered by walking up from the project directory.
- Layers, lowest to highest precedence: `Settings::defaults()` →
  `.craft-tidy` (or `--config-file`; skipped with `--no-config`) → inline
  `--config` YAML → individual flags (`Cli::settings()`).
- Merge rules (`Settings::merge`): `GlobList` fields are appended (last
  matching glob wins); `check-options` are overridden per check and key.
  New scalar settings should be `Option<T>` and override.
- `Settings` uses `#[serde(default, deny_unknown_fields, rename_all =
  "kebab-case")]`; keep it that way so typos are reported with locations.
- Custom deserializers use `deserialize_any` and accept only the intended
  YAML types: `GlobList` takes a string, a list of strings or null;
  `OptionValue` takes a bool, integer, float or string. serde-saphyr
  otherwise converts plain scalars such as `1` to `String`, so do not
  deserialize user-facing strings via plain `String` where a number would be
  a mistake.
- `--dump-config` output must remain a valid `.craft-tidy` file (there is a
  round-trip test).
- Every setting must be configurable both in the file and on the CLI.

## Checks

- Names are kebab-case with a category prefix (e.g. `metadata-summary-length`).
- Implement `Check`, register in `checks::registry()`, read options via
  `ctx.options.get(name, key)`, emit `Diagnostic::warning(..)`. The runner
  applies `warnings-as-errors`.

## Testing

- All tests live in `craft-tidy/tests/`, one file per module (`glob.rs`,
  `config.rs`, `cli.rs`, `project.rs`, `checks.rs`, `app.rs`), plus
  `example.rs` (the example config) and `binary.rs` (end-to-end runs of the
  compiled binary). Shared helpers are in `tests/common/mod.rs`, included
  with `mod common;`.
- Do not add `#[cfg(test)]` modules to `src/`; test through the public API.
- Every change needs both positive tests (valid input works) and negative
  tests (invalid input is rejected, ideally asserting on the error).
- Use `common::FakeCheck` with `app::run_with_checks` / `checks::run` to test
  the framework; use `tempfile::tempdir()` for filesystem tests.
- `examples/.craft-tidy` must contain every top-level `Settings` key and only
  reference registered checks in `check-options`; `tests/example.rs` fails
  otherwise. Update it whenever settings or check options change.

## Exit codes

`0` success, `1` at least one error-severity finding, `2` usage,
configuration or I/O error (same as clap's usage error code).

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

## Committing

- Commit as you go, at logical checkpoints: each commit is one
  self-contained step (e.g. a new setting, a new check, a bug fix, a docs
  update), not a dump of a whole session's work.
- Every commit must build and pass `cargo fmt --check`, `cargo clippy
  --all-targets -- -D warnings` and `cargo test`, and include the tests and
  doc updates (`README.md`, `CONTRIBUTING.md`, `AGENTS.md`,
  `examples/.craft-tidy`) for the code it adds.
- Commit messages: summary line of 50 characters or fewer, blank line, then
  what changed and why. Always sign off (`git commit -s`).
- Stage files explicitly and check `git status` before committing; never
  commit build artifacts or unrelated changes.

## Conventions

- Rust edition 2024; follow `rustfmt` defaults.
- Every source file begins with `// SPDX-License-Identifier: GPL-3.0-only`.
- Prefer `Result`-based error handling over panics in non-test code.
- Program output goes to the `out` writer and warnings to the `err` writer
  passed to `app::run`; do not print directly from library code.
- Keep dependencies minimal; do not add crates without a clear need. Current
  runtime dependencies: `clap`, `serde`, `serde-saphyr`. Do not use the
  deprecated `serde_yaml`/`serde_yml` crates.
- Add positive and negative tests in `craft-tidy/tests/` for any new check,
  setting or bug fix.
- Keep `version` in `craft-tidy/Cargo.toml` and `snap/snapcraft.yaml` in sync.
- If the app needs new system access, add the appropriate interface plug in
  `snap/snapcraft.yaml` and document it in `README.md`.
- Do not commit build artifacts (`target/`, `*.snap`, `parts/`, `stage/`,
  `prime/`).

## Keeping documentation current

`README.md`, `CONTRIBUTING.md` and `AGENTS.md` must be updated in the same
change whenever features, CLI usage, checks, configuration keys, repository
layout, build steps, dependencies or conventions change. In particular:

- `README.md`: features, installation, usage, options, configuration keys,
  exit codes, status, layout table.
- `CONTRIBUTING.md`: prerequisites, build/test commands, code structure,
  test file table, how to add checks/settings, dependency table, workflow.
- `AGENTS.md`: overview, layout, configuration model, testing, committing,
  commands, conventions.
- `examples/.craft-tidy`: every setting, and options of existing checks.
