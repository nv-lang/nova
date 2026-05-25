// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100 — Remaining Implementation Roadmap (Sonnet 4.6 launch guide)

> **Создан 2026-05-25 после закрытия [Plan 100.1 foundation](100.1-core-must-consume.md)**
> (merge `ab60167f3e5`). Цель — single source of truth для launching
> Sonnet 4.6 агентов на оставшихся sub-plans Plan 100 family.
>
> **Что закрыто:** 100.1 foundation ✅ (parser + check_consume + LinearityRegistry + 23/0 fixtures).
>
> **Что осталось:** 11 sub-plans, ~42 dev-day, можно параллелить (см. §«Sequencing»).

---

## Текущий статус Plan 100 family

| Plan | Статус | dev-day | D-block |
|---|---|---|---|
| 100.1 foundation | ✅ ЗАКРЫТ 2026-05-25 (merge `ab60167f3e5`) | 5 | D133 |
| 100.2 generic propagation | 📋 unblocked | 4 | D156 |
| 100.3 borrow/view + D9 rvalue-rule | 📋 unblocked | 3 | D157 |
| 100.4 cleanup-on-failure umbrella | 📋 unblocked | 14 (5 sub) | D158-D162 |
| 100.4.1 failable cleanup body | 📋 unblocked | 3 | D158 |
| 100.4.2 async/suspend cleanup | 📋 GATED on 100.4.1 | 3 | D159 |
| 100.4.3 okdefer + reason-aware | 📋 unblocked | 2 | D160 |
| 100.4.4 multi-defer error accumulation | 📋 GATED on 100.4.1 | 3 | D161 |
| 100.4.5 consume-integration final | 📋 GATED on 100.4.1-4 | 3 | D162 |
| 100.5 FFI / external integration | 📋 unblocked | 4 | D163 |
| 100.6 cross-module + visibility + mangling | 📋 unblocked | 3 | D164 |
| 100.7 stdlib migration playbook | 📋 GATED on 100.1-6 | 3 | D165 |
| 100.8 performance + IDE / tooling | 📋 unblocked (polish) | 3 | D166 |

**Итого осталось:** ~42 dev-day. Критический путь (with parallelization): **~6 недель** wall-clock (1 агент per plan, sub-plans 100.2/100.3/100.5/100.8 параллельно).

---

## Sequencing — рекомендованный порядок запуска агентов

```
                       ┌──────────────────────────┐
                       │  100.1 ✅ FOUNDATION     │
                       └────────────┬─────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
              ▼                     ▼                     ▼
         ┌────────┐            ┌────────┐            ┌────────┐
         │ 100.2  │ ◄────────► │ 100.3  │ ◄────────► │ 100.5  │
         │ 4 d.d. │  parallel  │ 3 d.d. │  parallel  │ 4 d.d. │
         └───┬────┘            └───┬────┘            └───┬────┘
             │                     │                     │
             └─────────┬───────────┴─────────┬───────────┘
                       │                     │
                       ▼                     ▼
                  ┌────────┐            ┌──────────┐
                  │ 100.6  │            │  100.4   │
                  │ 3 d.d. │            │ umbrella │
                  └───┬────┘            │ 14 d.d.  │
                      │                 └─────┬────┘
                      │                       │ (sub-sub seq.)
                      │                       ▼
                      │           ┌─────────────────────┐
                      │           │ 100.4.1 (3)         │
                      │           │  └▶ 100.4.2 (3)     │
                      │           │  └▶ 100.4.3 (2) ║   │
                      │           │  └▶ 100.4.4 (3)     │
                      │           │       └▶ 100.4.5 (3)│
                      │           └───────────┬─────────┘
                      └──────────┬────────────┘
                                 │
                                 ▼
                            ┌────────┐
                            │ 100.7  │  (stdlib pilots)
                            │ 3 d.d. │
                            └────────┘

100.8 (3 d.d., polish/LSP) — параллельно anytime после 100.1.
```

### Critical path (~6 weeks)
1. Week 1-2: 100.2 + 100.3 + 100.5 (параллельно, 3 агента)
2. Week 2-3: 100.4.1 → {100.4.2, 100.4.3} parallel → 100.4.4 → 100.4.5
3. Week 4: 100.6 (после 100.2/3/5)
4. Week 5: 100.7 (stdlib pilots)
5. Week 6: 100.8 (polish, параллельно весь период)

---

