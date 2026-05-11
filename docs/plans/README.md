# Планы Nova

В этой директории — только **планы** (что и когда делаем). Справочные
материалы (таблицы сравнений, research-заметки, бенчмарки) живут в
[docs/research/](../research/).

## Схема нумерации

- `01-…`, `02-…` — главные планы по порядку создания.

## Текущие планы

| # | Файл | О чём | Статус |
|---|---|---|---|
| 01 | [01-roadmap-v0.1.md](01-roadmap-v0.1.md) | Roadmap разработки компилятора v0.1–v1.0+ | активный |
| 02 | [02-codegen-c-backend.md](02-codegen-c-backend.md) | C backend: компиляция Nova в нативный бинарь | активный |
| 03 | [03-package-ecosystem-roadmap.md](03-package-ecosystem-roadmap.md) | Package ecosystem: self-host → CLI → lockfile → registry | будущий (после v2.0+) |
| 04 | [04-buffer-split-and-external.md](04-buffer-split-and-external.md) | Buffer → StringBuilder/WriteBuffer/ReadBuffer + `external` keyword | ✅ выполнено (Buffer удалён из языка) |
| 05 | [05-as-cast-codegen.md](05-as-cast-codegen.md) | `as`-cast — реализация narrowing в codegen (D54 compliance) | ✅ выполнено |
| 06 | [06-iter-protocol-codegen.md](06-iter-protocol-codegen.md) | `Iter[T]` protocol в codegen — общий for-in (D58 compliance) | ✅ выполнено |
| 07 | [07-as-cast-saturation.md](07-as-cast-saturation.md) | `as`-cast saturation для float→int + spec D54 narrowing semantics | ✅ выполнено |
| 08 | [08-from-into-conversions.md](08-from-into-conversions.md) | `From`/`Into` framework + char/byte/bool + strict if-cond + conversions.md | ✅ почти выполнено, Ф.6 отложен |
| 09 | [09-clang-migration.md](09-clang-migration.md) | Миграция Windows-сборки с MSVC на Clang/LLVM (10-15% perf) | активный, не начат |
| 10 | [10-pgo-integration.md](10-pgo-integration.md) | PGO integration (stub, после плана 09) — 15-30% perf на hot path | stub / future |
| 11 | [11-method-values-and-overload.md](11-method-values-and-overload.md) | Method values + overload по типу аргумента (закрывает Q-overloading вариант 1) | ✅ ЗАКРЫТ (Ф.8 sweep std — optional, не блокер) |
| 12 | [12-builtins-driven-codegen.md](12-builtins-driven-codegen.md) | builtins.nv-driven external dispatch (Q-codegen-builtins-cleanup) | ✅ ЗАКРЫТ (Ф.6 type-checker gate отложен; Ф.4.5 auto-derive ❌ ОТМЕНЕНО Plan 13 Ф.9.5) |
| 13 | [13-runtime-stdlib-and-autogen.md](13-runtime-stdlib-and-autogen.md) | Runtime stdlib (str/math) + auto-gen std/runtime/*.nv (read-only projection реестра компилятора) | ✅ ЗАКРЫТ (включая Ф.9.2 `+` через `@plus` body + Ф.9.6 bag-fix `StringBuilder.@len` codepoints); user-defined `@plus` routing отложен до method_overloads expansion |
| 14 | [14-stdlib-codegen-gaps.md](14-stdlib-codegen-gaps.md) | Закрыть codegen-gap'ы блокирующие std/* (Iter[T] element-type, const non-trivial, free-fn-as-value, fn-в-record, D69 variadic, `int as char` literal) | ✅ **ЗАКРЫТ 2026-05-12** (Ф.1/2/3/4/6/6-bis/7); Ф.5 → [Plan 35](35-cross-file-resolve.md); type-check регресс → [Plan 34](34-stdlib-typecheck-fix.md) |
| 15 | [15-generic-bounds-enforcement.md](15-generic-bounds-enforcement.md) | D72 generic bounds `[T Protocol]` enforcement в type-checker'е | ✅ ЗАКРЫТ (Ф.1-Ф.5; Ф.4 закрыта 2026-05-11 — 3 negative + forward-dep positive; D53 anonymous-protocol literals — отдельная задача) |
| 16 | [16-capability-enforcement.md](16-capability-enforcement.md) | D63 `forbid` + D64 `realtime` compile-time capability checks (закрытие spec-vs-impl drift) | ✅ ЗАКРЫТ (Ф.1-Ф.9 ✅; 97/97 PASS включая 5 negative-tests) |
| 17 | [17-q-resolutions.md](17-q-resolutions.md) | Закрыть полу-открытые Q (string interpolation, clone semantics, array API, и др.) | ✅ ЗАКРЫТ (включая Ф.4 — string interpolation полная реализация: lexer/parser/AST/codegen StringBuilder/interp; 13 regression-тестов) |
| 18 | [18-stdlib-roadmap.md](18-stdlib-roadmap.md) | Stdlib gap-анализ Rust/Go → Nova: P0/P1/P2 приоритизация под backend/CLI нишу + зафиксированные дизайн-решения (libuv, rustls, M:N-aware sync) | proposal, не начат |
| 20 | [20-defer-implementation.md](20-defer-implementation.md) | D90 implementation: `defer` и `errdefer` scope-level cleanup statements | 🟡 DRAFT, не начат |
| 21 | [21-channel-revision-implementation.md](21-channel-revision-implementation.md) | D91 implementation: Channel revision на capability-split (Rust mpsc-style); зависит от Plan 20 | 🟡 DRAFT, не начат |
| 22 | [22-sleep-libuv-integration.md](22-sleep-libuv-integration.md) | `Time.sleep` через libuv `uv_timer_t` + унифицированный event-loop в scheduler'е; D92 implicit main-scope + D93 park/wake API. Открывает дорогу для Plan 18 P0 stdlib и Plan 21 Channel revision. | ✅ ЗАКРЫТ (Ф.1-Ф.6 ✅; 134/134 PASS + 5 sleep + 2 bench) |
| 23 | [23-mn-runtime-roadmap.md](23-mn-runtime-roadmap.md) | M:N runtime — архитектурный roadmap (thread pool + work-stealing + concurrent GC + TLS migration). Зависит от Plan 22/21/18-P0. Открывает Q-mn-* в open-questions. | roadmap, v1.0+ milestone |
| 24 | [24-cross-platform-test-runner.md](24-cross-platform-test-runner.md) | `nova-codegen test-build`/`test-all` subcommands — единая логика runner'а в Rust, run_tests.ps1/.sh thin wrappers | ✅ ЗАКРЫТ (Ф.1-Ф.3 ✅; Linux smoke — отдельно) |
| 25 | [25-production-readiness-roadmap.md](25-production-readiness-roadmap.md) | Honest gap analysis vs Go/Rust: gaps (M:N, growable stacks, **G3a default malloc-only**, G3b GC pauses, Linux smoke, preemption, cancel propagation) с blockers и acceptance criteria. G7 ✅ closed (Ф.8). | roadmap, не начат |
| 27 | [27-gc-switch.md](27-gc-switch.md) | **GC switch: Boehm как default** (Plan 25 G3a closer). vcpkg gc.lib уже vendored. Ф.1 add --gc flag → Ф.4 switch к Boehm после bench. **Production blocker для long-running workloads.** | ✅ почти закрыт (Ф.1/1.5/2/3/4/6 ✅ Б.2–Б.8 ✅ Ф.D/G audit-fixes ✅ — detect_boehm graceful fallback с Linux/macOS hints, pause measurement в spec/overview.md; Ф.5 Linux smoke-test ждёт CI) |
| 28 | [28-nova-cli.md](28-nova-cli.md) | `nova` CLI binary — единая точка входа для пользователя (`nova test/build/run/check/regen-runtime`). Заменяет run_tests.ps1/.sh и regen_runtime.ps1. nova-codegen остаётся внутренним инструментом. | ✅ ЗАКРЫТ (Ф.0-7 ✅; nova-cli/ crate, все субкоманды, скрипты удалены) |
| 29 | [29-repo-layout.md](29-repo-layout.md) | Реорганизация корня: `compiler-codegen/` → `compiler/`, `nova-cli/` → `cli/`. Nova-пакеты (nova_tests/, std/) не переименовываются — D78 риск. | план, не начат, низкий приоритет |
| 30 | [30-channel-improvements.md](30-channel-improvements.md) | Channel improvements: `send`→`bool` (no panic on closed) + multi-writer (`tx.clone()` + `writer_count` ref-count). Зависит от Plan 21. | план, не начат |
| 31 | [31-channel-select.md](31-channel-select.md) | `select` — multiplexed channel receive: ожидание на N каналах одновременно, пробуждение по первому готовому. Парсер + runtime SelectWaiter + codegen. | ✅ ЗАКРЫТ (Ф.1-6 ✅; all-closed panic реализован; 179/179 PASS после D97 fix memory_footprint) |
| 32 | [32-gc-introspection.md](32-gc-introspection.md) | `std.runtime.gc` introspection API (`heap_size`, `live_count`, `alloc_count`, `collect`, `reset_stats`) для differential GC-тестов и production memory pressure observability. | ✅ ЗАКРЫТ (Ф.1-Ф.5 ✅; 181/181 PASS malloc+boehm; differential: heap bounded 348 KB под Boehm на 500k allocs; no-leak: max_growth=0 на 20 циклов × 10k allocs; pause < 16ms на all workloads) |
| 33 | [33-contracts-implementation.md](33-contracts-implementation.md) | D24 контракты — **roadmap-индекс**. Разбит на 33.1 / 33.2 / 33.3 с decision point после каждого. Цель — паритет с Dafny/Creusot, превосходство над Rust+kani/prusti (где tool отдельный, не часть языка). Закрывает Q-contract-dsl и Q-pure-view. | roadmap |
| 33.1 | [33.1-contracts-core.md](33.1-contracts-core.md) | **Core:** `requires`/`ensures`/`old`/`result` + `ContractResult[R,E]` + Z3 SMT + debug runtime fallback + zero-cost release CI gate. Только straight-line код. Decision point: после 33.1 решаем стоит ли 33.2. ~2 недели. | план, не начат |
| 33.2 | [33.2-contracts-imperative.md](33.2-contracts-imperative.md) | **Imperative:** `reads`/`modifies` frame + loop invariants + `decreases`/termination + composition `@pure` функций + record `invariant` + arrays в SMT + `assert_static`. Зависит от 33.1. ~3.5 недели. | план, зависит от 33.1 |
| 33.3 | [33.3-contracts-advanced.md](33.3-contracts-advanced.md) | **Advanced (Dafny-parity):** `pure_view`+`axiom`+обязательный `@verify_handler` + ghost state + `assume` + bounded quantifiers + IEEE 754 FP + strings/sets/maps теории + incremental SMT cache + parallel verification + Z3↔CVC5 cross-check + `@must_verify_module` + `@trusted` external + AI-friendly JSON diag + CLI hooks + Dafny-tutorial port (20 примеров). Зависит от 33.2. ~3.5 недели. | план, зависит от 33.2 |
| 34 | [34-stdlib-typecheck-fix.md](34-stdlib-typecheck-fix.md) | stdlib type-check регресс fix (50/50 → текущий 33/44) + `std.testing` handler-фабрики `seeded()` / `fixed_ms()` для детерминированных тестов bcrypt/uuid/ulid/snowflake/jwt/cron/retry + parser-fix sweep (json/regex/property). Закрывает регресс после расширения stdlib новыми модулями. | активный, в работе (2026-05-12) |
| 35 | [35-cross-file-resolve.md](35-cross-file-resolve.md) | Cross-file resolve для compile-mode (`nova build foo.nv` с `import std.X.Y`). Вынесено из Plan 14 Ф.5 при закрытии того плана. Низкий ROI / высокая стоимость; cheap-вариант (multi-file через interp) — default. | план, не начат, низкий приоритет |
| 36 | [36-cli-production-hardening.md](36-cli-production-hardening.md) | Production hardening `nova check` / `nova test` — **30 requirements (R1-R30)** после **3-way audit** (cargo G01-G30 + go/CI H01-H30 + nova-specific N01-N25 = 85 gaps closed). MUST: polymorphic path, exit codes 0/1/2/3/101, 6 output formats (human/json/json-rendered/short/junit/sarif), color control (NO_COLOR/CI auto-detect), verbosity ladder, streaming NDJSON, **Ф.0 fix existing `cmd_check` correctness bug** (не вызывает infer_effects/lint_module), severity discrimination + `--deny warnings`, stable diagnostic codes (E0001-E9999), spec_link в JSON, **transitive cache** (с known v1 limit без Plan 35), reproducible builds (SOURCE_DATE_EPOCH), parallel = num_cpus, GC backend + ALLOC_REQUIRES awareness, nova.toml manifest validation, effect/capability info в JSON. SHOULD: pre-commit.com framework, GitHub Actions annotations, CI matrix examples, test artifacts contract. COULD/deferred: LSP, coverage, race detection, bench, telemetry, explain, i18n. | план, не начат |

> Plan 19 — see `19-closure-and-error-ops.md` (closure-rev + D85 error-ops).
> Plan 20 и 21 — последовательные (Plan 21 зависит от Plan 20).
> Plan 22 — самостоятельный, не блокирует Plan 20/21.
> Plan 25 — gap analysis vs Go/Rust state-of-the-art; не план-исполнения, а honest assessment.

## Связанные директории

- [docs/research/](../research/) — справочные материалы и сравнения
