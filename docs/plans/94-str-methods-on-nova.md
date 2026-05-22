// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 94 — перенос алгоритмов `str` в Nova (`.nv`)

> **Статус:** 📋 proposed 2026-05-22, не начат
> **Приоритет:** P3 (de-magic / single-source-of-truth + шаг к
> self-hosting stdlib; корректность не меняется)
> **Оценка:** ~4–5 dev-day (8 фаз; основная работа — инфраструктура
> многострочных тел + 2 примитива, сами алгоритмы тривиальны)
> **Зависимости:** [Plan 90](90-memory-access-primitives.md) ✅
> (`str.byte_at`, `[]u8.compare`, `copy_*`); [Plan 93](93-option-predicates-nova-body.md)
> — **смежный** (та же де-магификация, но для sum-методов через
> `DeclaredBody`; `str` идёт другим путём — см. §«Привязка»), не hard dep.
> **Источник:** обсуждение 2026-05-22 — после Plan 90 (`byte_at`)
> str-методы стали Nova-выразимы.

## Зачем

str-методы (`starts_with`, `find`, `to_lower`, `len`, …) сейчас —
**компилятор-магия**: C-функции `nova_str_*` в `nova_rt/nova_rt.h`,
`external fn`-заглушки в авто-ген `std/runtime/string.nv`,
маршрутизация — хардкод-таблица `str_method_to_rt` в `emit_c.rs`.
«Что делает `starts_with`» нельзя прочитать в `.nv` — надо лезть в C.

После [Plan 90](90-memory-access-primitives.md) недостающий примитив
(`str.byte_at` — O(1) доступ к байту) есть, и алгоритмы str выражаются
на Nova поверх `byte_at` + `byte_len` + `StringBuilder`:

```nova
fn str @starts_with(prefix str) -> bool {
    let plen = prefix.byte_len()
    if plen > @byte_len() { return false }
    let mut i = 0
    while i < plen {
        if @byte_at(i) != prefix.byte_at(i) { return false }
        i = i + 1
    }
    true
}
```

Выигрыш — **de-magic / single-source** и шаг к self-hosting stdlib
(алгоритмы на самом языке). **Не производительность** — где это важно
(сравнение длинных строк), сохраняем memcmp-скорость через примитив
(Ф.1), а не байтовый цикл.

### Связь с self-hosting (для приоритизации)

Перенос str-алгоритмов в `.nv` — это «**stdlib на самом языке**» +
de-magic, **не предусловие self-hosting компилятора**. Самохостинг
компилятора (переписать lexer/parser/typechecker/codegen на Nova) от
этого не зависит: компилятор-на-Nova вызывает `s.starts_with(...)`
одинаково — C-трамплин за методом или Nova-тело; выхлоп — тот же C,
C-рантайм остаётся (прецедент: компилятор Go на Go, рантайм Go —
частично C/asm). Цель «stdlib/рантайм по максимуму на Nova» законна и
рядом с self-hosting, но это **отдельная** цель. → Приоритет **P3
корректен**; не помечать как self-hosting-блокер.

## Сравнение с Go / Rust / TS

| Язык | str-алгоритмы |
|---|---|
| **Rust** | `str::starts_with`/`find`/`to_lowercase`/… — обычный код в `core`/`alloc`, **не интринсики**. Ядро языка написано на самом языке. |
| **Go** | пакет `strings` — чистый Go поверх примитивов индексации. |
| **TS** | `String.prototype.*` — engine-native (C++); язык не self-hosting-ориентирован. |
| **Nova (сейчас)** | **все** str-методы — C-магия (`nova_str_*` + `external fn`-заглушки). Хуже Rust/Go — магия там, где алгоритм тривиален. |
| **Nova (цель)** | алгоритмы str — обычные Nova-методы; в C — только неустранимые примитивы (`byte_at`, `byte_len`, `from(char)`, ranged-compare). |

## Привязка к коду (сверено 2026-05-22)

- **C-реализации:** `nova_rt/nova_rt.h` — `nova_str_starts_with`,
  `nova_str_ends_with`, `nova_str_contains`, `nova_str_find`,
  `nova_str_to_upper`, `nova_str_slice`, … (+ `nova_str_byte_at` —
  Plan 90, **остаётся** примитивом).
- **Декларации:** авто-ген `std/runtime/string.nv` из
  `runtime_registry.rs` → `str_runtime()`.
- **Маршрутизация/перехват:** `emit_c.rs` — `str_method_to_rt(method)`
  (метод → C-функция) + `str_method_ret_type(method)`. Вызов
  `s.method(...)` для `obj_ty == "nova_str"` перехватывается здесь ДО
  обычного method-dispatch'а.
