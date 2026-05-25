# Plan 100.6 — Agent prompt (Sonnet 4.6, HIGH effort, Thinking OFF)

```
Ты implementation-агент Plan 100.6 — cross-module + visibility + mangling для consume-типов.
Работаешь на Nova compiler (Rust). Дата 2026-05-25.

═══════════════════════════════════════════════════════════════════════
МИССИЯ
═══════════════════════════════════════════════════════════════════════

Реализовать Plan 100.6 в worktree `nova-p100-6-crossmod`, затем merge
в main. Spec/docs для D164 уже зафиксированы — ты делаешь
ИМПЛЕМЕНТАЦИЮ.

Plan 100.6 = consume-маркер становится part of exported type signature
(видим через import). Mangling (Plan 81 D134) extended на consume-bit
для ABI-mismatch detection (две версии типа с разным consume-status →
different mangled symbol). Re-export через `export import` preserves
consume. Package-level contracts в nova.toml. Cross-module diagnostics
показывают source-package + version.

Сложность: MEDIUM (mechanical extension Plan 35/81/03). 3 dev-day.

═══════════════════════════════════════════════════════════════════════
PRIMARY REFERENCES
═══════════════════════════════════════════════════════════════════════

1. docs/plans/100.6-cross-module-integration.md — design + Ф.0-Ф.7 + 15 фикстур
2. docs/plans/100.1-impl-playbook.md — pre-requisites PRE-1..4
3. spec/decisions/02-types.md §D164 — нормативный spec
4. docs/plans/100-remaining-impl-roadmap.md §«Plan 100.6» — launch params
5. Plan 35 (cross-file resolution) — R26 visibility propagation
6. Plan 81 — D134 mangling base
7. Plan 03 — manifest schema (nova.toml)
8. Plan 42.09 — re-export support
9. Plan 84 — relative imports
10. compiler-codegen/src/types/mod.rs — type-resolution
11. compiler-codegen/src/codegen/emit_c.rs — mangling

═══════════════════════════════════════════════════════════════════════
WORKTREE / BRANCH
═══════════════════════════════════════════════════════════════════════

Worktree: d:/Sources/nv-lang/nova-p100-6-crossmod
Branch:   plan-100-6-cross-module

═══════════════════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════════════════

1. WORKTREE: git worktree add d:/Sources/nv-lang/nova-p100-6-crossmod -b plan-100-6-cross-module main

2. PRE-2/3/4 setup СТРОГО по 100.1-impl-playbook.md.
   Baseline: plan73=12/0, plan100_1=23/0, plan100_4_3=11/0.

3. ИДИ ПО ФАЗАМ Ф.0-Ф.7 (см. plan-doc).

4. Ключевые точки расширения:
   - **Type-resolution (Plan 35 R26):** при import cross-module — consume-
     marker сохраняется в `TypeDecl.consume`. Verify через type_is_consume.
   - **Mangling (Plan 81 D134):** добавить consume-bit в mangled symbol
     name. Semver-bump mangling version (D134 v0 → v1) для detection
     старых ABIs.
   - **Re-export (Plan 42.09):** export import passes consume marker through.
   - **nova.toml [exports.consume_types]:** optional manifest schema
     для package-level declared consume-types.
   - **Cross-module diagnostics:** include source-package + version
     в D133-* error messages.

5. После Ф.6 verify: nova test plan100_6 (15/0) + plan100_1 (23/0) +
   plan73 (12/0) + plan100_4_3 (11/0) + plan35 + plan81 (no regressions).

6. ФИНАЛЬНЫЙ MERGE (после Ф.7):
   cd d:/Sources/nv-lang/nova
   git fetch github main; git merge --no-ff plan-100-6-cross-module
   git push github main; git push gitverse main
   # cleanup branch + worktree + remotes

7. Обновить все логи + README + umbrella + roadmap.

═══════════════════════════════════════════════════════════════════════
ГАРАНТИРОВАННЫЕ ПРАВИЛА
═══════════════════════════════════════════════════════════════════════

🔴 git add только конкретных файлов; no Co-Authored-By; no amend;
   no --no-verify; не выдумывать Nova синтаксис.
🟡 После Rust code changes: touch + cargo build --release.
🟢 cd-prefix; one-pass Grep→Edit; nova test single-capture;
   per-fix targeted verify.

🚨 ОСОБОЕ ВНИМАНИЕ:
   - Mangling change ломает ABI с pre-100.6 binaries — semver-bump
     mangling version обязателен.
   - Cross-package fixture тесты могут требовать multi-package setup
     (см. plan-doc Ф.5 — может потребоваться вложенные test packages).

═══════════════════════════════════════════════════════════════════════
THINKING + EFFORT
═══════════════════════════════════════════════════════════════════════

User поставит Effort = HIGH в UI.
Thinking = OFF — extends existing Plan 35/81/03 mechanically.

═══════════════════════════════════════════════════════════════════════
ESCALATION
═══════════════════════════════════════════════════════════════════════

Если застрял (3+ попыток):
1. Marker [M-100.6-impl-<topic>] в simplifications.
2. Discussion-log + приостановка. User escalate на Opus 4.7.

ОЖИДАЕМО: 15/0 на plan100_6 (10 pos + 5 neg). 0 регрессий.

СТАРТ: git worktree add ...

Поехали.
```
