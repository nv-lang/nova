#!/usr/bin/env bash
# marker-order-ungrammar (раздел 3 отчёта: противоречие текстов, не находка К6).
#
# Два нормативных места про канонический порядок маркеров type-декларации:
#   - spec/nova.ungrammar:149 и :159 (TypeDecl и SumDecl):
#     'type' 'ident' Generics? 'value'? 'consume'? ('priv' BalancedRun?)?
#     -- value РАНЬШЕ consume.
#   - spec/decisions/03-syntax.md, D241 (правка текста 2026-09-24,
#     строки ~11665-11679): consume=0 -> value=1 -> priv=2, принимается
#     ТОЛЬКО `consume value priv`.
# Новый читатель novac/src/parse/type_decl.nv:122-141 следует D241
# (consume, потом value, потом priv). Какое место верно -- развилка
# владельца, не охотника.
#
# Запуск: cmd.sh <корень-репозитория>; exit 0 = обе цитаты на месте.
set -u
root="${1:-.}"
fail=0
need() {
  if ! grep -qF "$3" "$1"; then echo "ANCHOR LOST: $2"; fail=1; fi
}

gram="$root/spec/nova.ungrammar"
d241="$root/spec/decisions/03-syntax.md"
tdecl="$root/novac/src/parse/type_decl.nv"

need "$gram" "ungrammar TypeDecl: value before consume" \
  "'type' 'ident' Generics? 'value'? 'consume'? ('priv' BalancedRun?)?"
need "$d241" "D241 ranks: consume=0, value=1, priv=2" \
  'ТОЛЬКО `consume value priv`'
need "$tdecl" "the reader now follows D241 (consume first)" \
  'if @peek() == TokenKind.KwConsume {'

echo "---- ungrammar ----"
grep -nF "'value'? 'consume'?" "$gram"
echo "---- D241 ----"
grep -nF 'ТОЛЬКО `consume value priv`' "$d241"
echo "---- the code ----"
grep -n 'KwConsume' "$tdecl" | head -3

[ "$fail" -eq 0 ] && echo "PROBE OK: both texts stand as quoted" || echo "PROBE RED"
exit "$fail"
