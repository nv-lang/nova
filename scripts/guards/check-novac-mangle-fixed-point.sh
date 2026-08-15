#!/bin/sh
# scripts/guards/check-novac-mangle-fixed-point.sh — правила мэнглинга novac
# держатся ДИФФОМ с оракулом, не прозой (владелец 2026-08-15: C-имя — только
# функцией мэнглинга; функция верна ровно настолько, насколько совпадает с
# эмиссией оракула).
#
# ПРОВЕРЯЕТ: novac эмитит C для каждого файла подмножества (novac emit), из
# эмиссии берутся все идентификаторы вида Nova_<X>_method_<m> (вызовы методов
# рантайма/std) — каждый ОБЯЗАН существовать в шаблоне оболочки
# (novac/src/emit_c/shell.tpl.c = эмиссия оракула по probe). Имя, которого нет
# в оболочке, — мэнгл разошёлся с оракулом (или probe не покрывает форму) —
# красный. Символы novac-собственного пространства (novac_user_*, novac_make_*,
# NOVAC_TAG_*, Nova_<UserType>) сюда не входят: их определяет сам novac.
# НЕ ПРОВЕРЯЕТ: поведение (это смоук/дифф-раннер); имена, которых эмиссия
# подмножества не порождает.
#
# Реестр правил: план 274 §10.3/§10.3а (каждое правило — против своего
# стража); подплан 274.3 — классы находок ревью и защита от них.
# $1 — корень репозитория. Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
NAME=check-novac-mangle-fixed-point
. "$(dirname "$0")/lib/novac.sh"
NOVAC="$ROOT/novac/target/novac.exe"
SHELL_TPL="$ROOT/novac/src/emit_c/shell.tpl.c"
novac_require_bin "$NAME" "$ROOT" "$NOVAC"
[ -f "$SHELL_TPL" ] || { echo "$NAME: FAIL — нет $SHELL_TPL" >&2; exit 1; }
T="${TMPDIR:-/tmp}/novac-mangle.$$"; mkdir -p "$T"; trap 'rm -rf "$T"' 0

# Oracle-defined symbols once: the shell (oracle emission of the probe) PLUS
# the runtime headers it includes (nova_rt.h & co — nova_print_str lives
# there, not in the emission). One pass, then set difference per file.
RT_HDRS=$(ls "$ROOT"/compiler-codegen/nova_rt/*.h "$ROOT"/compiler-codegen/*.h 2>/dev/null)
cat "$SHELL_TPL" $RT_HDRS | grep -oE '\b(Nova|nova)_[A-Za-z0-9_]+' | sort -u > "$T/shell_syms"
for f in "$ROOT"/examples/basics/*.nv; do
    rel=${f#"$ROOT"/}
    "$NOVAC" emit "$f" > "$T/out.c" 2>/dev/null || continue   # subset refusals are not this guard's matter
    # Every Nova_*/nova_* identifier the emission uses, minus novac's OWN
    # namespace (novac_*/NOVAC_*) and the file's own user types
    # (Nova_<UserType>[_Tag]) — the rest is oracle-defined and MUST exist in
    # the shell. Self-test 2026-08-15 taught the guard not to grep for the
    # rule's own spelling (a broken rule then just vanished from the count).
    grep -oE '\b(Nova|nova)_[A-Za-z0-9_]+' "$T/out.c" | grep -vE '^(novac_|NOVAC_)' | sort -u > "$T/syms"
    grep -oE '^(export )?type [A-Z][A-Za-z0-9_]*' "$f" | awk '{printf "Nova_%s\nNova_%s_Tag\n", $NF, $NF}' | sort -u > "$T/user_syms"
    comm -23 "$T/syms" "$T/user_syms" > "$T/oracle_syms"
    comm -23 "$T/oracle_syms" "$T/shell_syms" | sed "s|^|  $rel: |; s|\$| — нет в оболочке (эмиссии оракула по probe)|" >> "$T/bad_all"
done
[ -s "$T/bad_all" ] && mv "$T/bad_all" "$T/bad"
if [ -f "$T/bad" ]; then
    echo "$NAME: FAIL — мэнгл novac разошёлся с оракулом:" >&2
    cat "$T/bad" >&2
    echo "  Либо правило в novac/src/sem/mangle.nv неверно, либо probe не покрывает форму — чинить дверь/probe, не эмиттер." >&2
    exit 1
fi
echo "$NAME ok: все method-имена из эмиссии подмножества существуют в оболочке оракула"
exit 0
