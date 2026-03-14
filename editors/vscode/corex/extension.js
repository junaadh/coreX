const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

const TOKEN_TYPES = [
  'namespace',
  'type',
  'class',
  'enum',
  'interface',
  'struct',
  'typeParameter',
  'parameter',
  'variable',
  'property',
  'enumMember',
  'event',
  'function',
  'method',
  'macro',
  'keyword',
  'modifier',
  'comment',
  'string',
  'number',
  'regexp',
  'operator'
];

let client;

function resolveServerCwd() {
  const config = vscode.workspace.getConfiguration('corex');
  const cwd = config.get('languageServer.cwd', 'workspace');
  if (cwd === 'workspace') {
    const folder = vscode.workspace.workspaceFolders?.[0];
    return folder?.uri.fsPath;
  }

  const trimmed = String(cwd || '').trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

async function startLanguageClient(context) {
  const config = vscode.workspace.getConfiguration('corex');
  const command = config.get('languageServer.command', 'cxc');
  const args = config.get('languageServer.args', ['lsp']);

  const outputChannel = vscode.window.createOutputChannel('CoreX LSP');
  context.subscriptions.push(outputChannel);

  outputChannel.appendLine(`[CoreX] Starting Language Server...`);
  outputChannel.appendLine(`[CoreX] Command: ${command}`);
  outputChannel.appendLine(`[CoreX] Args: ${Array.isArray(args) ? args.join(' ') : 'lsp'}`);
  outputChannel.appendLine(`[CoreX] CWD: ${resolveServerCwd() || 'default'}`);

  const serverOptions = {
    command,
    args: Array.isArray(args) ? args : ['lsp'],
    options: {
      cwd: resolveServerCwd()
    }
  };

  const clientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'corex' },
      { scheme: 'untitled', language: 'corex' }
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.cx')
    },
    outputChannel,
    errorHandler: {
      error: (error, message, count) => {
        outputChannel.appendLine(`[CoreX LSP Error] ${error.message}`);
        outputChannel.appendLine(`[CoreX LSP Error] Message: ${message}`);
        outputChannel.appendLine(`[CoreX LSP Error] Count: ${count}`);
        return { action: 'continue' };
      },
      closed: () => {
        outputChannel.appendLine('[CoreX LSP] Connection closed');
        return false;
      }
    }
  };

  client = new LanguageClient(
    'corex-lsp',
    'CoreX Language Server',
    serverOptions,
    clientOptions
  );

  try {
    await client.start();
    context.subscriptions.push(
      new vscode.Disposable(() => {
        client.stop().then(() => {
          outputChannel.appendLine('[CoreX] Language Server stopped');
        });
      })
    );
    outputChannel.appendLine('[CoreX] Language Server started successfully');
  } catch (error) {
    outputChannel.appendLine(`[CoreX] Failed to start Language Server: ${error.message}`);
    outputChannel.show();
    vscode.window.showErrorMessage(`Failed to start CoreX Language Server: ${error.message}`);
  }
}

function mapCaptureToTokenType(captureName) {
  const base = String(captureName || '').split('.')[0];

  switch (base) {
    case 'comment':
      return 'comment';
    case 'string':
      return 'string';
    case 'number':
    case 'integer':
    case 'float':
      return 'number';
    case 'keyword':
      return 'keyword';
    case 'operator':
      return 'operator';
    case 'type':
    case 'struct':
    case 'enum':
    case 'trait':
    case 'interface':
    case 'protocol':
      return 'type';
    case 'constructor':
    case 'function':
      return 'function';
    case 'method':
      return 'method';
    case 'namespace':
      return 'namespace';
    case 'parameter':
      return 'parameter';
    case 'property':
    case 'field':
      return 'property';
    case 'constant':
      return 'enumMember';
    case 'variable':
    case 'identifier':
      return 'variable';
    default:
      return null;
  }
}

function lineLength(document, line) {
  if (line < 0 || line >= document.lineCount) {
    return 0;
  }
  return document.lineAt(line).text.length;
}

function captureToSegments(document, capture, tokenType) {
  const node = capture.node;
  if (!node || !node.startPosition || !node.endPosition) {
    return [];
  }

  const startLine = node.startPosition.row;
  const startChar = node.startPosition.column;
  const endLine = node.endPosition.row;
  const endChar = node.endPosition.column;

  if (endLine < startLine) {
    return [];
  }

  if (endLine === startLine && endChar <= startChar) {
    return [];
  }

  const segments = [];

  if (startLine === endLine) {
    segments.push({
      line: startLine,
      start: startChar,
      length: endChar - startChar,
      type: tokenType
    });
    return segments;
  }

  const firstLineLength = lineLength(document, startLine);
  if (firstLineLength > startChar) {
    segments.push({
      line: startLine,
      start: startChar,
      length: firstLineLength - startChar,
      type: tokenType
    });
  }

  for (let line = startLine + 1; line < endLine; line += 1) {
    const length = lineLength(document, line);
    if (length > 0) {
      segments.push({
        line,
        start: 0,
        length,
        type: tokenType
      });
    }
  }

  if (endChar > 0) {
    segments.push({
      line: endLine,
      start: 0,
      length: endChar,
      type: tokenType
    });
  }

  return segments;
}

