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

**Статус:** ✅ согласовано 2026-07-12. **Спека:** D372-amend2 (`spec/decisions/02-types.md`),
таблица `03-syntax.md`, пример `syntax.md` — обновлены 2026-07-12.

**Что:** заменить 0-арг `Vec[T].new()` на `Vec[T].new(cap int = 0) -> Self` (default-аргумент, ОДНА
функция — не overload). `new()` = пусто (cap 0, без аллокации, как сейчас); `new(cap: 1024)` = ровно 1024
слота, len 0 (именованная форма — самодокументирует намерение; позиционная `new(1024)` тоже легальна).

**Семантика (владелец 2026-07-12) — три разных намерения:**
- `new(cap)` и `@cap(n)` — **ТОЧНАЯ** ёмкость, без округления.
- `@reserve(additional)` — **амортизированный** рост, округление ВВЕРХ до степени 2 (8→16→32…).
  Уже такова (`std/collections/vec/core.nv:305`) — НЕ трогать.

**Правки кода:**
- `std/collections/vec/core.nv:98` — `Vec[T].new()` → `Vec[T].new(cap int = 0)`; тело: `cap == 0` → текущий
  пустой путь (`null_buf`); `cap > 0` → аллоцировать ровно `cap` (как setter `@cap(n)`: `alloc_buf[T](cap)`,
  `len 0`, `cap`).
- `std/collections/vec/core.nv:198` — `from`: `Vec[T].new().cap(items.len()).extend(items)` →
  `Vec[T].new(cap: items.len()).extend(items)` (одна аллокация, без chain).
- Обновить doc-комментарии `new` (строки 55-58, 97).

**Риск/секвенс:** `new(cap int = 0)` — default-arg, ОДНА функция → НЕ триггерит
`[M-vec-new-static-arity-overload]`, пока `new` единственный (не набор). Безопасно СЕЙЧАС, до фикса.

**Связанный фолд (ОТДЕЛЬНО, в Plan 196.2 W2, попутно с фиксом M):**
`fn Vec[T].from_raw_parts(ptr *T, len int, cap int) -> Self`
⇒ `fn Vec[T].new(ptr *mut T, len int, cap int) -> Self require cap >= len`
(3-арг overload + `*mut T` напрямую вместо `unsafe { ptr as *mut T }` + контракт `cap >= len`). Возможен
ТОЛЬКО после фикса `[M-vec-new-static-arity-overload]` (тогда `new(cap int=0)` + `new(ptr *mut T,len,cap)` =
легальный набор перегрузок). До фикса `from_raw_parts` остаётся именованным. Отслеживается 196.2, НЕ здесь.

**Приёмка:** conformance 95/0; byte-parity; `nova test std/collections/vec` без новых фейлов; D372-amend2
в спеке (сделано).

---

## Пункт 2 — `priv(type)` → `priv` для итераторов

**Статус:** ✅ согласовано (владелец: «баг конвенции — ты его нашёл»). **Спека/конвенция:**
`docs/nv-coding-style.md:221`.

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
