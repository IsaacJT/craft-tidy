# craft-tidy

[![craft-tidy](https://snapcraft.io/craft-tidy/badge.svg)](https://snapcraft.io/craft-tidy)

craft-tidy is a linter for the project files of Canonical's craft tools:
`snapcraft.yaml` ([Snapcraft](https://snapcraft.io)), `charmcraft.yaml`
(Charmcraft), `rockcraft.yaml` (Rockcraft), `imagecraft.yaml` (Imagecraft)
and `debcraft.yaml` (Debcraft). It reports common mistakes, deprecated
constructs and deviations from packaging best practices, so problems can be
caught before running a (potentially lengthy) build.

craft-tidy is written in Rust and is itself distributed as a snap.

> **Status:** early development. The command line interface, configuration
> system, check framework and automatic fixes are in place, and the first
> checks are available (see [Checks](#checks)).

## Installation

### From the Snap Store (edge)

Every commit on `main` that passes CI is published to the `edge` channel,
for amd64 and arm64:

```sh
sudo snap install craft-tidy --edge
```

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
of a project file. In a directory, craft-tidy lints the project file of
every craft tool it finds there, one after the other (in alphabetical order
of the tools), each in the place the tool itself reads it from:

| Tool       | Project file locations, in search order                                                  |
| ---------- | ---------------------------------------------------------------------------------------- |
| Charmcraft | `charmcraft.yaml`                                                                        |
| Debcraft   | `debcraft.yaml`                                                                          |
| Imagecraft | `imagecraft.yaml`                                                                        |
| Rockcraft  | `rockcraft.yaml`                                                                         |
| Snapcraft  | `snapcraft.yaml`, `snap/snapcraft.yaml`, `build-aux/snap/snapcraft.yaml`, `.snapcraft.yaml` |

Only the first file found for a tool is used, as the tool itself would. The
tool a file belongs to is recognised from its name; a file given explicitly
with another name is linted with the checks that apply to all tools.

All project files are loaded before any is linted: if one of them cannot be
read or is not valid YAML, craft-tidy reports that error, lints none of
them and exits with status 2. If no project file is found, craft-tidy also
exits with status 2.

Findings are printed one per line as
`path:line:column: severity: message [check-name]`, or
`path: severity: message [check-name]` for findings about the file as a
whole (such as its location).

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
to standard error; if nothing could be changed, no summary is printed. The
exit status reflects the remaining problems only.

Fixes are careful edits of the original text, so formatting and comments
elsewhere in the file are preserved. Every edit is checked by parsing the
result again: an edit is only kept if the file is still valid YAML and its
data changed exactly as intended (for example, a reformatted value must
still have the same content). Problems that cannot be fixed safely are left
alone and reported, where known with the reason they could not be fixed
(the reason is only shown when fixing is enabled, by `--fix` or by `fix:
true` in `.craft-tidy`):

```text
snap/snapcraft.yaml:12:81: warning: line is 95 characters long, exceeding the limit of 80 (cannot be fixed: it has no space at which it can be broken safely, such as in a long URL or word) [style-line-length]
```

Only enabled checks fix problems, so `--checks` also limits what `--fix`
changes.

### Suppressing findings

Findings can be suppressed with `NOLINT` comments in the project file, as
with clang-tidy's `// NOLINT` comments, written as ordinary YAML comments:

| Comment | Suppresses findings on |
| ------- | ---------------------- |
| `# NOLINT` | the comment's own line, whether it ends a line of YAML or is on a line of its own |
| `# NOLINTNEXTLINE` | the line after the comment |
| `# NOLINTBEGIN` … `# NOLINTEND` | every line from the `NOLINTBEGIN` comment to the `NOLINTEND` comment |

By default every check is suppressed. To suppress only some checks, list
them in parentheses directly after the keyword, separated by commas; globs
work as in `--checks`. Any text may follow, such as the reason:

```yaml
# NOLINTNEXTLINE(style-line-length): the URL cannot be shortened
source: https://example.com/a/very/long/url/that/cannot/be/broken/anywhere.tar.gz
plugs: [network, home] # NOLINT(style-sorted-lists, style-collapsed-*)
# NOLINTBEGIN(snapcraft-components)
parts:
  ...
# NOLINTEND(snapcraft-components)
```

- A `NOLINTEND` closes the most recent `NOLINTBEGIN` with the same check
  list (in any order). Ranges may nest and overlap.
- A finding is suppressed if the line it is reported on (as printed) is
  covered. Findings about the file as a whole, which have no line (such as
  `project-file-location`), cannot be suppressed this way; disable the
  check in `.craft-tidy` instead.
- `--fix` leaves suppressed lines alone: a fix that would change a line on
  which its check is suppressed is not made. `NOLINT` comments are never
  moved or wrapped by fixes.
- Mistakes in `NOLINT` comments are reported as `craft-tidy-nolint`
  warnings: an unclosed `(`, an empty `()`, an invalid glob, a `-` glob
  (which cannot be used to un-suppress), a `NOLINTEND` without a matching
  `NOLINTBEGIN`, and a `NOLINTBEGIN` without a matching `NOLINTEND` (which
  suppresses nothing). These are always reported, whatever `--checks`
  selects, and can be made errors with `--warnings-as-errors`.
- The keywords are case-sensitive, and must start the comment
  (`# NOLINT`, not `# see NOLINT`).

### Exit status

| Code | Meaning                                                    |
| ---- | ---------------------------------------------------------- |
| 0    | No errors were found (warnings may have been reported)     |
| 1    | At least one finding was reported as an error              |
| 2    | Invalid usage, invalid configuration, no project file found, a project file that is not valid YAML, or an I/O error |

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

Selection starts from the default `*` (all checks), and globs are added to
it, so a positive glob on its own changes nothing: `--checks=style-*` still
runs every check. To run only some checks, start with `-*`, as in
`--checks='-*,style-*'`.

### Precedence

Settings are combined from these sources, each overriding the previous ones:

1. Built-in defaults
2. The `.craft-tidy` file (or the file given with `--config-file`)
3. Inline YAML given with `--config`
4. Individual command line flags

Glob lists (`checks`, `warnings-as-errors`) from later sources are appended
to earlier ones, so `--checks=-style-sorted-lists` disables
`style-sorted-lists` on top of whatever the configuration file enables. A
`*` or `-*` glob matches every check, so it replaces all globs before it:
with `checks: "-*"` in `.craft-tidy` and `--checks=style-sorted-lists`,
the effective value is `-*,style-sorted-lists`. Other settings, such as `fix`, are
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

| Check                   | Applies to | Fixable | Description                                               |
| ----------------------- | ---------- | ------- | --------------------------------------------------------- |
| `project-file-location` | Snapcraft  | No      | The project file must be in a `snap/` directory (`snap/` or `build-aux/snap/`) |
| `snapcraft-components`  | Snapcraft  | No      | Components used by parts must be defined under `components` |
| `style-collapsed-sequences` | All tools | Yes | Short sequences must be on one line, longer ones one item per line |
| `style-line-length`     | All tools  | Partly  | Lines must not be longer than `max-length` characters     |
| `style-sorted-lists`    | All tools  | Yes     | Entries of unordered lists must be sorted alphabetically  |

`--list-checks` marks checks that only apply to some tools, and `--explain`
names them.

Check names start with their category, so whole categories can be selected
with globs such as `--checks='-*,style-*'`: `project-` checks are about the
layout of the project, `style-` checks about the formatting of the project
file, and `snapcraft-` checks about the content of Snapcraft projects.

Run `craft-tidy --list-checks --checks='*'` to list all checks and
`craft-tidy --explain CHECK` for details.

### project-file-location

Snapcraft only. Reports a project file named `snapcraft.yaml` or
`.snapcraft.yaml` that is not in a directory named `snap`, typically
because it is in the root of the project (the same applies to a file given
explicitly on the command line, wherever it is). Keep it at
`snap/snapcraft.yaml` instead, next to the
other snap-specific files such as `snap/hooks`, `snap/gui` and `snap/local`,
so that the packaging stays apart from the application's own files.
`build-aux/snap/snapcraft.yaml`, which Snapcraft supports for projects that
use `snap/` for something else, is accepted too.

Moving the file does not change how the snap is built, as Snapcraft
resolves paths such as `source: .` relative to the project root. The file is
not moved by `--fix`; move it yourself, for example with
`git mv snapcraft.yaml snap/snapcraft.yaml`. The other craft tools only read
their project file from the project root, so the check does not apply to
them.

### snapcraft-components

Snapcraft only. Reports [components](https://snapcraft.io/docs/explanation/how-snaps-work/snap-components/)
that parts use but that are not defined under the top-level `components`
key. Snapcraft rejects such projects, or for environment variables, the
variable is silently empty. A part uses a component through:

- a `(component/<name>)` path in its `organize` values, or its `stage` or
  `prime` entries (including excluded `-(component/<name>)/…` entries and
  entries inside advanced grammar);
- a `$CRAFT_COMPONENT_<NAME>_STAGE`, `_PRIME` or `_OVERLAY` variable,
  where `<NAME>` is the component name in uppercase with `-` replaced by
  `_`;
- `craftctl set components.<name>.<key>=…` (or `craftctl get`) in its
  scripts.

The closest defined name is suggested for likely typos:

```text
snap/snapcraft.yaml:12:22: warning: component 'translatons' is not defined under components; did you mean 'translations'? [snapcraft-components]
```

Components that are defined but that no part puts files in (through a path
or a variable; setting metadata with `craftctl` does not count) are
reported too, as they would be packed empty. Turn this off with
`report-unused: false`. These problems are not fixed by `--fix`.

| Option          | Type    | Default | Description |
| --------------- | ------- | ------- | ----------- |
| `report-unused` | boolean | `true`  | Also report defined components that no part puts files in |

### style-collapsed-sequences

Reports sequences of strings that are written in the wrong style for their
size: sequences with at most `max-collapsed-items` items (default: 1) must
be written on one line, and longer ones one item per line:

```yaml
plugs: [home]
build-packages:
  - cmake
  - curl
```

A short sequence stays one item per line, and is not reported, if writing
it on one line would make the line longer than `style-line-length`'s
`max-length`, so the two checks never ask for opposite changes.

Only sequences that are the value of a key and contain nothing but
single-line strings are checked. Sequences with comments (including on the
key's line), anchors, tags or aliases, nested lists or mappings (such as
advanced grammar entries), multi-line strings, and empty sequences are
left alone, as are strings that cannot be put in `[...]` without changing
them (such as `a, b`).

With `--fix`, sequences are rewritten in the right style. Sequences written
one item per line use the same indentation as the other such sequences in
the file (two spaces past the key if there are none).

| Option                | Type    | Default | Description |
| --------------------- | ------- | ------- | ----------- |
| `max-collapsed-items` | integer | `1`     | Largest number of items written on one line; `0` puts every sequence one item per line |

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
wrapped safely and are only reported. With `--fix`, each line that is
still too long says why it could not be wrapped, for example `it has no
space at which it can be broken safely, such as in a long URL or word` or
`lines of literal (|) block scalars cannot be wrapped without changing
them`.

| Option       | Type    | Default | Description                         |
| ------------ | ------- | ------- | ----------------------------------- |
| `max-length` | integer | `80`    | Maximum number of characters per line |

### style-sorted-lists

Reports lists whose entries are not in alphabetical order (ignoring case
by default). Only lists under the keys selected by the `keys` option are
checked. By default these are keys whose lists are unordered sets for the
craft tools (mostly part keys shared by all of them), so sorting them never changes the meaning of the project:
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
| `snap/gui/`            | Snap icon (Noto Emoji star, Apache-2.0) and its license |
| `snapcraft-test.yaml`, `spread/` | Spread tests of the snap, run with `snapcraft test` |
| `.github/workflows/`   | CI: cargo checks and tests, snap build and spread tests |
| `LICENSE`              | GNU General Public License, version 3    |

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
details on setting up a development environment and submitting changes.

## License

craft-tidy is licensed under the GNU General Public License, version 3 only
(`GPL-3.0-only`). See [LICENSE](LICENSE) for the full text.

The snap's icon, `snap/gui/craft-tidy.svg`, is the star emoji (U+2B50)
from [Noto Emoji](https://github.com/googlefonts/noto-emoji) (file
`2D/svg/emoji_u2b50.svg`), copyright 2013 Google, Inc. and licensed under
the Apache License, Version 2.0. Its license is in
[snap/gui/LICENSE.noto-emoji](snap/gui/LICENSE.noto-emoji) and is also
shipped in the snap.
