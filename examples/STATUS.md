<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# examples/ — Ф.1/Ф.2 ревизии выполнены (Plan 197), Ф.3-Ф.5 ещё впереди

Аудит + чистка мёртвой поверхности (2026-07-12): удалены явно
нерабочие/не-user-facing файлы (compiler-тесты, намеренно-нерабочее
reading-only содержимое), retracted-синтаксис (`with Detach`/`effect Detach`
custom-хендлеры → канонический `SyncDetach`; `use std.X` → `import
std.data.X.{...}`; wildcard-импорт и т.п.) заменён на канон везде, где это
было дёшево. Пример без сохранённого канонического концепта, для которого
переписка не окупалась, — удалён; пример с ценным концептом, но требующий
переписи начисто, — перемещён в **[`_wip/`](_wip/)** (не участвует в
будущем CI-гейте компиляции).

Большинство `examples/**/*.nv` теперь реально компилируются текущим
тулчейном (`nova build`). Несколько файлов остаются заблокированы
**известными багами компилятора** (не авторским содержимым examples) —
подробности и repro в
[`docs/plans/197-audit-progress.md`](../docs/plans/197-audit-progress.md):
generic `.map()`/`Result.map` type-argument inference (ICE), `with EFFECT =
value { ... }` не парсится внутри тела handler-method, extern-FFI tuple
return type codegen. Эти баги — вне границ Plan 197, ждут отдельной волны
compiler-codegen.

См. **[Plan 197](../docs/plans/197-examples-revision.md)** — Ф.3
(канонический showcase-набор) и Ф.5 (CI-гейт компиляции) ещё не сделаны.
Флагман — [Plan 187](../docs/plans/187-flagship-concurrency-demo.md).
