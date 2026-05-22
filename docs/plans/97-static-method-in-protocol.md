// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 97 — синтаксис статических методов в `protocol {}` (резолв `Q-static-method-protocol`)

> **Статус:** 📋 proposed 2026-05-23, не начат
> **Приоритет:** P3 (закрытие spec-open-question + корректные декларации
> `From`/`TryFrom` в `protocols.nv`; корректность не меняется)
> **Оценка:** ~2–3 dev-day (парсер + AST + type-checker + protocols.nv +
> spec + тесты)
> **Зависимости:** D35 (`fn Type.name(...)` static-форма) ✅; Plan 56
> D122 amended (эффекты в protocol-методах разрешены) ✅; Plan 08 / D77
> (4-way auto-derive `from`/`try_from`) ✅.
> **Источник:** обсуждение 2026-05-23 — `protocols.nv` `From[T]`
> декларирует `from(t T) -> Self`, но `from` это **статический** метод
> (D35: `Type.from(...)`); синтаксиса static-в-protocol сейчас нет
> (`spec/decisions/03-syntax.md:3247` — `Q-static-method-protocol`).

## Зачем

Спека `03-syntax.md:3247` (раздел открытых вопросов D58) явно фиксирует:

> **Static-метод в protocol через `.method()`-префикс** — `Q-static-method-protocol`.

То есть **нет** синтаксиса, чтобы пометить метод протокола статическим.
`From[T] protocol { from(t T) -> Self }` — `from` это статический метод
по D35 (`Celsius.from(f)`), но в `protocol {}` теле он записан «голо»,
неотличимо от instance-методов (`Hashable.hash()`). Это:

- Делает декларацию неточной (теряется информация «static vs instance»).
- Делает doc-comment в `protocols.nv` противоречивым: реализация там
  показана как `fn Celsius @from(...)` (с `@` = instance, D35), что
  **противоречит** spec `fn str.from(...)` (D35 static).
- Блокирует корректное оформление `From`/`Into`/`TryFrom`/`TryInto`,
  где две статические (`from`/`try_from`) и две instance (`into`/`try_into`)
  методики идут парой.

`Q-static-method-protocol` — реальный пробел; этот план его закрывает.

## Сравнение с Go / Rust / TS

| Язык | Static-метод в trait/interface |
|---|---|
| **Rust** | `trait T { fn associated() -> Self; }` — функция без `self` это associated function. Синтаксически отличима — нет `self`-параметра. `Self::associated()` — вызов. |
| **Go** | interface'ы не имеют static-методов (только методы с receiver). |
| **TS** | interface'ы (тип-уровень) не имеют static — это инстансовые сигнатуры. Static — отдельная конструкция (`abstract class`). |
| **Nova (сейчас)** | `from(t T) -> Self` в `protocol {}` — неотличимо от instance; static не выражается. Хуже Rust (там разделение в синтаксисе). |
| **Nova (цель)** | `.from(t T) -> Self` (static, как D35-точка) vs `from(t T) -> Self` (instance) внутри `protocol {}` — явное разделение. |

## Привязка к коду (сверено 2026-05-23)

- **Spec:** `03-syntax.md:3247` — `Q-static-method-protocol` (открытый вопрос).
- **Spec:** `03-syntax.md:1262` — `fn str.from(i int) -> Self` (D35 static-форма реализации).
- **`protocols.nv`** (`std/prelude/protocols.nv`):
  - `From[T] protocol { from(t T) -> Self }` — статический метод записан как обычная сигнатура.
  - `TryFrom[T, E] protocol { try_from(t T) -> Result[Self, E] }` — то же.
  - Комментарий 101-108 («`Fail[E]` ... prohibited by Plan 56 Ф.2.7») — **stale**: запрет снят 2026-05-20 (D122 amended).
  - Doc-comment `fn Celsius @from(...)` (`@` = instance) — **противоречит** D35.
- **Парсер протоколов:** `compiler-codegen/src/parser/` — место принятия solution Ф.1.
- **Type-checker:** `compiler-codegen/src/types/mod.rs` — структурное матчинг типа против протокола; static-методы матчатся через `Type.method` (D35), instance через `value.method`.
- **D77 4-way auto-derive** существует (`emit_c.rs:383`, `:15213`); связь `from`/`into`/`try_from`/`try_into` через Fail↔Result — план не меняет, но **корректные декларации** под него важны.

## Scope

**Входит:**
- Синтаксис `.method(...)`-префикса для static-методов в `protocol {}` теле
  (`.from(t T) -> Self`, `.try_from(t T) -> Result[Self, E]`).
