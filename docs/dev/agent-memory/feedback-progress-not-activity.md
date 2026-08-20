---
name: feedback-progress-not-activity
description: "Plan 172 legacy-deletion: владелец недоволен «постоянно что-то делаем, но не приближаемся к удалению легаси». Не спиннить на диагностике/de-risk/инертных заходах — исполнять recipe-ready фазы плана, измеримо двигать gate."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

**Что (2026-06-30, дважды переспросил, потом прямо):** владелец видит МНОГО активности
(workflows, de-risk-карты, эмпирические пробы, инертные заходы с откатами) и СПРАВЕДЛИВО
замечает: к удалению легаси (`infer_expr_c_type_legacy` ~3119 строк, headline 172.1) это НЕ
приближает.

**Why:** диагностика/локализация ценна ОДИН раз, но повторные de-risk-карты + инертные
producer-side заходы (7 откатов класса call-site/channel-relax/drain-time) = activity-without-
structural-progress. Цель — УДАЛИТЬ легаси, а не идеально описать почему трудно.

**How to apply:**
- Источник декомпозиции есть: `docs/plans/172.1-p67-execution-plan.md` (ФАЗА 0-6, file:line,
  риски, порядок). НЕ перекартировать — ИСПОЛНЯТЬ.
- Объективный gate: `infer_expr_c_type` = 5 точек выхода; 2 на канале ✅; **3 держат legacy-тело**
  (:36417 SelfAccess, :36423 Ident, :36498 fall-through). Удаление = заменить эти 3 + fall-through
  tally (NOVA_U45_GAP) → 0 на полном корпусе.
- ПРИОРИТЕТ tractable recipe-ready: **ФАЗА 4 (reliable-locals 4B SelfAccess + 4C Ident)** — bounded,
  side-effect-free, готовый код в плане → убирает 2 из 3 точек выхода. ФАЗА 2 (checker-annotation
  fall-through kinds) = основной объём, incremental по kind, КАЖДЫЙ kind = §7-коммит, измеримое
  падение tally. ФАЗА 3 reloc → ФАЗА 6 delete.
- Каждый ход — ИЗМЕРИМОЕ движение (точка выхода заменена / tally упал на N), не «понял почему».
- НЕ повторять инертные producer-side попытки value-record (2a) — фиксить на instantiation-consumer
  ([[feedback-plan172-whole-not-half]] — но «целиком» = реальные коммиты-к-удалению, не диагностика).
