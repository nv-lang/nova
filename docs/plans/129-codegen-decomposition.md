// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 129 — Codegen decomposition (split `emit_c.rs` / `types/mod.rs` / `parser/mod.rs` / `field_cache.rs`)

> **Статус:** 📋 DRAFT 2026-06-06 (proposed, NOT scheduled)
> **Приоритет:** P2 — нужен после shipping 0.1, не блокирует feature work
> **Origin:** community feedback re: agent productivity tax на 28k+ line files
> **Дата готовности к старту:** строго после тега 0.1 (Plan 91 целиком).
>   Рефактор во время активного feature-дева = merge hell (см. R1). Не
>   стартовать пока emit_c.rs в активной работе.

## Что и зачем (одной фразой)

Расщепить 4 monolithic файла — `emit_c.rs` (30,928 LOC), `types/mod.rs`
(18,331 LOC), `parser/mod.rs` (10,156 LOC), `field_cache.rs` (12,790 LOC)
— в submodule trees БЕЗ изменения семантики, сохраняя monolithic struct
patterns (CEmitter / TypeCtx) через inherent-impl-across-files.

Зачем:
- **Agent productivity tax:** работа в 28k-line файлах требует 6+ Grep'ов
  для navigation, 14+ Read'ов для full understanding, повышенный риск
  Edit "not unique" errors. Workflow sub-agents особенно страдают — в
  этой сессии `wjuje6jus` упал на StructuredOutput предположительно из-за
  context-exhaustion работая в emit_c.rs.
- **Code review fatigue:** 30k-line файл не помещается в любой review
  tool side-by-side. Любой PR который касается emit_c.rs — paratrooper
  reviewers видят hunks без context.
- **Compile-time:** Rust перекомпиливает целый crate при touch одного
  файла. Split на cohesive modules уменьшает rebuild scope при typical
  development (touch one emit_X — rebuild only that module + dependents).
- **Onboarding:** новый contributor видит `emit_c.rs` 30k LOC и сразу
  закрывает.

## Корневая причина (audit)

`CEmitter` struct (line ~370+) имеет ~200 fields — shared codegen state:
- mono worklists / instantiation caches
- registries (NovaOpt/NovaArr/NovaRes typedefs)
- generic_type_templates / generic_types
- current_fn / current_receiver / current_type_subst (traversal context)
- type_aliases / record_schemas / sum_schemas / protocol_types
- effect tracking / sync class / fiber state
- emit buffer / line counter / indent

Все методы — `impl CEmitter { ... }`. Rust supports **inherent-impl
split across files** через `mod X;` declarations в parent + `impl
CEmitter { ... }` в child files. Семантически identical, lexically
modular.

Pre-existing structural split в emit_c.rs:
- Lines ~10-100: type aliases / structs (MethodSig, MutableContext)
- Lines ~370-1100: CEmitter struct + constructor
- Lines ~1100-10000: pre-passes (forward decls, registration)
- Lines ~10000-22000: main emission methods (per ExprKind)
- Lines ~22000-28000: match-emit + helpers
- Lines ~28000-30928: infer_expr_c_type + utility helpers

## Зависимости

- **Не требует** новых language features — pure refactor.
- **Требует** stable codegen (Plan 125 family closed ✅).
- **Hard blocker:** активные feature PRs в emit_c.rs — конкуренция за
  тот же файл порождает merge hell. Рекомендуется starting window когда
  emit_c.rs не trogается ≥1 неделю.

## Фазы

### Ф.0 — Audit + decomposition map (1-2 days)

**Scope:**
1. Сгенерировать table: каждый method в emit_c.rs → cohesion group
   - Group examples: `emit_expressions`, `emit_statements`,
     `emit_type_decls`, `emit_match_machinery`, `infer_types`,
     `mono_pipeline`, `helpers`
2. Identify cross-cutting state на CEmitter — поля, к которым обращается
   ≥3 method groups. Эти поля остаются на CEmitter (нет point splitting
   struct itself).
3. Build dependency graph: которые группы вызывают какие → определить
   `pub(super)` boundaries.
4. **Same for `types/mod.rs` / `parser/mod.rs` / `field_cache.rs`** —
   each gets its own decomposition map.
5. Записать decomposition map в `docs/architecture/codegen-decomposition-audit.md`
   (публичный — контрибьюторы видят структуру разбиения) с per-file map.

