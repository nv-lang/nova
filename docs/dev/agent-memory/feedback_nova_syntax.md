---
name: feedback-nova-syntax
description: Никогда не выдумывать синтаксис Nova — всегда смотреть в спеке перед написанием кода
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b0d63fe6-28de-4e3a-960e-cfb879e77d2e
---

Перед написанием любого Nova-кода (примеры, тесты, документация, сайт) — сначала найти аналогичный паттерн в `spec/decisions/` или `examples/`, и только потом писать.

**Why:** В прошлом было выдумано: `uses X` (несуществующий keyword), `-> !` (нет такого return type), `|>` (нет pipe оператора), `Vec[T]` (правильно `[]T`), `Type::CONST` (нет `::`, правильно `Type.CONST`), `format("...", x)` (нет такой функции, правильно `"${x}"`), `implies` (нет, правильно `==>`), `realtime nogc` на сигнатуре (это блок внутри тела), `spawn fn() { ... }` (нет fn-literal формы у spawn; правильно `spawn { block }` или `spawn expr_call()`), `tok.cancel_ch()` (CancelToken не имеет cancel-канала; API: `cancel()`/`cancel(reason)`/`is_cancelled()`/`reason() -> Option[T]`; отмена кооперативная через polling `is_cancelled()` или cancel-throw на yield-point'е).

**Additional:** В doc-примерах для сайта — тоже смотреть в реальные тесты (`nova_tests/`). Пример: `([]int []).first()` — несуществующий синтаксис; правильно `[]int.new().first()` или через `let empty []int = []`.

**How to apply:**
- grep по `D:\Sources\nv-lang\nova\spec\decisions\` и `D:\Sources\nv-lang\nova\examples\` за реальным примером похожего паттерна
- Ключевые файлы: `03-syntax.md` (синтаксис), `04-effects.md` (эффекты), `02-types.md` (типы)
- Если паттерн не найден в спеке — сказать об этом явно, не изобретать
