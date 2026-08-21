---
name: project-ci-monitoring-protocol
description: CI github = авторитетный гейт (решение владельца 2026-07-16); мониторить ПОСТОЯННО — сторож в каждой сессии + проверка перед батчем слияний
metadata: 
  node_type: memory
  type: project
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Решение владельца 2026-07-16: авторитетный гейт (conformance + флагман-examples)
ПЕРЕЕХАЛ на GitHub CI (workflow nova-gate.yml, ветка ci-gate-workflow / План 197 Ф.5).
Локально — только таргетные проверки. Полный гейт локально НЕ гонять (3ч+ под
нагрузкой против 30-60 мин на CI).

**Протокол мониторинга — сторож ТОЛЬКО ПО ЯВНОЙ КОМАНДЕ ВЛАДЕЛЬЦА (2026-07-16:
«хочу чтобы сторож поднимался только при моём желании в той сессии, в которой
я хочу»). НИКАКОГО автоподъёма при старте сессии — ни Monitor, ни крона.**
1. Сторож/крон поднимаются ТОЛЬКО когда владелец скажет («подними CI-сторожа»,
   «поставь крон») — в той сессии, где сказал.
2. Разовая проверка `gh run list --repo nv-lang/nova --branch main --limit 5`
   перед батчем слияний — остаётся обязанностью интегратора (это не сторож,
   одна команда без фона). Красное вне известного списка = стоп-волна.
3. Защита от дублей при подъёме по команде: проверить пульс
   `/tmp/rp/ci_watchdog.beat` — метка свежее 5 минут = сторож уже жив в другой
   сессии, сказать об этом владельцу вместо второго сторожа.
4. Рецепт подъёма (по команде владельца):
   а) `mkdir -p /tmp/rp && gh run list --repo nv-lang/nova --branch main --limit 15 --json databaseId,status --jq '.[] | select(.status=="completed") | .databaseId' > /tmp/rp/ci_seen.txt`
   б) Monitor (persistent), цикл пишет пульс каждые 90с:
   `while true; do date +%s > /tmp/rp/ci_watchdog.beat; gh run list --repo nv-lang/nova --branch main --limit 12 --json databaseId,conclusion,workflowName,status --jq '.[] | select(.status=="completed") | "\(.databaseId)\t\(.conclusion)\t\(.workflowName)"' 2>/dev/null | while IFS=$'\t' read -r id concl wf; do if ! grep -q "^$id$" /tmp/rp/ci_seen.txt 2>/dev/null; then echo "CI main: $wf -> $concl (run $id)"; echo "$id" >> /tmp/rp/ci_seen.txt; fi; done; sleep 90; done`
   в) Сессионный крон каждые 4 часа (страховка: сверка с известно-красным +
   новое красное чинить сразу + перезапуск сторожа по устаревшему пульсу) —
   тоже только в интегратор-сессии.
3. Перед КАЖДЫМ батчем слияний — проверка статуса CI.
4. Красный CI = стоп-волна: триаж немедленно, фикс той же волной (нулевая толерантность).
5. Различать «известно-красное» (nova-lint hard gate красен на накопленных
   варнингах до чистка-волны) от НОВОГО красного.

**Тир-гейты** (конвенция вписывается в test-conventions.md В МОМЕНТ включения):
docs-only → без гейта; .nv-only → таргетный локально; Rust-компилятор → CI-авторитет.

Связано: [[feedback-push-after-green-gate]] (пуш-правило теперь = таргет-чек → пуш → CI вдогонку).
