---
name: feedback-module-tests-beside-module
description: Тесты std-модулей — РЯДОМ с модулем (*_test.nv); spec_tests = только conformance; в nova_tests НЕ писать
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Владелец (2026-07-06): «nova_tests наполовину поломан, хотим удалить; почему не пишешь тесты модулей рядом с модулями в файлах *_test.nv?»

**Правило размещения тестов:**
1. Тесты std-модуля → рядом с модулем: `std/<модуль>/<имя>_test.nv` (Go-модель; прецедент `std/runtime/sync_test.nv`; раннер понимает суффикс `_test` — из обычной сборки вырезается).
2. Языковые/D-блок тесты → ТОЛЬКО `spec_tests/conformance` (один CU, позитив + neg/).
3. В `spec_tests/` нет ничего, кроме conformance.
4. В `nova_tests/` НОВЫЕ тесты НЕ писать — корпус полусломан, судьба = санация (план 182) и удаление.

**Why:** новые тесты в nova_tests смешиваются со сломанным старьём — потом не отличить рабочее при удалении корпуса.

**How to apply:** в каждом задании агенту на std-код указывать место тестов `std/<модуль>/*_test.nv`; проверять поставки грепом «нет новых файлов в nova_tests». Детали — docs/test-conventions.md (источник истины). См. [[feedback-test-conventions-strict]], [[feedback-nova-tests-not-correctness-gate]].
