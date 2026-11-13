#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# Generates the spread tasks of the craft tools,
# spread/general/<tool>/task.yaml; edit this script rather than those files,
# and run it again (from anywhere) after changing it.
#
# Every project is a realistic, conforming project file for its tool (with a
# license, except for Charmcraft, which does not accept one), so that all
# checks run, with no .craft-tidy file, and each section changes exactly
# one thing. Expected output, including lines and columns, is computed from
# the fixtures and compared exactly.

import os
import textwrap

LONG_SUMMARY = "summary: A summary that is far too long for a single line of eighty characters here"
WRAPPED_SUMMARY = "summary: A summary that is far too long for a single line of eighty characters\n  here"
URL_COMMENT = "# https://example.com/a/very/long/url/that/cannot/be/broken/anywhere/at/all.tar.gz"
NO_BREAK = "it has no space at which it can be broken safely, such as in a long URL or word"
LICENSE_MESSAGE = ("the project does not declare its license; add a top-level 'license' with an "
                   "SPDX expression, such as 'license: GPL-3.0-only', or 'license: proprietary'")

TOOLS = {
    "snapcraft": dict(file="snap/snapcraft.yaml", key="stage-packages", items=["ca-certificates", "python3"], text="""\
name: hello
base: core24
version: '1.0'
license: GPL-3.0-only
summary: A snap for testing craft-tidy
description: A snap for testing craft-tidy.
confinement: strict
platforms:
  amd64:
apps:
  hello:
    command: bin/hello
    plugs:
      - home
      - network
      - wayland
parts:
  hello:
    plugin: nil
    stage-packages:
      - ca-certificates
      - python3
"""),
    "rockcraft": dict(file="rockcraft.yaml", key="stage-packages", items=["ca-certificates", "python3"], text="""\
name: my-rock
base: ubuntu@24.04
version: '1.0'
license: Apache-2.0
summary: A rock for testing craft-tidy
description: A rock for testing craft-tidy.
platforms:
  amd64:
parts:
  my-rock:
    plugin: nil
    stage-packages:
      - ca-certificates
      - python3
"""),
    "imagecraft": dict(file="imagecraft.yaml", key="stage-packages", items=["linux-generic", "systemd"], text="""\
name: my-image
base: ubuntu@24.04
version: '1.0'
license: Apache-2.0
summary: An image for testing craft-tidy
description: An image for testing craft-tidy.
platforms:
  amd64:
parts:
  rootfs:
    plugin: nil
    stage-packages:
      - linux-generic
      - systemd
"""),
    "debcraft": dict(file="debcraft.yaml", key="build-packages", items=["gcc", "make"], text="""\
name: my-package
base: ubuntu@24.04
version: '1.0'
license: GPL-2.0-or-later
summary: A package for testing craft-tidy
description: A package for testing craft-tidy.
platforms:
  amd64:
parts:
  my-package:
    plugin: nil
    build-packages:
      - gcc
      - make
"""),
    # Charmcraft does not accept a license key, and builds from the project
    # directory (source: .).
    "charmcraft": dict(file="charmcraft.yaml", key="build-packages", items=["cargo", "libffi-dev"], text="""\
name: my-charm
type: charm
title: My charm
summary: A charm for testing craft-tidy
description: A charm for testing craft-tidy.
base: ubuntu@24.04
platforms:
  amd64:
parts:
  charm:
    plugin: charm
    source: .
    build-packages:
      - cargo
      - libffi-dev
"""),
}


def position(text, line_contains, sub=None):
    """1-based line and column of `sub` (default: the line's first
    non-space) on the first line containing `line_contains`."""
    for number, line in enumerate(text.splitlines(), 1):
        if line_contains in line:
            if sub is None:
                return number, len(line) - len(line.lstrip()) + 1
            return number, line.index(sub) + 1
    raise ValueError(line_contains)


def heredoc(path, text):
    body = textwrap.indent(text, "  ").replace("\n  \n", "\n\n")
    return f"  cat > {path} <<'EOF'\n{body}  EOF\n"


def finding(file, line, column, message, check, severity="warning"):
    location = f"{file}:{line}:{column}" if line else file
    return f"{location}: {severity}: {message} [{check}]\n"


def expect(output):
    """Shell lines checking that out.txt is exactly `output`."""
    if not output:
        return "  test ! -s out.txt\n"
    return heredoc("expected.txt", output) + "  diff -u expected.txt out.txt\n"


def unsorted(tool):
    t = TOOLS[tool]
    a, b = t["items"]
    return t["text"].replace(f"      - {a}\n      - {b}\n", f"      - {b}\n      - {a}\n")


def sorted_finding(tool, text, file=None):
    t = TOOLS[tool]
    a, b = t["items"]
    line, column = position(text, f"- {b}", b)
    return finding(
        file or "./" + t["file"], line, column,
        f"entries of '{t['key']}' are not sorted: '{a}' should come before '{b}'",
        "style-sorted-lists",
    )


