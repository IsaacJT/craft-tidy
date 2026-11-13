# Contributing to craft-tidy

Thank you for your interest in contributing to craft-tidy! This document
describes how to set up a development environment and the conventions the
project follows.

## Prerequisites

- A recent stable Rust toolchain (edition 2024, Rust 1.89 or newer), with
  `rustfmt` and `clippy` components
- [Snapcraft](https://snapcraft.io/snapcraft) (only needed to build the snap)
- LXD or Multipass, as required by Snapcraft for managed builds

## Building and testing

All Rust commands are run from the `craft-tidy/` directory:

```sh
cd craft-tidy
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

To build and install the snap, run from the repository root:

```sh
snapcraft pack
sudo snap install --dangerous ./craft-tidy_*.snap
```

### Testing the snap

The [spread](https://github.com/canonical/spread) suite in `spread/general/`
installs the built snap in fresh Ubuntu 24.04 and 26.04 VMs and runs it
end to end, under strict confinement. There is one task per craft tool
(`charmcraft`,
`debcraft`, `imagecraft`, `rockcraft`, `snapcraft`: where the project file
is looked for, findings, `--fix`, and which checks apply), plus tasks for
informational options (`cli`), `--fix` (`fix`), configuration files
(`config`), error handling (`errors`), `NOLINT` comments (`nolint`) and
the language server (`lsp`, driven by a small Python LSP client). Run it from
the repository root (it needs LXD, as `snapcraft pack` does):

```sh
snapcraft test                                # all tasks
snapcraft test craft:ubuntu-26.04:spread/general/fix   # a single task
```

`snapcraft-test.yaml` defines the backend, its systems and the suite.
On CI, `snapcraft test` runs the tasks directly on the runner instead of in
a VM, and only for the system matching the runner's OS, so the CI runner's
system must be among the listed systems;
`spread/.extension` is Snapcraft's stock backend script, changed to wait for
cloud-init before using apt. Each task is a `task.yaml` in its own
directory under `spread/general/`. Tasks create their projects in `$WORK`
(`/root/work`, emptied before and after each task), since the snap can only
reach non-hidden files in home directories, and check output with spread's
`MATCH`/`NOMATCH`. Keep the tasks up to date with every user-visible
change: a new check needs assertions in the task of each tool it applies
to, and support for a new craft tool needs a task of its own.

### Checking every commit

Every commit must pass the cargo checks above and build the snap. To verify
all commits on your branch, run from the repository root (replace `main`
with the branch you are based on):

```sh
git rebase main --exec \
  '(cd craft-tidy && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test) && snapcraft pack'
```

Commits that change user-visible behaviour must also pass `snapcraft test`;
run it on those commits as well (it is slower, as it starts VMs).

The rebase stops at the first commit that fails, so you can fix it in place
with `git commit --amend` and continue with `git rebase --continue`.

## Code structure

The crate is a library (`src/lib.rs`) with a thin binary (`src/main.rs`):

| Module        | Responsibility                                                   |
| ------------- | ---------------------------------------------------------------- |
| `cli`         | Command line definition (clap derive) and flag parsing           |
| `config`      | `Settings` layers, `.craft-tidy` parsing, merging and discovery  |
| `glob`        | clang-tidy style check selection globs                           |
| `nolint`      | Parsing `NOLINT` comments that suppress findings                 |
| `lsp`         | The language server (`craft-tidy lsp`): sync, diagnostics, code actions, hover, configuration reloading, UTF-16 positions |
| `craft`       | The supported craft tools and their project file locations       |
| `project`     | Locating, loading and parsing craft project files                |
| `yaml`        | YAML document tree with source positions, and the `Value` model  |
| `fix`         | Verified text edits used by checks to fix problems               |
| `checks`      | The `Check` trait, the check registry and the check runner       |
| `app`         | Top-level program flow: resolving settings and running actions   |
| `error`       | The crate's error type                                           |

## Tests

All tests live in `craft-tidy/tests/`; do not add `#[cfg(test)]` modules to
the source files. There is one test file per module, and each tests that
module through the crate's public API:

| File               | Covers                                                          |
| ------------------ | --------------------------------------------------------------- |
| `tests/glob.rs`    | `glob`: parsing, matching, precedence, YAML (de)serialization   |
| `tests/nolint.rs`  | `nolint`: directives, check lists, mistakes in comments         |
| `tests/lsp.rs`     | `lsp`: sessions driven over an in-memory connection, positions, URIs |
| `tests/config.rs`  | `config`: parsing, validation, merging, serialization, discovery |
| `tests/cli.rs`     | `cli`: argument parsing and validation                          |
| `tests/craft.rs`   | `craft`: tool names, project file locations, file recognition   |
| `tests/project.rs` | `project`: locating, loading and parsing project files          |
| `tests/yaml.rs`    | `yaml`: tree structure, spans, values, lines, syntax errors     |
| `tests/fix.rs`     | `fix`: applying, verifying and skipping edits                   |
| `tests/check_*.rs` | One file per check, e.g. `check_line_length.rs`: findings, options and fixes |
| `tests/checks.rs`  | `checks`: selection, running, `warnings-as-errors`, options     |
| `tests/app.rs`     | `app`: settings layering, output and every action               |
| `tests/example.rs` | `examples/.craft-tidy` is valid, complete and up to date        |
| `tests/binary.rs`  | End-to-end runs of the compiled binary and its exit codes       |
| `tests/common/`    | Shared helpers, including `FakeCheck` and `pyyaml_equal`        |

Guidelines:

- Test both positive cases (valid input is accepted and behaves correctly)
  and negative cases (invalid input is rejected with a useful error).
- Use `common::FakeCheck` with `app::run_with_checks` or `checks::run` to
  test the framework independently of the real checks.
- Use `tempfile::tempdir()` for anything touching the filesystem.
- Tests of fixes should check that fixing is idempotent and, with
  `common::pyyaml_equal`, that PyYAML (the parser Snapcraft uses) loads the
  same data before and after. That helper returns `None` when Python 3 with
  PyYAML is not installed, so install `python3-yaml` to get the full
  coverage locally.
- `examples/.craft-tidy` must set every configuration key and every option
  of every check, to its default, and nothing else; `tests/example.rs`
  enforces this, so update the example whenever you add a setting, a check
  or a check option.

## Adding a check

1. Pick a kebab-case name prefixed with a category, so that users can
   select related checks with globs such as `style-*`. Reuse an existing
   category (`project-` for the project layout and how parts use it,
   `style-` for how the project
   file is written, such as formatting and redundancy,
   `snapcraft-` for the content of Snapcraft projects)
   where it fits, and document any new category in `README.md`.
2. Implement the `Check` trait from `craft-tidy/src/checks/mod.rs` in a new
   module under `craft-tidy/src/checks/`. `summary()` is shown by
   `--list-checks` and `explanation()` by `--explain`. Checks apply to the
   project files of all craft tools by default; if a check only makes sense
   for some of them, return those from `crafts()` (for example
   `Some(&[Craft::Snapcraft])`), and the runner skips it for other files.
3. Declare the check's options in `options()` as `OptionSpec`s (name, type,
   default, description); craft-tidy rejects any other option and values of
   the wrong type. Implement `validate_option()` for further constraints,
   such as ranges. Read options with
   `ctx.options.get(self.name(), "option-name")`, falling back to the
   declared default when an option is not set.
