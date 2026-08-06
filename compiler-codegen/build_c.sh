#!/usr/bin/env bash
# build_c.sh — wrapper for full Nova build pipeline (.nv -> .c -> binary)
#
# Usage:
#     ./build_c.sh hello.nv                  # produces ./hello in same dir
#     ./build_c.sh hello.nv --run            # build and run
#     ./build_c.sh hello.nv -o out           # custom output path
#     ./build_c.sh hello.nv --keep-c         # don't delete intermediate .c
#     ./build_c.sh hello.nv --cc clang       # use clang instead of gcc
#
# Pipeline:
#     1. nova-codegen compile <file>.nv  -> <file>.c
#     2. gcc/clang link with nova_rt     -> <file>
#     3. (optional) run <file>
#
# Requirements:
#     - nova-codegen built: cargo build (in compiler-codegen/)
#     - gcc or clang in PATH

set -euo pipefail

# Resolve compiler-codegen root (where this script lives)
COMPILER_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CODEGEN="$COMPILER_ROOT/target/debug/nova-codegen"
RT_DIR="$COMPILER_ROOT/nova_rt"

# Defaults
RUN=0
KEEP_C=0
CC="${CC:-gcc}"
OUTPUT=""
INPUT=""

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --run)        RUN=1; shift ;;
        --keep-c)     KEEP_C=1; shift ;;
        --cc)         CC="$2"; shift 2 ;;
        -o)           OUTPUT="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,15p' "$0"
            exit 0
            ;;
        -*)
            echo "error: unknown option: $1" >&2
            exit 1
            ;;
        *)
            if [[ -z "$INPUT" ]]; then INPUT="$1"; else echo "error: too many positional args" >&2; exit 1; fi
            shift
            ;;
    esac
done

if [[ -z "$INPUT" ]]; then
    echo "error: no input file" >&2
    echo "usage: $0 <file.nv> [--run] [-o out] [--keep-c] [--cc gcc|clang]" >&2
    exit 1
fi

if [[ ! -f "$CODEGEN" ]]; then
    echo "error: nova-codegen not found at $CODEGEN" >&2
    echo "       run 'cargo build' in $COMPILER_ROOT first" >&2
    exit 1
fi

if ! command -v "$CC" >/dev/null 2>&1; then
    echo "error: C compiler '$CC' not found in PATH" >&2
    exit 1
fi

if [[ ! -f "$INPUT" ]]; then
    echo "error: input file not found: $INPUT" >&2
    exit 1
fi

# Resolve absolute path; on macOS readlink lacks -f, fall back to python.
if command -v realpath >/dev/null 2>&1; then
    NV_FILE="$(realpath "$INPUT")"
else
    NV_FILE="$(cd "$(dirname "$INPUT")" && pwd)/$(basename "$INPUT")"
fi

NV_NAME="$(basename "$NV_FILE" .nv)"
NV_DIR="$(dirname "$NV_FILE")"
C_FILE="$NV_DIR/$NV_NAME.c"
OUT_FILE="${OUTPUT:-$NV_DIR/$NV_NAME}"

# Step 1: .nv -> .c
echo "[1/3] codegen: $NV_FILE -> $C_FILE"
if ! "$CODEGEN" compile "$NV_FILE" 2>&1; then
    echo "codegen failed" >&2
    exit 1
fi
if [[ ! -f "$C_FILE" ]]; then
    echo "codegen produced no .c file at $C_FILE" >&2
    exit 1
fi

# Step 2: .c -> binary
echo "[2/3] $CC: $C_FILE -> $OUT_FILE"
if ! "$CC" \
    "$C_FILE" \
    "$RT_DIR/alloc.c" \
    "$RT_DIR/effects.c" \
    "$RT_DIR/fibers.c" \
    -I"$COMPILER_ROOT" \
    -o "$OUT_FILE" \
    -w; then
    echo "$CC failed" >&2
    exit 1
fi

# Cleanup intermediate .c
if [[ $KEEP_C -eq 0 ]]; then
    rm -f "$C_FILE"
fi

echo "[3/3] built: $OUT_FILE"

# Optional: run
if [[ $RUN -eq 1 ]]; then
    echo
    echo "running $OUT_FILE ..."
    "$OUT_FILE"
    rc=$?
    echo
    echo "exit code: $rc"
    exit $rc
fi
