#!/usr/bin/env bash
# cdb_session.sh — Windows kernel debugger session for crash localization
# of stochastic concurrency bugs. Prepared for Plan 83.11 §12.30
# [M-83.11-supervised-spawn-cancel-memcpy-segv] next-session investigation.
#
# Prerequisite: cdb.exe must be installed. Install via:
#   winget install Microsoft.WindowsSDK.10.0.22621
# Or download Windows SDK from microsoft.com.
#
# ## Usage
#
#   tools/cdb_session.sh <test.nv>
#
# ## What it does
#
#   1. Detects cdb.exe (Windows Kits path, fallback to PATH)
#   2. Builds nova-cli (release) + the test exe with --keep-artifacts
#   3. Launches cdb with crash-on-first-exception, captures full stack
#   4. Reports frame[1] (caller of crashing function — typically memcpy)
#
# ## Why
#
# Plan 83.11 §12.27-29 bisected the bug to dd7a4f00bc5 (Plan 83.11 Ф.3 fix
# commit, 5 files +225/-103). The actual SEGV is in memcpy with WRITE to
# .rdata — caller frame unknown after 8 fix attempts. cdb with proper PDB
# symbol resolution should resolve frame[1] in one session. That caller is
# the keystone for the bug.
#
# ## Output
#
# Stack trace printed to stdout; key fields:
#   - Exception code (should be 0xC0000005 ACCESS_VIOLATION)
#   - Faulting address (should be in .rdata range per §12.9)
#   - Frame 0: memcpy or its inlined wrapper
#   - Frame 1: CALLER — the function passing bad dst ptr
#   - Frame 2+: trace up through cancel/spawn/supervised dispatch

set +e

TEST_FILE="${1:-nova_tests/concurrency/_min.nv}"
if [ ! -f "$TEST_FILE" ]; then
    echo "[cdb] test file not found: $TEST_FILE" >&2
    echo "Create one first, e.g.:" >&2
    cat >&2 << 'EXAMPLE'
mkdir -p nova_tests/concurrency
cat > nova_tests/concurrency/_min.nv << 'NV'
module concurrency._min
test "min" {
    ro tok = CancelToken.new()
    supervised(cancel: tok) {
        spawn { tok.cancel() }
    }
}
NV
EXAMPLE
    exit 1
fi

# Detect cdb.exe
CDB=""
for candidate in \
    "/c/Program Files (x86)/Windows Kits/10/Debuggers/x64/cdb.exe" \
    "/c/Program Files/Windows Kits/10/Debuggers/x64/cdb.exe" \
    "$(command -v cdb.exe 2>/dev/null)" \
    "$(command -v cdb 2>/dev/null)"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
        CDB="$candidate"
        break
    fi
done

if [ -z "$CDB" ]; then
    cat >&2 << 'ERR'
[cdb] cdb.exe not found. Install Windows SDK:

  winget install Microsoft.WindowsSDK.10.0.22621

After install, cdb.exe is typically at:
  C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe

Or set CDB_PATH env var:
  CDB_PATH="/path/to/cdb.exe" tools/cdb_session.sh ...
ERR
    if [ -n "$CDB_PATH" ] && [ -x "$CDB_PATH" ]; then
        CDB="$CDB_PATH"
    else
        exit 125
    fi
fi

echo "[cdb] using: $CDB" >&2

# Environment for Nova build
: "${NOVA_GC_LIB_DIR:=D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib}"
: "${NOVA_GC_INCLUDE_DIR:=D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include}"
export NOVA_GC_LIB_DIR NOVA_GC_INCLUDE_DIR

echo "[cdb] building nova-cli..." >&2
(cd nova-cli && cargo build --release --quiet 2>&1 | tail -3) >&2
if [ $? -ne 0 ]; then
    echo "[cdb] nova-cli build FAIL" >&2
    exit 125
fi

echo "[cdb] building test exe with --keep-artifacts..." >&2
TEST_BASE=$(basename "$TEST_FILE" .nv)
NOVA_LOG=$(mktemp)
./nova-cli/target/release/nova.exe test "$TEST_FILE" --keep-artifacts > "$NOVA_LOG" 2>&1
NOVA_EC=$?

EXE=$(find /tmp/nova_tests -name "${TEST_BASE}.exe" -mmin -2 2>/dev/null | head -1)
if [ -z "$EXE" ]; then
    echo "[cdb] EXE not found (nova test exit=$NOVA_EC)" >&2
    tail -20 "$NOVA_LOG" >&2
    rm -f "$NOVA_LOG"
    exit 125
fi
rm -f "$NOVA_LOG"

EXE_DIR=$(dirname "$EXE")
echo "[cdb] exe: $EXE" >&2
echo "[cdb] pdb dir: $EXE_DIR" >&2

# cdb command sequence:
#   sxe av    — break on first access violation (don't continue past it)
#   g         — go (start execution)
#   .lastevent — show what caused the break
#   r         — registers (RIP, RSP, RAX etc.)
#   kn 30     — stack 30 frames with frame numbers
#   kp        — stack with parameters
#   !analyze -v — full crash analysis (if available)
#   q         — quit
CDB_CMDS='sxe av; g; .lastevent; .echo === REGISTERS ===; r; .echo === STACK kn ===; kn 30; .echo === STACK kp ===; kp; .echo === ANALYZE ===; !analyze -v; q'

# Convert exe path to Windows-style for cdb (if running in MSYS bash)
EXE_WIN=$(cygpath -w "$EXE" 2>/dev/null || echo "$EXE")
PDB_WIN=$(cygpath -w "$EXE_DIR" 2>/dev/null || echo "$EXE_DIR")

echo "[cdb] launching session..." >&2
echo "===== cdb output ====="
"$CDB" -y "$PDB_WIN" -c "$CDB_CMDS" "$EXE_WIN" 2>&1
echo "===== end cdb output ====="

echo "" >&2
echo "[cdb] Key info to extract:" >&2
echo "  - Exception code (.lastevent line)" >&2
echo "  - Faulting address (Access violation - code c0000005 line)" >&2
echo "  - Frame 0: typically memcpy or _RtlMoveMemory" >&2
echo "  - Frame 1: CALLER (this is the keystone!)" >&2
echo "  - Continue up stack until familiar Nova function name" >&2
