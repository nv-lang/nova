#!/usr/bin/env bash
# fn-result-form-list (check x K6, task-134 territory).
#
# Вопрос, на который два места отвечают по-разному:
#   "fn-тип `fn(T) -> R` (D43) в сигнатуре КОМПИЛИРУЕМОЙ декларации --
#    это читаемый declared type с именованным отказом при связывании
#    (FN_PARAM_MSG, стадия E4), или форма, отворачиваемая оптом
#    (TYPE_FORM_MSG, стадия E2-b)?"
#
#   - novac/src/check/rules.nv:144-152 (параметр): KwFn ИЗЪЯТ из списка
#     форм-отказов 2026-10-31 (task 134, class A2) -- "the fn type reads".
#   - novac/src/check/rules.nv:257-263 (результат, @report_if_return_form):
#     KwFn СТОИТ в том же списке -- `-> fn(int) -> int` по-прежнему бьёт
#     TYPE_FORM_MSG ("fn types ... arrive with generics, E2-b").
#
# Одна и та же типовая ссылка -- в позиции параметра "тип объявлен",
# в позиции результата "форма не из языка". Перечисление позиций обновлено
# в двух списках из трёх.
#
# Запуск: cmd.sh <корень-репозитория>; exit 0 = расхождение подтверждено,
# exit 1 = якорь потерян (перепроверить находку, а не считать закрытой).
set -u
root="${1:-.}"
rules="$root/novac/src/check/rules.nv"
params="$root/novac/src/check/params.nv"
fail=0
need() { # need <file> <label> <literal-anchor>
  if ! grep -qF "$3" "$1"; then echo "ANCHOR LOST: $2"; fail=1; fi
}

# --- сторона параметра: ретракция состоялась -------------------------------
need "$rules" "param-side declared-type arm" \
  'if t.kind == TokenKind.KwFn { return }'
need "$rules" "param-side retraction comment" \
  'LEFT this list on 2026-10-31 (task 134, class A2)'
need "$params" "named refusal at binding (the only FN_PARAM_MSG door)" \
  'leaf_kind(tnode) == TokenKind.KwFn {'

# --- сторона результата: тот же список KwFn НЕ покинул ---------------------
if awk '/fn Checker mut @report_if_return_form/,/^}/' "$rules" | grep -qF 't.kind == TokenKind.KwFn'; then
  echo "DIVERGENCE PRESENT: KwFn still in the RESULT-side refusal list"
else
  echo "ANCHOR LOST: KwFn gone from report_if_return_form -- re-check the finding"
  fail=1
fi

echo "---- parameter side (rules.nv, is_form list + declared arm) ----"
grep -n 'KwFn' "$rules"
echo "---- result side (report_if_return_form) ----"
awk '/fn Checker mut @report_if_return_form/,/^}/' "$rules" | grep -n 'KwFn\|is_form\|TYPE_FORM_MSG'
echo "---- the named refusal the parameter side fires instead ----"
grep -n 'FN_PARAM_MSG' "$params"

[ "$fail" -eq 0 ] && echo "PROBE OK: the two positions answer differently" || echo "PROBE RED"
exit "$fail"
