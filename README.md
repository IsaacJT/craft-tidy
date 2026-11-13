# craft-tidy

craft-tidy is a linter for [Snapcraft](https://snapcraft.io) projects. It
inspects `snapcraft.yaml` files and reports common mistakes, deprecated
constructs and deviations from packaging best practices, so problems can be
caught before running a (potentially lengthy) snap build.

craft-tidy is written in Rust and is itself distributed as a snap.

> **Status:** early development. The command line interface, configuration
> system, check framework and automatic fixes are in place, and the first
> checks are available (see [Checks](#checks)).

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
in the same locations and order as Snapcraft does, so that craft-tidy checks
the file Snapcraft would build: `snapcraft.yaml`, `snap/snapcraft.yaml`,
`build-aux/snap/snapcraft.yaml` and `.snapcraft.yaml`.

Findings are printed one per line as
`path:line:column: severity: message [check-name]`.

| Option                                 | Description                                                         |
| -------------------------------------- | ------------------------------------------------------------------- |
| `--checks GLOBS`                       | Select the checks to run (see [Selecting checks](#selecting-checks)) |
| `--warnings-as-errors GLOBS`           | Report findings from matching checks as errors                      |
| `--fix`                                | Fix problems where possible (see [Fixing problems](#fixing-problems)) |
| `--no-fix`                             | Do not fix problems, even if `fix: true` is configured              |
| `-o`, `--check-option CHECK.KEY=VALUE` | Set a check option, e.g. `-o style-line-length.max-length=100`; may be repeated |
| `--config YAML`                        | Inline configuration, in the same format as `.craft-tidy`           |
| `--config-file PATH`                   | Use PATH instead of searching for `.craft-tidy`                     |
| `--no-config`                          | Do not read any configuration file                                  |
| `--dump-config`                        | Print the effective configuration and exit                          |
| `--list-checks`                        | List the enabled checks and exit (add `--checks='*'` to list all)   |
| `--explain CHECK`                      | Describe a check and its options, and exit                          |

Check option values are interpreted as YAML scalars: `79` is an integer,
`true` a boolean and `"'79'"` (quoted) a string.

Run `craft-tidy --help` for the full reference.

### Fixing problems

With `--fix` (or `fix: true` in `.craft-tidy`), craft-tidy fixes the
problems it can and rewrites the project file in place. It then reports the
problems that remain, and prints a summary such as
`craft-tidy: applied fixes to snap/snapcraft.yaml (5 issue(s) found, 1 remaining)`
to standard error. The exit status reflects the remaining problems only.

Fixes are careful edits of the original text, so formatting and comments
elsewhere in the file are preserved. Every edit is checked by parsing the
result again: an edit is only kept if the file is still valid YAML and its
data changed exactly as intended (for example, a reformatted value must
still have the same content). Problems that cannot be fixed safely are left
alone and reported. Only enabled checks fix problems, so `--checks` also
limits what `--fix` changes.

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
checks: "*,-style-sorted-lists"
warnings-as-errors: "project-*"
fix: true
check-options:
  style-line-length:
    max-length: 100
```

A fully commented example covering every key is available in
[`examples/.craft-tidy`](examples/.craft-tidy); copy it into your project to
get started.

| Key                  | Description                                                          |
| -------------------- | -------------------------------------------------------------------- |
| `checks`             | Globs selecting the checks to run. Default: `"*"` (all checks)       |
| `warnings-as-errors` | Globs selecting checks whose findings are errors. Default: `""`      |
| `fix`                | Fix problems where possible, like `--fix`. Default: `false`          |
| `check-options`      | Options for individual checks, as `check-name: {option: value}`      |

Glob lists can be written either as a comma-separated string or as a YAML
list of strings; an empty value means an empty list. Check option values
must be booleans, integers, floats or strings. Unknown keys, duplicate keys
and invalid values are rejected with the location of the problem.

### Check options

Each check declares the options it accepts, with their types and defaults;
`craft-tidy --explain CHECK` lists them. Setting an option the check does
not have, or a value of the wrong type, is an error (exit status 2), so
typos do not go unnoticed. Options for checks that do not exist only produce
a warning, so that a configuration written for a newer craft-tidy still
works with an older one.

### Selecting checks

Checks are selected with comma-separated globs, as in clang-tidy. `*`
matches any sequence of characters and a leading `-` disables the matching
checks. Globs are evaluated in order and the last one matching a check
decides whether it is enabled. For example, `-*,style-*,-style-sorted-lists`
enables all `style-*` checks except `style-sorted-lists`.

### Precedence

Settings are combined from these sources, each overriding the previous ones:

1. Built-in defaults
2. The `.craft-tidy` file (or the file given with `--config-file`)
3. Inline YAML given with `--config`
4. Individual command line flags

Glob lists (`checks`, `warnings-as-errors`) from later sources are appended
to earlier ones, so `--checks=-style-sorted-lists` disables
`style-sorted-lists` on top of
whatever the configuration file enables. Other settings, such as `fix`, are
replaced by later sources, and check options are overridden individually.
Use `--dump-config` to see the result; its output is itself a valid
`.craft-tidy` file.

### Snap confinement

The strictly confined snap uses the `home` interface, so it can read (and,
with `--fix`, rewrite) projects located in your home directory. That
interface does not give access
to hidden files directly in your home directory, so a `~/.craft-tidy` file
is ignored by the snap; place `.craft-tidy` in the project directory (or a
non-hidden parent directory) instead.

## Checks

| Check                   | Fixable | Description                                                    |
| ----------------------- | ------- | -------------------------------------------------------------- |
| `project-file-location` | No      | The project file must be in `snap/`, not the project root      |
| `style-line-length`     | Partly  | Lines must not be longer than `max-length` characters          |
| `style-sorted-lists`    | Yes     | Entries of unordered lists must be sorted alphabetically       |

Check names start with their category, so whole categories can be selected
with globs such as `--checks='-*,style-*'`: `project-` checks are about the
layout of the project, `style-` checks about the formatting of the project
file.

Run `craft-tidy --list-checks --checks='*'` to list all checks and
`craft-tidy --explain CHECK` for details.

### project-file-location

Reports a project file named `snapcraft.yaml` or `.snapcraft.yaml` in the
root of the project. Keep it at `snap/snapcraft.yaml` instead, next to the
other snap-specific files such as `snap/hooks`, `snap/gui` and `snap/local`,
so that the packaging stays apart from the application's own files.
`build-aux/snap/snapcraft.yaml`, which Snapcraft supports for projects that
use `snap/` for something else, is accepted too.

Moving the file does not change how the snap is built, as Snapcraft
resolves paths such as `source: .` relative to the project root. The file is
not moved by `--fix`; move it yourself, for example with
`git mv snapcraft.yaml snap/snapcraft.yaml`.

### style-line-length

Reports every line longer than `max-length` characters (default: 80),
counting characters rather than bytes.

With `--fix`, long lines are wrapped where this provably does not change
the data:

- Plain and quoted strings continue on further indented lines, which YAML
  joins back with single spaces. Breaks are only made at single spaces, and
  never where a continuation line could look like YAML syntax (for example
  before `- ` or `&`). A trailing comment stays at the end of the last line.
- Lines of folded (`>`) block scalars are broken at spaces. More-indented
  lines are left alone, since YAML does not fold them.
- Flow sequences such as `plugs: [a, b, c]` become block sequences, one item
  per line; a trailing comment moves to the key's line.
- Comments on their own line that look like prose (`# text`) are re-wrapped.

Literal (`|`) block scalars such as `override-build` scripts, long words
such as URLs, mapping keys and anything inside flow mappings cannot be
wrapped safely and are only reported.

| Option       | Type    | Default | Description                         |
| ------------ | ------- | ------- | ----------------------------------- |
| `max-length` | integer | `80`    | Maximum number of characters per line |

### style-sorted-lists

Reports lists whose entries are not in alphabetical order (ignoring case
by default). Only lists under the keys selected by the `keys` option are
checked. By default these are keys whose lists are unordered sets for
Snapcraft, so sorting them never changes the meaning of the project:
`after`, `assumes`, `before`, `build-attributes`, `build-for`, `build-on`,
`build-packages`, `build-snaps`, `plugs`, `slots`, `stage-packages` and
`stage-snaps`, wherever they appear. Lists whose order matters, such as
`command-chain` or build tool arguments like `cmake-parameters`, are not
checked unless you add them.

[Advanced grammar](https://documentation.ubuntu.com/snapcraft/latest/reference/advanced-grammar/)
entries (`on …:`, `to …:`, `on … to …:`, `for …:`, `try:`, `else:` and
`else fail`, recognised the same way as craft-grammar does) must come after
all plain entries. They keep their original order, so that each `else`
stays with the statement it belongs to, and the lists inside them must be
sorted too:

```yaml
build-packages:
  - curl
  - zlib1g-dev
  - on arm64:
      - libfoo-arm
  - else:
      - libfoo
```

Lists that contain other kinds of entries (such as mappings or anchors), or
an `else` that does not directly follow another grammar entry, are not
checked.

With `--fix`, the entries are reordered. Whole entries move, including any
nested content, trailing comments, and comment lines directly above them.
Comments above the first entry are kept in place as a header for the whole
list. Flow lists such as `[b, a]` keep their formatting; a flow list with
comments between its entries is reported but not reordered.

| Option           | Type    | Default        | Description |
| ---------------- | ------- | -------------- | ----------- |
| `keys`           | string  | see above      | Globs selecting the keys whose lists must be sorted, in the same syntax as `--checks`, e.g. `"*,-command-chain"` |
| `case-sensitive` | boolean | `false`        | Sort uppercase letters before lowercase ones instead of ignoring case |

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
