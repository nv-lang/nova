#!/bin/sh
# probe: p_check-novac-no-string-keys_othersyntax
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-no-string-keys.py" "$D/root" "$D/root/src"
echo "rc=$?"
