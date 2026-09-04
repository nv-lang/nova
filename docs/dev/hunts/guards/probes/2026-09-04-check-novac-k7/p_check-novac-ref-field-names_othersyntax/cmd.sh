#!/bin/sh
# probe: p_check-novac-ref-field-names_othersyntax
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-ref-field-names.py" "$D/root" "$D/root"
echo "rc=$?"
