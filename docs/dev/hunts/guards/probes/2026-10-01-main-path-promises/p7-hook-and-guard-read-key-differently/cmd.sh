#!/usr/bin/env bash
# p7 -- K4: the escape key NOVA_PUSH_UNPROVEN is judged by TWO predicates.
#   check-push-proven-by-ci.py line 170:  re.search(u"[#№]\\d{2,5}", esc)
#   scripts/githooks/pre-push (no-python branch):  case ... in *[#][0-9][0-9]*)
# The guard accepts the number sign "№" (U+2116), the hook's fallback does not.
# Both are reached by `git push ... main` with the same key value; which one
# answers depends only on whether a working python is on PATH.
#
# Setup (all inside work-repo/, built here): a throw-away git repo holding a copy
# of the REAL pre-push and the REAL guard; stubbin/ shadows python3 and python
# with stubs that print nothing (the Microsoft Store stub shape the hook itself
# names), so the hook takes its no-python branch. NOVA_SKIP_CI_CHECK=1 keeps the
# later check-ci-status.sh (network) out of the way -- it does NOT affect the
# proof step (hook comment and selftest say so).
#
# A: guard directly, key "github down №1442"            -> ?
# B: pre-push without python, same key                   -> ?
# C: pre-push without python, key "github down #1442"    -> control, expect pass
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
HOOK="$ROOT/scripts/githooks/pre-push"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
KEY_NUMSIGN="$(cat "$HERE/key-numsign.txt")"
for f in "$G" "$HOOK" "$HERE/key-numsign.txt"; do [ -s "$f" ] || { echo "MISSING $f"; exit 2; }; done
W="$HERE/work-repo"; B="$HERE/stubbin"
rm -rf "$W" "$B"; mkdir -p "$W/scripts/githooks" "$W/scripts/guards" "$B"
git -C "$W" init -q
cp "$HOOK" "$W/scripts/githooks/pre-push"; cp "$G" "$W/scripts/guards/"
printf '#!/bin/sh\nexit 0\n' > "$B/python3"; printf '#!/bin/sh\nexit 0\n' > "$B/python"; chmod +x "$B/python3" "$B/python"
[ -s "$W/scripts/githooks/pre-push" ] && [ -x "$B/python3" ] || { echo "MISSING setup"; exit 2; }
echo "stub check: python3 -c 'print(42)' under stub PATH prints: [$(PATH="$B:$PATH" python3 -c 'print(42)')]"
echo "=== A. guard directly, key: $KEY_NUMSIGN"
NOVA_PUSH_UNPROVEN="$KEY_NUMSIGN" "$PY" "$G" "$SHA" 2>&1; echo "rc=$?"
echo "=== B. pre-push, no python, key: $KEY_NUMSIGN"
( cd "$W" && printf 'refs/heads/main %s refs/heads/main %s\n' "$SHA" 0000000000000000000000000000000000000000 \
  | PATH="$B:$PATH" NOVA_SKIP_CI_CHECK=1 NOVA_PUSH_UNPROVEN="$KEY_NUMSIGN" bash scripts/githooks/pre-push origin url 2>&1 ); echo "rc=$?"
echo "=== C. pre-push, no python, key: github down #1442"
( cd "$W" && printf 'refs/heads/main %s refs/heads/main %s\n' "$SHA" 0000000000000000000000000000000000000000 \
  | PATH="$B:$PATH" NOVA_SKIP_CI_CHECK=1 NOVA_PUSH_UNPROVEN="github down #1442" bash scripts/githooks/pre-push origin url 2>&1 ); echo "rc=$?"