4. Report findings with `Diagnostic::warning(...)`, adding the byte range
   of the text the finding is about with `.spanning(document, range)`
   (which also sets the line and column; use `.at(line, column)` only if
   there is no meaningful range). Editors underline that range, and fix
   the finding alone by fixing only edits overlapping it. Severity is promoted to error by the
   runner according to `warnings-as-errors`; checks should not do this
   themselves. If the check has a fix but cannot fix a finding, say why
   with `.unfixable(reason)`; the reason is shown for findings that remain
   after `--fix`.
5. If the problems can be fixed automatically, implement `fix()`. Express
   the fix as `fix::Edit`s of the source text and apply them with
   `ctx.apply_edits(NAME, …)`, which skips edits on lines where `NOLINT`
   comments suppress the check and otherwise works like
   `fix::apply_edits`, passing a verification closure that compares the
   data before and after (`Document::value()`): edits that only reformat
   must leave it unchanged, and edits that change data must produce
   exactly the intended result. Edits are verified in batches (the
   closure gets the data of every edit in the batch), so the closure must
   accept any combination of the check's non-overlapping edits. Never
   rewrite the whole file, and never parse the document once per edit:
   to find out in `run()` which edits would be kept, call
   `fix::kept_edits` with all of them.
6. Register the check in `checks::registry()`.
7. Add tests in a new file `craft-tidy/tests/check_<name>.rs` covering
   projects that pass, projects that fail, each option, and for fixable
   checks the fixed output as well as problems that must not be fixed.
