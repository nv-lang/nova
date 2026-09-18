#!/usr/bin/env bash
# Самотест check-gate-daily-budget.py.
#
# Оси: (ярус тяжёлый / дешёвый) × (отметка свежая / старая / отсутствует),
# плюс осознанный обход и край — общий `.git` вместо дерева.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-gate-daily-budget.py"
TMP="${TMPDIR:-/tmp}/selftest_gdb_$$"
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() { rm -rf "$TMP"; mkdir -p "$TMP"; S="$TMP/gate.stamp"; }
now()   { date +%s; }

# 1. Тяжёлый ярус, отметки нет — норма, и отметка ПОЯВЛЯЕТСЯ.
setup
NOVA_GATE_STAMP="$S" python "$G" push "$TMP" > "$TMP/.out" 2>&1
if [ $? -eq 0 ] && [ -f "$S" ]; then
    ok "первый тяжёлый прогон проходит и ставит отметку"
else
    bad "1: первый прогон не прошёл либо не поставил отметку: $(cat "$TMP/.out")"
fi

# 2. ВТОРОЙ тяжёлый прогон сразу — ОТКАЗ. Это и есть предмет.
NOVA_GATE_STAMP="$S" python "$G" push "$TMP" > "$TMP/.out" 2>&1
if [ $? -eq 1 ] && grep -q "УЖЕ ШЁЛ" "$TMP/.out"; then
    ok "второй тяжёлый прогон в те же сутки отвергается"
else
    bad "2: второй прогон не отвергнут: $(cat "$TMP/.out")"
fi

# 3. Дешёвый ярус НЕ судится даже при свежей отметке: иначе окно осталось бы
#    вовсе без быстрой проверки и пошло бы вслепую.
NOVA_GATE_STAMP="$S" python "$G" loop "$TMP" > "$TMP/.out" 2>&1
if [ $? -eq 0 ]; then ok "ярус loop не под пределом"; else bad "3: loop отвергнут: $(cat "$TMP/.out")"; fi

# 4. Осознанный обход с ПРИЧИНОЙ проходит, и причина ПЕЧАТАЕТСЯ.
NOVA_GATE_STAMP="$S" NOVA_GATE_DAILY_OVERRIDE="чужое окно ждёт" \
    python "$G" push "$TMP" > "$TMP/.out" 2>&1
if [ $? -eq 0 ] && grep -q "чужое окно ждёт" "$TMP/.out"; then
    ok "обход с причиной проходит и причина печатается"
else
    bad "4: обход не сработал либо причина не напечатана: $(cat "$TMP/.out")"
fi

# 5. ПУСТОЙ обход не принимается: «сниму просто так» — это не причина.
NOVA_GATE_STAMP="$S" NOVA_GATE_DAILY_OVERRIDE="   " \
    python "$G" push "$TMP" > "$TMP/.out" 2>&1
if [ $? -eq 1 ]; then ok "пустая причина обхода не принимается"; else bad "5: пустой override прошёл"; fi

# 6. Через сутки — снова норма.
setup
printf '%s push\n' "$(( $(now) - 90000 ))" > "$S"
NOVA_GATE_STAMP="$S" python "$G" push "$TMP" > "$TMP/.out" 2>&1
if [ $? -eq 0 ]; then ok "спустя сутки предел снят сам"; else bad "6: через сутки всё ещё отказ: $(cat "$TMP/.out")"; fi

# 7. КРАЙ, РАДИ КОТОРОГО ОТМЕТКА ЛЕЖИТ В ОБЩЕМ `.git`: без подмены пути страж
#    обязан выбрать ОДИН файл для всех worktree. Проверяем, что путь по
#    умолчанию берётся из `git rev-parse --git-common-dir`, а не из дерева.
setup
git -C "$TMP" init -q 2>/dev/null
out=$(cd "$TMP" && python "$G" push "$TMP" 2>&1)
if [ -f "$TMP/.git/nova-gate-daily.stamp" ]; then
    ok "отметка легла в общий .git, а не в дерево"
else
    bad "7: отметка не в общем .git: $out"
fi

rm -rf "$TMP"
if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-gate-daily-budget: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-gate-daily-budget: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
