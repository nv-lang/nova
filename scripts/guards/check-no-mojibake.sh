#!/usr/bin/env bash
# scripts/guards/check-no-mojibake.sh
# Текст, испорченный «UTF-8 прочитан как CP1251», не попадает в дерево.
#
# ДОМ И ОСНОВАНИЕ: реестр 221.1 №637 (обратные апострофы и кириллица через
# оболочку), №643; ремонтный инструмент `scripts/tools/demojibake.py`
# (GitHub issue #1), план 231 трек Д.
#
# ЗАЧЕМ. Класс известен давно — под него написан ремонтный инструмент, — но
# СТРАЖА не было, и 2026-08-13 порча прошла в `main` трижды за смену: запись
# реестра №635 лишилась всех трёх обязательных полей (их «съело» на пути через
# heredoc), а правило «Отправляй то, что проверял» на странице правил уехало
# на три зеркала нечитаемым. Ни один существующий механизм этого не увидел:
# `check-no-control-chars` смотрит управляющие БАЙТЫ, а мохибейк состоит из
# вполне законных символов; `doc-conventions` проверяет язык и парность, а не
# читаемость.
#
# ПРИЗНАК — и почему именно такой. Первая редакция искала «кириллица, за
# которой идёт символ Latin-1» и дала 2468 ложных срабатываний: русские
# кавычки «» — это U+00AB/U+00BB, ровно Latin-1 Supplement. Признак обязан
# быть таким, какого в правильном русском тексте НЕ БЫВАЕТ ВОВСЕ: буквы
# сербско-македонского ряда (Ђ Ѓ ђ ѓ ћ ќ љ њ ї ѕ), в которые превращаются
# вторые байты UTF-8 при чтении как CP1251. Их наличие — не эвристика, а
# подпись поломки.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно):
#   * порчу, не дающую этих букв (короткие слова, где второй байт лёг иначе);
#   * английский текст — он такой порчи не переживает заметно;
#   * `scripts/tools/demojibake.py` исключён НАМЕРЕННО: он ХРАНИТ образцы
#     порчи как данные, и краснеть на ремонтном инструменте значит учить
#     обходить стража.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-no-mojibake.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-no-mojibake.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
BASELINE="${NOVA_MOJIBAKE_BASELINE:-$ROOT/scripts/guards/mojibake.baseline}"
CORE="$(dirname "${BASH_SOURCE[0]}")/mojibake-scan.py"

[ -f "$CORE" ] || { echo "check-no-mojibake: нет ядра $CORE" >&2; exit 1; }

OUT=$(python "$CORE" "$ROOT" 2>/dev/null)
TOTAL=$(printf '%s\n' "$OUT" | grep -c . || true)
TOTAL=${TOTAL:-0}

BASE=0
[ -f "$BASELINE" ] && BASE=$(sed -n 's/^mojibake_lines=\([0-9][0-9]*\).*/\1/p' "$BASELINE" | head -1)
BASE=${BASE:-0}

echo "check-no-mojibake: строк с подписью порчи $TOTAL (база $BASE)"

if [ "$TOTAL" -gt "$BASE" ]; then
    echo "check-no-mojibake: ВЫРОСЛО — $TOTAL > базы $BASE" >&2
    printf '%s\n' "$OUT" | head -20 | sed 's/^/    /' >&2
    echo "" >&2
    echo "    Это «UTF-8 прочитан как CP1251»: текст записан через оболочку." >&2
    echo "    Пиши файл инструментом записи и запускай его; сообщения коммитов" >&2
    echo "    передавай через -F <файл>. Починить уже испорченное:" >&2
    echo "      python scripts/tools/demojibake.py <файл>" >&2
    echo "    Реестр 221.1 №637." >&2
    echo "check-no-mojibake: FAIL" >&2
    exit 1
fi

if [ "$TOTAL" -lt "$BASE" ]; then
    echo "check-no-mojibake: долг СНИЗИЛСЯ ($TOTAL < базы $BASE) — опусти базу в $BASELINE"
fi
echo "check-no-mojibake ok: новой порчи текста нет ($TOTAL <= $BASE)"
exit 0
