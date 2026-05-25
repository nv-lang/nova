# Plan 100.8 — Agent prompt (Sonnet 4.6, HIGH effort, Thinking OFF)

```
Ты implementation-агент Plan 100.8 — performance + IDE / tooling polish для consume-типов.
Работаешь на Nova compiler (Rust). Дата 2026-05-25.

═══════════════════════════════════════════════════════════════════════
МИССИЯ
═══════════════════════════════════════════════════════════════════════

Реализовать Plan 100.8 в worktree `nova-p100-8-tooling`, затем merge
в main. Spec/docs для D166 уже зафиксированы — ты делаешь
ИМПЛЕМЕНТАЦИЮ.

Plan 100.8 = production developer experience для consume-типов:
- LSP quick-fixes для ~12 Plan 100 error codes (auto-add errdefer,
  auto-add consume-marker, add `consume` keyword binding и т.д.)
- LSP hover info (consume status + coverage analysis)
- nova doc integration (consume-badges, Resource lifecycle section)
- Diagnostic format spec D166 (structured format для AI-first parsing)
- CLI `nova consume-analyze` (standalone report)
- Bench: check_consume overhead < 5% budget

Сложность: MEDIUM (polish — нет новых runtime/compiler features,
интеграция existing tools). 3 dev-day. Параллельно с любыми другими
sub-plans (не блокирует).

═══════════════════════════════════════════════════════════════════════
PRIMARY REFERENCES
═══════════════════════════════════════════════════════════════════════

1. docs/plans/100.8-performance-ide-tooling.md — design + Ф.0-Ф.7 + 15 фикстур
2. docs/plans/100.1-impl-playbook.md — pre-requisites PRE-1..4
3. spec/decisions/09-tooling.md §D166 — нормативный spec
4. docs/plans/100-remaining-impl-roadmap.md §«Plan 100.8» — launch params
5. Plan 45 — nova doc renderer
6. Plan 50 — D102 suggestion infrastructure
7. Plan 57 — bench framework
8. Plan 28 / Plan 36 — CLI subcommand pattern

═══════════════════════════════════════════════════════════════════════
WORKTREE / BRANCH
═══════════════════════════════════════════════════════════════════════

Worktree: d:/Sources/nv-lang/nova-p100-8-tooling
Branch:   plan-100-8-tooling

═══════════════════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════════════════

1. WORKTREE: git worktree add d:/Sources/nv-lang/nova-p100-8-tooling -b plan-100-8-tooling main

2. PRE-2/3/4 setup СТРОГО по 100.1-impl-playbook.md.
   Baseline: plan73=12/0, plan100_1=23/0, plan100_4_3=11/0.

3. ИДИ ПО ФАЗАМ Ф.0-Ф.7 (см. plan-doc).

4. Ключевые блоки реализации:
   - **LSP quick-fixes:** handlers для ~12 D133-* error codes. Каждый
     handler — mechanical (text-replacement suggestions). Cm. Plan 50
     D102 pattern.
   - **LSP hover:** при наведении на consume-typed var — показать
     «consume status: Live | Consumed | MaybeConsumed» + список
     consume-methods + currently-covering errdefer/okdefer.
   - **nova doc:** добавить consume-badge на typed-pages + новая
     секция «Resource lifecycle» для consume-типов.
   - **D166 diagnostic format:** structured JSON output mode для
     AI-first parsing (parallel с existing human-readable).
   - **nova consume-analyze CLI:** запускает check_consume + emit
     report (per-fn coverage, consume-types in use, defer-family usage).
   - **Bench:** Plan 57 bench для check_consume overhead. Acceptance
     < 5% vs baseline.

5. После Ф.6 verify: nova test plan100_8 (15/0) + plan100_1 (23/0) +
   plan73 (12/0) + plan100_4_3 (11/0) + LSP tests + bench.

6. ФИНАЛЬНЫЙ MERGE (после Ф.7):
   cd d:/Sources/nv-lang/nova
   git fetch github main; git merge --no-ff plan-100-8-tooling
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
   - Diagnostic format breaking change — если existing CI tools relyют
     на формат, добавить dual-output transition (старый + новый).
   - LSP performance — quick-fix evaluation costly → lazy eval/caching.

═══════════════════════════════════════════════════════════════════════
THINKING + EFFORT
═══════════════════════════════════════════════════════════════════════

User поставит Effort = HIGH в UI.
Thinking = OFF — polish/integration работа, нет non-trivial design.

═══════════════════════════════════════════════════════════════════════
ESCALATION
═══════════════════════════════════════════════════════════════════════

Если застрял (3+ попыток):
1. Marker [M-100.8-impl-<topic>] в simplifications.
2. Discussion-log + приостановка. User escalate на Opus 4.7.

ОЖИДАЕМО: 15/0 на plan100_8 (10 pos + 5 neg) + LSP quick-fix tests +
bench check_consume overhead < 5%. 0 регрессий.

СТАРТ: git worktree add ...

Поехали.
```
