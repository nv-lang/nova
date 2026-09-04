#!/bin/sh
# probe: p_check-novac-frontend-shape_othersyntax
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-frontend-shape.py" "$D/root" "$D/root/src"
echo "rc=$?"
