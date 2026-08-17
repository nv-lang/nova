#!/bin/sh
# Самотест check-novac-branch-complete (П16). Красноту доказываем МУТАЦИЕЙ
# ПОДСУДИМОГО: добавляем в подложное дерево неполное ветвление и ждём роста
# счётчика выше базы; отдельно проверяем, что все три ПОЛНЫЕ формы страж
# принимает и на них счётчик не растёт.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-branch-complete.sh"
T="${TMPDIR:-/tmp}/selftest-branch.$$"
mkdir -p "$T/src/m" "$T/scripts/guards"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

printf 'incomplete-branches 0\n' > "$T/scripts/guards/novac-branch.baseline"

# три ПОЛНЫЕ формы: терминатор, else с телом, пустой else с причиной
cat > "$T/src/m/a.nv" <<'NV'
module m

fn a(x int) -> int {
    if x > 0 { return 1 }
    2
}

fn b(x int) -> int {
    if x > 0 { 1 } else { 2 }
}

fn c(x int) -> () {
    if x > 0 {
        println(1)
    } else {
        // nothing to do: a non-positive x is already reported upstream
    }
}
NV
sh "$G" "$T" "$T/src" >/dev/null 2>&1 && ok "три полные формы -> зелено" || bad "полные формы покрасили"

# НЕПОЛНОЕ ветвление: счётчик растёт выше базы
cat >> "$T/src/m/a.nv" <<'NV'

fn d(x int) -> () {
    if x > 0 {
        println(1)
    }
}
NV
OUT=$(sh "$G" "$T" "$T/src" 2>&1); RC=$?
if [ "$RC" -eq 0 ]; then
    bad "неполное ветвление прошло зелёным"
else
    printf '%s' "$OUT" | grep -q "РОСТ" && ok "неполное ветвление -> красный, причина названа" || bad "красный, но причина не названа"
fi

# база отсутствует -> красный, а не «нечего судить»
rm -f "$T/scripts/guards/novac-branch.baseline"
sh "$G" "$T" "$T/src" >/dev/null 2>&1 && bad "без базы прошло зелёным" || ok "нет базы -> красный"

[ "$fails" -eq 0 ] && echo "test-check-novac-branch-complete ok" && exit 0
echo "test-check-novac-branch-complete FAIL: $fails" >&2
exit 1
