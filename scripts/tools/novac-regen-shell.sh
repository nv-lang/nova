#!/bin/sh
# scripts/tools/novac-regen-shell.sh — регенерация файлонезависимой оболочки
# эмиттера novac из зонтичного probe (план 274 Э2; фикс-пойнт-приём Э1,
# доведённый до машины). С 274.3/F6 инструмент ещё и СУДЬЯ свежести: режим
# --check ничего не пишет, а сверяет репозиторный шаблон с эмиссией оракула
# (страж scripts/guards/check-novac-shell-freshness.sh зовёт именно его).
#
# Механика: оракул собирает novac/probe/shell_probe.nv → его полная эмиссия C
# ложится в артефакты сборки → отсюда вырезается ТОЛЬКО тело
# nova_fn_main_impl, на его месте штампуются два слота novac
# (/*__NOVAC_STRLITS__*/ и /*__NOVAC_BODY__*/) → результат становится
# novac/src/emit_c/shell.tpl.c. Всё остальное — рантайм-прелюдия, typeinfo,
# vtables эффектов, std-слой, строковые литералы probe (безвредные unused
# static) и хвост от nova_consts_init — остаётся оракульским байт-в-байт:
# novac не выдумывает ABI (§2/§9). DCE оракула держит в эмиссии ровно то, что
# probe ИСПОЛЬЗУЕТ — потому probe обязан расти той же волной, что подмножество
# эмиттера: форма без рантайм-хелпера в probe умирает на линковке смоука,
# громко.
#
# ЭМИССИЯ БЕРЁТСЯ ПО СОДЕРЖИМОМУ, НЕ ПО ВРЕМЕНИ (274.3/F6). До фикса ключом
# был самый свежий *.c в ОБЩЕМ target/.nova-cache (ls -t + окно 2 минуты) —
# гонка: параллельное окно клало туда свою эмиссию, и шаблон молча собирался
# из чужой сборки. Теперь probe строится с --keep-artifacts в ЧАСТНЫЙ каталог
# артефактов (TEMP/TMPDIR этого запуска), а нужный .c ищется по уникальному
# литералу probe; ноль кандидатов или больше одного — честный отказ, а не
# «возьму первый». Общий кэш при этом не читается и не пишется (его отключает
# --keep-artifacts), эмиссия байт-в-байт та же (сверено 2026-08-15: артефакт
# == запись кэша для того же входа).
#
# После регенерации ОБЯЗАТЕЛЬНА пересборка novac (embed) и смоуки всего
# подмножества — шаблон меняет каждый эмитируемый файл.
#
# Usage:
#   sh scripts/tools/novac-regen-shell.sh              # регенерировать и ЗАПИСАТЬ шаблон
#   sh scripts/tools/novac-regen-shell.sh --check      # НЕ писать: сверить (cmp) с шаблоном
#   sh scripts/tools/novac-regen-shell.sh --check ФАЙЛ # то же, но сверить с ФАЙЛОМ (шов самотеста)
# Коды выхода: 0 — записано / совпало; 1 — расхождение либо отказ механики;
#              2 — неверный вызов; 3 — оракул не собран («судить нечего»).
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PROBE="novac/probe/shell_probe.nv"
TPL="$ROOT/novac/src/emit_c/shell.tpl.c"
# Литерал-дискриминатор: строка, которая есть в shell_probe.nv и по которой
# опознаётся «этот .c — эмиссия ИМЕННО нашего probe». Пропал из probe —
# правится ЗДЕСЬ тем же коммитом (проверяется ниже, вслепую не опознаём).
PROBE_LIT='probe string'

