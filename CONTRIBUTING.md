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

## Code structure

The crate is a library (`src/lib.rs`) with a thin binary (`src/main.rs`):

| Module        | Responsibility                                                   |
| ------------- | ---------------------------------------------------------------- |
| `cli`         | Command line definition (clap derive) and flag parsing           |
| `config`      | `Settings` layers, `.craft-tidy` parsing, merging and discovery  |
| `glob`        | clang-tidy style check selection globs                           |
| `project`     | Locating and loading the `snapcraft.yaml` file                   |
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
| `tests/config.rs`  | `config`: parsing, validation, merging, serialization, discovery |
| `tests/cli.rs`     | `cli`: argument parsing and validation                          |
| `tests/project.rs` | `project`: locating and loading `snapcraft.yaml`                |
| `tests/checks.rs`  | `checks`: selection, running, `warnings-as-errors`, options     |
| `tests/app.rs`     | `app`: settings layering, output and every action               |
| `tests/binary.rs`  | End-to-end runs of the compiled binary and its exit codes       |
| `tests/common/`    | Shared helpers, including `FakeCheck`                           |

Guidelines:

- Test both positive cases (valid input is accepted and behaves correctly)
  and negative cases (invalid input is rejected with a useful error).
- Use `common::FakeCheck` with `app::run_with_checks` or `checks::run` to
  test the framework independently of the real checks.
- Use `tempfile::tempdir()` for anything touching the filesystem.

## Adding a check

1. Pick a kebab-case name prefixed with a category, e.g.
   `metadata-summary-length`, so that users can select it with globs such as
   `metadata-*`.
2. Implement the `Check` trait from `craft-tidy/src/checks/mod.rs` in a new
   module under `craft-tidy/src/checks/`. `summary()` is shown by
   `--list-checks` and `explanation()` by `--explain`.
3. Read check options with `ctx.options.get(self.name(), "option-name")` and
   fall back to a sensible default when an option is not set.
4. Report findings with `Diagnostic::warning(...)`, adding a location with
   `.at(line, column)` where possible. Severity is promoted to error by the
   runner according to `warnings-as-errors`; checks should not do this
   themselves.
5. Register the check in `checks::registry()`.
6. Add tests in a new file `craft-tidy/tests/check_<name>.rs` covering
   projects that pass, projects that fail, and each option.
7. Document the check and its options in `README.md`.

## Adding a configuration setting

Settings must be available both in `.craft-tidy` and on the command line:

1. Add the field to `Settings` in `craft-tidy/src/config.rs`. Keys are
   kebab-case in YAML.
2. Define how it merges in `Settings::merge`: glob lists are appended, while
   scalar settings should be `Option<T>` with later layers overriding earlier
   ones.
3. Add the matching flag to `Cli` in `craft-tidy/src/cli.rs` and include it in
   `Cli::settings()`.
4. Add tests in `tests/config.rs`, `tests/cli.rs` and `tests/app.rs`, for both
   valid and invalid values.
5. Document the setting in `README.md`.

## Dependencies

| Crate          | Purpose                                                       |
| -------------- | ------------------------------------------------------------- |
| `clap`         | Command line parsing, help output                             |
| `serde`        | Deserializing and serializing configuration                   |
| `serde-saphyr` | YAML parsing with line/column error reporting                 |
| `tempfile`     | Temporary directories in tests (dev-dependency only)          |

## Submitting changes

1. Fork the repository and create a topic branch from `main`.
2. Keep each change focused; unrelated changes belong in separate pull
   requests.
3. Make sure the code is formatted, passes `clippy` without warnings, and
   that all tests pass.
4. Add positive and negative tests in `craft-tidy/tests/` for new features
   and bug fixes.
5. Update the documentation (`README.md`, `CONTRIBUTING.md`, `AGENTS.md`)
   whenever behaviour, usage, layout or workflow changes.
6. Write clear commit messages: a short summary line (50 characters or
   fewer), a blank line, then a description of what changed and why.
7. Sign off your commits (`git commit -s`) to certify the
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
