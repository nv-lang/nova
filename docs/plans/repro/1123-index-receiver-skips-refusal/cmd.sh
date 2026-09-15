#!/bin/sh
# Run from anywhere; the script finds the repository root itself.
#
# Registry row #1123. FOUR probes that must be run TOGETHER: each one alone
# reads as something else. Each lives in its own directory because a folder is
# ONE module in Nova -- two probes side by side would collide on `main` and the
# run would measure their sum.
#
# Expected today (measured 2026-09-16 by the integrator, binary
# nova-cli/target/release/nova):
#
#   a-literal-index-accepts-retired      built, prints 3
#   b-var-index-answers-container        built, prints 1   <- silently wrong
#   c-binding-refuses-as-it-must         REFUSED [E_STR_NO_LEN] (D249)
#   d-var-index-refuses-the-replacement  REFUSED [E_RECV_METHOD_MISMATCH]
#
# The point is the PAIR a+b against c. The very same call, `str.len()`, is
# refused when the receiver is a binding and accepted when the receiver is an
# index expression -- so the retirement refusal (D249) never runs on the index
# path at all. And d shows the other side of it: the replacement the diagnostic
# itself names cannot be called through a variable index either.
#
# Probe b's number is the container's length, not the string's: the vector
# holds one element and "abc" is three bytes.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
cd "$R" || exit 2
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
NOVA=./nova-cli/target/release/nova
for p in a-literal-index-accepts-retired b-var-index-answers-container \
         c-binding-refuses-as-it-must d-var-index-refuses-the-replacement; do
    echo "=== $p"
    if "$NOVA" build "$D/$p/t.nv" -o "$T/$p.exe" 2>&1 | grep -o '\[E_[A-Z_]*\]' | head -1; then
        :
    fi
    if [ -f "$T/$p.exe" ]; then "$T/$p.exe"; echo "exit=$?"; else echo "not built"; fi
done
