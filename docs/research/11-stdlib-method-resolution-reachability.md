<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Stdlib-методы на примитивах + достижимость: как у Rust/Swift/Zig/Go и как у нас

> **Дата:** 2026-06-15. **Тип:** research / cross-ecosystem + наши эмпирические замеры.
> **Метод:** multi-agent web-верификация первоисточников (4 языка) + прямые замеры на нашем
> компиляторе (генерация C + grep). **Повод:** char Unicode-методы (`'Ω'.is_alphabetic()`) у нас
> opt-in (нужен `import std.unicode`) из-за стоимости таблиц; вопрос — как делают другие и можно
> ли у нас «без импорта + без лишней стоимости». **Родитель:** [Plan 159](../plans/159-reachability-codegen.md).

## TL;DR
- **Две НЕзависимые оси** (часто путают): (1) нужен ли `import`; (2) как выкидывается неиспользуемый stdlib.
- **Import:** Rust/Swift — **нет** (метод/свойство на `char`, core авто-в-scope). Zig/Go — **да** (свободная функция, явный импорт). → **наш `import std.unicode` = ровно как Zig/Go, это норма.**
- **Отсев неиспользуемого:** **только Zig** = ленивый фронтенд (компилит лишь достижимое от entrypoint). **Rust/Go/Swift** = прекомпиляция stdlib + срез **линкером** (`--gc-sections`/`deadcode`/`dead_strip`). Имена методов **ни у кого не зашиты в компилятор** — все в исходниках библиотеки.
- **У нас (замер):** **отсева нет вообще** — codegen эмитит ВСЁ объявленное/импортированное, даже неиспользуемые функции в том же файле. Это и есть причина opt-in.
- **Рекомендация:** реализовать **reachability-codegen (вариант A, Zig-модель)** — он у нас отсутствует, окупается на КАЖДОЙ компиляции, и снимает opt-in/цикл prelude↔unicode. Прекомпиляция-кэш (вариант B, Rust/Go) — отдельная опт-задача под скорость сборки.

## Сравнение Rust / Swift / Zig / Go (web-верифицировано)

| Язык | Нужен import? | Метод/функция | Отсев неиспользуемого | Прекомпилён std? | = вариант |
|---|---|---|---|---|---|
| **Rust** | НЕТ (`c.is_alphabetic()` inherent на `char`, в core) | метод (в `core/src/char/methods.rs`, не в компиляторе) | прекомпил `.rlib` + линкер `--gc-sections`/LTO; mono-on-use для дженериков. Каждое св-во — отдельный `static`, срезается если не ссылаются | да (`.rlib` per target) | **B** (+A-привкус для generics) |
| **Swift** | НЕТ (`Character.isLetter`, stdlib авто) | computed property (в stdlib-исходнике) | прекомпил stdlib + `dead_strip`/`--gc-sections` + WMO | да (`.swiftmodule`+runtime) | **B** |
| **Zig** | **ДА** (`@import("std")`; std только ASCII, полный Unicode — 3rd-party ziglyph/zg) | свободная функция на `u8`/`u21` (у int нет методов) | **ленивый Sema/AIR**: анализ+codegen только для декл, достижимых от entrypoint; неиспользуемое не попадает в объектник вообще (до линкера) | **нет** (std из исходников, один compilation unit) | **A** (канон) |
| **Go** | **ДА** (`import "unicode"`, `unicode.IsLetter(r)`; unused import = ошибка) | свободная функция `func IsLetter(r rune) bool` (rune=int32) | прекомпил-пакеты `.a` + линкер `cmd/link` `deadcode` flood-fill от roots; срезает неиспользуемые символы + `tables.go` RangeTable | да (per-package archive) | **B** |

**Ключевые выводы из сравнения:**
1. «Без импорта» — это **Rust/Swift** (методы на типе + core авто-импортируется). **Zig/Go требуют импорт** — как мы. Значит наш `import std.unicode` — не косяк, а мейнстрим (половина из четырёх).
2. Имя метода/функции **везде в библиотеке, не в компиляторе** (`is_alphabetic`/`IsLetter` — обычные library-символы). Мы не должны зашивать `std.unicode` в компилятор.
3. Отсев — **две модели**: Zig (ленивый фронтенд, вариант A) vs Rust/Go/Swift (прекомпил + линкер-срез, вариант B). Оси «импорт» и «отсев» ортогональны.

## Наши эмпирические замеры (этот компилятор, 2026-06-15)

Метод: `nova-codegen test-build <file>`, инспекция сгенерённого C рядом с исходником (`grep`).

**Замер 1 — folder-module целиком.** Программа `import std.unicode.{is_alphabetic}` + вызывает только `is_alphabetic(0x41)`:
- сгенерённый C = **10652 строки** (на одну assert-проверку);
- в нём НЕиспользуемые peer'ы: collate (58 совпадений), normalize (53), words/sentences/graphemes (10), case (11). → импорт folder-module тянет **все peer'ы + все `*_data` таблицы**.

