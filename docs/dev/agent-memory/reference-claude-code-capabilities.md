---
name: reference-claude-code-capabilities
description: Шпаргалка для владельца про возможности Claude Code (Artifact/Workflow/скиллы/хуки) — где лежит и когда напоминать
metadata: 
  node_type: memory
  type: reference
  originSessionId: 589967a3-3a2d-4bbe-991e-abef519edea0
---

Владелец 2026-07-09 узнал про инструмент **Artifact** (публикация HTML как веб-страницы на claude.ai) и попросил сохранить обзор возможностей Claude Code «на будущее».

Файл-шпаргалка: **docs/claude-code-capabilities.md** (в репо nova). Покрывает: Artifact, Workflow (многоагентная оркестрация), скиллы (/code-review, /verify, /simplify, /deep-research), автоматизацию (хуки/schedule/loop), фоновые агенты, память.

**Когда напоминать:** если владелец делает что-то визуальное (граф/дашборд/мокап) → предложить Artifact; крупный аудит/миграция → Workflow (но осторожно, лимиты токенов — см. [[feedback-agent-token-economy]]); ревью диффа на баги → /code-review (в тему [[feedback-zero-tolerance-bugs]]).

Владелец раньше НЕ знал про Artifact — вероятно, не знает и про часть остального; уместно предлагать подходящий инструмент под задачу, а не ждать явной просьбы.