**Gate:**
- 4 decomposition map'a written + reviewed
- No proposed cross-cutting state movement (только методы перемещаются)
- Estimated split: emit_c.rs → ~10 модулей × 2-4k LOC each

### Ф.1 — emit_c.rs split (3-5 days)

**Scope:**
Создать `compiler-codegen/src/codegen/c/` (rename `codegen/emit_c.rs`)
→ split в submodules:
- `codegen/c/mod.rs` — CEmitter struct definition + ctor + основной entry point
- `codegen/c/preamble.rs` — pre-passes (forward decls, registration)
- `codegen/c/types.rs` — type_ref_to_c, return_type_c, infer_expr_c_type
- `codegen/c/expressions/mod.rs` — emit_expr dispatch
  - `emit_call.rs`, `emit_match.rs`, `emit_if.rs`, `emit_block.rs`,
    `emit_lambda.rs`, `emit_literal.rs`, `emit_member.rs`, ...
- `codegen/c/statements.rs` — emit_stmt
- `codegen/c/mono.rs` — monomorphization pipeline
- `codegen/c/registries.rs` — NovaOpt/NovaArr/NovaRes typedef emission
- `codegen/c/effects.rs` — Fail/with-handler emission
- `codegen/c/helpers.rs` — utilities (`coerce_for_assignment`, etc.)
- `codegen/c/plan125.rs` — Plan 125 divergence helpers
  (`expr_diverges_125`, `block_trailing_diverges`, etc.)

**Migration approach:**
1. Создать submodule scaffold (empty `.rs` files + `mod X;` declarations)
2. **Move method-by-method** через `git mv`-style cut+paste:
   - Cut full method body из emit_c.rs
   - Paste в `impl CEmitter { ... }` block в target submodule
   - Cargo check после каждой move — restore если breaks
3. Use `pub(super) fn` для cross-module method calls. Private methods
   на CEmitter остаются `fn`.
4. После каждой группы move — full `cargo build` + targeted regression
   (plan125, plan91_13).

**Gate per group:**
- `cargo build --release` clean
- Targeted regression: plan125 family 61/0 + plan91_13 8/0 PASS
- No semantic changes detectable in C output (binary-compare emitted
  .c files против baseline pre-refactor для sample fixtures)

**Hard cap:** если ≥3 unexplained regressions в одной фазе → revert
группа, переосмыслить cohesion boundary.

### Ф.2 — types/mod.rs split (2-3 days)

**Scope:**
`compiler-codegen/src/types/mod.rs` (18k LOC) → split:
- `types/mod.rs` — TypeCtx struct + Ty enum + entry points
- `types/infer.rs` — `infer_expr_type` + sibling inference functions
- `types/check.rs` — `assignable` / `coerce` / compatibility checks
- `types/effects.rs` — effect inference + tracking
- `types/divergence.rs` — `expr_diverges` / `block_diverges` /
  `stmt_diverges` (Plan 100.7 D165 helpers)
- `types/consume.rs` — Plan 100 D156 consume-walk-* helpers
- `types/mono.rs` — generic mono pipeline в type-checker
- `types/handlers.rs` — handler-body must-diverge / interrupt validation
- `types/plan125_1.rs` — Plan 125.1 never-first-class additions

**Same migration approach** как Ф.1.

**Gate:** full nova test (целиком, not just sample) — 0 regressions.
Type-checker changes исторически регрессируют stdlib (R7/R8 from Plan
125).

### Ф.3 — parser/mod.rs split (1-2 days)

**Scope:**
`compiler-codegen/src/parser/mod.rs` (10k LOC) → split:
- `parser/mod.rs` — Parser struct + entry points (`parse`, `parse_module`)
- `parser/expressions.rs` — Pratt expression parser
- `parser/statements.rs` — statement parser
- `parser/types.rs` — type-ref parser
- `parser/patterns.rs` — pattern parser (для match / if-let / let)
- `parser/declarations.rs` — fn / type / import / module declarations
- `parser/effects.rs` — effect signature parser
- `parser/literals.rs` — literal parsing (strings, numbers, char escapes)

Parser проще split'ить т.к. функции более локальны (recursive descent).

**Gate:** plan125 + plan91_13 + parser-heavy syntax tests PASS.

