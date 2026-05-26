#!/usr/bin/env sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Nova Neovim smoke test runner — Plan 104.8.Ф.2
#
# Requires nvim in PATH. If not available, exits with 0 and logs a skip notice.
#
# Usage: sh editors/neovim/tests/run_smoke.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SMOKE_LUA="${SCRIPT_DIR}/smoke.lua"

if ! command -v nvim >/dev/null 2>&1; then
    echo "[SKIP] nvim not found in PATH — Nova Neovim smoke tests skipped."
    echo "       To run manually: nvim --headless -u NONE -l ${SMOKE_LUA}"
    echo "       [M-104.8-tool-nvim-unavailable]"
    exit 0
fi

echo "Running Nova Neovim smoke tests..."
nvim --headless -u NONE -l "${SMOKE_LUA}"
STATUS=$?

if [ $STATUS -eq 0 ]; then
    echo "[OK] Neovim smoke tests passed."
else
    echo "[FAIL] Neovim smoke tests failed with exit code $STATUS."
fi

exit $STATUS