- **Прецедент Nova-тел:** `str.is_empty` (`nova_body: "@len() == 0"`),
  `str.plus` (`nova_body: "@concat(other)"`) — уже Nova-тела, но
  **expression-only** (`runtime_registry` `nova_body` — одна строка
  после `=>`). Многострочное тело (цикл) так не выразить.
- **Отличие от Plan 93:** `str` — примитивный тип, идёт через
  `str_method_to_rt`, НЕ через `sum_schema_registry`/`DeclaredBody`.
  Механизм проще: снять метод из `str_method_to_rt` → он резолвится
  как обычный Nova-метод. Plan 93 (`DeclaredBody` для sum) — смежная
  линия, общей инфры не делят.

## Корень проблемы №2 — где живут многострочные тела

`runtime_registry` `nova_body` — выражение. `starts_with` (цикл) —
блок. Авто-ген `string.nv` нельзя «дописать руками». Ф.0 выбирает:
- **A:** алгоритмы-методы вынести в отдельный **hand-written** `.nv`
  (напр. `std/runtime/string_ops.nv` либо sec_ция в `std/text/`);
  авто-ген `string.nv` оставить только под `external fn`-примитивы.
- **B:** расширить генератор `render_nv` под блочные тела (`nova_body`
  с многострочной строкой → `{ ... }`).
Рекомендация — A (чистое разделение: авто-ген = примитивы,
hand-written = алгоритмы).

## Scope

**Входит** — перенос на Nova-тело алгоритмов str:
- сравнение/поиск: `starts_with`, `ends_with`, `contains`, `find`,
  `rfind`, `lt`/`le`/`gt`/`ge` (`eq` — по Ф.0);
- codepoint: `len`, `char_at`;
- конструкторы: `slice`, `trim`, `to_lower`, `to_upper`, `concat`;
- коллекции: `bytes`, `chars`, `split`; `hash`.
- инфраструктура: многострочные тела (§выше) + примитивы Ф.1.

**Не входит:**
- `byte_at`, `byte_len`, `str.from(char)` (UTF-8 encode) — неустранимые
  C-примитивы, остаются.
- Внутренности `StringBuilder` / `[]T`.
- Option/Result-методы — [Plan 93](93-option-predicates-nova-body.md).
- Юникод-таблицы для `to_lower`/`to_upper` за пределами ASCII — текущая
  C-реализация ASCII-only; Nova-версия сохраняет тот же объём (полный
  Unicode-casefold — отдельный план).

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит + decision points (~0.5 д) — GATE

- **Ф.0.1** Инвентарь: для каждого str-метода — на чём пишется
  (`byte_at`-цикл / `StringBuilder` / примитив), аллоцирует ли,
  perf-чувствительность. Probe-фикстуры.
- **Ф.0.2** **Decision — примитив ranged-compare.** `starts_with`/
  `ends_with`/`eq`/`lt..ge` через `byte_at`-цикл — byte-at-a-time
  (медленнее `memcmp` на длинных входах; ранний `return` не
  векторизуется). Решить: добавить `external fn` ranged byte-compare
  на `str` (memcmp под капотом, без аллокации) — алгоритм на Nova,
  inner-compare через примитив. Рекомендация — **да** (паритет
  Go/Rust по скорости; `[]u8.compare` из Plan 90 не подходит — `str →
  []u8` аллоцирует).
- **Ф.0.3** **Decision — конструкторы строк.** `slice` — copy или
  view (`{s.ptr+off, len}` без аллокации — модель Go/Rust substring;
  Boehm interior-pointer это допускает)? `slice`/`trim` через
  `StringBuilder` посимвольно vs примитив `str.from([]u8)` /
  `StringBuilder.append` byte-range для C-скорости. Зафиксировать.
- **Ф.0.4** **Decision — `eq`/`hash`.** `hash` (FNV-1a) — byte-цикл,
  perf-нейтрально → переносится. `eq` — горячий путь (ключи HashMap);
  переносится только если Ф.0.2=да (memcmp-примитив). Зафиксировать
  границу.
- **Ф.0.5** **Decision — где тела** (вариант A/B из §выше).
- **Ф.0.6** Decision — место hand-written `.nv` + влияние на резолв
  (методы примитива `str` из stdlib-файла; проверить, что type-checker
  и codegen их подхватят — прецедент `str.is_empty`).
  Зафиксировать всё в «Итог Ф.0».

### Ф.1 — Инфраструктура (~1 д)

- **Ф.1.1** Поддержка многострочных тел (вариант Ф.0.5).
- **Ф.1.2** Примитив(ы) Ф.0.2/Ф.0.3 — ranged byte-compare на `str`;
  при необходимости `str.from([]u8)` / `StringBuilder.append`-range.
  C-реализация + registry + codegen-dispatch + тесты примитивов.
