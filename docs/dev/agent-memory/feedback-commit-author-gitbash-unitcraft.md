---
name: feedback-commit-author-gitbash-unitcraft
description: "Канон авторства коммитов — unitcraft@nv-lang.org, локальный git config во ВСЕХ семи репах (владелец выставил 2026-08-06); агенту git config запрещён хуком"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-08-06T13:48:02.541Z
---

Авторство и подпись (`-s`) коммитов — всегда владелец: **Evgeniy Golovin
&lt;unitcraft@nv-lang.org&gt;**.

enforcement: машинное — хук `scripts/claude-hooks/guard-git.py` блокирует агенту
запись `git config user.*`; плюс локальный `user.email` выставлен владельцем во
всех семи репах, поэтому канон исполняется сам, пока конфиг не тронут. Проверка
глазами перед push: `git log -1 --format='%an <%ae>'`.

**2026-08-06 приведено к единому виду во всех семи репах** (владелец выполнил
сам, по моему списку команд): `nova`, `nova-tls`, `nova-polaris`, `nova-http`,
`nova-bignum`, `nova-compress`, `nova-socks`. **Отменяет прежний канон**
«глобальный git bash = `unitcraft@inbox.ru`»: до этой даты канон стоял локально
только в `nova` и `nova-tls`, остальные писались глобальным `inbox.ru` — это
признано разнобоем, а не нормой.

**Мне `git config user.*` запрещён** (урок 2026-07-25: 349 коммитов под чужим
именем через общий `.git` worktree). Обходить не пытаться — защита поставлена
именно от агентов; нужна правка конфига, готовить команды владельцу.

**PowerShell, не bash** (владелец в PS): `foreach ($r in "a","b") { … }`, без
`done`. Мой bash-цикл `for r in …; do … done` там падает
`MissingOpenParenthesisAfterKeyword` — проверено 2026-08-06.

**Переписывание авторства в истории** — без конфига, через окружение:
`GIT_AUTHOR_EMAIL`/`GIT_COMMITTER_EMAIL` + `git rebase --root --exec 'git commit
--amend --no-edit --reset-author -q'`. Для новой (не запушенной) репы безопасно
— так исправлен `nova-socks`. Для запушенных меняет хеши → нужен принудительный
пуш на все три зеркала каждой репы; владелец разрешил это 2026-08-06.

Связано: [[feedback-agents-must-not-touch-git-config]],
[[project-three-remote-mirrors]], [[feedback-no-claude-coauthor]].