- AST: пометка static на protocol-методах.
- Type-checker: матчинг static-методов протокола через `Type.method`-сигнатуру.
- Backwards-compat: bare-имена (`hash()`, `next()`, `into()`) остаются **instance** —
  существующие протоколы (`Iter`/`Hashable`/`Equatable`/`Comparable`/`Display`/`Into`/`TryInto`)
  не ломаются.
- Update `protocols.nv` — `From`/`TryFrom` под новый синтаксис.
- Destale comment 101-108 — переписать причину `Result`-формы под
  `try_`-prefix convention + D77 4-way auto-derive (не «ban»).
- Spec — `03-syntax.md`: закрыть `Q-static-method-protocol`; новый
  D-block или amend D58.
- Тесты pos/neg.

**Не входит:**
- `@method`-префикс для явных instance (тоже Q-open). Backwards-compat
  «bare = instance» закрывает потребность; явный `@` — отдельный вопрос,
  можно сделать позже как сахар-симметрию.
- Изменение `From`/`TryFrom` семантики (D77 auto-derive — как есть).
- Перевод `TryFrom`/`TryInto` с `Result` на `Fail`-эффект — `try_`-prefix
  convention диктует `Result`, остаётся.

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит + decision (~0.5 д) — GATE

- **Ф.0.1** Локализовать parser-точку: где парсится `protocol { }` тело
  и сигнатуры методов внутри.
- **Ф.0.2** Decision на синтаксис: подтвердить **`.method()`-префикс**
  для static (намечено spec `03-syntax.md:3247`). Альтернативы
  (`static method`, `[static] method`) — отвергнуть по симметрии с D35.
- **Ф.0.3** Decision на backwards-compat: bare-имена = instance (текущее
  поведение, ничего не ломается); `.method` = static (новое). Зафиксировать.
- **Ф.0.4** Матчинг типа против протокола: что меняется в type-checker'е.
  Currently все методы матчатся как instance — нужно различать static.
- **Ф.0.5** Влияние на codegen: protocol-bound dispatch (vtable / mono).
  Static-методы протокола вызываются как `T.method` — codegen-routing
  через `current_type_subst` (Plan 88) для generic-bound `T`. Зафиксировать.
- **Ф.0.6** «`@method` для instance тоже Q-open» — фиксируем: НЕ в scope
  этого плана; bare = instance остаётся. Симметрия — отдельный мелкий
  followup, если потребуется.

### Ф.1 — Парсер + AST + type-checker (~1.2 д)

- **Ф.1.1** Парсер `protocol { }` body — принять `.identifier(...)` как
  static-метод; bare `identifier(...)` остаётся instance.
- **Ф.1.2** AST: добавить поле `is_static: bool` (или эквивалент) на
  protocol-метод. По умолчанию `false` (backwards-compat).
- **Ф.1.3** Type-checker: при структурном матчинге типа против
  протокола, для `is_static = true` — искать `fn Type.method(...)` (не
  `fn Type @method`).
- **Ф.1.4** Codegen: при вызове static protocol-метода (`T.from(...)` в
  generic-bound контексте) — через mono-substitution `T` → концретный
  тип (как Plan 88 — `apply_type_subst_to_ref`).
- **Ф.1.5** Build + targeted parse-test.

### Ф.2 — Update `protocols.nv` (~0.3 д)

- **Ф.2.1** `From[T] protocol { .from(t T) -> Self }` — static-форма.
- **Ф.2.2** `TryFrom[T, E] protocol { .try_from(t T) -> Result[Self, E] }`.
- **Ф.2.3** `Into[U] protocol { into() -> U }` — instance, без изменений.
- **Ф.2.4** `TryInto[U, E] protocol { try_into() -> Result[U, E] }` — instance.
- **Ф.2.5** Build + проверить, что существующий stdlib (где `Type.from`/`Type.try_from`
  объявлены через `fn Type.from(...)` D35-static) — продолжает удовлетворять
  обновлённым протоколам.

### Ф.3 — Destale + spec sync (~0.3 д)

- **Ф.3.1** `protocols.nv` 101-108 — переписать комментарий:
  - убрать ссылку на «prohibited by Plan 56 Ф.2.7» (снято 2026-05-20);
  - объяснить `Result`-форму через `try_`-prefix convention + D77 4-way
    auto-derive (Fail↔Result);
  - cross-ref D77.
- **Ф.3.2** Doc-comment пример «реализация: `fn Celsius @from(...)`» —
  поправить на `fn Celsius.from(...)` (D35 static-точка, не `@`).
