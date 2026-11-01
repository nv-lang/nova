#!/usr/bin/env bash
# anon-ctor-door-forms (check x K6, task-134 territory: parse/stmt.nv @anon_ctor_ahead).
#
# Вопрос, на который два места отвечают по-разному:
#   "открывает ли `{` анонимный record-конструктор?"
#
#   - novac/src/parse/stmt.nv:179-185 (НОВАЯ дверь голого блока):
#     перечисление первых записей конструктора = `..` / Ident + (`:`|`,`|`}`).
#     НЕ названы: `@x` (receiver punning, D52 правило 1) и `...base`
#     (спред, D60) -- и `..` назван ВМЕСТО `...`: спред конструктора
#     по грамматике Ellipsis, а дверь ждёт DotDot.
#   - novac/src/parse/expr.nv:495-516 + spec/nova.ungrammar:372-383:
#     читатель полей конструктора принимает `@x` (CtorField = SelfField)
#     и спред именно как '...' Expr (CtorSpread).
#   - novac/src/emit_c/emit_c.nv:851-853: эмиттер печатает панning `{ @x }`.
#
# Следствие по чтению: в statement/tail-позиции (@block читает хвост через
# тот же @stmt) `{ @x }` и `{ ...base, f: v }` уходят в голый блок --
# молча, без отказа, с иным деревом (Block вместо RecordCtor); хвост-значение
# функции при этом не рождается ("этот блок ничьё тело", typing.nv:621-631).
# Пустой `{}` (RecordCtor допускает ноль записей -- звёздочка в ungrammar:373)
# той же дверью переключён из конструктора в голый блок.
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
emit="$root/novac/src/emit_c/emit_c.nv"
gram="$root/spec/nova.ungrammar"

# --- дверь: перечисление первых записей ------------------------------------
need "$stmt" "the door itself" 'fn Cursor @anon_ctor_ahead() -> bool {'
need "$stmt" "spread arm names DotDot" 'if t1 == TokenKind.DotDot { return true }'
need "$stmt" "ident arms (colon/comma/rbrace)" \
  't2 == TokenKind.Colon || t2 == TokenKind.Comma || t2 == TokenKind.RBrace'
if awk '/fn Cursor @anon_ctor_ahead/,/^}/' "$stmt" | grep -qE 'TokenKind\.(At|Ellipsis)'; then
  echo "ANCHOR LOST: the door now names At/Ellipsis -- re-check the finding"
  fail=1
else
  echo "DIVERGENCE PRESENT: @anon_ctor_ahead names neither At (@x) nor Ellipsis (...base)"
fi

# --- выражение/грамматика/эмиттер: обе формы -- формы конструктора ---------
need "$expr" "ctor reader accepts the @x entry (D52 rule 1)" \
  '} else if @peek() == TokenKind.At && @peek(skip: 1) == TokenKind.Ident {'
need "$expr" "ctor reader takes the spread as Ellipsis" \
  'if @peek() == TokenKind.Ellipsis {'
need "$gram" "grammar: CtorField includes SelfField" '| SelfField'
need "$gram" "grammar: CtorSpread is '...' Expr" "'...' Expr"
need "$emit" "emitter prints the receiver punning" 'Receiver punning'

echo "---- the door's enumeration ----"
awk '/fn Cursor @anon_ctor_ahead/,/^}/' "$stmt" | grep -n 'TokenKind'
echo "---- the ctor reader's entry forms ----"
grep -n 'TokenKind.Ellipsis\|TokenKind.At &&\|TokenKind.Ident {' "$expr" | head -5
echo "---- the grammar ----"
sed -n '372,383p' "$gram"

[ "$fail" -eq 0 ] && echo "PROBE OK: the two doors answer differently" || echo "PROBE RED"
exit "$fail"
