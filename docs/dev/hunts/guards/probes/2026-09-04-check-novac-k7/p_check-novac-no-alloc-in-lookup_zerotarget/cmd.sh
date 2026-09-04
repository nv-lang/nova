#!/bin/sh
# probe: p_check-novac-no-alloc-in-lookup_zerotarget
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-no-alloc-in-lookup.py" "$D/root" "$D/root/src"
echo "rc=$?"
