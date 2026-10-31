#!/usr/bin/env bash
# consume-scope-init-glue (check x K6, task-134 territory: parse/stmt.nv D188 door).
#
# Вопрос, на который два места отвечают по-разному:
#   "кто забирает `{` после инициализатора `consume x = EXPR` --
#    дверь scope-блока или читатель выражения?"
#
#   - novac/src/parse/stmt.nv:313-316 (НОВАЯ дверь D188 scope-блока):
#     считает форму по тому, что после инициализатора стоит LBrace
#     на той же строке.
#   - novac/src/parse/stmt.nv:300: инициализатор читается через @expr()
#     (без @no_ctor), а novac/src/parse/expr.nv:828-831 склеивает
#     `Name {` в RecordCtor без какой-либо форменной оговорки;
#     expr.nv:315-325 склеивает и `a.b {` (qualified head).
#     Для инициализатора-имени (`consume x = file { use(file) }`) скобка
#     съедается выражением РАНЬШЕ, чем дверь успевает её увидеть.
#   - дисциплина от этого ровно измеренного класса записана в том же
#     файле (stmt.nv, плечо with): "THE HANDLER VALUE GOES THROUGH
#     @head_expr, not @expr -- ... let the LAST binding's expression
#     swallow the body as a record constructor (`o { 1 }`)".
#
# Фикстура novac/fixtures/consume_scope_block/pos_1.nv покрывает
# инициализатор-конструктор `Flag { n: 7 }` (после него `{` не клеится --
# у postfix-цикла нет плеча LBrace); путь с голым/точечным именем
# инициализатора не покрыт.
#
# Запуск: cmd.sh <корень-репозитория>; exit 0 = расхождение подтверждено.
set -u
root="${1:-.}"
fail=0
need() {
  if ! grep -qF "$3" "$1"; then echo "ANCHOR LOST: $2"; fail=1; fi
}

stmt="$root/novac/src/parse/stmt.nv"
expr="$root/novac/src/parse/expr.nv"
fix="$root/novac/fixtures/consume_scope_block/pos_1.nv"

need "$stmt" "scope-block door keys on the brace after the initializer" \
  '&& @peek() == TokenKind.LBrace'
need "$stmt" "the same file names the discipline for this measured class" \
  'THE HANDLER VALUE GOES THROUGH @head_expr, not @expr'
need "$expr" "atom glues a bare Name + brace into a ctor, no shape guard" \
  'mut kids = []Node.of(@node(NodeKind.Name, []Node.of(name)), @take())'
need "$expr" "postfix glues a qualified head + brace into a ctor" \
  'is_qualified_ctor_head(kids)'
need "$fix" "the fixture covers only a ctor-headed initializer" \
  'consume f = Flag { n: 7 } {'

echo "---- the consume binding reads its initializer with plain @expr ----"
grep -n -B3 'The D188 SCOPE BLOCK' "$stmt" | head -12
echo "---- the glue doors the initializer passes through ----"
grep -n -A1 'if @peek() == TokenKind.LBrace && !@no_ctor {' "$expr"
echo "---- the named discipline the new door did not take ----"
grep -n 'GOES THROUGH @head_expr' "$stmt"

[ "$fail" -eq 0 ] && echo "PROBE OK: the initializer can eat the scope brace" || echo "PROBE RED"
exit "$fail"
