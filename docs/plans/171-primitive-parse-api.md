# Plan 171 — Primitive parse API (One-Engine, radix-only `parse`)

**Статус:** 📋 proposed 2026-06-19
**D-блок:** предв. **D309** (финал при impl; D305/D306 заняты 104.10, D307=170, D308=169.2.1)
**Ветка:** TBD (`plan-171-primitive-parse`)
**Приоритет:** P2 (std API polish + закрывает баг truncation + разрешает D74↔D77).

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

- **Десятичный** str→примитив = **только `from`/`try_from`** (D77 соблюдён). `T.try_from(s) -> Result` каноничен, `T.from(s)` — авто-derive (throw), Option — через `.ok()`. Никакого `_opt`.
- **Radix** (только целые) = `T.try_parse(s, radix int)` — существует **только** как radix-форма, **БЕЗ дефолта `radix=10`**. Десятичное — всегда `try_from`; radix — всегда `try_parse(s, radix:N)`. Поверхности не пересекаются.

### Публичные сигнатуры

```nova
// ── ДВИЖОК — std/runtime/string/parse.nv (module runtime.string). Вся логика тут. ──
export type ParseIntError | Empty | InvalidDigit | Overflow | InvalidRadix   // SHIPPED
export fn str @parse_int(radix int = 10) Fail[ParseIntError] -> int          // SHIPPED
export fn str @try_parse_int(radix int = 10) -> Result[int, ParseIntError]    // SHIPPED (i64, no-trim, 2..=36)
export fn str @parse_int_opt(radix int = 10) -> Option[int] requires radix >= 2 && radix <= 36  // SHIPPED
export fn str @try_parse_uint(radix int = 10) -> Result[uint, ParseIntError]  // NEW: u64-домен, '-' ⇒ InvalidDigit

// ── TYPE-LEVEL — std/runtime/parse_prim.nv (#no_prelude). Тонкие делегаты. ──
// ДЕСЯТИЧНОЕ (D77): пишем try_from, from авто-derive'ится (4-way):
export fn int.try_from(s str)  -> Result[int,  ParseIntError] => s.@try_parse_int(radix: 10)
export fn i32.try_from(s str)  -> Result[i32,  ParseIntError]   // тело: range-check (см. §4)
export fn i16.try_from(s str)  -> Result[i16,  ParseIntError]
export fn i8.try_from(s str)   -> Result[i8,   ParseIntError]
export fn uint.try_from(s str) -> Result[uint, ParseIntError] => s.@try_parse_uint(radix: 10)
export fn u64.try_from(s str)  -> Result[u64,  ParseIntError] => s.@try_parse_uint(radix: 10)
export fn u32.try_from(s str)  -> Result[u32,  ParseIntError]   // range-check
export fn u16.try_from(s str)  -> Result[u16,  ParseIntError]
export fn u8.try_from(s str)   -> Result[u8,   ParseIntError]
export type ParseFloatError | Empty | Invalid                   // NEW (нет Overflow: strtod сатурирует в ±Inf)
export fn f64.try_from(s str)  -> Result[f64,  ParseFloatError] // тонкая обёртка над strtod
export fn f32.try_from(s str)  -> Result[f32,  ParseFloatError]
export type ParseBoolError | Invalid                            // NEW
export fn bool.try_from(s str) -> Result[bool, ParseBoolError]  // case-sensitive true/false
// T.from(s) Fail[E] НЕ пишем руками — авто-derive из try_from (compiler-guaranteed equiv).
// Option — всегда T.try_from(s).ok(). into()/try_into() работают тем же derive.

// ── RADIX (только целые) — БЕЗ дефолта radix. ──
export fn int.try_parse(s str, radix int)  -> Result[int,  ParseIntError] => s.@try_parse_int(radix: radix)
export fn i32.try_parse(s str, radix int)  -> Result[i32,  ParseIntError]   // range-check
export fn i16.try_parse(s str, radix int)  -> Result[i16,  ParseIntError]
export fn i8.try_parse(s str, radix int)   -> Result[i8,   ParseIntError]
export fn uint.try_parse(s str, radix int) -> Result[uint, ParseIntError] => s.@try_parse_uint(radix: radix)
export fn u64.try_parse(s str, radix int)  -> Result[u64,  ParseIntError] => s.@try_parse_uint(radix: radix)
export fn u32.try_parse(s str, radix int)  -> Result[u32,  ParseIntError]   // range-check
export fn u16.try_parse(s str, radix int)  -> Result[u16,  ParseIntError]
export fn u8.try_parse(s str, radix int)   -> Result[u8,   ParseIntError]
// float/bool НЕ имеют try_parse — radix бессмыслен (вызов = ошибка компиляции).
```

