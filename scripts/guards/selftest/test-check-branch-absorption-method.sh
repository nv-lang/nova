#!/usr/bin/env bash
# Самотест check-branch-absorption-method.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-branch-absorption-method.sh"
TMP="${TMPDIR:-/tmp}/selftest_absorb_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/scripts/tools" "$TMP/scripts/guards" "$TMP/docs/dev"
    printf '#!/usr/bin/env bash\ngit merge-base --is-ancestor "$1" "$2"\n' \
        > "$TMP/scripts/tools/branch-absorbed.sh"
    printf '#!/usr/bin/env bash\ngit branch --merged main\n' \
        > "$TMP/scripts/guards/check-accepted-branch-merged.sh"
    printf '#!/usr/bin/env bash\ngit for-each-ref --no-merged main\n' \
        > "$TMP/scripts/guards/check-no-accumulation.sh"
}
trap 'rm -rf "$TMP"' EXIT

# 1. Чистое дерево — норма.
setup
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистое дерево проходит"; else bad "ложный отказ: $out"; fi

# 2. Трёхточечный дифф без пометки — отказ. Ровно та форма, что 2026-08-13
#    дважды объявила давно влитую ветку «настоящей работой».
setup
printf '#!/usr/bin/env bash\nD=$(git diff main...$BR --stat)\n' > "$TMP/scripts/tools/probe.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "без пометки"; then ok "ловит трёхточечную форму"; else bad "не поймал (rc=$rc): $out"; fi

# 3. Пометка в ТОЙ ЖЕ строке — проходит.
setup
printf '#!/usr/bin/env bash\nD=$(git diff main...$BR --stat)  # [3DOT-OK: добавленные строки]\n' \
    > "$TMP/scripts/tools/probe.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "пометка в строке снимает запрет"; else bad "пометка не сработала: $out"; fi

# 4. Пометка ЭТАЖОМ ВЫШЕ — по-прежнему отказ. Пометка адресована тому, кто
#    читает СТРОКУ, а не тому, кто прочёл абзац над ней.
setup
printf '#!/usr/bin/env bash\n# [3DOT-OK: так надо]\nD=$(git diff main...$BR --stat)\n' \
    > "$TMP/scripts/tools/probe.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "пометка в соседней строке не считается"; else bad "приняло пометку не в той строке: $out"; fi

# 5. Двухточечная форма — законна, тревоги быть не должно.
setup
printf '#!/usr/bin/env bash\nN=$(git rev-list --count main..$BR)\n' > "$TMP/scripts/tools/probe.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "двухточечная форма не задета"; else bad "ложняк на двух точках: $out"; fi

# 6. Многоточие в прозе — не команда, тревоги быть не должно.
setup
printf '# Как быть\n\nСмотри `git diff`, потом решай...\n' > "$TMP/docs/dev/notes.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "многоточие в прозе не путается с формой"; else bad "ложняк на многоточии: $out"; fi

# 7. Двери нет — отказ: без неё каждый соберёт свою команду заново.
setup
rm -f "$TMP/scripts/tools/branch-absorbed.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "branch-absorbed"; then ok "ловит отсутствие двери"; else bad "не поймал (rc=$rc): $out"; fi

# 8. Дверь есть, но без предкового теста — тот же дифф под другим именем.
setup
printf '#!/usr/bin/env bash\ngit diff "$1" "$2"\n' > "$TMP/scripts/tools/branch-absorbed.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "ловит дверь без предкового теста"; else bad "приняло дверь-обманку: $out"; fi

# 9. Страж о слиянии, «упрощённый» до диффа, — отказ.
setup
printf '#!/usr/bin/env bash\ngit diff main $BR\n' > "$TMP/scripts/guards/check-accepted-branch-merged.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "предкового теста"; then ok "ловит подмену предкового теста диффом"; else bad "не поймал (rc=$rc): $out"; fi

# 10. `--no-merged` — тот же предковый тест с отрицанием. Первая редакция
#     образца требовала строго `--merged` и краснела на check-no-accumulation:
#     ложняк, пойманный первым прогоном на настоящем дереве.
setup
printf '#!/usr/bin/env bash\ngit for-each-ref --no-merged main\n' > "$TMP/scripts/guards/check-no-accumulation.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "--no-merged принимается наравне с --merged"; else bad "ложняк на --no-merged: $out"; fi

# 11. На настоящем дереве зелёный.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 12. Дверь на настоящем дереве отвечает на настоящий вопрос.
out=$(bash "$REAL/scripts/tools/branch-absorbed.sh" main main 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "ВЛИТА"; then ok "branch-absorbed.sh работает"; else bad "дверь не отвечает (rc=$rc): $out"; fi

# 13. Страж назван на странице правил.
if grep -q "check-branch-absorption-method.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-branch-absorption-method: 13/13 ok"; exit 0; fi
echo "селфтест check-branch-absorption-method: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
