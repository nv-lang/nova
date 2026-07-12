<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 200 — зонтичный план улучшений std

**Статус:** 📋 реестр. **Приоритет:** ниже Plan 196 (196 — высший, без остановки). Пункты 200
**file-disjoint** от компилятора (правят `std/*.nv` + спеку/конвенцию), могут идти параллельно/после 196
отдельными дешёвыми коммитами. Каждый пункт = свой D-реф, своя приёмка, свой коммит; НЕ мега-коммит.

## Назначение

Единая точка сбора и секвенирования эргономических/корректностных улучшений stdlib. Реестр: по мере
появления новых std-улучшений — пункт сюда (с D-рефом и приёмкой), не плодить микро-планы.

---

## Пункт 1 — `Vec[T].new(cap int = 0)` точный pre-alloc конструктор

**Статус:** ✅ СДЕЛАНО 2026-07-12 (plan-200 агент [sonnet]) — `[M-vec-new-cap-default-arg-backfill]`
зачинен в РЕЗОЛВЕ (не заплатка), код внедрён. **Спека:** D372-amend2 (`spec/decisions/02-types.md`),
таблица `03-syntax.md`, пример `syntax.md`.

**Бывший блокер `[M-vec-new-cap-default-arg-backfill]` — ЗАКРЫТ, корень НЕ в 196-зоне.** Диагностика
подтвердила: класс бага **НЕ** `infer_call_ret_c`/mono-clone (196.2 W2) — это чисто callnorm-classification
gap, вне запретной зоны. Root cause: `try_normalize_call` (`callnorm.rs`) резолвит callee → `FnDecl.params`
ОДНИМ classify-match'ем (`Ident`→free / `Path`(2)→static / `Member`→instance) — единственный резолв-путь,
которым ЛЮБОЙ вызов (включая обычные static/free) доходит до `param.default` перед backfill'ом. Для
generic-static-ctor (`Type[Args].method(...)` турбофиш, `[]T.method(...)` slice-sugar D38/D239) парсер кладёт
turbofish/`__array`-Path ОДНИМ уровнем ВНУТРИ `Member.obj`, а не на верхнем уровне `func.kind` — тот же
`Member`-арм, что и обычный `obj.method()`. Этот арм БЕЗ рассмотрения этой формы шёл сразу в
`instance_by_name` (там нет ctor-имени "new") → резолв обрывался → default НЕ подставлялся. Фикс — ТА ЖЕ
точка: `Member`-арм теперь СНАЧАЛА проверяет, не является ли `obj` type-position turbofish/`__array`-путём
(если да — тот же `static_methods`, что уже используют `Path`-static-вызовы), и только иначе падает в
`instance_by_name` — сходимость на существующий резолв-путь, не новый резолвер. Отдельно найден и зачинен
ВТОРОЙ, независимый источник арности "too few arguments — cap": 3 места в `emit_c.rs`
(`try_emit_typed_vec_literal` — литерал `[...]`; `ParallelFor`-аккумулятор; rest-bind `[a, ...rest]`),
которые синтезируют `Vec.new`-вызов НАПРЯМУЮ C-строкой (нет Nova-AST `Call`-узла вообще → callnorm их не
видит по построению) — арность руками приведена к новой C-сигнатуре (`(0)`). Оба класса вне заявленной
196-зоны (`infer_call_ret_c` 46293-48883 / mono-clone) — 196-агент не тронут.

**Что:** заменить 0-арг `Vec[T].new()` на `Vec[T].new(cap int = 0) -> Self` (default-аргумент, ОДНА
функция — не overload). `new()` = пусто (cap 0, без аллокации, как сейчас); `new(cap: 1024)` = ровно 1024
слота, len 0 (именованная форма — самодокументирует намерение; позиционная `new(1024)` тоже легальна).

**Семантика (владелец 2026-07-12) — три разных намерения:**
- `new(cap)` и `@cap(n)` — **ТОЧНАЯ** ёмкость, без округления.
- `@reserve(additional)` — **амортизированный** рост, округление ВВЕРХ до степени 2 (8→16→32…).
  Уже такова (`std/collections/vec/core.nv:305`) — НЕ трогать.

**Правки кода (СДЕЛАНО):**
- `std/collections/vec/core.nv:98` — `Vec[T].new()` → `Vec[T].new(cap int = 0)`; тело: `cap == 0` → пустой
  путь (`null_buf`); `cap > 0` → `alloc_buf[T](cap)`, `len 0`, `cap` (одна аллокация).
- `std/collections/vec/core.nv:198` — `from`: `Vec[T].new().cap(items.len()).extend(items)` →
  `Vec[T].new(cap: items.len()).extend(items)`.
