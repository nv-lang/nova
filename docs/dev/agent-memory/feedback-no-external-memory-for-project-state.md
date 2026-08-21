---
name: feedback-no-external-memory-for-project-state
description: "Не дублировать состояние проекта (статусы планов, приоритеты, что закрыто) во внешней памяти ~/.claude/...; source of truth — файлы в репозитории"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 24acf57e-c1ac-413d-ad9b-47532fa3017a
---

Никогда не читать из `~/.claude/projects/.../memory/project-plan*-status.md`,
`project-priorities-*.md`, `project-remaining-plans-triage.md` и подобных при
вопросах вида «что закрыто / что осталось / какой статус Plan N». Эти memory
обычно устаревшие (5-10 дней) и дублируют проектные источники.

**Why:** пользователь явно сказал «все сохранять в проекте» (2026-06-01).
Внешняя auto-memory дрейфует от реальности: например в этой сессии
`project-priorities-2026-05-26.md` говорил «103.4 READY» когда оно уже закрыто
неделю как; `project-remaining-plans-triage.md` 6-дневной давности писал
«Plan 51 не начат» когда оно в main с 2026-05-16. Опираться на них вредно.

**How to apply:** для статуса планов / приоритетов / «что делать дальше»:
1. `docs/plans/README.md` — authoritative таблица статусов (включая 📋 proposed / 🟡 partial / ✅ ЗАКРЫТ).
2. `docs/simplifications.md` — журнал решений и закрытий.
3. `nova-private/discussion-log.md` + `project-creation.txt` — приватные обсуждения и chronological log.
4. `git log --oneline main -30` + `git branch --no-merged main` — фактическое состояние.

Никаких `Read` или `Grep` на `~/.claude/projects/.../memory/project-*-status.md`,
`project-priorities-*` или `project-remaining-plans-triage.md`. Если auto-loaded
MEMORY.md упоминает статус плана — verify через проектные источники до использования.

User feedback / process-related memories (`feedback-*.md`, `user-identity.md`,
`reference-*.md`) — нормально читать, они не дублируют код/проект.
