<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# cmpxchg-волна B — cas-lint progress (2026-07-16, sonnet, ветка `p207-cas-lint`)

**Статус: В РАБОТЕ.** compare_exchange/compare_exchange_weak failure-ordering диагностика
(hard-error + warning) на call-сайтах.

## Задача

1. Hard-error `E_CAS_FAILURE_ORDER_INVALID` — `failure ∈ {Release, AcqRel}` на литеральном
   `compare_exchange`/`compare_exchange_weak`.
2. Warning `W_CAS_FAILURE_STRONGER` — `strength(failure) > strength(success)` (тотальный
   порядок `Relaxed < Acquire≈Release < AcqRel < SeqCst`).
3. Только литеральные orderings; не-литералы (переменные) не диагностируются.
4. D425-амендмент в спеке тем же слиянием.

## Сделано

- **Разведка сайта**: `compiler-codegen/src/types/mod.rs` — существующий (НО МЁРТВЫЙ,
  `#[allow(dead_code)]`, ни одного вызова в дереве) прецедент `check_atomic_load_ordering`/
  `check_atomic_store_ordering` (Plan 103.1 Ф.4, ~L32488). Реальный, РАБОТАЮЩИЙ pipeline —
  `BoundCtx::walk_expr` (impl at ~L17336..) — вызывает `check_call_bounds`/
  `check_call_argbind`/`check_protocol_method_call` на каждый `ExprKind::Call`. Добавлен туда
  же `check_cas_ordering` (hard-error, `errors`-sink) + `cas_call_arg` helper.
- Warning — ОТДЕЛЬНЫЙ pipeline: `compiler-codegen/src/lints.rs::lint_module` (unconditional,
  вызывается из `main.rs` на КАЖДОМ `nova build`/`check`/`test`, в отличие от `CONV_RULES`/
  `nova lint`, который opt-in через `--lint` — проверено чтением `main.rs` шаг 5 vs шаг 6).
  Добавлен `lint_cas_failure_stronger` + `cas_lint_arg` helper, вызывается из
  `walk_expr_lints`'s `ExprKind::Call` arm. Чисто AST-уровня (нет `self.sig`/
  `infer_expr_type` в этом файле) — receiver гейтится ТОЛЬКО по имени метода (обоснование в
  doc-комментарии функции: сигнатура `compare_exchange(expected,desired,success,failure)`
  уникальна для `Atomic*`-семейства во всём языке).
- `mem_ordering_variant` (types/mod.rs) расширен: раньше ловил ТОЛЬКО `MemOrdering.X`
  (`ExprKind::Path`); реальный код (`sync_test.nv`, `spec_tests/conformance/plan103_2_*`)
  почти everywhere пишет orderings БЕЗ префикса (`SeqCst`, `Acquire`, ...) — bare
  `ExprKind::Ident` sugar. Добавлена bare-Ident ветка (gated на фиксированный 5-элементный
  набор имён вариантов). Сделан `pub(crate)` — используется из `lints.rs`.
  Добавлен `mem_ordering_strength` (тот же total-order).
- `E_CAS_FAILURE_ORDER_INVALID` const добавлен рядом с `E_INVALID_ORDERING_LOAD/STORE`.
- `Self::infer_arg_ty` (BoundCtx static helper, НЕ полный type-inference — тот живёт в
  ДРУГОМ struct'е `TypeCheckCtx`) расширен узкой веткой `Atomic*.new(...)` → `Self` (нужно,
  чтобы `mut a = AtomicI64.new(0)` дало receiver-тип на следующей строке
  `a.compare_exchange(...)` — БЕЗ этого `check_method_call_bounds`-стиль gate вообще не мог
  бы увидеть receiver, т.к. до этой правки `infer_arg_ty` не резолвил `ExprKind::Call` вовсе).
  Специально НЕ обобщено на произвольный `Type.new()` — узкий `Atomic`-префикс, чтобы не
  расширять blast radius bound-checker'а на несвязанные call-сайты.
- D425-амендмент дописан в `spec/decisions/06-concurrency.md` (секция «Амендмент
  (cmpxchg-lint волна B, 2026-07-16)»).
- Фикстуры: `spec_tests/conformance/neg/plan207_cas_failure_order_release_neg.nv` (failure=
  Release), `.../plan207_cas_failure_order_acqrel_neg.nv` (failure=AcqRel, bare-variant),
  `.../plan207_cas_failure_stronger_warn.nv` (success=Relaxed,failure=SeqCst → W_), pos —
  `spec_tests/conformance/plan207_cas_ordering_legal_pairs_pos.nv` (SeqCst/SeqCst,
  AcqRel/Acquire, Release/Relaxed, bare-variant).
- Аудит существующих call-сайтов (`sync_test.nv` + `spec_tests/conformance/plan103_2_*`):
  ВСЕ explicit-ordering вызовы уже легальны (`SeqCst/SeqCst` ×2, `SeqCst/Acquire` ×3,
  `AcqRel/Acquire` ×1) — НИ ОДНОГО существующего теста не пришлось чинить.

## НАЙДЕНО (в работе) — E_CAS_FAILURE_ORDER_INVALID НЕ СРАБОТАЛ на первом прогоне

`nova test` на `plan207_cas_failure_order_release_neg.nv` (4-арный явный `failure=Release`)
дал `NEG-NO-ERROR ... codegen succeeded` — ошибка НЕ всплыла. Разбираюсь: либо
receiver-type inference (`Atomic*.new()` ветка `infer_arg_ty`) не сработала на этом пути
(Call-shape `AtomicI64.new(0)` может парситься НЕ как `Member{obj:Ident,name}`, а иначе),
либо другая причина в `check_cas_ordering`. Синхронная точечная отладка — следующий шаг.

## Координация (owner, во время работы)

- Параллельная ветка `p207-cmpxchg-rename` **УЖЕ СЛИТА в main** (`69c7be2e9`, до этого чекпоинта
  не увидел — worktree был создан от старого main). Изменения: `AtomicIsize→AtomicInt`,
  `AtomicUsize→AtomicUint`, `AtomicPtr` СНЯТ, `@__cas_raw→@cmpxchg`, overloads схлопнуты в
  ОДНУ сигнатуру с default-параметрами (`success MemOrdering = MemOrdering.SeqCst, failure
  MemOrdering = MemOrdering.SeqCst`) — БОЛЬШЕ НЕТ отдельной 2-арной overload, это
  default-args на единственной 4-парам сигнатуре. `D426` amends `D168`/`D425`.
  План: смёржить актуальный main в `p207-cas-lint`, перепроверить (моё распознавание уже
  prefix-based `Atomic*`, не завязано на конкретные имена — фикстуры используют
  I64/I32/U32/Bool, не затронутые переименованием).
- Владелец убил фоновые тест-прогоны (не будят агента) — переход на точечные
  СИНХРОННЫЕ прогоны (timeout ≤600с) только своих фикстур + существующих atomic-фикстур
  conformance, не полные прогоны.