8. Document the check, the tools it applies to and its options in
   `README.md` (check table and a section of its own), show its options in
   `examples/.craft-tidy`, and in `AGENTS.md` add its module to the layout
   list and its name to the "Implemented checks" list in the overview.
9. Add assertions for the check to the spread task of every craft tool it
   applies to (`spread/general/<tool>/task.yaml`), and make sure the tasks
   of the other tools do not expect it; for fixes, extend the `fix` task.
10. Run the cargo checks, `snapcraft pack` and `snapcraft test` before
    committing.

## Adding a configuration setting

Settings must be available both in `.craft-tidy` and on the command line:

1. Add the field to `Settings` in `craft-tidy/src/config.rs`. Keys are
   kebab-case in YAML.
2. Define how it merges in `Settings::merge`: glob lists are appended (a
   `*` or `-*` glob replaces the globs before it), while
   scalar settings should be `Option<T>` with later layers overriding earlier
   ones (see `fix`). Give scalar settings a strict deserializer that rejects
   other YAML types, and a value in `Settings::defaults()` so that
   `--dump-config` shows it.
3. Add the matching flag to `Cli` in `craft-tidy/src/cli.rs` and include it in
   `Cli::settings()`.
4. Add tests in `tests/config.rs`, `tests/cli.rs` and `tests/app.rs`, for both
   valid and invalid values.
5. Add an end-to-end test of the flag and the file key in
   `tests/binary.rs`.
6. Add the setting to `examples/.craft-tidy` (enforced by
   `tests/example.rs`), document it in `README.md` (options table and
   configuration section) and update the configuration model in
   `AGENTS.md`.
7. Add assertions for the flag and the `.craft-tidy` key to the `cli` and
   `config` spread tasks, invalid values to `errors`, and anything
   affecting `--fix` to `fix`.
8. Run the cargo checks, `snapcraft pack` and `snapcraft test` before
   committing.

## Dependencies

| Crate          | Purpose                                                       |
| -------------- | ------------------------------------------------------------- |
| `clap`         | Command line parsing, help output                             |
| `serde`        | Deserializing and serializing configuration                   |
| `serde-saphyr` | YAML parsing with line/column error reporting; its re-exported `granit_parser` event parser builds the `yaml` document tree |
| `lsp-server`   | The language server's connection and message loop (from rust-analyzer; synchronous, so no async runtime) |
| `lsp-types`    | Language Server Protocol message types                        |
| `serde_json`   | JSON values of LSP messages                                   |
| `tempfile`     | Temporary directories in tests (dev-dependency only)          |

## The VS Code extension

`editors/vscode/` is a small JavaScript extension (no build step) that
starts `craft-tidy lsp` through `vscode-languageclient`; all features are
in the server. Package it with `npm install && npm run package` (Node.js
22). Its icon is rendered from `snap/gui/craft-tidy.svg` (Noto Emoji,
Apache-2.0), whose license it ships as `LICENSE.noto-emoji`. Keep its list
of project file names in `extension.js` and `package.json` in sync with
`Craft::project_file_locations()`, and its `version` with craft-tidy's.

## Continuous integration

`.github/workflows/ci.yaml` runs on every push to `main`, every pull
request, and on demand (`workflow_dispatch`):

