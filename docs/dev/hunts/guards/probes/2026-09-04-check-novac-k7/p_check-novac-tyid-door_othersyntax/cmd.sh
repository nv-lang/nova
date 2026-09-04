#!/bin/sh
# probe: p_check-novac-tyid-door_othersyntax
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-tyid-door.py" "$D/root" "$D/root"
echo "rc=$?"
