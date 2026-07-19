# Числовой паритет — рабочие заметки (волна 2026-07-19)

Контекст: владелец 2026-07-18 — «i64.clamp — ДА, для всех чисел должен быть,
что-то забыли?» → аудит + добор. Worktree `nova-numparity`, ветка
`p-numeric-parity`. Модель: sonnet.

Промежуточный чекпоинт — черновик, будет дополняться по ходу работы.

## Найдено ДО начала добора (текущее состояние HEAD b2bfa0505)

Ключевой факт: блокер Plan 200 п.10 **уже закрыт в дереве** —
`fn[T Ints] T @clamp(lo T, hi T) -> T` (std/src/prelude/protocols.nv:1057-1073,
комментарий «Plan 200 Step 0 (D74 amend, владелец 2026-07-16)») уже покрывает
i8/i16/i32/i64/int/u8/u16/u32/u64/uint. Старый конкретный `int @clamp`
(std/src/runtime/defaults.nv) retracted с явной пометкой. `f64 @clamp` остаётся
отдельным конкретным методом (f64 ∉ Ints). `duration/core.nv::sat_add_i64`/
`sat_sub_i64` (~строки 400-410) уже используют `r.clamp(lo, hi)` через этот
бланкет.

Plan 200 п.10 (docs/plans/200-std-improvements.md:308-329) уже помечен
«✅ СДЕЛАНО 2026-07-16», но приёмка явно отмечает: «Полный nova test/
conformance — НЕ прогнан этой сессией». Моя задача по этому пункту —
ПРОГНАТЬ `nova test std/src/time` и подтвердить зелёную талли, обновить
статус пункта явной записью о факте прогона.

## Аудит семейств методов (в процессе)

Источники: std/src/runtime/defaults.nv, std/src/runtime/numeric.nv,
std/src/runtime/math.nv, std/src/prelude/protocols.nv (Ints/SignedInt/
UnsignedInt type-sets, D423 бланкеты).

Подтверждено:
- **MIN/MAX** — компиляторный builtin (emit_c.rs `numeric_type_constant_mapping`,
  ~строка 48727), покрывает ВСЕ int-типы (i8..i64/int/u8..u64/uint) +
  f32/f64 (+ MIN_POSITIVE/EPSILON/NAN/INFINITY/NEG_INFINITY/PI/E только f32/f64,
  ожидаемо). Дыр нет.
- **clamp** — Ints-бланкет (все 10 int-типов) + f64 concrete. **f32 clamp
  ОТСУТСТВУЕТ** — гол, тривиальный зеркальный добор (копия f64-тела).
- **min/max (@min/@max scalar)** — defaults.nv, все 12 числовых типов
  (i8..i64/int/u8..u64/uint/f32/f64). Дыр нет.
- **checked_add/sub/mul/div/rem/neg, wrapping_*, saturating_add/sub/mul** —
  Ints-бланкеты (D423/Plan 206), все 10 int-типов. `saturating_div`/
  `saturating_neg`/`overflowing_div`/`overflowing_neg` — не встречены (Rust
  их тоже не даёт для sub/mul/add-only saturating семейства — не гол, а
  соответствие эталону).
- **abs** — ТОЛЬКО `int` (extern "nova", C `llabs`) и f32/f64 (extern "nova",
  `fabsf`/`fabs`). i8/i16/i32/i64 (SignedInt \ {int}) — ОТСУТСТВУЕТ.
  Unsigned abs не имеет смысла (как и в Rust). **Семантический вопрос**:
  `int.abs()` на `int.MIN`/`i64.abs()` на `i64.MIN` — через `llabs` UB по
  C-стандарту при NEGATE overflow; корректный трап-политике D423 вариант —
  чистое `.nv`-тело `if @ < 0 { -@ } else { @ }` (unary negate уже трапит на
  T.MIN согласно D423/D427). Это меняет поведение существующего `int.abs()`
  (сейчас — llabs, недетерминированно/UB на MIN, не трап) → ТРЕБУЕТ решения
  владельца, не мержу молча в рамках "тривиального зеркала".
