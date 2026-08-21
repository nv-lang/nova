---
name: feedback-worktree-cwd-clarity
description: "Bash cwd ПЕРСИСТИТ между вызовами и ДРЕЙФУЕТ (не «всегда main»). Для worktree-работы каждую git/cargo команду префиксовать абсолютным `cd <worktree>` + verify pwd; не полагаться на персистнутый cwd."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 1b171c1a-1ebe-41ac-8384-bdab1b0efdce
---

**КОРРЕКЦИЯ (2026-06-30):** прежняя версия утверждала «cwd всегда стартует в main». Это
НЕВЕРНО. Bash-харнесс **ПЕРСИСТИТ cwd между вызовами** (per tool-desc «Working directory
persists between calls») — после `cd X` следующая команда стартует в X, НЕ в main. cwd
ДРЕЙФУЕТ непредсказуемо (особенно после `cd .../nova-cli && cargo build` — застрял в nova-cli).

**ЧЕМ УКУСИЛО (эта сессия, ДВАЖДЫ):**
- `cd /…/nova-p172/nova-cli && cargo build` → cwd застрял в nova-cli.
- Затем `git checkout -- compiler-codegen/src/…` БЕЗ cd → выполнился из nova-cli с
  несовпадающими относительными путями → откатил/no-op'нул не те файлы (потерял правки в worktree).
- Позже `git add && commit` БЕЗ cd → cwd дрейфнул в main → «nothing to commit / On branch main»
  (правки были в worktree, коммит целил main).

**ЖЁСТКОЕ ПРАВИЛО:**
- КАЖДУЮ git/cargo/test команду для worktree-работы префиксовать **абсолютным**
  `cd /d/Sources/nv-lang/nova-p172 && …` (или нужный worktree). НЕ относительный `cd nova-cli`.
- В ТОЙ ЖЕ команде verify: `cd <wt> && echo "PWD=$(pwd) BRANCH=$(git branch --show-current)" && …`
  перед мутирующими операциями (checkout/apply/add/commit).
- Файловые правки (Edit/Write) с АБСОЛЮТНЫМИ путями надёжны (не зависят от cwd) — это безопасно.
  Проблема ТОЛЬКО у shell-команд с относительными путями / git, зависящих от cwd.
- `git apply`/`git checkout` с repo-root-relative путями ТРЕБУЮТ cwd = worktree root.

**Verification (всё ОК):** `git -C /d/Sources/nv-lang/nova-p172 log main..plan-172-… --oneline | wc -l`
> 0 → коммиты в worktree, не в main. Либо `git -C <path>` вместо cd вообще — самое надёжное.

**How to apply:** не полагаться на персистнутый cwd; явный абсолютный `cd <wt> && …` + verify pwd
в каждой git-команде; рассмотреть `git -C <wt> …` для cwd-независимости. См. [[feedback-isolated-worktree]],
[[feedback-ff-into-shared-main-repo]].