## Sonnet 4.6 launch parameters per plan

Все запускаются в собственном worktree `nova-p100-NN-*`. Все используют общий setup из [100.1-impl-playbook.md](100.1-impl-playbook.md) PRE-1..4 (правильные пути vcpkg/libuv).

### 🟢 Plan 100.2 — generic propagation `[T consume]`

| Параметр | Значение |
|---|---|
| **Сложность** | MEDIUM |
| **Effort** | **HIGH** |
| **Thinking** | **OFF** (mechanical extension parser+checker) |
| **Sessions** | 4-6 sessions × 1-1.5 h |
| **Worktree** | `nova-p100-2-generic` |
| **Branch** | `plan-100-2-generic-propagation` |
| **Калибровка** | `nova test plan100_2` должен дать 15/0 (9 pos + 6 neg) |
| **Риски** | Tuple-destructure ambiguity — force explicit consume params |

### 🟢 Plan 100.3 — borrow/view + D9 rvalue-rule

| Параметр | Значение |
|---|---|
| **Сложность** | MEDIUM |
| **Effort** | **HIGH** |
| **Thinking** | **ON** (closure escape detection — non-trivial) |
| **Sessions** | 4-6 sessions × 1-1.5 h |
| **Worktree** | `nova-p100-3-borrow-view` |
| **Branch** | `plan-100-3-borrow-and-view` |
| **Калибровка** | `nova test plan100_3` должен дать 14/0 (8 pos + 6 neg, включая D9 fixtures) |
| **Риски** | Closure escape detection; match-view distinct от Plan 73 alias |

### 🔶 Plan 100.4.1 — failable cleanup body (D158)

| Параметр | Значение |
|---|---|
| **Сложность** | MEDIUM |
| **Effort** | **HIGH** |
| **Thinking** | **ON** (effect-checker integration + Plan 49 composition) |
| **Sessions** | 4-5 sessions × 1.5-2 h |
| **Worktree** | `nova-p100-4-1-failable` |
| **Branch** | `plan-100-4-1-failable-cleanup` |
| **Калибровка** | `nova test plan100_4_1` 18/0 (12 pos + 6 neg) |
| **Риски** | D90 §4 amend — backward-compat existing handler code (zero migration ожидается) |

### 🔶 Plan 100.4.2 — async/suspend cleanup (D159)

| Параметр | Значение |
|---|---|
| **Сложность** | MEDIUM |
| **Effort** | **HIGH** |
| **Thinking** | **ON** (cancel-shielding semantics) |
| **GATED** | После 100.4.1 |
| **Sessions** | 4-5 sessions × 1.5-2 h |
| **Worktree** | `nova-p100-4-2-async` |
| **Branch** | `plan-100-4-2-async-suspend-cleanup` |
| **Калибровка** | `nova test plan100_4_2` 15/0 |
| **Риски** | Deadlock при cleanup ждёт cancel-related ресурс → Time.timeout обязателен |

### 🟢 Plan 100.4.3 — okdefer + reason-aware (D160)

| Параметр | Значение |
|---|---|
| **Сложность** | **LOW-MEDIUM** (mechanical extension defer) |
| **Effort** | **HIGH** |
| **Thinking** | **OFF** |
| **Sessions** | 3-4 sessions × 1 h |
| **Worktree** | `nova-p100-4-3-okdefer` |
| **Branch** | `plan-100-4-3-okdefer-reason-aware` |
| **Калибровка** | `nova test plan100_4_3` 12/0 |
| **Риски** | Минимальные — добавляется keyword + mirror AST nodes от existing defer |

### 🔴 Plan 100.4.4 — multi-defer error accumulation (D161)

| Параметр | Значение |
|---|---|
| **Сложность** | **HIGH** (fundamental runtime change) |
| **Effort** | **HIGH** |
| **Thinking** | **ON** (defer-stack должен continue despite errors — non-Rust pattern) |
| **GATED** | После 100.4.1 (+ 100.4.2 рекомендуется) |
| **Sessions** | 5-7 sessions × 1.5-2 h |
| **Worktree** | `nova-p100-4-4-multidefer` |
| **Branch** | `plan-100-4-4-multi-defer-error-accumulation` |
| **Калибровка** | `nova test plan100_4_4` 16/0 |
| **Риски** | Panic composition; memory pressure при many failures (composite-error N suppressed) |

### 🔶 Plan 100.4.5 — consume-integration final (D162)

