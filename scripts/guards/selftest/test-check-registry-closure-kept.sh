#!/bin/sh
# Самотест check-registry-closure-kept.py (П16). Швы: $2 — реестр, $3 — база.
# Форма строки ok: два пробела, слово ok, пробелы — по ней считают случаи.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-registry-closure-kept.py"
T="${TMPDIR:-/tmp}/registry-closure-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }

# Реестр из двух записей: 100 закрыта, 200 открыта.
{
  printf '%s\n' '| 100 | K1 | something. **Статус:** ЗАКРЫТ. |'
  printf '%s\n' '| 200 | K1 | other. **Статус:** ОТКРЫТ. |'
} > "$T/reg_ok.md"
printf '%s\n' '# chronicle' '100' > "$T/base.baseline"

run "$T/reg_ok.md" "$T/base.baseline" \
  && ok "здоровый реестр проходит" || bad "здоровый реестр покраснел"

# ГЛАВНЫЙ случай: закрытая запись вернулась в ОТКРЫТ
{
  printf '%s\n' '| 100 | K1 | something. **Статус:** ОТКРЫТ. |'
  printf '%s\n' '| 200 | K1 | other. **Статус:** ОТКРЫТ. |'
} > "$T/reg_lost.md"
if run "$T/reg_lost.md" "$T/base.baseline"; then
    bad "потерянное закрытие прошло - главный случай не ловится"
else
    grep -q "100" "$T/err" && ok "потерянное закрытие поймано и номер назван" \
        || bad "покраснел, но номер не назван - окно будет искать руками"
fi

# Потеря НЕ прячется за чужим закрытием (потому и множество, а не счётчик)
{
  printf '%s\n' '| 100 | K1 | something. **Статус:** ОТКРЫТ. |'
  printf '%s\n' '| 200 | K1 | other. **Статус:** ЗАКРЫТ. |'
} > "$T/reg_swap.md"
run "$T/reg_swap.md" "$T/base.baseline" \
  && bad "обмен закрытий прошёл - счётчик вместо множества" \
  || ok "обмен закрытий пойман (счётчик бы промолчал)"

# Новые закрытия сверх базы законны
{
  printf '%s\n' '| 100 | K1 | something. **Статус:** ЗАКРЫТ. |'
  printf '%s\n' '| 200 | K1 | other. **Статус:** ЗАКРЫТ. |'
} > "$T/reg_more.md"
run "$T/reg_more.md" "$T/base.baseline" \
  && ok "новое закрытие сверх базы проходит" || bad "новое закрытие покраснело"

# Осознанное переоткрытие: номер убран из базы
printf '%s\n' '# chronicle: 100 reopened on purpose' > "$T/base_wo.baseline"
run "$T/reg_lost.md" "$T/base_wo.baseline" \
  && ok "осознанное переоткрытие через базу проходит" \
  || bad "переоткрытие через базу покраснело - у правила нет выхода"

# Потерянная мишень: базы нет
run "$T/reg_ok.md" "$T/nosuch.baseline" \
  && bad "отсутствие базы прошло - страж без базы ничего не держит" \
  || ok "отсутствие базы красное"

# Нет реестра — судить нечего
run "$T/nosuch.md" "$T/base.baseline" \
  && ok "отсутствие реестра - нечего судить" || bad "отсутствие реестра покраснело"

[ "$fails" -eq 0 ] && echo "test-check-registry-closure-kept: ok" || exit 1