### Call-site формы (что пишет автор)
```nova
ro a = int.try_from(s)?                  // десятичное, Result
ro b = int.try_from(s).ok()             // десятичное, Option
ro c int = int.from(s)                  // десятичное, throw (Fail[ParseIntError])
ro h = i32.try_parse(s, radix: 16)?     // hex в sized int, range-checked
ro u = u64.try_parse(s, radix: 16).ok() // полный u64-диапазон
```

## 3. Разрешение спеки D74 ↔ D77

- **D74 (08-runtime.md:2273-2283) — частичный RETRACT:** убрать `f64.try_parse(s)->Option[f64]` + окружающую прозу; заменить на `f64.try_from(s)->Result[f64, ParseFloatError]` + «Option via `.ok()`». Закрывает Q-from-builtins resolved-in-favor-of-D77. f64-константы (PI/E/MAX) остаются.
- **D77 (08-runtime.md:2586+) — UPHELD + carve-out:** «Option через `.ok()`» остаётся абсолютным (никакого `_opt`/`try_parse→Option`). Bullet «отвергнуто `u64.parse(s)`» (2642) дополнить: «`parse`/`try_parse` возвращается ИСКЛЮЧИТЕЛЬНО как radix-несущая целочисленная форма, т.к. фикс-сигнатура `from(s)` не вмещает radix; это НЕ альтернативное десятичное имя — десятичное всегда `from`/`try_from`, поэтому D9 не нарушен». Bullet semver обновить `u64.try_parse`→`u64.try_from`.
- **Новый D-блок (предв. D309) «Канон парсинга примитивов»:** одно правило — «radix-free decimal ⇒ from/try_from (Option через .ok()); radix-bearing integer ⇒ try_parse(s, radix); float/bool radix не имеют ⇒ parse нет». Инвариант: вся логика в str-движках, type-level — тонкие делегаты, `from` авто-derive'ится. Записать, что per-type обёртки — **interim** (схлопнутся после Plan 175.3).
- **D73 / D178 / D25 — UPHELD** без изменений.

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
| 0 | **Спека** (до кода): новый D-блок + amend D74 (retract f64.try_parse) + amend D77 (carve-out + semver bullet) + декрет no-trim как канон | D-блоки |
| 1 | **Движок**: Nova-body `str.@try_parse_uint` (u64-acc, overflow guard, '-'⇒InvalidDigit) + ParseFloatError/ParseBoolError ADT + re-export prelude | parse.nv + unit-фикстуры |
| 2 | **Type-level обёртки**: `std/runtime/parse_prim.nv` — per-type try_from (decimal) + try_parse (radix, только int) + range-check тела; f64/f32/bool через C-helpers. Проверить авто-derive `from`+`into()`/`try_into()` | parse_prim.nv |
| 3 | **Вырезать хардкод**: удалить try_parse-ветку (27036-87) + str→numeric-арм try_from (27117-78); **СОХРАНИТЬ** str.try_from([]byte) (27106) и int→char (27180). Пересобрать nova-cli (parse.nv = include_str!) | codegen clean |
| 4 | **Миграция**: json.nv:454 `f64.try_parse`→`f64.try_from` (Some/None→Ok/Err); _experimental complex/toml/url; verify semver = 0 правок; grep trim-зависимых call-sites | мигрировано |
| 5 | **Регресс-фикстуры**: range-check (i8 "999"⇒Overflow, u8 "-1"⇒InvalidDigit, sized radix overflow), full-u64 radix, match-ability ParseFloatError/ParseBoolError, decimal-via-try_from+Option-via-.ok(), radix-via-try_parse, **trim-is-error** | nova_tests/plan171 |
| 6 | **Закрытие**: полный nova test (батчи <10мин), STATUS.md/project-creation/discussion-log/simplifications, memory-статус, отметить Plan 175.3 как future-collapse | CLOSED |

