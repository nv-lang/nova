# Plan 171 — Primitive parse API (One-Engine, radix-only `parse`)

**Статус:** 📋 proposed 2026-06-19
**D-блок:** предв. **D309** (финал при impl; D305/D306 заняты 104.10, D307=170, D308=169.2.1)
**Ветка:** TBD (`plan-171-primitive-parse`)
**Приоритет:** P2 (std API polish + закрывает баг truncation + разрешает D74↔D77).

> ⚠️ **Выровнен под Plan 181 / D325 (2026-06-25).** Действует единое правило «вся падающая публичная std → `Result`» (D325, R1-R5). Конкретно для 171: **(1)** radix-форма = **`T.parse(s, radix)` → Result** (без `try_`-префикса: нет инфаллибл-сиблинга `parse`, R2/R3). **(2)** Бросковой `T.from(s)`/`T.into()` **НЕ генерится** — D77 amend 4-way→2-way (остаются `try_from`/`try_into`, оба Result); throw на call-site = `try_from(s)!!`. **(3)** str-движок схлопывается в один `@parse_int → Result`; bare `@parse_int`(Fail) + `@parse_int_opt`(Option) ретрактированы (D178-retract). **Сам rename/удаление SHIPPED-форм = Plan 181 Ф.2b (compiler-gated, emit_c.rs).** D309 подчинён D325: D325 = канон нейминга, D309/171 = примитив-специфика (radix-движок + range-check).

---

## 1. Мотивация

Парсинг примитивов из строки в Nova сегодня — **зоопарк из трёх рассогласованных механизмов** плюс баг и противоречие в спеке:

1. **str-триада** (Nova-body, только int): `str.@parse_int(radix=10)` / `@try_parse_int -> Result` / `@parse_int_opt -> Option` (parse.nv, radix 2..=36, no-trim, cap i64). D178/D25.
2. **codegen-магия** `T.try_parse(s)->Option[T]` **и** `T.try_from(s)->Result[T, <flat str>]` — захардкожена в [emit_c.rs:27036/27091](../../compiler-codegen/src/codegen/emit_c.rs), без .nv/spec/тестов, для int/i*/u*/f*/bool/char. Зовётся в `json.nv:454`, `complex.nv:369`.
3. **C-ядра** conv.h: `nova_str_to_i64/u64` (decimal-only, **триммят пробелы**, +/-), `nova_str_to_f64` (strtod), `nova_str_to_bool`.

**Баг:** range-check для sub-int отсутствует → `i8.try_parse("999")` молча даёт `Some(-25)` (C-truncation, [emit_c.rs:27073](../../compiler-codegen/src/codegen/emit_c.rs)), а не Overflow. **Ноль тестов** на это.

**Конфликт спеки:** D77 (08-runtime.md:2586) отверг `T.parse`/`T.try_parse` в пользу `from`/`try_from` (Option через `.ok()`), но D74 (08-runtime.md:2273) узаконивает `f64.try_parse->Option`. Помечено open-вопросом Q-from-builtins.

**Цель автора:** `int.parse`/`int.try_parse` с **radix** для целых. radix — ключ: фикс-сигнатура `from(s str)->T` его не вмещает.

## 2. Дизайн (выбран design-панелью: 5 дизайнов → 3 судьи → синтез)

**Принцип — разнести поверхности по сигнатуре, чтобы не было «двух дверей в десятичное» (D9):**

- **Десятичный** str→примитив = **только `try_from`** (D77 под D325). `T.try_from(s) -> Result` каноничен; throw — через `try_from(s)!!`; Option — через `.ok()`. Бросковой `T.from(s)` **не генерится** (D77 amend 4-way→2-way). Никакого `_opt`.
- **Radix** (только целые) = `T.parse(s, radix int)` — существует **только** как radix-форма, **БЕЗ дефолта `radix=10`** (R2/R3: обычное имя = Result-форма, `try_` не нужен — нет инфаллибл-сиблинга). Десятичное — всегда `try_from`; radix — всегда `parse(s, radix:N)`. Поверхности не пересекаются.

### Публичные сигнатуры