def task(tool):
    t = TOOLS[tool]
    file, base = t["file"], t["text"]
    a, b = t["items"]
    snap = tool == "snapcraft"
    project = "snap" if snap else "."
    sh = []
    w = sh.append

    w(f"summary: Lint {tool} projects\n\n# Generated by spread/generate-tool-tasks.py; edit that instead.\n\nexecute: |\n")
    w("  # Every project is a conforming project file for the tool, so that all\n"
      "  # checks run and each section only finds what it is about.\n")

    # A conforming project.
    w("\n  # A conforming project passes every check.\n")
    w(f'  mkdir -p "$WORK/clean/{project}"\n  cd "$WORK/clean"\n')
    w(heredoc(file, base))
    w("  craft-tidy > out.txt\n" + expect(""))

    if not snap:
        # Discovery.
        w(f"\n  # Only {file} in the project root is read, like {tool} does.\n")
        w('  mkdir -p "$WORK/discovery/sub" "$WORK/discovery/build-aux"\n  cd "$WORK/discovery"\n')
        w(heredoc(f"sub/{file}", base))
        w(f"  cp sub/{file} .{file}\n  cp sub/{file} build-aux/{file}\n")
        w("  set +e\n  craft-tidy 2> err.txt\n  status=$?\n  set -e\n  test \"$status\" -eq 2\n"
          '  MATCH "no craft project file found" < err.txt\n')

    # Sorting, with locations.
    messy = unsorted(tool)
    w(f"\n  # Problems are reported with locations{'' if snap else '; project-file-location only applies to Snapcraft, so a ' + file + ' in the root is fine'}.\n")
    w(f'  mkdir -p "$WORK/project/{project}"\n  cd "$WORK/project"\n')
    w(heredoc(file, messy))
    w("  craft-tidy > out.txt\n" + expect(sorted_finding(tool, messy)))
    w("  # The file can also be given explicitly.\n")
    w(f'  craft-tidy "$WORK/project/{file}" | sed "s|^$WORK/project/|./|" > out.txt\n')
    w(expect(sorted_finding(tool, messy)))

    # Long lines.
    long = messy.replace(next(l for l in messy.splitlines() if l.startswith("summary:")), LONG_SUMMARY)
    w("\n  # Long lines are found too.\n")
    w(heredoc(file, long))
    line, _ = position(long, LONG_SUMMARY)
    w("  craft-tidy > out.txt\n" + expect(
        "".join(sorted((
            finding("./" + file, line, 81, f"line is {len(LONG_SUMMARY)} characters long, exceeding the limit of 80", "style-line-length"),
            sorted_finding(tool, long),
        ), key=lambda f: [int(x) for x in f.split(":")[1:3]]))))

    # Fixing.
    fixed = base.replace(next(l for l in base.splitlines() if l.startswith("summary:")), WRAPPED_SUMMARY)
    w("\n  # --fix wraps and sorts without changing the data, and is idempotent.\n")
    w("  craft-tidy --fix > out.txt 2> err.txt\n" + expect(""))
    w(f'  MATCH "^craft-tidy: applied fixes to \\./{file} \\(2 issue\\(s\\) found, 0 remaining\\)$" < err.txt\n')
    w(heredoc("expected.yaml", fixed))
    w(f"  diff -u expected.yaml {file}\n")
    w("  craft-tidy --fix > out.txt 2> err.txt\n" + expect("") + "  test ! -s err.txt\n")
    w(f"  diff -u expected.yaml {file}\n")

    # Unfixable lines.
    with_url = fixed + URL_COMMENT + "\n"
    w("\n  # Lines that cannot be wrapped are reported with the reason, with --fix.\n")
    w(heredoc(file, with_url))
    line = len(with_url.splitlines())
    message = f"line is {len(URL_COMMENT)} characters long, exceeding the limit of 80"
    w("  craft-tidy --fix > out.txt\n" + expect(finding("./" + file, line, 81, f"{message} (cannot be fixed: {NO_BREAK})", "style-line-length")))
    w("  craft-tidy > out.txt\n" + expect(finding("./" + file, line, 81, message, "style-line-length")))

    if not snap:
        # Next to a Snapcraft project.
        snap_messy = unsorted("snapcraft")
        w("\n  # Next to a Snapcraft project, both files are linted, in alphabetical\n  # order of the tools.\n")
        w('  mkdir -p "$WORK/project/snap"\n')
        w(heredoc(file, messy))
        w(heredoc("snap/snapcraft.yaml", snap_messy))
        w("  craft-tidy > out.txt\n" + expect(sorted_finding(tool, messy) + sorted_finding("snapcraft", snap_messy)))

    # Collapsed sequences.
    flow = base.replace(f"    {t['key']}:\n      - {a}\n      - {b}\n", f"    {t['key']}: [{a}, {b}]\n")
    part_plugin = next(l for l in base.splitlines() if l.startswith("    plugin:"))
    collapse = flow.replace(part_plugin + "\n", part_plugin + "\n    after:\n      - other\n", 1)
    after_line, after_col = position(collapse, "      - other")
    list_line, list_col = position(collapse, f"{t['key']}: [", "[")
    w("\n  # Short sequences go on one line, longer ones one item per line.\n")
    w(f'  mkdir -p "$WORK/collapse/{project}"\n  cd "$WORK/collapse"\n')
    w(heredoc(file, collapse))
    after_finding = finding("./" + file, after_line, after_col, "'after' has a single item, so it should be on one line: after: [other]", "style-collapsed-sequences")
    w("  craft-tidy > out.txt\n" + expect(after_finding + finding(
        "./" + file, list_line, list_col,
        f"'{t['key']}' has 2 items, more than max-collapsed-items (1), so it should have one item per line",
        "style-collapsed-sequences")))
    w("  craft-tidy -o style-collapsed-sequences.max-collapsed-items=2 > out.txt\n" + expect(after_finding))
    w("  craft-tidy --fix > out.txt 2> err.txt\n" + expect(""))
    w(heredoc("expected.yaml", base.replace(part_plugin + "\n", part_plugin + "\n    after: [other]\n", 1)))
    w(f"  diff -u expected.yaml {file}\n")

    # Redundant platforms.
    platforms = base.replace("platforms:\n  amd64:\n", """\
platforms:
  amd64:
    build-on: [amd64]
    build-for: [amd64]
  arm64:
    build-on:
      - amd64
      - arm64
    build-for: [arm64]
""")
    line, column = position(platforms, "  amd64:")
    w("\n  # Platforms that build on and for their own architecture need no\n  # definition.\n")
    w(f'  mkdir -p "$WORK/platforms/{project}"\n  cd "$WORK/platforms"\n')
    w(heredoc(file, platforms))
    w("  craft-tidy > out.txt\n" + expect(finding(
        "./" + file, line, column,
        "platform 'amd64' only builds on and for amd64, which is what 'amd64:' alone means; remove its build-on and build-for",
        "style-redundant-platforms")))
    w("  craft-tidy --fix > out.txt 2> err.txt\n" + expect(""))
    w(heredoc("expected.yaml", platforms.replace("  amd64:\n    build-on: [amd64]\n    build-for: [amd64]\n", "  amd64:\n")))
    w(f"  diff -u expected.yaml {file}\n")

    # Root sources.
    w("\n  # Parts should not use the whole project directory as their source.\n")
    w(f'  mkdir -p "$WORK/rootsource/{project}"\n  cd "$WORK/rootsource"\n')
    if tool == "charmcraft":
        w("  # Except for Charmcraft, whose charm plugin builds from there (as the\n"
          "  # conforming project does).\n")
        w(heredoc(file, base) + "  craft-tidy > out.txt\n" + expect(""))
    else:
        root = base + "  data:\n    plugin: dump\n    source: .\n"
        line, column = position(root, "    source: .", ".")
        w(heredoc(file, root))
        message = ("part 'data' uses the whole project directory as its source ('.'), so it picks up build "
                   "artifacts there, which with --destructive-mode trigger rebuilds; use the directory with the "
                   "code instead, such as 'src'")
        w("  craft-tidy > out.txt\n" + expect(finding("./" + file, line, column, message, "project-root-source")))
        w("  craft-tidy --fix > out.txt\n" + expect(finding(
            "./" + file, line, column,
            message + " (cannot be fixed: craft-tidy cannot know which directory holds the code)",
            "project-root-source")))

    # Licenses.
    w("\n  # Projects must declare their license.\n")
    w(f'  mkdir -p "$WORK/license/{project}"\n  cd "$WORK/license"\n')
    if tool == "charmcraft":
        w("  # Except for Charmcraft, which does not accept a license key (the\n"
          "  # conforming project has none).\n")
        w(heredoc(file, base) + "  craft-tidy > out.txt\n" + expect(""))
    else:
        unlicensed = "\n".join(l for l in base.splitlines() if not l.startswith("license:")) + "\n"
        w(heredoc(file, unlicensed))
        w("  craft-tidy > out.txt\n" + expect(finding("./" + file, 1, 1, LICENSE_MESSAGE, "metadata-license")))

    # Components.
    if snap:
        w(SNAPCRAFT_EXTRA)
    else:
        w("\n  # snapcraft-components only applies to Snapcraft.\n")
        w('  mkdir -p "$WORK/components"\n  cd "$WORK/components"\n')
        w(heredoc(file, base.replace(part_plugin + "\n", part_plugin + "\n    organize:\n      lib: (component/nope)\n", 1)))
        w("  craft-tidy > out.txt\n" + expect(""))
    return "".join(sh)


