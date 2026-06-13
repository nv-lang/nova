<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 154 — Method coherence: запрет silent no-op переопределения метода

> **Создан:** 2026-06-13. **Статус:** ✅ **CLOSED 2026-06-13** (commit `809e8605` +
> логи `063b743b`; смёржен в main `e7bad2cd` через bidirectional-синк). **P1**
> (correctness/footgun). **Эстимат:** ~½ dev-day (факт ~½).
> **Закрывает:** `[M-method-override-silent-noop]`. **D-блок:** D267 (NEW, 10-overloading.md).
> **Верификация:** `nova test plan154` 5/5 PASS (release nova); корпус-скан 0 регрессий.
> **Model:** Sonnet 4.6 + High + Thinking ON.

---

## Проблема (репро)

Пользователь определяет свою реализацию существующего метода built-in/std типа:

```nova
module user

fn str @to_lower() -> str => "USER-OVERRIDE"

fn main() -> () { print("ABC".to_lower()) }   // печатает "abc" — НЕ "USER-OVERRIDE"
```

- `nova check` / `nova run` / `test-build` — **PASS, без ошибки/warning**.
- В рантайме **выигрывает std**, тело пользователя **никогда не вызывается** —
  **тихий no-op** (мёртвый код, выглядящий рабочим). Худший исход: не ошибка, не
  override, а молчаливое игнорирование.

Сравнение: дубль метода в **одном** модуле → жёсткая ошибка D84. То есть проблема
именно в cross-module пути.

## Корневая причина (трассировка)

