# План 214 Ф.1-Ф.3b — рабочие заметки (sonnet, worktree nova-214, ветка p214-coerce)

Базовая ревизия: 6411de6f3 (main). Карта — `docs/plans/214-coerce-attribute.md`
(ревью-7, финал), спека — D429 `spec/decisions/02-types.md:15547` (Ф.0 — DONE
до этой волны, интегратор).

## Статус: Ф.1-Ф.3b ЗАВЕРШЕНЫ. Полный вердикт — см. финальный отчёт сессии.

Эта заметка — рабочий журнал находок (не требуется для дальнейшей работы,
оставлена как история расследования, по образцу `d55-coercion-notes.md`).
Временная scratch-директория (`214-scratch/`) удалена перед закрытием волны
(её роль — только внутренний pre-Ф.3 гейт; постоянные фикстуры — задача Ф.4).

## Хронология находок (git log ветки p214-coerce, по коммитам)

1. `#coerce` атрибут: AST-поле `FnDecl.coerce_attr`, парсер + `pre_coerce`
   pre-export парс (авторы пишут `#coerce` ДО `export`, конвенция
   `#realtime`) — найдено первым же смоук-прогоном (парсер "терял" is_export
   без pre-parse). R15 — parse-time `E_COERCE_ON_PROTOCOL`.
2. `CoercePairEntry` + `collect_coerce_pairs` — реестр + валидации R1-R3,
   R11, R12, R14. Дедуп по `Span` (module.items ∩ peer_files self-collision,
   найдено смоуком).
3. Accept-путь (`assignable`) + Rewrite (`try_coerce_leaf` в
   `MapLitAnnotator`, try_wrap_leaf-семья).
4. std: `#coerce` на 3 seed-парах.
5. **ConsumeRegistry D133-credit** (finalize free-fn call-arg) — без него
   `log(sb)` давал ложный `D133-not-consumed`.
6. **Return-position rewrite gap** — `Stmt::Return`/block-trailing НЕ
   прокидывали expected-тип вообще; добавлены `current_fn_return_ty` +
   `walk_fn_body_block`, СКОУП сужен только к `#coerce` (не к
   pre-existing single-wrapper — отдельный фикс после первичной, слишком
   широкой версии).
7. R7-диагностика use-after-consume (называет implicit-#coerce-финализацию
   по методу).
8. Ф.2: `synthesize_bytes_lit_call_args` **СОХРАНЁН** (эмпирический
   эксперимент — temp-stub → CC-FAIL на `f.write("[")`/protocol-erased
   receiver — отклонение от буквального текста плана, задокументировано
   развёрнутым doc-comment на функции).
9. Ф.3b: линт `W_COERCE_EXPLICIT_REDUNDANT` (SEMANTIC-UPGRADE-класс правило,
   `nova lint` не имеет типов/import-резолва) — две итерации фикса
   (block-trailing покрытие + leaf-only ресивер, оба найдены прогоном на
   реальном std).
10. Ф.3: 23 сайта / 15 файлов мигрированы на голую форму.
11. **`simple_expr_type` fluent-chain баги (2 волны):** (а) не распознавал
    `Type.new(...).append(...)` как тип `Type` → return-position rewrite
    молчал → CC-FAIL в `std/text.nv []str @join` (через `sql_test`); (б)
    ПЕРВЫЙ фикс (а) был слишком широким — рекурсия через ЛЮБОЙ Member-вызов
    приписывала `"lit".bytes()` тип `str` (должно быть `[]u8`) → ложная
    ВСТАВКА второго `.bytes()` на уже-`[]u8` значении → CC-FAIL в
    `std/src/checksums/*_test.nv` (обязательный гейт плана) + скрыто
    поражённые `crypto`/`fs`-тесты. Финальный фикс — точное исключение
    `bytes`/`into_str`/`into_bytes` из рекурсии (эти методы НЕ fluent по
    построению, D429 R2).

## Ф.2 решение (сводка; полное обоснование — doc-comment на функции)

`emit_c.rs::synthesize_bytes_lit_call_args` СОХРАНЁН. AST-rewrite
(`MapLitCtx::resolve_call_params`/`unique_method_param_types`) резолвит
call-arg expected-тип только для ГЛОБАЛЬНО-уникального имени метода;
`write` объявлен на ≥8 разных типах (protocol `Write`, D374/D422) →
неуникален → rewrite не покрывает ИМЕННО тот protocol-erased/overloaded-
receiver кейс, для которого этот codegen pre-pass изначально писался
(Plan 208 Ф.3). Механизм уже type-directed (диспатч по `method_overloads`,
не по имени) — остаточный хардкод сузился до одной пары (str,[]u8) через
структурный C-тип предикат. Полное обобщение — легитимный follow-up.

## Известные scope-границы (не баги, документированные пределы этой волны)

- `try_coerce_leaf`/`simple_expr_type` — leaf-only (литерал/Ident с известным
  var_type), включая fluent-chain-root recovery. Chain-РЕЗУЛЬТАТ САМ (не его
  root) как значение в коэрсибельной позиции — не переписывается (accept-path
  всё равно типа-корректен через `infer_expr_type`, просто AST не
  трансформируется explicit-в-implicit).
- Return-position rewrite — НЕ протянут во вложенный tail-position
  (`if`/`match` как последнее выражение функции без `return`).
- ConsumeRegistry D133-credit — только free-fn call-arg (не method-call-arg,
  не record-literal-field).
- Линт `W_COERCE_EXPLICIT_REDUNDANT` — не флагает call-arg позиции (нет
  типов в `nova lint`), scoped к 3 std seed-методам по имени.
