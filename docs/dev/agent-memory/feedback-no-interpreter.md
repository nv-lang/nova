---
name: feedback-no-interpreter
description: "интерпретатор (`nova run`) не делаем/выключаем — тестировать только через C-codegen (test-build/test)"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 05bf8a8d-0eab-408c-b172-639e6e948c68
---

`nova run` = интерпретатор — проект его **не развивает и собирается выключить**.
Не использовать `nova run` ни для проверки кода, ни для тестов; он отстаёт от
C-codegen пути (напр. не знает `str.from` и др. зарегистрированных overloadّ'ов,
которые работают в C-codegen).

**Why:** source of truth — C-codegen pipeline. Интерпретатор — мёртвая ветка,
расхождения дают ложные выводы (я гонял `str.from(f64)` через `nova run` → «undefined»,
хотя в C-codegen он корректен).

**How to apply:** всегда `nova test-build FILE` / `nova test DIR` (C-codegen). Если
всплывёт задача — выключить/удалить интерпретатор отдельным планом. См.
[[project-nova-test-vs-test-build]].
