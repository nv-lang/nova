#!/bin/bash
set -eu
S="$(cd "$(dirname "$0")" && pwd)"
export DOUBLE_BUILD_TREE=/d/Sources/nv-lang/worktrees/nova-opencode-54-bootstrap-source
SHORT="$(cat "$S/env-probe-deadline/short-path.txt")"
export TMPDIR="$(cygpath -m "$SHORT")" TEMP="$SHORT" TMP="$SHORT"
unset DOUBLE_BUILD_A GC_DONT_GC DOUBLE_BUILD_SABOTAGE BASH_XTRACEFD
cd "$DOUBLE_BUILD_TREE"
[ ! -e "$S/target/double-build" ] || { echo 'Previous artifacts exist: preserve before another run'; exit 2; }
{
  git rev-parse HEAD
  git status --porcelain
  sha256sum nova-cli/target/release/nova.exe
  diff -qr scripts "$S/scripts"
  printf 'DOUBLE_BUILD_TREE=%s\nTMPDIR=%s\nTEMP=%s\nTMP=%s\n' "$DOUBLE_BUILD_TREE" "$TMPDIR" "$TEMP" "$TMP"
  printf 'DOUBLE_BUILD_A=unset\nGC_DONT_GC=unset\nDOUBLE_BUILD_SABOTAGE=unset\n'
  . scripts/guards/lib/novac.sh
  novac_borrow_main_gc "$DOUBLE_BUILD_TREE" 'environment:'
  printf 'NOVA_GC_LIB_DIR=%s\nNOVA_GC_INCLUDE_DIR=%s\nNOVA_CLANG=%s\n' "${NOVA_GC_LIB_DIR:-}" "${NOVA_GC_INCLUDE_DIR:-}" "${NOVA_CLANG:-}"
  if [ -n "${NOVA_GC_LIB_DIR:-}" ]; then sha256sum "$NOVA_GC_LIB_DIR/gc.lib" "$NOVA_GC_INCLUDE_DIR/gc.h"; fi
  "${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}" --version
} > "$S/double-build-environment.txt" 2>&1
echo 'Starting standard double-build A -> B -> C, fresh A and normal GC'
export PS4='+ ${EPOCHREALTIME} ${BASH_SOURCE}:${LINENO}: '
start=$SECONDS
set +e
bash -x "$S/scripts/tools/double-build.sh" > "$S/double-build.log" 2>&1
rc=$?
printf 'double-build rc=%s duration_seconds=%s\n' "$rc" "$((SECONDS-start))" | tee "$S/double-build-result.txt"
cat "$S/double-build.log"
[ ! -f "$S/target/double-build-verdict.txt" ] || cat "$S/target/double-build-verdict.txt"
exit "$rc"