- Doc-комментарии `new` обновлены.
- Компиляторный фикс (см. выше): `compiler-codegen/src/callnorm.rs` (`try_normalize_call` classify-match) +
  `compiler-codegen/src/codegen/emit_c.rs` (3 hand-formatted ctor-call сайта).

**Риск/секвенс — снят.** `new(cap int = 0)` — default-arg, ОДНА функция → НЕ триггерит
`[M-vec-new-static-arity-overload]` (тот overload-класс отдельный, `from_raw_parts` остаётся именованным до
своего фикса — ниже).

**Связанный фолд (ОТДЕЛЬНО, в Plan 196.2 W2, НЕ этим коммитом):**
`fn Vec[T].from_raw_parts(ptr *T, len int, cap int) -> Self`
⇒ `fn Vec[T].new(ptr *mut T, len int, cap int) -> Self require cap >= len`
(3-арг overload + `*mut T` напрямую вместо `unsafe { ptr as *mut T }` + контракт `cap >= len`). Возможен
ТОЛЬКО после фикса `[M-vec-new-static-arity-overload]` (arity-overload cross-wiring — ДРУГОЙ класс: codegen
второе окно путает 0-арг/3-арг перегрузки; НЕ то же самое, что default-arg backfill выше, и НЕ закрыт этим
коммитом). До фикса `from_raw_parts` остаётся именованным. Отслеживается 196.2, НЕ здесь.

**Приёмка (все зелёные 2026-07-12):** conformance PASS 3/3 (single-CU; включая НОВЫЙ regression-тест на
`[M-vec-new-cap-default-arg-backfill]` — `spec_tests/conformance/d372_canonical_new_defaults.nv` +
`types_generic_static_ctor.nv` peer; red-before/green-after подтверждён на baseline-бинаре); byte-parity
(`std/collections` — единственный CC-FAIL `vec_lazy` pre-existing δ0, см. Plan 172.12); `nova test
std/collections/vec` — PASS (`access`, тот самый repro-кейс); `nova test std/runtime/string` — чистая
компиляция; D372-amend2 в спеке (сделано, расхождение спека/код снято).

---

## Пункт 2 — `priv(type)` → `priv` для итераторов

**Статус:** ✅ СДЕЛАНО 2026-07-12 (в main `c81d28419`). **Спека/конвенция:** `docs/nv-coding-style.md:221`.
`str` (`std/prelude/core.nv:211`) сознательно НЕ тронут — lang-item (ABI-мост к `nova_str`, bootstrap
pre-method, Plan 139.1); больший blast-radius, приёмочных тестов нет → **открытый вопрос владельцу.**

**Что:** итераторные типы задают поля через `value priv(type)`; правильно — field-level `priv` (D281
module-boundary). Мотивация `priv(type)` для полей итератора отсутствует и ложно-строга: по **D267** любой
модуль может писать extension-методы для типа → `priv(type)`-поля обходимы; реальная внешняя граница — `priv`
(module). Коллекционные итераторы (`VecIter` и др.) уже на field-`priv` — эталон.

**Правки:**
- `docs/nv-coding-style.md:221` — исправить правило (итераторы → `priv`, не `priv(type)`).
- 5 `*Iter`-типов на field-level `priv`:
  - `CharsIter` (`std/runtime/string/chars.nv:58`)
  - `CharIndicesIter` (`std/runtime/string/chars.nv:155`)
  - `GraphemesIter` (`std/runtime/string/unicode/graphemes.nv:84`)
  - `SentencesIter` (`std/runtime/string/unicode/sentences.nv:81`)
  - `WordsIter` (`std/runtime/string/unicode/words.nv:219`)
- **Проверить `str`** (`std/prelude/core.nv:211` `type str value priv(type)`) — ОТДЕЛЬНО: возможно намеренно
  (тип-обёртка над буфером с инвариантами), решить по месту, не менять слепо.

**Приёмка:** `nova test std` без новых фейлов.

---

## Пункт 3 (КАНДИДАТ, НЕ согласовано) — единый префикс протоколов `As*`

Владелец (2026-07-12) высказал мысль: протоколы именовать `As…` (`AsEqual`/`AsCompare`/`AsHash`/…, по образцу
`AsSlice`). Только идея. Объём большой — затрагивает `Equal`/`Compare`/`Hash`/`Display`/`Clone`/… (D229/D230/
D262 и весь std + conformance). Требует отдельного решения (D + миграция). Держу как открытый вопрос до go.

---

## Кандидаты на будущее

_(сюда — новые std-эргономические/корректностные улучшения по мере появления; каждый с D-рефом и приёмкой)_
