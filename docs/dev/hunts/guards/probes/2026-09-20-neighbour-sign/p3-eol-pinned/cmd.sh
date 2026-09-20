#!/usr/bin/env bash
# Проба к находке 3: check-script-eol-pinned.py спрашивает `git check-attr` про
# РАБОЧУЮ КОПИЮ, то есть меряет атрибут, СОБРАННЫЙ В ЭТОЙ СРЕДЕ, а не то, что
# получит чистый чекаут. НЕзакоммиченный (или вовсе неотслеживаемый)
# `.gitattributes` красит стража в зелёный — ровно в том классе «локально
# зелено, на CI красно», ради которого страж и заведён.
#
# Запуск:  bash cmd.sh <КОРЕНЬ-РЕПОЗИТОРИЯ>
set -u
REPO="${1:?ukazhi koren repozitoriya nova pervym argumentom}"
GUARD="$REPO/scripts/guards/check-script-eol-pinned.py"
[ -f "$GUARD" ] || { echo "net $GUARD" >&2; exit 2; }

HERE="$(cd "$(dirname "$0")" && pwd)"
T="$HERE/tree"; rm -rf "$T" "$HERE/clone"; mkdir -p "$T/scripts"
git init -q "$T"
git -C "$T" config user.email probe@example.com
git -C "$T" config user.name probe
git -C "$T" config core.autocrlf true
printf '#!/bin/sh\nfor x in a b; do\n  echo "$x"\ndone\n' > "$T/scripts/foo.sh"
git -C "$T" add scripts/foo.sh
git -C "$T" -c core.autocrlf=false commit -q -m init

echo "=== A. nichego ne zakrepleno -> ozhidaem KRASNYY"
python "$GUARD" "$T"; echo "rc=$?"

echo
echo "=== B. .gitattributes sozdan, NO NE ZAKOMMICHEN (dazhe ne dobavlen)"
printf '*.sh text eol=lf\n' > "$T/.gitattributes"
echo "-- git status vidit ego kak neotslezhivaemyy:"
git -C "$T" status --porcelain | sed 's/^/   /'
echo "-- verdikt strazha:"
python "$GUARD" "$T"; echo "rc=$?"

echo
echo "=== C. PREDMET: chto na samom dele privezet chistyy chekaut"
git -c core.autocrlf=true clone -q "$T" "$HERE/clone"
echo -n "   okonchaniya strok v clone/scripts/foo.sh: "
if grep -qU $'\r' "$HERE/clone/scripts/foo.sh"; then echo "CRLF (predmet NARUSHEN)"; else echo "LF"; fi
od -c "$HERE/clone/scripts/foo.sh" | head -3 | sed 's/^/   /'

echo
echo "=== D. to zhe cherez .git/info/attributes (nikogda ne razdelyaetsya)"
rm -f "$T/.gitattributes"
printf '*.sh text eol=lf\n' > "$T/.git/info/attributes"
echo "-- git status chist:"
git -C "$T" status --porcelain | sed 's/^/   /'
echo "-- verdikt strazha:"
python "$GUARD" "$T"; echo "rc=$?"
exit 0
