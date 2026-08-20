---
name: reference-mn-race-case-study
description: "Case study файл — STALE-slot race в M:N runtime (2026-05): симптом, ложные пути, state-dump алгоритм, Fix A+B+C, lessons learned"
metadata: 
  node_type: memory
  type: reference
  originSessionId: cb55672e-a54e-40fb-8c49-dfaca83fd088
---

Standalone case study о поиске и исправлении concurrency race в Nova M:N runtime.

**Файл:** `docs/articles/mn-race-stale-slot.md` в nova-private репо (материалы для статьи)  
**Краткий case study:** `docs/cases/mn-race-stale-slot-2026-05.md` в main репо

**Содержит:**
- Root cause (STALE slot: `fibers[i]=NULL` + `parked[i]=true`)
- 5 ложных путей с объяснением почему не сработали
- Диагностический алгоритм (state-dump → counters → локализация)
- Финальный код Fix A (alloc_slot) + Fix B+C (close_cb, sentinel -2)
- 6 lessons learned (heisenbug pattern, state-dump vs debugger, expected_co identity, etc.)

**Использовать как шаблон** при будущих M:N race investigations.

**Связано:** [[project-plan83_11-status]] (Plan 83.11 §10 — более детальный post-mortem)
