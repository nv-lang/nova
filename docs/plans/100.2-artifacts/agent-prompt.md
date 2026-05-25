# Plan 100.2 — Agent prompt (Sonnet 4.6, HIGH effort, Thinking OFF)

```
Ты implementation-агент Plan 100.2 — generic propagation `[T consume]` bound + collection-aware.
Работаешь на Nova compiler (Rust). Дата 2026-05-25.

═══════════════════════════════════════════════════════════════════════
МИССИЯ
═══════════════════════════════════════════════════════════════════════

Реализовать Plan 100.2 в worktree `nova-p100-2-generic`, затем merge
в main. Spec/docs для D156 уже зафиксированы — ты делаешь
ИМПЛЕМЕНТАЦИЮ против чёткого контракта.

Plan 100.2 = новый generic-bound `[T consume]` — opt-in strict-mode,
закрывающий silent-leak в generic-коде типа
`fn first[T](pair (T,T)) -> T => pair.0` (тихо теряет pair.1 если
T = consume-тип). Без bound — silent-ignore (backward-compat).
С bound — внутри generic-body T трактуется как possibly-consume,
forget → error. Плюс collection-aware iteration (for tx in []Tx).

Сложность: MEDIUM (mechanical extension AST + checker). 4 dev-day.

═══════════════════════════════════════════════════════════════════════
PRIMARY REFERENCES
═══════════════════════════════════════════════════════════════════════

1. docs/plans/100.2-generic-propagation.md — design + Ф.0-Ф.7 + 15 фикстур
2. docs/plans/100.1-impl-playbook.md — pre-requisites PRE-1..4 (vcpkg paths,
   env vars, libuv, baseline). ИСПОЛЬЗУЙ как reference для setup.
3. spec/decisions/02-types.md §D156 — нормативный spec
4. docs/plans/100-remaining-impl-roadmap.md §«Plan 100.2» — launch params
5. compiler-codegen/src/types/mod.rs — где живёт `LinearityRegistry::type_is_consume`
   (Plan 100.1 foundation) — твоя точка расширения

═══════════════════════════════════════════════════════════════════════
WORKTREE / BRANCH
═══════════════════════════════════════════════════════════════════════

Worktree: d:/Sources/nv-lang/nova-p100-2-generic
Branch:   plan-100-2-generic-propagation

═══════════════════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════════════════

1. WORKTREE: git worktree add d:/Sources/nv-lang/nova-p100-2-generic -b plan-100-2-generic-propagation main

2. PRE-2/3/4 setup СТРОГО по 100.1-impl-playbook.md:
   - $env:NOVA_GC_INCLUDE_DIR / NOVA_GC_LIB_DIR на main vcpkg
   - libuv submodule check
   - cargo build --release
   - baseline: plan73=12/0, plan100_1=23/0, plan100_4_3=11/0

3. ИДИ ПО ФАЗАМ Ф.0-Ф.7 (см. plan-doc). После каждой фазы — commit с
   осмысленным message + targeted fixture verify.

4. AST extension: добавь `consume_bound: bool` в `GenericParam`
   (compiler-codegen/src/ast/mod.rs:554). Update constructors.

5. Parser: extend `parse_generic_decl_params` для `[T consume]` форма
   (mirror к existing `[T Bound]` syntax).

6. Type-checker: extend `LinearityRegistry::type_is_consume` —
   generic-param с `consume_bound: true` трактуется как consume.
   Внутри generic-body — strict flow analysis (как для concrete consume-type).

7. После Ф.6 — verify regression:
   nova test plan100_1 (23/0), plan73 (12/0), plan100_4_3 (11/0).

8. ФИНАЛЬНЫЙ MERGE (после Ф.7):
   cd d:/Sources/nv-lang/nova
   git fetch github main
   git merge --no-ff plan-100-2-generic-propagation
   git push github main; git push gitverse main
   # cleanup: branch -d + worktree remove + remote delete

9. Обновить ВСЕ: project-creation.txt + simplifications.md +
   nova-private/discussion-log.md + README row + umbrella +
   100-remaining-impl-roadmap.md row.

═══════════════════════════════════════════════════════════════════════
ГАРАНТИРОВАННЫЕ ПРАВИЛА
═══════════════════════════════════════════════════════════════════════

🔴 git add только конкретных файлов (никогда -A); no Co-Authored-By;
   no amend; no --no-verify; не выдумывать Nova синтаксис.
🟡 После Rust code changes: touch файла + cargo build --release
   (избежать stale-cache — известный gotcha из 100.1 closure).
🟢 cd-prefix in worktree commands; one-pass Grep→Edit; nova test
   single-capture; per-fix targeted verify.

═══════════════════════════════════════════════════════════════════════
THINKING + EFFORT
═══════════════════════════════════════════════════════════════════════

User поставит Effort = HIGH в UI.
Thinking = OFF — план mechanical (расширение существующего
LinearityRegistry + AST + parser).

═══════════════════════════════════════════════════════════════════════
ESCALATION
═══════════════════════════════════════════════════════════════════════

Если застрял (3+ попытки fix не помогли):
1. Marker [M-100.2-impl-<topic>] в simplifications с P-приоритетом.
2. Опиши в discussion-log; приостанови. User escalate на Opus 4.7.

ОЖИДАЕМО: 15/0 на plan100_2 (9 pos + 6 neg). 0 регрессий.

СТАРТ: git worktree add ...

Поехали.
```
