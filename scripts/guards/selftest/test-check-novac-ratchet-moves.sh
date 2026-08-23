#!/usr/bin/env bash
# Самотест check-novac-ratchet-moves.sh — обе стороны, на фикстурном git-дереве.
#
# ЦЕНТРАЛЬНЫЙ СЛУЧАЙ — СДВИГ ВНИЗ СУДИТСЯ ТАК ЖЕ. Сужение храповика выглядит
# безобидным («стало лучше»), но без причины оно ровно так же скрывает: сегодня
# `names 11 -> 10` было законным (удалён экспорт без вызывающего), а могло быть
# подгонкой под удалённое имя. Число изменилось — причина обязана приехать.
#
# Второй — РАБОЧЕЕ ДЕРЕВО, а не только индекс: волна судится до `git add`, иначе
# страж молчит ровно в тот момент, когда решение принимается.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-ratchet-moves.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

R="$TMP/repo"
mkdir -p "$R/scripts/guards"
git -C "$R" init -q 2>/dev/null || git init -q "$R"
git -C "$R" config user.email t@t
git -C "$R" config user.name t
B="$R/scripts/guards/novac-surface.baseline"
printf '# base\nsem 10\ntypes 5\n' > "$B"
git -C "$R" add -A >/dev/null 2>&1
git -C "$R" commit -qm base >/dev/null 2>&1

echo "== проходит =="
bash "$G" "$TMP/nowhere" >/dev/null 2>&1
check "нет git-дерева — зелёный (судить нечего)" "$?" "0"

OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "дифф пуст — зелёный" "$RC" "0"

printf '# base\n# sem 10 -> 11: новое имя наружу, потому что его спрашивает эмиттер\nsem 11\ntypes 5\n' > "$B"
OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "сдвиг ВВЕРХ с причиной — зелёный" "$RC" "0"
has "назвал число файлов со сдвигом" "$OUT" "1"

printf '# base\n# types 5 -> 4: имя удалено, вызывающих не было\nsem 10\ntypes 4\n' > "$B"
OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "сдвиг ВНИЗ с причиной — зелёный" "$RC" "0"

printf '# base\nsem 10\ntypes 5\n# хвостовой комментарий без сдвига\n' > "$B"
OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "комментарий без сдвига числа — зелёный" "$RC" "0"

echo "== краснеет =="
printf '# base\nsem 11\ntypes 5\n' > "$B"
OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "сдвиг ВВЕРХ без причины — красный" "$RC" "1"
has "назвал файл" "$OUT" "novac-surface.baseline"
has "назвал, что причин ноль" "$OUT" "причин 0"

printf '# base\nsem 10\ntypes 4\n' > "$B"
OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "сдвиг ВНИЗ без причины — красный (сужение скрывает так же)" "$RC" "1"

printf '# base\n# sem 10 -> 11: причина\nsem 11\ntypes 4\n' > "$B"
OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "ОДНА причина на ДВА сдвига в одном файле — зелёный (минимум, не счёт)" "$RC" "0"

printf '# base\nsem 11\ntypes 5\n' > "$B"
git -C "$R" add -A >/dev/null 2>&1
OUT=$(bash "$G" "$R" "$R" 2>&1); RC=$?
check "тот же сдвиг в ИНДЕКСЕ — красный (индекс основной вход)" "$RC" "1"

echo "самотест check-novac-ratchet-moves: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
