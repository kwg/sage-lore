// VS Code extension for Scroll Assembly LSP client
const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const config = vscode.workspace.getConfiguration("scroll.lsp");
  const command = config.get("path", "sage-lore");

  const serverOptions = {
    command: command,
    args: ["lsp"],
    transport: TransportKind.stdio,
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "scroll" }],
  };

  client = new LanguageClient(
    "sage-scroll-lsp",
    "Scroll Assembly LSP",
    serverOptions,
    clientOptions
  );

  client.start();
}

function deactivate() {
  if (client) return client.stop();
}

module.exports = { activate, deactivate };