| Параметр | Значение |
|---|---|
| **Сложность** | **MEDIUM-HIGH** |
| **Effort** | **HIGH** |
| **Thinking** | **ON** (check_consume multi-path enumeration) |
| **GATED** | После ВСЕХ 100.4.1-4 |
| **Sessions** | 4-5 sessions × 1.5-2 h |
| **Worktree** | `nova-p100-4-5-final` |
| **Branch** | `plan-100-4-5-consume-integration` |
| **Калибровка** | `nova test plan100_4_5` 20/0 |
| **Риски** | D90 §7 amend (interrupt → errdefer) — breaking для existing handler code; обязательный Ф.0 audit |

### 🔶 Plan 100.5 — FFI / external integration (D163)

| Параметр | Значение |
|---|---|
| **Сложность** | **MEDIUM-HIGH** |
| **Effort** | **HIGH** |
| **Thinking** | **ON** (capability integration + C-runtime helpers — cross-cutting) |
| **Sessions** | 5-7 sessions × 1.5-2 h |
| **Worktree** | `nova-p100-5-ffi` |
| **Branch** | `plan-100-5-ffi-external-integration` |
| **Калибровка** | `nova test plan100_5` 18/0 |
| **Риски** | C-side defensive helpers — programmer correctness; mitigation: nv_consume_validate обязателен |

### 🔶 Plan 100.6 — cross-module + mangling (D164)

| Параметр | Значение |
|---|---|
| **Сложность** | MEDIUM |
| **Effort** | **HIGH** |
| **Thinking** | **OFF** (extends existing Plan 35/81/03 — mechanical) |
| **Sessions** | 3-4 sessions × 1.5 h |
| **Worktree** | `nova-p100-6-crossmod` |
| **Branch** | `plan-100-6-cross-module` |
| **Калибровка** | `nova test plan100_6` 15/0 |
| **Риски** | Mangling-bit collision с Plan 81 D134 v0 → semver-bump mangling version |

### 🔶 Plan 100.7 — stdlib migration playbook (D165)

| Параметр | Значение |
|---|---|
| **Сложность** | MEDIUM |
| **Effort** | **HIGH** |
| **Thinking** | **OFF** (integration existing tooling) |
| **GATED** | После 100.1+100.2+100.3+100.4+100.5+100.6 |
| **Sessions** | 4-5 sessions × 1.5 h |
| **Worktree** | `nova-p100-7-stdlib` |
| **Branch** | `plan-100-7-stdlib-migration` |
| **Калибровка** | 4 pilot migrations PASS + `nova consume-migrate` CLI работает |
| **Риски** | Edition split debugging; migration-tool false-positives → --interactive mode |

### 🟢 Plan 100.8 — performance + IDE / tooling (D166)

| Параметр | Значение |
|---|---|
| **Сложность** | MEDIUM (polish) |
| **Effort** | **HIGH** |
| **Thinking** | **OFF** (LSP/doc integration straightforward) |
| **Параллель** | ANYTIME после 100.1 — не блокирует ничего |
| **Sessions** | 4-5 sessions × 1.5 h |
| **Worktree** | `nova-p100-8-tooling` |
| **Branch** | `plan-100-8-tooling` |
| **Калибровка** | LSP quick-fix tests 10+; `nova consume-analyze` CLI; bench overhead < 5% |
| **Риски** | LSP performance — quick-fix evaluation costly → lazy eval/caching |

---

## Промпт-шаблон для запуска агента

Для каждого sub-plan используй adapted version промпта из закрытия 100.1:

