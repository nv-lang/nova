#!/usr/bin/env bash
# Самотест check-novac-legacy-workarounds.sh — обе стороны, на фикстурном корне.
# Покрывает ЧЕТЫРЕ живые формы маркера ([LEGACY-#123], [LEGACY-#123-slug],
# [LEGACY-#123-slug until:<этап>] и [LEGACY-#TBD-<slug>]) — четвёртой и
# третьей тут не было до 2026-08-17, и страж их не судил: из пятнадцати
# живых носителей в суд попадали пять.
# все три формы закрытия записи реестра («Статус: ЗАКРЫТ», «✅ ЗАКРЫТ дата»,
# «✅ ЗАКРЫТО дата»), протухший #TBD и живость самой строки-счётчика.
# Возраст #TBD на подложке моделируется через env NOVA_LEGACY_TBD_TIME
# (подмена git blame; см. шапку стража) — в подложке git-истории нет.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-legacy-workarounds.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

NOW=$(date +%s)
FRESH=$((NOW - 86400))          # сутки — свежая заявка на номер
STALE=$((NOW - 9 * 86400))      # девять суток — протухшая

FIX="$TMP/root"; mkdir -p "$FIX/novac/src/lex" "$FIX/docs/plans"
REG="$FIX/docs/plans/221.1-bug-sweep.md"
SITE="$FIX/novac/src/lex/lex.nv"

echo "== проходит =="
sh "$G" "$TMP/empty-root" >/dev/null 2>&1
check "нет novac — зелёный" "$?" "0"

printf '| 900 | K1 | bug demo. Статус: ОТКРЫТ |\n' > "$REG"
printf '// [LEGACY-#900] workaround site\nfn f() -> int => 1\n' > "$SITE"
OUT=$(sh "$G" "$FIX" 2>/dev/null); RC=$?
check "номерной маркер открытого бага — зелёный" "$RC" "0"
has   "счётчик печатается на зелёном" "$OUT" 'налог оракула'
has   "счётчик считает носителей" "$OUT" 'носителей 1 в 1 файлах'

printf '// [LEGACY-#TBD-char-literal-permissive] site\nfn f() -> int => 1\n' > "$SITE"
OUT=$(sh "$G" "$FIX" 2>/dev/null); RC=$?
check "#TBD без git-даты — зелёный (стареть будет с первого коммита)" "$RC" "0"
has   "счётчик видит #TBD-форму (F7: раньше был слеп)" "$OUT" '#TBD: 1'

OUT=$(NOVA_LEGACY_TBD_TIME="$FRESH" sh "$G" "$FIX" 2>/dev/null); RC=$?
check "#TBD моложе порога — зелёный" "$RC" "0"
has   "счётчик печатает возраст старейшего #TBD" "$OUT" 'старейший 1 сут'

printf '// EXPECT_CC_ERROR boom\n// [LEGACY-#TBD-never-in-tail] attributed\nfn f() -> int => 1\n' > "$SITE"
OUT=$(sh "$G" "$FIX" 2>/dev/null); RC=$?
check "EXPECT_CC_ERROR с атрибуцией #TBD — зелёный" "$RC" "0"

printf '// EXPECT_CC_ERROR boom\n// [LEGACY-#900] attributed\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "EXPECT_CC_ERROR с номерной атрибуцией — зелёный" "$?" "0"

echo "== ловит =="
printf '// [LEGACY-#901] workaround site\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "маркер бага без строки в реестре — красный" "$?" "1"

printf '| 901 | K1 | bug demo. Статус: ЗАКРЫТ 2026-08-14 |\n' >> "$REG"
OUT=$(sh "$G" "$FIX" 2>/dev/null); RC=$?
check "закрытый формой «Статус: ЗАКРЫТ» — красный (фоссилизация)" "$RC" "1"
has   "счётчик печатается и на красном" "$OUT" 'налог оракула'

printf '| 900 | K1 | bug demo. Статус: ОТКРЫТ |\n' > "$REG"
printf '| 902 | K1 | bug demo | ✅ ЗАКРЫТ 2026-08-14 (окно p274) |\n' >> "$REG"
printf '// [LEGACY-#902] workaround site\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "закрытый формой «✅ ЗАКРЫТ дата» — красный (F8: 163 записи реестра)" "$?" "1"

