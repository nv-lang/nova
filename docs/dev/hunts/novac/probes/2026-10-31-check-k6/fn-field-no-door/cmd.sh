#!/usr/bin/env bash
# fn-field-no-door (check x K6, task-134 territory).
#
# Вопрос, на который два места отвечают по-разному:
#   "компилируемая декларация НАЗЫВАЕТ fn-тип -- где её отказывают по имени?"
#
#   - novac/src/check/params.nv:82-86: отказ по имени (FN_PARAM_MSG)
#     существует ТОЛЬКО в позиции параметра (перечисление одной позиции).
#   - novac/src/check/check.nv:298-301: плечо Field спрашивает лишь
#     НАЛИЧИЕ типа ("field must declare a type") -- fn-тип поля читается
#     (typeref, TkFn) и не отвергается нигде.
#   - novac/src/emit_c/emit_decls.nv:61-67: КАЖДОЕ поле собственной записи
#     печатается через c_type -> novac/src/sem/mangle.nv:370-372: TkFn = ice
#     ("a fn-typed value reached emission past the closure refusal") --
#     при том что значения в носителе нет вовсе: `type R { f fn(int) -> int }`
#     не требует никакого fn-значения, чтобы дойти до эмиссии.
#
# Обещание коммита ("a compiled declaration naming one is refused by name")
# держится перечислением ОДНОЙ синтаксической позиции; та же семантика
# в позиции поля (и в generic-аргументе скомпилированного инстанса)
# падает не именованным отказом, а ice.
#
# Запуск: cmd.sh <корень-репозитория>; exit 0 = расхождение подтверждено.
set -u
root="${1:-.}"
fail=0
need() {
  if ! grep -qF "$3" "$1"; then echo "ANCHOR LOST: $2"; fail=1; fi
}

check="$root/novac/src/check/check.nv"
params="$root/novac/src/check/params.nv"
emd="$root/novac/src/emit_c/emit_decls.nv"
mangle="$root/novac/src/sem/mangle.nv"

need "$params" "the one named refusal, parameter position only" \
  'leaf_kind(tnode) == TokenKind.KwFn {'
need "$check" "Field arm asks only that A type stands there" \
  'outside the E1 subset: field must declare a type'
need "$emd" "every own record field spelled through c_type" \
  'c_type(@ctx, fd.type_id)'
need "$mangle" "the ice the field position reaches" \
  'mangle: c_type of a FUNCTION type'

echo "---- every door that names FN_PARAM_MSG (definition + use) ----"
grep -rn 'FN_PARAM_MSG' "$root/novac/src" --include='*.nv'
echo "---- the Field arm of the subset walk ----"
grep -n -A2 'Field => {' "$check" | head -8
echo "---- field spelling at emission ----"
grep -n 'c_type(@ctx, fd.type_id)' "$emd"
echo "---- the ice ----"
grep -n -B2 'c_type of a FUNCTION type' "$mangle" | head -6

[ "$fail" -eq 0 ] && echo "PROBE OK: no refusal door between the field and the ice" || echo "PROBE RED"
exit "$fail"
