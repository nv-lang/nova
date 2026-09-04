#!/bin/sh
# probe: p_check-novac-arch-invariants_zerotarget
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-arch-invariants.py" "$D/root" "$D/root/arch.md"
echo "rc=$?"
