<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Backlog — project-wide OPEN followup markers (`[M-…]`)

> **Роль.** Единый **OPEN-view** — что из `[M-…]`-followup'ов **реально открыто** прямо сейчас
> (actionable «что живо»), по всему проекту. Каждая строка указывает свой **home** (план или floating).
>
> **Чем НЕ является.** Это **не** полная история — она в [`docs/simplifications.md`](../simplifications.md)
> (append-only log, ~573 записи). Backlog = только живой OPEN-срез + индекс. Детали plan-bound маркера
> живут в Followups его плана; здесь — индекс с home.
>
> **Lifecycle (для агентов):**
> 1. Новый floating-маркер → **добавить строку сюда** + залогировать в `simplifications.md` (house style).
> 2. Маркер сделан/закрыт → **убрать строку отсюда** (история остаётся в `simplifications.md` + commit). Держим OPEN-view коротким — только живое.
> 3. Маркер дорос до своего плана → перенести в Followups плана (здесь оставить индекс-строку с home).
> 4. Перед работой над смежной подсистемой — **заглянуть сюда**.
>
> Конвенция: [AGENTS.md → Followup markers](../../AGENTS.md).
> **Создан/выверен:** 2026-06-11 (триаж 58 OPEN-tagged → 24 really-open + 34 stale; workflow w33ant6rp).

---

## P1 — Correctness / Safety / Debuggability

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-83.10.4-iso-cancel-startup-race]` | Iso-cancel startup race (supervised(cancel:) первый тест TIMEOUT); 83.10.5 tactical fix неадекватен (~55%), арх. Ф.B не сделана; 3 stress-теста disabled. **Структурный фикс запланирован: Plan 83-go-cmn Ф.5** (gopark unlockf + READY-latch на стабильном SpawnCtxBase). | Plan 83-go-cmn Ф.5 | P1 |
| `[M-83.11-grow-vs-wake-race]` | ✅ **CLOSED 2026-06-11** (Plan 83-go-cmn Ф.1b, commit `e1525d90671`). Структурный фикс: `NovaSchedState` chunked stable-address storage (chunk'и never-realloc → torn-pointer невозможен); GitHub issue #2. Closure: grow_vs_wake_explicit 100/100 + stress_iso_3e 66/66 + semaphore_batch_n 30/30 armed. История — simplifications.md + plan §9.5. | Plan 83-go-cmn Ф.1b | ✅ done |
| `[M-debug-line-directives]` | Нет `#line N "file.nv"` → дебаггер показывает C, не Nova. Только comment-only `/* SRC */`. | Plan 25 G9 → dedicated план | P1 |
| `[M-83-study-go-c-mn]` | Порт рабочего M:N из Go ≤1.4 C-рантайма. **✅ research+8-фаз декомпозиция; ✅ Ф.1a ring-port (deque→runq, clang 103/5); ✅ Ф.1b chunked park-state — закрыл grow-vs-wake.** OPEN до Ф.2-Ф.8 (gopark/nspinning/timer-heap/sysmon/netpoll). | Plan 83-study-go-c-mn | P1 |
| `[M-msvc-bounds-check-stmt-expr]` | Codegen эмитит GNU statement-expression `(*({ __typeof__(arr)... &_a->data[_i]; }))` для bounds-checked индексации (emit_c.rs ~9700/9720/15783/18571) → cl.exe C2059, **MSVC сломан широко** (регрессия после Plan 82 1049/16; bounds-check добавлен Plan 90/131/138). Fix: per-type inline helper `nova_idx_<T>`. Обнаружено в Plan 83-go-cmn (MSVC baseline). | Plan 145 | P1 |
| `[M-tsan-race-detector]` | M:N runtime C под `clang -fsanitize=thread` (Go `-race`) → авто-ловит M:N-гонки. **⚠ БЛОКЕР (2026-06-11): Windows clang НЕ поддерживает TSAN** (`unsupported for x86_64-pc-windows-msvc` — LLVM limitation, TSAN=Linux/macOS; проверено). WSL Ubuntu есть, но без clang/Linux-сборки. **Требует Linux-сборки Nova** (compiler+libuv+Boehm+runtime под Linux clang) — отдельный prerequisite. Дизайн: `--tsan` flag на Linux clang-ветке test_runner + Boehm-suppressions (conservative-скан/SuspendThread тригерят TSAN) + Linux-CI verify. **Gated на Linux-build Nova** → `[M-nova-linux-build]`. | floating (Linux-CI) | P1 |
| `[M-nova-linux-build]` | Prerequisite для TSAN: проверить/наладить сборку+тесты Nova на Linux (cargo + libuv + Boehm + runtime C под Linux clang). Nova разрабатывается на Windows — Linux-сборка не верифицирована (возможны Windows-измы в runtime C). Разблокирует `[M-tsan-race-detector]` + Linux-CI вообще. | floating | P2 |
| `[M-146-growable-stacks]` | Растущие fiber-стеки — снять потолок ~16k одновременных fiber'ов (Plan 82 fixed-8MB). segmented (Boehm-ok, hot-split) vs copying (gated на Plan 144). Research-first. | Plan 146 | P2 |

