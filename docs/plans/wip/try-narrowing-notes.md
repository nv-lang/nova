# [M-numeric-try-narrowing] — чекпоинт (2026-07-20)

Worktree: `d:/Sources/nv-lang/nova-tryfrom`, ветка `p-try-narrowing`. Не влито
в main, push не делался (по заданию). Модель sonnet.

## Статус: РЕАЛИЗОВАНО, гейты зелёные, готово к слиянию интегратором.

## Файлы

- `std/src/prelude/errors.nv` — `export type RangeError` (unit-тип, новая
  секция после `TryFromCharError`).
- `std/src/prelude/protocols.nv` — `import std.prelude.errors.{RangeError}`
  (после существующего `fmt_buf` import) + 10 бланкетов
  `fn[S Ints] S @try_to_i8()`/`@try_to_i16()`/`@try_to_i32()`/`@try_to_i64()`/
  `@try_to_int()`/`@try_to_u8()`/`@try_to_u16()`/`@try_to_u32()`/`@try_to_u64()`/
  `@try_to_uint()` — новая секция в конце файла (после `@wrapping_pow`).
- `std/src/prelude.nv` — `RangeError` добавлен в facade re-export строку
  (`export import std.prelude.errors.{...}` — там же, где `RuntimeError` и
  т.д.), `PRELUDE_VERSION` 18→19 + новая doc-запись.
- `std/src/math/try_narrowing_test.nv` — новый файл, 11 test-блоков.
- `spec/decisions/04-effects.md` — новый блок **D430** (после D427, конец
  файла).
- `spec/decisions/README.md` — строка D430 в таблице (после D429).
- `docs/plans/backlog-followups.md` — `[M-numeric-try-narrowing]` помечен
  ✅ RESOLVED.

## Design-решения (см. D430 для полного обоснования)

1. **10 бланкетов, не 20 и не 100.** Один `fn[S Ints] S @try_to_<T>()` на
   целевой тип `<T>`, единое тело для signed+unsigned источника через
   `if @ < 0 {...} else {...}` (sign-agnostic, форма `@saturating_pow`).
   Рассматривал split `SignedInts`/`UnsignedInts` на одно имя (20
   бланкетов) — отклонил: нет прецедента двух одноимённых бланкетов над
   непересекающимися type-set в файле, риск неизвестного поведения
   резолвера не стоил экономии кода.
2. **Soundness: расширение `@` ВВЕРХ (`i64`/`u64`), не сужение границы цели
   ВНИЗ.** Наивный `(Ti.MAX as S)` ломается, когда `Ti` шире `S`
   (`u32.MAX as i32 == -1` — реальный баг, найден и исправлен ДО коммита
   при проектировании, не в проде).
3. **Полная матрица 10×10 (100 пар)**, не только «настоящие» сужения —
   Nova type-sets не имеют width-based исключения; тот же выбор, что Rust
   `TryFrom` (widening-имплы существуют, просто никогда не Err).
4. **`RangeError` — новый unit-тип**, не переиспользован `ParseIntError`
   (str-parse-специфичные варианты `Empty`/`InvalidDigit`/`InvalidRadix`
   не имеют смысла для число→число).

## Найденный (НЕ починенный, вне зоны) разрыв

`for v in vec { v.try_to_u8() }` — receiver `v` из for-bound, даже
присвоенный typed `ro`-локали ПОСЛЕ вызова, всё равно падает в
`[P67-LEGACY] method call return type unknown` (emit_c.rs). НЕ specific
для `try_to_*` — общий generic-type-set-bound-blanket-dispatch разрыв
(тот же класс, что уже задокументирован в заголовке
`overflow_policy_test.nv` про inline-Call-receiver). Обошёл в тестах:
элементы Vec читаются по индексу в typed `ro`-локаль ПЕРЕД вызовом
`.try_to_*()` (`ro v0 i32 = fits[0]`), не через `for`-цикл. Отдельный
followup-маркер НЕ заводил — покрыт существующим `[P67-LEGACY]`
(D423/D427 «Известные разрывы»).

## external_registry.rs — ПРОВЕРЕНО, touch НЕ нужен

Бриф предупреждал «protocols.nv — ВХОДИТ в external_registry snapshot».
Эмпирически перепроверил: `compiler-codegen/src/codegen/external_registry.rs`
эмбеддит через `include_str!` ТОЛЬКО string_builder/write_buffer/
read_buffer/char/sync/raw_mem/net-addr-tcp-udp/gc/fibers/vclock/runtime/
bench/numeric — `protocols.nv` и `errors.nv` там НЕТ (grep подтверждён,
файл полностью прочитан). Эти два файла резолвятся обычным
manifest-based import graph, не bootstrap-embedded registry. Не трогал.

## Гейты (все прогнаны на СВОЁМ бинаре в worktree, env NOVA_GC_LIB_DIR/
INCLUDE_DIR → main repo vcpkg, libuv скопирован из main + .git снят)

- `nova build` (nova-cli release, 2m45s) — чисто, 0 errors.
- `nova test std/src/math` — **PASS: 4 FAIL: 0** (includes новый
  `try_narrowing_test`).
- `nova build examples/flagship/aggregator/src/main.nv --strict-effects`
  — **built OK** (38.97s), только warnings (pre-existing, не мои файлы).
- `nova lint --deny std` — **5 находок, ВСЕ в нетронутых файлах**
  (`fmt_buf.nv` ×3, `string_builder.nv` ×1, `write_buffer.nv` ×1 —
  известный `[M-p200-17-remaining-3]`), **0 новых от моих файлов**.
- `nova test nova_tests/modules nova_tests/plan107 nova_tests/plan62
  std/src/runtime/string` — PASS: 29 FAIL: 2. Оба FAIL
  (`folder_per_file_imports_use` CC-FAIL undefined symbol
  `nova_fn_sum_range`; `plan62/neg/prelude_shadow_warning` NEG-WRONG-WARN)
  **подтверждены pre-existing**: тот же прогон на `git checkout HEAD --
  std/src/prelude.nv std/src/prelude/errors.nv std/src/prelude/protocols.nv`
  (мои правки временно убраны, потом восстановлены из backup +
  переприменён PRELUDE_VERSION-эдит) даёт ИДЕНТИЧНЫЕ 2 FAIL — не мои
  регрессии.
- `nova check std` (вспомогательный, НЕ часть заданного гейт-списка) —
  19 FAIL, 17 из них — intentional `neg/`-директории (plain `nova check`
  не знает EXPECT_*-маркеры), 2 (`protocols.nv`-attribution →
  `fmt_buf.nv`, `fmt_buf.nv`-attribution → `string_builder.nv`) —
  pre-existing, задокументированы в git log HEAD-коммита
  (`[M-p200-17-remaining-3]` "base64/fmt_buf/handlers"). Не гейт per
  заданию — использовал только для sanity, не для решения.
- Мега-CU (`spec_tests/conformance` единым CU) — **НЕ гонял** (по заданию).
