#!/usr/bin/env bash
# Самотест scripts/tools/merge-precheck.sh, режим --branch (поручение владельца
# 2026-09-23, пункт 3). Настоящие гейты идут минуты — судится ЛОГИКА разбора
# отказов на три кучи, поэтому оба гейта в подложке поддельные: печатают строки
# из файлов своего дерева и выходят заданным кодом.
#
# Клетки, законное первым — инструмент, врущий на здоровой ветке, снесут:
#   1. у ветки только унаследованное -> ok, rc=0, строка «УНАСЛЕДОВАНО — 1»;
#   2. три кучи сразу: свой отказ, унаследованный, починенный -> rc=1, каждый в своей;
#   3. цифры в тексте отказа («провалов 3» / «провалов 4») — это ОДИН отказ;
#   4. гейт вышел не нулём БЕЗ строки отказа -> свой отказ, а не тишина;
#   5. рубеж основного гейта на ветке -> предупреждение, что дальше не видно;
#   6. вердикт гейта Карины уведён во временный каталог, а не в /tmp/gate_novac.done;
#   7. база с чистым git-деревом берётся из кэша на втором прогоне, грязная — нет.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TOOL="$ROOT/scripts/tools/merge-precheck.sh"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
export TMPDIR="$T/tmp"; mkdir -p "$TMPDIR"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
has()   { if printf '%s' "$1" | grep -q -- "$2"; then ok "$3"; else bad "$3 (нет '$2' в: $(printf '%s' "$1" | tr '\n' '|' | cut -c1-600))"; fi; }
hasnt() { if printf '%s' "$1" | grep -q -- "$2"; then bad "$3 (есть '$2')"; else ok "$3"; fi; }

# mktree <dir>: дерево с поддельными гейтами. Поведение — файлами:
#   main.fails / novac.fails — строки в stderr, main.rc / novac.rc — код выхода,
#   runs — счётчик прогонов основного гейта, novac.verdict — куда велели писать вердикт.
mktree() {
    local d="$1"
    mkdir -p "$d/scripts"
    cat > "$d/scripts/gate.sh" <<'EOF'
#!/usr/bin/env bash
echo x >> runs
echo "[    1s] == gate: fake step =="
[ -f main.fails ] && cat main.fails >&2
exit "$(cat main.rc 2>/dev/null || echo 0)"
EOF
    cat > "$d/scripts/gate-novac.sh" <<'EOF'
#!/usr/bin/env bash
echo "${NOVA_NOVAC_VERDICT:-<unset>}" > novac.verdict
[ -f novac.fails ] && cat novac.fails >&2
exit "$(cat novac.rc 2>/dev/null || echo 0)"
EOF
}
reset() { rm -rf "$T/br" "$T/base"; mktree "$T/br"; mktree "$T/base"; }
run() { OUT=$(bash "$TOOL" --branch "$T/br" --base "$T/base" 2>&1); RC=$?; }

INH='GATE FAIL: мёртвых ссылок прибавило (было 3, стало 5)'

echo "== 1. только унаследованное — ok =="
reset
printf '%s\n' "$INH" > "$T/br/main.fails"; echo 1 > "$T/br/main.rc"
printf '%s\n' "$INH" > "$T/base/main.fails"; echo 1 > "$T/base/main.rc"
run
[ "$RC" -eq 0 ] && ok "1: rc=0 при одном унаследованном" || bad "1: rc=$RC: $OUT"
has "$OUT" 'УНАСЛЕДОВАНО (красно на обеих — долг базы) — 1' "1: отказ в куче УНАСЛЕДОВАНО"
has "$OUT" 'СВОИ ВЕТКИ (красно на ветке, зелено на базе) — 0' "1: своих ноль"
has "$OUT" 'merge-precheck ok:' "1: строка ok напечатана"

echo "== 2. три кучи сразу =="
reset
printf '%s\n' "$INH" 'GATE FAIL: страж новой ветки' > "$T/br/main.fails"; echo 1 > "$T/br/main.rc"
printf '%s\n' "$INH" > "$T/base/main.fails"; echo 1 > "$T/base/main.rc"
printf '%s\n' 'NOVAC-GATE FAIL: долг базы, который ветка закрыла' > "$T/base/novac.fails"; echo 1 > "$T/base/novac.rc"
run
[ "$RC" -eq 1 ] && ok "2: rc=1 при своём отказе" || bad "2: rc=$RC: $OUT"
has "$OUT" 'СВОИ ВЕТКИ (красно на ветке, зелено на базе) — 1' "2: один свой"
has "$OUT" '   GATE FAIL: страж новой ветки' "2: свой назван текстом"
has "$OUT" 'ВЕТКА ПОЧИНИЛА (красно на базе, зелено на ветке) — 1' "2: один починенный"
has "$OUT" 'долг базы, который ветка закрыла' "2: починенный назван текстом"
hasnt "$OUT" 'merge-precheck ok:' "2: ok НЕ напечатано"

echo "== 3. цифры в тексте не делят отказ надвое =="
reset
printf '%s\n' 'GATE FAIL: mega-CU: провалов 4' > "$T/br/main.fails"; echo 1 > "$T/br/main.rc"
printf '%s\n' 'GATE FAIL: mega-CU: провалов 3' > "$T/base/main.fails"; echo 1 > "$T/base/main.rc"
run
[ "$RC" -eq 0 ] && ok "3: разные числа — один унаследованный отказ" || bad "3: rc=$RC: $OUT"

echo "== 4. отказ без строки отказа — громко =="
reset
echo 3 > "$T/br/novac.rc"
run
[ "$RC" -eq 1 ] && ok "4: rc≠0 без строки — свой отказ" || bad "4: rc=$RC: $OUT"
has "$OUT" 'не напечатав ни одной строки отказа' "4: причина названа"

echo "== 5. рубеж на ветке =="
reset
printf '%s\n' 'GATE FAIL: что-то' 'GATE: отказов на этом рубеже — 1:' > "$T/br/main.fails"; echo 1 > "$T/br/main.rc"
run
has "$OUT" 'на ветке гейт встал на рубеже' "5: предупреждение о невидимом хвосте"

echo "== 6. вердикт гейта Карины уведён =="
v=$(cat "$T/br/novac.verdict" 2>/dev/null)
case "$v" in
    ''|'<unset>'|/tmp/gate_novac.done) bad "6: гейт Карины писал бы в '$v'" ;;
    *) ok "6: вердикт уведён во временный путь" ;;
esac

echo "== 7. кэш базы: чистое дерево — да, грязное — нет =="
reset
G="git -C $T/base -c user.name=selftest -c user.email=selftest@example.invalid"
if $G init -q 2>/dev/null && $G add -A && $G commit -q -m base 2>/dev/null; then
    run; run
    n=$(grep -c . "$T/base/runs")
    [ "$n" -eq 1 ] && ok "7: второй прогон базы взят из кэша (прогонов 1)" || bad "7: прогонов базы $n, ждал 1: $OUT"
    has "$OUT" 'прогон взят из кэша' "7: кэш назван строкой"
    echo change >> "$T/base/scripts/gate-novac.sh"
    run
    n=$(grep -c . "$T/base/runs")
    [ "$n" -eq 2 ] && ok "7: грязная база прогнана заново" || bad "7: прогонов базы $n, ждал 2"
    has "$OUT" 'в кэш НЕ кладётся' "7: грязь названа строкой"
else
    bad "7: подложке не удалось завести git — кэш не доказан"
fi

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-merge-precheck ok: $PASS/$PASS"
    exit 0
fi
exit 1
