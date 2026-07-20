<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 216 — Consume-дисциплина: enforce по букве (вариант А)

**Статус:** ✅ Основная волна ЗАКРЫТА (слияние `bcb3c6bd6`, 2026-07-20, в `main`);
**хвосты Err-payload + nested-tuple ЗАКРЫТЫ 2026-07-21** (ветка `p216-defer-tails`,
план 221 атом A-B7). **Приоритет P1 (основная волна) / P2 (хвосты).**
**Маркер-источник:** `[M-d180-consume-propagation-match-payload-mut-rebind]`
(backlog-followups.md §P1; найден владельцем 2026-07-17 чтением TLS-регресс-теста; home = этот план).

## Хвосты (закрыты 2026-07-21, ветка `p216-defer-tails`)

Bootstrap-волна (05-memory.md, D157-амендмент «Область») честно defer'ила
3 пункта. Разобраны по итогам:
1. **Err-пейлоад** (`Err(x)` над must-consume `E` в `Result[T,E]`) — **ЗАКРЫТО**.
   Симметрично Ok/Some: `Err(consume e)` обязателен, та же
   `E_CONSUME_PATTERN_REQUIRED`. Новая инфраструктура (`unwrap_result_err_name`,
   `unwrapped_*_return_err_types`, `var_unwrapped_err_types`,
   `infer_unwrapped_call_err_type`, `scrutinee_unwrapped_err_type`) — зеркало
   Ok/Some-путей, `compiler-codegen/src/types/mod.rs`.
2. **Nested/tuple-пейлоад** — **ЧАСТИЧНО ЗАКРЫТО: tuple-форма.**
   `Ok((a, b))`/`Some((a, b))`/`Err((a, b))` — per-element must-consume гейт
   (arity-совпадение + известный per-element unwrapped-тип; иначе
   honest-defer fallback, unchanged). Новая инфраструктура
   (`unwrap_result_option_tuple_names`/`unwrap_result_err_tuple_names`,
   `unwrapped_*_return_(err_)tuple_types`, `var_unwrapped_(err_)tuple_types`,
   `ScrutUnwrapped`-бандл, `consume_require_pattern_binding` — общий gate для
   scalar-арма И per-tuple-элемента).
   **Record-форма (`Ok({ a, b })`) НЕ покрыта** — нет per-field-type registry
   в `ConsumeCtx` для этого пути; новый followup `[M-216-record-payload-consume]`
   (P3, узкий — 0 существующих сайтов паттерн-матчат record-payload).
3. **Field/`@field`-скрутини** (`match @session { … }`) — **НЕ ТРОНУТО**
   (отдельный, более узкий пробел — followup `[M-73.2-field-scrutinee-unwrap]`,
   вне мандата этой волны хвостов).

Фикстуры: `d157_d180_consume_pattern_err_payload_ok.nv`,
`d157_d180_consume_pattern_nested_tuple_ok.nv` (pos) +
`neg/d157_err_payload_consume_required_neg.nv`,
`neg/d157_nested_tuple_consume_required_neg.nv` (neg). Спека — D157-амендмент
расширен (05-memory.md, «Amendment Plan 216 tails» + обновлённая «Область»).
Модель: sonnet.

## Мотив (одной фразой)

Цепочка `match TcpStream.connect(...) { Ok(tcp) => ... } → mut stream = session →
stream.close()` обходит статическую consume-дисциплину: double-close/use-after-close
компилятором не ловятся, хотя вся D131/D180-модель ради этого и строилась.

## Решение владельца (2026-07-18): вариант А — enforce по букве

Философия D180 «visible ownership transfer на каждом binding-site» — заявленное
преимущество Nova; узаконивание неявного move (вариант Б, Rust-стиль) отвергнуто.
Ключевой факт, удешевивший А (поправка владельца): consume-биндинг = owned-ось
D184-триады и УЖЕ mut-capable — амендмент Rule 4 не нужен, миграция механическая.

## Три слоя (ОДНО слияние — язык-меняющее, амендменты в нём же)

1. **Спека** (05-memory.md): D157-амендмент «rvalue-скрутини» — plain-биндинг
   consume-обязательного пейлоада в match/if-let по rvalue = ошибка
   `E_CONSUME_PATTERN_REQUIRED` (machine-applicable подсказка «вставьте consume»);
   владение — только `Ok(consume tcp)` (симметрия place-match `Some(consume f)`).
   D180-амендмент: Rule 2 (`E_VIEW_BINDING_FORBIDDEN`) подтверждён для pattern-bound
   consume-значений (`mut x = consume_var` → ошибка с подсказкой `consume x = ...`);
   D156-пропагация consume-обязательства через Option/Result-пейлоад нормативно enforced.
2. **Чекер** (types/mod.rs, consume-flow): пропагация обязательства на pattern-биндинги
   (rvalue И place), требование consume-паттерна на rvalue, Rule 2 начинает ловить
   алиасы. Качество диагностик — лицо фичи (точные подсказки обязательны).
3. **Миграция** (по выводу нового чекера, механически): nova (examples/tls/net,
   flagship aggregator, spec_tests, std) + nova-tls (ветка consume-a) + nova-http
   (ветка consume-a). Формы: `Ok(tcp)` → `Ok(consume tcp)` (только consume-пейлоады),
   `mut s = session` → `consume s = session`.

## Тесты

Conformance pos (consume-паттерн на rvalue: полный цикл на локальном consume-типе;
place-match view-default не сломан — примеры D157 компилируются; consume-биндинг
mut-capable) + neg (plain-биндинг → E_CONSUME_PATTERN_REQUIRED с пином текста;
`mut y = consume_var` → E_VIEW_BINDING_FORBIDDEN; double-close после паттерна →
use-after). Существующие consume-фикстуры (D131/D133/D157/D180) — зелёные или
мигрированы формой (каждая миграция — с обоснованием в отчёте; тесты не ослабляются).

## Гейты

Таргетно: новые фикстуры pos+neg; standalone-CU FAIL:0; nova check std (мигрированный);
флагман --strict-effects; nova-tls `nova test src` (+ loopback _slow один прогон).
Полный: CI nova-gate после слияния интегратором. Слияние строго ДО 214
(сериализация types/mod.rs: consume-А → 214).

## Исполнение

Волна: sonnet по карте (worktree nova-consumeA, ветка p-consume-enforce-a);
приёмка по коду + слияние + CI — интегратор. Модель фиксируется в отчёте волны.

## Связи

D131 · D133 · D156 · D157 · D180 · D184 · маркер-источник в backlog-followups.md ·
уроки: [M-tls-tests-consume-keyword-d180-drift] (симптом-родня).
