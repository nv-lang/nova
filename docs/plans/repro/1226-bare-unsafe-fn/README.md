<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1226 — голый `unsafe fn` в двух диспетчерах

## Находка

Найдено помощником 2026-09-21, вторая строка таблицы 274.5 §5в
(`examples/typed_pointers/unsafe_fn_keyword.nv:6`). Лексер знает
`unsafe` (`KwUnsafe`) — пробел не в токене, а в ДВУХ местах парсера
разом: главный цикл верхнего уровня (`parse.nv`) и `@export_decl`
(`decls.nv`) перечисляют ведущие ключевые слова объявлений явным
списком `if/else if`, и `KwUnsafe` не входит ни в один из двух
списков. Единственное место, где `unsafe` читается сегодня —
ОПЦИОНАЛЬНЫЙ хвост внутри `@extern_decl` (`extern "ABI" unsafe fn`,
#815), который срабатывает только ПОСЛЕ `extern`.

## Метод

`@fn_decl(lead []Node)` уже принимает произвольный список leaf-узлов
как "lead" — `@extern_decl` пользуется этим, передавая
`[KwExtern, StrLit, KwUnsafe?]`. Для голого `unsafe fn` нужен тот же
приём с `lead = [KwUnsafe]`, добавленный в оба диспетчера:

- `parse.nv`'s цикл верхнего уровня: новая ветка
  `k == TokenKind.KwUnsafe && p.peek(skip: 1) == TokenKind.KwFn` перед
  веткой `KwModule`.
- `decls.nv`'s `@export_decl`: та же форма ветки, тем же условием,
  перед общим `else`.

Лукахэд на `KwFn` (а не голое совпадение по `KwUnsafe`) — чтобы форма
без `fn` (гипотетическая порча текста) по-прежнему падала в общий
junk-bucket, а не забирала токен вслепую.

## Замер ДО фикса

Перепроверено ЖИВЬЁМ (не по цитате): временный откат `parse.nv` и
`decls.nv` разом (`git checkout --`, безопасно — свои некоммиченные
файлы), пересборка. `novac.exe check` на:
- `unsafe fn risky() -> int { 0 }` (верхний уровень) — безымянный
  `E_NOVAC_SUBSET`.
- `export unsafe fn risky() -> int { 0 }` — тот же безымянный отказ,
  смещённый на позицию `unsafe`.

## Замер ПОСЛЕ фикса

`git apply` тем же патчем обратно, пересборка: обе формы —
`exit=0`, чисто.

## Гейты

`nova check novac/src` — PASS 16/FAIL 0. Новый тест `parse_test.nv`
("shape: bare `unsafe fn` parses as FnDecl through both entry doors")
разбирает ОБЕ формы, ищет `NodeKind.FnDecl` (для голой — среди
top-level `children`; для `export`-формы — внутри `NodeKind.ExportDecl`),
проверяет побайтовую реcериализацию каждой — `nova test
novac/src/parse/parse_test.nv` PASS 1/FAIL 0.

## Оговорка

Приёмка требовала обоих путей входа (топ-левел и `@export_decl`) — оба
покрыты одним фиксом и одним тестом, каждый своей формой.