**Замер 2 — гранулярность (функции).**
- *Один файл:* `fn unused_fn_marker(x)=>x+424242` (не вызывается) + вызываемая `used_fn_marker` → `424242` в C = **2** → **неиспользуемая функция эмитится**.
- *Импорт одного символа:* при вызове только `is_alphabetic` в C попали соседние функции `category.nv` (`is_whitespace`/`general_category`/`char_to_uppercase`/`char_to_lowercase`) **и** функции других peer'ов (`collate_compare`, `normalize_nfc`).

**Вывод:** в нашем codegen **анализа достижимости нет в принципе** — эмитится всё объявленное/импортированное (даже неиспользуемые функции в том же файле). C-линкер *может* срезать мёртвое из финального `.exe` (`--gc-sections`), но **стоимость компиляции** (сгенерить + прогнать через `cc` весь этот код + таблицы) платится полностью каждый раз. Это и сделало `std.unicode` opt-in.

## Рекомендация для Nova
**Вариант A (reachability-codegen, Zig-модель) — приоритетный**, потому что:
- Его у нас **нет вообще** (замеры выше) → это не оптимизация поверх существующего, а недостающий базовый механизм.
- Мы и так перегоняем std в C из исходников каждую сборку (нет `.o`/`.a`-кэша) — **это ровно модель Zig**, под которую вариант A создан. Вариант B (прекомпил-кэш) потребовал бы строить кэш-инфраструктуру + стабильный C-ABI + надеяться на `--gc-sections` единообразно на MSVC/clang/gcc (болело в Plan 82).
- Окупается на **каждой** компиляции (быстрее, меньше бинарь), не только для Unicode.
- Чисто **ломает цикл prelude↔std.unicode** и открывает no-import-эргономику (char-методы forward-объявлены в prelude, тела лениво резолвятся при достижении).

**Вариант B (прекомпил std + `cc --gc-sections`)** — отложить как ускорение СБОРКИ, когда появится формат прекомпилированного std-артефакта. Не пререквизит для корректных opt-in/no-import char-методов.

`import std.unicode` в промежутке — **не дефект** (ровно как Zig/Go).

## Источники
Rust: doc.rust-lang.org/std/primitive.char (inherent, no import); github rust-lang/rust `library/core/src/char/methods.rs` + `library/core/src/unicode/unicode_data.rs` (генерённые таблицы, per-property static); rustc-dev-guide monomorph (collector от roots); `-C link-dead-code` (gc по умолчанию). Swift: stdlib `CharacterProperties.swift`, `dead_strip`/WMO. Zig: mitchellh.com/zig/sema (lazy Sema/AIR per referenced decl от entrypoint); `lib/std/ascii.zig` (`isAlphabetic(c: u8)`); ziglang.org/learn/overview (один compilation unit, lazy top-level). Go: pkg.go.dev/unicode (`IsLetter(r rune)`); `src/cmd/link/internal/ld/deadcode.go` (flood-fill); `src/unicode/tables.go` (генерённые RangeTable). Наши замеры — `nova-codegen test-build` + grep сгенерённого C (2026-06-15).

---

## Update 2026-06-15: реализовано как Plan 159 (вариант A)

Рекомендация выше **реализована** — Plan 159 Ф.1–Ф.4 ([D283](../../spec/decisions/09-tooling.md#d283),
ветка `plan-159-reachability-impl`). Вариант A (Zig-модель, lazy reachability codegen) зашиплен: codegen
эмитит в C только достижимое от `main` (free fns + module-level `const` + `ro` lazy-static globals +
методы), worklist-обход + засев непрямых/desugar-селекторов, kill-switch `NOVA_REACH_DCE` (unset/`!=0` ⇒
ON default; `0` ⇒ байт-идентичное старое поведение). Library / no-`main` ⇒ DCE OFF (полнота API).

**Замер на той же программе** (`import std.unicode.{is_alphabetic}` + вызов только `is_alphabetic`):
сгенерённый C **10606 → 2494 строки** (~4.25×↓); `collate`/`normalize`/`GC_DATA` совпадений
**37/9/2 → 0/0/0**; нужная `ALPHA_DATA` сохранена; программа компилируется + запускается + печатает
корректно. Kill-switch A/B (`NOVA_REACH_DCE=0`) воспроизводит BEFORE точно (10606/37/9/2). Method-level
DCE — **coarse-by-name** (type∧name intersection, over-keep на name-collision: G0 «никогда не отрезать
достижимое»). Ф.4 (Option A: инъекция `import std.unicode` в entry-модуль при детекте char-method-call)
закрыла no-import char-методы (`'Ω'.is_alphabetic()` без `import`) и сняла цикл prelude↔std.unicode без
полной lazy-module-resolution. Вариант B (прекомпил std + `cc --gc-sections`) остаётся отложенным под
скорость сборки. Остаток точности — `[M-159-method-pruning]` / `[M-159-lazy-module-resolution]` (P3,
оба over-keep, не корректность; см. Q-reach-dce-precision).