```nova
// ── ДВИЖОК — std/runtime/string/parse.nv (module runtime.string). Вся логика тут. ──
// D325: один Result-движок на домен; bare(Fail)+_opt(Option) РЕТРАКТИРОВАНЫ (D178-retract). Целевые имена ниже; rename/удаление SHIPPED-форм = Plan 181 Ф.2b (compiler-gated, emit_c.rs).
export type ParseIntError | Empty | InvalidDigit | Overflow | InvalidRadix   // SHIPPED
// РЕТРАКТ D325 (Ф.2b): export fn str @parse_int(radix int = 10) Fail[ParseIntError] -> int          // SHIPPED
export fn str @parse_int(radix int = 10) -> Result[int, ParseIntError]   // = бывш. @try_parse_int; rename ← Ф.2b. Старое:    // SHIPPED (i64, no-trim, 2..=36)
// РЕТРАКТ D325 (Ф.2b): export fn str @parse_int_opt(radix int = 10) -> Option[int] requires radix >= 2 && radix <= 36  // SHIPPED
export fn str @parse_uint(radix int = 10) -> Result[uint, ParseIntError]   // = бывш. @try_parse_uint; rename ← Ф.2b. Старое:  // NEW: u64-домен, '-' ⇒ InvalidDigit

// ── TYPE-LEVEL — std/runtime/parse_prim.nv (#no_prelude). Тонкие делегаты. ──
// ДЕСЯТИЧНОЕ (D77 под D325): пишем только try_from (Result); инфаллибл from(s) не существует (str→int падает):
export fn int.try_from(s str)  -> Result[int,  ParseIntError] => s.@parse_int(radix: 10)
export fn i32.try_from(s str)  -> Result[i32,  ParseIntError]   // тело: range-check (см. §4)
export fn i16.try_from(s str)  -> Result[i16,  ParseIntError]
export fn i8.try_from(s str)   -> Result[i8,   ParseIntError]
export fn uint.try_from(s str) -> Result[uint, ParseIntError] => s.@parse_uint(radix: 10)
export fn u64.try_from(s str)  -> Result[u64,  ParseIntError] => s.@parse_uint(radix: 10)
export fn u32.try_from(s str)  -> Result[u32,  ParseIntError]   // range-check
export fn u16.try_from(s str)  -> Result[u16,  ParseIntError]
export fn u8.try_from(s str)   -> Result[u8,   ParseIntError]
export type ParseFloatError | Empty | Invalid                   // NEW (нет Overflow: strtod сатурирует в ±Inf)
export fn f64.try_from(s str)  -> Result[f64,  ParseFloatError] // тонкая обёртка над strtod
export fn f32.try_from(s str)  -> Result[f32,  ParseFloatError]
export type ParseBoolError | Invalid                            // NEW
export fn bool.try_from(s str) -> Result[bool, ParseBoolError]  // case-sensitive true/false
// D325/D77 amend (4-way→2-way): бросковой from(s)/into() НЕ генерится. throw = try_from(s)!!.
// Option — всегда T.try_from(s).ok(). try_into() (Result) авто-derive'ится; bare into() — нет.

// ── RADIX (только целые) — БЕЗ дефолта radix. Имя `parse` (R2/R3: Result-форма, без try_). ──
export fn int.parse(s str, radix int)  -> Result[int,  ParseIntError] => s.@parse_int(radix: radix)
export fn i32.parse(s str, radix int)  -> Result[i32,  ParseIntError]   // range-check
export fn i16.parse(s str, radix int)  -> Result[i16,  ParseIntError]
export fn i8.parse(s str, radix int)   -> Result[i8,   ParseIntError]
export fn uint.parse(s str, radix int) -> Result[uint, ParseIntError] => s.@parse_uint(radix: radix)
export fn u64.parse(s str, radix int)  -> Result[u64,  ParseIntError] => s.@parse_uint(radix: radix)
export fn u32.parse(s str, radix int)  -> Result[u32,  ParseIntError]   // range-check
export fn u16.parse(s str, radix int)  -> Result[u16,  ParseIntError]
export fn u8.parse(s str, radix int)   -> Result[u8,   ParseIntError]
// float/bool НЕ имеют parse — radix бессмыслен (вызов = ошибка компиляции).
```

### Call-site формы (что пишет автор)
```nova
ro a = int.try_from(s)?                  // десятичное, Result
ro b = int.try_from(s).ok()             // десятичное, Option
ro c int = int.try_from(s)!!                  // десятичное, throw (Fail[ParseIntError])
ro h = i32.parse(s, radix: 16)?     // hex в sized int, range-checked
ro u = u64.parse(s, radix: 16).ok() // полный u64-диапазон
```

