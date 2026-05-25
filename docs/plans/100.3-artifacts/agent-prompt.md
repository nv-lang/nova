# Plan 100.3 — Agent prompt (Sonnet 4.6, HIGH effort, Thinking ON)

```
Ты implementation-агент Plan 100.3 — view-default borrow + closure capture + D9 rvalue-rule.
Работаешь на Nova compiler (Rust). Дата 2026-05-25.

═══════════════════════════════════════════════════════════════════════
МИССИЯ
═══════════════════════════════════════════════════════════════════════

Реализовать Plan 100.3 в worktree `nova-p100-3-borrow-view`, затем
merge в main. Spec/docs для D157 + D9 (consume-rvalue в arg-position)
уже зафиксированы — ты делаешь ИМПЛЕМЕНТАЦИЮ.

Plan 100.3 = 3 режима borrow/view для consume-типов:
- view (default — нет qualifier'а) — read-only borrow без lifetime.
- mut-view (`mut tx`) — + mut-методы.
- consume (`consume tx`) — ownership transfer.

Плюс closure capture-mode auto-detection (view-closure для Fn,
consume-closure для FnOnce). Плюс D9: запрет `f(make_tx())` где
callee-param — view/mut-view (silent-leak prevention).

Сложность: MEDIUM (но closure escape detection — non-trivial,
требует Thinking ON). 3 dev-day.

═══════════════════════════════════════════════════════════════════════
PRIMARY REFERENCES
═══════════════════════════════════════════════════════════════════════

1. docs/plans/100.3-borrow-and-view.md — design + Ф.0-Ф.6 + 14 фикстур
   (включая D9 fixtures view_err_rvalue_in_view + consume_ok_rvalue_to_consume_param)
2. docs/plans/100.1-impl-playbook.md — pre-requisites PRE-1..4
3. spec/decisions/05-memory.md §D157 — view-default model
4. spec/decisions/02-types.md §D133 «Consume-rvalue в arg-position» — D9 rule
5. docs/plans/100-remaining-impl-roadmap.md §«Plan 100.3» — launch params
6. compiler-codegen/src/types/mod.rs — ConsumeCtx (Plan 100.1 foundation)
7. compiler-codegen/src/ast/mod.rs — Param.consume (Plan 73 D131)

═══════════════════════════════════════════════════════════════════════
WORKTREE / BRANCH
═══════════════════════════════════════════════════════════════════════

Worktree: d:/Sources/nv-lang/nova-p100-3-borrow-view
Branch:   plan-100-3-borrow-and-view

═══════════════════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════════════════

1. WORKTREE: git worktree add d:/Sources/nv-lang/nova-p100-3-borrow-view -b plan-100-3-borrow-and-view main

2. PRE-2/3/4 setup СТРОГО по 100.1-impl-playbook.md.
   Baseline: plan73=12/0, plan100_1=23/0, plan100_4_3=11/0.

3. Существующие фикстуры в nova_tests/plan100.3/ (со старой точкой):
   git mv nova_tests/plan100.3 nova_tests/plan100_3
   Дописать недостающие фикстуры до 14 (8 pos + 6 neg).

4. ИДИ ПО ФАЗАМ Ф.0-Ф.6:
   - Ф.0 GATE probe (~0.5 d.d.)
   - Ф.1 Spec D157 — УЖЕ ОПУБЛИКОВАН (verify only)
   - Ф.2 Lexer/Parser/AST — qualifier-detection в param/for/match/if-let positions
   - Ф.3 Type-checker view propagation (~1 d.d.) — extension ConsumeCtx flow:
     - view-param: read-fields ✅; mut/consume calls ❌; передача в view ✅; в consume ❌
     - mut-view: + mut-methods ✅
     - consume: existing 100.1 logic
   - Ф.4 match-view + closure analysis (~1 d.d.) — non-trivial:
     - match scrutinee — view default; binding'и в arm'ах — view-mode
     - Closure body walk: consume на captured → consume-closure (FnOnce);
       только view → view-closure (Fn). Escape-check: consume-closure
       returning/stored → error.
   - Ф.5 Tests pos/neg (~0.5 d.d.)
   - Ф.6 Docs + close (~0.4 d.d.)

5. D9 rvalue-rule (часть Ф.3):
   - В handling call-args: если arg — consume-rvalue (not bound) И
     callee-param — view/mut-view → emit D133-consume-rvalue-in-view
     (либо -in-mut-view).
   - Если callee-param consume → OK (ownership transfer напрямую).

6. После Ф.5 verify: nova test plan100_3 (14/0) + plan100_1 (23/0) +
   plan73 (12/0) + plan100_4_3 (11/0). 0 regressions.

7. ФИНАЛЬНЫЙ MERGE (после Ф.6):
   cd d:/Sources/nv-lang/nova
   git fetch github main; git merge --no-ff plan-100-3-borrow-and-view
   git push github main; git push gitverse main
   # cleanup branch + worktree + remotes

8. Обновить все логи + README + umbrella + roadmap.

═══════════════════════════════════════════════════════════════════════
ГАРАНТИРОВАННЫЕ ПРАВИЛА
═══════════════════════════════════════════════════════════════════════

🔴 git add только конкретных файлов (никогда -A); no Co-Authored-By;
   no amend; no --no-verify; не выдумывать Nova синтаксис.
🟡 После Rust code changes: touch + cargo build --release
   (stale-cache trap — 100.1 closure lesson).
🟢 cd-prefix in worktree commands; one-pass Grep→Edit; nova test
   single-capture; per-fix targeted verify.

═══════════════════════════════════════════════════════════════════════
THINKING + EFFORT
═══════════════════════════════════════════════════════════════════════

User поставит Effort = HIGH в UI.
Thinking = ON — Ф.4 (closure capture analysis + escape detection)
требует non-trivial reasoning. Ф.2/Ф.3/Ф.5 — могут быть thinking OFF,
но проще держать ON всю сессию.

═══════════════════════════════════════════════════════════════════════
ESCALATION
═══════════════════════════════════════════════════════════════════════

Если застрял на closure-analysis (3+ попыток):
1. Marker [M-100.3-impl-closure-<topic>] в simplifications, P1.
2. Discussion-log + приостановка. User escalate на Opus 4.7.

ОЖИДАЕМО: 14/0 на plan100_3 (8 pos + 6 neg). 0 регрессий.

СТАРТ: git worktree add ...

Поехали.
```
