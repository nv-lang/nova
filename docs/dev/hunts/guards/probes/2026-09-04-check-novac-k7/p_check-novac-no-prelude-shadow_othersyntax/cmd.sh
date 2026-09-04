#!/bin/sh
# probe: p_check-novac-no-prelude-shadow_othersyntax
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../../../../.." && pwd)"
python "$R/scripts/guards/check-novac-no-prelude-shadow.py" "$D/root" "$D/root/src" "$D/root/prelude"
echo "rc=$?"
