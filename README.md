# craft-tidy

craft-tidy is a linter for [Snapcraft](https://snapcraft.io) projects. It
inspects `snapcraft.yaml` files and reports common mistakes, deprecated
constructs and deviations from packaging best practices, so problems can be
caught before running a (potentially lengthy) snap build.

craft-tidy is written in Rust and is itself distributed as a snap.

> **Status:** early development. The command line interface, configuration
> system and check framework are in place, but no lint checks have been
> implemented yet.

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
craft-tidy [OPTIONS] [PATH]
```

`PATH` is a project directory (default: the current directory) or the path
of a `snapcraft.yaml` file. In a directory, the project file is searched for
in the same locations as Snapcraft uses: `snap/snapcraft.yaml`,
`snapcraft.yaml`, `.snapcraft.yaml` and `build-aux/snap/snapcraft.yaml`.

Findings are printed one per line as
`path:line:column: severity: message [check-name]`.

| Option                                 | Description                                                         |
| -------------------------------------- | ------------------------------------------------------------------- |
| `--checks GLOBS`                       | Select the checks to run (see [Selecting checks](#selecting-checks)) |
| `--warnings-as-errors GLOBS`           | Report findings from matching checks as errors                      |
| `-o`, `--check-option CHECK.KEY=VALUE` | Set a check option, e.g. `-o metadata-summary-length.max-length=79`; may be repeated |
| `--config YAML`                        | Inline configuration, in the same format as `.craft-tidy`           |
| `--config-file PATH`                   | Use PATH instead of searching for `.craft-tidy`                     |
| `--no-config`                          | Do not read any configuration file                                  |
| `--dump-config`                        | Print the effective configuration and exit                          |
| `--list-checks`                        | List the enabled checks and exit (add `--checks='*'` to list all)   |
| `--explain CHECK`                      | Describe a check and its options, and exit                          |

Check option values are interpreted as YAML scalars: `79` is an integer,
`true` a boolean and `"'79'"` (quoted) a string.

Run `craft-tidy --help` for the full reference.

### Exit status

| Code | Meaning                                                    |
| ---- | ---------------------------------------------------------- |
| 0    | No errors were found (warnings may have been reported)     |
| 1    | At least one finding was reported as an error              |
| 2    | Invalid usage, invalid configuration, a project file that is not valid YAML, or an I/O error |

## Configuration

craft-tidy can be configured with command line flags or with a YAML file
named `.craft-tidy`. The file is looked for in the project directory and then
in each of its parent directories; the first one found is used.

```yaml
# .craft-tidy
checks: "-*,metadata-*,parts-*,-parts-deprecated-plugin"
warnings-as-errors: "metadata-*"
check-options:
  metadata-summary-length:
    max-length: 79
```

A fully commented example covering every key is available in
[`examples/.craft-tidy`](examples/.craft-tidy); copy it into your project to
get started.

| Key                  | Description                                                          |
| -------------------- | -------------------------------------------------------------------- |
| `checks`             | Globs selecting the checks to run. Default: `"*"` (all checks)       |
| `warnings-as-errors` | Globs selecting checks whose findings are errors. Default: `""`      |
| `check-options`      | Options for individual checks, as `check-name: {option: value}`      |

Glob lists can be written either as a comma-separated string or as a YAML
list of strings; an empty value means an empty list. Check option values
must be booleans, integers, floats or strings. Unknown keys, duplicate keys
and invalid values are rejected with the location of the problem.

### Selecting checks

Checks are selected with comma-separated globs, as in clang-tidy. `*`
matches any sequence of characters and a leading `-` disables the matching
checks. Globs are evaluated in order and the last one matching a check
decides whether it is enabled. For example, `-*,parts-*,-parts-foo` enables
all `parts-*` checks except `parts-foo`.

### Precedence

Settings are combined from these sources, each overriding the previous ones:

1. Built-in defaults
2. The `.craft-tidy` file (or the file given with `--config-file`)
3. Inline YAML given with `--config`
4. Individual command line flags

Glob lists (`checks`, `warnings-as-errors`) from later sources are appended
to earlier ones, so `--checks=-parts-foo` disables `parts-foo` on top of
whatever the configuration file enables. Check options are overridden
individually. Use `--dump-config` to see the result; its output is itself a
valid `.craft-tidy` file.

### Snap confinement

The strictly confined snap uses the `home` interface, so it can read
projects located in your home directory. That interface does not give access
to hidden files directly in your home directory, so a `~/.craft-tidy` file
is ignored by the snap; place `.craft-tidy` in the project directory (or a
non-hidden parent directory) instead.

## Repository layout

| Path                   | Description                              |
| ---------------------- | ---------------------------------------- |
| `craft-tidy/`          | Rust crate containing the application    |
| `craft-tidy/tests/`    | Test suite                               |
| `examples/.craft-tidy` | Example configuration file               |
| `snap/snapcraft.yaml`  | Snap packaging for craft-tidy itself     |
| `LICENSE`              | GNU General Public License, version 3    |

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
details on setting up a development environment and submitting changes.

## License

craft-tidy is licensed under the GNU General Public License, version 3 only
(`GPL-3.0-only`). See [LICENSE](LICENSE) for the full text.
