---
name: feedback-no-claude-coauthor
description: "никогда не добавлять Co-Authored-By: Claude trailer в commit messages"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: ca2f3c9b-ac4e-4c85-bb2a-3603d383275b
---

Не добавлять `Co-Authored-By: Claude ... <noreply@anthropic.com>` (или любую вариацию с Claude) в commit messages — ни в `git commit -m`, ни в HEREDOC, ни в PR описаниях.

**Why:** Политика репо — AI co-authorship не афишируется в git истории. 28 коммитов с trailer'ом пролезли в main на github+gitverse, пришлось делать filter-repo rewrite + force-push на оба remote.

**How to apply:**
- Не использовать default-шаблон Claude Code commit message с трейлером.
- В nova/.githooks/commit-msg стоит sed-страйп (срабатывает на любом `git commit`).
- В global settings.json `PreToolUse` hook с matcher `Bash|PowerShell` тоже страйпает.
- Обе защиты дублируют друг друга — если одна не сработает, поймает вторая.
- При commit'ах из других репо (где нет .githooks) — следить руками.

Связано с [[feedback_git_add_specific]] (политика git дисциплины) и [[feedback-commit-per-task]].
