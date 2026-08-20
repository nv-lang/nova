---
name: feedback-www-deploy-is-the-verdict
description: "Правка сайта www считается сделанной только по зелёному деплою GitHub Pages (gh run list --repo nv-lang/www), не по пушу; локальная сборка sync+astro до пуша"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-08-15T16:14:05.709Z
---

**Правило:** «сайт обновлён» = деплой `Deploy to GitHub Pages` на моём SHA — success.
Пуш в main www — не результат. Проверка одной командой сразу после пуша:
`gh run list --repo nv-lang/www --limit 1 --json conclusion,status,headSha`.

enforcement: немашинное — кандидат в машинное: pre-push-хук репо www, гоняющий
`node scripts/sync-decisions.mjs && npx astro build` (та же пара, что и деплой; ~1 мин),
плюс строка в 231.2 «www: сборка до пуша». До этого — ритуал ниже.

**Why:** 2026-08-15 — запушил фикс hero-примера, отчитался «готово»; владелец спросил
«проверил?» — деплой был красный (`RenderUndefinedEntryError`): сайт тянет спеку из
nova, а её файлы переименовали (`X.md`/`X.ru.md`), фикс лежал в чужой невлитой ветке.
Локальная сборка в `www-p-www-sync/site` ловит ровно этот класс до пуша.

**How to apply:** для www — сначала локальная сборка (sync + astro build), потом пуш,
потом дождаться зелёного run'а и лишь тогда отчитываться. Правки www делать в
worktree `www-p-www-sync` (main); главный чекаут `www` может стоять на чужой ветке —
всегда `git branch --show-current` перед коммитом.

Смежное: [[feedback-push-after-green-gate]], [[feedback-site-docs-guide-only]].
