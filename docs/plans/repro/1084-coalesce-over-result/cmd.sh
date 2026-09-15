#!/bin/sh
# Run from anywhere; the script finds the repository root itself.
#
# Three probes that separate THREE questions about `??` over a `Result`. They
# are RUN, not read -- which is why they keep the bare `.nv` suffix (rule #695
# item 2a) and why this file exists: the suffix guard takes a recorded command
# as the mechanical sign of a probe that is invoked.
#
# Expected today, one line per probe:
#   1 -- refused for INFERENCE, not for `Result`:
#        "the type argument of this sum cannot be inferred from the payload" (E2-b3)
#   2 -- EXACTLY ONE refusal: "`??` unwraps an `Option`, and this left side is not one"
#   3 -- empty, rc=0 (the control alone is green)
#
# The reading of the three together is in README.md; stopping at probe 1 alone
# gives the WRONG conclusion about how much work the wave is.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
cd "$R" || exit 2
for f in 1-inference-blocks-construction 2-the-refusal-and-its-control 3-control-alone-is-green; do
    echo "=== $f"
    ./novac/target/novac.exe check "$D/$f.nv"
    echo "rc=$?"
done