```
Ты implementation-агент Plan 100.X (<краткое описание>).
Работаешь на Nova compiler (Rust). Текущая дата 2026-05-25.

═══════════════════════════════════════════════════════════════════════
МИССИЯ
═══════════════════════════════════════════════════════════════════════

Реализовать Plan 100.X в worktree `<nova-p100-N-name>`,
затем merge в main. Spec/docs/fixtures уже зафиксированы Ред. 2 — ты
делаешь ИМПЛЕМЕНТАЦИЮ против чёткого контракта.

Plan 100.X = <одно предложение цели>.

═══════════════════════════════════════════════════════════════════════
PRIMARY REFERENCE
═══════════════════════════════════════════════════════════════════════

1. docs/plans/100.X-<name>.md — design + фазы Ф.0-Ф.7
2. docs/plans/100.1-impl-playbook.md — pre-requisites (PRE-1..4):
   правильные пути vcpkg, env vars, libuv, baseline. ИСПОЛЬЗУЙ как
   reference для setup.
3. spec/decisions/<file>.md §D<NNN> — нормативный spec

═══════════════════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════════════════

1. WORKTREE SETUP (из main repo cwd):
   git worktree add d:/Sources/nv-lang/nova-p100-N-name -b plan-100-N-name main

2. ENV vars (vcpkg + libuv) — см. 100.1-impl-playbook.md PRE-2.

3. BASELINE `nova test` в worktree. Записать XXXX PASS / YY FAIL.

4. Для GATED plans — проверить что dependency plan уже merged
   (e.g. для 100.4.5 проверить наличие D158-D161 в spec).

5. ИДИ ПО ФАЗАМ. После каждой фазы — commit + targeted fixture verify.

6. ФИНАЛЬНЫЙ MERGE (после Ф.7 done):
   cd d:/Sources/nv-lang/nova
   git merge --no-ff plan-100-N-name
   git push github main
   git push gitverse main
   # cleanup remote + local branch + worktree

═══════════════════════════════════════════════════════════════════════
ГАРАНТИРОВАННЫЕ ПРАВИЛА (memory + project)
═══════════════════════════════════════════════════════════════════════

🔴 НИКОГДА git add -A / .  — только конкретные файлы.
🔴 НИКОГДА Co-Authored-By: Claude trailer.
🔴 НИКОГДА амендить commits.
🔴 НИКОГДА выдумывать Nova синтаксис — смотри spec/decisions/ + examples/.
🔴 НИКОГДА не переходи в next sub-step пока current не green.
🔴 НИКОГДА --no-verify / skip pre-commit hooks.

🟢 Bash cwd = main repo. Для worktree — обязательный cd-префикс.
🟢 Один проход на исправление: Grep → Edit за один запуск.
🟢 nova test один раз: capture summary + FAIL details одной командой.
🟢 Per-fix verify — только targeted fixture; full nova test в конце фазы.

═══════════════════════════════════════════════════════════════════════
THINKING MODE / EFFORT
═══════════════════════════════════════════════════════════════════════

Effort: HIGH (поставит user в UI).
Thinking: <ON|OFF> — см. таблицу в roadmap.

═══════════════════════════════════════════════════════════════════════
ESCALATION
═══════════════════════════════════════════════════════════════════════

Если застрял (regression не диагностируется, fixture EXPECT не
совпадает > 3 попыток) — НЕ ломай дальше, НЕ удаляй tests как
"limitation". Вместо этого:
1. Зафиксируй blocker в commit-marker (отдельная ветка).
2. Запиши marker в docs/simplifications.md с [M-100.X-impl-<topic>] + P-приоритет.
3. Опиши в discussion-log; приостанови. User escalate на Opus 4.7.

═══════════════════════════════════════════════════════════════════════
СТАРТ
═══════════════════════════════════════════════════════════════════════

Первая команда — git worktree add ... Затем читай plan-doc + playbook PRE-1..4.

Поехали.
```

---

## Calendar estimate

| Strategy | Calendar | Описание |
|---|---|---|
| **Sequential** | ~10-12 недель | 1 агент за раз; safest |
| **Parallel (recommended)** | **~6 недель** | 100.2+100.3+100.5 параллельно; 100.4 sub-sub sequence; 100.6/100.7/100.8 в финале |
| **Aggressive parallel** | ~4-5 недель | + 100.8 от начала, риск merge conflicts |

---

## Что предлагаю запускать первым

1. **Plan 100.4.3 (okdefer, LOW-MEDIUM, ~2 dev-day)** — самый простой, не GATED, быстрая победа. Хороший warm-up для агента.

2. **Plan 100.2 (generic propagation, MEDIUM, ~4 dev-day)** + **Plan 100.3 (borrow/view, MEDIUM, ~3 dev-day)** — параллельно. Mid-complexity, unblock много downstream.

3. **Plan 100.4.1 (failable body, MEDIUM, ~3 dev-day)** — после 100.4.3 closed. Foundation для 100.4.2/4.4/4.5.

4. **Plan 100.5 (FFI, MEDIUM-HIGH, ~4 dev-day)** — параллельно с 100.4.X.

5. **Plan 100.8 (tooling, MEDIUM polish, ~3 dev-day)** — anytime, не блокирует.
