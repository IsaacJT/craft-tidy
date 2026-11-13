# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project overview

craft-tidy is a linter for the project files of Canonical's craft tools
(`snapcraft.yaml`, `charmcraft.yaml`, `rockcraft.yaml`, `imagecraft.yaml`,
`debcraft.yaml`), written in
Rust and distributed as a strictly confined snap. It is licensed
`GPL-3.0-only`.

The project is in early development: the CLI, the configuration system, the
check framework and `--fix` exist. Implemented checks:
`project-file-location`, `style-line-length`, `style-sorted-lists`.

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
  - `src/craft.rs` - `Craft` enum: tool names, `project_file_locations()`
    (each tool's search order; only Snapcraft has more than `<name>.yaml`)
    and `from_path()` (tool from the file name)
  - `src/project.rs` - `find()` returns the project file of every tool found
    in a directory (or the given file); `Project` holds the `craft`, the
    path and the parsed `yaml::Document` (`project.source()` for the text)
  - `src/yaml.rs` - `Document` (source text, node tree, comment spans, line
    helpers), `Node` with byte-range `span`s, and the presentation-free
    `Value` model used to verify fixes. Built from
    `serde_saphyr::granit_parser` events (no extra dependency)
  - `src/checks/mod.rs` - `Check` trait, `Diagnostic`, `registry()`,
    `enabled()`, `validate_options()`, `run()`, `fix()` (repeats passes over
    the enabled checks' `Check::fix` until nothing changes, max 10)
  - `src/checks/project_file_location.rs` - `project-file-location`: reports
    `snapcraft.yaml`/`.snapcraft.yaml` whose (absolute) parent directory is
    not named `snap` (so `build-aux/snap/` is fine); Snapcraft-only
    (`crafts()`), path-only, no fix
  - `src/checks/line_length.rs` - `style-line-length` (option `max-length`);
    fix wraps plain/quoted scalars, folded-scalar lines, flow sequences and
    prose comments; lines it skips get an `unfixable` reason
  - `src/checks/sorted_lists.rs` - `style-sorted-lists` (options `keys`,
    `case-sensitive`). Only lists under `keys` (default: known unordered
    sets; never order-sensitive ones like `command-chain` or `*-parameters`).
    Advanced grammar (`on`/`to`/`for`/`try`/`else`/`else fail`, matched like
    craft-grammar's `_processor.py`: keyword + any whitespace) goes last in
    original order; an `else` must follow a grammar entry; nested grammar
    lists are checked too. Fix moves whole
    entry lines (with comments above, except above the first entry) and
    verifies the result equals the original `Value` with that list permuted
  - `src/fix.rs` - `Edit` and `apply_edits()`: applies edits from the end of
    the file backwards, skips overlapping ones, and keeps an edit only if the
    result parses and the check's verification closure accepts it
  - `src/error.rs` - crate `Error` enum
  - `tests/` - the whole test suite (see [Testing](#testing))
- `examples/.craft-tidy` - commented example configuration, verified by
  `craft-tidy/tests/example.rs`
- `snap/snapcraft.yaml` - snap packaging (base `core26`, `rust` plugin,
  part source `craft-tidy/`, `home` plug)
- `.github/workflows/ci.yaml` - CI: fmt + clippy, `cargo test` on stable
  and the `rust-version` (keep the matrix in sync with `Cargo.toml`), and
  `canonical/craft-actions/snapcraft/pack@v1` with `test: true` (pack +
  spread), uploading the snap as an artifact
- `snapcraft-test.yaml`, `spread/` - spread suite for `snapcraft test`:
  `spread/general/<task>/task.yaml` run the installed snap in an LXD VM;
  one task per craft tool (`charmcraft`, `debcraft`, `imagecraft`,
  `rockcraft`, `snapcraft`) plus `cli`, `config`, `errors`, `fix`;
  projects go in `$WORK` (`/root/work`); `spread/.extension` is Snapcraft's
  template plus a cloud-init wait
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
  Scalar settings are `Option<T>` (`None` = not set in this layer) and
  override; `fix` is the model to follow (`--fix`/`--no-fix`, value in
  `Settings::defaults()`, skipped when `None` on serialization, strict
  deserializer).
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

- Names are kebab-case with a category prefix. Existing categories:
  `project-` (project layout), `style-` (formatting of the project file).
  Reuse one where it fits; a new category must be documented in `README.md`.
- Implement `Check`, register in `checks::registry()`, read options via
  `ctx.options.get(name, key)`, emit `Diagnostic::warning(..)`. The runner
  applies `warnings-as-errors`.
- Checks apply to every tool's project file by default. Restrict one with
  `Check::crafts()` (e.g. `project-file-location` is Snapcraft-only);
  `checks::applies_to` filters both `run` and `fix`. Files whose tool is
  unknown (`Project::craft == None`) only get unrestricted checks.
- Declare every option in `Check::options()` (`OptionSpec`: name, `OptionKind`,
  default shown by `--explain`, description) and put extra constraints in
  `Check::validate_option()`. `checks::validate_options()` runs before any
  action: unknown options / wrong types / failed validation of a known check
  → `Error::InvalidOption` (exit 2); options of unknown checks → warning.
  `tests/checks.rs` verifies declared defaults are valid.

## Fixes

- With `fix` enabled, `app` runs the checks, calls `checks::fix()`, writes
  the file only if it changed, re-runs the checks and reports what remains
  (summary on stderr). Exit status reflects the remaining findings.
- A check's `fix()` must build `fix::Edit`s on the original text (never
  regenerate the file) and use `fix::apply_edits` with a closure verifying
  `Document::value()` before/after: pure reformatting must leave the value
  identical; data changes (e.g. reordering) must equal the expected value
  exactly. When unsure whether an edit is safe for Snapcraft's YAML parser
  too, do not propose it.
- Fixes must be idempotent and must not fight other checks' fixes.
- Findings a check's fix leaves alone carry the reason via
  `Diagnostic::unfixable(reason)`; `app` appends `(cannot be fixed:
  reason)` only when `fix` is enabled. `style-line-length` gets its reasons
  from the same `Wrapper` pass that builds the fix edits (`Wrapper::skip`),
  so the reason always matches what the fix did.

## Testing

- All tests live in `craft-tidy/tests/`, one file per module (`glob.rs`,
  `config.rs`, `cli.rs`, `craft.rs`, `project.rs`, `yaml.rs`, `fix.rs`, `checks.rs`,
  `app.rs`), plus
  `example.rs` (the example config) and `binary.rs` (end-to-end runs of the
  compiled binary). Shared helpers are in `tests/common/mod.rs`, included
  with `mod common;`.
- Do not add `#[cfg(test)]` modules to `src/`; test through the public API.
- Every change needs both positive tests (valid input works) and negative
  tests (invalid input is rejected, ideally asserting on the error).
- Each check has its own `tests/check_<name>.rs`. Fix tests assert
  idempotence and use `common::pyyaml_equal` (PyYAML, Snapcraft's parser;
  skipped if unavailable) to confirm the data is unchanged.
- Use `common::FakeCheck` with `app::run_with_checks` / `checks::run` to test
  the framework; use `tempfile::tempdir()` for filesystem tests.
- `examples/.craft-tidy` must contain every top-level `Settings` key and
  every option of every registered check, set to its declared default, and
  nothing else in `check-options`; `tests/example.rs` fails otherwise.
  Update it whenever settings, checks or check options change.

## Exit codes

`0` success, `1` at least one error-severity finding, `2` usage,
configuration, project YAML syntax or I/O error (same as clap's usage error
code).

## Commands

Run from `craft-tidy/`:

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Run from the repository root to build the snap, and to run the spread
tests against it in a VM:

```sh
snapcraft pack
snapcraft test
```

Before finishing a change, ensure `cargo fmt`, `cargo clippy` (no warnings)
and `cargo test` all pass, and that `snapcraft pack` succeeds. Snap builds
take a few minutes; run them in the background while doing other work.
For changes to user-visible behaviour, also run `snapcraft test`.

Keep the spread tasks in `spread/general/` up to date as the tool
develops, in the same commit as the change: a new or changed check gets
assertions in the task of every craft tool it applies to (and none where
it does not apply), a new craft tool gets its own task, and new options,
settings or exit behaviour go in `cli`, `config`, `errors` or `fix`.

## Committing

- Commit as you go, at logical checkpoints: each commit is one
  self-contained step (e.g. a new setting, a new check, a bug fix, a docs
  update), not a dump of a whole session's work.
- Every commit must pass `cargo fmt --check`, `cargo clippy --all-targets --
  -D warnings` and `cargo test`, **and** build the snap with `snapcraft pack`
  from the repository root. This applies to docs-only commits too. Include
  the tests and doc updates (`README.md`, `CONTRIBUTING.md`, `AGENTS.md`,
  `examples/.craft-tidy`) for the code a commit adds.
- To check a series of commits, run the checks on each one, e.g. with
  `git rebase --exec` as shown in `CONTRIBUTING.md`, or in a separate
  worktree so the main checkout is left alone. If a commit fails, fix that
  commit (unpushed history may be rewritten) rather than adding a fix-up on
  top.
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
