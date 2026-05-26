// SPDX-License-Identifier: MIT OR Apache-2.0
// Nova Language — VSCode LSP Client Extension
// Plan 104.8.Ф.1
//
// Implements LanguageClient that spawns nova-lsp via stdio JSON-RPC.
// Binary discovery priority:
//   1. User setting: nova.lsp.path
//   2. PATH lookup (where.exe on Windows, which on Unix)
//   3. Workspace-relative: target/release/nova-lsp[.exe]
//   4. Graceful error — no crash, shows informative message.
//
// Auto-restart on crash: max 3 consecutive errors → shutdown with message.
// Config change (nova.lsp.path / nova.lsp.enabled) → stop + restart.

import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as child_process from 'child_process';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
    RevealOutputChannelOn,
    ErrorAction,
    CloseAction,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let outputChannel: vscode.OutputChannel;

// ─────────────────────────────────────────────────────────────
// Extension lifecycle
// ─────────────────────────────────────────────────────────────

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    outputChannel = vscode.window.createOutputChannel('Nova LSP');
    context.subscriptions.push(outputChannel);

    outputChannel.appendLine('[Nova] Extension activating...');

    await startClient(context);

    // Watch for config changes → restart client
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(async (e) => {
            if (
                e.affectsConfiguration('nova.lsp.path') ||
                e.affectsConfiguration('nova.lsp.enabled')
            ) {
                outputChannel.appendLine('[Nova] Config changed — restarting LSP client...');
                await stopClient();
                await startClient(context);
            }
        })
    );

    outputChannel.appendLine('[Nova] Extension activated.');
}

export async function deactivate(): Promise<void> {
    outputChannel?.appendLine('[Nova] Extension deactivating...');
    await stopClient();
}

// ─────────────────────────────────────────────────────────────
// Client start / stop
// ─────────────────────────────────────────────────────────────

async function startClient(context: vscode.ExtensionContext): Promise<void> {
    const config = vscode.workspace.getConfiguration('nova');
    const enabled = config.get<boolean>('lsp.enabled', true);

    if (!enabled) {
        outputChannel.appendLine('[Nova] LSP disabled via nova.lsp.enabled = false.');
        return;
    }

    const serverPath = await findNovaLsp();
    if (!serverPath) {
        outputChannel.appendLine(
            '[Nova] ⚠️  nova-lsp binary not found. LSP features are disabled.\n' +
            '  Solutions:\n' +
            '    1. Build from source:  cargo build --release -p nova-lsp\n' +
            '    2. Add nova-lsp to your PATH\n' +
            '    3. Set "nova.lsp.path" in VSCode settings to the full path\n' +
            '  Syntax highlighting remains active.'
        );
        // Fire-and-forget: don't await the warning dialog (it waits for user input
        // and would block activate() in headless/test environments).
        void vscode.window.showWarningMessage(
            'Nova: LSP server (nova-lsp) not found — diagnostics disabled. ' +
            'See Output > Nova LSP for details.',
            'Open Settings',
            'Dismiss'
        ).then((choice) => {
            if (choice === 'Open Settings') {
                void vscode.commands.executeCommand(
                    'workbench.action.openSettings',
                    'nova.lsp.path'
                );
            }
        });
        return;
    }

    outputChannel.appendLine(`[Nova] Starting LSP at: ${serverPath}`);

    const serverOptions: ServerOptions = {
        run: {
            command: serverPath,
            transport: TransportKind.stdio,
        },
        debug: {
            command: serverPath,
            transport: TransportKind.stdio,
            options: {
                env: { ...process.env, NOVA_LSP_LOG: 'debug' },
            },
        },
    };

    const clientOptions: LanguageClientOptions = {
        // Serve all Nova files (scheme: file or untitled)
        documentSelector: [
            { scheme: 'file', language: 'nova' },
            { scheme: 'untitled', language: 'nova' },
        ],
        synchronize: {
            // Re-validate on changes to nova.toml (project config)
            fileEvents: [
                vscode.workspace.createFileSystemWatcher('**/*.nv'),
                vscode.workspace.createFileSystemWatcher('**/nova.toml'),
            ],
        },
        outputChannel,
        traceOutputChannel: outputChannel,
        revealOutputChannelOn: RevealOutputChannelOn.Error,
        // Workspace-aware root detection: prefer nova.toml, fallback to .git
        workspaceFolder: vscode.workspace.workspaceFolders?.[0],
        initializationOptions: {
            workspaceFolders:
                vscode.workspace.workspaceFolders?.map((f) => f.uri.fsPath) ?? [],
        },
        // Auto-restart on crash: up to 3 errors, then shutdown with user message
        errorHandler: {
            error(
                error: Error,
                _message: unknown,
                count: number | undefined
            ): { action: ErrorAction; handled?: boolean; message?: string } {
                const attempt = count ?? 1;
                outputChannel.appendLine(
                    `[Nova] LSP error (attempt ${attempt}): ${error.message}`
                );
                if (attempt >= 3) {
                    outputChannel.appendLine(
                        '[Nova] Too many errors — stopping auto-restart. Check Output > Nova LSP.'
                    );
                    vscode.window.showErrorMessage(
                        'Nova LSP stopped after 3 consecutive errors. ' +
                        'Check "Output > Nova LSP" for details.'
                    );
                    return { action: ErrorAction.Shutdown };
                }
                return { action: ErrorAction.Continue };
            },
            closed(): { action: CloseAction; handled?: boolean; message?: string } {
                outputChannel.appendLine(
                    '[Nova] LSP connection closed — restarting...'
                );
                return { action: CloseAction.Restart };
            },
        },
    };

    client = new LanguageClient(
        'nova-lsp',
        'Nova Language Server',
        serverOptions,
        clientOptions
    );

    try {
        await client.start();
        outputChannel.appendLine('[Nova] LSP client started successfully.');
        context.subscriptions.push(client);
    } catch (err) {
        outputChannel.appendLine(`[Nova] LSP client failed to start: ${err}`);
        client = undefined;
    }
}

