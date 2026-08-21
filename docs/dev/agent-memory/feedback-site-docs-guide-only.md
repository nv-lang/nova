---
name: feedback-site-docs-guide-only
description: Сайт www тянет доку nova ТОЛЬКО из docs/guide/; docs/dev/ не линковать/не цитировать; плоские docs/<имя>.md мертвы
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a394fdf8-5cc0-4a5b-9325-28450ad4693a
  modified: 2026-08-02T00:45:53.123Z
---

Реструктуризация docs/ в nova (2026-08-02, мерж p-docs-split): docs/guide/ =
единственный источник doc-контента для сайта (en+ru пары); docs/dev/ (включая
бывший docs/promts/ → docs/dev/promts/) на сайт не попадает НИКОГДА; docs/plans/
без изменений; карта — docs/README.md.

enforcement: немашинное — правило отбора контента; машинная часть частична:
npm run build (sync-decisions.mjs + check-links) падает на мёртвых/старых путях,
но «dev/ не утёк» проверяется глазами в приёмке каждой сдачи сайта.

**Why:** внутренние конвенции/промпты не должны утекать на публичный сайт;
sync-decisions.mjs и DOC_GUIDES (site/src/data/docs.ts) переведены на
docs/guide/-пути (ветка www p-docs-split-paths).

**How to apply:** любая ссылка на доку nova в контенте сайта — только
docs/guide/...; старые плоские пути docs/<имя>.md при встрече обновлять на
docs/guide/<имя>.md; в приёмку каждой сдачи по сайту входит проверка «синк не
тянет docs/dev/». Порядок деплоя жёсткий: сначала пуш nova main, потом мерж
путей в www main — падение npm run build на старых путях в этом окне не чинить,
ждать. См. [[feedback-parallel-session-compiler-queue]].
