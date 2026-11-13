# craft-tidy for Zed

Shows the findings of [craft-tidy](https://github.com/IsaacJT/craft-tidy)
in the project files of Canonical's craft tools (`snapcraft.yaml`,
`charmcraft.yaml`, `rockcraft.yaml`, `imagecraft.yaml`, `debcraft.yaml`)
as you edit them, with quick fixes and hover explanations.

## Requirements

The extension runs craft-tidy's language server, `craft-tidy lsp`, so
craft-tidy must be installed and on your `PATH`:

```sh
sudo snap install craft-tidy --edge
```

The server is attached to all YAML files, alongside Zed's YAML language
server, but only reports findings in craft project files.

## Installing

Until it is published, install it from this directory with **zed: install
dev extension** (Zed builds it, which needs Rust installed with rustup).

## Settings

To use another executable or arguments:

```json
"lsp": {
  "craft-tidy": {
    "binary": { "path": "/snap/bin/craft-tidy", "arguments": ["lsp"] }
  }
}
```

To fix all problems on save:

```json
"languages": {
  "YAML": {
    "code_actions_on_format": { "source.fixAll": true }
  }
}
```