### Ф.4 — field_cache.rs split (1 day)

**Scope:**
`compiler-codegen/src/field_cache.rs` (12.7k LOC) → split:
- `field_cache/mod.rs` — FieldCacheCtx + public API
- `field_cache/ipa.rs` — IPA analysis (V7.x chain receiver detection)
- `field_cache/loop_body.rs` — V2.x loop-body LICM coordination
- `field_cache/explain.rs` — V5.x explain deep-walk
- `field_cache/heuristics.rs` — V3-V4 various rules

**Gate:** plan123 family regression (V2.1/V5.4/V7.5/V7.6/V7.7) PASS.

### Ф.5 — Cross-cutting helpers extract (1-2 days, optional)

**Scope:**
После Ф.1-Ф.4 — identify utility methods used ≥2 split modules,
extract в shared utility crate / module:
- C-name mangling helpers (`sanitize_for_novaopt`, etc.)
- Diagnostic emission (`err_*` helpers)
- AST traversal patterns (common visitor templates)

**Gate:** zero regression + LOC reduction в каждом split module.

### Ф.6 — Closure: docs + AGENTS.md update (1 day)

**Scope:**
1. `AGENTS.md` — update navigation hints для new structure
2. `docs/architecture/codegen.md` (new) — describe module organization,
   adding-new-features playbook (где emit_X for ExprKind::X goes)
3. `docs/plans/README.md` — mark Plan 129 ✅ CLOSED
4. Internal dev log — entry с total LOC delta + agent
   productivity expectation (до/после tool-call measurement)
5. Optional: announcement в community channels — refactor done, no
   semantic changes

## Acceptance criteria

1. **emit_c.rs** разбит на ≤12 modules, ни один > 5k LOC
2. **types/mod.rs** разбит на ≤8 modules, ни один > 4k LOC
3. **parser/mod.rs** разбит на ≤8 modules, ни один > 2k LOC
4. **field_cache.rs** разбит на ≤6 modules, ни один > 3k LOC
5. **Zero semantic regression** — full `nova test` baseline-clean после
   каждой phase + после final
6. **C output binary-identical** для sample fixtures (plan125,
   plan91_13, plan100_1, plan76 — 50+ fixtures) против pre-refactor
   baseline (или explicit diff explanation если differs)
7. **CEmitter / TypeCtx / Parser struct shapes unchanged** — pure
   method reorganization, не field reorg
8. **Cargo build wall-time** не regressed (cohesive modules should
   help, NOT hurt)
9. **Agent productivity sanity test** — после refactor, spawn agent
   с task "find emit_match_arm_body + understand call sites" — total
   tool calls должен снизиться на ≥30% vs pre-refactor measurement

## Risks + Mitigations