- **Ф.1.3** Механизм «снять метод из `str_method_to_rt`-перехвата →
  резолв в Nova-тело» — проверить targeted-фикстурой на одном методе.

### Ф.2 — Сравнение и поиск (~0.7 д)

- `starts_with`, `ends_with`, `contains`, `find`, `rfind`,
  `lt`/`le`/`gt`/`ge` (+ `eq` если Ф.0.4=да) → Nova-тела.
- Снять перенесённые методы из `str_method_to_rt`; удалить ставшие
  мёртвыми C-функции `nova_str_*`.
- Тесты `nova_tests/plan94/` для группы.

### Ф.3 — Codepoint-методы (~0.5 д)

- `len` (подсчёт кодпойнтов: байты, где `(b & 0xC0) != 0x80`),
  `char_at` (UTF-8 decode 1–4 байта → кодпойнт) → Nova-тела.
- Снять трамплины, тесты.

### Ф.4 — Конструкторы строк (~0.8 д)

- `concat`, `to_lower`, `to_upper`, `slice`, `trim` → Nova (поверх
  `StringBuilder` / решения Ф.0.3).
- Снять трамплины, тесты.

### Ф.5 — Коллекции и hash (~0.5 д)

- `bytes` (`[]u8`), `chars` (`[]char`), `split` (`[]str`), `hash`
  (FNV-1a) → Nova-тела.
- Снять трамплины, тесты.

### Ф.6 — Закрытие (~0.5 д)

- Полный `nova test` — 0 новых FAIL; cross-toolchain smoke.
- Проверить, что в `nova_rt.h` не осталось мёртвых `nova_str_*`
  (single-source — зеркал нет).
- spec: аменд D26/D141 — str-алгоритмы на Nova, перечень оставшихся
  C-примитивов.
- `docs/plans/README.md`, `docs/simplifications.md` (если остались
  perf-маркеры — напр. byte-at-a-time там, где Ф.0.2 решил не
  добавлять примитив), `docs/project-creation.txt`,
  `nova-private/discussion-log.md`.

## Итог Ф.0

> Заполняется по результатам аудита: таблица «метод → механизм →
> аллокации»; решения Ф.0.2–Ф.0.6 + обоснования; пер-методный план
> переноса; граница «что переносится / что остаётся C-примитивом».
> До аудита раздел пуст.

## Acceptance criteria

- [ ] Алгоритмы str (`starts_with`/`find`/`len`/`slice`/`to_lower`/
      `split`/… — полный список из Ф.0) — Nova-тела, читаемы в `.nv`.
- [ ] В C остаются только неустранимые примитивы (`byte_at`,
      `byte_len`, `from(char)`, ranged-compare, при необходимости
      `str.from([]u8)`); мёртвые `nova_str_*` удалены — зеркал нет.
- [ ] memcmp-скорость сохранена там, где Ф.0.2 это требует
      (сравнение через примитив, не byte-цикл).
- [ ] Корректность UTF-8: `len`/`char_at`/`slice` — codepoint-точны;
      byte-методы — byte-точны.
- [ ] Полный `nova test` — 0 новых FAIL; существующие потребители
      str-методов (stdlib, `nova_tests/**`) зелёные.
- [ ] spec обновлён (оставшиеся C-примитивы перечислены).

## Non-scope

- `byte_at` / `byte_len` / `str.from(char)` — остаются C.
- Полный Unicode case-folding (`to_lower`/`to_upper` за пределами
  ASCII) — Nova-версия сохраняет текущий ASCII-объём; полный Unicode —
  отдельный план.
- Option/Result-методы — [Plan 93](93-option-predicates-nova-body.md).
- `StringBuilder` / `[]T` внутренности.
- Самохостинг компилятора целиком — [Plan 01](01-roadmap-v0.1.md) v2.0+.

## Связь

- [Plan 90](90-memory-access-primitives.md) — `byte_at` и примитивы
  доступа к памяти; разблокировал этот план.
- [Plan 93](93-option-predicates-nova-body.md) — смежная де-магификация
  (Option-предикаты); общая идея «ядро на самом языке».
- [Plan 13](13-runtime-stdlib-and-autogen.md) — `runtime_registry` и
  авто-ген `std/runtime/*.nv`.
- [Plan 78](78-prelude-codegen-single-source.md) — single-source
  принцип для prelude/runtime.
- [D26](../../spec/decisions/08-runtime.md#d26),
  [D141](../../spec/decisions/08-runtime.md#d141) — stdlib/prelude и
  примитивы доступа к памяти.
- Ориентиры: Rust `core::str`, Go `strings`.
