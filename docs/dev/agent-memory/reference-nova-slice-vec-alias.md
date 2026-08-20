---
name: reference-nova-slice-vec-alias
description: "[]T — синтаксический алиас для Vec[T] в Nova; @map и др. методы на []T работают как на Vec[T]"
metadata: 
  node_type: memory
  type: reference
  originSessionId: dd5c4857-a3c7-46cd-9695-cab9b466d77d
---

`[]T` — синтаксический алиас для `Vec[T]`. Это один и тот же тип, не отдельная концепция.

- `vec_seq.nv` определяет методы на `[]T` (например `fn[T] []T @map[U](f fn(T) -> U) -> []U`) — они применяются к `Vec[T]` напрямую.
- `v.map(f)` работает без `.iter()` именно через это: метод определён на `[]T` = `Vec[T]`, eager, возвращает `[]U` = `Vec[U]`.
- Ленивые адаптеры (`vec_iter.nv`) доступны через `v.iter().map(f)` — нулевые аллокации в цепочке.
