<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру: модульный `ro`-биндинг не парсился вовсе

## Минимум

```nova
module a

ro x = 5
```

## Замер ДО фикса

```
{"code":"E_NOVAC_SUBSET", "message":"novac did not parse this -- ..."}
```

Безымянный fallback -- даже без array-literal, record-ctor или `export`.
Реальный носитель (`novac/src/builtins/builtins.nv`) выглядел как проблема
array-literal-инициализатора (`ro universe = []BuiltinType.of(...)`), но
адрес соврал бы, если бы форму не сузили до трёх строк: причина -- сама
позиция `ro` на уровне модуля, а не то, чем она инициализирована.

## Замер ПОСЛЕ фикса

Минимум -- чисто, без диагностик. Носитель (`builtins.nv`) больше не даёт
безымянный fallback: теперь честный, названный отказ про запись-конструктор
внутри аргумента вызова (`arg_pos` не протянут через `[]T.of(...)`) --
отдельный, не тронутый этой правкой пробел.

## Норма (проверено `spec-reader` перед кодом)

- `spec/decisions/03-syntax.md`, D184, точная грамматика:
  `module_item ::= "export"? "ro" IDENT type_opt "=" expr | const_decl`
- `spec/syntax.ru.md:827-839` -- пример `ro cache_root str = compute_root()
  // ok` (без `export`) как ЗАКОННАЯ, обычная форма ("в std живёт десятками").
- Противоречие, НЕ разрешённое этой правкой: D184's BNF разрешает голый
  `export ro IDENT`, амендмент D200 (№701(а)) требует КВАЛИФИЦИРОВАННУЮ
  форму (`export ro Type.NAME`) и отвергает голую. Не тронуто -- `export`
  не подключён к новой двери вовсе, до слова интегратора.

## Корень и фикс

- `novac/src/tree/tree.nv` -- новый вариант суммы `NodeKind.LetDecl`
  (имя зеркалит спеку: D184 называет узел `Item::Let`).
- `novac/src/parse/decls.nv` -- `@let_decl()`, зеркалит `@const_decl()`
  дословно (то же `type_opt`-устройство).
- `novac/src/parse/parse.nv` -- диспетчер верхнего уровня: `KwRo` рядом
  с `KwConst`.
- `novac/src/check/check.nv` -- arm `LetDecl` в структурном обходе, той же
  схемы, что `BindStmt`: последний потомок получает `init_pos: true`.

## Доказано в обе стороны

`novac/src/parse/parse_test.nv` -- два новых теста (форма `LetDecl`,
типизированная и нетипизированная), плюс `tree_reserialize` подтверждает
losslessness. `check_test.nv` и `d49_statement_separator_newlines.nv` --
без регрессии (не проверялись на откате кода отдельно -- фикс сугубо
аддитивный: новый вариант суммы и новая ветка диспетчера, ничего
существующего не менялось).