| # | Risk | Mitigation |
|---|---|---|
| R1 | Merge conflicts with concurrent feature PRs | M1: announce in advance, freeze emit_c.rs touches на refactor window |
| R2 | Semantic drift во время cut+paste | M2: method-by-method, full build + plan125 regression после каждой group |
| R3 | Inherent-impl-across-files Rust quirks | M3: prototype в Ф.0 — verify 2-3 methods split works pre-commitment |
| R4 | Hidden cross-module dependencies на private state | M4: audit Ф.0 maps; если field used cross-module → keep на CEmitter via pub(super) accessor |
| R5 | Reviewer fatigue (huge PR series) | M5: split each Ф into 2-4 commits, daily merge cadence, NOT mega-PR |
| R6 | Regression hidden by sampling | M6: full `nova test` gate (1500+ tests) после каждой Ф |
| R7 | Cargo build time regression | M7: measure pre-refactor baseline; abort if >10% slowdown |
| R8 | Refactor creates more mental overhead, not less | M8: agent productivity sanity test (criterion #9) — empirical, not opinion-based |

## Prior lessons (от Plan 125 V1 24-regression)

1. **L1 (codegen-local invariants):** Plan 125 V1 урок — codegen helpers
   must look ONLY at trailing/specific structural positions. Refactor
   разбивает helpers по модулям — НЕ ломать helper-locality invariants.
2. **L2 (regression visibility):** Plan 125 V1 24 регрессии были silent
   runtime UB не CC-FAIL. Phase gate должен включать **runtime** test,
   not just compile.
3. **L3 (helper extraction pattern):** Plan 125 V1 extract'нул
   `block_trailing_diverges` / `expr_diverges_125` cleanly. Аналогичные
   helpers (`assignable_*`, `infer_*`, `coerce_*`) — natural extraction
   candidates.

## Spec impact

**No spec change.** Pure source-tree reorganization. AGENTS.md и
docs/architecture/ updates только.

## Tooling impact

- **Editor navigation:** smaller files → faster fuzzy-finder, better LSP
  performance
- **rust-analyzer indexing:** maybe slight speedup (cohesive modules
  reduce dependency cascade)
- **git blame / log:** WILL fragment — historical archaeology harder.
  Mitigation: `git log --follow` works across renames; in commit message
  write `Plan 129 Ф.N — moved methods from old location`

## Test plan

### Per-phase regression suite

```bash
# After each module-move in Ф.1-Ф.4:
cargo build --release --manifest-path nova-cli/Cargo.toml

# Targeted (fast):
./nova-cli/target/release/nova.exe test nova_tests/plan125 nova_tests/plan91_13

# Per-phase full gate:
./nova-cli/target/release/nova.exe test  # 1500+ tests
```

### Sample C-output diff verification

```bash
# Pre-refactor — capture baselines
for f in nova_tests/plan125/*.nv nova_tests/plan91_13/json_*.nv; do
    cp "${f%.nv}.c" "${f%.nv}.c.baseline"
done

# Post-refactor — diff
for f in nova_tests/plan125/*.nv; do
    if ! diff -q "${f%.nv}.c" "${f%.nv}.c.baseline" > /dev/null; then
        echo "DIFF: $f"
    fi
done
```

Expected: zero diffs (или explained diffs если intentional).

## Deliverables

- `compiler-codegen/src/codegen/c/` — directory с ≤12 submodules
- `compiler-codegen/src/types/` — submodule tree
- `compiler-codegen/src/parser/` — submodule tree
- `compiler-codegen/src/field_cache/` — submodule tree
- `docs/architecture/codegen-decomposition-audit.md` — decomposition map per file
- `docs/architecture/codegen.md` — NEW, module organization doc
- `AGENTS.md` — updated navigation hints
- `docs/plans/README.md` + project-creation.txt + discussion-log.md —
  closure entries

## Связь с другими планами

- **Dependency:** Plan 125 family closed ✅ (avoid mid-refactor codegen
  features)
- **Coordination:** announced freeze of emit_c.rs concurrent edits
  во время refactor window
- **Unblocks:** future agent productivity для любых codegen changes
- **No language-feature dependency** — purely organizational

## Open questions

- **Q1:** `compiler-codegen/src/codegen/emit_c.rs` → `codegen/c/mod.rs`
  или `codegen/c.rs` + `c/` submodule? (Rust supports both; mod.rs
  convention более устоявший)
- ~~**Q2:** Split timing~~ — РЕШЕНО: строго после тега 0.1 (Plan 91
  целиком), чтобы не конкурировать с активным feature-девом в emit_c.rs.
- **Q3:** Дополнительно split `lints.rs`, `manifest.rs`, etc.? Или
  ограничиться 4 крупными файлами?
- **Q4:** Agent productivity metric — measure pre-refactor baseline
  через что? Synthetic prompt + count tool calls?

## Timing

**Estimate (sequential):** 9-15 dev-days
- Ф.0 audit: 1-2d
- Ф.1 emit_c: 3-5d (largest, highest risk)
- Ф.2 types: 2-3d
- Ф.3 parser: 1-2d
- Ф.4 field_cache: 1d
- Ф.5 cross-cutting (optional): 1-2d
- Ф.6 closure: 1d

**Estimate (sequential, рекомендуемый):** 9-15 dev-days. Параллелить
Ф.1-Ф.4 НЕ рекомендуется: emit_c.rs / types/mod.rs / parser/mod.rs
связаны (codegen вызывает типы, парсер строит AST для обоих). Их
одновременный рефактор даёт merge-конфликты в местах стыка — та же
проблема что R1, только внутренняя. Последовательно безопаснее.

**Recommended model:** Opus + Thinking ON для Ф.0 audit (decomposition
boundaries require careful reasoning), Sonnet + High для Ф.1-Ф.4
(mechanical move + verify loops), Opus + Thinking ON для Ф.6 closure.
