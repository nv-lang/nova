<!-- AUTO-GENERATED — НЕ РЕДАКТИРОВАТЬ РУКАМИ. Регенерация: bash scripts/tools/gen-plan-status.sh -->

# Статусы планов (сводный обзор)

> **Автосгенерировано**: `bash scripts/tools/gen-plan-status.sh`, дата генерации: 2026-08-02 12:07 UTC.
> **⚠ Этот файл ПРОТУХАЕТ между перегенерациями** — git-копия отражает момент
> последнего запуска, не текущее состояние. Источник правды — ТОЛЬКО строка
> `**Статус:**` в самом файле плана; при любом сомнении — перегенерируй или
> читай план напрямую. Редактировать руками бессмысленно — следующий запуск
> перезапишет.

| План | Название | Статус |
|---|---|---|
| [01](01-roadmap-v0.1.md) | Nova — план разработки | — (нет Статус-строки) |
| [02](02-codegen-c-backend.md) | Plan 02 — Nova C Backend (compiler-codegen) | — (нет Статус-строки) |
| [03](03-package-ecosystem-roadmap.md) | Plan 03: Package ecosystem — roadmap-индекс | roadmap-индекс. **Приоритет:** P3 — после bootstrap/stdlib; |
| [03.1](03.1-path-git-dependencies.md) | Plan 03.1: path- и git-зависимости (bootstrap, без registry) | ✅ **ЗАКРЫТ** (2026-05-22) — Ф.1–Ф.6 целиком. |
| [03.2](03.2-version-resolution.md) | Plan 03.2: Version resolution — semver-диапазоны + backtracking-резолвер | ✅ **ЗАКРЫТ** (2026-05-22) — Ф.1–Ф.5 целиком. |
| [03.3](03.3-registry-protocol.md) | Plan 03.3: Registry protocol — HTTP-реестр, content-addressing, подпись | 📋 **proposed, отложен** — нужна серверная инфраструктура |
| [03.4](03.4-effect-aware-tooling.md) | Plan 03.4: Effect-aware package tooling — nova info, effect-surface | ✅ **ЗАКРЫТ** (2026-05-22) — Ф.1–Ф.4 (effect-срез). |
| [04](04-buffer-split-and-external.md) | План: Split Buffer на StringBuilder/WriteBuffer/ReadBuffer + keyword external | — (нет Статус-строки) |
| [05](05-as-cast-codegen.md) | План 05: as-cast — реализация narrowing в codegen | ✅ выполнено (2026-05-08). |
| [06](06-iter-protocol-codegen.md) | План 06: Iter[T] protocol в codegen — общий for-in | ✅ ВЫПОЛНЕНО. Ф.1-Ф.3 + Ф.5 закрыты (`emit_for` Case 3 — |
| [07](07-as-cast-saturation.md) | План 07: as-cast — saturation для float→int (закрытие UB-gap'а из плана 05) | ✅ выполнено (2026-05-08). |
| [08](08-from-into-conversions.md) | План 08: From/Into framework + сводная конверсионная инфраструктура | ✅ почти выполнено (2026-05-08); Ф.6 отложен. |
| [09](09-clang-migration.md) | План 09: миграция с MSVC на Clang/LLVM | ✅ Ф.1-Ф.5 закрыты 2026-05-11. Ф.6 (бенчмарки) отложен до std/encoding/json. |
| [10](10-pgo-integration.md) | План 10: PGO integration (stub / future) | stub / future. Полный план будет написан после |
| [11](11-method-values-and-overload.md) | План 11: Method values + overload resolution | ✅ **ЗАКРЫТ (2026-05-08, вечер).** Ф.1-Ф.3 + Ф.4 + Ф.4.5 + |
| [12](12-builtins-driven-codegen.md) | План 12 — builtins.nv-driven external dispatch | ✅ ЗАКРЫТ (2026-05-08, кроме Ф.6 — отложен). |
| [13](13-runtime-stdlib-and-autogen.md) | План 13 — Runtime stdlib (str/math) + auto-gen std/runtime/*.nv | ✅ MVP closed (2026-05-08). Ф.1-Ф.3, Ф.5, Ф.6, Ф.7 готовы. |
| [14](14-stdlib-codegen-gaps.md) | План 14: stdlib-codegen gaps — to compile std/* natively | ✅ **ЗАКРЫТ** (2026-05-12). Ф.1 ✅, Ф.2 ✅, Ф.3 ✅, Ф.4 ✅, |
| [15](15-generic-bounds-enforcement.md) | План 15: Generic bounds enforcement (D72) | ✅ ЗАКРЫТ (2026-05-11). Все фазы Ф.1-Ф.5 реализованы; Ф.4 |
| [16](16-capability-enforcement.md) | План 16: Capability enforcement — forbid и realtime compile-time checks | ✅ **ЗАКРЫТ** (2026-05-10). Ф.1-Ф.9 реализованы; |
| [17](17-q-resolutions.md) | План 17: Q-resolutions — закрыть полу-открытые вопросы | ✅ ЗАКРЫТ (2026-05-08). |
| [18](18-stdlib-roadmap.md) | План 18: Stdlib roadmap для Nova | — (нет Статус-строки) |
| [19](19-closure-and-error-ops.md) | План 19: Closure & error-ops & handler-rev — миграция на \|x\| + fn(...) + !! + Handler[E, IRT] | ✅ **ЗАКРЫТ** (2026-05-10). C1–C14 реализованы; C16 |
| [20](20-defer-implementation.md) | План 20: D90 implementation — defer / errdefer | ✅ **ЗАКРЫТ** — реализовано: лексер `KwDefer`/`KwErrdefer`, |
| [21](21-channel-revision-implementation.md) | План 21: D91 implementation — Channel revision (capability-split) | ✅ **ЗАКРЫТ** — D91 capability-split реализован |
| [22](22-sleep-libuv-integration.md) | План 22: Time.sleep через libuv + production-grade event-loop scheduler | ✅ **ЗАКРЫТ** (2026-05-11). Production-grade pass завершён. |
| [24](24-cross-platform-test-runner.md) | План 24: cross-platform test runner | ✅ ЗАКРЫТ (Ф.1-Ф.3). Runner реализован в |
| [25](25-production-readiness-roadmap.md) | План 25: Production readiness — honest gap analysis vs Go/Rust | roadmap, не план-исполнения. Анализ остающегося отставания |
| [26](26-test-runner-hardening.md) | План 26: hardening test-runner до cargo/go-test уровня | ✅ Ф.1-Ф.4, Ф.6-Ф.15 ЗАКРЫТЫ 2026-05-11. Ф.5 (caching) отложен. |
| [27](27-gc-switch.md) | План 27: GC switch + test-runner polish | в работе. Ф.1 ✅. Ф.1.5 ✅. Ф.2 ✅. Ф.4 ✅. Ф.6 ✅. Б.2 ✅. Б.3 ✅. Б.4 ✅. Б.5 ✅. Б.6 ✅. Б.7 ✅. Б.8 ✅. |
| [28](28-nova-cli.md) | План 28: nova CLI binary | ✅ ЗАКРЫТ 2026-05-18 (Ф.0-Ф.7; nova-cli/ crate, все субкоманды реализованы, run_tests.ps1/run_tests.sh/regen_runtime.ps1 удалены). |
| [29](29-repo-layout.md) | План 29: реорганизация корня репозитория | план, не начат. |
| [30](30-channel-improvements.md) | План 30: Channel improvements — send→bool + multi-writer | ✅ закрыт (2026-05-11). |
| [31](31-channel-select.md) | План 31: select — multiplexed channel operations | ✅ ЗАКРЫТ (2026-05-11). Реализован production-ready select. |
| [32](32-gc-introspection.md) | Plan 32: GC introspection API (gc.heap_size() / gc.collect()) | ✅ ЗАКРЫТ — все фазы Ф.1-Ф.5 выполнены. Ф.1 runtime API |
| [33](33-contracts-implementation.md) | Plan 33: Контракты (D24) — roadmap-индекс | — (нет Статус-строки) |
| [33.1](33.1-contracts-core.md) | Plan 33.1: Контракты — Core (requires/ensures/old/result + Z3 + runtime) | — (нет Статус-строки) |
| [33.2](33.2-contracts-imperative.md) | Plan 33.2: Контракты — Imperative (frame + loops + termination + composition + invariant) | — (нет Статус-строки) |
| [33.3](33.3-contracts-advanced.md) | Plan 33.3: Контракты — Advanced (#pure views + ghost + quantifiers + FP/strings + perf + Dafny-parity) | — (нет Статус-строки) |
| [33.4](33.4-contracts-parity-gaps.md) | Plan 33.4: Contracts — Production-Parity Gap Closure | — (нет Статус-строки) |
| [33.5](33.5-contracts-verifier-production.md) | Plan 33.5: Contracts — Verifier Production Hardening | — (нет Статус-строки) |
| [33.6](33.6-contracts-production-hardening.md) | Plan 33.6: Contracts — Production Hardening | — (нет Статус-строки) |
| [33.7](33.7-bitvector-overflow.md) | Plan 33.7: Bit-vectors + integer overflow theory | — (нет Статус-строки) |
| [33.8](33.8-verifier-soundness.md) | Plan 33.8: Verifier soundness hardening (D24) | — (нет Статус-строки) |
| [33.9](33.9-opaque-reveal-fuel.md) | Plan 33.9: Contracts — Opaque / Reveal / Fuel | — (нет Статус-строки) |
| [33.14](33.14-z3-cvc5-crosscheck.md) | Plan 33.14: Z3 ↔ CVC5 cross-check | — (нет Статус-строки) |
| [34](34-stdlib-typecheck-and-compile-fix.md) | План 34: stdlib type-check + compile fix | ✅ **ЗАКРЫТ 2026-05-12** (расширенный scope). |
| [35](35-cross-file-resolve.md) | План 35: Cross-file resolve | — (нет Статус-строки) |
| [36](36-cli-production-hardening.md) | Plan 36: CLI production hardening — nova check / nova test | **MVP ✅ закрыт** (Ф.0 + Ф.1 + R7 + R10 basic, commits |
| [37](37-typecheck-semantic-parity.md) | План 37: type-checker semantic parity with codegen | план, не начат. Средний приоритет (UX: ошибки D54 |
| [38](38-numeric-type-constants.md) | Plan 38: Numeric type constants (int.MAX / f64.MAX / etc.) | ✅ ЗАКРЫТ (commit 64d1a41c1a — codegen mapping для D26 |
| [39](39-range-stdlib-fixes.md) | Plan 39: std/collections/range.nv stdlib fixes | — (нет Статус-строки) |
| [42](42-folder-modules.md) | (нет заголовка) | — (нет Статус-строки) |
| [42.04](42.04-per-file-imports-scope.md) | Plan 42.4: per-file imports scope (AST refactor) | — (нет Статус-строки) |
| [42.08](42.08-audit-closing.md) | Plan 42.8: Closing audit gaps (Plan 42 quality) | — (нет Статус-строки) |
| [42.09](42.09-re-export.md) | Plan 42.09: Re-export (export import) | — (нет Статус-строки) |
| [42.10](42.10-module-level-forbid.md) | Plan 42.10: Module-level #forbid / #requires (правило I) | — (нет Статус-строки) |
| [42.11](42.11-inline-module-doc.md) | Plan 42.11: Inline module-level doc syntax | — (нет Статус-строки) |
| [42.12](42.12-cfg-conditional-compilation.md) | Plan 42.12: Conditional compilation (#cfg) | — (нет Статус-строки) |
| [42.13](42.13-internal-naming.md) | Plan 42.13: internal/ extended naming (D29 rev-3.1) | — (нет Статус-строки) |
| [42.14](42.14-backlog-closure.md) | Plan 42.14: Backlog closure (6 deferred items) | honest re-defer. Реализация требует NameResCtx per-import |
| [42.15](42.15-per-peer-import-isolation.md) | Plan 42.15: Per-peer / per-import isolation (NameResCtx refactor) | — (нет Статус-строки) |
| [42.16](42.16-module-attr-syntax.md) | Plan 42.16: Module-attribute syntax (position + #cfg operators) | — (нет Статус-строки) |
| [42.17](42.17-audit-closure.md) | Plan 42.17: Production-readiness audit closure | — (нет Статус-строки) |
| [44](44-mn-runtime-roadmap.md) | План 44: M:N runtime — архитектурный roadmap | roadmap, **не** план-исполнения. Целит на milestone v1.0+. |
| [44.1](44.1-channel-hardening.md) | План 44.1: Channel hardening — production parity с Go/Rust | — (нет Статус-строки) |
| [44.2](44.2-fiber-arena-posix.md) | План 44.2: Per-thread fiber stack arena с lazy commit | ✅ ЗАКРЫТ (Этапы 1-5 + R8 audit + P41-3/P41-6 — per-thread |
| [44.3](44.3-fiber-arena-windows.md) | План 44.3: Windows fiber stack arena с SEH lazy commit | ⬛ **SUPERSEDED BY [Plan 82](82-windows-fiber-arena.md)** |
| [44.4](44.4-mn-runtime-stage0.md) | План 44.4: M:N Runtime — Этап 0 (executable sub-plan для Plan 44) | executable sub-plan, Этап 0 реализован 2026-05-13. |
| [44.5](44.5-work-stealing-scheduler.md) | План 44.5: Work-stealing scheduler + per-worker libuv loop | executable sub-plan, начат 2026-05-13. |
| [44.7](44.7-preemption.md) | План 44.7: Signal-based preemption (SIGURG / CONTEXT) | ✅ CLOSED — Вариант B реализован (2026-05-14). |
| [45](45-nova-doc.md) | Plan 45: nova doc — production-grade documentation tooling | — (нет Статус-строки) |
| [46](46-named-parameters.md) | Plan 46: Именованные аргументы и значения параметров по умолчанию | — (нет Статус-строки) |
| [47](47-supervised-cancel.md) | Plan 47: supervised(cancel:) — удаление keyword cancel_scope | — (нет Статус-строки) |
| [48](48-closures-in-generics.md) | Plan 48: Monomorphization — generic functions без type-erasure | — (нет Статус-строки) |
| [48.1](48.1-cross-module-generic-template-registration.md) | Plan 48.1 — Cross-module generic template registration ordering | 🟡 IN PROGRESS 2026-06-03 |
| [49](49-cancel-throw-routing.md) | Plan 49: Cancellation semantics — kinded throws + typed cancel reason | — (нет Статус-строки) |
| [50](50-default-keyword-only.md) | Plan 50: Параметр с дефолтом — keyword-only на месте вызова | — (нет Статус-строки) |
| [51](51-d55-record-literal-unification.md) | Plan 51: Record-literal — тип пишется ровно один раз | — (нет Статус-строки) |
| [52](52-hashmap-literals.md) | Plan 52: HashMap-литералы — {field: v} coercion + [k: v] литерал | — (нет Статус-строки) |
| [52.1](52.1-hashmap-literals-advanced.md) | Plan 52.1: HashMap-литералы — advanced features (deferred from Plan 52) | — (нет Статус-строки) |
| [52.2](52.2-bootstrap-codegen-fixes.md) | Plan 52.2: Bootstrap codegen fixes — unblock Ф.1 (spread) + Ф.2 (const map) | — (нет Статус-строки) |
| [52.3](52.3-remaining-map-features.md) | Plan 52.3: Remaining HashMap-литерал features | — (нет Статус-строки) |
| [53](53-let-destructuring.md) | Plan 53: Record-destructuring в let-биндингах | — (нет Статус-строки) |
| [54](54-codegen-followups-from-48-49-audit.md) | Plan 54: Codegen follow-ups от Plan 48/49 audit | — (нет Статус-строки) |
| [55](55-codegen-followups-from-plan-54.md) | Plan 55: Codegen follow-ups от Plan 54 + Plan 52.x mono-pass blockers | — (нет Статус-строки) |
| [56](56-vtable-dispatch-erased-generics.md) | Plan 56: Vtable dispatch для bound-K methods в erased generics | — (нет Статус-строки) |
| [57](57-perf-benchmark-infrastructure.md) | Plan 57: Performance benchmark infrastructure | — (нет Статус-строки) |
| [57.E.2](57.E.2-distributed-bench-sketch.md) | Plan 57.E.2 — Distributed bench coordination (design sketch) | — (нет Статус-строки) |
| [57.E.3](57.E.3-ai-regression-interpretation-sketch.md) | Plan 57.E.3 — AI-driven regression interpretation (design sketch) | — (нет Статус-строки) |
| [57.E.4](57.E.4-memory-bandwidth-sketch.md) | Plan 57.E.4 — Memory bandwidth measurement (design sketch, Linux-only) | — (нет Статус-строки) |
| [58](58-cross-toolchain-msvc-verification.md) | Plan 58: Cross-toolchain CI matrix (Clang / MSVC / GCC × 3 OS) | — (нет Статус-строки) |
| [59](59-tuple-monomorphization.md) | Plan 59: Tuple monomorphization (mono'd _NovaTuple_<K>_<V> структуры) | — (нет Статус-строки) |
| [59.1](59.1-generic-anon-tuple-mono.md) | Plan 59.1 — Generic anonymous tuple monomorphization | — (нет Статус-строки) |
| [60](60-len-access-uniformity.md) | Plan 60: size-accessor uniformity (.len() / .is_empty() / .cap() — method-only across all collections) | — (нет Статус-строки) |
| [61](61-typed-error-effect-codegen.md) | Plan 61: Typed Fail[E] codegen — hybrid (per-E mono + erased Fail[any] fallback) | — (нет Статус-строки) |
| [62](62-prelude-hardcode-migration.md) | Plan 62: Migrate hardcoded prelude → std/prelude.nv (full D26 compliance + splittable + no_prelude enforcement) | — (нет Статус-строки) |
| [62.A.bis](62.A.bis-sum-schema-registry.md) | Plan 62.A.bis: Generic schema registry для sum-types | — (нет Статус-строки) |
| [62.B.bis](62.B.bis-print-println-migration.md) | Plan 62.B.bis: print / println миграция в std/prelude/runtime.nv через D69 variadic + any | — (нет Статус-строки) |
| [62.D.bis](62.D.bis-opaque-types-and-external-type-d-block.md) | Plan 62.D.bis: Opaque types в std/prelude/collections.nv + D126 external type syntax | — (нет Статус-строки) |
| [62.F.bis](62.F.bis-edition-shadow-and-runtime-effects.md) | Plan 62.F.bis: Edition versioning + W_PRELUDE_SHADOW lint + ambient runtime effects + spec amendments | — (нет Статус-строки) |
| [63](63-cross-module-mono-dispatch-correctness.md) | Plan 63: cross-module + mono dispatch correctness | — (нет Статус-строки) |
| [64](64-nova-lang-website.md) | Plan 64 — nv-lang.org (dogfooding initiative) | Ф.0 ✅ ЗАКРЫТ 2026-05-18 |
| [65](65-chanreader-close-after.md) | Plan 65: ChanReader.close_after(Duration) — timer-channel API parity с Go/Rust/TS | ✅ ЗАКРЫТ (MVP Ф.0-Ф.9 + hardening Ф.10-Ф.14 — close_after, |
| [66](66-timer-wheel-and-tick-every.md) | Plan 66: Timer-wheel runtime + ChanReader.tick_every(Duration) periodic ticker | proposed (outline only — full plan to be written when |
| [67](67-println-overload-return-type.md) | Plan 67: println/print — overload resolution через return-type inference | ✅ ЗАКРЫТ 2026-05-18 (Ф.0–Ф.4 done; см. |
| [68](68-print-as-nova-function.md) | Plan 68: print/println как Nova-функции через protocol-as-value | proposed — открытые вопросы не решены. |
| [69](69-byte-to-u8.md) | Plan 69 — Remove# Plan 69 — Remove byte type alias, canonicalise u8 | — (нет Статус-строки) |
| [70](70-no-silent-nova-int-fallback.md) | Plan 70: Strict type propagation в codegen — no silent nova_int fallback | — (нет Статус-строки) |
| [70.1](70.1-module-alias-resolution.md) | Plan 70.1: Module alias resolution в codegen — import X as th + th.func() | ✅ ЗАКРЫТ 2026-05-19. |
| [70.2](70.2-linkedlist-sum-type-mono.md) | Plan 70.2: LinkedList sum-type monomorphization | ✅ closed 2026-05-19 (partial→full |
| [70.3](70.3-char-int-mono-distinction.md) | Plan 70.3: char↔int distinction в codegen mono'd generics | ✅ closed 2026-05-19 (full cascade |
| [70.4](70.4-primitive-type-distinction-complete.md) | Plan 70.4: Complete primitive-type distinction в codegen mono'd generics | ✅ ЗАКРЫТ — Ф.1 (f32/f64) ✅ closed 2026-05-19; Ф.2 (sized-int) ✅ closed 2026-05-19; Ф.3 (spec D129) ✅ closed 2026-05-19; Ф.4 (byte/u8 mangle unification) ✅ closed 2026-05-19; full byte-removal deferred. Метка обновлена 2026-05-21 (аудит план-статусов: все Ф.1-Ф.4… |
| [70.5](70.5-uint-primitive-symmetry.md) | Plan 70.5: uint symmetric primitive type | ✅ closed 2026-05-19 — Q1-Q4 accepted, Ф.1-Ф.3 implemented. |
| [71](71-doc-stability-scope.md) | Plan 71: doc-check stability-tier scope — opt-in vs implicit | ✅ **ЗАКРЫТ 2026-05-19** (Ф.0-Ф.6: план + spec D127 + idiom page + manifest field + lint config/severity + fixture-skip + stdlib opt-in + 17 unit + 11 integration tests + smoke green). |
| [72](72-protocol-and-type-system-followups.md) | Plan 72: Protocol and type-system followups | — (нет Статус-строки) |
| [73](73-consume-qualifier.md) | Plan 73: consume qualifier — D131 | — (нет Статус-строки) |
| [73.1](73.1-consume-binding-syntax.md) | Plan 73.1 — consume binding syntax (D131 V2 extension) | ✅ **ЗАКРЫТ 2026-05-28** — D180 spec + type-checker enforcement (2 error codes) + plan73/ migration + plan73_1/ 8 fixtures. См. §«Status — closure summary» в конце файла. |
| [74](74-primitive-bitcast.md) | Plan 74: Primitive bitcast — to_bits / from_bits | — (нет Статус-строки) |
| [75](75-str-test-coverage.md) | Plan 75 — Полное тестовое покрытие встроенного типа str | ✅ ЗАКРЫТ 2026-05-20 — Ф.0–Ф.7, коммит 50d8790f7c0 |
| [76](76-never-lowercase-keyword.md) | Plan 76: never — bottom-тип как строчный встроенный keyword | — (нет Статус-строки) |
| [77](77-fluent-return.md) | Plan 77: Fluent-return — точное «метод возвращает receiver» | — (нет Статус-строки) |
| [78](78-prelude-codegen-single-source.md) | Plan 78: Prelude codegen single-source — устранить hardcoded зеркала | ✅ **ЗАКРЫТ 2026-05-22** — Ф.1–Ф.5 все выполнены |
| [79](79-typecheck-hardening-no-silent-fallback.md) | Plan 79: Type-checker hardening — «no silent fallback» на уровне типов | ✅ **ЗАКРЫТ 2026-05-21** (worktree `nova-p79`, ветка |
| [80](80-must-consume-linear.md) | Plan 80: must-consume — линейные значения (обязательное потребление) | 📋 proposed, не начат. |
| [81](81-module-resolution-hardening.md) | Plan 81: Module-resolution hardening — production-grade резолв модулей | ✅ **ЗАКРЫТ 2026-05-21** — Ф.1–Ф.11 ✅ |
| [82](82-windows-fiber-arena.md) | Plan 82: Windows fiber stack arena — re-diagnosis + production-реализация | ✅ **ЗАКРЫТ ЦЕЛИКОМ (Ф.0–Ф.6, 2026-05-22).** Windows |
| [82.1](82.1-linux-arena-dealloc-investigation.md) | Plan 82.1: Linux fiber arena fiber_dealloc ptr outside arena warnings — investigation + V2 fix plan | ✅ **V1 INVESTIGATION DELIVERED 2026-05-26** — root cause |
| [82.2](82.2-linux-arena-dealloc-fix.md) | Plan 82.2: Linux fiber arena cross-thread dealloc fix — POSIX global arena registry | ✅ **V1 IMPLEMENTED + LINUX VERIFIED 2026-05-26** на ветке |
| [83](83-audit-2026-05-24.md) | Plan 83 — переанализ с чистого листа (2026-05-24) | ✅ audit-документ (readonly research deliverable). |
| [83](83-mn-default-on-gomaxprocs.md) | Plan 83: M:N по умолчанию + GOMAXPROCS-style конфигурация — roadmap-индекс | roadmap — 🟡 в работе. 83.1 ✅ ЗАКРЫТ + 83.3 ✅ ЗАКРЫТ |
| [83](83-mn-runtime-roadmap.md) | Plan 83 — M:N runtime roadmap (umbrella) | 📋 UMBRELLA (M:N-семейство; работа в 83.x sub-планах). |
| [83](83-study-go-c-mn.md) | Plan 83-study-go-c-mn: Port Go 1.4 C-era M:N runtime into Nova | 🟡 Ф.1 в работе; Ф.2-Ф.8 PLANNED. Декомпозиция выведена из research-workflow (11 агентов, Go 1.4 fetch + Nova M:N map + gap-анализ). |
| [83.1](83.1-mn-infrastructure.md) | Plan 83.1: M:N-инфраструктура (pre-flip) | ✅ **ЗАКРЫТ ЦЕЛИКОМ** (Ф.1-Ф.5, 2026-05-22, в main). |
| [83.2](83.2-mn-default-flip.md) | Plan 83.2: Флип дефолта M:N — вкл по умолчанию | 🟡 **Ф.0 ✅ + Ф.1 infrastructure ✅, full flip отложен.** |
| [83.3](83.3-blocking-effect-threadpool.md) | Plan 83.3: Blocking-эффект → libuv threadpool offload | ✅ **ЗАКРЫТ ЦЕЛИКОМ** (2026-05-22). Ф.0 аудит ✅ + |
| [83.4](83.4-mn-supervised-drain-hardening.md) | Plan 83.4: M:N supervised-drain hardening — production-grade umbrella | 🟡 ЧАСТИЧНО — критичные runtime races закрыты, остаток в followup. |
| [83.4.1](83.4.1-d93-park-wake-hardening.md) | Plan 83.4.1: Park-with-predicate primitive — D93 hardening под M:N | 📋 proposed, не начат. |
| [83.4.2](83.4.2-supervised-drain-ownership.md) | Plan 83.4.2: Fiber lifecycle state machine + handler-storage per-fiber миграция | 📋 proposed, не начат. |
| [83.4.3](83.4.3-mn-api-semantic-alignments.md) | Plan 83.4.3: Hierarchical cancellation + yield-semantics + global introspection | 📋 proposed, не начат. |
| [83.4.4](83.4.4-flip-activation-and-83.2-closure.md) | Plan 83.4.4: Активация флипа + production-grade closure (Plan 83.2 Ф.2+Ф.3 + stress/TSAN/bench) | 📋 proposed, не начат. **GATED.** |
| [83.4.5](83.4.5-mn-drain-edge-cases.md) | Plan 83.4.5: M:N drain edge-case sweep — production-grade umbrella | 🟡 roadmap-индекс — **5 / 7 sub-plan'ов ✅ ЗАКРЫТЫ** и СМЁРЖЕНЫ в main (merge `b6afffc8cc2`, 2026-05-23): 83.4.5.1 (cancel wake-all), 83.4.5.2 (detach), 83.4.5.3 (test-suite cleanup), 83.4.5.4 (handler-scoping), 83.4.5.5 (main_yield + NO_AUTOARM). Остаются: 83.4.5.6 (flip reactivat… |
| [83.4.5.1](83.4.5.1-cancel-wake-all-and-cascade.md) | Plan 83.4.5.1: Cancel wake-all parked fibers + scope-tree cascade | ✅ **ЗАКРЫТ 2026-05-23** (commit `ed4bd699719`, merge `b6afffc8cc2`). cancel wake-all + dispatch_ready re-queue для SYNC slots. |
| [83.4.5.2](83.4.5.2-detach-mn-semantics.md) | Plan 83.4.5.2: Detach M:N semantics — detach { … } → worker spawn, не inline | ✅ **ЗАКРЫТ 2026-05-23** (commit `0e0f64bab90`, merge `b6afffc8cc2`). detach_test с `NOVA_NO_AUTOARM=1` directive — semantics закрыт через test-isolation (re-investigation подтвердил, что detach уже корректно spawn'ит worker под M:N, тест нуждался в cooper… |
| [83.4.5.3](83.4.5.3-test-suite-mn-cleanup.md) | Plan 83.4.5.3: Test suite M:N cleanup — set-equality + relaxed precision budgets | ✅ **ЗАКРЫТ 2026-05-23** (commit `f4f2606bd57`, merge `b6afffc8cc2`). set-equality + NOVA_MAXPROCS=1 directives для тестов, чувствительных к worker-ordering. |
| [83.4.5.4](83.4.5.4-handler-scoping-nested.md) | Plan 83.4.5.4: Handler-scoping nested corner cases | ✅ **ЗАКРЫТ 2026-05-23** (commit `2942094f600`, merge `b6afffc8cc2`). spawn-time TLS handler-snapshot capture для M:N inheritance (parent → child). |
| [83.4.5.5](83.4.5.5-main-yield-armed-runtime.md) | Plan 83.4.5.5: runtime.yield() на main thread под armed runtime | ✅ **ЗАКРЫТ 2026-05-23** (commit `c5bb733cceb`, merge `b6afffc8cc2`). `NOVA_NO_AUTOARM=1` escape hatch + main_yield fix (runtime.c:215 — respect env flag). |
| [83.4.5.6](83.4.5.6-flip-reactivation-and-closure.md) | Plan 83.4.5.6: Flip re-activation + Plan 83.2/83.4/83 closure | 🟡 PARTIAL — flip + D138 + infrastructure ✅; speedup |
| [83.4.5.7](83.4.5.7-mn-supervised-double-resume-race.md) | Plan 83.4.5.7: M:N supervised double-resume race deeper fix | 🟡 Ф.1 ✅ DONE; Ф.3+Ф.4 ❌ DEFERRED. |
| [83.4.5.8](83.4.5.8-mn-ctx-gc-reachability.md) | Plan 83.4.5.8: M:N ctx memory visibility / GC reachability fix | ✅ ЗАКРЫТ 2026-05-24. |
| [83.4.5.9](83.4.5.9-rename-noautoarm-env.md) | Plan 83.4.5.9: rename NOVA_NO_AUTOARM → NOVA_AUTOARM (inverted env-name) | 📋 in-progress. |
| [83.4.5.10](83.4.5.10-mn-perf-quick-wins.md) | Plan 83.4.5.10: M:N runtime performance quick wins (Go-style mcache + smaller stack + inline parallel-for) | 🟡 PARTIAL — Ф.3 ✅ DONE (≥1× speedup acceptance MET); Ф.1 + Ф.2 deferred (cancellation_test stack overflow на 1MB; per-worker pool complex). |
| [83.4.5.11](83.4.5.11-test-races-cleanup.md) | Plan 83.4.5.11: Concurrency test race cleanup — rewrite shared-mut к race-free patterns | 🟢 V1 IMPLEMENTED 2026-05-26 — 4 tests fixed, 63/12 → 68/7. |
| [83.4.5.12](83.4.5.12-supervised-nested-fiber-slot-race.md) | Plan 83.4.5.12 — Supervised nested-fiber slot-race (сервер виснет на 2-3-м запросе) | ✅ ЗАКРЫТ 2026-07-15 (opus-рекон+фикс). **Приоритет: P1** (блокер непрерывной работы флагман-сервера). |
| [83.5](83.5-boehm-thread-local-alloc.md) | Plan 83.5: Boehm THREAD_LOCAL_ALLOC — ❌ REJECTED (wrong hypothesis) | ❌ **REJECTED 2026-05-24 same-day** — wrong hypothesis. |
| [83.6](83.6-spawn-ctx-pool.md) | Plan 83.6: Per-worker SpawnCtx free-list pool (Go P-mcache аналог) | 🟡 **V1 IMPLEMENTED 2026-05-25** — pool active, gc_no_leak |
| [83.7](83.7-runnext-lifo-slot.md) | Plan 83.7: runnext LIFO slot — cache-warm handler chains (Go/Tokio parity) | 🟡 **V1 IMPLEMENTED 2026-05-25** — runnext priority slot |
| [83.8](83.8-direct-wake.md) | Plan 83.8: Direct wake primitive — eventfd/SetEvent intra-runtime wake | 📋 proposed. |
| [83.9](83.9-stress-armed-production.md) | Plan 83.9: Armed M:N stress production — закрыть Plan 83.4.5.6 §6.4 acceptance gap | 🟡 **V1 IMPLEMENTED 2026-05-25 (standalone-validated).** |
| [83.10](83.10-negative-concurrency-tests.md) | Plan 83.10: Negative concurrency tests — closure coverage gap | 🟡 **V1 IMPLEMENTED 2026-05-25** — 20 negative tests |
| [83.10.1](83.10.1-autoarm-directive-sweep.md) | Plan 83.10.1: NOVA_AUTOARM=0 directive sweep — verify какие ещё нужны после Plan 83.10 fix | 🟡 **V2 PARTIAL 2026-06-08** — sweep завершён, но 2 файла re-gated |
| [83.10.2](83.10.2-armed-cancel-timer-hang.md) | Plan 83.10.2: Armed M:N cancel-timer-hang fix — cross-thread uv_close dispatch | 🟢 V1 IMPLEMENTED 2026-05-26 — merged `plan-83.10.2-cancel-timer-hang` into main. |
| [83.10.3](83.10.3-nested-armed-routing.md) | Plan 83.10.3: Nested supervised armed M:N routing fix | 📋 proposed — **execution-ready для Sonnet 4.6 + High + Thinking ON**. |
| [83.10.4](83.10.4-armed-mn-remaining-races.md) | Plan 83.10.4: Armed M:N remaining concurrency races — full audit + fix | 📋 proposed — **execution-ready, production-grade без упрощений**. |
| [83.10.4](83.10.4-audit-report.md) | Plan 83.10.4 — Ф.1 Audit Report | — (нет Статус-строки) |
| [83.10.5](83.10.5-iso-cancel-startup-race-fix.md) | Plan 83.10.5: Iso-cancel startup race — fix [M-83.10.4-iso-cancel-startup-race] | 🔴 PARTIAL — Ф.0+Ф.A.1 done (diagnostic confirmed), |
| [83.11](83.11-centralized-io-driver.md) | Plan 83.11: Centralized I/O driver — architectural pivot (Tokio paradigm) | 🟡 Ф.0-Ф.4 ✅ завершены (Ф.3: 2026-05-28, Ф.4: 2026-06-08); Ф.5-Ф.9 pending. |
| [83.11](83.11-deadline-effect.md) | Plan 83.11: Effect-aware deadline propagation — Nova differentiator | 📋 proposed. |
| [83.11](83.11-design.md) | Plan 83.11 — Ф.1 Architecture Design | — (нет Статус-строки) |
| [83.12](83.12-async-net-stdlib.md) | Plan 83.12: Async net/socket stdlib — std/net/{tcp,udp} через libuv | ✅ CLOSED (2026-05-27) — 10/10 тестов PASS; full nova test 1414/62 (62 pre-existing FAILs, no regressions). |
| [83.13](83.13-precise-gc-roadmap.md) | Plan 83.13: Precise GC roadmap-prep — research deliverable (Boehm replacement) | 📋 proposed — **execution-ready для Sonnet 4.6 + High + Thinking ON**. |
| [83.14](83.14-reentrant-mutex-mn-owner.md) | Plan 83.14 — ReentrantMutex owner-tracking под M:N | 📋 proposed — execution-ready для агента. |
| [84](84-relative-imports.md) | Plan 84: Относительные импорты ./ / ../ — package-scoped (D29 rev-4) | ✅ ЗАКРЫТ 2026-05-22 — Ф.1–Ф.6, смёржен в `main` (`4db1b62`) |
| [85](85-builtin-types-test-coverage.md) | Plan 85 — Полное тестовое покрытие встроенных типов и протоколов | ✅ ЗАКРЫТ 2026-05-22 (85.1–85.5) |
| [85.1](85.1-stringbuilder-coverage.md) | Plan 85.1 — Полное тестовое покрытие StringBuilder | ✅ ЗАКРЫТ 2026-05-22 (Ф.1–Ф.4) |
| [85.2](85.2-buffers-coverage.md) | Plan 85.2 — Полное тестовое покрытие ReadBuffer и WriteBuffer | ✅ ЗАКРЫТ 2026-05-22 (Ф.1–Ф.5) |
| [85.3](85.3-conversion-protocols.md) | Plan 85.3 — Полное тестовое покрытие протоколов конверсии | ✅ ЗАКРЫТ 2026-05-22 (Ф.1–Ф.3) |
| [85.4](85.4-comparison-protocols.md) | (нет заголовка) | ✅ ЗАКРЫТ 2026-05-22 (Ф.1–Ф.3) |
| [85.5](85.5-iter-protocol.md) | Plan 85.5 — Полное тестовое покрытие протокола Iter[T] | ✅ ЗАКРЫТ 2026-05-22 (Ф.1–Ф.4) |
| [87](87-for-in-explicit-elem-type.md) | Plan 87 — for-in с явным типом элемента (for x TYPE in iter) | ✅ ЗАКРЫТ 2026-05-22 (Ф.1-Ф.5; ветка `plan-87`) |
| [88](88-generic-static-method-on-typevar.md) | Plan 88 — generic static-method dispatch на type-параметре | ✅ ЗАКРЫТ 2026-05-22 (Ф.0-Ф.4; ветка `plan-88`) |
| [89](89-iflet-match-boxed-sum-ptr.md) | Plan 89 — деструктуризация боксированного sum-элемента (for o in []Option[T]) | ✅ ЗАКРЫТ 2026-05-22 (Ф.0-Ф.3; ветка `plan-89`) |
| [90](90-memory-access-primitives.md) | Plan 90 — примитивы доступа к памяти (byte_at, bulk slice-операции) + аудит FFI / unsafe | ✅ ЗАКРЫТ 2026-05-22 (worktree `nova-p90`, ветка `plan-90`) — Ф.0–Ф.5; вариант A (safe-only); 6/6 фикстур `nova_tests/plan90/` PASS |
| [90.1](90.1-array-extend-family.md) | Plan 90.1 — []T extend-family API + copy_from hardening | ✅ **ЗАКРЫТ 2026-05-27** (Ф.0–Ф.7; worktree `nova-p90-1`; 20/20 plan90_1 PASS; D141 amend; 0 regressions). |
| [91](91-stdlib-mvp-for-0.1.md) | Plan 91 — std MVP для релиза 0.1 | 🟢 Ф.0+Ф.7.1+Ф.4 ЗАКРЫТЫ 2026-05-27; Ф.2.5 (D177) ЗАКРЫТ 2026-05-28; |
| [91.1](91.1-re-baseline.md) | Plan 91.1 — Re-baseline (Ф.0) | — (нет Статус-строки) |
| [91.2](91.2-quarantine.md) | Plan 91.2 — Quarantine non-MVP modules (Ф.7.1) | — (нет Статус-строки) |
| [91.3](91.3-sort-module.md) | Plan 91.3 — Sort module (Ф.4) | — (нет Статус-строки) |
| [91.4](91.4-str-nova-body-dispatch.md) | Plan 91.4 — str Nova-body dispatch (Ф.2.5, D177) | — (нет Статус-строки) |
| [91.5](91.5-str-api-cleanup.md) | Plan 91.5 — str API cleanup + D132 amendment (Ф.2.6, D178) | — (нет Статус-строки) |
| [91.6](91.6-stringbuilder-nova-type.md) | Plan 91.6 — StringBuilder pure Nova consume type (Ф.2.6 sub-phase, D179) | — (нет Статус-строки) |
| [91.7](91.7-array-methods-and-default-new.md) | Plan 91.7 — Array methods# Plan 91.7 — Array methods cleanup + canonical .new() (D180/D181/D182) | convention (stdlib provides, compiler does NOT auto-generate). |
| [91.8a](91.8a-protocol-canon-renames.md) | Plan 91.8a — Protocol canon renames + Ordering removal + default bodies (D183) | — (нет Статус-строки) |
| [91.8b](91.8b-operator-dispatch-protocols.md) | Plan 91.8b — Operator dispatch через protocols (D363, ex-D184) | — (нет Статус-строки) |
| [91.8c](91.8c-generic-array-api.md) | Plan 91.8c — Generic array API# Plan 91.8c — Generic array API: sort/min/max/binary_search + _by (D185) | — (нет Статус-строки) |
| [91.8a.2](91.8a.2-default-body-codegen-and-from-blanket.md) | Plan 91.8a.2 — Default body codegen synthesis + protocols refactor + From identity blanket (D183 amendment) | — (нет Статус-строки) |
| [91.9](91.9-impl-annotation.md) | Plan 91.9 — #impl(Protocol1 + Protocol2) annotation (D186) | — (нет Статус-строки) |
| [91.10](91.10-d163-retract-capability-syntax.md) | Plan 91.10 — Retract D163 needs <Cap> syntax | — (нет Статус-строки) |
| [91.11](91.11-sb-cleanup-rename-steal.md) | Plan 91.11 — StringBuilder cleanup + rename family + zero-copy steal + parser multi-line chain | 🟢 IN PROGRESS 2026-05-30 |
| [91.12](91.12-d126-retract.md) | Plan 91.12 — D126 retract (WriteBuffer/ReadBuffer pure Nova migration) | ✅ V1 CLOSED 2026-06-01 (WriteBuffer + ReadBuffer pure Nova; sync types deferred). |
| [91.12](91.12-net-effect-and-hardening.md) | Plan 91.12 — std/net V2: C FFI layer + Net effects (mockable, handler-dispatched) | Ф.-1 ✅ CLOSED (2026-06-15). Ф.0–Ф.4 ✅ CLOSED (2026-06-16). Ф.5 ✅ CLOSED (2026-06-16). Ф.6 ✅ CLOSED (2026-06-16). Ф.7 ✅ CLOSED (2026-06-16). Ф.8 ✅ CLOSED (2026-06-16). Ф.9 ✅ CLOSED (2026-06-16) — str @as_ptr + DnsNet V1 + TcpListener/TcpStream/UdpSocket consume value. **21/0 PASS.** D294+D295 s… |
| [91.13](91.13-dns-multi-address.md) | Plan 91.13 — DNS Multi-Address API (vtable erasure fix) | — (нет Статус-строки) |
| [91.13](91.13-json-conformance-smoke.md) | Plan 91.13 — JSON Ф.3 conformance smoke (partial) | ✅ **V2 CLOSED 2026-06-05** (test-suite delivered + known |
| [91.14](91.14-debug-printable-and-format-spec.md) | Plan 91.14 — Debug protocol + ${expr:?} format spec | ✅ CLOSED 2026-06-17 (branch plan-91-14). 21/21 PASS. |
| [91.15](91.15-net-polish.md) | Plan 91.15 — std/net API Polish | — (нет Статус-строки) |
| [91.15](91.15-std-api-tuning.md) | Plan 91.15 — std API tuning | ✅ closed (2026-06-17). |
| [91.16](91.16-tcp-split.md) | Plan 91.16 — TcpStream split: TcpReadHalf + TcpWriteHalf | — (нет Статус-строки) |
| [91.17](91.17-udp-split.md) | Plan 91.17 — net.c send_to TOCTOU fix + UDP socket split | — (нет Статус-строки) |
| [91.18](91.18-str-unicode-api.md) | Plan 91.18 — str + unicode API audit & cleanup (v2) | — (нет Статус-строки) |
| [92](92-flaky-mn-test-stabilization.md) | Plan 92 — стабилизация флаки M:N-тестов | ✅ ЗАКРЫТ 2026-05-22 (Ф.0-Ф.3; ветка `plan-92`) |
| [93](93-option-predicates-nova-body.md) | Plan 93 — Option.is_some/is_none как Nova-методы (DeclaredBody routing) | 🟣 **SUPERSEDED by [Plan 95](95-builtin-sum-method-mono.md)** |
| [94](94-str-methods-on-nova.md) | Plan 94 — перенос алгоритмов str в Nova (.nv) | 📋 proposed 2026-05-22, не начат |
| [95](95-builtin-sum-method-mono.md) | Plan 95 — Option/Result как generic-method-able типы (мономорфизация методов builtin sum-типов) | ✅ **ЗАКРЫТ 2026-05-23** (Ф.0–Ф.7 все выполнены; |
| [95.bis](95.bis-option-result-pure-methods-nova-body.md) | Plan 95.bis — Перенос «чистых» Option/Result-методов на Nova-body | ✅ **ЗАКРЫТ 2026-05-23** (Ф.0–Ф.4 все выполнены; |
| [96](96-array-slices.md) | Plan 96 — sub-slice views для []T (production-grade, paritет/лучше Go/Rust/TS) | ✅ **ЗАКРЫТ 2026-05-23** (Ф.1-Ф.7 на ветке `plan-96`; |
| [96.1](96.1-array-slices-followup.md) | Plan 96.1 — followup: W_VIEW_PUSH_DETACH lint + delete str.@slice (bracket-only) | ✅ **ЗАКРЫТ 2026-05-23** (Ф.1-Ф.6 на ветке `plan-96.1`, |
| [97](97-protocol-effect-syntax-symmetry.md) | Plan 97 — protocol/effect syntax: .method static + анон-литерал + handler → effect rename | ✅ ЗАКРЫТ 2026-05-23 (Ф.0/Ф.1/Ф.2/Ф.3 + Ф.4 partial + Ф.6 spec sweep) |
| [97.1](97.1-protocol-literal-codegen.md) | Plan 97.1 — codegen для protocol-литерала (vtable struct + dispatch) | ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p97-1`, ветка `plan-97-1`, |
| [98](98-free-fn-generic-type-param-inference.md) | Plan 98 — Free-fn generic type-param inference на generic-типах (Option/Result/user-generics) | ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p98`, ветка |
| [99](99-option-result-closure-applying-methods-nova-body.md) | Plan 99 — Closure-applying Option/Result методы на Nova-body (master) | ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p99-impl`, |
| [99.1](99.1-method-level-generic-in-declared-body.md) | Plan 99.1 — Method-level generic в DeclaredBody (Option/Result) | ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p99-impl`, ветка `plan-99-impl`). |
| [99.2](99.2-contextual-variant-constructors.md) | Plan 99.2 — Contextual variant constructors | ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p99-impl`, ветка `plan-99-impl`). |
| [99.3](99.3-migrate-6-closure-methods.md) | Plan 99.3 — Migrate 6 closure-applying methods (consumer) | ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p99-impl`, ветка `plan-99-impl`). |
| [99.4](99.4-tests-spec-docs-close.md) | Plan 99.4 — Comprehensive tests + spec + docs + close | ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p99-impl`, ветка `plan-99-impl`). |
| [100](100-linear-must-consume.md) | Plan 100 (umbrella): consume-типы — production-grade «must-be-consumed» | — (нет Статус-строки) |
| [100](100-remaining-impl-roadmap.md) | Plan 100 — Remaining Implementation Roadmap (Sonnet 4.6 launch guide) | — (нет Статус-строки) |
| [100.1](100.1-core-must-consume.md) | Plan 100.1: core static analysis — type-level consume (foundation) | 📋 proposed, не начат, **P3** (foundation; blocks 100.2/3/4). |
| [100.1](100.1-impl-playbook.md) | Plan 100.1 — Implementation Playbook (Sonnet 4.6 minimal-settings ready) | ✅ **CLOSED** в `spec/decisions/02-types.md` D133 (Ред. 2 |
| [100.2](100.2-generic-propagation.md) | Plan 100.2: generic propagation — [T consume] bound + collection-aware | 📋 proposed, **GATED на [100.1](100.1-core-must-consume.md)**. |
| [100.3](100.3-borrow-and-view.md) | Plan 100.3: implicit view default + closure capture + match consume | 📋 proposed (Ред. 2 2026-05-24: drop `view T` keyword, |
| [100.4](100.4-cleanup-on-failure.md) | Plan 100.4 (umbrella): cleanup-on-failure — production-grade defer/errdefer rework | ✅ ЗАКРЫТ 2026-05-26 — все 5 sub-sub-plan'ов завершены. |
| [100.4.1](100.4.1-failable-cleanup-body.md) | Plan 100.4.1: failable cleanup body — defer/errdefer с Fail effect | ✅ ЗАКРЫТ 2026-05-26 (Ф.0-Ф.7, 18/18 PASS). |
| [100.4.2](100.4.2-async-suspend-cleanup.md) | Plan 100.4.2: async/suspend в cleanup body — graceful drain support | ✅ ЗАКРЫТ 2026-05-26 (Ф.0+Ф.1+Ф.3+Ф.5+Ф.6+Ф.7, 11/11 PASS). |
| [100.4.3](100.4.3-okdefer-reason-aware.md) | Plan 100.4.3: okdefer + reason-aware defer \|result\| form | ✅ ЗАКРЫТ 2026-05-25. **GATED на [100.1](100.1-core-must-consume.md)** — gate снят (100.1 merged). |
| [100.4.4](100.4.4-multi-defer-error-accumulation.md) | Plan 100.4.4: multi-defer LIFO error accumulation + panic-in-defer | ✅ ЗАКРЫТ 2026-05-26 (Ф.0+Ф.2+Ф.5+Ф.6+Ф.7, 17/17 PASS). |
| [100.4.5](100.4.5-consume-integration.md) | Plan 100.4.5: consume-integration final — check_consume + defer/errdefer/okdefer + cancel-aware | ✅ ЗАКРЫТ 2026-05-26 (Ф.0+Ф.2+Ф.5+Ф.6+Ф.7, 5/5 PASS — bootstrap MVP Option B). |
| [100.5](100.5-ffi-external-integration.md) | Plan 100.5: FFI / external integration — consume-типы через C-границу | 📋 proposed. **GATED на [100.1](100.1-core-must-consume.md)**. |
| [100.6](100.6-cross-module-integration.md) | Plan 100.6: cross-module + visibility + mangling — consume через границы пакетов | ✅ **ЗАКРЫТ** (2026-05-26). **MERGED** в `main`. |
| [100.7](100.7-stdlib-migration-playbook.md) | Plan 100.7: stdlib migration playbook — реальная миграция консьюм-типов | 📋 proposed. **GATED на 100.1+100.2+100.3+100.4+100.5+100.6**. |
| [100.8](100.8-performance-ide-tooling.md) | Plan 100.8: performance + IDE / tooling — production developer experience | ✅ ЗАКРЫТ 2026-05-26. Merge: [78b954d3a52] на ветке plan-100-8-tooling. |
| [101](101-receiver-generic-prefix.md) | Plan 101 — fn[T] receiver-generic prefix + bounds + protocol composition (master) | 🟡 roadmap — декомпозирован на 5 sub-plan'ов. |
| [101.1](101.1-fn-prefix-core.md) | Plan 101.1 — fn[T] core grammar + codegen + vec.nv migration | — (нет Статус-строки) |
| [101.2](101.2-bound-integration.md) | Plan 101.2 — Bound integration fn[T Hash] (reuse D72) | — (нет Статус-строки) |
| [101.3](101.3-multi-bound.md) | Plan 101.3 — Multi-bound [T A + B] (closes Q-multi-bound) | — (нет Статус-строки) |
| [101.4](101.4-protocol-composition.md) | Plan 101.4 — Protocol composition use A, B (closes D53 open question) | — (нет Статус-строки) |
| [101.5](101.5-stdlib-audit-close.md) | Plan 101.5 — Stdlib audit + LSP quick-fixes + close | — (нет Статус-строки) |
| [103](103-sync-primitives-spec-formalization.md) | Plan 103 — std.runtime.sync production-grade spec + API expansion (roadmap) | 🟢 V1 ЗАКРЫТ 2026-05-27 (103.1-103.8 ✅; V2=103.9 gated на Plan 100.7) |
| [103.1](103.1-memory-ordering-api.md) | Plan 103.1 — Memory ordering API foundation (Q-memory-model closure) | ✅ ЗАКРЫТ 2026-05-25, смёржен в main (plan-103.1) |
| [103.2](103.2-atomics-full-suite.md) | Plan 103.2 — Atomics full suite (sized I8-I64 / U8-U64 / Usize / Bool / Ptr) | ✅ ЗАКРЫТ 2026-05-25 — merge 69d7605cc1c; 17/17 tests PASS |
| [103.3](103.3-mutex-family.md) | Plan 103.3 — Mutex / RwLock / ReentrantMutex family | ✅ **ЗАКРЫТ 2026-05-26** — Mutex extensions (`try_lock_for`/`is_locked`/`with_lock`/`new_unfair`, unlock invariant via Nova_Fail_fail), RwLock (writer-priority default M7 + reader-priority opt-in), ReentrantMutex (mco_running owner tracking). 25/25 PASS (10 + 8 + 4 + 3 prop). D169 в `spec/decisions/06-concurr… |
| [103.4](103.4-coordination-primitives.md) | Plan 103.4 — Coordination primitives (Semaphore / Barrier / CountDownLatch / Condvar) | ✅ **ЗАКРЫТ 2026-05-27** — все 4 примитива (Semaphore/Barrier/CountDownLatch/Condvar) реализованы через parallel-agent split (4 Sonnet 4.6 sub-agents + Opus 4.7 final merge); 25/25 plan103_4 PASS; D170 draft в spec; разблокирует Plan 103.6. |
| [103.5](103.5-once-lazy-oncecell.md) | Plan 103.5 — Once hardening + Lazy[T] + OnceCell[T] | ✅ **ЗАКРЫТ 2026-05-26** — Once hardening (`call_once(fn)` closure-form primary M8) + `OnceCell[T]` (get/set/get_or_init/take) + `Lazy[T]` (auto-init wrapper, M9 distinct poison semantics). 20/20 PASS (11 pos + 3 neg + 2 prop + 1 stress в `nova_tests/plan103_5/`). D171 в `spec/decisions/06-concurrency.md`. M… |
| [103.6](103.6-realtime-blocking-integration.md) | Plan 103.6 — realtime { } / blocking { } integration для sync primitives | ✅ ЗАКРЫТ 2026-05-27 (Plan 103.6 Ф.0-Ф.6 complete) |
| [103.7](103.7-spec-d-blocks.md) | Plan 103.7 — Spec D-blocks (D167-D173) + AI-first guidance + open-questions cleanup | ✅ ЗАКРЫТ 2026-05-27 |
| [103.8](103.8-audit-report.md) | Plan 103.8 — Three-Way Consistency Audit Report | — (нет Статус-строки) |
| [103.8](103.8-conformance-and-close.md) | Plan 103.8 — Conformance + stress + litmus + audit + close | ✅ ЗАКРЫТ 2026-05-27 |
| [103.9](103.9-consume-guards-migration.md) | Plan 103.9 — Consume guards migration V2 (GATED on Plan 100.7) | ✅ ЗАКРЫТ 2026-05-28 — 20/20 plan103_9 PASS + 4 pilot concurrency fixtures PASS; D174 spec shipped |
| [104](104-ide-integration.md) | Plan 104 — Production-grade IDE integration (LSP server + tree-sitter + editor distributions) | ✅ **ЗАКРЫТ 2026-06-17** — все 9 sub-plans (104.0–104.9) выполнены. |
| [104.1](104.1-lsp-diagnostics.md) | Plan 104.1 — LSP Diagnostics (production-grade) | 📋 proposed 2026-05-26, не начат |
| [104.2](104.2-hover-goto-sigp.md) | Plan 104.2 — Hover + Goto-definition + Signature Help | ✅ ЗАКРЫТ 2026-06-17 (Ф.7 body-walk hover добавлен) |
| [104.3](104.3-completion.md) | Plan 104.3 — LSP Completion (Keywords + Identifiers + Methods + Imports) | ✅ ЗАКРЫТ 2026-06-16 — 167 tests PASS (52 completion-specific), branch plan-104-3 |
| [104.4](104.4-symbols-references.md) | Plan 104.4 — Document/Workspace Symbols + Find-References | ✅ ЗАКРЫТ 2026-06-16 |
| [104.5](104.5-code-actions.md) | Plan 104.5 — Code Actions / Quick-fixes (≥25) — Production-grade | ✅ ЗАКРЫТ 2026-06-16 |
| [104.6](104.6-rename-format.md) | Plan 104.6 — Rename + Format-on-save (nova-lsp) | 🟡 IN PROGRESS |
| [104.7](104.7-tree-sitter-grammar-playbook.md) | Plan 104.7 — Sonnet 4.6 execution playbook (Ф.1-Ф.7) | — (нет Статус-строки) |
| [104.7](104.7-tree-sitter-grammar.md) | Plan 104.7 — Tree-sitter grammar для Nova (tree-sitter-nova) | 📋 proposed 2026-05-25, не начат |
| [104.8](104.8-editor-packaging.md) | Plan 104.8 — Editor packaging & distribution (production-grade) | ✅ ЗАКРЫТ 2026-05-26 (Ф.1-Ф.7 все завершены) |
| [104.9](104.9-syntax-highlight-keyword-sync.md) | Plan 104.9 — Syntax-highlighting keyword sync + conformance guard | ✅ DONE (2026-06-14) |
| [104.10](104.10-lsp-v2-production.md) | Plan 104.10 — LSP V2: production-grade gap closure (parity с 7 LSP-пирами) | ✅ **ВЫПОЛНЕН 2026-07-04** — все 24 фазы (BLOCK A+B+C) + Ф.14 close-out. Ветка `plan-104-10`. См. «## Статус выполнения» ниже. |
| [105](105-sum-type-explicit-base.md) | Plan 105 — type X u8 \| A = 0 \| B = 1 парсер + codegen для явного базового типа sum'ов | 📋 proposed, **P2**. |
| [106](106-if-let-chains.md) | Plan 106 — Guard-условие && в if/while pattern-bind | ✅ CLOSED 2026-06-17. |
| [107](107-prelude-attribute-syntax.md) | Plan 107 — Prelude attribute syntax migration# Plan 107 — Prelude attribute syntax migration | ✅ CLOSED 2026-05-27 |
| [108](108-readonly-type-modifier.md) | Plan 108: readonly field enforcement (D175) + readonly T type modifier (D176) | 🟡 Ф.1+Ф.2+Ф.3 ✅ реализованы, Ф.4 closure в процессе |
| [108.1](108.1-params-readonly-default.md) | Plan 108.1 — Параметры readonly по умолчанию | — (нет Статус-строки) |
| [108.2](108.2-locals-readonly-default.md) | Plan 108.2 — Локальные let без mut = read-only (enforcement D36) | — (нет Статус-строки) |
| [108.3](108.3-loop-pattern-mut-residual.md) | Plan 108.3 — Loop-var mut + pattern-binding mut + residual migrations | — (нет Статус-строки) |
| [108.4](108.4-protocol-method-receiver-mut.md) | Plan 108.4 — Protocol method @ + receiver mutability (mut @/@/consume @ + impl enforcement) | 🆕 PLANNED. |
| [110](110-scoped-resources-radical-simplification.md) | Plan 110: consume-scope — radical simplification cleanup-семейства | ✅ ЗАКРЫТ (2026-06-01) — merge `874f5766ca5` в main |
| [110.1](110.1-core-protocol-syntax-codegen.md) | Plan 110.1: Core — Consumable[E] protocol + syntax + codegen (Ф.0-2) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge |
| [110.2](110.2-cancel-shield-async-cleanup.md) | Plan 110.2: Cancel-shield + async cleanup + 3-level timeout (Ф.3) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge |
| [110.3](110.3-stdlib-migration.md) | Plan 110.3: Stdlib migration to Consumable[E] (Ф.4 + Ф.5) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge `874f5766ca5`). Phases shipped через atomic commits; close summary в `1c09f315f02`. Fixtures в `nova_tests/plan110/` верифицируют integrated behavior. Status update metadata-only commit 2026-06-03. |
| [110.4](110.4-multierror-cleanup-app-effects.md) | Plan 110.4: MultiError typed + Cleanup effect + Application effect (Ф.6 + Ф.7 + Ф.8) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge `874f5766ca5`). Phases shipped через atomic commits; close summary в `1c09f315f02`. Fixtures в `nova_tests/plan110/` верифицируют integrated behavior. Status update metadata-only commit 2026-06-03. |
| [110.5](110.5-migration-autofix.md) | Plan 110.5: Migration deprecation + auto-fix tool (Ф.9) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge `874f5766ca5`). Phases shipped через atomic commits; close summary в `1c09f315f02`. Fixtures в `nova_tests/plan110/` верифицируют integrated behavior. Status update metadata-only commit 2026-06-03. |
| [110.6](110.6-diagnostic-lsp-stress-bench.md) | Plan 110.6: Diagnostic UX + LSP + Stress + Benchmarks (Ф.10 + Ф.11) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge `874f5766ca5`). Phases shipped через atomic commits; close summary в `1c09f315f02`. Fixtures в `nova_tests/plan110/` верифицируют integrated behavior. Status update metadata-only commit 2026-06-03. |
| [110.7](110.7-ffi-integration.md) | Plan 110.7: FFI integration with Consumable (Ф.12) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge `874f5766ca5`). Phases shipped через atomic commits; close summary в `1c09f315f02`. Fixtures в `nova_tests/plan110/` верифицируют integrated behavior. Status update metadata-only commit 2026-06-03. |
| [110.8](110.8-docs-close.md) | Plan 110.8: Regression + cross-platform + docs finalize + close (Ф.13 + Ф.14) | ✅ ЗАКРЫТ (2026-06-01) — sub-plan Plan 110 umbrella (merge `874f5766ca5`). Phases shipped через atomic commits; close summary в `1c09f315f02`. Fixtures в `nova_tests/plan110/` верифицируют integrated behavior. Status update metadata-only commit 2026-06-03. |
| [110.9](110.9-v1.1-production-grade-closure.md) | Plan 110.9 — V1.1 Production-Grade Closure | — (нет Статус-строки) |
| [110.10](110.10-existing-type-consumable-wrappers.md) | Plan 110.10 — Existing-type Consumable[E] wrappers | 🆕 PLANNED. |
| [110.11](110.11-new-stdlib-types-consumable.md) | Plan 110.11 — New stdlib types + Consumable[E] impls (umbrella) | 🆕 PLANNED. |
| [110.12](110.12-cross-cutting-orphan-closures.md) | Plan 110.12 — Cross-cutting orphan markers + scheduler investigation | 🆕 PLANNED. |
| [113](113-realtime-blocking-attribute-only.md) | Plan 113 — #realtime / #blocking attribute-only simplification | 🆕 PLANNED. |
| [114](114-keyword-refresh-ro-mut-no-let.md) | Plan 114 — Keyword refresh: ro/mut/consume bindings, drop let, narrow + generalize const (data + fn), rename readonly → ro | 🆕 PLANNED. |
| [114.4](114.4-const-narrow-generalize-fn.md) | Plan 114.4 — const narrow + generalize + const fn (extracted from Plan 114 Ф.9-Ф.11) | 🆕 PLANNED. |
| [114.4.1](114.4.1-associated-constants.md) | Plan 114.4.1 — Associated constants (const field в type X) | 🆕 PLANNED. |
| [114.4.2](114.4.2-const-fn.md) | Plan 114.4.2 — const fn — comptime evaluable functions | 🆕 PLANNED. |
| [114.4.3](114.4.3-const-fn-v2-extensions.md) | Plan 114.4.3 — const fn V2 extensions (5 followup markers) | 🆕 PLANNED. |
| [114.4.4](114.4.4-const-fn-v3-completion.md) | Plan 114.4.4 — const fn V3 completion (9 followup markers) | 🆕 PLANNED. |
| [114.4.4.1](114.4.4.1-pattern-record-sum.md) | Plan 114.4.4.1 — Record/sum patterns в const fn match (V2.1 extension) | 🆕 PLANNED. |
| [114.4.4.2](114.4.4.2-t-reflection.md) | Plan 114.4.4.2 — Type reflection в const fn (size_of[T]/align_of[T]) | 🆕 PLANNED. |
| [114.4.4.3](114.4.4.3-runtime-hof.md) | Plan 114.4.4.3 — Runtime HOF через trampoline для const fn | 🆕 PLANNED. |
| [114.4.4.4](114.4.4.4-closure-from-const-fn.md) | Plan 114.4.4.4 — Closure-returning const fn | 🆕 PLANNED. |
| [114.4.4.5](114.4.4.5-mono-specialization.md) | Plan 114.4.4.5 — True per-const-arg monomorphization | 🆕 PLANNED. |
| [114.4.4.6](114.4.4.6-v4-followups.md) | Plan 114.4.4.6 — V4 followups bundle | 🟡 PARTIAL — Ф.1 + Ф.2 LANDED; Ф.3 + Ф.4 DEFERRED (design). |
| [114.4.4.7](114.4.4.7-v4-6-final-followups.md) | Plan 114.4.4.7 — V4 final followups bundle (V4.6) | 🚧 IN PROGRESS. |
| [115](115-ptr-type-and-tuple-ffi.md) | Plan 115 — Foundational FFI: ptr type + tuple-return FFI + opaque handle pattern | 🆕 PLANNED. |
| [116](116-std-tls-effect.md) | Plan 116 — std/tls: TLS-слой поверх TcpStream (mbedTLS C-шим; HTTPS-энейблер) | ✅ ЗАКРЫТ 2026-07-11 — **https-prod-ready ЯДРО ГОТОВО:** mbedTLS-бэкенд |
| [118](118-typed-pointers-and-unsafe.md) | Plan 118 — Typed pointers (*T family) + unsafe model (core) | 🟡 PARTIAL — V1 core + `addr_of` + `as_cstr` + `AtomicPtr` + `Debug` landed (2026-06-01–09); 37/40 plan118 PASS (3 pre-existing runtime). Deferred: `addr_of!` macro syntax, `cstr"..."` literal, `AtomicPtr[T]` generic refactor — текущие реализации через `int`-proxy или функции. |
| [118.1](118.1-ffi-intrinsics-and-cstring.md) | Plan 118.1 — FFI memory intrinsics + C-string convention | 🆕 PLANNED (refined). |
| [118.1.5](118.1.5-unsafe-attr-on-external-fn.md) | Plan 118.1.5 — #unsafe attribute on external fn | 🆕 PLANNED. |
| [118.1.6](118.1.6-unsafe-fn-pointer-type.md) | Plan 118.1.6 — #unsafe как часть типа function pointer (*fn(...)) | 🆕 PLANNED. |
| [118.1.7](118.1.7-unsafe-fn-keyword-syntax.md) | Plan 118.1.7 — unsafe fn как часть типа: миграция с #unsafe атрибута | 📋 PLANNED. |
| [118.2](118.2-slice-fat-pointer-and-uninit.md) | Plan 118.2 — []T @as_ptr extraction + MaybeUninit (NO Slice[T], NO ManuallyDrop) | 🆕 PLANNED (refined). |
| [118.3](118.3-pointer-concurrency-safety.md) | Plan 118.3 — Pointer concurrency safety + AtomicPtr integration | 🆕 PLANNED (refined). |
| [118.5](118.5-right-binding-rule-migration.md) | Plan 118.5 — universal right-binding rule + unsafe T first-class migration | 🆕 PLANNED. |
| [118.6](118.6-addr-of-safe-model.md) | Plan 118.6 — Safe &x model + убрать addr_of/addr_of_mut | ✅ ЗАКРЫТ 2026-06-17. **Приоритет:** P1. |
| [118.7](118.7-raw-addr-of-syntax.md) | Plan 118.7 — raw &x syntax для сырого стек-адреса | ✅ CLOSED. **Приоритет:** P1. |
| [120](120-named-tuples-and-allocation-contract.md) | Plan 120 — Named tuple fields + value/reference allocation contract | ✅ ЗАКРЫТ (2026-06-01). Branch `plan-120` pushed для review. |
| [121](121-stack-fixed-arrays.md) | Plan 121 — Stack-allocated fixed-size arrays [N]T | 📋 PLANNED |
| [123](123-baseline-test-pattern-fix.md) | Plan 123 — Baseline Test Pattern Fix (..Default::default() spread) | — (нет Статус-строки) |
| [123](123-followups-2026-06-04.md) | Plan 123 V*.followups (2026-06-04) — umbrella for 5 open markers | — (нет Статус-строки) |
| [123](123-followups-2026-06-05.md) | Plan 123 V*.followups (2026-06-05) — umbrella for 7 open markers | — (нет Статус-строки) |
| [123](123-receiver-field-cse.md) | Plan 123 — Method-local field load optimization (umbrella) | — (нет Статус-строки) |
| [123](123-v2-followups.md) | Plan 123 V*.2 Followups — Umbrella | — (нет Статус-строки) |
| [123.1](123.1-core-cse.md) | Plan 123.1 — Method-local receiver field caching V1 (Core CSE) | — (нет Статус-строки) |
| [123.1.1](123.1.1-mut-multi-region.md) | Plan 123.1.1 — Multi-region mut cache (V1.1) | — (нет Статус-строки) |
| [123.1.2](123.1.2-nested-regions.md) | Plan 123.1.2 — Nested-region mut cache (V1.2) | — (нет Статус-строки) |
| [123.2](123.2-licm.md) | Plan 123.2 — LICM (Loop-Invariant Code Motion) для receiver fields | — (нет Статус-строки) |
| [123.2.1](123.2.1-loop-body-coord.md) | Plan 123.2.1 — Loop-body LICM coordination (V2.1) | — (нет Статус-строки) |
| [123.3](123.3-pure-call-cache.md) | Plan 123.3 — Pure call result caching (effect-aware, Nova edge) | — (нет Статус-строки) |
| [123.3.1](123.3.1-pure-literal-args.md) | Plan 123.3.1 — Pure-call cache with literal args | — (нет Статус-строки) |
| [123.3.2](123.3.2-tuple-record-literal-args.md) | Plan 123.3.2 — Pure-call tuple/record literal args (V3.2) | — (нет Статус-строки) |
| [123.4](123.4-chain-cache.md) | Plan 123.4 — Chain caching @a.b.c | — (нет Статус-строки) |
| [123.4.2](123.4.2-chain-prefix-sharing.md) | Plan 123.4.2 — Chain prefix sharing (V4.2) | — (нет Статус-строки) |
| [123.4.3](123.4.3-deep-prefix-sharing.md) | Plan 123.4.3 — Deep chain prefix sharing (V4.3) | — (нет Статус-строки) |
| [123.4.4](123.4.4-codegen-chain-root-temp.md) | Plan 123.4.4 — Codegen fluent-chain root-temp pre-pass | — (нет Статус-строки) |
| [123.5](123.5-lsp-diag.md) | Plan 123.5 — LSP code-lens + diagnostic mode | — (нет Статус-строки) |
| [123.5.1](123.5.1-lsp-integration.md) | Plan 123.5.1 — LSP code-lens + hover provider | — (нет Статус-строки) |
| [123.5.2](123.5.2-semantic-tokens.md) | Plan 123.5.2 — LSP semantic tokens (V5.2) | — (нет Статус-строки) |
| [123.5.3](123.5.3-pure-quickfix.md) | Plan 123.5.3 — LSP quickfix: add #pure (V5.3) | — (нет Статус-строки) |
| [123.5.4](123.5.4-explain-deep-walk.md) | Plan 123.5.4 — Explain deep-walk (V5.4) | — (нет Статус-строки) |
| [123.5.5](123.5.5-incremental-semantic-tokens.md) | Plan 123.5.5 — Incremental LSP semantic-tokens delta (V5.5) | — (нет Статус-строки) |
| [123.6](123.6-telemetry.md) | Plan 123.6 — Telemetry + production rollout + CLI flags | — (нет Статус-строки) |
| [123.6.1](123.6.1-cli-flags-ci-gates.md) | Plan 123.6.1 — Sugar CLI flags + CI perf gates | — (нет Статус-строки) |
| [123.6.2](123.6.2-plan57-bench.md) | Plan 123.6.2 — Plan 57 nova bench integration (V6.2) | — (нет Статус-строки) |
| [123.6.2.1](123.6.2.1-real-wallclock-bench.md) | Plan 123.6.2.1 — Real wall-clock bench (V6.2.1) | — (нет Статус-строки) |
| [123.6.3](123.6.3-configurable-thresholds.md) | Plan 123.6.3 — Configurable gate thresholds (V6.3) | — (нет Статус-строки) |
| [123.7](123.7-ipa.md) | Plan 123.7 — IPA (Inter-Procedural Analysis) | — (нет Статус-строки) |
| [123.7.1](123.7.1-ipa-full-integration.md) | Plan 123.7.1 — IPA full integration | — (нет Статус-строки) |
| [123.7.2](123.7.2-explicit-ipa-threading.md) | Plan 123.7.2 — Explicit IpaCtx parameter threading | — (нет Статус-строки) |
| [123.7.3](123.7.3-scc-closure.md) | Plan 123.7.3 — SCC-based exact closure (V7.3) | — (нет Статус-строки) |
| [123.7.4](123.7.4-incremental-scc.md) | Plan 123.7.4 — Incremental SCC cache (V7.4) | — (нет Статус-строки) |
| [123.7.5](123.7.5-callee-non-self-ipa.md) | Plan 123.7.5 — Callee-non-self-mutation IPA (V7.5) | — (нет Статус-строки) |
| [123.7.6](123.7.6-same-field-ref-type.md) | Plan 123.7.6 — Same-field reference-type IPA (V7.6) | — (нет Статус-строки) |
| [123.7.7](123.7.7-chain-receiver.md) | Plan 123.7.7 — Chain receiver IPA extension (V7.7) | — (нет Статус-строки) |
| [124](124-priv-field-visibility.md) | Plan 124 — Private field visibility (priv modifier для records + named tuples) | — (нет Статус-строки) |
| [124.2](124.2-pattern-literal-edges.md) | Plan 124.2 — Pattern destructure + literal init edge cases | — (нет Статус-строки) |
| [124.3](124.3-generics-uniform.md) | Plan 124.3 — Generics uniform handling | — (нет Статус-строки) |
| [124.6](124.6-friend-attrs.md) | Plan 124.6 — Test access escape + #visible_to friend attrs (D224) | — (нет Статус-строки) |
| [124.8](124.8-tuple-value-refine.md) | Plan 124.8 — Tuple+Value-Record design refinement | — (нет Статус-строки) |
| [124.9](124.9-nested-struct-literal-codegen.md) | Plan 124.9 — Nested struct-literal codegen (field-value type inference) | — (нет Статус-строки) |
| [125](125-divergence-aware-inference.md) | Plan 125 — Divergence-aware result-type inference (never bottom-type subtype propagation) | ✅ V1 CLOSED + MERGED + PUSHED 2026-06-05 (merge `d27f3341a0c`) |
| [126](126-auto-derive-protocols.md) | Plan 126 — Auto-derive протоколов через #impl(...) annotation | — (нет Статус-строки) |
| [126.2](126.2-codegen-method-table.md) | Plan 126.2 — Codegen method_table integration для auto-derived protocols | — (нет Статус-строки) |
| [127](127-value-record-escape-and-auto-promote.md) | Plan 127 — Value-record escape analysis + auto-heap-promote | — (нет Статус-строки) |
| [127.1](127.1-codegen-promoted-field-access.md) | Plan 127.1 — Codegen: field access через ValueHeapPromoted pointer | — (нет Статус-строки) |
| [128](128-mut-receiver-abi.md) | Plan 128 — mut @method receiver ABI: recv.mutable wiring + D215 NamedTuple pointer + E_PRIMITIVE_MUT_METHOD | — (нет Статус-строки) |
| [128.1](128.1-v1-limitations-fix.md) | Plan 128.1 — V1 limitations fix: lvalue-projection mut @method + []NamedTuple element type inference | — (нет Статус-строки) |
| [128.2](128.2-ro-binding-mut-chain.md) | Plan 128.2 — ro binding + mut-method через field/index chain: type-checker enforcement | — (нет Статус-строки) |
| [129](129-codegen-decomposition.md) | План 129 — Декомпозиция кодогенератора (разбить emit_c.rs / types/mod.rs / parser/mod.rs / field_cache.rs) | 📋 ЧЕРНОВИК 2026-06-06 (предложен, НЕ запланирован) |
| [130](130-human-facing-docs.md) | План 130 — Документация для людей: обзорная точка входа + актуализация | 📋 ЧЕРНОВИК 2026-06-06 (предложен, НЕ запланирован) |
| [131](131-vec-in-nova.md) | Plan 131 — Vec[T] implemented in Nova | 📋 PLANNED 2026-06-08 |
| [132](132-remove-bound-method-value.md) | Plan 132 — Убрать bound method value obj.@method; разрешить field/method одного имени | ✅ ЗАКРЫТ 2026-06-09. |
| [133](133-remove-usize-isize.md) | Plan 133 — Удалить usize/isize; int = адресное целое везде | ✅ CLOSED 2026-06-09. |
| [134](134-remove-ptr-type.md) | Plan 134 — Удалить встроенный тип ptr; заменить на *() | ✅ CLOSED 2026-06-09 (Ф.1–Ф.3 merged в main); |
| [135](135-receiver-mut-overload-dispatch.md) | Plan 135 — Dispatch по receiver-mutability для одноимённых методов | ✅ ЗАКРЫТ 2026-06-09. |
| [136](136-tuple-destructuring-assignment.md) | Plan 136 — Tuple destructuring assignment | ✅ ЗАКРЫТ 2026-06-09. |
| [136.1](136.1-cycle-decomp.md) | Plan 136.1 -- Tuple assign codegen V2: cycle-decomposition | — (нет Статус-строки) |
| [137](137-protocol-rename-drop-able-suffix.md) | Plan 137 — Protocol rename: drop -able suffix | ✅ ЗАКРЫТ 2026-06-09. |
| [138](138-array-sugar-index-protocol.md) | Plan 138 — []T sugar over Vec[T] + Index protocol | ✅ ЗАКРЫТ (Ф.1-Ф.4, 2026-06-10). Ф.5-Ф.6 deferred → `[M-138-array-sugar-alias]`. |
| [138.1](138.1-array-sugar-alias.md) | Plan 138.1 — []T → Vec[T] pure-Nova sugar (D239) | ✅ CLOSED-PARTIAL (2026-06-10) — флип `[]T`→Vec (gated) + |
| [138.2](138.2-vec-in-prelude-novarray-retirement.md) | Plan 138.2 — Universal Vec + str-in-Nova + NovaArray retirement | — (нет Статус-строки) |
| [138.3](138.3-clone-deep-semantics.md) | Plan 138.3 — Clone protocol = deep/recursive (collections element-wise) | ✅ CLOSED ПОЛНОСТЬЮ — spec-complete + deep-collection-clone |
| [138.4](138.4-generic-method-codegen-hardening.md) | Plan 138.4 — Generic-method codegen hardening (unblock 4-marker cluster) | ✅ CLOSED (все 4 gap landed, 2026-06-11).  **Эстимат:** ~2-4 dev-day (deep .rs codegen). |
| [138.5](138.5-d216-v2-v3-simplification.md) | Plan 138.5 — D216 V2/V3 simplification (pointer model: pointee-mut only, no prefix modifiers) | ✅ ЗАКРЫТ Ф.1-Ф.5 (2026-06-11, branch `plan-138.1`). |
| [139](139-str-as-nova-value-type.md) | Plan 139 — str as a Nova value type ({ ptr *ro u8, len int }) | ✅ **CLOSED (2026-06-11)** — все 8 фаз |
| [139.1](139.1-str-lang-item-decl.md) | Plan 139.1 — str lang-item declaration (complete E1/E4; close [M-139-f0-lang-item-decl]) | ✅ **CLOSED (2026-06-12)** — Ф.A/Ф.C приземлены (lang-item decl + privacy + content-eq + 3 neg-фикстуры), Ф.B = VERIFY-OR-DOCUMENT (0 из 10 методов мигрируемо сегодня, sequencing-gated). **E1 → ✅ FULL** (декларация + privacy fires + ABI-alias + 3 neg). **E… |
| [139.2](139.2-str-methods-full-nova-migration.md) | Plan 139.2 — Full str-method Nova migration | — (нет Статус-строки) |
| [140](140-contracts-enforced-in-release.md) | Plan 140 — Contracts enforced in release (Z3-proven elided, unproven checked) | ✅ CLOSED Ф.0-Ф.5 (2026-06-12, branch `plan-140`, НЕ merged). |
| [140.1](140.1-contract-custom-message.md) | Plan 140.1 — Contract & assert diagnostics (short location-first format + custom message) | 📋 PLANNED (gated на Plan 140). |
| [140.2](140.2-vec-bounds-as-contract.md) | Plan 140.2 — Vec @index bounds as elidable contract (prerequisite-first) | ✅ CLOSED 2026-06-13 (Part A + Part B; см. §«Статус» ниже). |
| [140.4](140.4-overflow-check-elision.md) | Plan 140.4 — Proven-safe int-overflow check elision | — (нет Статус-строки) |
| [141](141-structural-equality-field-by-field.md) | Plan 141 — Structural equality field-by-field (fix memcmp tuple/sum eq) | ✅ CLOSED (Ф.1-Ф.3, 2026-06-11).  **Эстимат:** ~0.5-1 dev-day (.rs codegen + tests). |
| [142](142-d227-literal-range-enforcement.md) | Plan 142 — D227 literal range enforcement (E_LIT_OUT_OF_RANGE) | ✅ CLOSED (Ф.1–Ф.3, 2026-06-11).  **Эстимат:** ~0.5 dev-day. |
| [143](143-deferred-enhancements-umbrella.md) | Plan 143 — Deferred enhancements umbrella (post-138) | 📋 PLANNED (backlog umbrella; future, не imminent). |
| [143.2](143.2-leaf-preempt-entry-elision.md) | Plan 143.2 — Leaf function-entry preempt-check elision  [M-opt-leaf-preempt-entry-elision] | ✅ **DONE** (Ф.0–Ф.5, 2026-06-14). |
| [144](144-precise-gc-implementation.md) | Plan 144: Precise GC implementation — Boehm replacement | 🟡 DECOMPOSED — механизм выбран (Henderson shadow-stack); фазы Ф.0–Ф.8 (non-moving → moving), см. §7–§8. |
| [144.0](144.0-may-gc-effect-analysis.md) | Plan 144.0: may-GC effect analysis (Ф.0 prerequisite — closes H4/Q15) | ✅ DONE (2026-06-14) — все фазы Ф.0–Ф.8 закрыты; emit-nothing верифицирован. |
| [144.0.1](144.0.1-gate-frame-abi-object-start.md) | Plan 144.0.1: Ф.0 GATE — shadow-frame ABI freeze + object-start lookup + H3/H5 + roots registry | 🔴 NOT STARTED — **design+spec GATE**, без кода (кроме уже доставленного |
| [144.1](144.1-heap-layout-bitmaps.md) | Plan 144.1: Heap layout bitmaps (per-type pointer-offset карты) | ✅ DONE (аналитическая / emit-nothing половина) 2026-06-15, ветка |
| [144.2](144.2-shadow-frame-codegen.md) | Plan 144.2: Codegen shadow-frame (non-moving) + тир O1 | 🔴 NOT STARTED. |
| [144.3](144.3-runtime-precise-root-scan.md) | Plan 144.3: Runtime precise root-scan | 🔴 NOT STARTED. |
| [144.4](144.4-safepoint-completeness.md) | Plan 144.4: Safe-point completeness | 🔴 NOT STARTED. |
| [144.5](144.5-nonmoving-precise-gc-online.md) | Plan 144.5: Non-moving precise GC online ✦ (ВЕХА) | 🔴 NOT STARTED — ✦ пользовательская веха (milestone). |
| [144.6](144.6-regions-bump-arenas.md) | Plan 144.6: Regions / bump-арены (вместо general moving) ⚠ | 🔴 NOT STARTED — ⚠ ПЕРЕСМОТРЕНО (см. §7.6 moving-вердикт). |
| [144.7](144.7-growable-fiber-stacks.md) | Plan 144.7: Растущие стеки (GC-сторона) ✦ | 🔴 NOT STARTED — ✦ пользовательская веха (milestone). |
| [144.8](144.8-generational-concurrent-groundwork.md) | Plan 144.8: Generational / concurrent groundwork | 🔴 NOT STARTED — **Post-v1.0, опционально / deferred**. |
| [145](145-msvc-codegen-portability.md) | Plan 145: MSVC codegen portability — bounds-check stmt-expr → portable | 🟢 CODEGEN PORTABILITY ЗАКРЫТА + MSVC ВОССТАНОВЛЕН (majority) — 2026-06-14, |
| [145.2](145.2-codegen-emission-determinism.md) | Plan 145.2: Codegen emission determinism | 🟢 **ЗАКРЫТ + СМЁРЖЕН в main** (`5c11bce8`, FF 2026-06-15) + разблокировал Plan 145.1 — ветка `plan-145.2`. |
| [146](146-growable-fiber-stacks.md) | Plan 146: Growable fiber stacks — lift the ~16k concurrent-fiber ceiling | 📋 PROPOSED — research-first (segmented vs copying — реальная развилка). |
| [147](147-pointer-mut-flip-scan-model.md) | Plan 147 — Three-axis mutability model (supersede flip-scan / D245) | ✅ **CLOSED** (Ф.1-Ф.7 LANDED; 3-axis модель D246 в spec+parser+checker+codegen; codebase мигрирован; oracle 37/0). **Worktree:** `nova-p147` @ `plan-147-f7`. |
| [148](148-independent-cleanups.md) | Plan 148 — Independent compiler cleanups | — (нет Статус-строки) |
| [149](149-configurable-fiber-arena.md) | Plan 149 — Configurable fiber arena (stack size + max fibers): env + nova.toml | ✅ ЗАКРЫТ Ф.0-Ф.6 (2026-06-12, D233). 7/7 plan149 fixtures PASS (clang); |
| [150](150-chained-comparison-relational-safety.md) | Plan 150 — Reject chained comparison + ban bool relational operands (Rust-style) | 📋 PLANNED.  **Приоритет:** P1 (security: вакуумные контракты). |
| [151](151-codegen-mono-recursion-closure-generics.md) | Plan 151 — M:N runtime: GC premature-collect замыкания в supervised{spawn{body()}} (НЕ mono-recursion) | ✅ **ЗАКРЫТ Ф.0-Ф.5 (2026-06-13).** Title-мисдиагноз исправлен (см. §1). |
| [152](152-gate-verification.md) | Plan 152.0 — Gate verification & baseline-методология (lesson) | — (нет Статус-строки) |
| [152](152-string-coordinate-model.md) | Plan 152 (umbrella) — Production-grade строковая модель: линзы, координаты, Unicode-корректность | ✅ **PHASE A + PHASE B ЗАКРЫТЫ** (2026-06-16), все sub-plans 152.0–152.7 + 152.7.1 закрыты; 152.8 открыт (post-merge, P2/P3). |
| [152.0](152.0-module-restructure.md) | Plan 152.0 — Реструктуризация модуля str: папка + internal _buffer + RawMem | ✅ **ЗАКРЫТ 2026-06-13** (Ф.0.0–Ф.6), branch |
| [152.1](152.1-coordinate-model-lenses.md) | Plan 152.1 — Координатная модель + линзы as_bytes/as_chars (D249, D250) | ✅ **ЗАКРЫТ** (Ф.0-Ф.5, 2026-06-13, ветка `plan-152`), P1. **Эстимат:** ~2–3 dev-day. |
| [152.2](152.2-string-surface-parity.md) | Plan 152.2 — Полный str-surface (паритет Go/Rust/TS/Kotlin/Java) (D251) | ✅ **ЗАКРЫТ** (Ф.0-Ф.5, 2026-06-13, ветка `plan-152`), P1. **Эстимат:** ~2–3 dev-day. |
| [152.3](152.3-char-type-api.md) | Plan 152.3 — char-тип: классификация / case / digit (D252) | ✅ **152.3a (ASCII) ЗАКРЫТ** (2026-06-13, ветка `plan-152`); |
| [152.4](152.4-std-unicode.md) | Plan 152.4 — std/unicode: нормализация / сегментация / folding / case-mapping (D253, Q-unicode-data) | 🟢 **152.4.1+152.4.2 (нормализация) + 152.4.3 (graphemes) |
| [152.5](152.5-comparison-collation.md) | Plan 152.5 — Сравнение и collation (D254, Q-string-collation) | ✅ **152.5a (core, вкл. D-R4) ЗАКРЫТ** (2026-06-13) + |
| [152.6](152.6-utf16-encoding-interop.md) | Plan 152.6 — Encoding interop (UTF-16 / UTF-32 / code points) (D255) | ✅ **ЗАКРЫТ** (Ф.0-Ф.4, 2026-06-13, ветка `plan-152`), P1. **Эстимат:** ~1–1.5 dev-day. |
| [152.7](152.7-interpolation-formatting.md) | Plan 152.7 — Интерполяция строк и форматирование (D258, Q-format-spec) | ✅ **ЗАКРЫТ ПОЛНОСТЬЮ 2026-06-16** — B1 (формат-спеки, |
| [152.7.1](152.7.1-write-sink.md) | Plan 152.7.1 — Write-sink: деcouple Display/Debug от StringBuilder (D258 AMEND) | ✅ **CLOSED 2026-06-16** (commits `a313926b` + `3d0e30fa`). |
| [152.7.2](152.7.2-format-context.md) | План 152.7.2 — формат-контекст в Display (D419) + интерполяция прямо-в-sink | ⛔ SUPERSEDED планом 208 (2026-07-21, фикс рассинхрона): D419 ретрактирован в пользу D422; interp-direct-наработки переиспользованы 208-волнами; план 208 ЗАКРЫТ целиком. Исходная строка: 🔨 В РАБОТЕ (2026-0… |
| [152.8](152.8-char-u32-codepoint.md) | Plan 152.8 — char и code-point буферы: переход на 32-бит (оптимизация памяти) | ✅ ЗАКРЫТ 2026-06-16. Слой 1 + Слой 2 в ветке `plan-152.8`. |
| [153](153-compiler-bugs-phase-b.md) | Plan 153 — Compiler Bug Fixes (Phase B blockers) | ✅ ЗАКРЫТ 2026-06-16 (коммиты `d505c0e5` + `542a3db8` + тесты) |
| [153](153-vec-production-model.md) | Plan 153 (umbrella) — Production-grade Vec[T] / []T: API-паритет, итераторы, слайсы | ✅ **ЗАКРЫТ** — все под-планы 153.0–153.6 закрыты |
| [153.2](153.2-Z-zero-alloc-lazy.md) | План 153.2-Z — Zero-allocation ленивый конвейер Vec[T].lazy() | 🟡 ЧАСТИЧНО ЗАКРЫТ — Ступень 1 ✅, Ступень 2 ✅ (`515de5742`, generic-over-source), |
| [153.3.1](153.3.1-pdqsort.md) | Plan 153.3.1 — pdqsort: замена heapsort в @sort_unstable* | ✅ CLOSED 2026-06-18. |
| [154](154-no-silent-dispatch.md) | Plan 154 (umbrella) — No silent no-op dispatch | ✅ ЗАКРЫТ (154.0 ✅ CLOSED 2026-06-13; 154.1 ✅ CLOSED 2026-06-13, |
| [154.0](154.0-method-override-coherence.md) | Plan 154 — Method coherence: запрет silent no-op переопределения метода | ✅ **CLOSED 2026-06-13** (commit `809e8605` + |
| [154.1](154.1-impl-conformance-primitive-format.md) | Plan 154.1 — Явная opt-in конформность протоколов (#impl) + конкретные Display/Debug примитивов | ✅ **CLOSED 2026-06-13** (ветка `plan-154.1`, |
| [155](155-json-performance.md) | Plan 155 — std/encoding/json production-grade performance rewrite | 🟡 ЧАСТИЧНО ЗАКРЫТ ПОБОЧНО — P1 (O(n²)-лексер) закрыт |
| [156](156-test-runner-slow-lane.md) | Plan 156 — Slow-test lane: большие тесты в репо, вне дефолт-регресса ([M-test-runner-large-test-lane]) | ✅ IMPLEMENTED (suffix-only механизм `_slow.nv` + флаги |
| [157](157-interpreter-unsupported.md) | Plan 157: Tree-walking interpreter — UNSUPPORTED (C-codegen only) | ✅ DONE (2026-06-14) — `nova run` громко ошибается, мёртвые interp-тесты |
| [158](158-test-runner-worker-stack.md) | Plan 158 — Test-runner worker-thread stack size ([M-codegen-conformance-stack-overflow]) | ✅ **DONE** (ветка `plan-cgstack`, worktree nova-p156). |
| [159](159-reachability-codegen.md) | Plan 159 — Reachability-based codegen (dead-code elimination на эмиссии) | ✅ IMPLEMENTED (Ф.1–Ф.4 green; см. «Статус по завершении»). P2. |
| [159.1](159.1-method-reachability-dce.md) | Plan 159.1 — Method-reachability DCE: точность + звучность-как-МОДЕЛЬ | 🔧 Ф.1 ✅ ЗАКРЫТА 2026-07-16 (P0-риск `[M-159.1-onexit-drop-overprune]` |
| [160](160-module-privacy.md) | Plan 160 — Module-level field privacy (type X priv { … }) | ✅ ЗАКРЫТ (Ф.1–Ф.3 + Ф.4 симметрия, 2026-06-15). |
| [161](161-blanket-protocol-receiver.md) | Plan 161 — Blanket protocol-receiver methods (fn[I Next[T]] I @m) | ✅ CLOSED Ф.0-Ф.4 2026-06-15 (branch plan-161). |
| [162](162-enumerate-zc.md) | Plan 162 — EnumerateIter: zero-cost enumerate adapter | ✅ CLOSED+MERGED 2026-06-16 (merge `08a3db41`, branch plan-162). |
| [162](162-rust-model-module-resolution.md) | Plan 162 — Rust-модель резолва: ленивый резолв модулей + «методы едут с типом» + снятие Ф.4-хардкода | ✅ CLOSED (2026-06-16). P2 (архитектурный; разблокирует эргономику и убирает долг). |
| [162.1](162.1-resolver-split-lazy-bodies.md) | Plan 162.1 — Полный split резолвера: collect-signatures → lazy-bodies | ✅ CLOSED (2026-06-16). P2. |
| [162.2](162.2-sig-table-wiring.md) | Plan 162.2 — Wiring sig_table в compile path + lazy cross-module fn lookup | ✅ CLOSED (2026-06-16). P2. |
| [163](163-import-export-glob-hygiene.md) | Plan 163 — Гигиена import/export: запрет glob-форм (named + alias only) | ✅ CLOSED+AMENDED (2026-06-16). P3 (мелкий, шипится независимо). |
| [164](164-method-resolution-blanket-and-impl-attr.md) | Plan 164 — Method resolution: blanket dispatch fix + #impl(P[T]) + vec_iter rename | ✅ CLOSED (Ф.1–Ф.4, 2026-06-16). **Branch:** plan-zfix. **Worktree:** D:\Sources\nv-lang\nova-p-zfix. |
| [165](165-value-iter-types.md) | Plan 165 — Value-record iterator types + codegen generic-forward-decl fix | ✅ CLOSED 2026-06-16 |
| [166](166-udp-split.md) | Plan 166 — net.c send_to TOCTOU fix + UDP socket split | ✅ CLOSED 2026-06-17 |
| [167](167-type-name-too-short.md) | Plan 167 — E_TYPE_NAME_TOO_SHORT: запрет однобуквенных имён типов | ✅ CLOSED 2026-06-17. **Приоритет:** M. |
| [168](168-vec-body-forward-decl.md) | Plan 168 — Vec generic forward-decl missing for body-only instantiations (D300) | ✅ CLOSED 2026-06-17. **P1**; Sonnet 4.6. |
| [169](169-test-suite-health.md) | Plan 169 — Test-suite health (umbrella) | ⛔ SUPERSEDED — рамка «nova_tests = гейт шиппинга» отменена |
| [169.1](169.1-test-suite-profiling-and-speedup.md) | Plan 169.1 — Test-suite profiling + speedup | ✅ ЗАКРЫТ 2026-06-17 (followups closed 2026-06-17). |
| [169.1.1](169.1.1-test-lane-flags-and-ci.md) | Plan 169.1.1 — CLI category-селекторы + CI авто-регресс | ✅ CLOSED 2026-06-19. Реализовано Ф.1 (TestSelection + CLI флаги) + Ф.2 (CI workflow). |
| [169.1.2](169.1.2-consolidate-tests.md) | Plan 169.1.2 — Консолидация тестов по темам | ✅ Level-2 ВЫПОЛНЕНО 2026-06-20 (−51 CU, 8 семейств; итого консолидация ~−102 CU). Остаток: gated на 172 (красные суб-папки) + plan118/70 (merge-conflict). Метод доказан plan103_2. |
| [169.2](169.2-nova-tests-fix-sweep.md) | Plan 169.2 — nova_tests fix-sweep (folder-module name-collision class) | ⛔ SUPERSEDED — рамка «nova_tests = гейт шиппинга» отменена конвенцией |
| [169.2.1](169.2.1-core-unicode-decouple.md) | Plan 169.2.1 — Развязать prelude.core от std.unicode | ✅ CLOSED 2026-06-19, MERGED в main (коммит `729ac6b66`; |
| [170](170-file-private-visibility.md) | Plan 170 — priv(file): file-private видимость для peer-модулей | ✅ **CLOSED 2026-06-19** (ветка `plan-170-priv-file`). |
| [172](172-closure-roadmap.md) | Plan 172 — единый секвенированный roadmap закрытия (синтез 2026-06-28) | — (нет Статус-строки) |
| [172](172-compiler-rework.md) | Plan 172 — Переработка компилятора (umbrella) | 🔄 ПОГЛОЩЁН [Plan 196](196-one-truth-closeout.md) («Умбрелла над: 172.1, 172.12, 172.13») — координируется, не дублируется отдельным треком. |
| [172](172-spec-tests-salvage-state.md) | spec_tests salvage-state — batch-workflow w8w3huvrz (2026-06-29) | — (нет Статус-строки) |
| [172.1](172.1-reg-execution.md) | REG-трек execution-план — единый реестр методов/stdlib (§0.6, ultracode recon 2026-06-28) | — (нет Статус-строки) |
| [172.1](172.1-tally-audit-2026-07-02.md) | Workflow digest 2026-07-02: tally audit + clusters | — (нет Статус-строки) |
| [172.1](172.1-unified-type-engine.md) | Plan 172.1 — Unified type engine | 🔄 ПОГЛОЩЁН [Plan 196](196-one-truth-closeout.md) («Умбрелла над: 172.1 (U-хвосты), 172.12, 172.13») — координируется, не дублируется отдельным треком. |
| [172.1.1](172.1.1-generic-mono-erased-stub-lowering.md) | Plan 172.1.1 — Generic-mono / erased-stub lowering reconciliation (U.4.5 substrate) | 📋 proposed 2026-06-27 (выделено из 172.1 U.4.5 — решение владельца) |
| [172.2](172.2-method-arg-type-checking.md) | Plan 172.2 — Method-argument type-checking + scalar-narrowing enforcement | ✅ closed 2026-06-26 (реализовано — checker-side method-arg narrowing через `check_instance_overload` + миграция unicode на `codepoint = u32` (D327); гейт зелёный). Closure — §10. |
| [172.3](172.3-type-set-bounds.md) | Plan 172.3 — Type-set bounds (Go-style generic constraints) | ✅ **CLOSED 2026-06-28** (Ф.0–Ф.5; 9/9 тестов; 0-new-FAIL vs clean baseline). 2 осознанных отложения (§5, не soundness-долг). |
| [172.4](172.4-value-abi-auto-placement.md) | Plan 172.4 — Единый value-ABI + автоматический placement (by-ref / heap↔stack) | ✅ CLOSED 2026-07-04 (correctness-core Ф.1-Ф.3 = owner-acceptance выполнен: fluent `mut @ -> @` + структурное `==` + единый receiver-ABI VR+NT; §0-консолидация предиката f8d6bfaa). P3-perf Ф.4-Ф.6 ВЫНЕСЕНЫ → [172.14](172.14-value-abi-perf-placement.md) (ре… |
| [172.5](172.5-inout-ref-params.md) | Plan 172.5 — In-out ref-параметры (safe by-ref borrow) + формализация @/-> @ | ⛔ СУПЕРСЕДЕД Plan 184 (2026-07-06/08) — весь дизайн этого плана («`ref` — режим параметра, НЕ тип»; `mut ref x T`/`ro ref x T`; call-site маркер `ref x`) РЕТРАКТИРОВАН заходом-1/5 Плана 184 в пользу «`ref T` — ограни… |
| [172.6](172.6-cli-test-require-path.md) | Plan 172.6 — nova test требует явный путь | ✅ DONE (см. «## Статус» ниже — все пункты выполнены, коммит в plan-172-unified-type-engine). |
| [172.12](172.12-typed-ir-mono.md) | Plan 172.12 — Typed-IR mono path | 🔧 ✅ ЗАКРЫТ 2026-07-09 (A1-A8 + коллапс триплификации §14.19; [N]T value-семантика — заморожена отдельно) (заход 1: Ф.0 карта закрыта; заход 2: A1 попытка → нулевой итог, карта пересеквенирована;… |
| [172.13](172.13-constraint-inference.md) | Plan 172.13 — Constraint-based inference core | 🚧 in-progress (снята пауза 2026-07-10, [sonnet]). Ф.0 (инвентарь) + Ф.1 (ядро-скелет: unify/occurs-check/type-set, 16 юнитов) + Ф.2 (миграция пакета C — literal-coercion семья на Constraint/TypeSet/Solver, byte-parity подтверждён: conformance 91… |
| [172.14](172.14-value-abi-perf-placement.md) | Plan 172.14 — Value-ABI perf: авто by-ref / heap↔stack / copy-elision (P3) | 🟢 CORE LANDED 2026-07-10 (волна value-abi-172-14, см. «Итог волны» ниже) — Ф.1 авто by-ref для free-fn (>16Б C-ABI ro value-struct: NovaValue_/NovaTuple_/`[N]T`-inline) + Ф.3-дёшево (элизия temp) + фикс регрессии main «срез `[N]T`» (тип среза =… |
| [173](173-error-system-unify-harden.md) | Plan 173 — Система ошибок и cleanup: унификация + hardening (panic/fail/defer/on_exit), production-grade | ✅ ЗАКРЫТ — все фазы Ф.0R-Ф.6 закрыты по телу файла (Ф.0R 2026-07-09, Ф.1 2026-07-04, |
| [173.0](173.0-concurrency-runtime-substrate.md) | Plan 173.0 — Рантайм-субстрат для structured concurrency (гейт) | ✅ ЗАКРЫТО 2026-07-08 [sonnet, ветка substrate-173-0/nova-unders]. Ф.1 (drain-race, рантайм+deliverable) закрыта предыдущей волной; Ф.2 (per-slot `child_error[]` retention) + Ф.3 (serialized decision-loop + ctx-pinning/R1-guard) закрыты этой волной. Гей… |
| [173.1](173.1-parallel-collect-and-supervised-value.md) | Plan 173.1 — parallel for → []T (сбор через канал+consume) + supervised как значение | ✅ **DONE ядро 2026-07-09** (ветка `parallel-collect-173-1`; spec = D414 §4 + D71-amend). |
| [173.2](173.2-supervision-as-effect.md) | Plan 173.2 — Supervision как эффект (Supervisor / on_child_fail / Decision) | ✅ ЗАКРЫТО ПРОД-РЕДИ 2026-07-10. **Амендмент 2026-07-10 (владелец): Restart-семейство |
| [173.3](173.3-data-race-freedom-share.md) | Plan 173.3 — Data-race-freedom: атрибут #share + capture-check + consume-в-spawn | ✅ ЗАКРЫТ 2026-07-10 (sonnet, ветка `drf-173-3`, worktree nova-p182; D-блок = **D415** |
| [174](174-lang-ffi-features.md) | Plan 174 — Language & FFI features on the unified type engine (umbrella) | 📋 READY (umbrella). Создан 2026-06-27; **Ред. 2 — 2026-07-03**: полная сверка семейства |
| [174.1](174.1-primitive-parse-api.md) | Plan 174.1 — Primitive parse API (One-Engine, radix-only parse) | ✅ ПОВЕРХНОСТЬ SHIPPED 2026-07-08 (ветка `parse-174-1`) — итоговый канон ОТЛИЧАЕТСЯ от черновика ниже, см. §0-ИТОГ. Остаток (codegen-хардкод `T.try_from(str)`, float-канон) — `[M-174.1-parse-engine-structural]`. |
| [174.2](174.2-question-mark-return-only.md) | Plan 174.2 — ? строго return-only (Rust-стиль) + чистка spec | ✅ **CODEGEN-ЧАСТЬ РЕАЛИЗОВАНА** (Plan 173 Ф.1 #3, |
| [174.3](174.3-any-type-and-is-downcast.md) | Plan 174.3 — any top-type + is/try_as runtime type-check & downcast | ✅ Ф.1+Ф.2 ВЫПОЛНЕНЫ (2026-07-04); |
| [174.4](174.4-effect-registry-compile-time-size.md) | Plan 174.4 — Effect-registry: compile-time размер вместо хардкода 32 | ✅ Ф.1 DONE 2026-07-04 (Ф.2 static-indices — follow-up). |
| [174.5](174.5-pointer-ops-methods.md) | Plan 174.5 — Указатели: операции через методы (retire *p/p+i/p[i]) + полный метод-набор + write-cap fix | 🟡 Ф.2-Ф.4 DONE |
| [174.6](174.6-ffi-abi-types.md) | Plan 174.6 — C-FFI ABI: типы extern "C" + ABI fn-указателей (спека догоняет реальность) | ✅ **ЗАКРЫТ 2026-07-15** (дизайн/спека финализированы; M4 unsafe-когерентность решена — D424, вариант A; M4-enforcement ✅ РЕАЛИЗОВАН 2026-07-17; хвост M3 — по маркерам). M0 (spec) ✅ DONE 2026-07-04; **M1 (parser `*extern "C" fn` … |
| [175](175-time-system-rework.md) | Plan 175 — Переработка системы времени: типизированный Time-эффект (retire int-wire) + overflow-safe Duration + Monotonic из builtin в .nv + единый источник схемы | 🚧 IN PROGRESS (ядро закрыто, доводочные пункты остаются TODO) — **Ф.0/Ф.1 ✅; Ф.1b ✅ + Ф.3 ✅ |
| [175.1](175.1-civil-time.md) | Plan 175.1 — Гражданское (календарное) время: Date/TimeOfDay/DateTime/ZonedDateTime/Period + ISO-8601/RFC-3339 | ✅ SHIPPED 2026-07-10 (ветка `civil-time-175-1`, `std/time/civil/` folder-module; D319/D320/D321 внесены; отступления реализации — D321 §impl-отступления + маркеры `[M-175.1-*]`; полный IANA-snapshot — `[M-175.1-full-tzdb-embed]`). `Offset.local()`/`[M-175.… |
| [175.2](175.2-typed-effects.md) | План 175.2 — Эффекты «без магии»: typed ops + общий codegen + #default_handler + ретракция ambient | 🔨 В РАБОТЕ (2026-07-21, волна в полёте — sonnet, worktree `nova-typedfx`, |
| [176](176-io-fs-os.md) | Plan 176 — I/O + Filesystem + OS: io-core (Read/Write/Seek) + Fs-эффект + Os (env/args/cwd) | 🟢 **Ф.0.5 + Ф.1 + Ф.2 (fs+Path) + Ф.3 (os) + Ф.4 (net-миграции) + Ф.5 (тесты/spec/docs/Q-sweep) — ВСЕ DONE (Ф.4/Ф.5: 2026-07-09). Plan 176 ЗАКРЫТ.** **Маркер:** `[M-176-io-fs-os]` (CLOSED — см. `docs/plans/backlog-followups.md`). |
| [177](177-fallible-result-everywhere.md) | Plan 177 — Единый fallible-контракт std: Result-everywhere (no bare-throws convention) | ✅ **CLOSED 2026-07-04** (D325-конвенция полностью в спеке + guard + conformance 41/0; stable-std in-scope мигрирована; остаток — явные маркеры, см. §14) — **D325 ✅ committed** + **amend-пакет §4a ✅ внесён** (`04-effects.md`: R0/R4-крите… |
| [178](178-std-http.md) | Plan 178 — std/http: message-model + URL + HTTP/1.1 (client+server) + HTTPS + HTTP/2 | 📋 READY (**Ред. 2 — 2026-07-03**: renumber → D357–D362 выполнен; стейл-номера сняты; siblings-дыры закрыты: Monotonic/from_secs, cancel-семантика, ErrSource-export, write-backpressure, 1xx-loop, NO_PROXY-матрица, TE-trailers). |
| [179](179-std-encoding-compress.md) | Plan 179 — std/encoding/compress (DEFLATE / zlib / gzip / brotli) | 🟢 Ф.1 + Ф.3 + Ф.2(decode) LANDED — inflate/gzip/zlib **decode** (pure-Nova) + deflate/gzip/zlib **encode** (pure-Nova, levels + streaming) + **brotli decode (C-FFI, 2026-07-06)** готовы. **Ф.2 итог (2026-07-06):** vendored google/brotli v1.2.0 decoder (headers+lib, стиль libuv, БЕЗ исход… |
| [180](180-serde-derive.md) | Plan 180 — std/encoding/serde (Serialize/Deserialize + компиляторный auto-derive) | `✅ ВЫПОЛНЕН (record-path, honest-complete) — 2026-07-05`. **Completeness-аудит close-out (2026-07-05):** adversarial-аудит нашёл 6 реальных дыр полноты record-path — ВСЕ починены (не отложены): `Option[value-record]` + `HashMap[str,value-record]` mono-… |
| [180.1](180.1-serde-parity-and-beyond.md) | План 180.1 — serde: паритет с Rust serde + где мы можем быть лучше | 🟡 IN PROGRESS 2026-07-22 — **Ф.1 (field-attributes) + Ф.7 (strict-by-default |
| [181](181-same-scope-rebinding.md) | Plan 181 — Same-scope re-binding (ro x = ... повторно, тип может меняться) | ✅ **РЕАЛИЗОВАН (2026-07-04)** — R1–R7 + B1/B2/B3 закрыты; D347 в спеке; conformance 38/0 (d347 + amend d90/d131/d133/d22/d34); pos/neg `nova_tests/rebind/` 4/4; zero-regression vs d97c0dbe delta 0 (~135 тестов). Ф.4 R5-lint `W_SHADOW_UNRELATED` — реализован как **пр… |
| [182](182-test-suite-sanation.md) | Plan 182 — Санация тестового корпуса nova_tests/ (черновик) | `📋 ✅ ЗАКРЫТ 2026-07-09 (санация выполнена волнами 2026-07-06..08; довливные классы раннера — маркерами в backlog) (аудит-карта + план фаз; правок в nova_tests НЕ вносилось)`. |
| [183](183-net-rework.md) | План 183 — Переработка std/net: один слой extern "C", байты, M:N-безопасность | ✅ **ЯДРО ВЫПОЛНЕНО (Ф.0-Ф.4 ЗАКРЫТЫ, 2026-07-06)** — см. журнал §Заход 1-4 |
| [184](184-ref-type-revision.md) | План 184 — Ревизия D326: ref T как ограниченный тип (черновик на проверку владельцу) | ✅ ЗАКРЫТ 2026-07-08 (Р1-Р14 реализованы; D326-ревизия в спеке) (обсуждение 2026-07-06, дизайн надиктован владельцем). |
| [185](185-nova-lint.md) | План 185 — nova lint: машинные проверки конвенций | — (нет Статус-строки) |
| [186](186-hex-blob-embed.md) | План 186 — hex-блоб литерал x"…" и интринсик embed("path") (D412) | — (нет Статус-строки) |
| [187](187-flagship-concurrency-demo.md) | План 187 — Флагманское демо: конкурентный агрегатор с живой визуализацией | — (нет Статус-строки) |
| [188](188-heterogeneous-any-collections.md) | Plan 188 — Гетерогенные коллекции []any (вынос 174.3 Ф.3) | 📋 PROPOSED 2026-07-10 (решение владельца — выделить в отдельный план). |
| [189](189-virtual-clock-mn-ordering.md) | Plan 189 — Виртуальные часы: гарантия порядка под armed M:N (вынос из 175) | 📋 PROPOSED 2026-07-10 (решение владельца — выделить в отдельный план). |
| [190](190-full-tzdb-embed.md) | Plan 190 — Полный вшитый tzdb (#embed_tzdata-флаг) (вынос из 175.1) | 📋 PROPOSED 2026-07-10 (решение владельца — отдельный план). |
| [191](191-bcrypt-security-hardening.md) | Plan 191 — bcrypt: закрыть security-surface (демо-KDF под именем bcrypt) | ✅ **Вариант A (де-риск) ЗАКРЫТ 2026-07-10** (ветка `bcrypt-derisk-191`, |
| [193](193-nova-tls-repo.md) | Plan 193 — nova-tls: вынос TLS в отдельную репу + examples + доки | 🚧 Ф.1 ✅ ЗАКРЫТА, Ф.2 BLOCKED 2026-07-12 — файловый вынос |
| [194](194-contract-execution-model.md) | Plan 194 — Модель исполнения контрактов: #debug + переопределение --contracts | ✅ ЗАКРЫТ 2026-07-15 (Ф.0-Ф.4; спека `09-tooling.md` D24-амендмент сама фиксирует закрытие; коммиты `eff4f080d..36d82ddac`). **Приоритет:** P1 (язык-семантика; разблокирует vrange-роутинг). |
| [195](195-native-modules-c-not-rust.md) | Plan 195 — src/ layout std-миграция (общий native-модуль-паттерн → доки) | 🟢 ЗАВЕРШЁН 2026-07-13 — Ф.1-2 (mbedTLS-своп TLS, Rust удалён, T40) И |
| [196](196-one-truth-closeout.md) | Plan 196 — «Одна правда»: удалить второе окно infer_expr_c_type | 🔥 IN PROGRESS (НЕ ЗАКРЫТ — см. «Итог финальной closeout-волны |
| [196](196-retracted.md) | Plan 196-retracted — тупики и мисдиагнозы миграции «одного окна» | — (нет Статус-строки) |
| [196.2](196.2-class-c-relocation.md) | Plan 196.2 (ВОЛНА-1) — СЛИТА в 196.5 Stage-D (владелец 2026-07-13) | 🔄 = **ВОЛНА-1** (bottom-up), АКТИВНА |
| [196.3](196.3-wave2-d-driven.md) | Plan 196.3 — Волна-2: встречная D-driven миграция сиблингов второго окна | 🔄 в работе [opus, worktree `nova-wave2`]. |
| [196.4](196.4-call-resolvedtype-channel.md) | Plan 196.4 — Канал Call-expr ResolvedType: фундамент Tier-2 | 🔄 ДИЗАЙН (opus-разведка, worktree `nova-spike`, ветка `spike-196`). **Это ДИЗАЙН, НЕ реализация — |
| [196.5](196.5-node-substs-channel.md) | Plan 196.5 — Канал node_substs: per-call значения подстановки generic-параметров | ✅ ДИЗАЙН РЕАЛИЗОВАН (сверка по коду 2026-07-16, см. |
| [196.7](196.7-method-dispatch-resolved-callees.md) | 196.7 — Method-dispatch через resolved_callees (одно окно для диспетча метода по ресиверу) | ✅ ЗАКРЫТ 2026-07-15 (ветка `p196-dispatch`, worktree `nova-196-7`; в main вливает оркестратор ПОСЛЕ 206/209Ф.3/D39 — `emit_c.rs` конкурентно правится). |
| [196.8](196.8-primitive-receiver-bounded-blanket.md) | 196.8 — Bounded-бланкет на примитивном ресивере (одно окно для D310 type-set bound) | ✅ ЗАКРЫТ 2026-07-16 (ветка `p196-8-dispatch`, worktree `nova-p196-8`; в main вливает оркестратор — `emit_c.rs` конкурентно правится). |
| [196.9](196.9-primitive-concrete-overload.md) | 196.9 — Concrete-vs-concrete на РАЗНЫХ примитивах (одно окно для pattern-bound receiver scope) | ✅ ЗАКРЫТ 2026-07-16 (ветка `p196-9-overload`, worktree `nova-p196-9`; в main вливает оркестратор — `types/mod.rs` конкурентно правится семьёй 196.7/196.8). |
| [197](197-examples-revision.md) | Plan 197 — examples/ ревизия: снести устаревшее, пересобрать канон, дом для 187 | 🚧 Ф.1/Ф.2/Ф.3 ГОТОВЫ (Ф.3 — 2026-07-21, worktree `nova-ex197`/ |
| [198](198-nova-tests-triage.md) | Plan 198 — nova_tests/ триаж: keep-and-migrate ценное, снести stale (не оптом) | 🔨 Ф.1-Ф.4c ВЫПОЛНЕНЫ (рассинхрон-фикс 2026-07-21 по аудиту 172→221: файл |
| [199](199-str-drop-nul-termination.md) | Plan 199 — снять NUL-termination инвариант str (модель Rust/Go) | ✅ CLOSED 2026-07-12 — ВСЕ 4 фазы СЛИТЫ в main (Ф.1 retract D26 `c4f446477`, Ф.2 `to_cstr` |
| [200](200-std-improvements.md) | Plan 200 — зонтичный план улучшений std | 📋 ЖИВОЙ РЕЕСТР — **НЕ закрывать** (владелец 2026-07-12: «будем добавлять много новых штук»). |
| [200.1](200.1-std-test-speed.md) | План 200.1 — скорость nova test std (подплан плана 200) | 📋 СОГЛАСОВАН 2026-07-13; ЖДЁТ ОЧЕРЕДИ (подтверждено владельцем 2026-07-20, после хвостов П4/П15/П17) (владелец: «оформи отдельным подпланом»). |
| [201](201-consume-block-expression-and-share.md) | Plan 201 — consume X { } как выражение + @share()-канон (alias vs Clone) | ✅ ЗАКРЫТ 2026-07-13 [sonnet, worktree `nova-174`, ветка `d188-consume-block`] (D188 v1/v2/multi-var/v3/v3.1 + `@share`/refcount в nv + M-178 прямой move в consume-поле; спек-амендменты в тех же слияниях; conformance 104/0). |
| [202](202-module-registry-path-keyed-and-root-module.md) | План 202 — D78: реестр модулей по пути + корневой модуль пакета | ✅ ЗАКРЫТ 2026-07-13 (Ф.1+Ф.1b+Ф.2+Ф.3 — path-keyed реестр D78 rev-4 + root peers + |
| [203](203-http-out-of-std.md) | План 203 — вынос http из std в nv-lang/nova-http | ✅ ЗАКРЫТ 2026-07-13 (Ф.1-Ф.3 влиты — `b6818a137`: http выехал из std в репу-сиблинг nova-http по root peers D78 rev-4, std снова самодостаточен, +2 фикса резолвера). |
| [204](204-dependency-versioning.md) | План 204 — версии зависимостей: git + semver + nova.lock | ✅ ЗАКРЫТ 2026-07-13 (D420; Ф.1-Ф.3 влиты — `00c0d085f`+`1e14481b8`: [replace]-секция + W_DEP_PATH_NO_RELEASE + lock-семантика (replace не течёт в lock); nova-http на git-форме v0.1.0 с lock в репе). |
| [205](205-compress-out-of-nova-rt.md) | План 205 — компрессия из nova_rt в пакет nv-lang/nova-compress | ✅ ЗАКРЫТ 2026-07-17. Ф.0-Ф.1 (репа `nv-lang/nova-compress`, тег `v0.1.0`, |
| [206](206-arithmetic-overflow-policy.md) | План 206 — Арифметическая политика: пять исходов из одного overflow-примитива | 📋 СОГЛАСОВАН 2026-07-14 (наблюдение + дизайн подтверждены владельцем). **После:** [194](194-contract-execution-model.md). Ф.0/Ф.1/Ф.1b — чекпоинт волны удалён при закрытии, см. git-историю (статус актуален … |
| [206.1](206.1-div-neg-trap.md) | План 206.1 — Trap для div/mod/neg (подплан 206) | ✅ РЕАЛИЗОВАН 2026-07-16 (ветка `p206-1-divtrap`, worktree `nova-2061`; Ф.0-Ф.3 завершены, |
| [207](207-atomic-cas-witnessed-value.md) | План 207 — compare_exchange возвращает свидетеля (bool → Result[unit, T]) | ✅ ЗАКРЫТ 2026-07-15 (sonnet, ветка `plan207-cas`). **Приоритет:** P3 |
| [208](208-unified-formatter.md) | Plan 208 — Unified Formatter (@display(mut f Fmt), байтовый Write, zero-alloc) | ✅ ЗАКРЫТ 2026-07-21 — «один путь форматирования» достигнут (Ш0-Ш4 + Ш2 |
| [209](209-multi-tu-codegen.md) | План 209 — Multi-TU codegen: большой CU → N .c-единиц (параллельная компиляция) | 🚧 В РАБОТЕ. Ф.0 (рекон) + Ф.1 (codegen split) + Ф.2 (тулчейн) + Ф.3 (const-value |
| [210](210-embed-dir.md) | План 210 — embed_dir(...): вшить папку в бинарь (расширение D412) | — (нет Статус-строки) |
| [211](211-park-join-research.md) | Plan 211 — Park-join для nested supervised (research: остаточный race) | 📋 RESEARCH (заведён 2026-07-16, решение владельца: «митигация сейчас + park-join в |
| [212](212-audit-150plus-closeout.md) | Plan 212 — доделки по аудиту планов ≥150 (сверка «статус ↔ код», 2026-07-16) | ✅ ЗАКРЫТ 2026-07-18 — последний пункт (№6, `nova lint --deny`) реализован и ВКЛЮЧЁН: |
| [213](213-nova-lsp-performance.md) | Plan 213 — nova-lsp: диагностика и фикс жора CPU (27 CPU-часов/день) | ✅ ЗАКРЫТ 2026-07-17 — Ф.1 (диагностика: check_workspace тайпчекал ВСЕ 3074 файла |
| [214](214-coerce-attribute.md) | План 214 — #coerce: декларативные неявные конверсии (первые инстансы: str→[]u8, StringBuilder→str, WriteBuffer→[]u8) | 📘 СПЕКА ЗАФИКСИРОВАНА 2026-07-18 — D429 в `spec/decisions/02-types.md` написан |
| [214.1](214.1-generic-coerce.md) | План 214.1 — generic-#coerce: снятие R14 (реестр пар → матчер образцов) | 📋 ДИЗАЙН, СОГЛАСОВАН к исполнению (владелец 2026-07-23: «это было упущение, надо |
| [215](215-lsp-index-cache.md) | План 215 — персистентный кэш индекса nova-lsp (модель зрелых LSP) | ✅ ЗАКРЫТ 2026-07-19 (sonnet, ветка `p215-lsp-cache`, worktree `nova-lspcache`). |
| [216](216-consume-enforce-a.md) | План 216 — Consume-дисциплина: enforce по букве (вариант А) | ✅ Основная волна ЗАКРЫТА (слияние `bcb3c6bd6`, 2026-07-20, в `main`); |
| [217](217-auto-cleanup.md) | План 217 — авто-@cleanup для непотреблённых consume-переменных (гибрид C) | ✅ РЕАЛИЗОВАН И ВЛИТ 2026-07-21 (D432 + амендменты D133/D180 Rule 6/D314; |
| [217.1](217.1-cleanup-resource-rollout.md) | План 217.1 — раскатка @cleanup на ресурсы без него (подплан 217) | 📋 ОТЛОЖЕН — стартует ПОСЛЕ keystone [217](217-auto-cleanup.md) (авто-`@cleanup` |
| [218](218-prebuilt-runtime-archive.md) | План 218 — предсобранный архив рантайма libnova_rt.a (build-latency P1) | ✅ РЕАЛИЗОВАН 2026-07-20 (sonnet, worktree `nova-218`, ветка |
| [219](219-build-daemon.md) | План 219 — build-демон (build-latency P2) | ✅ РЕАЛИЗОВАН 2026-07-20 (sonnet, worktree `nova-219`, ветка |
| [220](220-tcc-dev-backend.md) | План 220 — TCC / быстрый dev-бэкенд (build-latency P3, разведочный) | ⏸️ ЗАМОРОЖЕН В Ф.0 2026-07-20 — гейт окупаемости пройден |
| [221](221-history.md) | История плана 221 — архив | — (нет Статус-строки) |
| [221](221-release-v0-1.md) | План 221 — Release v0.1 (первая прод-версия Nova) | 🔨 В РАБОТЕ (утверждён 2026-07-21; атомарная декомпозиция 2026-07-21 по запросу |
| [221.1](221.1-bug-sweep.md) | План 221.1 — Bug Sweep (подплан релиза 221: единый реестр зачистки бэклога дефектов) | 🔨 В РАБОТЕ (заведён 2026-07-21 по запросу владельца: «есть план, в котором |
| [221.2](221.2-luck-audit.md) | 221.2 — Каталог «мест на удаче» (аудит-волна 2026-08-02) | 📒 РЕЕСТР-КАТАЛОГ (не план работ; работы разнесены по волнам |
| [222](222-http-framework.md) | План 222 (зонтик) — nova-http как веб-фреймворк: скелет по Axum, продуктовая планка по FastAPI, механизм свой | 🔨 В РАБОТЕ (owner-go 2026-07-22: «старт до тегов»). **Волна A ВЛИТА (2026-07-23):** |
| [222.0](222.0-module-map.md) | План 222.0 — карта модулей Polaris/http: слой · статус · основа · под-план | 📋 КАРТА (навигация по под-планам зонтика [222](222-http-framework.md)), 2026-07-23. |
| [222.3](222.3-extractors.md) | План 222.3 — модуль polaris.extract: типизированное извлечение из запроса | 🔨 ДО ТЕГА (A-V11), дизайн-решение принято 2026-07-31 (см. блок ниже). **Слой:** 2 (Polaris-ядро). |
| [222.4](222.4-middleware.md) | План 222.4 — модуль polaris.middleware: ядро композиции | ✅ ЗАКРЫТ 2026-07-25 — Middleware = голый newtype-над-fn (nova-http 70fb77a; блокер-семья №78/90/96/97 закрыта планом 228 канал-материализацией; ре-гейт 26/0/19, флагман built) |
| [222.5](222.5-respond.md) | План 222.5 — модуль polaris.respond: IntoResponse + типизированный статус | ✅ РЕАЛИЗОВАН (ревизия «план vs код» 2026-07-31, ~98%): IntoResponse + 6 impl + blanket'ы, StatusCode-канон, deprecated int-мост — всё в nova-polaris. Кортеж-impl сознательно отклонён (задокументировано в response.nv). Остаток:… |
| [222.8](222.8-openapi-gen.md) | План 222.8 — OpenAPI-генерация через #impl(Reflect) (только (а); (б)-верификация отложена) | ✅ РЕАЛИЗОВАН ЦЕЛИКОМ (ревизия 2026-07-31): Reflect+эмиттер+golden-тест живого примера; (б)-верификация — за v0.1 по самому плану. |
| [222.11](222.11-multipart.md) | План 222.11 — модуль polaris.multipart: формы и загрузка файлов | ✅ РЕАЛИЗОВАН ЦЕЛИКОМ (ревизия 2026-07-31): Part/Multipart/лимиты (сверх плана — проводка через ServerPolicy), RFC 7578 парсер, base64, тесты. |
| [222.12](222.12-http-batteries.md) | План 222.12 — батарейки nova-http: модульная раскладка внутри пакета | ✅ РЕАЛИЗОВАН ~90% (ревизия 2026-07-31): cors/log/static/compress/ratelimit/recover — в nova-polaris (репо-раскол — решение владельца, план писался до него). Остаток V1: НЕТ brotli-НЕГОЦИАЦИИ (в пакете compress нет br-энк… |
| [222.13](222.13-auth.md) | План 222.13 — модуль polaris.auth: JWT/Basic/Bearer + сессии | ✅ РЕАЛИЗОВАН ~97% (ревизия 2026-07-31): Bearer/Basic/JWT/CookieJar/сессии+пример. SessionStore = EFFECT (эволюция 222.20 Ф.3 §Q2, не protocol из текста плана). OAuth2 — вне V1 по плану. |
| [222.14](222.14-websocket.md) | План 222.14 — WebSocket: http.ws (слой 1) + upgrade-extractor (слой 2) | ✅ РЕАЛИЗОВАН ~95% (ревизия 2026-07-31): кодек/handshake/сокет/extractor/РЕАЛЬНЫЙ hijack до живого сокета + пример 08. Остаток до тега: нагрузочный смок N параллельных сокетов (§6 Ф.3); устаревший комм… |
| [222.20](222.20-design-note.md) | 222.20 Ф.2 — Дизайн-нота: эффекты в Polaris | 📐 ДИЗАЙН (Ф.2, opus) — варианты + цены + рекомендации ВЛАДЕЛЬЦУ. |
| [222.20](222.20-effects-audit.md) | План 222.20 — Исследование эффект-поверхности Polaris и пакетов Nova | 📋 ЗАВЕДЁН по решению владельца (2026-07-26: «надо исследовать, что нужно |
| [222.20](222.20-effects-inventory.md) | 222.20 Ф.1 — Инвентарь эффект-поверхности (polaris / http / tls / compress) | 📋 РАЗВЕДКА (Ф.1, sonnet) — данные для Ф.2 (дизайн, opus). Ничего не |
| [222.21](222.21-router-api-surface.md) | План 222.21 — поверхность Router/Middleware: кросс-фреймворк-разбор + именование | 📋 Ф.0 ИСПОЛНЕНА 2026-07-27 (оркестратор, opus) — см. [docs/dev/research/http-frameworks-gap.md §6](../dev/research/http-frameworks-gap.md#6-ф0-22221--routermiddleware-поверхность-именование--композит-форма--фичи-разведка). |
| [222.22](222.22-polaris-serve.md) | План 222.22 — polaris.serve + ServerPolicy: высокоуровневый старт сервера | ОТКРЫТ (решение владельца 2026-07-30: «в свою сессию и исправить до релиза»). |
| [222.23](222.23-polaris-observability.md) | План 222.23 — наблюдаемость Polaris: Log / Metrics / Audit | ОТКРЫТ (вопрос владельца 2026-07-30: «как организовать сбор метрик, |
| [223](223-src-transparency-entry-mode.md) | План 223 — src-прозрачность в entry-режиме («src/ невидим всегда», D78 rev-5) | ✅ РЕАЛИЗОВАН 2026-07-23 (ОКНО-2, sonnet, ветка `p-okno2-derive-seed-223`) — |
| [224](224-vela-runtime-naming.md) | План 224 — Vela: имя M:N-рантайма в коде и доках | 📋 ДИЗАЙН, СОГЛАСОВАН (владелец выбрал имя **Vela** 2026-07-23; объём — этот план). |
| [225](225-blank-line-between-decls.md) | План 225 — sweep: пустая строка между top-level декларациями | ✅ ГОТОВО (исполнено 2026-07-23: 79 вставок в 20 файлах; nova-http 5+51, nova-blank/std 14+26, examples 1+2). |
| [225.1](225.1-blank-line-full-sweep.md) | План 225.1 — полный sweep вертикального ритма (все классы, все репы) | ✅ Ф.1+Ф.2 ИСПОЛНЕНЫ 2026-07-26 (899 правок / 178 файлов; nova c0461229f + polaris 5df9e3e + http d0e6b36; байт-C-гейт: нормализованный diff пуст — дельта только line-номера trace-метаданных; детектор-дефект пойман аг… |
| [226](226-ro-launder-l1-coercion.md) | Plan 224 — L1-ro launder via mut-binding: закрыть [M-ro-launder-via-mut-binding] | Ф.0б/Ф.1/Ф.2 (std+examples+spec_tests) СДЕЛАНЫ и провалидированы |
| [227](227-std-validate-library.md) | План 227 — std/validate: библиотека валидаторов (.nv, ноль компилятора) | 📋 ПРЕДЛОЖЕН (владелец 2026-07-24: «заведи известные валидаторы общего назначения + |
| [228](228-fnnt-channel-materialization.md) | План 228 — fn-newtype: материализация в канал (№94-v2) + закрытие №96/№97 | 📋 ДИЗАЙН (opus-разведка интегратора 2026-07-25; два тупика пройдены: №94-окно-1 |
| [229](229-polaris-docs.md) | План 229 — Полная пользовательская документация Polaris (EN+RU) | 📋 УТВЕРЖДЁН владельцем (2026-07-25: «нужна полная дока по Polaris, русский и анг |
| [230](230-polaris-examples.md) | План 230 — Примеры-приложения Polaris | ✅ ИСПОЛНЕН 2026-07-25 (nova-polaris master 05c180e): 10/10 примеров + run_smokes (10/10 агент 3× + приёмка интегратора) + README-пары 11×2 + кросс-ссылки docs↔examples. Отклонение от §2: примеры ведут accept-цикл через `handle… |
| [231](231-bug-cycle-exit.md) | План 231 — Выход из цикла точечных фиксов (систематизация качества компилятора) | ✅ УТВЕРЖДЁН владельцем (2026-07-26, «ДА» — порядок: аудит+матрица немедленно, IR — дизайн-нота). |
| [231.1](231.1-enforce-audit-table.md) | План 231-А — Таблица спека-энфорс-аудита | 🔨 ПЕРВЫЙ ПРОХОД (механическая инвентаризация, sonnet-волна, 2026-07-26). |
| [231.2](231.2-enforcement-infra.md) | План 231.2 — Инфраструктура машинного принуждения (исполнительный дом треков Д/Е плана 231) | 🔨 В РАБОТЕ (ядро исполнено 2026-07-26; остаток ниже). |
| [232](232-spec-overview-sync.md) | План 232 — Синхронизация обзорной спеки и сайта с D-решениями | 📋 ЗАВЕДЁН (2026-07-26, вопрос владельца: «спека менялась, дока нет — /spec/syntax/, |
| [232.1](232.1-f1a-closeouts.md) | План 232.1 — Закрытие вопросов волны Ф.1А (решения владельца 2026-07-26) | ✅ УТВЕРЖДЁН владельцем («1 - добавить; 2 - ок, как рекомендуешь; 3 - да, только |
| [233](233-pkg-tooling.md) | План 233 — Пакетный тулинг: прокси скачивания + переименование lock-файла | ✅ **ИСПОЛНЕН в репе nova 2026-07-27** (окно `p233-pkg-tooling`, sonnet, влито |
| [234](234-bitwise-operator-family.md) | План 234 — побитовое семейство: bit-префикс + оператор ~ | 🔶 ЧАСТИЧНО, В КОМПИЛЯТОРНОЙ ОЧЕРЕДИ (актуализировано 2026-07-30). |
| [235](235-bigint.md) | План 235 — std.math.bigint: целые произвольной точности на чистом Nova | ✅ **V1 РЕАЛИЗОВАН 2026-07-30** — репа `nova-bigint` (`master`, github, коммит `35bc047`). |
| [236](236-bigdecimal.md) | План 236 — bigdecimal (пакет nova-bigint): десятичная произвольной точности поверх BigInt | ✅ ЗАКРЫТ 2026-07-31 — V1 сдан (окно sonnet, приёмка интегратора: check 5/0, test 4/0/1, запушено на 3 ремоута). Три пина Ф.0 проверены фактом; 3 дефекта компилятора заведены (№170-№172), порядок атрибу… |
| [237](237-bigfloat.md) | План 237 — bigfloat (пакет nova-bigint): двоичная произвольной точности поверх BigInt | ЗАКРЫТ (2026-08-01) — BigFloat V1 влит в nova-bigint master (1ef06b1): mantissa/exp, нормализация, конверсии f64 (включая субнормали), sqrt; тесты 5/0, strict-effects 7/0. Попутный компилятор-дефект — [M-option-int-cast-u64-cc-fail] в р… |
| [238](238-fiber-memory-model.md) | План 238 — модель памяти между файберами: энфорс №150 + спек-амендмент | ✅ ИСПОЛНЕН И ВЛИТ 2026-07-31 (окно p238-fiber-memory, sonnet; приёмка интегратора; мега-CU 599/0/67, пуш 3dd71901b). №150 закрыт, D441 в спеке. Остаток — честные границы §5: №167/№168, подняты владельцем до блок… |
| [239](239-use-contextual-keyword.md) | План 239 — use: hard keyword → контекстный keyword (по образцу bench) | ✅ ГОТОВО (2026-08-01). Спека — ВНЕСЕНА (D443, `spec/decisions/02-types.md`, |
| [240](240-bigrat.md) | План 240 — bigrat (пакет nova-bigint): точные рациональные числа поверх BigInt | 🔨 В РАБОТЕ (отдан 2026-08-02 по слову владельца «отдавай в работу»; исполнитель — opencode/big-pickle, ветка p240 в nova-bigint). Открытые вопросы §6 закрыты рекомендациями плана как решениями (вла… |
| [241](241-spec-bilingual.md) | План 241 — spec на двух языках: en-переводы + двуязычные маршруты сайта | 🆕 PLANNED (записан 2026-08-02 по обсуждению с владельцем; запуск — по слову владельца). |
| [242](242-doc-conventions-guard.md) | План 242 — enforcement doc-conventions: страж + CI-job | 🆕 PLANNED (записан 2026-08-02; запуск — по слову владельца). |
