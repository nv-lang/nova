#!/bin/sh
# Zapusk iz kornya dereva novac (put k dereву peredayotsya $1).
R="${1:-/d/Sources/nv-lang/nova-p274}"
cd "$R" || exit 2
D="$(cd "$(dirname "$0")" && pwd)"
echo '== podozrevaemaya forma =='
NOVAC_SELF_PATH=novac/src NOVAC_UNIT=1 ./novac/target/novac.exe check "$D/p.nv"
echo "rc=$?"
echo '== KONTROL: sosednyaya forma, kotoraya DOLZHNA prinimatsya =='
NOVAC_SELF_PATH=novac/src NOVAC_UNIT=1 ./novac/target/novac.exe check "$D/control.nv"
echo "rc=$?"