- **signum/is_negative/is_positive** — ТОЛЬКО `int` (defaults.nv, чистые
  `.nv`-тела, никакого overflow-риска). Гол: i8/i16/i32/i64 (SignedInt \
  {int}) не имеют этих методов. Это МЕХАНИЧЕСКИЙ зеркальный добор (тела не
  делают арифметики с переполнением — только сравнения) → добираю как
  `fn[T SignedInt] T @signum() -> T` + `@is_negative`/`@is_positive`
  (SignedInt-бланкет). Unsigned версии НЕ добавляю (Rust тоже не даёт
  is_negative/is_positive/signum для unsigned — соответствие эталону,
  не искусственное расширение охвата).
- **pow** — ТОЛЬКО f32/f64 (extern "nova" math.nv). Целочисленного `pow`
  (int/i64/...) НЕТ ВООБЩЕ, ни в каком виде — соответственно и
  checked_pow/wrapping_pow/saturating_pow тоже нет. Это НЕ «зеркало соседнего
  типа» (зеркалить нечего — ни один числовой тип не имеет int-pow) → голая
  дыра, но реализация c нуля (exponentiation-by-squaring + overflow-detect на
  каждом шаге) — НЕ тривиально-механическая. Отчёт владельцу, не реализую в
  этой волне.
- **Display/Debug (`@display`/`@debug`, → `.to_str()`/интерполяция)** —
  explicit `#impl(Display)`/`#impl(Debug)` конкретные тела в protocols.nv
  СУЩЕСТВУЮТ только для `int`, `f64`, `f32`, `bool`, `char`, `str` (~строка
  654-687). Явных тел для i8/i16/i32/i64(отдельно от int)/u8/u16/u32/u64/uint
  НЕТ. Однако ЭМПИРИЧЕСКИ (пробный `.nv`-файл, build+run) подтверждено: интерпол
  яция `"${x}"` и `x.to_str()` для `x i8`/`x u32` РАБОТАЮТ корректно (значения
  печатаются верно), и generic-функция `fn[T Display] show(x T)` компилируется
  и работает и для `i8`, и для `u32`-аргумента — т.е. Display bound
  satisfaction для узких int-типов ГДЕ-ТО есть (не найден явный текст —
  вероятно, компиляторный fallback/структурная проверка через сигнатуру, а
  не текстовый `#impl`). Нужно доследовать источник (либо признать НЕ голом
  ввиду эмпирического подтверждения работы). Промежуточный вывод: скорее
  всего НЕ гол по факту поведения — но нужно решить, нужно ли явно приписать
  `#impl(Display)` тела для узких int (симметрии ради) или оставить как есть
  (работает через какой-то другой механизм, трогать не нужно — не ломать
  molчаливо работающее).
