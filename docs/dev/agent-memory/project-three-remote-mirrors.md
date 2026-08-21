---
name: project-three-remote-mirrors
description: Все 9 реп nv-lang зеркалируются на github + gitverse + sourcecraft; схема имён и авторизации
metadata: 
  node_type: memory
  type: project
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-07-30T16:22:30.313Z
---

Все репозитории nv-lang живут на ТРЁХ ремоутах (владелец завёл 2026-07-30):
`github.com/nv-lang/*`, `gitverse.ru/nv-lang/*`, `git.sourcecraft.dev/nv-lang/*`.

Девять реп: nova, nova-bigint, nova-compress, nova-http, nova-polaris, nova-tls,
tree-sitter-nova, www, www-nv-lang-ru.

**Имена совпадают везде, КРОМЕ сайта на русском:** локальная папка `www.nv-lang.ru` →
gitverse `www.nv-lang.ru`, но sourcecraft `www-nv-lang-ru` (точки в имя не пускает).
Не путать с `www` — это отдельная репа (сайт nv-lang.org).

**Авторизация:** gitverse — без токена в URL (credential helper). sourcecraft — токен
прямо в URL вида `https://<токен>@git.sourcecraft.dev/nv-lang/<repo>.git`. Токен НЕ
хранить в памяти — брать из уже настроенного ремоута:
`git -C d:/Sources/nv-lang/nova remote get-url sourcecraft | sed -e 's|https://||' -e 's|@.*||'`

Имена ремоутов в локальных репах: `origin`/`github` (github), `gitverse`, `sourcecraft`.
Пуш после зелёного гейта — во все три (см. [[feedback-push-after-green-gate]]).

**Зеркала расходятся молча.** Проверка — сравнением sha без клона:
`git -C <репа> ls-remote --heads <ремоут> <ветка>`. Прецедент 2026-07-30: копия
nova-polaris на sourcecraft отстала на 2 коммита, среди них A-V7 — тот самый фикс,
который снимал внешний блокер тега v0.1. Никто бы не заметил до релиза.
