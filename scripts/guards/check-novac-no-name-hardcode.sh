#!/bin/sh
# scripts/guards/check-novac-no-name-hardcode.sh — никакого хардкода имён
# Nova/std в компиляторе (конвенция П5; заведён 2026-08-15 по слову владельца:
# «subset_method_ret — хардкод, страж почему не ловит?» — потому что стража
# не было; статус П5 был 🕐 Э2, Э2 идёт).
#
# ПРАВИЛО: строковый литерал с именем языка/std (ключевые слова-сущности,
# имена std-типов и методов, entry/print, оракульские C-имена моно-инстансов
# и тэгов) законен ТОЛЬКО в novac/src/sem/builtins.nv — едином реестре
# «легитимного остатка П5». Везде ещё в novac/src — красный. Остаток
# снимается Э2-б (чтение деклараций std) — файл builtins.nv худеет, страж
# остаётся.
#
# ПРОВЕРЯЕТ: грепом по novac/src/**/*.nv (кроме builtins.nv и *_test.nv)
# литералов из списка NAMES ниже. Список — данные стража, растёт вместе с
# builtins.nv (новое имя в builtins без строки здесь — ревью-красный).
# НЕ ПРОВЕРЯЕТ: имена в комментариях (греп по литералам в кавычках);
# ключевые слова ГРАММАТИКИ в лексере (`"fn"`, `"module"` — это лексер по
# определению, у rustc тоже таблица kw::*; П5 — про сущности std/языка,
# которые обязаны браться из деклараций, а не про синтаксис);
# коды диагностик E_*/W_* (это имена novac, не Nova).
#
# $1 — корень репозитория; $2 — override сканируемой директории (самотест).
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-no-name-hardcode

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

# Names of the language and std the compiler is tempted to spell out.
NAMES='main|println|print|Some|None|Ok|Err|Option|Result|Vec|HashMap|byte_len|to_ascii_upper|starts_with|contains|len|get|push|to_str|int|str|bool|f64|u8|\[\]int|Nova_str_method_|Vec____nova_int|Nova_Vec____|NovaOpt_|NOVA_TAG_|nova_int|nova_str|nova_bool|nova_f64'

BAD=$(find "$SRC" -type f -name '*.nv' ! -name 'builtins.nv' ! -name '*_test.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    # a literal is "..." on a non-comment line; strip //-comments first
    sed 's|//.*$||' "$f" | grep -n -E "\"($NAMES)\"" | sed "s|^|  $rel:|"
done)

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — имена языка/std как строковые литералы вне sem/builtins.nv (П5):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Имя — в novac/src/sem/builtins.nv (единый реестр остатка П5), здесь — константа/дверь." >&2
    exit 1
fi
N=$(find "$SRC" -type f -name '*.nv' ! -name 'builtins.nv' ! -name '*_test.nv' | wc -l | tr -d '[:space:]')
echo "$NAME ok: файлов .nv: $N, хардкод-имён вне builtins.nv: 0"
exit 0
