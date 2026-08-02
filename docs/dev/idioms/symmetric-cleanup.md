# Symmetric Cleanup: `errdefer` + `okdefer` — RETRACTED

> ⚠️ **Этот паттерн УДАЛЁН из языка ([D189](../../../spec/decisions/03-syntax.md#d189),
> Plan 110.5.7 hard cutover, 2026-06-20).** `errdefer`, `okdefer` и `defer |result|`
> больше не существуют — парсер реджектит их (`[D189-removed-errdefer]` и т.п.).
> Документ оставлен как tombstone-редирект; полная история — в git.

## Чем заменить

| Было (ретракнуто) | Стало (D188, Plan 110) |
|---|---|
| `errdefer { rollback }` | `@cleanup(o) match { Failure(_) / Panic(_) => rollback }` |
| `okdefer { commit }` | `@cleanup(o) match { Success => commit }` |
| `errdefer` + `okdefer` пара | `consume X = … { body }` + `Cleanup.@cleanup(outcome)` (exactly-once) |
| `defer \|result\| { … }` | `@cleanup(outcome ScopeOutcome)` |
| `defer { close }` (безусловный) | **остаётся** — `defer` жив (D90) |

Error-only откат без Cleanup-ресурса — флаг-паттерн:
`mut done = false; defer { if !done { rollback } }; …; done = true`.

## Куда идти

- **Идиома cleanup'а** → [consume-scope-cleanup.md](consume-scope-cleanup.md) (Plan 110 / D188).
- **Модель panic/fail/defer/@cleanup** → [error-and-cleanup-model.md](error-and-cleanup-model.md).
- **Стиль написания** → [nv-coding-style.md](../nv-coding-style.md) §20.4.
- Спека: [D189](../../../spec/decisions/03-syntax.md#d189) (retraction) ·
  [D188](../../../spec/decisions/03-syntax.md#d188) (Cleanup.@cleanup) ·
  [D90](../../../spec/decisions/03-syntax.md#d90) (defer).
