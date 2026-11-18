#!/usr/bin/env bash
# timeout-expr-never-hoisted (emit_c x K3, hunt 2026-11-14-emit_c-k3).
#
# Вопрос: "что может стоять выражением timeout:/deadline:?"
#
#   - novac/src/check/with_typing.nv:333-336: выражение типизируется как любое
#     значение (@type_expr), формы не перечислены и не отказаны.
#   - novac/src/lower/lower_with.nv (@lower_supervised): узел с аргументами
#     записывается ЦЕЛИКОМ как @ir.eval(st) -- выражение дедлайна никто не
#     понижает.
#   - novac/src/emit_c/emit_supervised.nv:92: @emit_expr(timeout_node) печатает
#     его из дерева; форма, которая существует в @emit_expr только как имя
#     построенного hoist'ом временного (`??`, `?`), падает ice'ом
#     emit_expr.nv:449 "a `??` the hoist never built (coalesce wave)".
#
# Оракул тот же файл собирает и печатает 5 (замер 2026-11-14): его
# собственный читатель дедлайна.emit_expr умеет форму.
#
# Запуск: cmd.sh <корень>; exit 0 = свидетели на месте (check принял, emit
# ответил ice'ом на прошедшем check коде).
set -u
root="${1:-.}"
P="$(cd "$(dirname "$0")" && pwd)"
NOVAC="$root/novac/target/novac.exe"
[ -f "$NOVAC" ] || { echo "NO $NOVAC (собери novac)"; exit 2; }
export NOVA_STD_PATH="$root/std/src"
T="${TMPDIR:-/tmp}/hunt-timeout-ice.$$"
mkdir -p "$T"; trap 'rm -rf "$T"' 0
fail=0

"$NOVAC" check "$P/probe.nv" >"$T/chk" 2>&1
[ $? -eq 0 ] && echo "check: принял (rc 0)" || { echo "ANCHOR LOST: check не принял -- носитель больше не законен"; cat "$T/chk"; fail=1; }

"$NOVAC" emit "$P/probe.nv" >"$T/p.c" 2>"$T/p.err"
rc=$?
if [ "$rc" -ne 0 ]; then
    echo "emit: rc $rc"
    grep -q 'a `??` the hoist never built (coalesce wave)' "$T/p.c" "$T/p.err" \
        && echo "witness: ice «a ?? the hoist never built» на check-чистой программе" \
        || { echo "ANCHOR LOST: emit упал другой диагностикой"; cat "$T/p.c" "$T/p.err"; fail=1; }
    grep -q 'emit_expr.nv:449' "$T/p.c" "$T/p.err" \
        && echo "witness: адрес ice -- emit_expr.nv:449" \
        || { echo "ANCHOR LOST: адрес ice не 449"; fail=1; }
else
    echo "ANCHOR LOST: emit прошёл (rc 0) -- дыра закрылась или переносится"; fail=1
fi

# якорь: читатель дедлайна печатает выражение из дерева, hoist не позван
grep -q '@emit_expr(timeout_node)' "$root/novac/src/emit_c/emit_supervised.nv" \
    && echo "witness: emit_supervised.nv печатает дедлайн из дерева" \
    || { echo "ANCHOR LOST: в emit_supervised.nv нет @emit_expr(timeout_node)"; fail=1; }
grep -q 'a `??` the hoist never built (coalesce wave)' "$root/novac/src/emit_c/emit_expr.nv" \
    && echo "witness: emit_expr.nv держит ice про нестроенный hoist" \
    || { echo "ANCHOR LOST: в emit_expr.nv нет текста ice"; fail=1; }

[ "$fail" -eq 0 ] && echo "PROBE OK: timeout: из ?? (коалесценция) проходит check и валит emit ice'ом" || echo "PROBE RED"
exit "$fail"
