<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# cmpxchg-волна B — cas-lint progress (2026-07-16, sonnet, ветка `p207-cas-lint`)

**Статус: ✅ ЗАКРЫТО этой волной.** compare_exchange/compare_exchange_weak
failure-ordering диагностика (hard-error + warning) на call-сайтах реализована,
отлажена, верифицирована точечными фикстурами (`nova check`, synchronous, узкий
scope). Финальный коммит — см. хэши в отчёте волны.

## Задача

1. Hard-error `E_CAS_FAILURE_ORDER_INVALID` — `failure ∈ {Release, AcqRel}` на литеральном
   `compare_exchange`/`compare_exchange_weak`.
2. Warning `W_CAS_FAILURE_STRONGER` — `strength(failure) > strength(success)` (тотальный
   порядок `Relaxed < Acquire≈Release < AcqRel < SeqCst`).
3. Только литеральные orderings; не-литералы (переменные) не диагностируются.
4. D425-амендмент в спеке тем же слиянием.

## Сайты диагностики

- **Hard-error**: `compiler-codegen/src/types/mod.rs::check_cas_ordering` (в
  `impl<'a> BoundCtx<'a>`), вызывается из `BoundCtx::walk_expr` на каждый `ExprKind::Call`
  (тот же реальный, работающий pipeline что `check_call_bounds`/`check_call_argbind`).
  Существующий (но МЁРТВЫЙ — `#[allow(dead_code)]`, ноль вызовов в дереве) прецедент
  `check_atomic_load_ordering`/`check_atomic_store_ordering` (Plan 103.1 Ф.4, ~L32488) НЕ
  был реальной рабочей инфраструктурой — не переиспользован как есть, только формат
  сообщения/код взят за образец.
- **Warning**: `compiler-codegen/src/lints.rs::lint_cas_failure_stronger`, вызывается из
  `walk_expr_lints`'s `ExprKind::Call`-ветки → `lint_module` → **unconditional** pipeline
  (main.rs шаг 5, каждый `nova build`/`check`/`test`) — в отличие от `CONV_RULES`/`nova lint`
  (main.rs шаг 6, opt-in только через `--lint`). Чисто AST-уровня (нет `self.sig`/
  `infer_expr_type` в этом файле) — receiver гейтится ТОЛЬКО по имени метода (обоснование в
  doc-комментарии: `compare_exchange(expected,desired,success,failure)` уникальна для
  `Atomic*`-семейства во всём языке — false-positive risk нулевой).

## Инфраструктурные правки (общие для обоих)

- `mem_ordering_variant` (types/mod.rs, теперь `pub(crate)`) расширен: раньше ловил ТОЛЬКО
  `MemOrdering.X` (`ExprKind::Path`); реальный код (`sync_test.nv`,
  `spec_tests/conformance/plan103_2_*`) почти everywhere пишет orderings БЕЗ префикса
  (`SeqCst`, `Acquire`, ...) — bare `ExprKind::Ident` sugar. Добавлена bare-Ident ветка
  (gated на фиксированный 5-элементный набор имён вариантов).
  Добавлен `mem_ordering_strength` (тот же total-order), тоже `pub(crate)`.