MODE=write
case "${1:-}" in
    "")      ;;
    # Путь-аргумент абсолютизируется СРАЗУ: ниже скрипт делает cd в корень
    # репы, и относительный путь после этого указывал бы не туда.
    --check) MODE=check
             if [ -n "${2:-}" ]; then
                 case "$2" in
                     /* | [A-Za-z]:[/\\]*) TPL="$2" ;;
                     *)                    TPL="$(pwd)/$2" ;;
                 esac
             fi ;;
    *)       echo "novac-regen-shell: неверный вызов '$1'; usage: novac-regen-shell.sh [--check [ФАЙЛ]]" >&2; exit 2 ;;
esac

T="${TMPDIR:-/tmp}/novac-regen-shell.$$"
mkdir -p "$T" || exit 2
trap 'rm -rf "$T"' 0

ORACLE="$ROOT/nova-cli/target/release/nova.exe"
if [ ! -f "$ORACLE" ]; then
    MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    [ -n "$MAINROOT" ] && ORACLE="$MAINROOT/../nova-cli/target/release/nova.exe"
fi
[ -f "$ORACLE" ] || { echo "novac-regen-shell: оракул не собран (nova-cli/target/release/nova.exe)" >&2; exit 3; }

cd "$ROOT" || exit 2
[ -f "$ROOT/$PROBE" ] || { echo "novac-regen-shell: нет probe $ROOT/$PROBE" >&2; exit 1; }
grep -qF "\"$PROBE_LIT\"" "$ROOT/$PROBE" \
    || { echo "novac-regen-shell: литерал-дискриминатор \"$PROBE_LIT\" исчез из $PROBE — обнови PROBE_LIT тем же коммитом" >&2; exit 1; }

# Probe в репо объявляет module probe.shell_probe (читаемый адрес), но
# внутри пакета novac любой .nv вне src/ нелегален по D78 — оракул собирает
# ВРЕМЕННУЮ копию вне пакета, где работает root-peer правило (module = имя
# файла). Подмена module — единственная правка копии.
sed 's/^module probe\.shell_probe$/module shell_probe/' "$ROOT/$PROBE" > "$T/shell_probe.nv"

# Артефакты сборки — в каталог ЭТОГО запуска: оракул кладёт .c в
# $TEMP|$TMPDIR/nova_tests-<pid>/build-<hash>/, и никто, кроме нас, туда не
# пишет — это и убирает гонку параллельных окон.
ART="$T/art"
mkdir -p "$ART"
ART_NATIVE="$ART"
command -v cygpath >/dev/null 2>&1 && ART_NATIVE=$(cygpath -w "$ART")
TEMP="$ART_NATIVE" TMPDIR="$ART_NATIVE" "$ORACLE" build "$T/shell_probe.nv" \
    -o "$T/probe.exe" --keep-artifacts > "$T/build.log" 2>&1 \
    || { echo "novac-regen-shell: оракул не собрал probe (копия $T/shell_probe.nv):" >&2; tail -20 "$T/build.log" >&2; exit 1; }

# Ключ — по СОДЕРЖИМОМУ: единственный .c артефактов с литералом probe.
find "$ART" -type f -name '*.c' > "$T/cands.all" 2>/dev/null
: > "$T/cands"
while IFS= read -r c; do
    [ -n "$c" ] || continue
    grep -qF "\"$PROBE_LIT\"" "$c" && printf '%s\n' "$c" >> "$T/cands"
done < "$T/cands.all"
n=$(wc -l < "$T/cands" | tr -d ' ')
n_all=$(wc -l < "$T/cands.all" | tr -d ' ')
if [ "$n" -eq 0 ]; then
    echo "novac-regen-shell: эмиссия probe не найдена — ни один .c в артефактах не содержит \"$PROBE_LIT\" (файлов .c: $n_all)" >&2
    exit 1
fi
if [ "$n" -gt 1 ]; then
    echo "novac-regen-shell: кандидатов на эмиссию probe $n (из $n_all) — ключ по содержимому неоднозначен:" >&2
    sed 's/^/  /' "$T/cands" >&2
    exit 1
fi
KEY=$(cat "$T/cands")

# Вырезать определение nova_fn_main_impl (до первой '}' в нулевой колонке),
# на его месте — два слота novac.
awk '
    /^static nova_unit nova_fn_main_impl\(void\) \{/ {
        inmain = 1
        print "/*__NOVAC_STRLITS__*/"
        print "/*__NOVAC_BODY__*/"
        next
    }
    inmain && /^\}/ { inmain = 0; next }
    inmain { next }
    { print }
