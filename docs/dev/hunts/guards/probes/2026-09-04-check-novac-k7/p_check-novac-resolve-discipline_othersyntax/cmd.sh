#!/bin/sh
# probe: p_check-novac-resolve-discipline_othersyntax
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-resolve-discipline.py" "$D/root" "$D/root/src"
echo "rc=$?"