- `E_CAS_FAILURE_ORDER_INVALID` const рядом с `E_INVALID_ORDERING_LOAD/STORE`.
- `Self::infer_arg_ty` (BoundCtx static helper, НЕ полный type-inference — тот в ДРУГОМ
  struct'е `TypeCheckCtx`) расширен узкой веткой `Atomic*.new(...)` → `Self`. **Debug-
  верифицировано** (temp `eprintln!`, снято перед коммитом): `AtomicI64.new(0)` парсится
  как `ExprKind::Path(["AtomicI64", "new"])`, НЕ `Member{obj:Ident,name}` — компилятор
  распознаёт `AtomicI64` как known-type-name на этапе парсинга. Ветка матчит ОБА shape'а
  (`Path` — реальный; `Member` — defensive fallback). Именно эта находка была причиной
  первого NEG-NO-ERROR (receiver-тип не резолвился → gate тихо skip'ал).
  Специально НЕ обобщено на произвольный `Type.new()` — узкий `Atomic`-префикс, чтобы не
  расширять blast radius bound-checker'а на несвязанные call-сайты.
- Обе функции обрабатывают **default-параметры** (Plan 207 cmpxchg-rename схлопнул
  overloads в ОДНУ сигнатуру `success MemOrdering = MemOrdering.SeqCst, failure MemOrdering
  = MemOrdering.SeqCst`, влито в main ПОСЛЕ первой версии этого чекера, см. ниже): опущенный
  `success`/`failure` (arity < 4, или named-only на другом параметре) — известный литерал
  `SeqCst`, участвует в обеих проверках наравне с explicit литералом, НЕ молча пропускается.

## D425-амендмент

`spec/decisions/06-concurrency.md`, секция «Амендмент (cmpxchg-lint волна B, 2026-07-16)» —
сразу после «### Границы» исходного D425, перед разделителем в D426 (слияние-конфликт
разрешён вручную, обе секции сохранены). Плюс отдельное «Замечание (после merge с D426)»
про default-параметры и D102 keyword-only требование.

## Фикстуры

- `spec_tests/conformance/neg/plan207_cas_failure_order_release_neg.nv` — hard-error,
  `failure: MemOrdering.Release`.
- `.../plan207_cas_failure_order_acqrel_neg.nv` — hard-error, `compare_exchange_weak`,
  `failure: AcqRel` (bare-variant написание).
- `.../plan207_cas_failure_stronger_warn.nv` — 2 теста: (1) explicit
  `success: Relaxed, failure: SeqCst`; (2) `failure` ПОЛНОСТЬЮ опущен (default SeqCst)
  против explicit `success: Relaxed` — оба должны дать `W_CAS_FAILURE_STRONGER`.
- `spec_tests/conformance/plan207_cas_ordering_legal_pairs_pos.nv` — 6 тестов: три пары
  из амендмента (SeqCst/SeqCst, AcqRel/Acquire, Release/Relaxed) + bare-variant + 2
  omitted-param сценария (`success:` без `failure:` и наоборот) — ни один не диагностируется.

Все фикстуры используют **именованные** `success:`/`failure:` (см. находку ниже) —
позиционная форма для этих двух параметров теперь compile error (D102), независимо от
моей диагностики.

## НАХОДКА (не мой баг, но блокировал верификацию) — существующие тесты сломаны D102 после merge p207-cmpxchg-rename

`std/src/runtime/sync_test.nv` (строки ~73/77) и
`spec_tests/conformance/plan103_2_atomic_{explicit_ordering,bool_expanded}.nv` вызывали
`compare_exchange`/`_weak` с `success`/`failure` **позиционно** (`a.compare_exchange(1, 2,
SeqCst, Acquire)`), что было легально при СТАРОЙ 2-overload-схеме. После
p207-cmpxchg-rename (`69c7be2e9`, слито в main ДО того как я это увидел — worktree был
создан от старого main) `success`/`failure` стали default-параметрами, а компилятор
энфорсит правило D102 «параметры с default — keyword-only»: позиционная передача теперь
`CODEGEN-FAIL` (`параметр success имеет значение по умолчанию — передаётся только по
имени`). Это ЧУЖОЙ регресс (не моя диагностика, не мой код), но он блокировал ЛЮБУЮ
попытку верифицировать «существующие тесты не ловят мой warning», т.к. эти 3 файла вообще
не компилировались. Мехническая правка (positional → `success:`/`failure:` named form,
БЕЗ изменения семантики/значений) внесена в РАМКАХ этой волны — тривиальный дрейв-бай фикс,
разблокировавший верификацию; не относится к cas-lint диагностике по существу.
Полный грep `\.compare_exchange(_weak)?\(` по `*.nv` — 15 файлов; 8 из оставшихся используют
ТОЛЬКО 2-арную форму (expected, desired) — не задеты D102 вообще; examples/ — 0 хитов.

## Верификация (синхронная, точечная — по указанию координатора)

`nova test` (dev-mode, `--jobs 16`) на: 2 neg + 1 warn(×2 теста) + 1 pos(×6 тестов)
фикстуры + 3 починенных pre-existing файла (`sync_test.nv`,
`plan103_2_atomic_explicit_ordering.nv`, `plan103_2_atomic_bool_expanded.nv`) — команда
и полный результат сверяются перед финальным коммитом (см. отчёт).

## Координация (owner, во время работы)

- Параллельная ветка `p207-cmpxchg-rename` **слита в main** (`69c7be2e9`) ДО того, как я
  синканулся — worktree был создан от старого main (до слияния). Смёржил актуальный main
  в `p207-cas-lint` (merge-commit, 1 конфликт в `06-concurrency.md`, разрешён вручную —
  обе секции D425-амендмент + D426 сохранены).
- Моё распознавание — prefix-based `Atomic*`, НЕ завязано на конкретные имена типов —
  survived rename (AtomicIsize→AtomicInt, AtomicUsize→AtomicUint, AtomicPtr снят) без
  единой правки логики распознавания.
- Владелец убил фоновые тест-прогоны (не будят агента) — весь дальнейший прогон
  синхронный, timeout ≤600с, узкий scope (свои фикстуры + затронутые существующие файлы),
  без полных conformance-прогонов.
