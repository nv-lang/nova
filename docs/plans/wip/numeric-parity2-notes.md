# Числовой паритет-2 — добор дыр (волна 2026-07-20)

Контекст: владелец 2026-07-20 — «закрывать» 4 из 5 оставшихся пунктов
аудита `docs/plans/wip/numeric-parity-notes.md` (try_from ОТЛОЖЕН, не
трогать). Worktree `nova-parity2`, ветка `p-numeric-parity2`. Модель: sonnet.

Промежуточный чекпоинт после сбоя VSCode-сессии (checkpoint-коммит
89753a81c) — реализация сделана, приёмочные гейты прогоняются сейчас.

## Реализовано (коммит 89753a81c)

1. **abs (SignedInt-бланкет, protocols.nv)** — `int @abs()` был единственным
   существующим (`extern "nova"` -> C `llabs`, UB на `LLONG_MIN`). Retracted
   целиком (runtime_registry.rs RuntimeFn-запись убрана, emit_c.rs
   `int_method_to_c` функция + её emit_call-перехватывающий блок для
   `nova_int` убраны целиком — иначе новый `.nv`-бланкет никогда не
   достигался бы, т.к. хардкод-блок стоял ПЕРЕД обычным method-dispatch).
   Заменён `fn[T SignedInt] T @abs() -> T => if @ < 0 { -@ } else { @ }` —
   трапит на `T.MIN` через УЖЕ существующий unary-negate trap guard (D427
   §R2, "negation overflow", landed до этой волны) — НЕ новая
   overflow-политика, прямое следствие уже действующего trap-дефолта
   `+`/`-`/`*`/unary-`-` (D423 §R3/D427 §R2). `checked_abs`/`saturating_abs`/
   `wrapping_abs` — та же тройка политик, что `checked_neg`/`saturating_neg`/
   `wrapping_neg`. Rust-паритет: `.abs()`-семья существует ТОЛЬКО для signed
   (unsigned не входит — как и в Rust). `std/runtime/math.nv` регенерирован
   через `nova-codegen emit-runtime-stubs` (убрана строка `int @abs()`).

2. **pow/checked_pow/saturating_pow/wrapping_pow (Ints-бланкет,
   protocols.nv)** — раньше не было вообще ни в каком виде. `exp u32`
   (Rust-паритет: `Self::pow(self, exp: u32) -> Self`, отрицательных
   степеней у целых нет). Exponentiation-by-squaring, тот же алгоритм, что
   Rust core `checked_pow` (squaring ТОЛЬКО пока экспонента не исчерпана —
   не квадрирует лишний раз на последнем шаге). `pow` — трапит на overflow
   через СЫРОЙ `*` (D423 §R3 trap-дефолт, тот же путь что `+`/`-`/`*` —
   никакой новой политики). `checked_pow`/`saturating_pow`/`wrapping_pow` —
   вызывают компиляторный интринсик `@overflowing_mul` НАПРЯМУЮ на локалях
   (`acc`/`base`, ТИПА `T`, приём — как `zero.overflowing_sub(rhs)` в
   `checked_div` выше), НЕ вызывают `.nv`-бланкет `@checked_mul`/
   `@wrapping_mul` изнутри — nested-.nv-blanket-call ICE задокументирован
   D427 §R4.3 (`[P67-LEGACY]`). `saturating_pow` — направление
   насыщения зависит ТОЛЬКО от знака ИСХОДНОГО `@` и чётности `exp`
   (истинный мат. знак результата), тот же однократный post-hoc выбор, что
   Rust `saturating_pow`; для unsigned `@ < 0` константно `false` ->
   формула автосхлопывается в «всегда MAX».

3. **str.to_i8/to_i16/to_i32/to_u16/to_uint (parse.nv)** — добор остатка
   Plan 174.1 live-set (`to_int`/`to_i64`/`to_u64`/`to_u32`/`to_u8` уже
   были) по решению владельца («закрывать»). Тот же движок
   (`@parse_int_core`/`@parse_uint_core`) + range-check-после-разбора
   паттерн, что `to_u32`/`to_u8`. `to_uint` — без range-check (сама ширина
   движка `@parse_uint_core`, мирроит `to_int`).