## 3. Разрешение спеки D74 ↔ D77 (под D325 / Plan 181)

- **D74 (08-runtime.md:2273-2283) — частичный RETRACT:** убрать `f64.try_parse(s)->Option[f64]` + окружающую прозу; заменить на `f64.try_from(s)->Result[f64, ParseFloatError]` + «Option via `.ok()`». Закрывает Q-from-builtins resolved-in-favor-of-D77. f64-константы (PI/E/MAX) остаются.
- **D77 (08-runtime.md:2586+) — AMEND (D325):** 4-way auto-derive → **2-way** — бросковой `from`/`into` больше НЕ генерится; остаются `try_from`/`try_into` (оба Result). «Option через `.ok()`» абсолютна (никакого `_opt`). Bullet «отвергнуто `u64.parse(s)`» (2642) дополнить: «`parse` существует ИСКЛЮЧИТЕЛЬНО как **radix-несущая** целочисленная форма `parse(s, radix)`, т.к. фикс-сигнатура `try_from(s)` не вмещает radix; это НЕ альтернативное десятичное имя — десятичное всегда `try_from`, поэтому D9 не нарушен». Bullet semver обновить `u64.try_parse`→`u64.try_from`.
- **D309 (предв.) — ПОДЧИНЁН D325:** D325 (Plan 181) задаёт канон нейминга (R1-R5: всё падающее → Result, `try_` только для `from`/`try_from`). D309/171 — **применение к примитивам**: «radix-free decimal ⇒ `try_from` (Option через `.ok()`, throw через `!!`); radix-bearing integer ⇒ `parse(s, radix)`; float/bool radix не имеют ⇒ `parse` нет». Инвариант: вся логика в str-движках (один Result-движок), type-level — тонкие делегаты. Per-type обёртки — **interim** (схлопнутся после Plan 172.3).
- **D178 — RETRACT (D325):** bare `parse_int`(Fail) + `parse_int_opt`(Option) удаляются; одна форма `@parse_int → Result` (rename = Plan 181 Ф.2b). **D73 / D25 — UPHELD** без изменений.

## 4. Фикс бага truncation (range-check)

Корень: [emit_c.rs:27073](../../compiler-codegen/src/codegen/emit_c.rs) делает сырой C-narrowing-cast без проверки. Фикс **структурный** — удалить хардкод, range-check в Nova-body обёртке до каста:
```nova
fn i8.try_from(s str) -> Result[i8, ParseIntError] {
    ro v = s.@try_parse_int(radix: 10)?
    if v < i8.MIN || v > i8.MAX { return Err(Overflow) }
    Ok(v as i8)
}
```
`i8.MIN`/`i8.MAX`/… резолвятся per-concrete-type через `numeric_type_constant_mapping` (emit_c.rs:35113, подтверждено для i8/i16/i32). Unsigned sub-width (u8/u16/u32) — через `@try_parse_uint` + верхняя граница `if v > u32.MAX { Err(Overflow) }` (unsigned-движок не даёт негативов). int/i64/uint/u64 пост-проверки не нужны (overflow ловит сам движок). Per-type (не generic) **обязательно** — T.MAX в generic-теле не резолвится до mono.

## 5. Фазы

