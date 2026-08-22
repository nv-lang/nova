#!/bin/sh
# Самотест check-novac-build-clean (П16: страж без доказательства красноты
# запрещён). Доказываем обе стороны на ПОДЛОЖНОМ логе — настоящая сборка
# здесь не нужна и стоила бы восемь секунд на каждый прогон гейта.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-build-clean.sh"
T="${TMPDIR:-/tmp}/selftest-build-clean.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0

printf 'built: novac/target/novac.exe (7.7s)\n' > "$T/clean.log"
if sh "$G" "$ROOT" "$T/clean.log" >/dev/null 2>&1; then
    echo "  ok: чистый лог -> зелено"
else
    echo "  FAIL: чистый лог дал красный" >&2; fails=$((fails+1))
fi

printf 'warning: doc-comment before bare module-level ro is ignored\nbuilt: ok\n' > "$T/warn.log"
if sh "$G" "$ROOT" "$T/warn.log" >/dev/null 2>&1; then
    echo "  FAIL: лог с предупреждением дал ЗЕЛЁНЫЙ — страж ничего не судит" >&2; fails=$((fails+1))
else
    echo "  ok: предупреждение -> красный"
fi

printf 'warning: unused variable in nova-cli/target/release/build.rs\n' > "$T/alien.log"
if sh "$G" "$ROOT" "$T/alien.log" >/dev/null 2>&1; then
    echo "  ok: чужое предупреждение (не наша земля) -> зелено"
else
    echo "  FAIL: страж краснеет на предупреждении о ЧУЖОМ коде" >&2; fails=$((fails+1))
fi

# ПРОТУХШИЙ ЛОГ НЕ СУДИТСЯ. Подложное дерево: чистый лог, а исходник новее
# него; оракула в дереве нет, поэтому единственный законный исход — красный
# «судить нечем». Зелёный здесь означал бы, что страж принял историю за
# настоящее (ровно то, на чём он покраснел в первый же прогон).
mkdir -p "$T/tree/novac/src"
printf 'built: ok
' > "$T/tree/build.log"
printf 'module m
' > "$T/tree/novac/src/main.nv"
# Возраст лога задаётся ЯВНО, а не сном. `sleep 1` давал обоим файлам ОДНУ
# секунду в двух прогонах из пяти (замер 2026-08-19): find -newer не находил
# ничего, страж судил чистый лог и давал зелёный — случай, доказывающий
# красноту, сам зеленел через раз.
touch -t 202001010000 "$T/tree/build.log"
if sh "$G" "$T/tree" "$T/tree/build.log" >/dev/null 2>&1; then
    echo "  FAIL: протухший лог принят за свежий" >&2; fails=$((fails+1))
else
    echo "  ok: лог старше исходников -> не судится"
fi

if sh "$G" "$ROOT" "$T/no-such-file.log" >/dev/null 2>&1; then
    echo "  note: лога нет -> страж собрал сам и получил зелено"
else
    echo "  note: лога нет -> страж собрал сам/отказал (это законный красный)"
fi


# Владелец чужого кода определяется ПУТЁМ, а не каталогом сборки
# (адверсарная проверка 2026-08-17: шапка обещала «предупреждение о std/
# рантайме/оракуле не наше», а фильтр отбрасывал строки со словом target).
printf 'warning: unused import in std/src/runtime/x.nv
' > "$T/std.log"
if sh "$G" "$ROOT" "$T/std.log" >/dev/null 2>&1; then
    echo "  ok: предупреждение про std -> зелено"
else
    echo "  FAIL: предупреждение про std покрасило" >&2; fails=$((fails+1))
fi

printf 'warning: doc-comment before bare ro is ignored. span: Span { start: 12 }
' > "$T/nopath.log"
if sh "$G" "$ROOT" "$T/nopath.log" >/dev/null 2>&1; then
    echo "  FAIL: предупреждение БЕЗ пути прошло зелёным — а собираем мы novac" >&2; fails=$((fails+1))
else
    echo "  ok: предупреждение без пути -> красный"
fi

printf 'warning: something in novac/src/lex/lex.nv:12
' > "$T/ours.log"
if sh "$G" "$ROOT" "$T/ours.log" >/dev/null 2>&1; then
    echo "  FAIL: предупреждение про novac/src прошло зелёным" >&2; fails=$((fails+1))
else
    echo "  ok: предупреждение про novac/src -> красный"
fi

[ "$fails" -eq 0 ] && echo "test-check-novac-build-clean ok" && exit 0
echo "test-check-novac-build-clean FAIL: $fails" >&2
exit 1