4. **saturating_neg (SignedInt-бланкет, protocols.nv)** — Rust даёт
   `saturating_neg` ТОЛЬКО signed-типам (в отличие от `checked_neg`/
   `wrapping_neg`, которые остаются `Ints`-бланкетами — Rust даёт их обоим
   знакам). `@ == T.MIN -> T.MAX`, иначе `-@`.

НЕ ТРОНУТО: `try_from` (отложен владельцем), numeric↔numeric конверсии.

## Тесты (коммит 89753a81c)

- `std/src/math/overflow_policy_test.nv` — добавлены test-блоки: abs
  (обычные значения + 2 panics-пина на int.MIN/i32.MIN), checked_abs/
  saturating_abs/wrapping_abs (MIN-edge + обычные), saturating_neg
  (MIN-edge + обычные), pow (0/1/обычные + panics-пин на i32 2^31),
  checked_pow (None/Some edge, включая u8 base-squaring overflow),
  saturating_pow (MAX/MIN направление по знаку+чётности, i8/-3^5 -> MIN
  кейс), wrapping_pow (модульно, u8 2^8->0, i32 2^31->i32.MIN).
- `std/src/runtime/string_test.nv` (НОВЫЙ файл, module `runtime.string_test`
  — отдельный peer-модуль, та же форма что `std/time/units_test.nv` для
  folder-модуля `time.duration`; НЕ co-equal файл внутри `#no_prelude`
  `runtime.string`, чтобы не терять `assert()`/prelude) — to_i8/to_i16/
  to_i32/to_u16/to_uint: в диапазоне / overflow -> Err(Overflow) / мусор ->
  Err(InvalidDigit) / to_u16 '-1' -> InvalidDigit (unsigned) / to_uint
  пусто -> Err(Empty) / radix-16 smoke на to_i32/to_u16.

## Rust-сторона (тоже коммит 89753a81c)

- `compiler-codegen/src/codegen/runtime_registry.rs` — `math_runtime()`:
  RuntimeFn-запись `int.abs()` (c_name `llabs`) убрана, заменена
  ретракт-комментарием.
- `compiler-codegen/src/codegen/emit_c.rs` — `int_method_to_c` функция
  ПОЛНОСТЬЮ удалена + её `emit_call`-перехватывающий блок
  (`if obj_ty == "nova_int" { ... }`) удалён (иначе `.nv`-бланкет `@abs()`
  для `int` никогда не был бы достигнут — хардкод стоял раньше обычного
  method-dispatch). Обновлены 2 доккомментария (`primitive_instance_method_
  known`, B11aa-заметка), ссылавшиеся на "int_method_to_c остаётся" —
  теперь честно говорят "убран".
- `std/src/prelude.nv` — обновлён комментарий над `import std.runtime.math`
  (был неточен после ретракта: "codegen emission не меняется" — теперь
  неверно для `abs`).
- `std/src/runtime/math.nv` — регенерирован (`nova-codegen
  emit-runtime-stubs --root .`), убрана `export extern "nova" fn int @abs()`.

Rust-сборка (`cargo build --release` для `nova-cli` И отдельно
`compiler-codegen --bin nova-codegen`) прошла чисто (только pre-existing
warnings, 0 errors).

## Окружение worktree (после сбоя VSCode)

`nova-parity2` не содержал `compiler-codegen/nova_rt/libuv` (submodule) —
скопирован из main (`cp -r`, `.git` внутри удалён) + `target/libuv-cache`
скопирован для ускорения линковки. `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`
указывают на main repo `vcpkg_installed` (per project-worktree-nova-test-
setup.md).

## Приёмка — ЗАВЕРШЕНО

Rust-сборка (`cargo build --release`, `nova-cli` И отдельно `compiler-codegen
--bin nova-codegen`) — чисто, 0 errors (только pre-existing warnings).
`nova-codegen emit-runtime-stubs --root .` перегенерировал
`std/src/runtime/math.nv` (убрана строка `int @abs()`).

