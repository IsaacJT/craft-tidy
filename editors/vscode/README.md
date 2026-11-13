# craft-tidy for Visual Studio Code

Shows the findings of [craft-tidy](https://github.com/IsaacJT/craft-tidy)
in the project files of Canonical's craft tools (`snapcraft.yaml`,
`charmcraft.yaml`, `rockcraft.yaml`, `imagecraft.yaml`, `debcraft.yaml`)
as you edit them, with quick fixes.

## Requirements

The extension runs craft-tidy's language server, `craft-tidy lsp`, so
craft-tidy must be installed:

```sh
sudo snap install craft-tidy --edge
```

If it is not on your `PATH`, set `craftTidy.path` to the executable, such
as `/snap/bin/craft-tidy`.

## Features

- Findings are underlined as you type, without saving, using the
  `.craft-tidy` configuration of the project.
- Quick fixes (`Ctrl+.`): fix a finding, or suppress it with a
  `# NOLINTNEXTLINE` comment.
- **Fix all craft-tidy problems**, which can run on save:

  ```json
  "[yaml]": {
    "editor.codeActionsOnSave": { "source.fixAll": "explicit" }
  }
  ```

- Hovering over a finding explains its check.

## Settings

| Setting          | Default      | Description |
| ---------------- | ------------ | ----------- |
| `craftTidy.path` | `craft-tidy` | The craft-tidy executable |

Run **craft-tidy: Restart language server** after changing it.

## Building

```sh
npm install
npm run package
```

This creates `craft-tidy-<version>.vsix`, which can be installed with
**Extensions: Install from VSIX…**.

## License

GPL-3.0-only, like craft-tidy. The icon is the star emoji from
[Noto Emoji](https://github.com/googlefonts/noto-emoji), copyright 2013
Google, Inc., rendered from `2D/svg/emoji_u2b50.svg` and licensed under the
Apache License, Version 2.0; see `LICENSE.noto-emoji`.