| Job               | What it does                                                                 |
| ----------------- | ---------------------------------------------------------------------------- |
| Format and lint   | `ubuntu-latest`: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`          |
| Test              | `ubuntu-latest`: `cargo test` on stable Rust and on the `rust-version` from `Cargo.toml` (with PyYAML installed, so fixes are cross-checked) |
| Build and test the snap | For amd64 (`ubuntu-24.04` runner) and arm64 (`ubuntu-24.04-arm` runner), each built natively: [`canonical/craft-actions/snapcraft/pack`](https://github.com/canonical/craft-actions/tree/main/snapcraft/pack) packs the snap and runs `snapcraft test`; the snaps are uploaded as the `craft-tidy-snap-amd64` and `craft-tidy-snap-arm64` artifacts |
| Package the VS Code extension | `npm ci` and `npm run package` in `editors/vscode/`; the `.vsix` is uploaded as the `craft-tidy-vscode` artifact |
| Publish to edge   | Pushes to `main` only, after all other jobs pass: [`snapcore/action-publish`](https://github.com/snapcore/action-publish) uploads both snaps to the Snap Store and releases them to `edge` |

When raising `rust-version`, update the matrix in the workflow too. When
adding an architecture, add a runner of that architecture to the snap
matrix and the architecture to the publish matrix. `snap/snapcraft.yaml`
has no `platforms` key on purpose: the snap builds for whatever
architecture it is built on, and CI builds amd64 and arm64 by building on
runners of each.

Publishing needs, once, in the repository settings on GitHub:

1. The `craft-tidy` name registered in the Snap Store
   (`snapcraft register craft-tidy`).
2. An environment named `snap-store` (Settings → Environments), which the
   publish job uses; add protection rules there if releases should need
   approval.
3. A `SNAPCRAFT_STORE_CREDENTIALS` secret in that environment, created
   with:

   ```sh
   snapcraft export-login --snaps=craft-tidy --channels=edge \
     --acls=package_access,package_push,package_update,package_release -
   ```

Pull requests (including from forks) never publish and have no access to
the secret. Runs on `main` are not cancelled by newer pushes, so a publish
is never interrupted.

## Submitting changes

1. Fork the repository and create a topic branch from `main`.
2. Keep each change focused; unrelated changes belong in separate pull
   requests.
3. Make sure the code is formatted, passes `clippy` without warnings, and
   that all tests pass; the snap must build and, for user-visible changes,
   pass `snapcraft test` too (see step 6). If you cannot run `snapcraft
   pack` or `snapcraft test` (they need LXD or Multipass), say so in the
   pull request; CI runs both.
4. Add positive and negative tests in `craft-tidy/tests/` for new features
   and bug fixes.
5. Update the documentation (`README.md`, `CONTRIBUTING.md`, `AGENTS.md`)
   whenever behaviour, usage, layout or workflow changes.
6. Split work into commits at logical checkpoints, one self-contained step
   per commit. Every commit must pass `cargo fmt --check`, `cargo clippy`
   and `cargo test`, build the snap with `snapcraft pack` (see
   [Checking every commit](#checking-every-commit)), pass `snapcraft test`
   if it changes user-visible behaviour (see
   [Testing the snap](#testing-the-snap)), and include the tests
   and documentation for the code it adds, so that any commit can be
   checked out, bisected or reverted on its own.
7. Write clear commit messages: a short summary line (50 characters or
   fewer), a blank line, then a description of what changed and why.
8. Sign off your commits (`git commit -s`) to certify the
   [Developer Certificate of Origin](https://developercertificate.org/).

## Coding guidelines

- Follow standard Rust style as enforced by `rustfmt`.
- Prefer returning errors over panicking; avoid `unwrap()`/`expect()` outside
  of tests unless the invariant is obvious and documented.
- Keep dependencies to a minimum and justify new ones in the pull request.
- Every source file starts with the SPDX header:

  ```rust
  // SPDX-License-Identifier: GPL-3.0-only
  ```

## Versioning

The version is declared in two places, which must be kept in sync:

- `craft-tidy/Cargo.toml` (`package.version`)
- `snap/snapcraft.yaml` (`version`)

## License

By contributing, you agree that your contributions will be licensed under the
GNU General Public License, version 3 only (`GPL-3.0-only`). See
[LICENSE](LICENSE).
