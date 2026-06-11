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
| `[M-83.10.4-iso-cancel-startup-race]` | Iso-cancel startup race (supervised(cancel:) первый тест TIMEOUT); 83.10.5 tactical fix неадекватен (~55%), арх. Ф.B не сделана; 3 stress-теста disabled. | plan-83.11 Followups | P1 |
| `[M-83.11-grow-vs-wake-race]` | Grow-vs-wake torn pointer-read race (grow_state swap несинхр. с driver wake); тесты gated AUTOARM=0; 3 попытки фикса провалены — НЕ повторять. | plan-83.11 Followups | P1 |
| `[M-debug-line-directives]` | Нет `#line N "file.nv"` → дебаггер показывает C, не Nova. Только comment-only `/* SRC */`. | Plan 25 G9 → dedicated план | P1 |
| `[M-83-study-go-c-mn]` | Изучить рабочий M:N из C-исходников Go (≤1.4 `runtime/proc.c`, work-stealing, sysmon-preempt) + подтянуть Nova-M:N, если уступает (открытые race'ы 83.10.4/83.11). Go доказал M:N в C-рантайме. | Plan 83 (M:N umbrella) | P1 |

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
| `[M-codegen-unify-tuple-repr]` | Унифицировать кортежи на **on-demand mono'd typed** структуры (реальные C-типы полей, эмит только использованных типов кортежей). Ретайр blanket all-int `_NovaTuple1..8` pre-decl + int-boxing не-int элементов + `(intptr_t)`-касты. Type-precise → нет лишних аллокаций, лучше eq/debug. Существенный рефактор (~15+ codegen-сайтов: build/field-read/pass/eq). Связь: Plan 131 typed-storage D232, Plan 141 eq, value-types thread. | new codegen-план | P2 |

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
| **Plan 139 — ✅ CLOSED 2026-06-11** | umbrella str=value-record закрыт Ф.0-Ф.7; spec финализирован (D26 MAJOR AMEND + D216 §1 + D228 + D52); 0 new FAIL. Маркеры ниже — gated followups (корень `[M-139-f0-lang-item-decl]`), не блокеры закрытия. |
| `[M-139-f0-lang-item-decl]` | Plan 139 Ф.0 followup / Ф.1 — Nova-декл `type str value priv {...}` как lang-item + privacy-enforcement (`s.ptr`→E_PRIV_FIELD, direct-construct forbidden, `*ro u8` write→E_RO_POINTER_WRITE); требует новой lang-item checker-инфры; 3 neg-фикстуры ждут |
| `[M-139-f0-rt-header-ptr-sign-casts]` | Plan 139 Ф.5 — 59 -Wpointer-sign warnings в рантайм-C-хедерах (array.h/conv.h/effects.h/nova_rt.h) после typedef→`const uint8_t*`; source-compatible, подавлены `-w`; cast string-литералов отложен (часть 354-site работы) |
| `[M-139-f1-trim-view]` | Plan 139 Ф.1 followup — `str @trim()` Nova-body возвращает аллоцированную копию (byte-loop + from_bytes_unchecked); бывшая C `nova_str_trim` возвращала zero-copy slice-view без alloc. View-форма требует `@ptr` field-access (`*ro u8` slice) → gated на `[M-139-f0-lang-item-decl]`. Контент идентичен, разница только перф (alloc vs view) |
| `[M-139-f2-ptr-field-producers]` | Plan 139 Ф.2 followup — `as_bytes`/`split`/`from_bytes_lossy`/`from_bytes_unchecked`/`from_bytes_unchecked_steal` остаются C-примитивами: все требуют `@ptr` field-access str value-record'а (as_bytes zero-copy `Vec{data:@ptr,..}`; split zero-copy view-slices `str{ptr:s.ptr+start,len}`; from_bytes_* construct `str{ptr,len}`). Сегодня `s.ptr`→type-error «cannot access .ptr on str» — gated на `[M-139-f0-lang-item-decl]`. `to_bytes`/`to_chars` мигрированы в Nova-body (копия/decode через `@as_bytes()`). C as_bytes уже zero-copy (array.h:638) — контракт сохранён до lang-item |
| `[M-139-f4-to-cstr-owning]` | Plan 139 Ф.4 followup → совпадает с `[M-118.1-cstr-to-cstr-distinct-copy]` (Plan 118.2): owning `@to_cstr()` (буфер, переживающий source str — needs malloc/free API). Ф.4 НЕ deferred — D26 §3 `as_cstr` alloc-fallback РЕАЛИЗОВАН (NEW C-примитив `nova_fn_nova_str_terminated_ptr`: peek `ptr[len]` + conditional `nova_alloc`). Этот примитив = естественный home для будущей owning-copy (alloc-ветка уже делает GC-tracked копию; owning-вариант снимет zero-copy fast-path). eq/hash/clone (doc-Ф.3) — НЕ in scope этой задачи: str ==/< уже content-eq через direct BinOp lowering (emit_c.rs:16985), не Plan 141 field-by-field |
| `[M-139-f3-bare-return-type-str]` | Plan 139 Ф.3 — pre-existing compiler-баг (НЕ введён Ф.3): top-level `fn f(x str) str` (bare return-type, БЕЗ `->`) лоуэрит return-тип как `nova_unit` → CC-FAIL «returning 'nova_str' from a function with incompatible result type 'nova_unit'». Канонический `-> str` работает корректно. Парсер bare-return-type формы теряет/мисс-парсит return-тип. Низкий приоритет (one-obvious-way = `->`); вынесено при написании t3_clone_independent fixture |
| `[M-139-f6-vec-mut-local-enforcement]` | Plan 139 Ф.6 — DISCOVERED (НЕ введён Ф.6; pre-existing на plan-138.1, вне зоны literal-lowering diff): plan108_2 neg-фикстуры `let_nomut_{array_push,clear,pop,truncate}_neg` ожидают `E_LOCAL_NOT_MUT` при вызове Vec-мутирующих методов (push/clear/pop/truncate) на `let` (non-mut) локале, но codegen succeeds (NEG-NO-ERROR, 9/4). Gap из 138.x Vec-миграции: mut-enforcement на Vec-методах не срабатывает для local-binding mutability (D36). Home: Plan 138.x Vec-mut follow-up. Низкий приоритет (orthogonal к str) |
| `[M-139-interning]` (НЕ ОТКРЫТ — landed in full) | Plan 139 Ф.6 — doc допускал defer dedup-interning если «risky/large». Реализация small (1 файл emit_c.rs, +113/-6) + low-risk (R14/R15 LOW, semantically invisible) → landed целиком, маркер НЕ открыт. Per-CU rodata dedup идентичных литералов: один `static const uint8_t[]` + `static const nova_str` на distinct content; FNV-1a content-hash символы. Записан здесь для аудита (что defer-опция рассмотрена и отклонена в пользу полной реализации) |

## Follow-up: stale-tag cleanup
Триаж (w33ant6rp) нашёл **34 маркера с устаревшим OPEN-тегом** (30 RESOLVED + 4 SUPERSEDED — gap закрыт, текст висит): `[M-115-ptr-arithmetic]`, `[M-83.10.4-residual-flaky]`, `[M-83.10.4-supervised-cancel-armed-race]`, `[M-138-getmut-rename]` (superseded) + 30 resolved (полный список в workflow-output w33ant6rp). **Followup:** поправить их статус в source-планах (отдельный doc-проход), чтобы grep по OPEN был честным.

## Конвенция
- **Planned** маркер → Followups своего плана (+ индекс-строка здесь с home).
- **Floating** (нет плана) → здесь полностью.
- Закрыл → убрал строку (история в simplifications.md). Держим только живое.
