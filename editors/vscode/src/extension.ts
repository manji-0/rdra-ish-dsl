import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function resolveServerPath(): string {
  const configured = vscode.workspace
    .getConfiguration("rdra-ish")
    .get<string>("languageServerPath")
    ?.trim();
  if (configured) {
    return configured;
  }

  const sibling = path.resolve(
    __dirname,
    "..",
    "..",
    "..",
    "target",
    "debug",
    process.platform === "win32" ? "rdra-ish-lsp.exe" : "rdra-ish-lsp",
  );
  if (fs.existsSync(sibling)) {
    return sibling;
  }

  return "rdra-ish-lsp";
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const serverPath = resolveServerPath();
  const serverOptions: ServerOptions = {
    run: { command: serverPath, transport: TransportKind.stdio },
    debug: { command: serverPath, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "rdra" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.rdra"),
    },
  };

  client = new LanguageClient(
    "rdra-ish",
    "RDRA-ish Language Server",
    serverOptions,
    clientOptions,
  );

  await client.start();
  context.subscriptions.push({ dispose: () => client?.stop() });
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}
