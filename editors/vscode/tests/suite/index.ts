// SPDX-License-Identifier: MIT OR Apache-2.0
// Mocha test suite entry point for Nova VSCode extension tests.

import * as path from 'path';
import * as fs from 'fs';
import Mocha from 'mocha';

export async function run(): Promise<void> {
    const mocha = new Mocha({
        ui: 'tdd',
        color: true,
        timeout: 30000, // 30s — LSP start can take time
    });

    const testsRoot = path.resolve(__dirname, '.');

    // Find all compiled test files (*.test.js) in this directory
    const files = fs
        .readdirSync(testsRoot)
        .filter((f) => f.endsWith('.test.js'));

    for (const file of files) {
        mocha.addFile(path.resolve(testsRoot, file));
    }

    return new Promise<void>((resolve, reject) => {
        mocha.run((failures) => {
            if (failures > 0) {
                reject(new Error(`${failures} test(s) failed.`));
            } else {
                resolve();
            }
        });
    });
}