| Ф | Тема | Выход |
|---|------|-------|
| 0 | **Спека** (до кода): D309 (подчинён D325) + amend D74 (retract f64.try_parse) + amend D77 (4-way→2-way per D325 + carve-out + semver) + декрет no-trim как канон | D-блоки |
| 1 | **Движок**: Nova-body `str.@try_parse_uint` (u64-acc, overflow guard, '-'⇒InvalidDigit) + ParseFloatError/ParseBoolError ADT + re-export prelude | parse.nv + unit-фикстуры |
| 2 | **Type-level обёртки**: `std/runtime/parse_prim.nv` — per-type try_from (decimal) + parse (radix, только int) + range-check тела; f64/f32/bool через C-helpers. Проверить 2-way derive `try_into()` (bare `from`/`into` НЕ генерятся, D325) | parse_prim.nv |
| 3 | **Вырезать хардкод**: удалить try_parse-ветку (27036-87) + str→numeric-арм try_from (27117-78); **СОХРАНИТЬ** str.try_from([]byte) (27106) и int→char (27180). Пересобрать nova-cli (parse.nv = include_str!) | codegen clean |
| 4 | **Миграция**: json.nv:454 `f64.try_parse`→`f64.try_from` (Some/None→Ok/Err); _experimental complex/toml/url; verify semver = 0 правок; grep trim-зависимых call-sites | мигрировано |
| 5 | **Регресс-фикстуры**: range-check (i8 "999"⇒Overflow, u8 "-1"⇒InvalidDigit, sized radix overflow), full-u64 radix, match-ability ParseFloatError/ParseBoolError, decimal-via-try_from+Option-via-.ok()+throw-via-`!!`, radix-via-parse, **trim-is-error** | nova_tests/plan171 |
| 6 | **Закрытие**: полный nova test (батчи <10мин), STATUS.md/project-creation/discussion-log/simplifications, memory-статус, отметить Plan 172.3 как future-collapse | CLOSED |

## 6. Критерии приёмки

- [ ] **Без упрощений, как для прода** (обязательный критерий) — нет заглушек/TODO/хардкода пользовательских типов.
- [ ] Десятичное: `T.try_from(s)`/`.ok()`/`!!` работают для int/i*/u*/f*/bool; bare `from`/`into` НЕ генерятся (D325 2-way); `try_into()` жив.
- [ ] Radix: `i32.parse("ff", radix:16)` ⇒ Ok с range-check; `u64.parse` полный диапазон; `f64.parse`/`bool.parse` ⇒ ошибка компиляции.
- [ ] Баг truncation исправлен: `i8.try_from("999")` ⇒ `Err(Overflow)` (было `Some(-25)`).
- [ ] no-trim зафиксирован фикстурой: `int.try_from(" 42 ")` ⇒ `Err(InvalidDigit)`.
- [ ] Ошибки match-абельны (ParseIntError/ParseFloatError/ParseBoolError), плоская строка устранена.
- [ ] codegen-хардкод удалён; str.try_from([]byte) + int→char НЕ задеты (regression).
- [ ] POS+NEG тесты на релизном nova (см. test-conventions.md); 0 регрессий в json/std.
- [ ] Spec D-блоки + D74/D77 amend + README; project-creation.txt/simplifications.md/backlog-followups.md/discussion-log.md.

## 7. Риски (из синтеза)

1. **TRIM-регрессия (высший):** conv.h триммят пробелы, Nova-движки нет → `int.try_from(" 42 ")` флипнется `Ok(42)→Err(InvalidDigit)`. До удаления хардкода — grep call-sites + зафиксировать no-trim фикстурой.
2. **Хрупкость вырезания codegen:** str→numeric-арм сидит в том же `parts[1]=="try_from"` блоке, что str.try_from([]byte) и int→char, которые ДОЛЖНЫ выжить — легко over-delete. Regression на оба пути.
3. **2-way derive (D325):** проверить, что `try_into()` синтезируется (Result) И что generic `[U TryFrom[str,E]]` резолвится на сгенерированную форму (D72); бросковой `from`/`into` НЕ генерится (D77 amend) — throw только через `try_from(s)!!`.
4. **Два движка (signed i64 + unsigned u64)** дублируют digit-loop — держать в lock-step (no-trim, sign, overflow); общие фикстуры на оба. Plan 172.3 их НЕ сольёт (разные overflow-домены).
5. f64/bool остаются C-backed (strtod/memcmp) — `все логика в Nova` частична; ParseFloatError теряет гранулярность strtod (только Empty|Invalid).

## 8. Связи

- **Plan 172.3 (type-set bounds)** — desirable, НЕ blocking. Схлопнёт ~13 per-type обёрток в ~2-3 generic после landing. Сейчас per-type обёртки — корректная форма (T.MAX per-concrete-type).
- Закрывает дыру `[M-91.fe2-parse-f64]` (parse_float не существовал) и параллельные для bool/char.
- Опора: D325/D77(amend 2-way)/D73/D25 (D178 retract), static-методы примитивов (defaults.nv), numeric_type_constant_mapping.
- **Plan 181 / D325** — задаёт нейминг-канон (вся падающая std → Result); 171 = его per-type реализация для примитивов (radix-движок + range-check). См. баннер вверху.
