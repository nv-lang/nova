<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1244 -- эффект-строка перед `->` не парсилась вообще

## Минимальное репро

`effect_list.nv.txt`:

```nova
fn f() Fs -> int {
    0
}

fn g() Fs Net -> int {
    1
}

fn plain() -> int {
    3
}

fn param(a int, b str) Fs Net Time -> int {
    a
}
```

Прогон:

```sh
cp docs/plans/repro/1244-effect-list-before-arrow/effect_list.nv.txt D:/Temp/el.nv
./novac/target/novac.exe check D:/Temp/el.nv
```

## Норма (проверено spec-reader, без противоречий)

- `spec/decisions/04-effects.md` D3 (строки 191-287): эффекты между `)` и
  `->` перечисляются ПРОБЕЛОМ, без маркеров; граница структурная. Пустая
  эффект-строка легальна (`fn add(a int, b int) -> int`).
- `spec/decisions/03-syntax.md` D20 (245-381) и D45 (2846-2919): `-> ()`
  ОПУСКАЕТСЯ ВСЕГДА, включая после эффект-строки -- `fn cleanup() Io` без
  стрелки вовсе.
- `spec/decisions/04-effects.md` D209 (6239-6249): формальный нетерминал
  для этой продукции -- `effect_list`; узел дерева назван по нему,
  `EffectItem` (одно значение списка, не сама обёртка-список -- список не
  нужен как узел, эффекты просто сидят сиблингами, как `Requires`-клаузы).

## Корень

`novac/src/parse/parse.nv:473` (номер до фикса) -- разбор сигнатуры
после `)` проверял ТОЛЬКО `@peek() == TokenKind.Arrow`. Если между `)` и
`->` стоит эффект-строка, `@peek()` видит `Ident`, ветка "no arrow"
решает, что возврата нет вовсе, `RetType` не строится, а токены
эффект-строки рассыпаются в generic-фоллбек дальше по конвейеру.

## Фикс

Перед проверкой `Arrow` добавлен цикл: `while @peek() == TokenKind.Ident
{ kids.push(@node(NodeKind.EffectItem, @type_ref())) }` -- каждый эффект
читается ТЕМ ЖЕ `@type_ref()`, что и типы везде (поддерживает
параметризованные эффекты, `Fail[E]`), и становится отдельным сиблингом
`FnDecl`, как `Requires`-клаузы. Новый вид узла `EffectItem` добавлен в
`tree.nv` и `spec/nova.ungrammar`; `check.nv`'s субсетный walk получил
тривиальную ветку `EffectItem => {}` (эффекты не проверяются, E2, известный
долг -- как `ProtoMethod`/`EffectOp`).

## Замер ДО фикса

```
d0: start=3  "a function without a declared return type is not compiled yet" (f)
d1: start=7  "novac did not parse this ... parser's fallback" (Fs -> int съедено)
d2: start=31 "a function without a declared return type..." (g)
d3: start=35 "novac did not parse this..." (Fs Net -> int съедено)
d4: start=92 "a function without a declared return type..." (param)
d5: start=112 "novac did not parse this..." (Fs Net Time -> int съедено)
```
Шесть диагностик каскадом; `plain()` (без эффектов) не задет.

## Замер ПОСЛЕ фикса

Ноль диагностик, `exit=0` -- все четыре формы (один эффект, несколько
эффектов, без эффектов, эффекты с параметрами) парсятся и типизируются
начисто.

## Контроль отсутствия регрессии

- `novac/src/lex/lex_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/check/check_test.nv` -- все PASS.
- `check-novac-grammar-kinds`, `check-novac-no-default-branch`,
  `check-novac-file-size` -- зелёные.
- Тест-свидетель добавлен в `parse_test.nv`: "shape: an effect_list before
  `->` parses as EffectItem siblings (registry #1244)".
- Обнаруженная ПОБОЧНАЯ находка ПРИ проверке этого фикса (не сам предмет
  #1244): `fn h() Fs { println(...) }` (unit-возврат, block-body, без
  стрелки) ВСЁ ЕЩЁ отказывается "a function without a declared return
  type is not compiled yet" -- ПРОВЕРЕНО, что это то же самое БЕЗ
  эффект-строки вовсе (`fn h() { println(...) }` даёт идентичный отказ):
  предсуществующий, несвязанный долг подмножества (unit-return
  block-body без явной `-> ()` пока не читается novac в принципе), не
  входит в область фикса.

## Оговорка про носителя

Фикс носителя приёмкой не считается -- закрытие требует пройтись по
широкому списку `std/`-носителей (18+ файлов, найдено находкой), не
проверено в этой пробе; здесь закрыт только СИНТАКСИЧЕСКИЙ разбор формы.