snap_base = TOOLS["snapcraft"]["text"]
components = snap_base.replace("platforms:\n", """\
components:
  translations:
    type: standard
    summary: Translations
    description: Translations
platforms:
""").replace("""\
  hello:
    plugin: nil
""", """\
  hello:
    plugin: dump
    source: translations
    organize:
      la: (component/translatons)
    override-prime: |
      craftctl default
      cp -r extra $CRAFT_COMPONENT_DOCS_PRIME/
""")
c_line, c_col = position(components, "  translations:")
t_line, t_col = position(components, "(component/translatons)", "translatons")
d_line, d_col = position(components, "$CRAFT_COMPONENT_DOCS_PRIME", "CRAFT_COMPONENT")
F = "./snap/snapcraft.yaml"
unused = finding(F, c_line, c_col, "component 'translations' is defined but no part puts files in it, so it would be empty", "snapcraft-components")
typo = finding(F, t_line, t_col, "component 'translatons' is not defined under components; did you mean 'translations'?", "snapcraft-components")
docs = finding(F, d_line, d_col, "$CRAFT_COMPONENT_DOCS_PRIME refers to component 'docs', which is not defined under components", "snapcraft-components")
root_text = snap_base
root_long = root_text.replace(next(l for l in root_text.splitlines() if l.startswith("summary:")), LONG_SUMMARY)
r_line, _ = position(root_long, LONG_SUMMARY)

