// SPDX-License-Identifier: GPL-3.0-only

// Starts craft-tidy's language server (`craft-tidy lsp`) for the project
// files of the craft tools.

const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

/** The project files the server checks; other files are not sent to it. */
const PROJECT_FILES =
  "**/{snapcraft,charmcraft,rockcraft,imagecraft,debcraft,.snapcraft}.yaml";

let client;

async function activate(context) {
  const command = vscode.workspace
    .getConfiguration("craftTidy")
    .get("path", "craft-tidy");
  const server = { command, args: ["lsp"] };
  client = new LanguageClient(
    "craftTidy",
    "craft-tidy",
    { run: server, debug: server },
    {
      documentSelector: [{ scheme: "file", pattern: PROJECT_FILES }],
    },
  );
  context.subscriptions.push(
    vscode.commands.registerCommand("craftTidy.restart", () => client.restart()),
  );
  try {
    await client.start();
  } catch (error) {
    vscode.window.showErrorMessage(
      `craft-tidy: could not start '${command} lsp': ${error.message}. ` +
        "Install craft-tidy (sudo snap install craft-tidy --edge) or set craftTidy.path.",
    );
  }
}

async function deactivate() {
  if (client) {
    await client.stop();
  }
}

module.exports = { activate, deactivate };
