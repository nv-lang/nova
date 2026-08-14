#!/bin/sh
# scripts/tools/novac-regen-shell.sh — регенерация файлонезависимой оболочки
# эмиттера novac из зонтичного probe (план 274 Э2; фикс-пойнт-приём Э1,
# доведённый до машины).
#
# Механика: оракул собирает novac/probe/shell_probe.nv → его полная эмиссия C
# ложится в кэш → отсюда вырезается ТОЛЬКО тело nova_fn_main_impl, на его
# месте штампуются два слота novac (/*__NOVAC_STRLITS__*/ и
# /*__NOVAC_BODY__*/) → результат становится novac/src/emit_c/shell.tpl.c.
# Всё остальное — рантайм-прелюдия, typeinfo, vtables эффектов, std-слой,
# строковые литералы probe (безвредные unused static) и хвост от
# nova_consts_init — остаётся оракульским байт-в-байт: novac не выдумывает
# ABI (§2/§9). DCE оракула держит в эмиссии ровно то, что probe ИСПОЛЬЗУЕТ —
# потому probe обязан расти той же волной, что подмножество эмиттера: форма
# без рантайм-хелпера в probe умирает на линковке смоука, громко.
#
# После регенерации ОБЯЗАТЕЛЬНА пересборка novac (embed) и смоуки всего
# подмножества — шаблон меняет каждый эмитируемый файл.
#
# Usage: sh scripts/tools/novac-regen-shell.sh
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PROBE="novac/probe/shell_probe.nv"
TPL="$ROOT/novac/src/emit_c/shell.tpl.c"
T="${TMPDIR:-/tmp}/novac-regen-shell.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

ORACLE="$ROOT/nova-cli/target/release/nova.exe"
if [ ! -f "$ORACLE" ]; then
    MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    [ -n "$MAINROOT" ] && ORACLE="$MAINROOT/../nova-cli/target/release/nova.exe"
fi
[ -f "$ORACLE" ] || { echo "novac-regen-shell: оракул не собран" >&2; exit 2; }

cd "$ROOT" || exit 2
# Probe в репо объявляет module probe.shell_probe (читаемый адрес), но
# внутри пакета novac любой .nv вне src/ нелегален по D78 — оракул собирает
# ВРЕМЕННУЮ копию вне пакета, где работает root-peer правило (module = имя
# файла). Подмена module — единственная правка копии.
sed 's/^module probe\.shell_probe$/module shell_probe/' "$ROOT/$PROBE" > "$T/shell_probe.nv"
"$ORACLE" build "$T/shell_probe.nv" -o "$T/probe.exe" >/dev/null 2>&1 \
    || { echo "novac-regen-shell: оракул не собрал probe (копия $T/shell_probe.nv)" >&2; exit 1; }
KEY=$(ls -t "$ROOT/target/.nova-cache/"*.c 2>/dev/null | head -1)
[ -n "$KEY" ] || { echo "novac-regen-shell: кэш C не найден" >&2; exit 1; }
# Кэш обязан быть СВЕЖИМ (эта же минута): сборка вне пакета может класть
# эмиссию в чужой target — старый ключ означал бы молчаливо чужой шаблон.
now=$(date +%s); mt=$(date -r "$KEY" +%s 2>/dev/null || echo 0)
[ $((now - mt)) -lt 120 ] || { echo "novac-regen-shell: кэш $KEY старее 2 мин — эмиссия probe легла не сюда" >&2; exit 1; }

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
grep -q 'Nova_str_method_byte_len' "$T/shell.tpl.c" || fail "std-слой probe не в шаблоне (DCE выкинул byte_len?)"
grep -q 'nova_int_checked_div' "$T/shell.tpl.c" || fail "checked_div не в шаблоне (probe обязан делить)"

cp "$T/shell.tpl.c" "$TPL"
echo "novac-regen-shell ok: $(wc -l < "$TPL" | tr -d ' ') строк из эмиссии probe ($(basename "$KEY")); пересобери novac и прогони смоуки подмножества"
exit 0
