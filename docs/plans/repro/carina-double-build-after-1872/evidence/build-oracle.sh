#!/bin/bash
set -eu
S="$(cd "$(dirname "$0")" && pwd)"
TREE=/d/Sources/nv-lang/worktrees/nova-opencode-54-bootstrap-source
export TMPDIR="$S" TEMP="$(cygpath -w "$S")" TMP="$(cygpath -w "$S")"
export CARGO_TARGET_DIR="$(cygpath -w "$S/cargo-target")"
unset GC_DONT_GC DOUBLE_BUILD_A DOUBLE_BUILD_SABOTAGE
cd "$TREE"
{
  git rev-parse HEAD
  git status --porcelain
  git diff --stat 02908e5ad35007d82a3b23eb27c319451563807d
  rustc -Vv
  cargo -V
  printf 'TMPDIR=%s\nTEMP=%s\nTMP=%s\nCARGO_TARGET_DIR=%s\n' "$TMPDIR" "$TEMP" "$TMP" "$CARGO_TARGET_DIR"
  sha256sum scripts/tools/double-build.sh "$S/scripts/tools/double-build.sh"
  diff -qr scripts "$S/scripts"
} > "$S/environment.txt" 2>&1
echo 'Building fresh release oracle for task 54'
start=$SECONDS
set +e
cargo build --release --manifest-path "$TREE/nova-cli/Cargo.toml" > "$S/cargo-build.log" 2>&1
rc=$?
printf 'cargo rc=%s duration_seconds=%s\n' "$rc" "$((SECONDS-start))" | tee "$S/cargo-result.txt"
tail -n 20 "$S/cargo-build.log"
if [ "$rc" = 0 ]; then sha256sum "$TREE/nova-cli/target/release/nova.exe" | tee "$S/oracle.sha256"; fi
exit "$rc"
