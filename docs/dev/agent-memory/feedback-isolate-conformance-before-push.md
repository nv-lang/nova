---
name: feedback-isolate-conformance-before-push
description: behavior-changing слияние — прогнать conformance-семплы изолированно ДО пуша; standalone+флагман зелёные НЕ гарантируют мега-CU
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-07-20T00:41:48.448Z
---

Для behavior-changing слияний (enforce/#coerce/D-ретракции — 214, consume-А, closure) локальные гейты **standalone-CU + флагман-src** могут быть зелёными, а **conformance мега-CU (folder-CU, все ~990 файлов одним CU) — красным**: регрессия прячется в файле, который standalone/флагман не тянут.

**Why:** дважды за сессию 2026-07-19/20 запушил регрессию, невидимую локально:
- closure (d22/d402) — new-фикстура name-collision, всплыла ТОЛЬКО в мега-CU;
- 214 (#coerce) — neg-фикстура d55 не мигрирована под D429-ретракцию (NEG-NO-ERROR), + typo `assert x == 4` без скобок в моей же consume-А фикстуре дал голый `assert;` в C.
Оба раза standalone+флагман были зелёные → ложная уверенность → пуш → красный CI.

**How to apply:** перед пушем behavior-changing — прогнать затронутые conformance-семплы через **полный gate-фильтр** (awk-фильтр из nova-gate.yml, учитывающий known-red whitelist и NEG-NO-ERROR), а не только `^PASS:`. Мой grep ловил лишь CC-FAIL и пропускал `NEG-NO-ERROR`. Изолировать conformance-файл: folder-CU тянет весь каталог (E_D78 при наивном соло) — обходить dev-модулем/мини-структурой. known-red whitelist МАСКИРУЕТ deterministic CC-FAIL (app_effect был мисатрибутирован «Linux M:N-race», на деле — typo). Мега-CU локально дорог/CPU — но хотя бы узкий прогон затронутого поддерева обязателен. См. [[project-conformance-single-cu-run]].
