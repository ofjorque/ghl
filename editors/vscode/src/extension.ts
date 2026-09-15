import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let replTerminal: vscode.Terminal | undefined;

interface GhlTerminalLink extends vscode.TerminalLink {
  filePath: string;
  isParquet: boolean;
}

async function openInDataExplorer(uri: vscode.Uri) {
  try {
    await vscode.commands.executeCommand('vscode.openWith', uri, 'positron.dataExplorer');
  } catch (_) {
    vscode.commands.executeCommand('vscode.open', uri);
  }
}

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

  // Register command to open DataFrame in Positron Data Explorer
  context.subscriptions.push(
    vscode.commands.registerCommand('ghl.openDataExplorer', async (uri?: vscode.Uri) => {
      if (!uri) {
        const uris = await vscode.window.showOpenDialog({
          canSelectFiles: true,
          filters: { 'Parquet & Arrow Files': ['parquet', 'arrow'] }
        });
        if (uris && uris.length > 0) {
          uri = uris[0];
        }
      }
      if (uri) {
        openInDataExplorer(uri);
      }
    })
  );

  // Register Terminal Link Provider for Parquet (Data Explorer) and SVG/PNG (Plots)
  context.subscriptions.push(
    vscode.window.registerTerminalLinkProvider({
      provideTerminalLinks: (linkContext, _token) => {
        const line = linkContext.line;
        const links: GhlTerminalLink[] = [];

        // Match .parquet file paths (e.g. C:\...ghl_data_...parquet or /tmp/...parquet)
        const parquetRegex = /(?:[a-zA-Z]:[\\\/]|\/)[^\s"']+\.parquet\b/g;
        let pMatch: RegExpExecArray | null;
        while ((pMatch = parquetRegex.exec(line)) !== null) {
          links.push({
            startIndex: pMatch.index,
            length: pMatch[0].length,
            tooltip: 'Open in Positron Data Explorer (=^･ω･^=)',
            filePath: pMatch[0],
            isParquet: true
          });
        }

        // Match .svg and .png plot paths
        const plotRegex = /(?:[a-zA-Z]:[\\\/]|\/)[^\s"']+\.(?:svg|png)\b/g;
        let sMatch: RegExpExecArray | null;
        while ((sMatch = plotRegex.exec(line)) !== null) {
          links.push({
            startIndex: sMatch.index,
            length: sMatch[0].length,
            tooltip: 'Open in Plots Viewer (U・ᴥ・U)',
            filePath: sMatch[0],
            isParquet: false
          });
        }

        return links;
      },
      handleTerminalLink: (link: GhlTerminalLink) => {
        const uri = vscode.Uri.file(link.filePath);
        if (link.isParquet) {
          openInDataExplorer(uri);
        } else {
          vscode.commands.executeCommand('vscode.open', uri);
        }
      }
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

function findQuartoChunk(document: vscode.TextDocument, currentLine: number): string | null {
  const isMarkdownOrQuarto = document.languageId === 'quarto' || document.languageId === 'markdown';
  if (!isMarkdownOrQuarto) {
    return null;
  }

  // Look upwards for ```{ghl or ```{gh
  let startLine = -1;
  for (let i = currentLine; i >= 0; i--) {
    const text = document.lineAt(i).text.trim();
    if (/^```+\s*\{?\s*(?:gh|ghl)\b/.test(text)) {
      startLine = i;
      break;
    }
    // If we hit a closing fence before finding start, we aren't in a chunk
    if (i < currentLine && /^```+\s*$/.test(text)) {
      return null;
    }
  }

  if (startLine === -1) {
    return null;
  }

  // Look downwards for closing ```
  let endLine = -1;
  for (let i = currentLine; i < document.lineCount; i++) {
    const text = document.lineAt(i).text.trim();
    if (i > startLine && /^```+\s*$/.test(text)) {
      endLine = i;
      break;
    }
  }

  if (endLine === -1) {
    return null;
  }

  // Extract code between startLine + 1 and endLine - 1
  const lines: string[] = [];
  for (let i = startLine + 1; i < endLine; i++) {
    const l = document.lineAt(i).text;
    // Skip chunk option directives like `#| echo: false`
    if (!/^\s*#\|/.test(l)) {
      lines.push(l);
    }
  }

  return lines.join('\n');
}

function sendToRepl() {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }

  const selection = editor.selection;
  let code = '';

  if (selection.isEmpty) {
    const currentLine = selection.active.line;
    // Check if cursor is inside a Quarto / Markdown executable chunk
    const chunkCode = findQuartoChunk(editor.document, currentLine);
    if (chunkCode !== null) {
      code = chunkCode;
    } else {
      const line = editor.document.lineAt(currentLine);
      code = line.text;

      // Advance cursor to next non-empty line (standard scientific computing behavior)
      if (currentLine + 1 < editor.document.lineCount) {
        let nextLine = currentLine + 1;
        while (nextLine < editor.document.lineCount - 1 && !editor.document.lineAt(nextLine).text.trim()) {
          nextLine++;
        }
        const newPos = new vscode.Position(nextLine, 0);
        editor.selection = new vscode.Selection(newPos, newPos);
      }
    }
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
    const plotsDir = process.env.POSITRON_PLOTS_DIR || path.join(os.tmpdir(), 'ghl_plots');
    try {
      if (!fs.existsSync(plotsDir)) {
        fs.mkdirSync(plotsDir, { recursive: true });
      }
    } catch (_) {}

    replTerminal = vscode.window.createTerminal({
      name: terminalName,
      shellPath: binaryPath,
      shellArgs: ['repl'],
      env: {
        POSITRON_PLOTS_DIR: plotsDir,
        GHL_PLOTS_DIR: plotsDir
      }
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
