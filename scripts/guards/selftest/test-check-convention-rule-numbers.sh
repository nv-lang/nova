#!/usr/bin/env bash
# Селфтест scripts/guards/check-convention-rule-numbers.py — номер правила
# конвенции принадлежит ОДНОМУ правилу.
#
# Страж заведён по замеру 2026-09-07: в `docs/dev/gate-guard-conventions.md`
# оказалось 17 заголовков на 16 номеров — `## Г14` дважды (строки 160 и 311),
# и так прожило две недели. Ссылка «Г14» означала РАЗНОЕ в зависимости от того,
# кто читал.
#
# ПРОБА НА НАСТОЯЩЕМ ДЕФЕКТЕ СНЯТА ПРИ ЗАВЕДЕНИИ, и это сильнее синтетики:
# файл из коммита ДО правки (`git show 03696f8ab^:docs/dev/gate-guard-conventions.md`)
# дал дословно «FAIL — один номер на два правила: docs/dev/gate-guard-conventions.md:
# номер Г14 занят 2 раз — строки 160, 311», код 1. Здесь эта проба НЕ
# воспроизводится по хэшу: селфтест не должен зависеть от истории репозитория,
# поэтому дубль синтезируется из ТЕКУЩЕГО файла — механизм тот же.
#
# Доказываем ПЯТЬ свойств:
#   1. На реальном дереве — зелено (страж пригоден к подключению).
#   2. Дубль номера — ОТКАЗ, и вердикт называет номер и ОБЕ строки.
#   3. Уникальные номера — зелено (нет ложного срабатывания на здоровом файле).
#   4. НЕПУСТОТА: дерево без документов с правилами — ОТКАЗ, а не зелень.
#      Без этого случая страж, чей шаблон перестал совпадать, годами сообщал бы
#      «дублей 0» о том, чего не читал.
#   5. Документ с ДВУМЯ заголовками конвенцией не считается (порог MIN_RULES),
#      то есть случайная пара заголовков в обычной странице стража не будит.
#
# Работаем на ВРЕМЕННЫХ деревьях; реальный docs/dev не трогаем.
#
# Запуск: scripts/guards/selftest/test-check-convention-rule-numbers.sh
# Выход: 0 — страж исправен, 1 — страж сломан.

set -uo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD="$REPO_ROOT/scripts/guards/check-convention-rule-numbers.py"
CONV="$REPO_ROOT/docs/dev/gate-guard-conventions.md"

FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-convention-rule-numbers =="

if [ ! -f "$GUARD" ]; then
    echo "  ПРОВАЛ: не найден $GUARD" >&2
    exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# --- 1: реальное дерево ----------------------------------------------------
if python "$GUARD" "$REPO_ROOT" >"$TMP/real.txt" 2>&1; then
    ok "реальное дерево зелено: $(tail -1 "$TMP/real.txt")"
else
    bad "реальное дерево краснеет: $(tail -2 "$TMP/real.txt" | tr '\n' ' ')"
fi

# --- 2: дубль номера, синтезированный из ТЕКУЩЕГО файла --------------------
# Берём первый заголовок правила и дописываем его копию в конец: номер тот же,
# название другое — ровно форма сегодняшнего дефекта.
mkdir -p "$TMP/dup/docs/dev"
cp "$CONV" "$TMP/dup/docs/dev/gate-guard-conventions.md"
# Заголовок достаём ПИТОНОМ, а не грепом: `LC_ALL=C` в шапке стоит намеренно,
# а в этой локали кириллица не попадает в `[[:print:]]` — греп нашёл бы ноль
# (поймано этим же селфтестом при заведении).
DUP_FILE="$TMP/dup/docs/dev/gate-guard-conventions.md"
if ! python -c "
import io, re, sys
p = sys.argv[1]
s = io.open(p, encoding='utf-8', newline='').read().replace('\r\n', '\n')
m = re.search(r'^#{2,4}[ \t]+[\u0410-\u042f]\d+\.[ \t].*$', s, re.M)
if not m:
    sys.exit(1)
io.open(p, 'a', encoding='utf-8', newline='\n').write('\n' + m.group(0) + ' BIS\n')
" "$DUP_FILE"; then
    bad "в реальной конвенции не нашлось ни одного заголовка правила — шаблон разошёлся с файлом"
else
    if python "$GUARD" "$TMP/dup" >"$TMP/dup.txt" 2>&1; then
        bad "дубль номера НЕ пойман (страж зелен на двух правилах с одним номером)"
    elif grep -q 'gate-guard-conventions.md' "$TMP/dup.txt" \
         && grep -qE '[0-9]+, [0-9]+' "$TMP/dup.txt"; then
        ok "дубль номера — отказ, и вердикт называет файл и обе строки"
    else
        bad "отказ есть, но вердикт не называет файл и строки: $(tail -3 "$TMP/dup.txt" | tr '\n' ' ')"
    fi
fi

# --- 3: уникальные номера — зелено ----------------------------------------
mkdir -p "$TMP/uniq/docs/dev"
{
    echo "# Синтетическая конвенция"
    echo
    echo "## Г1. Первое"
    echo
    echo "## Г2. Второе"
    echo
    echo "## Г3. Третье"
} > "$TMP/uniq/docs/dev/synthetic.md"
if python "$GUARD" "$TMP/uniq" >"$TMP/uniq.txt" 2>&1; then
    ok "уникальные номера — зелено (ложного срабатывания нет)"
else
    bad "ложное срабатывание на здоровом файле: $(tail -2 "$TMP/uniq.txt" | tr '\n' ' ')"
fi

# --- 4: непустота — дерево без правил обязано КРАСНЕТЬ ---------------------
mkdir -p "$TMP/empty/docs/dev"
echo "# Просто страница без правил" > "$TMP/empty/docs/dev/plain.md"
if python "$GUARD" "$TMP/empty" >"$TMP/empty.txt" 2>&1; then
    bad "дерево без документов с правилами прошло как зелёное — страж молчит о том, чего не читал"
elif grep -q 'НИ ОДНОГО' "$TMP/empty.txt"; then
    ok "непустота: дерево без правил — отказ с прямой причиной"
else
    bad "отказ есть, но причина не про пустоту: $(tail -2 "$TMP/empty.txt" | tr '\n' ' ')"
fi

# --- 5: порог MIN_RULES: два заголовка — не конвенция ----------------------
mkdir -p "$TMP/two/docs/dev"
{
    echo "# Обычная страница, у которой случайно есть нумерованные заголовки"
    echo
    echo "## Г1. Один"
    echo
    echo "## Г1. Тоже один, но это не конвенция"
} > "$TMP/two/docs/dev/page.md"
if python "$GUARD" "$TMP/two" >"$TMP/two.txt" 2>&1; then
    bad "страница с двумя заголовками признана конвенцией (и дубль в ней зазеленел)"
elif grep -q 'НИ ОДНОГО' "$TMP/two.txt"; then
    ok "порог соблюдён: два заголовка конвенцией не считаются"
else
    bad "порог не соблюдён — отказ пришёл по дублю, а не по пустоте: $(tail -2 "$TMP/two.txt" | tr '\n' ' ')"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-convention-rule-numbers: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-convention-rule-numbers: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
