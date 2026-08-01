<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# План 239 — `use`: hard keyword → контекстный keyword (по образцу `bench`)

**Статус:** ✅ ГОТОВО (2026-08-01). Спека — ВНЕСЕНА (D443, `spec/decisions/02-types.md`,
тем же слиянием). Плюс окно Polaris (отдельная репа) — `@layer` → `@use`,
`*_layer` фабрики → без суффикса.
**Маркер:** `[M-use-contextual-keyword]`.
**Тема:** ретракция `use` из hard keyword (`TokenKind::KwUse`) в контекстный
(лексится как `Ident`, парсер распознаёт по позиции). Язык-меняющее (расширяет
допустимые идентификаторы — не сужает).

## 0. Контекст

Владелец поставил задачу «`use` — контекстно-зарезервированное слово (по
образцу `bench`)». Разведка показала: `use` УЖЕ был зарезервирован — но как
**hard keyword** (`TokenKind::KwUse`, безусловный, как `let`/`const`), а не
контекстный. У него уже есть три синтаксические формы:

1. **import-synonym** — `use path.to.mod` наравне с `import` (парсер принимал
   оба взаимозаменяемо с момента bootstrap).
2. **record-field embed** (D39) — `use alias Type` внутри `type { ... }`.
3. **protocol embed** (D145 §Protocol composition) — `use TypeName` в начале
   `protocol { ... }` тела.

Будучи hard keyword, `use` был **невозможен** как идентификатор ВООБЩЕ (поле,
переменная, функция, параметр, namespace-сегмент) — в отличие от `bench`,
который всегда был обычным `Ident` с позиционным распознаванием (Plan 57,
D121). Грепом `std`/`examples` (127+7 вхождений `\buse\b`) подтверждено: ни
одного идентификатора `use` в кодовой базе нет и быть не может — это чистое
расширение возможностей, не bugfix и не breaking change.

## 1. Решение

Механизм — **1:1 копия `bench`/`apply`** (см. D121, D278 §3):

- Лексер: `"use"` больше НЕ маппится на `TokenKind::KwUse` (вариант удалён из
  `TokenKind` целиком) — falls through в `Ident("use")`.
- Парсер распознаёт `use` в трёх исходных позициях через lookahead
  (`Ident("use")` + доп. проверка следующего токена), симметрично тому, как
  `bench` распознаётся по «следующий токен — string-literal»:
  - import-synonym: `use` на top-level item position + следующий токен —
    `Ident`/`.`/`..` (похоже на начало пути).
  - record-field embed: `use` + `Ident` (alias/`_`) + токен ПОСЛЕ него —
    НЕ разделитель полей (`,`/newline/`;`/`}`) ⇒ embed; иначе — обычное поле
    с именем `use`.
  - protocol embed: `use` + `Ident` (имя типа) — без изменений в логике,
    только тип токена.
- Везде, где `use` вне этих позиций — обычный `Ident`, доступен как имя поля/
  переменной/функции/параметра.

## 2. Известный компромисс — `use <Generic[T]>` как имя поля

Record-field embed lookahead смотрит только на токен ПОСЛЕ alias-идентификатора
(разделитель vs не-разделитель). Поле `use Vec[T]` (имя поля буквально `use`,
тип `Vec[T]`) при этом двусмысленно с `use Vec [T]`-подобным embed-разбором:
второй токен `Vec` не разделитель ⇒ ошибочно трактуется как embed с alias'ом
`Vec`. Крайне маловероятный edge-case (поле, named `use`, generic-типа) —
задокументирован, не заблокирован отдельной эвристикой (тот же класс
компромисса, что и у `bench`/`apply`).

## 3. Затронутые файлы

- `compiler-codegen/src/lexer/mod.rs`, `lexer/token.rs` — снятие `KwUse`.
- `compiler-codegen/src/parser/mod.rs` — 5 сайтов (`parse_import_inner`
  caller x2, record-embed `is_embed`, protocol-embed leading-items,
  stray-`use`-in-method-loop error x2, `record_lit_keyword_field_error`).
- `nova-lsp/src/semantic_tokens.rs` — `import_line_set` + `is_keyword`.
- `compiler-codegen/tests/syntax_highlight_conformance.rs` — `use`:
  ACTIVE → NON_KEYWORDS (тест anchored к живому лексеру, D278).
- `editors/vscode/syntaxes/nova.tmLanguage.json`, `editors/vim/syntax/nova.vim`,
  `editors/zed/languages/nova/highlights.scm` — `use` снят из подсветки
  (симметрично `bench`/`apply`/`null`, см. D278 §3). **Отклонение от
  буквальной формулировки исходного брифа** («добавить use рядом с bench» в
  подсветку) — верное направление, установленное governance-правилом D278 и
  живым conformance-тестом, ровно противоположное: контекстные keyword'ы
  подсвечиваться НЕ должны (иначе тест `vscode_grammar_has_no_phantom_keywords`
  краснеет). См. D443 «Что отвергнуто».
- `spec/decisions/02-types.md` — новый D443 (после D39).
- `spec/open-questions.md` — Q-embed-syntax: update-заметка (контекстность
  снимает один аргумент «за» голосования против `use`, но НЕ закрывает сам
  вопрос выбора keyword'а).
- Фикстура: `spec_tests/conformance/standalone/p239_use_contextual_ident.nv`.

## 4. Не сделано / вне рамок этого окна

- `www/site/scripts/check-highlight-keywords.mjs` + `nova-highlight.js`
  (отдельная репа `www`) держат собственную ручную копию ACTIVE-списка,
  включающую `use` — теперь устарела (drift). НЕ тронуто в этом окне (вне
  явного мандата брифа — только nova + polaris репы); нужен follow-up.
- Известная ПРЕДСУЩЕСТВУЮЩАЯ красная проверка (не моя, не трогал):
  `vscode_grammar_has_no_phantom_keywords` падает на фантоме `safe`
  (retired keyword, всё ещё в `keyword.declaration`/`storage.modifier`
  паттернах tmLanguage) — воспроизведено на `main` ДО этого окна, не связано
  с `use`.