printf '| 900 | K1 | bug demo. Статус: ОТКРЫТ |\n' > "$REG"
printf '| 903 | K1 | bug demo | ✅ ЗАКРЫТО 2026-07-24 (окно №72) |\n' >> "$REG"
printf '// [LEGACY-#903] workaround site\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "закрытый формой «✅ ЗАКРЫТО дата» — красный" "$?" "1"

printf '// [LEGACY-#TBD-var-index-method] site\nfn f() -> int => 1\n' > "$SITE"
ERR=$(NOVA_LEGACY_TBD_TIME="$STALE" sh "$G" "$FIX" 2>&1 >/dev/null); RC=$?
check "#TBD старше 3 суток — красный" "$RC" "1"
has   "красный называет адресата эскалации" "$ERR" 'номер обязан прийти от интегратора'
has   "красный называет возраст" "$ERR" 'без номера 9 сут'

printf '// EXPECT_CC_ERROR boom\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "EXPECT_CC_ERROR без маркера — красный" "$?" "1"

rm -f "$REG"
printf '// [LEGACY-#900] workaround site\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "нет реестра — красный" "$?" "1"

echo "== настоящее дерево =="
OUT=$(sh "$G" "$ROOT" 2>/dev/null); RC=$?
check "novac проекта чист" "$RC" "0"
# Счёт #TBD проверяется на СВОЁМ дереве, а не на настоящем: в настоящем их
# может законно не быть (2026-08-16 их не осталось — все маркеры получили
# номера), и тогда тест утверждал бы свойство репозитория, а не стража.
has   "счёт #TBD печатается" "$OUT" '#TBD:'


echo "== формы носителя, которых судья не видел до 2026-08-17 =="

printf '| 901 | K1 | slug form demo. Статус: ОТКРЫТ |\n' > "$REG"
printf '// [LEGACY-#901-some-slug] site\nfn f() -> int => 1\n' > "$SITE"
OUT=$(sh "$G" "$FIX" 2>/dev/null); RC=$?
check "слаговая форма открытого бага — зелёная" "$RC" "0"
has   "слаговая форма ПОПАЛА в счётчик" "$OUT" 'носителей 1 в 1 файлах'

printf '| 902 | K1 | slug form closed. ✅ ЗАКРЫТ 2026-08-01 |\n' > "$REG"
printf '// [LEGACY-#902-some-slug] site\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "слаговая форма ЗАКРЫТОГО бага — красный (правило A видит номер сквозь слаг)" "$?" "1"

echo "== срок until:<этап> =="
mkdir -p "$FIX/novac"
printf '#   stage: E2\n' > "$FIX/novac/nova.toml"

printf '| 903 | K1 | expiring demo. Статус: ОТКРЫТ |\n' > "$REG"
printf '// [LEGACY-#903-user-error-as-ice until:E2b3] site\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "срок ещё не наступил (E2 < E2b3) — зелёный" "$?" "0"

printf '// [LEGACY-#903-user-error-as-ice until:E1] site\nfn f() -> int => 1\n' > "$SITE"
OUT=$(sh "$G" "$FIX" 2>&1); RC=$?
check "срок ИСТЁК (E2 >= E1) — красный" "$RC" "1"
has   "красный называет причину" "$OUT" 'дожил до своего этапа'

printf '// [LEGACY-#903-user-error-as-ice until:E99] site\nfn f() -> int => 1\n' > "$SITE"
sh "$G" "$FIX" >/dev/null 2>&1
check "этап вне порядка — красный" "$?" "1"

printf '// [LEGACY-#903-user-error-as-ice until:E2b3] site\nfn f() -> int => 1\n' > "$SITE"
rm -f "$FIX/novac/nova.toml"
sh "$G" "$FIX" >/dev/null 2>&1
check "есть срочный носитель, а этапа не прочесть — красный, а не 'нечего судить'" "$?" "1"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
