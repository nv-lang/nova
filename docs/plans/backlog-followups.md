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
| `[M-83.10.4-iso-cancel-startup-race]` | ✅ **CLOSED 2026-06-11 (Plan 83-go-cmn Ф.5).** Структурно закрыт Ф.2 (gopark timer-backed park вето́ит на cancel перед arming + READY-latch + driver async-close wake) — НЕ потребовался отдельный код. Подтверждено: design-workflow 380 armed-прогонов + мои 320 (160@MP=1 + 160@MP=4) = 0 hang. 3 disabled stress-теста (supervised_cancel_stress_test) re-enabled с wake-not-hang бюджетами (исходные latency-SLA флакали ~0.8% под jitter, не hang). | Plan 83-go-cmn Ф.5 | ✅ done |
| `[M-83-gopark-bare-park-cancel-veto]` | Теоретический gap: `nova_gopark` НЕ имеет cancel-veto перед WAIT-store. Timer-backed park (Time.sleep) не зависит от него (driver async-close wake → cancel-check в yield), поэтому iso-cancel закрыт. Но **bare gopark без stop_cb** (channels/net — channels.h ~1012, net.c) теоретически мог бы park'нуться с уже-выставленным cancel. Не воспроизведён (нет фикстуры). Fix если всплывёт: Go-style cancel_requested re-check в gopark до WAIT (композится с READY commit-recheck). | Plan 83-go-cmn (Ф.3+) | P3 |
| `[M-83-gocmn-note-primitive-deferred]` | Ф.3 design-finding: `uv_async` УЖЕ корректный note (idempotent + IOCP-backed Windows) → собственный `note.h`/Go lock_sema НЕ нужен. Понадобится только если Ф.6 (timer-heap) / Ф.8 (netpoll) уберут libuv из worker-park. | Plan 83-go-cmn (Ф.6/Ф.8) | P3 |
| `[M-83-f3-coalesce-gated-on-f4]` | Ф.3 nspinning wakep-coalescing (пропустить uv_async_send когда spinner найдёт работу) НЕБЕЗОПАСЕН в текущей per-worker `wake_pending` топологии (spinner не дренит чужой wake_pending → lost-wakeup, review GAP-1/2/3). **Gated на Ф.4** (global-queue routing — spinner сканит global → coalesce-safe). Порядок: Ф.4 → Ф.3-coalesce. | Plan 83-go-cmn Ф.4→Ф.3 | P2 |
| `[M-83.11-grow-vs-wake-race]` | ✅ **CLOSED 2026-06-11** (Plan 83-go-cmn Ф.1b, commit `e1525d90671`). Структурный фикс: `NovaSchedState` chunked stable-address storage (chunk'и never-realloc → torn-pointer невозможен); GitHub issue #2. Closure: grow_vs_wake_explicit 100/100 + stress_iso_3e 66/66 + semaphore_batch_n 30/30 armed. История — simplifications.md + plan §9.5. | Plan 83-go-cmn Ф.1b | ✅ done |
| `[M-debug-line-directives]` | Нет `#line N "file.nv"` → дебаггер показывает C, не Nova. Только comment-only `/* SRC */`. | Plan 25 G9 → dedicated план | P1 |
| `[M-83-study-go-c-mn]` | Порт рабочего M:N из Go ≤1.4 C-рантайма. **✅ research+8-фаз декомпозиция; ✅ Ф.1a ring-port; ✅ Ф.1b chunked park-state (закрыл grow-vs-wake); ✅ Ф.2 gopark/goready (D244, удалил pending_wake, commit d2830c73d7d).** OPEN до Ф.3-Ф.8 (nspinning/iso-cancel/timer-heap/sysmon/netpoll). | Plan 83-study-go-c-mn | P1 |
| `[M-83.11-f2-arm-tsan]` | Ф.2 gopark G0(RELEASE)/G1(SEQ_CST) x86-корректны (XCHG дренит store-buffer); для ARM/weak-memory валидировать под TSAN на Linux. Не регрессия (x86 целевая). Gated на `[M-nova-linux-build]`. | floating (Linux-CI) | P2 |
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
| `[M-138.2-self-in-param]` | `Self` в param-позиции generic-метода (`@append(other Self)`, `@copy_from`/`@compare`/`@equal`) мис-лоуэрит C-тип без receiver-subst → forward-decl≠def; workaround явный `Vec[T]`. (NB Ф.0d: `@append` теперь COPY, signature `@append(other Vec[T])` без `mut` — D141.) | plan-138 Followups | P2 |
| `[M-138.2-bulk-insert-overload]` | Ф.0a Open-Question resolution: bulk-insert живёт как `@splice(i, Vec[T])`, НЕ как второй overload `@insert(i, Vec[T])`. Generic-method overloads коллапсят в монорфизации — `mono_method_decls` (emit_c.rs ~8404) keyed `(type, name)` с одним FnDecl на key, mono-sentinel `MethodSig` несёт пустой `param_c_types` (~8408) → `resolve_overload` не дизамбигуирует single `insert(i,T)` от bulk `insert(i,Vec[T])` для concrete `Vec[int]` (verified: оба роутятся на single, Vec-arg force-fit'ится в `nova_int v` → garbage). Plan 138.2 Ф.0a явно санкционирует `@splice`-rename как fallback. Fold обратно в `@insert` overload ждёт `[M-138.2-generic-method-overload-mono]`. | plan-138.2 Ф.0a Followups | P2 |
| `[M-138.2-generic-method-overload-mono]` | Codegen: generic-метод overloads должны переживать монорфизацию с per-arg-type routing. Сегодня `mono_method_decls` (emit_c.rs ~8404) = `HashMap<(String,String), FnDecl>` (один decl на (type, method-name), overload-коллапс) + mono-sentinel `MethodSig` с пустым `param_c_types` (~8408). Нужно: keyed-by-mangled-sig storage + concrete `param_c_types` в sentinel, чтобы `resolve_overload` (emit_c.rs:9913) дизамбигуировал по C-типам аргументов. Разблокирует fold `@splice`→`@insert` overload ([M-138.2-bulk-insert-overload]). | new codegen-план | P2 |
| `[M-138.2-nested-vec-elem-readback]` | DISCOVERED (pre-existing на plan-138.1, orthogonal к Ф.0a; вне зоны bulk-insert): `Vec[Vec[T]]` второй push+get читает повреждённый nested-элемент. Narrowed: `single push get0` PASS, `two push get0` PASS, `two push len2` PASS, но `two push get1` FAIL (читает не тот контент); `new+push`-вариант → CC-FAIL `unknown type name NovaOpt_nova_int_p` (отсутствует mono'd Option для nested-Vec-elem). Storage/codegen defect для `Vec[Vec[T]]`-элементов. Home: Plan 138.x nested-Vec follow-up. Низкий приоритет (single-уровень Vec корректен). | plan-138.x Followups | P2 |
| `[M-138.2-shadow-warn-post-flip]` | Ф.0c verified-deferral (для Ф.0-final): W_PRELUDE_SHADOW lint (`lint_prelude_shadow`, lints.rs:1459) fires ТОЛЬКО на prelude-visibility. Pre-flip `Vec` НЕ в prelude → explicit `import vec_owned.{Vec}` + user `type Vec` = ordinary import collision → warning корректно НЕ срабатывает (t14 = positive clean-compile, НЕ EXPECT_COMPILE_WARNING). После Ф.0-final (Vec в prelude) shadow юзерского `type Vec` ДОЛЖЕН surface W_PRELUDE_SHADOW; t16 (`#allow(shadow)`) пиннит suppress на тот момент. Checklist-item: после флипа добавить EXPECT_COMPILE_WARNING вариант t14 (Vec уже prelude-visible) и подтвердить W_PRELUDE_SHADOW + suppress. **NOT a defect — семантически правильное pre-flip поведение.** | plan-138.2 Ф.0-final | P2 |
| `[M-138.2-flip-erased-base-body-mono]` | ✅ **CLOSED (re-attempt #2, 2026-06-11).** Принципиальный фикс (не heuristic): Array-арм `type_ref_to_c` (emit_c.rs:5109) — generic-stub element (`is_generic_stub_c` && !contains `____`) → erase в `nova_int`. Тот же int64-erasure что legacy NovaArray `_`-арм (5188) + Named `any_erased` carve-out (5054). Concrete per-element Vec mono эмитится на каждом mono'd call-site. | plan-138.2 Ф.0-final | ✅ DONE |
| `[M-138.2-flip-value-pos-arraylit-vec-gate]` | ✅ **CLOSED по факту (re-attempt #2, 2026-06-11).** 4 contains_key("Vec")-гейта (Plan 138.1) уже Vec-gate-aware на value-position (array-literal `emit_array_lit`/`infer_expr_c_type`); универсальная prelude-доступность Vec-template — это и есть фикс рассинхрона (gate всегда ON для prelude-юнитов, graceful #no_prelude degrade). Проверено t3/t17 (Vec-free → typed Vec storage). | plan-138.2 Ф.0-final | ✅ DONE |
| `[M-138.2-flip-array-ext-vec-recv-routing]` | ✅ **CLOSED (re-attempt #2, 2026-06-11).** Принципиальный фикс (Plan 101.1 alt-key precedent): (a) call-routing (emit_c.rs:~22014) splice'ит `("[]T",m)` sentinel в candidates когда direct-key пуст для `Vec____*` receiver; (b) worklist-drain (emit_c.rs:~3060) роутит `Vec____*::m` worklist-key на `[]T` base FnDecl (recv_type=`Vec____<elem>` → typed-Vec-receiver instance); (c) elem-extract de-mangle через `generic_type_instance_info` (string-strip `Vec____` даёт mangled `Nova_Wrap_p`, не C-тип). Mirror в `infer_expr_c_type` (~30968). plan91_fe1 8/2→10/0. | plan-138.2 Ф.0-final | ✅ DONE |
| `[M-138.2-parfor-vec]` | **OPEN (Ф.2.1, sanctioned exception, 2026-06-11).** parfor (D71) использует `NovaArray_{nova_int,nova_bool,nova_f64,nova_str}` для internal result-collection буфера (emit_c.rs:7242/7290) — internal codegen-путь, не user-facing `[]T`: layout-identical с Vec, никогда не escape'ит, ограничен 4 примитивами. Миграция на Vec требовала бы Vec-template в каждом concurrency-юните (graceful-degrade риск) ради нулевого семантического выигрыша. RETAINED как documented exception. Re-attempt = когда (если) NovaArray retire завершится после Plan 139 Ф.2. parallel_for 2/0, parallel_for_array 1/0 (GREEN). | plan-138.2 Ф.2 | P3 |
| `[M-138.2-closure-array-vec]` | **OPEN (Ф.2.2, sanctioned exception, 2026-06-11; = ранее `[M-138.1-closure-array]`).** `[]fn(...)` → `NovaArray_void_p*` (emit_c.rs:5106-5107, explicit exclusion из `[]T`→Vec flip). Closures = `void*` (`NovaClos_X*`); `Vec[fn]` mono требует closure-as-element schema, которой нет. Feasibility-investigation: closure-array fixtures (plan55 f1_closure_array_with_capture/f1_fn_array_collect_positive/f1_negative_fn_array_arity_mismatch) все PASS на `NovaArray_void_p`-пути. RETAINED как documented exception. Re-attempt вместе с финальным NovaArray retire (Plan 139 Ф.2). | plan-138.2 Ф.2 | P3 |
| `[M-138.5-right-binding-migration]` | prefix→postfix `*T`: codegen pointee (138.4 G-D) сделан, parser-level migration + `E_POINTER_PREFIX_MODIFIER` — landed 138.5 (verify после `wd3vgeu6l`). | plan-138.5 Followups | P2 |
| `[M-138-range-value]` | Range — reference-record, не value-record; Plan 138 Ф.0.3 migration не сделана. (138.5 трогает range.nv — re-confirm.) | plan-138 Followups | P2 |
| `[M-138-unsafe-block-postfix-stmt]` | Постфикс (`.method()`/`[i]`/`.field`) должен цепляться к `unsafe {}` block-expr и в **statement-позиции**: `unsafe { @data[i] }.display(sb)` без скобок. Сейчас leading `unsafe {}` в начале stmt = блок-заявление, постфикс не привязывается → нужны `(…)`. Fix: `parser/mod.rs::parse_stmt_or_expr` (8650) ветка leading-`unsafe` парсит полное выражение с постфиксом (обобщить на `if`/`match` block-expr; bare `{}` остаётся stmt). Cleanup: убрать скобки vec_owned.nv:863/875. | plan-138 Followups | P2 |
| `[M-138-double-pointer-codegen-test]` | `**T`/`***T` **парсятся** ✅ (Star-arm рекурсивен, parser/mod.rs:5214; нет `**` power-токена), per-level postfix pointee-mod консистентен (`*mut *ro T`). НЕ проверено: codegen `Pointer(Pointer(T))` → C `T**` + модификатор-комбо end-to-end. Нужен тест + D216 doc-note «N-level pointers, per-level postfix pointee-mod». Use: FFI `char**`/argv, out-params. | plan-138 Followups | P2 |
| `[M-138-binding-type-mut-conflict]` | `ro X mut T` валиден ⟺ у T есть **изменяемое поле/элемент, ВИДИМОЕ в use-site** (visibility-aware; переиспользует field-visibility + D175 readonly-field). Скаляры/`str`(priv+`*ro` fields)/`unit`/`fn` — 0 видимой mutable-начинки → отвергать везде; record c private полями — отвергать из public-кода, разрешать в модуле-владельце; tuple/value-record/ref-record c доступными mutable-полями — разрешать (immutable-handle/mutable-interior). Контекстно-зависимая диагностика. Семантически D176 amend, дом Plan 138 (user call). Edge: free-fn coercion-сайт без `self.types` (как D227 alias). | plan-138 Followups | P2 |
| `[M-ptr-cast-reinterpret-unsafe]` | Указательный reinterpret-каст должен требовать `unsafe`/`E_PTR_CAST_REINTERPRET`: (a) `*ro T → *mut T` widening (снятие ro, аналог E_READONLY_COERCE), (b) `*T → *U` смена pointee-типа (`*u8→*int` = OOB/align/aliasing UB). Safe: `*mut→*ro` narrowing. Сейчас `as *mut`/`as *U` = safe-reinterpret (deref-unsafe гейтит запись, но laundering в safe-коде). D216 cast-rules amend. | plan-138.5 Followups | P2 |
| `[M-138-canonical-modifier-order]` | Утвердить ЕДИНЫЙ порядок type-decl модификаторов: канон **`value priv`** (не `priv value`). Order-independence (намеренный — plan124_8 `modifier_order_independence_ok`) противоречит «one canonical syntax» Nova → enforce: out-of-canon → `E_MODIFIER_ORDER` + fix-it «переставь в `value priv`»; флип plan124_8-теста в negative; мигрировать редкие `priv value`. Обобщить на все type-modifier'ы (fixed canonical order). D-block (canonical-modifier-order / amend D124). | plan-138 Followups | P2 |
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
| `[M-139-f0-lang-item-decl]` | Plan 139 Ф.0 followup / Ф.1 — Nova-декл `type str value priv {...}` как lang-item + privacy-enforcement (`s.ptr`→E_PRIV_FIELD, direct-construct forbidden, write через `str.ptr`→E_POINTER_RO_ASSIGN); требует новой lang-item checker-инфры; 3 neg-фикстуры ждут. **РАЗГЕЙЧЕНО Plan 147 Ф.4 (2026-06-12):** поле объявляется `ptr *u8` (bare `*T ≡ *ro T` = ro-pointee canon под 3-axis/D246 — flip-scan не нужен; `*ro u8` был бы E_REDUNDANT_POINTER_RO). Spec-декл синхронизирована во всех 3 точках (02-types.md ×2 + 08-runtime.md D26). Остаётся только lang-item checker-инфра (само объявление в Nova-source). |
| `[M-139-f0-rt-header-ptr-sign-casts]` | Plan 139 Ф.5 — 59 -Wpointer-sign warnings в рантайм-C-хедерах (array.h/conv.h/effects.h/nova_rt.h) после typedef→`const uint8_t*`; source-compatible, подавлены `-w`; cast string-литералов отложен (часть 354-site работы) |
| `[M-139-f1-trim-view]` | Plan 139 Ф.1 followup — `str @trim()` Nova-body возвращает аллоцированную копию (byte-loop + from_bytes_unchecked); бывшая C `nova_str_trim` возвращала zero-copy slice-view без alloc. View-форма требует `@ptr` field-access (`*ro u8` slice) → gated на `[M-139-f0-lang-item-decl]`. Контент идентичен, разница только перф (alloc vs view) |
| `[M-139-f2-ptr-field-producers]` | Plan 139 Ф.2 followup — `as_bytes`/`split`/`from_bytes_lossy`/`from_bytes_unchecked`/`from_bytes_unchecked_steal` остаются C-примитивами: все требуют `@ptr` field-access str value-record'а (as_bytes zero-copy `Vec{data:@ptr,..}`; split zero-copy view-slices `str{ptr:s.ptr+start,len}`; from_bytes_* construct `str{ptr,len}`). Сегодня `s.ptr`→type-error «cannot access .ptr on str» — gated на `[M-139-f0-lang-item-decl]`. `to_bytes`/`to_chars` мигрированы в Nova-body (копия/decode через `@as_bytes()`). C as_bytes уже zero-copy (array.h:638) — контракт сохранён до lang-item |
| `[M-139-f4-to-cstr-owning]` | Plan 139 Ф.4 followup → совпадает с `[M-118.1-cstr-to-cstr-distinct-copy]` (Plan 118.2): owning `@to_cstr()` (буфер, переживающий source str — needs malloc/free API). Ф.4 НЕ deferred — D26 §3 `as_cstr` alloc-fallback РЕАЛИЗОВАН (NEW C-примитив `nova_fn_nova_str_terminated_ptr`: peek `ptr[len]` + conditional `nova_alloc`). Этот примитив = естественный home для будущей owning-copy (alloc-ветка уже делает GC-tracked копию; owning-вариант снимет zero-copy fast-path). eq/hash/clone (doc-Ф.3) — НЕ in scope этой задачи: str ==/< уже content-eq через direct BinOp lowering (emit_c.rs:16985), не Plan 141 field-by-field |
| `[M-139-f3-bare-return-type-str]` | Plan 139 Ф.3 — pre-existing compiler-баг (НЕ введён Ф.3): top-level `fn f(x str) str` (bare return-type, БЕЗ `->`) лоуэрит return-тип как `nova_unit` → CC-FAIL «returning 'nova_str' from a function with incompatible result type 'nova_unit'». Канонический `-> str` работает корректно. Парсер bare-return-type формы теряет/мисс-парсит return-тип. Низкий приоритет (one-obvious-way = `->`); вынесено при написании t3_clone_independent fixture |
| `[M-139-f6-vec-mut-local-enforcement]` | Plan 139 Ф.6 — DISCOVERED (НЕ введён Ф.6; pre-existing на plan-138.1, вне зоны literal-lowering diff): plan108_2 neg-фикстуры `let_nomut_{array_push,clear,pop,truncate}_neg` ожидают `E_LOCAL_NOT_MUT` при вызове Vec-мутирующих методов (push/clear/pop/truncate) на `let` (non-mut) локале, но codegen succeeds (NEG-NO-ERROR, 9/4). Gap из 138.x Vec-миграции: mut-enforcement на Vec-методах не срабатывает для local-binding mutability (D36). Home: Plan 138.x Vec-mut follow-up. Низкий приоритет (orthogonal к str) |
| `[M-139-interning]` (НЕ ОТКРЫТ — landed in full) | Plan 139 Ф.6 — doc допускал defer dedup-interning если «risky/large». Реализация small (1 файл emit_c.rs, +113/-6) + low-risk (R14/R15 LOW, semantically invisible) → landed целиком, маркер НЕ открыт. Per-CU rodata dedup идентичных литералов: один `static const uint8_t[]` + `static const nova_str` на distinct content; FNV-1a content-hash символы. Записан здесь для аудита (что defer-опция рассмотрена и отклонена в пользу полной реализации) |
| `[M-147-infer-call-ret-mut-axis]` (P2) | Plan 147 Ф.3 — checker `infer_expr_type` пропагирует return-тип для coercion/deref-write gate ТОЛЬКО для `ro`-wrapped и pointer-shaped returns, и только когда ВСЕ overload'ы согласны (call-resolution не выполнена на этом этапе). Method-call return (`v.f()`), generic-return, mixed-overload → `None`→no-gate. L3/coercion-нарушение для таких форм ловится позже C-компилятором (`const T*` write = CC-FAIL), не чистой Nova-диагностикой. Home: Plan 147 follow-up (полноценный return-type inference + monomorphization в checker'е). Soundness сохранён (отвергается, но позже) |
| `[M-147-deref-write-compound-lvalue]` (P2) | Plan 147 Ф.3 — L3 deref-write gate (`*p=v`→E_POINTER_RO_ASSIGN) срабатывает когда `p` — Ident/As-cast с известным типом в scope. Составные lvalue (`*(p+i)=v` Binary operand, и пр.) дают `infer_expr_type=None`→no-gate; ловится C-уровнем (const-pointee). Home: Plan 147 follow-up. Soundness сохранён |
| `[M-147-generic-element-deref-write]` (P2) | Plan 147 Ф.3 — oracle row E `Vec[*T]` vs `Vec[*mut T]` (`*v[i]=x`): element-deref-write через generic-instance index НЕ enforced на Nova-уровне (требует element-type inference через `[]` на mono'd generic в checker'е). Документирован в oracle (02-types.md D246), ловится C-уровнем (const element pointee). Home: Plan 147 follow-up. Soundness сохранён |
| `[M-147-null-star-ptr-retraction-guard]` (P3) | **PRE-EXISTING, обнаружен Plan 147 Ф.4 (не регрессия Ф.4).** Парсер `parse_primary` emit'ит `E_NULL_PTR_RETRACTED_USE_OPTION` только для `null <bare-prim-ident>` (`ptr`/`int`/…/`str`); форма `null *()` (typed-pointer-literal, Plan 134) не покрыта → fall-through → `undefined identifier null`. Фикстура plan118/t5_neg_null_ptr_retracted = NEG-WRONG-MSG. Регрессия с Plan 134 commit c41d568ae2c (тело `null ptr`→`null *()`, guard не расширён). Fix: расширить guard на `null` + Star/Pointer-type position. Orthogonal к 3-axis. Home: parser cleanup. Hard-error сохранён (другой код) |

## Follow-up: stale-tag cleanup
Триаж (w33ant6rp) нашёл **34 маркера с устаревшим OPEN-тегом** (30 RESOLVED + 4 SUPERSEDED — gap закрыт, текст висит): `[M-115-ptr-arithmetic]`, `[M-83.10.4-residual-flaky]`, `[M-83.10.4-supervised-cancel-armed-race]`, `[M-138-getmut-rename]` (superseded) + 30 resolved (полный список в workflow-output w33ant6rp). **Followup:** поправить их статус в source-планах (отдельный doc-проход), чтобы grep по OPEN был честным.

## Конвенция
- **Planned** маркер → Followups своего плана (+ индекс-строка здесь с home).
- **Floating** (нет плана) → здесь полностью.
- Закрыл → убрал строку (история в simplifications.md). Держим только живое.
