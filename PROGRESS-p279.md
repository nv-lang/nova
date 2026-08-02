# PROGRESS — окно p279

Задание: №279 [M-nested-err-pattern-shared-variant-wrong-enum-tag] — вложенные
`Err(Variant)`-паттерны при ОБЩИХ именах вариантов матчат теги ЧУЖОГО enum'а.

Модель: Claude Sonnet 5. Ветка: `p279` (база main c76157f6a).

## Диагноз

Корень (пофайлово):
- `compiler-codegen/src/codegen/emit_c.rs::pattern_cond` (Pattern::Variant арм,
  ~50654): для НЕСТЕД bare-варианта внутри `Err(..)`/`Ok(..)`/общего sum-варианта
  (например `Err(OnlySign)`) payload-поле НЕ регистрировалось в `var_types`
  (регистрация была только для Option/mono-Result спецслучаев) → `scr_ty` пуст →
  падение в `sum_schema_registry.find_variant_compat(&variant_name)` —
  first-registered-wins, независимо от типа скрутини.

Побочно вскрыт и закрыт СМЕЖНЫЙ, ранее незарегистрированный дефект (№280,
`[M-178-variant-ctor-target-sum]`, маркер существовал в коде БЕЗ номера,
revert-note у `expected_sum_hint`): та же болезнь в КОНСТРУКТОР-позиции
(`return Err(V)` / bare unit-вариант напрямую как аргумент `Err(..)`/`Ok(..)`) —
`debt_find_variant_ctx`'s `debt_current_fn_return_sum`-шаг возвращает `None` для
ЛЮБОГО Result/Option-wrapped return (mono-имя содержит "____"), падение в тот же
`find_variant_compat` first-wins. Оба бага делили ОДИН фоллбэк и взаимно
маскировали друг друга (мега-CU тест `neg/parse_int_overflow_err` держался на
СОВПАДЕНИИ двух независимых ошибок) — фикс паттерн-стороны (№279) в одиночку
раскрыл конструктор-сторону (№280), пришлось чинить ОБЕ той же волной
(zero-tolerance-bugs).

## Фикс (чекер-канал, find_variant-фоллбэк НЕ расширялся)

`compiler-codegen/src/types/mod.rs`:
- `TypeCheckCtx::pattern_variant_types_buf: RefCell<HashMap<Span, String>>` —
  новый write-буфер (мирроит `resolved_types_buf`).
- `resolve_pattern_variant_types(&self, pattern, scrutinee_ty)` — рекурсивно
  резолвит КАЖДЫЙ bare (single-segment) `Pattern::Variant` в паттерне против
  СТРУКТУРНОГО типа скрутини (Option[T]/Result[T,E]/generic-sum tuple-variant
  payload позиции, рекурсивно), пишет `pattern.span → sum_name`. Только когда
  однозначно резолвится — иначе не пишет (defensive, fallback не трогается).
- Вызовы добавлены в 5 местах f1_expr: `Match`-arm, `IfLet`, `WhileLet`,
  `ParallelFor`, `For`.
- `ModuleEnv.pattern_variant_types: HashMap<Span, String>` — новое поле
  (derive(Default)), материализуется из буфера рядом с `resolved_types`.

`compiler-codegen/src/codegen/emit_c.rs`:
- `CEmitter.pattern_variant_types: HashMap<Span, String>` + `set_pattern_
  variant_types` — новый канал, читается в `pattern_cond` ПЕРЕД
  `find_variant_compat`-фоллбэком (приоритет: C-derived scr_ty → канал →
  legacy first-wins).
- №280: приоритетное чтение `resolved_types[expr.id]` в `ExprKind::Ident`
  bare-unit-variant ctor ветке ПЕРЕД `debt_find_variant_ctx` (чекер уже
  резолвил корректно через `materialize_literal_coercion`'s Err/Ok-арм —
  codegen просто не читал канал для этой позиции, читал только для `None`).

`compiler-codegen/src/main.rs` + `test_runner.rs`: подключён
`emitter.set_pattern_variant_types(&module_env.pattern_variant_types)`.

## Верификация

- Изолированный репро `scratch38/p279/repro.nv` (два ЛОКАЛЬНЫХ enum, общее имя
  варианта `OnlySign`, nested `Err(Variant)`): RED без фикса (2/4 FAIL) →
  GREEN с фиксом (4/4 PASS). Подтверждено на пересобранных бинарях в обе
  стороны (checkout patch revert → red; git apply patch → green).
- Изолированный репро №280 `scratch38/p279b/overflow_repro.nv` (минимальный
  std-импорт `ParseIntError`): RED без фикса → GREEN с фиксом.
- Мега-CU `spec_tests/conformance --positive --compile-error` (канон.
  invocation, `scripts/gate.sh`): **PASS: 624  FAIL: 0  SKIP: 68** (включает
  новую фикстуру `m279_nested_err_shared_variant_pos.nv`, 4 теста).
- `nova check std/src`: **PASS: 147  FAIL: 26  WARN: 60** — байт-в-байт канон,
  не сдвинулся.
- `arch-ratchet.sh`: `lines=64389 <= 64389` (baseline поднят с 64384, +5, ПУТЬ
  B, обоснование в `scripts/guards/arch-ratchet.baseline`), `infer=348 <= 349`
  (не вырос).
- Флагман `examples/flagship/aggregator --strict-effects`: КРАСНЫЙ, но
  ИДЕНТИЧНО на baseline-компиляторе (main-репа, БЕЗ моего фикса) —
  `TlsStream.read_bytes` отсутствует (дрейф пакета polaris/nova-tls, НЕ
  связано с #279/№280, не регрессия этого окна).
- Носитель `nova-bigint-p240` (env: `NOVA_STD_PATH`/`NOVA_GC_LIB_DIR`/
  `NOVA_CG_INCLUDE`/`NOVA_RT_DIR` → main-репа, БЕЗ коммитов в ту репу):
  - `src/repro_parse_test.nv`: **PASS: 1  FAIL: 0** (было: симптом #279 —
    подтверждено RED на baseline-компиляторе, тот же файл).
  - `src/bigrat_test.nv`: было 2 FAIL на baseline (`str @to_bigrat — ошибки
    парсинга` = симптом #279; `@to_str roundtrip` = НЕСВЯЗАННЫЙ pre-existing
    баг, gcd-редукция тестовых чисел). С фиксом: симптомный тест ЗЕЛЁНЫЙ,
    остаётся ТОЛЬКО несвязанный `@to_str roundtrip` (не в объёме этого окна).
- Стражи: `check-marker-registry-sync.sh` ok (0<=0), `check-bug-number-sync.sh`
  ok, `check-doc-hygiene.sh` ok.

## Реестры

- `docs/plans/221.1-bug-sweep.md`: №279 → ЗАКРЫТ; №280 (новая запись,
  `[M-178-variant-ctor-target-sum]`) заведён и ЗАКРЫТ той же волной.

## Не в объёме этого окна

- `bigrat_test.nv`'s `@to_str roundtrip` (line 330, несвязанный gcd-баг) —
  отдельный дефект, не заведён под номером (не диагностировался глубоко,
  вне периметра #279/№280).
- Флагман-дрейф `TlsStream.read_bytes` (polaris/nova-tls) — не мой периметр,
  подтверждено как pre-existing на baseline.
