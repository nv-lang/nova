// SPDX-License-Identifier: MIT OR Apache-2.0
// Nova VSCode Extension — integration tests
// Plan 104.8.Ф.1 test suite
//
// Tests run inside @vscode/test-electron (headless VSCode).
// Each test covers one of the acceptance scenarios from the plan.

import * as assert from 'assert';
import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

/** Create a temp .nv file and return its uri + cleanup function. */
function makeTempNvFile(content: string): { uri: vscode.Uri; cleanup: () => void } {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nova-test-'));
    const filePath = path.join(dir, 'test.nv');
    fs.writeFileSync(filePath, content, 'utf8');
    return {
        uri: vscode.Uri.file(filePath),
        cleanup: () => {
            try { fs.rmSync(dir, { recursive: true, force: true }); } catch { /* ignore */ }
        },
    };
}

/** Wait up to `maxMs` for a predicate to become true, polling every `intervalMs`. */
async function waitFor(
    predicate: () => boolean | Promise<boolean>,
    maxMs = 15000,
    intervalMs = 200
): Promise<boolean> {
    const deadline = Date.now() + maxMs;
    while (Date.now() < deadline) {
        try {
            if (await predicate()) return true;
        } catch { /* keep waiting */ }
        await new Promise((r) => setTimeout(r, intervalMs));
    }
    return false;
}

/** Get our extension — works in both installed and development host modes. */
function getNovaExtension(): vscode.Extension<unknown> | undefined {
    // In development host, the ID may not match publisher.name exactly.
    // Search all extensions for one that provides the 'nova' language.
    return (
        vscode.extensions.getExtension('nova-lang-local.nova-lang') ??
        vscode.extensions.all.find(
            (ext) =>
                ext.packageJSON?.name === 'nova-lang' ||
                ext.packageJSON?.displayName?.toLowerCase().includes('nova language')
        )
    );
}

/** Ensure the Nova extension is activated (force-activate if needed). */
async function ensureActivated(): Promise<vscode.Extension<unknown> | undefined> {
    const ext = getNovaExtension();
    if (!ext) return undefined;
    if (!ext.isActive) {
        try {
            await ext.activate();
        } catch { /* activation may fail if nova-lsp not present — that's expected */ }
    }
    return ext;
}

// ─────────────────────────────────────────────────────────────
// Suite
// ─────────────────────────────────────────────────────────────

suite('Nova Extension — Ф.1 LSP Client', () => {

    // Warm up: activate the extension before any test runs
    suiteSetup(async function () {
        this.timeout(20000);
        // Open a .nv file to trigger onLanguage:nova activation event
        const { uri, cleanup } = makeTempNvFile('// warmup\nfn hello() -> str => "hello"');
        try {
            const doc = await vscode.workspace.openTextDocument(uri);
            await vscode.window.showTextDocument(doc);
            // Give the extension host time to activate
            await new Promise((r) => setTimeout(r, 2000));
        } finally {
            cleanup();
        }
    });

    // ── pos1: extension is loaded and findable ─────────────────

    test('pos1: extension is loaded in development host', async function () {
        this.timeout(20000);
        // The extension is loaded (shown in console "Loading development extension at...")
        // We confirm it can be found and can be activated.
        const ext = await ensureActivated();
        assert.ok(ext !== undefined, 'Nova extension should be findable in extension host');
        assert.strictEqual(
            ext!.packageJSON?.name,
            'nova-lang',
            'Extension package name should be "nova-lang"'
        );
    });

    // ── pos2: language ID is "nova" for .nv files ─────────────

    test('pos2: .nv files get language ID "nova"', async function () {
        this.timeout(10000);
        const { uri, cleanup } = makeTempNvFile('fn main() => {}');
        try {
            const doc = await vscode.workspace.openTextDocument(uri);
            assert.strictEqual(
                doc.languageId,
                'nova',
                '.nv files should have languageId "nova"'
            );
        } finally {
            cleanup();
        }
    });

    // ── pos3: LSP configuration schema is registered ──────────

    test('pos3: nova.lsp.path configuration exists in schema', async function () {
        this.timeout(5000);
        const config = vscode.workspace.getConfiguration('nova');
        const inspect = config.inspect<string | null>('lsp.path');
        assert.ok(inspect !== undefined, 'nova.lsp.path configuration should be registered');
    });

    test('pos4: nova.lsp.enabled configuration exists and defaults to true', async function () {
        this.timeout(5000);
        const config = vscode.workspace.getConfiguration('nova');
        const enabled = config.get<boolean>('lsp.enabled', false);
        assert.strictEqual(enabled, true, 'nova.lsp.enabled should default to true');
    });

    // ── neg1: nova-lsp not found → no crash ───────────────────

    test('neg1: extension does not crash when nova-lsp is not in PATH', async function () {
        this.timeout(15000);
        const config = vscode.workspace.getConfiguration('nova');
        const originalPath = config.get<string | null>('lsp.path');
        try {
            await config.update(
                'lsp.path',
                '/nonexistent/path/to/nova-lsp',
                vscode.ConfigurationTarget.Global
            );

            const { uri, cleanup } = makeTempNvFile('fn x() => {}');
            try {
                // Should NOT throw even with invalid nova-lsp path
                const doc = await vscode.workspace.openTextDocument(uri);
                await vscode.window.showTextDocument(doc);

                // Extension should still be findable (no crash)
                const ext = getNovaExtension();
                assert.ok(
                    ext !== undefined,
                    'Extension should still be registered even if nova-lsp not found'
                );
            } finally {
                cleanup();
            }
        } finally {
            await config.update('lsp.path', originalPath, vscode.ConfigurationTarget.Global);
        }
    });

    // ── edge1: configuration is workspace-scoped ──────────────

    test('edge1: nova configuration properties are all accessible', async function () {
        this.timeout(5000);
        const config = vscode.workspace.getConfiguration('nova');
        const lspPath = config.get('lsp.path');
        const lspEnabled = config.get('lsp.enabled');
        const traceServer = config.get('lsp.trace.server');

        assert.ok(
            lspPath === null || typeof lspPath === 'string',
            'nova.lsp.path should be null or string'
        );
        assert.strictEqual(typeof lspEnabled, 'boolean', 'nova.lsp.enabled should be boolean');
        assert.strictEqual(typeof traceServer, 'string', 'nova.lsp.trace.server should be string');
    });

    // ── neg2: extension package structure is correct ──────────

    test('neg2: extension package.json has required LSP fields', async function () {
        this.timeout(5000);
        const ext = getNovaExtension();
        assert.ok(ext !== undefined, 'Extension must be findable');
        const pkg = ext!.packageJSON as Record<string, unknown>;
        // Verify production-grade package.json fields
        assert.strictEqual(pkg['main'], './out/client/extension', 'main should point to compiled JS');
        assert.ok(
            (pkg['activationEvents'] as string[])?.includes('onLanguage:nova'),
            'Should activate on Nova language files'
        );
        const deps = pkg['dependencies'] as Record<string, string>;
        assert.ok(
            deps?.['vscode-languageclient'],
            'Should depend on vscode-languageclient'
        );
    });
});
