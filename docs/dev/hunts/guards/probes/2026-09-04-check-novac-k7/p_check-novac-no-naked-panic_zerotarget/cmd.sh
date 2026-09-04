#!/bin/sh
# probe: p_check-novac-no-naked-panic_zerotarget
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-no-naked-panic.py" "$D/root"
echo "rc=$?"
