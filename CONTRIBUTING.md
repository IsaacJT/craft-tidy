# Contributing to craft-tidy

Thank you for your interest in contributing to craft-tidy! This document
describes how to set up a development environment and the conventions the
project follows.

## Prerequisites

- A recent stable Rust toolchain (edition 2024, Rust 1.85 or newer), with
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

## Submitting changes

1. Fork the repository and create a topic branch from `main`.
2. Keep each change focused; unrelated changes belong in separate pull
   requests.
3. Make sure the code is formatted, passes `clippy` without warnings, and
   that all tests pass.
4. Add tests for new lint rules and bug fixes.
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