class CorexTreeSitterProvider {
  constructor(context) {
    this.context = context;
    this.legend = new vscode.SemanticTokensLegend(TOKEN_TYPES, []);
    this.resourcesPromise = this.loadResources(context).catch((error) => {
      console.error('[CoreX] Failed to initialize Tree-sitter resources:', error);
      const outputChannel = vscode.window.createOutputChannel('CoreX Tree-sitter');
      outputChannel.appendLine(`[CoreX Tree-sitter] Failed to initialize: ${error.message}`);
      outputChannel.show();
      return null;
    });
  }

  async loadResources(context) {
    const outputChannel = vscode.window.createOutputChannel('CoreX Tree-sitter');

    try {
      outputChannel.appendLine('[CoreX Tree-sitter] Loading web-tree-sitter module...');
      const { Parser } = require('web-tree-sitter');

      outputChannel.appendLine('[CoreX Tree-sitter] Initializing Parser...');
      await Parser.init();

      const wasmPath = path.join(
        context.extensionPath,
        'syntaxes',
        'tree-sitter-corex.wasm'
      );
      const queryPath = path.join(
        context.extensionPath,
        'syntaxes',
        'highlights.scm'
      );

      outputChannel.appendLine(`[CoreX Tree-sitter] Loading WASM from: ${wasmPath}`);
      if (!fs.existsSync(wasmPath)) {
        throw new Error(`WASM file not found: ${wasmPath}`);
      }

      outputChannel.appendLine(`[CoreX Tree-sitter] Loading language from WASM...`);
      const language = await Parser.Language.load(wasmPath);

      outputChannel.appendLine('[CoreX Tree-sitter] Creating parser...');
      const parser = new Parser();
      parser.setLanguage(language);

      outputChannel.appendLine(`[CoreX Tree-sitter] Loading query from: ${queryPath}`);
      if (!fs.existsSync(queryPath)) {
        throw new Error(`Query file not found: ${queryPath}`);
      }

      const queryText = fs.readFileSync(queryPath, 'utf8');
      const query = language.query(queryText);

      outputChannel.appendLine('[CoreX Tree-sitter] Resources loaded successfully');

      return {
        parser,
        query,
        outputChannel
      };
    } catch (error) {
      outputChannel.appendLine(`[CoreX Tree-sitter] Error loading resources: ${error.message}`);
      outputChannel.appendLine(error.stack || '');
      outputChannel.show();
      throw error;
    }
  }

  async provideDocumentSemanticTokens(document) {
    const enabled = vscode.workspace
      .getConfiguration('corex')
      .get('treeSitter.enabled', true);
    if (!enabled) {
      return null;
    }

    const resources = await this.resourcesPromise;
    if (!resources) {
      return null;
    }

    try {
      const tree = resources.parser.parse(document.getText());
      const matches = resources.query.matches(tree.rootNode);

      const segments = [];
      for (const match of matches) {
        for (const capture of match.captures) {
          const tokenType = mapCaptureToTokenType(capture.name);
          if (!tokenType) {
            continue;
          }
          segments.push(...captureToSegments(document, capture, tokenType));
        }
      }

      segments.sort((left, right) => {
        if (left.line !== right.line) {
          return left.line - right.line;
        }
        if (left.start !== right.start) {
          return left.start - right.start;
        }
        return right.length - left.length;
      });

      const builder = new vscode.SemanticTokensBuilder();
      let previousLine = -1;
      let previousEnd = -1;

      for (const segment of segments) {
        if (segment.length <= 0) {
          continue;
        }

        if (segment.line === previousLine && segment.start < previousEnd) {
          continue;
        }

        const typeIndex = TOKEN_TYPES.indexOf(segment.type);
        if (typeIndex < 0) {
          continue;
        }

        builder.push(
          segment.line,
          segment.start,
          segment.length,
          typeIndex,
          0
        );

        previousLine = segment.line;
        previousEnd = segment.start + segment.length;
      }

      return builder.build();
    } catch (error) {
      resources.outputChannel?.appendLine(`[CoreX Tree-sitter] Error providing tokens: ${error.message}`);
      console.error('[CoreX Tree-sitter] Error providing tokens:', error);
      return null;
    }
  }
}

async function activate(context) {
  const outputChannel = vscode.window.createOutputChannel('CoreX');
  context.subscriptions.push(outputChannel);

  outputChannel.appendLine('[CoreX] Activating CoreX extension...');

  try {
    const treeSitterProvider = new CorexTreeSitterProvider(context);

    context.subscriptions.push(
      vscode.languages.registerDocumentSemanticTokensProvider(
        { language: 'corex' },
        treeSitterProvider,
        treeSitterProvider.legend
      )
    );

    outputChannel.appendLine('[CoreX] Tree-sitter provider registered');

    await startLanguageClient(context);

    outputChannel.appendLine('[CoreX] Extension activated successfully');
  } catch (error) {
    outputChannel.appendLine(`[CoreX] Error during activation: ${error.message}`);
    outputChannel.appendLine(error.stack || '');
    outputChannel.show();
    vscode.window.showErrorMessage(`CoreX extension activation failed: ${error.message}`);
  }
}

async function deactivate() {
  if (!client) {
    return;
  }
  await client.stop();
}

module.exports = {
  activate,
  deactivate
};