' "$KEY" > "$T/shell.tpl.c"

# Самопроверка (урок «оболочка руками»: молчаливое искажение хуже отказа).
fail() { echo "novac-regen-shell: FAIL — $1" >&2; exit 1; }
grep -q '__NOVAC_STRLITS__' "$T/shell.tpl.c" || fail "слот STRLITS не встал"
grep -q '__NOVAC_BODY__' "$T/shell.tpl.c" || fail "слот BODY не встал"
n_def=$(grep -c '^static nova_unit nova_fn_main_impl(void) {' "$T/shell.tpl.c")
[ "$n_def" -eq 0 ] || fail "определение main_impl не вырезано ($n_def)"
grep -q 'nova_fn_main_impl();' "$T/shell.tpl.c" || fail "вызов main_impl пропал из хвоста"
# Уникальный литерал probe: два общих имени ниже удовлетворит ЛЮБАЯ эмиссия со
# строковыми методами — этот не удовлетворит никакая чужая (274.3/F6).
grep -qF "\"$PROBE_LIT\"" "$T/shell.tpl.c" || fail "литерала \"$PROBE_LIT\" нет в шаблоне — эмиссия не от нашего probe"
grep -q 'Nova_str_method_byte_len' "$T/shell.tpl.c" || fail "std-слой probe не в шаблоне (DCE выкинул byte_len?)"
grep -q 'nova_int_checked_div' "$T/shell.tpl.c" || fail "checked_div не в шаблоне (probe обязан делить)"

LINES=$(wc -l < "$T/shell.tpl.c" | tr -d ' ')
if [ "$MODE" = check ]; then
    if [ ! -f "$TPL" ]; then
        echo "novac-regen-shell --check: шаблона $TPL нет — прогони novac-regen-shell.sh и закоммить шаблон тем же слиянием" >&2
        exit 1
    fi
    if cmp -s "$T/shell.tpl.c" "$TPL"; then
        echo "novac-regen-shell --check ok: шаблон совпал с эмиссией оракула по probe ($LINES строк)"
        exit 0
    fi
    echo "novac-regen-shell --check: РАСХОЖДЕНИЕ — $TPL не равен эмиссии оракула по $PROBE" >&2
    # Частый ложный след на Windows: core.autocrlf=true отдаёт рабочей копии
    # CRLF, а эмиссия оракула — LF. Диффом это выглядит как «разошёлся весь
    # файл»; чинится .gitattributes, а не перегенерацией — назовём причину.
    tr -d '\r' < "$TPL" > "$T/tpl.nocr"
    tr -d '\r' < "$T/shell.tpl.c" > "$T/new.nocr"
    if cmp -s "$T/tpl.nocr" "$T/new.nocr"; then
        echo "  ПРИЧИНА — только переводы строк: содержимое совпадает после снятия CR." >&2
        echo "  Чинить НЕ перегенерацией: шаблон обязан лежать с LF (core.autocrlf=true" >&2
        echo "  портит рабочую копию) — строка '$(basename "$TPL") -text' в .gitattributes." >&2
        exit 1
    fi
    echo "  (в репо $(wc -l < "$TPL" | tr -d ' ') строк, свежая эмиссия $LINES строк; первые различия:)" >&2
    diff "$TPL" "$T/shell.tpl.c" 2>/dev/null | head -20 | sed 's/^/  /' >&2
    echo "  Чинить: прогони novac-regen-shell.sh и закоммить шаблон ТЕМ ЖЕ слиянием" >&2
    echo "  (пересобрав novac и прогнав смоуки подмножества — шаблон меняет каждый эмитируемый файл)." >&2
    exit 1
fi

cp "$T/shell.tpl.c" "$TPL"
echo "novac-regen-shell ok: $LINES строк из эмиссии probe ($(basename "$KEY")); пересобери novac и прогони смоуки подмножества"
exit 0
