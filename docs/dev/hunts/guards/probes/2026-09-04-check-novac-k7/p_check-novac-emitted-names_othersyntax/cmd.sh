#!/bin/sh
# probe: p_check-novac-emitted-names_othersyntax
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-emitted-names.py" "$D/root"
echo "rc=$?"