- **Ф.3.3** Spec `03-syntax.md` — закрыть `Q-static-method-protocol`:
  убрать из раздела «открытые вопросы D58», добавить нормативный текст
  про `.method`-префикс для static в `protocol {}`. Либо новый
  D-block, либо amend D35/D58 — решить в Ф.0.
- **Ф.3.4** `spec/decisions/README.md` — индекс, если новый D-block.

### Ф.4 — Тесты pos/neg (~0.4 д)

- **Ф.4.1** `nova_tests/plan97/protocol_static_from.nv` — позитив:
  user-type реализует `From[T]` через `fn MyT.from(t T)` (D35 static);
  bound `[T From[X]]` корректно резолвит `T.from(v)`.
- **Ф.4.2** `nova_tests/plan97/protocol_static_try_from.nv` — позитив:
  `TryFrom[T, E]` реализация + bound dispatch.
- **Ф.4.3** `nova_tests/plan97/protocol_instance_unchanged.nv` —
  регресс-позитив: bare-имена (`Hashable.hash`/`Iter.next`) продолжают
  работать как instance.
- **Ф.4.4** `nova_tests/plan97/neg_static_vs_instance_mismatch.nv` —
  негатив: тип объявил `fn T @method` (instance) когда протокол требует
  `.method` (static) → compile error.
- **Ф.4.5** `nova_tests/plan97/neg_instance_vs_static_mismatch.nv` —
  негатив: обратное (тип объявил `fn T.method` static когда протокол
  требует instance) → compile error.
- **Ф.4.6** `nova_tests/plan97/from_into_d77_autoderive.nv` — регресс:
  D77 4-way auto-derive продолжает работать с обновлёнными декларациями.
- **Ф.4.7** Полный `nova test` — 0 новых FAIL.

### Ф.5 — Финал: регресс + docs + логи (~0.3 д)

- **Ф.5.1** Полный регресс — 0 новых FAIL.
- **Ф.5.2** `docs/plans/README.md` — Plan 97 → ЗАКРЫТ.
- **Ф.5.3** `docs/simplifications.md` — если что-то отложили (`@method`
  явный instance) — маркер.
- **Ф.5.4** `docs/project-creation.txt` — запись.
- **Ф.5.5** `nova-private/discussion-log.md` — запись.
- **Ф.5.6** Merge `plan-97` → `main`.

## Итог Ф.0

> Заполняется по результатам аудита: parser-точка (Ф.0.1), подтверждение
> `.method`-префикса (Ф.0.2), фиксация backwards-compat (Ф.0.3), карта
> type-checker'а (Ф.0.4), codegen-маршрут (Ф.0.5). До аудита пусто.

## Acceptance criteria

- [ ] `protocol { .from(t T) -> Self }` парсится; `from` — static.
- [ ] `protocol { method() }` (bare) парсится; `method` — instance
      (backwards-compat).
- [ ] `From[T] protocol { .from(t T) -> Self }` — корректная декларация
      в `protocols.nv`; существующий stdlib продолжает удовлетворять
      протоколу.
- [ ] `TryFrom[T, E] protocol { .try_from(t T) -> Result[Self, E] }` —
      то же.
- [ ] Type-checker матчит static-метод протокола против `fn Type.method`
      (D35), instance — против `fn Type @method`.
- [ ] Compile error при mismatch static/instance.
- [ ] D77 4-way auto-derive (`from`/`into`/`try_from`/`try_into`) —
      продолжает работать.
- [ ] Spec `Q-static-method-protocol` закрыт; новый D-block или amend
      D58/D35.
- [ ] Полный `nova test` — 0 новых FAIL.

## Non-scope

- `@method`-префикс для явных instance в protocol — Q-open, отдельный
  followup. Bare = instance закрывает практику.
- Изменение D77 auto-derive — не трогаем.
- Перевод `TryFrom`/`TryInto` на `Fail`-эффект — отменено (`try_`-prefix
  convention).
- Полная сверка всех протокол-методов stdlib (`Iter`/`Hashable`/...) —
  они уже корректно bare-instance; правки не нужны.

## Связь

- D35 (`03-syntax.md`) — static (`.`) vs instance (`@`) методы.
- D58 (`03-syntax.md`) — `Q-static-method-protocol`; этот план его закрывает.
- D77 (`08-runtime.md`) — 4-way auto-derive (`from`/`into`/`try_from`/`try_into`).
- D122 amended (`02-types.md:3229`) — эффекты в protocol-методах разрешены
  (фон, не блокер плана).
- Plan 56 Ф.2.7 REVERTED 2026-05-20.
- Plan 08 — `From`/`Into`/`TryFrom`/`TryInto` базовая инфра.
