---
name: project-nova-private-restored
description: "nova-private опустела без решения владельца (обнаружено 2026-07-21); клон восстановлен с github; правило: каждая запись в discussion-log = немедленный пуш"
metadata: 
  node_type: memory
  type: project
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-07-21T12:15:49.271Z
---

2026-07-21: d:/Sources/nv-lang/nova-private оказалась ПУСТОЙ (не git). Восстановлена
клоном с https://github.com/nv-lang/nova-private (227 файлов, discussion-log.md wieder
на месте). Последний пуш до пропажи — 2026-07-10 18:38; записи 11–21.07, если велись
локально, потеряны. Причина опустошения НЕ установлена (mtime папки 17 мая противоречит
июльским коммитам с локальным email unitcraft@nv-lang.org — возможно чистил внешний
инструмент/OneDrive/антивирус; агентские брифы папку не трогали). Владельцу предложено
проверить корзину Windows.

**Why:** приватный журнал — единственная не-git-репа история решений; потеря = дыра.

**How to apply:** (1) после КАЖДОЙ записи в discussion-log.md — немедленный
`git push origin` (не копить); (2) локальный email в клоне = unitcraft@nv-lang.org
(восстановлен); (3) при старте сессии, если nova-private отсутствует/пуста — переклонировать
с github и доложить владельцу, НЕ писать в несуществующий путь молча. См.
[[feedback-update-logs]].