- **try_from / TryFrom конверсии** — `str @to_*` (parse) surface (std/runtime/
  string/parse.nv) — ЖИВОЙ подмножество ТОЛЬКО `to_int`/`to_i64`/`to_u64`/
  `to_u32`/`to_u8`, ЯВНО задокументированное как **owner decision** (Plan
  174.1, "Live set per actual consumers... NOT the full SignedInt/
  UnsignedInt type-set"). `to_i8`/`to_i16`/`to_i32`/`to_u16`/`to_uint`
  ОТСУТСТВУЮТ намеренно (не забыты). Расширение этого сета было бы тихим
  расширением уже принятого owner-решения → в отчёт как «требует решения
  владельца» (пересмотреть Plan 174.1 scoping?), НЕ добираю. Отдельно:
  numeric↔numeric `try_from` (типа `u8.try_from(300u32)`, Rust `TryFrom<T>`
  между целыми) ОТСУТСТВУЕТ вообще как класс — только `as`-cast. Большой
  комбинаторный вопрос дизайна, НЕ трогаю, в отчёт отдельным пунктом.
- **f64.MIN / f32.MIN — НАЙДЕН И ПОЧИНЕН БАГ** (не просто дыра): "MIN" уже в
  generic `is_numeric_const` списке чекера (types/mod.rs:7514), поэтому
  `f64.MIN`/`f32.MIN` проходили type-check молча, но падали на C-компиляции
  (`use of undeclared identifier 'f64_MIN'`) — не было записи в
  `numeric_type_constant_mapping` (emit_c.rs:48769). Добавлено: `f64.MIN =
  -DBL_MAX`, `f32.MIN = -FLT_MAX` (Rust-паритет: MIN = most-negative-finite,
  НЕ путать с уже существующим `MIN_POSITIVE`). Коммит bcc2d4f7e.
- **Display bound для узких int (i8/i16/i32/u8/u16/u32/u64/uint)** —
  эмпирически подтверждено (build+run пробников): интерполяция `"${x}"`,
  `x.to_str()` и generic `fn[T Display] show(x T)` РАБОТАЮТ корректно для
  этих типов, несмотря на отсутствие явного текстового `#impl(Display)` тела
  (в protocols.nv есть только для int/f64/f32/bool/char/str). НЕ гол по
  факту поведения — механизм не текстовый (видимо компиляторный
  interpolation-fallback), трогать не стал (работает — не трогаю).

## Важное уточнение по гейту «nova check std чист» (по запросу координатора)

`nova check std` (полный обход дерева, БЕЗ доп. флагов) даёт **18 FAIL** —
ЭТО ЖЕ ЧИСЛО воспроизведено на ПОЛНОСТЬЮ откаченных (git checkout --)
файлах, т.е. это **pre-existing базовый шум на HEAD b2bfa0505, НЕ внесён
этой волной**:
- 11 из 18 — `*_neg/*.nv` фикстуры (encoding/serde_neg ×7, fs/neg ×3,
  io/neg ×2, net/neg ×3, time/civil/neg ×2 = 17 фикстур сгруппированы в счёте
  как отдельные файлы) — они ПРЕДНАЗНАЧЕНЫ падать тайпчек (негативные тесты);
  `nova check` не понимает `EXPECT_ERROR`-семантику (это знает только `nova
  test`) — соответственно они ожидаемо красные под голым `check`, это НЕ шум
  корректности.
- 1 из 18 — `std/src/prelude/protocols.nv` — падает с `[E_UNKNOWN_METHOD]`
  на `int.min`/`int.to_char`/`str.bytes` (методы, которые ФАКТИЧЕСКИ
  существуют в std/runtime/*). Воспроизведено ТАКЖЕ на узком срезе `nova
  check std/src/prelude` (изолированно) — то же самое. `--include-runtime`
  флаг НЕ чинит (протестировано отдельно, тот же результат). Причина —
  вероятно `std.prelude.*` файлы имеют auto-import глобального prelude
  ОТКЛЮЧЁННЫМ (cycle protection, protocols.nv:13-18 коммент), и `nova check`
  в per-file/per-module режиме не собирает тот же полный merge-граф импортов,
  что `nova build`/`nova test`/`spec_tests/conformance` (single-CU). Тот же
  класс проблемы уже задокументирован рядом (std/src/math/
  overflow_policy_test.nv:11-17 — «файлы внутри std.prelude.* ломают
  assert()-инфраструктуру для ЛЮБОГО теста в этом namespace»).
- **Узкие срезы ЧИСТЫЕ**: `nova check std/src/runtime` -> `PASS: 17 FAIL: 0`
  (подтверждает наблюдение координатора «на моём смоуке std/src/runtime было
  чисто»). `nova check std/src/time` (см. ниже) — тоже чистый срез.
- **Вывод**: буквальный «`nova check std` (голая команда) = 0 FAIL» уже был
  НЕДОСТИЖИМ до этой волны (не моя регрессия) — использую как приёмочный
  критерий «мои правки НЕ добавили новых FAIL к уже известному набору из 18»
  (проверено diff'ом набора имён файлов до/после — идентичен) + чистые узкие
  срезы (runtime/time/prelude сам-по-себе минус тот один pre-existing).

## Следующие шаги (план)
1. ~~Доследовать механизм Display-bound-satisfaction для узких int~~ — готово
   (эмпирика, не гол).
2. ~~Проверить try_from/parse семейство по всем типам~~ — готово (Plan 174.1
   owner-scoped, не трогаю; numeric↔numeric try_from отсутствует как класс,
   в отчёт).
3. ~~Добрать: f32 @clamp; SignedInt @signum/@is_negative/@is_positive~~ —
   готово (см. коммиты).
3b. ~~f64.MIN/f32.MIN codegen-баг~~ — готово (коммит bcc2d4f7e).
4. Тесты рядом с модулем на добранное — В РАБОТЕ.
5. `nova test std/src/time` — подтвердить зелёную талли, обновить статус
   Пункта 10/200 (факт прогона).
6. Приёмочные гейты: lint --deny std, standalone-CU 69/0, nova check
   (узкие срезы, см. выше), strict-effects.
