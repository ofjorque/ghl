import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let replTerminal: vscode.Terminal | undefined;

function resolveBinaryPath(): string {
  const config = vscode.workspace.getConfiguration('ghl');
  const configured = config.get<string>('server.path', 'ghl');
  if (configured !== 'ghl' && fs.existsSync(configured)) {
    return configured;
  }

  // Look in workspace folders for target/debug/ghl or target/release/ghl
  if (vscode.workspace.workspaceFolders) {
    for (const folder of vscode.workspace.workspaceFolders) {
      const candidates = [
        path.join(folder.uri.fsPath, 'target', 'debug', 'ghl.exe'),
        path.join(folder.uri.fsPath, 'target', 'debug', 'ghl'),
        path.join(folder.uri.fsPath, 'target', 'release', 'ghl.exe'),
        path.join(folder.uri.fsPath, 'target', 'release', 'ghl'),
      ];
      for (const candidate of candidates) {
        if (fs.existsSync(candidate)) {
          return candidate;
        }
      }
    }
  }

  return configured;
}

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('ghl');
  const isServerEnabled = config.get<boolean>('server.enabled', true);

  if (isServerEnabled) {
    startLanguageServer(context);
  }

  // Register command to restart language server
  context.subscriptions.push(
    vscode.commands.registerCommand('ghl.restartServer', async () => {
      if (client) {
        await client.stop();
        client = undefined;
      }
      startLanguageServer(context);
      vscode.window.showInformationMessage('GHL Language Server restarted (=^･ω･^=)');
    })
  );

  // Register command to send code to REPL (Ctrl + Enter)
  context.subscriptions.push(
    vscode.commands.registerCommand('ghl.sendSelectionToREPL', () => {
      sendToRepl();
    })
  );
}

function startLanguageServer(context: vscode.ExtensionContext) {
  const serverPath = resolveBinaryPath();

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ['lsp']
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'ghl' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{gh,ghl}')
    }
  };

  client = new LanguageClient(
    'ghlLsp',
    'GHL Language Server',
    serverOptions,
    clientOptions
  );

  client.start().catch((err) => {
    vscode.window.showWarningMessage(
      `Could not start GHL Language Server ('${serverPath} lsp'): ${err.message}. Make sure 'ghl' is installed and in your PATH, or configure 'ghl.server.path'.`
    );
  });
}

function sendToRepl() {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }

  const selection = editor.selection;
  let code = '';

  if (selection.isEmpty) {
    const line = editor.document.lineAt(selection.active.line);
    code = line.text;
  } else {
    code = editor.document.getText(selection);
  }

  if (!code.trim()) {
    return;
  }

  // Find or create terminal
  const terminalName = 'GHL REPL';
  const existingTerminals = vscode.window.terminals.filter(t => t.name === terminalName);
  
  if (existingTerminals.length > 0) {
    replTerminal = existingTerminals[0];
  } else {
    const binaryPath = resolveBinaryPath();
    replTerminal = vscode.window.createTerminal({
      name: terminalName,
      shellPath: binaryPath,
      shellArgs: ['repl']
    });
  }

  replTerminal.show(true);
  replTerminal.sendText(code);
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
