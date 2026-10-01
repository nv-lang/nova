#!/usr/bin/env bash
# Самотест check-task-selection-order.py.
#
# Клетка «как в дереве — ok» и по мутации на каждое требование R1-R4.
# Мутации делаются на КОПИЯХ двух команд во временном каталоге, не в дереве.
# Каждая мутация обязана давать FAIL и НАЗЫВАТЬ нарушенное требование.

set -u
export LC_ALL=C

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
G="$HERE/../check-task-selection-order.py"
REAL="$(cd "$HERE/../../.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/selftest_tso.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

fresh() {
    rm -rf "$TMP/t"
    mkdir -p "$TMP/t/.claude/commands"
    cp "$REAL/.claude/commands/load-background.md" "$TMP/t/.claude/commands/"
    cp "$REAL/.claude/commands/cloud-task.md" "$TMP/t/.claude/commands/"
}

# mutate <файл> <python-выражение над переменной t (текст), возвращающее новый текст>
mutate() {
    F="$TMP/t/.claude/commands/$1" EXPR="$2" python - <<'PY'
import os, re
p = os.environ["F"]
t = open(p, encoding="utf-8", newline="").read()
ex = os.environ["EXPR"].strip()
try:
    t2 = eval(ex)
except SyntaxError:
    g = {"t": t, "re": re}
    exec(ex, g)
    t2 = g["out"]
assert t2 != t, "мутация ничего не изменила"
open(p, "w", encoding="utf-8", newline="").write(t2)
PY
}

run() { python "$G" "$TMP/t" > "$TMP/.out" 2> "$TMP/.err"; }

expect_fail() {   # $1 — метка, $2 — код требования в выводе
    run
    local rc=$?
    if [ $rc -eq 1 ] && grep -q "$2" "$TMP/.out"; then ok "$1 -> FAIL ($2)"
    else bad "$1: rc=$rc, ждали FAIL с $2: $(cat "$TMP/.out" "$TMP/.err")"; fi
}

# 0. Как в дереве — ok.
fresh; run
if [ $? -eq 0 ] && grep -q "^check-task-selection-order ok:" "$TMP/.out"; then ok "дерево как есть — ok"
else bad "0: ложняк на дереве: $(cat "$TMP/.out" "$TMP/.err")"; fi

# R1: переставить пункты 1 и 4.
fresh
mutate load-background.md '
L = t.split("\n")
i = next(k for k, l in enumerate(L) if l.startswith("1. ") and "novac-gate" in l)
j = next(k for k, l in enumerate(L) if l.startswith("4. ") and "дефект оракула" in l)
a, b = L[i][3:], L[j][3:]
L[i], L[j] = "1. " + b, "4. " + a
out = "\n".join(L)'
expect_fail "R1: пункты 1 и 4 переставлены" "R1"

# R2: вернуть первым пунктом выбор по БЛОКИРУЕТ ТЕГ.
fresh
mutate load-background.md '
re.sub(r"(?m)^1\. .*$", "1. Открытые строки реестра 221.1 с `**БЛОКИРУЕТ ТЕГ:** 0.2 — ДА`;", t, count=1)'
expect_fail "R2: первым пунктом снова БЛОКИРУЕТ ТЕГ" "R2"

# R2: убрать абзац «НЕ судит».
fresh
mutate load-background.md 't.replace("по выбору задачи НЕ судит", "по выбору задачи учитывается")'
expect_fail "R2: абзац «НЕ судит» снят" "R2"

# R1: убрать слова «ТОЛЬКО с доказанной связью».
fresh
mutate load-background.md 't.replace("ТОЛЬКО с доказанной связью", "если есть желание")'
expect_fail "R1: слова «ТОЛЬКО с доказанной связью» убраны" "R1"

# R3: удалить раздел САМОПРОВЕРКА.
fresh
mutate load-background.md '
re.sub(r"\*\*САМОПРОВЕРКА ПЕРЕД ВЫДАЧЕЙ.*?(?=\*\*Не занято ли:\*\*)", "", t, flags=re.S)'
expect_fail "R3: раздел САМОПРОВЕРКА удалён" "R3"

# R3: четыре вопроса вместо пяти.
fresh
mutate load-background.md 're.sub(r"(?m)^5\. \*\*Промпт несёт.*\n", "", t)'
expect_fail "R3: пятый вопрос снят" "R3"

# R3: удалить вопрос 6 целиком (строка и его подпункты).
fresh
mutate load-background.md 're.sub(r"(?m)^6\. .*\n(?:   .*\n)*", "", t)'
expect_fail "R3: вопрос 6 удалён" "R3"

# R3: удалить часть «насколько» из вопроса 6.
fresh
mutate load-background.md 're.sub(r"(?m)^   - \*\*насколько\*\*.*\n", "", t)'
expect_fail "R3: часть «насколько» удалена" "R3"

# R3: образец строки без «быстрее — ».
fresh
mutate load-background.md 't.replace("быстрее — ", "")'
expect_fail "R3: в образце нет «быстрее — »" "R3"

# R4: удалить «ожидаемый выигрыш» из пункта 1а cloud-task.
fresh
mutate cloud-task.md 't.replace("ожидаемый выигрыш", "заметки")'
expect_fail "R4: «ожидаемый выигрыш» удалён из 1а" "R4"

# R4: ссылка на самопроверку без «почему быстрее и насколько».
fresh
mutate cloud-task.md 't.replace("почему быстрее и насколько", "почему")'
expect_fail "R4: «почему быстрее и насколько» удалено" "R4"

# R4: удалить пункт 1а из cloud-task.
fresh
mutate cloud-task.md 're.sub(r"(?m)^1а\. \*\*Связь с релизом Карины:\*\*.*\n", "", t)'
expect_fail "R4: пункт 1а удалён" "R4"

# R4: удалить ссылку на самопроверку из cloud-task.
fresh
mutate cloud-task.md 't.replace("САМОПРОВЕРКА", "проверка")'
expect_fail "R4: ссылка на САМОПРОВЕРКУ удалена" "R4"

# R4: порядок в cloud-task переставлен (274.11 раньше 274.10).
fresh
mutate cloud-task.md 't.replace("очередь 274.10 → 274.11", "274.11 → очередь 274.10")'
expect_fail "R4: порядок в cloud-task переставлен" "R4"

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-task-selection-order: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-task-selection-order: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