- `types::check_module` ([types/mod.rs:351](../../compiler-codegen/src/types/mod.rs#L351))
  детектит дубль, но `classify_dup` для cross-module метода возвращает
  `Some(false)`/`Some(true)` → **silent user-wins в `env.fns`** (Plan 62 prelude-shadow).
- **НО** codegen использует свои реестры (emit_c.rs §1c): `method_overloads`
  (multi-key Vec, call-site **first-match**) — prelude/std prepend'ится **первым**
  (imports.rs:498-506) → при резолве `str.to_lower()` берётся **std**, user-метод
  в `env.fns` codegen'ом не используется. Рассогласование type-check ↔ codegen.
- Итог: user-метод зарегистрирован, но никогда не выбирается. Диагностики нет.

## D267 — Method coherence: extension да, override существующего — нет

**Решение.** Переопределение **метода** (`fn T @m` с receiver) с **той же
сигнатурой** (receiver-type + arity + arg-types + return + receiver-mut), что у уже
существующего метода `T.@m` из **другого** модуля (std/prelude/импорт) —
**compile-error `E_METHOD_REDEFINITION`**. Потому что (а) это всё равно silent no-op
(codegen first-match), (б) глобальный override built-in метода — coherence-хазард
(stdlib и чужие либы, зовущие `to_lower`, получили бы нелокальный сюрприз).

**Разрешено (НЕ задевается):**
- **Extension с другим именем:** `fn str @shout()` — у Nova нет orphan rule
  (02-types.md §«Структурная проверка вместо impl»), добавлять методы можно.
- **Overload по сигнатуре:** `fn T @m(int)` + `fn T @m(str)` — разные оси (D84).
- **Receiver-mut overload:** `fn T @m()` + `fn T mut @m()` (Plan 135).
- **Newtype + own-method:** `type Locale { use _ str }` + `fn Locale @to_lower()` —
  override-precedence (02-types.md §«Override через own-methods»), другой receiver-тип.
- **Protocol default + impl override** (`Compare.equals` over `Equal.equals`) — другой
  путь (`check_protocol_embeds`, local-override разрешён).
- **Co-equal файлы одного модуля** (Plan 152.0/153.0 folder-split) — разные методы =
  разные ключи; один и тот же модуль, не cross-module.
- **Type/const/free-fn shadowing** — поведение Plan 62 (user-wins + W_PRELUDE_SHADOW)
  **сохранено** (фикс только для методов с receiver).

**Почему ошибка, а не «user-wins + warning»:** для built-in/std методов user-wins —
ложь (codegen всё равно берёт std). Честный сигнал — ошибка: «так нельзя, возьми
другое имя / newtype».

### Реализация

[types/mod.rs](../../compiler-codegen/src/types/mod.rs) `check_module`, `Item::Fn`
ветка: перед `match classify_dup` — `let is_method = fd.receiver.is_some();`; новая
arm `Some(_) if is_method =>` эмитит `E_METHOD_REDEFINITION` + `continue`. Остальные
arm'ы (`Some(true)`/`Some(false)`/`None`) — для free-fn/type/const, без изменений.
Site type-check → компиляция падает до codegen, фикс самодостаточен.

## Spec / D / Q / доки

- **D267** (NEW) в `spec/decisions/` (10-overloading.md рядом с D84, или 02-types.md):
  method coherence — extension да, same-sig override cross-module = `E_METHOD_REDEFINITION`.
- **D84 cross-ref** (overload axes) + **02-types §override-через-own-methods** (newtype
  путь как замена).
- **`[M-method-override-silent-noop]`** — снять из backlog (closed).
- Доки: короткий раздел в `docs/` (или комментарий) «как сделать свой to_lower»
  (newtype / extension-new-name / free fn).

## Тесты (позитивные + негативные)

Фикстуры `nova_tests/plan154/`:
- **NEG-1:** `fn str @to_lower() -> str` (override built-in str) → `E_METHOD_REDEFINITION`.
- **NEG-2:** override `Vec[T] @len` или другой std-метод → error.
- **NEG-3:** дубль метода в одном модуле (старый D84) → по-прежнему error (регресс-guard).
- **POS-1:** `fn str @shout()` (extension новое имя) → OK.
- **POS-2:** overload `fn Foo @m(int)` + `fn Foo @m(str)` → OK.
- **POS-3:** receiver-mut overload `fn Foo @m()` + `fn Foo mut @m()` → OK.
- **POS-4:** newtype `type Locale { use _ str }` + `fn Locale @to_lower()` → OK (own wins).
- **POS-5:** co-equal peers одного модуля, разные методы на одном типе → OK.
- **Регрессия:** полный `nova test` без новых FAIL vs baseline.

## Критерии приёмки

- **A1.** Репро (`fn str @to_lower`) → `E_METHOD_REDEFINITION` (был silent PASS).
- **A2.** Extension с другим именем компилируется (POS-1).
- **A3.** Overload по сигнатуре / receiver-mut — компилируется (POS-2/3).
- **A4.** Newtype + own-method работает, вызывается user-версия (POS-4).
- **A5.** Same-module дубль (D84) по-прежнему ошибка (регресс-guard NEG-3).
- **A6.** Полный `nova test` без новых FAIL vs baseline (если ломает легит prelude-
  method-shadow тесты — те ассертили баг; мигрировать или, при риске, downgrade до
  warning с обоснованием в D267).
- **A7.** D267 записан; `[M-method-override-silent-noop]` снят.

## Статус выполнения

- [x] Investigation (workflow, 3 агента): корень = type-check user-wins ↔ codegen
  first-match std-wins; legit-кейсы каталогизированы.
- [x] Fix в `types/mod.rs`: arm `Some(_) if is_method && !recv_user_local`.
- [x] **Уточнение scope (адверсариальная проверка):** первый вариант ловил
  `nova_tests/syntax/for_in_range_iter.nv` (локальные `type Range`+`fn Range @step_by`,
  legit Plan 62 user-wins). Добавлен `user_declared_types` (типы из entry-peers):
  ошибка ТОЛЬКО если receiver-тип НЕ объявлен локально юзером. str (не объявлен)
  → error; Range (объявлен) → allow.
- [x] Build (release) + репро: `fn str @to_lower` → `E_METHOD_REDEFINITION` ✅
  (был silent PASS). `for_in_range_iter` — поведение идентично baseline (остаточный
  E7320 `_cur` — pre-existing type-shadow, не мой).
- [x] Pos/neg фикстуры `nova_tests/plan154/` (5): NEG-1 override→error, NEG-2
  same-module dup→D84, POS-1 extension, POS-2 overload, POS-4 newtype-own — все ✅.
- [x] std компилируется без E_METHOD_REDEFINITION (std self-build не ловится: метод
  определён один раз, dup_existing=None).
- [x] **Финальный корпус-скан** (`nova check nova_tests`): `E_METHOD_REDEFINITION` —
  **ровно 1 вхождение, только `neg_override_str_to_lower.nv`** → ноль регрессий
  (это и есть authoritative-проверка для type-check-only правки). Корпус 2127/673/173.
- [x] **Full test-runner `nova test nova_tests/plan154`: 5/5 PASS** (2 neg
  EXPECT_COMPILE_ERROR + 3 pos прогон+assert). libuv built, GC через main vcpkg.
- [x] D267 записан (10-overloading.md).
- [ ] Коммит на `plan-154` (НЕ merged — main активно двигается).

**Итог:** `[M-method-override-silent-noop]` ЗАКРЫТ. Silent no-op → `E_METHOD_REDEFINITION`.
Фикс — type-check-only (`types/mod.rs`), emit_c не тронут → C-codegen baseline неизменён
по построению; регрессия исключена корпус-сканом. Acceptance A1-A7 выполнены.
