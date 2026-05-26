// SPDX-License-Identifier: MIT OR Apache-2.0
// Nova VSCode Extension — test runner
// Uses @vscode/test-electron to run tests inside a downloaded VSCode instance.
//
// PROBLEM: Claude Code (and other Electron-based shells) set ELECTRON_RUN_AS_NODE=1.
// This causes Code.exe to start in Node.js CLI mode and reject all GUI arguments.
//
// SOLUTION: Use a thin batch-file wrapper as vscodeExecutablePath.
// The wrapper does `set ELECTRON_RUN_AS_NODE=` (clears it) then calls Code.exe %*.
// cmd.exe's %* passes all arguments through intact.

import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { runTests, downloadAndUnzipVSCode } from '@vscode/test-electron';

async function main(): Promise<void> {
    try {
        const extensionDevelopmentPath = path.resolve(__dirname, '../../');
        const extensionTestsPath = path.resolve(__dirname, 'suite/index');

        console.log('[nova-test-runner] extensionDevelopmentPath:', extensionDevelopmentPath);
        console.log('[nova-test-runner] extensionTestsPath:', extensionTestsPath);

        // Download (or reuse cached) VSCode
        const vscodeExecutablePath = await downloadAndUnzipVSCode();
        console.log('[nova-test-runner] Code.exe:', vscodeExecutablePath);

        // Create wrapper that clears ELECTRON_RUN_AS_NODE before launching Code.exe.
        // On Windows, `set VAR=` (no value) unsets the variable for all child processes.
        const wrapperPath = path.join(os.tmpdir(), 'nova-test-vscode-launcher.bat');
        const wrapperContent = [
            '@echo off',
            'set ELECTRON_RUN_AS_NODE=',
            `"${vscodeExecutablePath}" %*`,
        ].join('\r\n') + '\r\n';
        fs.writeFileSync(wrapperPath, wrapperContent, 'utf8');
        console.log('[nova-test-runner] Wrapper:', wrapperPath);

        await runTests({
            extensionDevelopmentPath,
            extensionTestsPath,
            vscodeExecutablePath: wrapperPath,
        });
    } catch (err) {
        console.error('[nova-test-runner] FAILED:', err);
        process.exit(1);
    }
}

main();
