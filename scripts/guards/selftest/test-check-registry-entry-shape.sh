#!/usr/bin/env bash
# Селфтест scripts/guards/check-registry-entry-shape.sh.
#
# Проверяем ОБА направления. Второе важнее первого: страж, краснеющий на
# правильно оформленной записи, будет отключён в первый же день, и правило умрёт
# вместе с ним — как уже случилось бы с check-no-accumulation, если бы он считал
# накоплением всякую рабочую ветку окна.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-registry-entry-shape.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/docs/plans" "$TMP/scripts/guards"
REG="$TMP/docs/plans/221.1-bug-sweep.md"
BASE="$TMP/base"

# Полностью оформленная запись — образец.
GOOD='| 900 | 🔴 К1 | **Заголовок дефекта.** Описание. **КЛАСС: одна операция размазана по трём местам.** Фикс носителя приёмкой НЕ считается. |'

mk() { printf '%s\n' "$@" > "$REG"; }

# 1. Полная запись — зелено при базе 0.
mk "$GOOD"; echo 'incomplete_entries=0' > "$BASE"
out=$(NOVA_REGSHAPE_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "полная запись проходит"; else bad "ложный отказ на полной записи: $out"; fi

# 2. Нет приоритета.
mk '| 901 | **Заголовок.** **КЛАСС: что-то.** Фикс носителя приёмкой НЕ считается. |'
out=$(NOVA_REGSHAPE_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'приоритет'; then ok "ловит запись без приоритета"; else bad "не поймал отсутствие приоритета (код $rc): $out"; fi

# 3. Нет класса — самый опасный случай: окно починит носителя.
mk '| 902 | 🔴 К1 | **Заголовок.** Описание. Фикс носителя приёмкой НЕ считается. |'
out=$(NOVA_REGSHAPE_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'класс'; then ok "ловит запись без КЛАССА"; else bad "не поймал отсутствие класса (код $rc): $out"; fi

# 4. Нет оговорки о носителе.
mk '| 903 | 🟡 К2 | **Заголовок.** **КЛАСС: что-то.** Описание. |'
out=$(NOVA_REGSHAPE_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'оговорка-о-носителе'; then ok "ловит запись без оговорки о носителе"; else bad "не поймал отсутствие оговорки (код $rc): $out"; fi

# 5. Храповик держит существующий долг: две неполные записи при базе 2 — зелено.
mk '| 904 | 🔴 К1 | **A.** Описание. |' '| 905 | 🔴 К1 | **B.** Описание. |'
echo 'incomplete_entries=2' > "$BASE"
out=$(NOVA_REGSHAPE_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "храповик держит долг в пределах базы"; else bad "ложный отказ на долге в базе (код $rc): $out"; fi

# 6. Рост сверх базы — красно.
mk '| 904 | 🔴 К1 | **A.** |' '| 905 | 🔴 К1 | **B.** |' '| 906 | 🔴 К1 | **C.** |'
out=$(NOVA_REGSHAPE_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'ВЫРОСЛО'; then ok "ловит рост сверх базы"; else bad "не поймал рост (код $rc): $out"; fi

# 7. Снижение долга — зелено, но сообщает, что базу надо опустить.
mk "$GOOD"
out=$(NOVA_REGSHAPE_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'СНИЗИЛСЯ'; then ok "сообщает о снижении долга"; else bad "не сообщил о снижении (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-registry-entry-shape: 7/7 ok"; exit 0; fi
echo "селфтест check-registry-entry-shape: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
