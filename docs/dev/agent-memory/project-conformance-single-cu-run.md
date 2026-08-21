---
name: project-conformance-single-cu-run
description: "spec_tests/conformance = ОДИН CU; точная команда запуска; fall-through tally по kind"
metadata: 
  node_type: memory
  type: project
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

`spec_tests/conformance/` — единый пир-модуль = **ОДИН compile unit** (~169 `.nv` файлов + neg/). Все D-тесты компилируются разом; CU собирается → запускаются ВСЕ `test`-блоки сразу. Репортится как ОДИН тест-entry.

**Точная команда запуска** (из `d:\Sources\nv-lang\nova-p172\nova-cli\`):
```powershell
$env:NOVA_GC_LIB_DIR="D:\Sources\nv-lang\nova\compiler-codegen\vcpkg_installed\x64-windows-static\lib"
$env:NOVA_INCLUDE_DIR="D:\Sources\nv-lang\nova\compiler-codegen\vcpkg_installed\x64-windows-static\include"
.\target\debug\nova.exe test --positive --compile-error ..\spec_tests\conformance
```
**НЕ гонять per-file** — маскирует cross-file folder-module баги (пример: d85 stale-var-leak виден только в whole-CU).

**ВАЖНО (урок 2026-07-07): статус/tally тут НЕ хранить** — 172.1 завершён 2026-07-04 (tally 6224→0, легаси-движок удалён), а моя точка-в-времени «tally=4506» пережила факт и дала неверное утверждение владельцу. Статусы — ТОЛЬКО docs/plans/README.md. Замечание: спаны диагностик folder-CU с 2026-07-07 честные (рендер через SourceMap).

Связано: [[feedback-test-conventions-strict]], [[project-nova-test-vs-test-build]], [[project-plan172-strategy]].