## P2 — Correctness / Completeness

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-114.4-strict-partition]` | ro-vs-const partition (`E_RO_FOR_CONSTEXPR_PREFER_CONST`) spec-only, нет в checker (обратное `E_CONST_NOT_CONSTEXPR` есть). | plan-114 Followups | P2 |
| `[M-127-consume-escape-path-sensitive]` | Consume-escape analysis всё ещё V1 syntactic; path-sensitive DFG V2 deferred. | plan-127 Followups | P2 |
| `[M-73.1-comprehensive-negatives]` | Returned-view + generic-propagation (D156) consume-binding negatives отсутствуют. | plan-73.1 Followups | P2 |

## P2 — Codegen

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-128.1-array-namedtuple-ro-method]` | `vs[i].ro_method()` на `[]NamedTuple`: pointer-cast в int-слот vs by-value receiver → clang mismatch; gated. | plan-128 Followups | P2 |
| `[M-128.1-nonpure-index-key]` | Side-effecting `arr[next_idx()]` на pointer-ABI receiver вычисляется дважды; hoist-to-temp V2 не сделан. | plan-128 Followups | P2 |
| `[M-138.2-self-in-param]` | `Self` в param-позиции generic-метода (`@append(mut other Self)`) мис-лоуэрит C-тип без receiver-subst → forward-decl≠def; workaround явный `Vec[T]`. | plan-138 Followups | P2 |
| `[M-138.5-right-binding-migration]` | prefix→postfix `*T`: codegen pointee (138.4 G-D) сделан, parser-level migration + `E_POINTER_PREFIX_MODIFIER` — landed 138.5 (verify после `wd3vgeu6l`). | plan-138.5 Followups | P2 |
| `[M-138-range-value]` | Range — reference-record, не value-record; Plan 138 Ф.0.3 migration не сделана. (138.5 трогает range.nv — re-confirm.) | plan-138 Followups | P2 |

## P2 — Perf optimization (escape/Z3-driven; correctness-neutral)

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-opt-auto-scoped-ref]` | Escape-analysis авто pass-value-param-by-ref + return-slot elision (NRVO); обобщить ресивер-`&obj`. | new perf-план (value-types thread) | P2 |
| `[M-opt-value-sum-types]` | Compiler-inferred value(stack)/heap для sum-типов (recursion+size+escape; прозрачно — immutable); payload-less интернирование. | new perf-план (Plan 120/139) | P2 |
| `[M-opt-elide-proven-overflow-checks]` | Z3/range-элизия доказуемо-безопасных integer-overflow чеков (proven→elide, как Plan 140). | new perf-план / Plan 140 | P2 |
| `[M-opt-preempt-strided-loop]` | `nova_preempt_check()` в back-edge КАЖДОГО цикла (emit_c.rs:14215/15100) блокирует clang'у memset/SIMD-векторизацию. Strip-mine: outer-chunk (check раз в N) + inner-N (без чека → векторизуется); + data-movement через RawMem. Long-term: signal-based async preemption (Go 1.14; в C-рантайме реализуемо — Go доказал M:N в C 2012-13). | Plan 143 §2 / cross-ref Plan 25 G5 + 82/83.x | P2 |

## P2 — Ergonomics / stdlib combinators

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-combinators-completion]` | Добавить `find` (short-circuit→`Option[T]`), `flat_map` (nested comprehensions), `enumerate` (`(i,x)` идиома), `zip` (parallel iter); обобщить `sum`/`min`/`max` с `[]int`-only → generic `[]T` (Num/Comparable bound). НЕ нужны: `collect` (комбинаторы eager), `take`/`skip` (Index[Range] `xs[a..b]` покрывает), `reduce` (fold), `count` (filter+len). | new stdlib-combinators mini-план | P2 |
| `[M-opt-iter-generic-combinators]` | Комбинаторы (map/filter/fold/any/all/…) generic над `Iter[I]`, не только `[]T`-ресивер → работают на Range/HashMap/custom без материализации в `[]T`. Главный рычаг comprehension-эргономики (Python-comprehension работает над любым iterable). | new stdlib-combinators mini-план | P2 |