## 6. Критерии приёмки

- [ ] **Без упрощений, как для прода** (обязательный критерий) — нет заглушек/TODO/хардкода пользовательских типов.
- [ ] Десятичное: `T.try_from(s)`/`T.from(s)`/`.ok()` работают для int/i*/u*/f*/bool; `from` авто-derive'ится; `into()`/`try_into()` живы.
- [ ] Radix: `i32.try_parse("ff", radix:16)` ⇒ Ok с range-check; `u64.try_parse` полный диапазон; `f64.try_parse`/`bool.try_parse` ⇒ ошибка компиляции.
- [ ] Баг truncation исправлен: `i8.try_from("999")` ⇒ `Err(Overflow)` (было `Some(-25)`).
- [ ] no-trim зафиксирован фикстурой: `int.try_from(" 42 ")` ⇒ `Err(InvalidDigit)`.
- [ ] Ошибки match-абельны (ParseIntError/ParseFloatError/ParseBoolError), плоская строка устранена.
- [ ] codegen-хардкод удалён; str.try_from([]byte) + int→char НЕ задеты (regression).
- [ ] POS+NEG тесты на релизном nova (см. test-conventions.md); 0 регрессий в json/std.
- [ ] Spec D-блоки + D74/D77 amend + README; project-creation.txt/simplifications.md/backlog-followups.md/discussion-log.md.

## 7. Риски (из синтеза)

1. **TRIM-регрессия (высший):** conv.h триммят пробелы, Nova-движки нет → `int.try_from(" 42 ")` флипнется `Ok(42)→Err(InvalidDigit)`. До удаления хардкода — grep call-sites + зафиксировать no-trim фикстурой.
2. **Хрупкость вырезания codegen:** str→numeric-арм сидит в том же `parts[1]=="try_from"` блоке, что str.try_from([]byte) и int→char, которые ДОЛЖНЫ выжить — легко over-delete. Regression на оба пути.
3. **Авто-derive `from` из `try_from`-делегата:** проверить, что 4-way реально синтезирует Fail-форму И что generic `[U TryFrom[str,E]]` резолвится на сгенерированную форму (D72) — purity-выигрыш зависит от реального срабатывания derive.
4. **Два движка (signed i64 + unsigned u64)** дублируют digit-loop — держать в lock-step (no-trim, sign, overflow); общие фикстуры на оба. Plan 175.3 их НЕ сольёт (разные overflow-домены).
5. f64/bool остаются C-backed (strtod/memcmp) — `все логика в Nova` частична; ParseFloatError теряет гранулярность strtod (только Empty|Invalid).

## 8. Связи

- **Plan 175.3 (type-set bounds)** — desirable, НЕ blocking. Схлопнёт ~13 per-type обёрток в ~2-3 generic после landing. Сейчас per-type обёртки — корректная форма (T.MAX per-concrete-type).
- Закрывает дыру `[M-91.fe2-parse-f64]` (parse_float не существовал) и параллельные для bool/char.
- Опора: D77/D73/D178/D25, static-методы примитивов (defaults.nv), numeric_type_constant_mapping.