| Гейт | Команда | Результат |
|---|---|---|
| Новые тесты (math) | `nova test std/src/math/overflow_policy_test.nv` | PASS: 1 (весь файл, все новые test-блоки abs/checked_abs/saturating_abs/wrapping_abs/saturating_neg/pow/checked_pow/saturating_pow/wrapping_pow, включая 3 panics-пина) |
| Новые тесты (string) | `nova test std/src/runtime/string_test.nv` (НОВЫЙ файл) | PASS: 1 (to_i8/to_i16/to_i32/to_u16/to_uint — диапазон/overflow/мусор/radix) |
| `nova test std/src/math` | вся папка | PASS: 3  FAIL: 0  SKIP: 2 (skip = statistics.nv/complex.nv, без test-блоков, компилируются ОК — pre-existing) |
| `nova test std/src/runtime` (string-parse срез) | `std/src/runtime/string` (folder-module parse.nv+siblings) | PASS: 0 FAIL: 0 (сам модуль без test-блоков, компилируется ОК) |
| | `std/src/runtime/string_test.nv`, `char_test.nv`, `string_builder_test.nv` | все PASS: 1 FAIL: 0 |
| | **ИСКЛЮЧЕНО из bare-folder прогона**: `std/src/runtime/sync_test.nv` — `nova: internal error … [P67-LEGACY] Ident \`guard\` not in var_types` — **ПОДТВЕРЖДЕНО pre-existing** (воспроизведено 1:1 на main HEAD c0294b4ab, ЧУЖИМ неизменённым бинарём/деревом, файл sync_test.nv мной НЕ трогался, к числовому паритету отношения не имеет) |
| `nova lint --deny std` | | 2 finding(s) (`string_builder.nv:145`, `write_buffer.nv:119`, `W_MANUAL_SLICE_COPY`) — **идентичны** findings на main HEAD (проверено тем же lint на неизменённом main-дереве, 250 файлов там vs 251 здесь — разница ровно +1 = новый `string_test.nv`, 0 новых findings от этой волны) |
| standalone-CU | `nova test spec_tests/conformance/standalone --jobs 4` | PASS: 68  FAIL: 0 |
| `nova check --strict-effects std/src/math` | | PASS: 5  FAIL: 0  WARN: 12 (все WARN — pre-existing `unused import Vec`, не новые) |
| Sanity: `nova check std/src/prelude` (protocols.nv-правки) | | PASS: 9 FAIL: 1 — **тот же** pre-existing изолированный gap (`str.bytes`/`int.min`/`int.to_char` — cycle-protection в `std.prelude.*`, задокументирован в numeric-parity-1 notes), grep по выводу на `abs\|pow\|saturating_neg\|checked_abs\|wrapping_abs` — 0 совпадений, новых ошибок от добавленных методов нет |

Вывод: ни один гейт не показывает регрессию от этой волны. Единственный
живой FAIL/пре-existing шум — задокументирован и подтверждён идентичным на
main HEAD (не моя регрессия).

## Итог — все 4 пункта закрыты, try_from отложен

1. abs/checked_abs/saturating_abs/wrapping_abs — готово (SignedInt, trap на
   MIN через D427 §R2, retract старого llabs-UB extern).
2. pow/checked_pow/saturating_pow/wrapping_pow — готово (Ints, exp u32,
   exponentiation-by-squaring, trap через D423 §R3).
3. str.to_i8/to_i16/to_i32/to_u16/to_uint — готово (parse.nv).
4. saturating_neg — готово (SignedInt).
5. try_from — ОТЛОЖЕН (владелец не понял пункт при постановке волны,
   обсудить отдельно) — НЕ тронут вообще, ни numeric↔numeric, ни
   str-parse-уровня.

Коммит: `89753a81c` (checkpoint, весь код+тесты) на ветке
`p-numeric-parity2` в worktree `nova-parity2`. Не смёржено в main, не
запушено (по инструкции волны).
