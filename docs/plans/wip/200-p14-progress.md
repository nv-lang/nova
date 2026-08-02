<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 200 Пункт 14 — прогресс-чекпоинт (комбинаторы Option/Result)

**Worktree:** `d:/Sources/nv-lang/nova-200p14`, ветка `p200-combinators` (от `main`). Модель: sonnet.

## Сделано

1. **`std/src/prelude/core.nv`**:
   - `fn Option[T] @flat_map[U](flat_map_fn fn(T) -> Option[U]) -> Option[U]`
     `=> match @ { Some(v) => flat_map_fn(v), None => None }` — после `@or` (Option-секция).
   - `fn Option[T] @filter(pred fn(T) -> bool) -> Option[T]`
     `=> match @ { Some(v) => if pred(v) { Some(v) } else { None }, None => None }` — сразу после
     `flat_map` (Option-секция).
   - `fn Result[T, E] @flat_map[U](flat_map_fn fn(T) -> Result[U, E]) -> Result[U, E]`
     `=> match @ { Ok(v) => flat_map_fn(v), Err(e) => Err(e) }` — после `@map_err` (Result-секция).
   - Все три — `#stable(since = "0.1")`, doc-комментарии в стиле соседей, Nova-body (`match @`) →
     codegen routing автоматом через `init_prelude_decls_from_items` → `MethodRouting::DeclaredBody`
     (проверено по `compiler-codegen/src/codegen/sum_schema_registry.rs`) — **компилятор НЕ трогался**.

2. **Спека** (амендмент-ноты тем же коммитом):
   - `spec/decisions/08-runtime.md` D26: новый AMEND-блок (2026-07-16, Plan 200 §14) сразу после
     AMEND unwrap-twins-retraction; сигнатуры добавлены в код-блоки «Что в prelude (v1.0)»
     (Option-блок, Result-блок); исправлена устаревшая идиома `and_then` → `flat_map` (метод не
     существовал до этого амендмента); закрыт (частично) пункт «Открытые вопросы → Q-monadic-api».
   - `spec/decisions/04-effects.md` D86: cross-ref AMEND-блок (та же философия отбора, в обратную
     сторону от ретракта unwrap-twins).

3. **Пункт 14** заведён в `docs/plans/200-std-improvements.md` (перед «Кандидаты на будущее»),
   статус ✅ РЕАЛИЗОВАНО, ссылка на research (`docs/dev/research/2026-07-16-option-result-combinators.md`,
   коммиты `f74cea01c` + `c24c5cae4`).

4. **Тест** — `spec_tests/conformance/plan200_14_option_result_flat_map_filter.nv`, 13 test-блоков:
   flat_map Some/None/short-circuit/type-changing (Option + Result), filter pass/fail/None-путь,
   композиция `filter().flat_map()`, `flat_map(...) ?? default` цепочка (env→parse идиома из research).

## НЕ добавлено (сознательно, по решению владельца)

`or_else` / `unwrap_or[_else]` / `map_or[_else]` — выразимы `??`/`match`, D86-философия. НЕ трогать.

## Верификация

Targeted `nova test` на фикстуре (главный бинарь `nova-cli/target/release/nova.exe` из main-репо,
env `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`/`NOVA_INCLUDE_DIR`/`NOVA_CG_INCLUDE`/`NOVA_RT_DIR` →
main-репо compiler-codegen — nova_rt/libuv/vcpkg главного репо, worktree их не содержит).
Результат — см. финальный отчёт агента.

## Дисциплина

git add по именам (core.nv, 08-runtime.md, 04-effects.md, 200-std-improvements.md, 200-p14-progress.md,
новая фикстура); греп конфликт-маркеров ОДНОЙ командой с commit; без Co-Authored-By; НЕ мёржить в main.