SNAPCRAFT_EXTRA = (
    "\n  # Project files are looked for in Snapcraft's order, and only the first\n"
    "  # one found is linted; project-file-location reports those outside snap/.\n"
    '  mkdir -p "$WORK/order/snap" "$WORK/order/build-aux/snap"\n  cd "$WORK/order"\n'
    + heredoc(".snapcraft.yaml", snap_base)
    + "  craft-tidy > out.txt\n"
    + expect("./.snapcraft.yaml: warning: .snapcraft.yaml is in the project root; move it to snap/snapcraft.yaml [project-file-location]\n")
    + "  # build-aux/snap/ is an accepted location.\n"
    + heredoc("build-aux/snap/snapcraft.yaml", unsorted("snapcraft"))
    + "  craft-tidy > out.txt\n"
    + expect(sorted_finding("snapcraft", unsorted("snapcraft"), "./build-aux/snap/snapcraft.yaml"))
    + heredoc("snap/snapcraft.yaml", snap_base)
    + "  craft-tidy > out.txt\n" + expect("")
    + heredoc("snapcraft.yaml", root_long)
    + "  craft-tidy > out.txt\n"
    + expect(
        "./snapcraft.yaml: warning: snapcraft.yaml is in the project root; move it to snap/snapcraft.yaml [project-file-location]\n"
        + finding("./snapcraft.yaml", r_line, 81, f"line is {len(LONG_SUMMARY)} characters long, exceeding the limit of 80", "style-line-length"))
    + "\n  # warnings-as-errors makes the run fail.\n"
    + "  set +e\n  craft-tidy --warnings-as-errors='style-*' > out.txt\n  status=$?\n  set -e\n  test \"$status\" -eq 1\n"
    + expect(
        "./snapcraft.yaml: warning: snapcraft.yaml is in the project root; move it to snap/snapcraft.yaml [project-file-location]\n"
        + finding("./snapcraft.yaml", r_line, 81, f"line is {len(LONG_SUMMARY)} characters long, exceeding the limit of 80", "style-line-length", "error"))
    + "\n  # Check selection and options.\n"
    + "  craft-tidy --checks='-*,style-line-length' -o style-line-length.max-length=100 > out.txt\n" + expect("")
    + "\n  # Components used by parts must be defined.\n"
    + '  mkdir -p "$WORK/components/snap"\n  cd "$WORK/components"\n'
    + heredoc("snap/snapcraft.yaml", components)
    + "  craft-tidy > out.txt\n" + expect(unused + typo + docs)
    + "  craft-tidy -o snapcraft-components.report-unused=false > out.txt\n" + expect(typo + docs)
    + "  # Not fixable; --fix leaves the file alone.\n"
    + "  cp snap/snapcraft.yaml before.yaml\n  craft-tidy --fix > /dev/null\n  diff -u before.yaml snap/snapcraft.yaml\n"
    + "  sed -i 's/translatons/translations/; s/\\$CRAFT_COMPONENT_DOCS_PRIME/$CRAFT_COMPONENT_TRANSLATIONS_PRIME/' snap/snapcraft.yaml\n"
    + "  craft-tidy > out.txt\n" + expect("")
)

GENERAL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "general")

for tool in TOOLS:
    path = os.path.join(GENERAL, tool, "task.yaml")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(task(tool))
