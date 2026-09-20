#!/usr/bin/env bash
# Селфтест scripts/guards/check-carina-no-idle-wait.py — обе стороны, и обе
# половины правила порознь.
#
# ЗАЧЕМ ИМЕННО ТАК. Страж судит ТЕКСТ команды, и такой страж особенно легко
# сделать бесполезным: если он краснеет только на пустом файле, то зелёный
# ничего не значит. Поэтому здесь несколько случаев, и большинство красные: нет
# правила вовсе; есть заголовок без «когда»; есть заголовок без «чем занять».
# Последние два — это лозунг, и он обязан краснеть наравне с отсутствием.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-carina-no-idle-wait.py"
FAILED=0
OKN=0
REDN=0
ok()  { OKN=$((OKN + 1)); echo "  ok: $1"; }
# Красный случай считается ОТДЕЛЬНО: число «из них красных» —
# такое же утверждение о теле, как и общее, и рукой ему не место.
red() { REDN=$((REDN + 1)); ok "$1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
CMD="$TMP/.claude/commands"
mk() { rm -rf "$TMP/.claude"; mkdir -p "$CMD"; }

# 1. Правило целиком: заголовок, повод ждать и чем занять паузу — зелено.
mk
{
  printf '## ОЖИДАНИЕ НЕ ЕСТЬ РАБОТА: занята машина — берётся работа без машины\n'
  printf 'Срабатывает, когда идёт чужой гейт либо ждёшь номер.\n'
  printf 'Вместо ожидания: работа, требующая только чтения; правка без прогона.\n'
} > "$CMD/carina.md"
out=$(python "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "полное правило — зелено"; else bad "ложный отказ: $out"; fi

# 2. Команда есть, правила нет — красно.
mk
printf '## Что сделать, по порядку\nРаботай в своём дереве.\n' > "$CMD/carina.md"
out=$(python "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "ozhidanie"; then
    red "правила нет — красно, и сказано какого"
else
    bad "отсутствие правила обязано краснеть (код $rc): $out"
fi

# 3. ЛОЗУНГ: заголовок есть, а повода ждать не названо — красно.
mk
{
  printf '## ОЖИДАНИЕ НЕ ЕСТЬ РАБОТА\n'
  printf 'Вместо ожидания: работа, требующая только чтения.\n'
} > "$CMD/carina.md"
out=$(python "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "vtoroy poloviny"; then
    red "заголовок без «когда» — красно"
else
    bad "лозунг без повода обязан краснеть (код $rc): $out"
fi

# 4. ЛОЗУНГ наоборот: повод назван, а чем занять паузу — нет; красно.
mk
{
  printf '## ОЖИДАНИЕ НЕ ЕСТЬ РАБОТА\n'
  printf 'Срабатывает, когда идёт чужой гейт.\n'
} > "$CMD/carina.md"
out=$(python "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "vtoroy poloviny"; then
    red "заголовок без «чем занять» — красно"
else
    bad "лозунг без замены обязан краснеть (код $rc): $out"
fi

# 5. Дерево без .claude/commands вовсе — судить нечего, зелено.
rm -rf "$TMP/.claude"
out=$(python "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "sudit nechego"; then
    ok "дерево без команд — судить нечего"
else
    bad "срез без .claude/commands не должен краснеть (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "test-check-carina-no-idle-wait ok: $OKN случаев, из них $REDN красных"
else
    echo "test-check-carina-no-idle-wait ПРОВАЛЕН" >&2
fi
exit "$FAILED"
