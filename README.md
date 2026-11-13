# craft-tidy

craft-tidy is a linter for [Snapcraft](https://snapcraft.io) projects. It
inspects `snapcraft.yaml` files and reports common mistakes, deprecated
constructs and deviations from packaging best practices, so problems can be
caught before running a (potentially lengthy) snap build.

craft-tidy is written in Rust and is itself distributed as a snap.

> **Status:** early development. The project scaffolding is in place, but no
> lint rules have been implemented yet.

## Installation

### From source (snap)

```sh
snapcraft pack
sudo snap install --dangerous ./craft-tidy_*.snap
```

### From source (cargo)

```sh
cargo install --path craft-tidy
```

## Usage

```sh
craft-tidy
```

The strictly confined snap uses the `home` interface, so it can read projects
located in your home directory.

## Repository layout

| Path                  | Description                              |
| --------------------- | ---------------------------------------- |
| `craft-tidy/`         | Rust crate containing the application    |
| `snap/snapcraft.yaml` | Snap packaging for craft-tidy itself     |
| `LICENSE`             | GNU General Public License, version 3    |

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
details on setting up a development environment and submitting changes.

## License

craft-tidy is licensed under the GNU General Public License, version 3 only
(`GPL-3.0-only`). See [LICENSE](LICENSE) for the full text.