async function stopClient(): Promise<void> {
    if (client) {
        try {
            await client.stop();
        } catch {
            // ignore stop-time errors
        }
        client = undefined;
    }
}

// ─────────────────────────────────────────────────────────────
// Binary discovery
// ─────────────────────────────────────────────────────────────

/**
 * Exported for testing.
 * Find the nova-lsp binary using priority order:
 *   1. User setting nova.lsp.path (explicit override)
 *   2. PATH lookup
 *   3. Workspace-relative target/release or target/debug
 *   4. Returns null — caller handles gracefully (no crash)
 */
export function findNovaLsp(): string | null {
    const isWindows = process.platform === 'win32';
    const binaryName = isWindows ? 'nova-lsp.exe' : 'nova-lsp';

    // 1. User setting
    const config = vscode.workspace.getConfiguration('nova');
    const userPath = config.get<string | null>('lsp.path', null);
    if (userPath && userPath.trim().length > 0) {
        const resolved = userPath.trim();
        if (fs.existsSync(resolved)) {
            outputChannel?.appendLine(`[Nova] Binary: user-configured → ${resolved}`);
            return resolved;
        } else {
            outputChannel?.appendLine(
                `[Nova] Warning: nova.lsp.path = "${resolved}" does not exist — falling back.`
            );
        }
    }

    // 2. PATH lookup
    const fromPath = lookupInPath(binaryName);
    if (fromPath) {
        outputChannel?.appendLine(`[Nova] Binary: PATH → ${fromPath}`);
        return fromPath;
    }

    // 3. Workspace-relative
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (workspaceFolders) {
        for (const folder of workspaceFolders) {
            for (const variant of ['release', 'debug']) {
                const candidate = path.join(
                    folder.uri.fsPath, 'target', variant, binaryName
                );
                if (fs.existsSync(candidate)) {
                    outputChannel?.appendLine(
                        `[Nova] Binary: workspace target/${variant} → ${candidate}`
                    );
                    return candidate;
                }
            }
        }
    }

    // 4. Not found
    outputChannel?.appendLine('[Nova] Binary: not found in setting / PATH / workspace.');
    return null;
}

function lookupInPath(binaryName: string): string | null {
    try {
        const command = process.platform === 'win32' ? 'where' : 'which';
        const result = child_process.execSync(`${command} ${binaryName}`, {
            encoding: 'utf8',
            timeout: 3000,
            stdio: ['ignore', 'pipe', 'ignore'],
        });
        const firstLine = result.trim().split('\n')[0].trim();
        if (firstLine && fs.existsSync(firstLine)) {
            return firstLine;
        }
    } catch {
        // binary not in PATH — expected, not an error
    }
    return null;
}

// ─────────────────────────────────────────────────────────────
// Export client for testing
// ─────────────────────────────────────────────────────────────

/** Returns the active LanguageClient instance, or undefined if not running. */
export function getClient(): LanguageClient | undefined {
    return client;
}
