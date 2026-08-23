#!/usr/bin/env bash
# scripts/guards/check-novac-smoke-wrapper.sh — POSIX-обёртка перехвата
# clang-argv работает, ХОТЯ ЭТА МАШИНА ЕЮ НЕ ХОДИТ (класс К3, план 274 §9.1д;
# замер 2026-08-23).
#
# ЗАЧЕМ. Перехват clang-argv в `scripts/tools/novac-e1-smoke.sh` существует в
# двух формах: `.cmd` на Windows и `sh`-скрипт всюду ещё. Автор идёт ровно одной
# из них — и вторая гниёт молча. Именно это и случилось: `.cmd` была
# единственной, и на CI (Linux) инструмент печатал `cygpath: command not found`
# на каждой фикстуре, а гейт объявлял «перехват clang-argv не сработал» —
# отказ, по которому идут искать поломку в компиляторе. Двое суток.
#
# ПРОВЕРЯЕТ: строка, порождающая POSIX-обёртку, ВЫРЕЗАЕТСЯ из инструмента и
# исполняется здесь с подставным clang. Обёртка обязана записать каждый
# аргумент ОТДЕЛЬНОЙ строкой (аргумент с пробелом — одной строкой, не двумя),
# поставить `__END__` и передать в настоящий clang все аргументы без изменений.
# Так ветка, которой эта машина не ходит, всё равно судится.
#
# ЧЕГО НЕ ПРОВЕРЯЕТ: `.cmd`-ветку (её судит сам смоук на Windows каждым
# прогоном гейта); что оракул примет обёртку как `NOVA_CLANG` (это уже
# поведение инструмента, а не экранирование).
#
# ИСТОРИЯ ПРОБЫ, потому что она поучительна: первая её версия ПЕРЕПЕЧАТАЛА
# строку обёртки вручную, потеряла удвоенный `%%s` и отрапортовала о дефекте,
# которого в инструменте не было. Отсюда правило: строка ВЫРЕЗАЕТСЯ, а не
# повторяется — иначе страж судит свою копию.
#
# $1 — корень; $2 — override инструмента (шов самотеста).
set -u
export LC_ALL=C

NAME="check-novac-smoke-wrapper"
ROOT="${1:-.}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
TOOL="${2:-$ROOT/scripts/tools/novac-e1-smoke.sh}"

if [ ! -f "$TOOL" ]; then
    echo "$NAME ok: судить нечего (нет $TOOL)"
    exit 0
fi

LINE=$(grep -nE "^ +printf '#!/bin/sh" "$TOOL" | head -1 | cut -d: -f1)
if [ -z "$LINE" ]; then
    echo "$NAME: FAIL — в $TOOL нет строки, порождающей POSIX-обёртку: либо ветка" >&2
    echo "  исчезла, либо страж потерял мишень (класс №519). Перехват clang-argv" >&2
    echo "  обязан иметь POSIX-форму: на Linux .cmd-обёртка не запускается, а гейт" >&2
    echo "  сообщает «перехват не сработал» вместо «инструмент Windows-only»." >&2
    exit 1
fi

CMD=$(sed -n "${LINE}p" "$TOOL")
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
LOG="$T/cc.log"; : > "$LOG"
REAL_CLANG="$T/fake-clang.sh"
printf '#!/bin/sh\nprintf "%%s\\n" "$@" > "%s"\n' "$T/seen" > "$REAL_CLANG"
chmod +x "$REAL_CLANG"

# ВЫРЕЗАННАЯ строка исполняется как есть: она видит $LOG, $REAL_CLANG и $T.
eval "$CMD" 2>/dev/null
if [ ! -f "$T/clang-log.sh" ]; then
    echo "$NAME: FAIL — строка $TOOL:$LINE не создала обёртку \$T/clang-log.sh" >&2
    exit 1
fi
chmod +x "$T/clang-log.sh"

"$T/clang-log.sh" -c "/tmp/a b.c" -o out.o -lnova_rt >/dev/null 2>&1

LINES=$(grep -c "" "$LOG" 2>/dev/null || printf '0')
SPACED=$(grep -c '^/tmp/a b\.c$' "$LOG" 2>/dev/null || printf '0')
END=$(grep -c '^__END__$' "$LOG" 2>/dev/null || printf '0')
SEEN=$(grep -c "" "$T/seen" 2>/dev/null || printf '0')

BAD=0
[ "$LINES" = "6" ] || { echo "  строк в логе $LINES, ждали 6 (пять аргументов плюс __END__)" >&2; BAD=1; }
[ "$SPACED" = "1" ] || { echo "  аргумент с ПРОБЕЛОМ не уцелел одной строкой (нашли $SPACED)" >&2; BAD=1; }
[ "$END" = "1" ] || { echo "  маркера __END__ в логе нет: awk-разбор argv остановить нечем" >&2; BAD=1; }
[ "$SEEN" = "5" ] || { echo "  до настоящего clang дошло $SEEN аргументов из 5" >&2; BAD=1; }

if [ "$BAD" -ne 0 ]; then
    echo "$NAME: FAIL — POSIX-обёртка перехвата clang-argv сломана ($TOOL:$LINE):" >&2
    echo "  Эта машина этой ветки не ходит, потому она и судится здесь: на CI её" >&2
    echo "  поломка выглядит как «перехват clang-argv не сработал», и идут искать" >&2
    echo "  поломку в компиляторе (замер 2026-08-23: двое суток красного CI)." >&2
    exit 1
fi

echo "$NAME ok: POSIX-обёртка ($TOOL:$LINE) — 5 аргументов построчно, пробел цел, __END__ на месте"
exit 0