## P2 — Const-fn / Language features

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-114.4.4-configurable-iterations]` | Const-fn eval loop-limit hardcoded 10_000 (6 sites), нет override. | plan-114 Followups | P2 |
| `[M-114.4.4-let-destructure]` | `let (a,b)=`/record destructure в const-fn body не поддержан (`E_CONST_FN_PATTERN_NOT_SUPPORTED`). | plan-114 Followups | P2 |
| `[M-115-newtype-multiarg-constructor]` | Multi-arg newtype `type X(A,B)` не поддержан (single-arg-only в emit_c). | plan-115 Followups | P2 |

## P2 — Concurrency / Backend / Tooling / Stdlib

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-116-openssl-backend]` | Опц. OpenSSL TLS 1.0/1.1 handler (rustls = default); Plan 116 не начат (PLANNED). | plan-116 Followups | P2 |
| `[M-91.fe5-math-time-conformance]` | math (sqrt/ln) есть; Instant/Duration time-API conformance pending. | plan-91 Followups | P2 |
| `[M-ide-integration-deferred]` | Production LSP (hover/goto/completion/refs/rename/format) 104.2–104.6 не построены (104.7 tree-sitter закрыт). | plan-104 Followups | P2 |
| `[M-118.1-ffi-perf-bench]` | memcpy/memmove bench harness для FFI intrinsics не построен (сами intrinsics landed). | plan-118.1 Followups | P2 |

## P3 — Codegen cleanliness (генерируемый C полиш; рантайм не затронут)

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-codegen-dead-erased-generic-stubs]` | Type-erased `Vec[any]` (prelude-вариадик) эмитит NULL-stub методы — DCE. | codegen-cleanup mini-план | P3 |
| `[M-codegen-unit-block-temp-elision]` | `unit`-block-expr в discard-позиции → бессмысленный `_nv_tmp`. | codegen-cleanup mini-план | P3 |
| `[M-codegen-src-synthesized-attribution]` | `/* SRC */` только statement-granular; синтезированный C без атрибуции. | codegen-cleanup mini-план | P3 |

## P3 — Docs / Sugar

| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-118.1-typed-pointer-cookbook]` | docs/typed-pointers.md cookbook не написан (есть только Plan 115 FFI cookbook). | plan-118.1 Followups | P3 |
| `[M-118.1.7-extern-block]` | `extern "C" { unsafe fn … }` block-сахар (gated на multi-ABI); сейчас individual `external unsafe fn`. | plan-118.1 Followups | P3 |
| `[M-D227-alias-newtype-range]` | D227 range-check НЕ покрывает alias/newtype над sized-int (`assignable()` чекает только direct Named + Readonly/Mut/Unsafe; резолв alias-имени требует `self.types`, недоступного на free-fn coercion-сайте). | plan-142 Scoped open-questions | P3 |
| `[M-D227-float-range-check]` | D227 Rule 5 (f32 exponent overflow) НЕ реализован; Ф.1 scope был integer-only (8 sized-int). | plan-142 Scoped open-questions | P3 |

## By-design / WON'T-DO (не actionable — кандидаты в dead-markers)

| Маркер | Почему не делаем |
|---|---|
| `[M-118-aliasing-xor-rules]` | Rust-style XOR aliasing намеренно НЕ нужен (GC + auto-promote); revisit только если перф потребует. |
| `[M-118-inline-assembly]` | Inline asm — вне scope языка. Открыт лишь в тривиальном «не реализовано». → drop. |
| `[M-118-lifetimes-rust-style]` | Rust lifetimes — вне scope (Nova GC + Go-style auto-promote). → drop. |

---

## Planned (НЕ floating — указатель)
| Маркер | План-дом |
|---|---|
| `[M-140-contract-message]` | Plan 140 (опц. `, "message"` на `requires`/`ensures`/`invariant`) |

## Follow-up: stale-tag cleanup
Триаж (w33ant6rp) нашёл **34 маркера с устаревшим OPEN-тегом** (30 RESOLVED + 4 SUPERSEDED — gap закрыт, текст висит): `[M-115-ptr-arithmetic]`, `[M-83.10.4-residual-flaky]`, `[M-83.10.4-supervised-cancel-armed-race]`, `[M-138-getmut-rename]` (superseded) + 30 resolved (полный список в workflow-output w33ant6rp). **Followup:** поправить их статус в source-планах (отдельный doc-проход), чтобы grep по OPEN был честным.

## Конвенция
- **Planned** маркер → Followups своего плана (+ индекс-строка здесь с home).
- **Floating** (нет плана) → здесь полностью.
- Закрыл → убрал строку (история в simplifications.md). Держим только живое.
