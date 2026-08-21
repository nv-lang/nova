---
name: feedback-vec-of-not-from-in-tests
description: "в тестах писать Vec[T].of(a, b, c) (вариадик-конструктор), НЕ Vec[T].from([a, b, c])"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

В тестах (conformance spec_tests + любые новые .nv-тесты) использовать вариадик-конструктор
`Vec[int].of(1, 2, 3)` вместо `Vec[int].from([1, 2, 3])` (from-array-literal).

**Why:** канонический/предпочитаемый владельцем способ литерала вектора в тестах; `.of(...)` —
прямой вариадик, без промежуточного array-литерала.

**How to apply:** при авторинге/правке тестов — `Vec[T].of(elems...)`. Существующие
`Vec[T].from([...])` в тестах заменять на `.of(...)`. Связано с [[feedback_nova_syntax]]
(не выдумывать синтаксис) и [[feedback-test-conventions-strict]].
