// SPDX-License-Identifier: MIT OR Apache-2.0
# Error & cleanup runtime model — panic / fail / defer / on_exit

> Сводный reference: как взаимодействуют `throw`/`Fail[E]`, `panic`, `exit`, `defer`
> и `Consumable.on_exit` в рантайме. Модель была разбросана по
> D13/D90/D158/D161/D188/D189/D194/D196 (4+ файла спеки) + код — этот документ сводит
> её в одну карту. Авторитет — спека (D-ссылки) и реализация
> (`compiler-codegen/nova_rt/effects.h`, `compiler-codegen/src/codegen/emit_c.rs`).
> Создан 2026-06-20.
>
> **⚠️ Описывает ТЕКУЩУЮ (частично противоречивую) модель. Идёт редизайн —
> [Plan 173](../plans/173-error-system-unify-harden.md)** (унификация в «defer-kernel»:
> `defer(o ScopeOutcome)`, consume=сахар, единый re-dispatch; устранение 11 дефектов вкл.
> with-Fail-глотает-panic; structured-concurrency error API). После Ф.2 хаб переписывается под единую модель.

## Три уровня катастрофы ([D13](../../spec/decisions/08-runtime.md#d13))

| Уровень | Конструкция | Что убивает | Перехват |
|---|---|---|---|
| Управляемая ошибка | `throw err` + `Fail[E]` | ничего — передаётся handler'у | **handler'ом в коде** (`with Fail[E] = …`, `?`) |
| Сбой fiber'а | `panic(msg)` | текущий fiber | **runtime'ом на границе fiber'а** (supervisor рестартует); НЕ ловится handler'ом |
| Смерть процесса | `exit(code, msg)` | весь процесс | не перехватывается; `defer`/`on_exit` НЕ запускаются |

Никаких `try_panic`/`catch` в языке ([rejected.md](../../spec/decisions/history/rejected.md)).
Программист не ловит panic — это работа runtime'а на границе fiber'а.

## Транспорт: fail-frame + setjmp/longjmp

`throw`, `panic`, assert и contract-violation используют ОДИН механизм — цепочку
`NovaFailFrame` (`effects.h:55`) на thread-local `_nova_fail_top`. Различает их `error_kind`:

- `NOVA_THROW_USER` — обычный throw (recoverable handler'ом).
- `NOVA_THROW_CANCEL` — отмена (структурная, ре-throw'ится сквозь Fail-handler).
- `NOVA_THROW_PANIC` — panic / assert / contract ([D188](../../spec/decisions/03-syntax.md#d188); НЕ recoverable).

`nv_panic` (`effects.h:542`) ставит `error_kind = NOVA_THROW_PANIC` и `longjmp`-ает в
**ближайший** `_nova_fail_top` ПЕРВЫМ (не `abort`).

## panic ЗАПУСКАЕТ cleanup (не пропускает)

Заблуждение: «panic/longjmp пропускает деструкторы». **Неверно:**

- В Nova нет RAII-деструкторов.
- `panic` идёт через fail-frame → каждый `defer`-кадр ловит longjmp, прогоняет свои
  defer'ы (LIFO) и ре-throw'ит наверх (`emit_c.rs:17615`). `defer` срабатывает на
  ЛЮБОМ exit, включая panic ([D90](../../spec/decisions/03-syntax.md#d90),
  `03-syntax.md:4509`).
- `consume X = … { }` ловит исход и зовёт `on_exit(Panic(msg))` (`emit_c.rs:19273`,
  [D188](../../spec/decisions/03-syntax.md#d188)); при двойной панике body-panic
  доминирует над on_exit-panic ([D196](../../spec/decisions/03-syntax.md#d196) R4b).
- `panic` в defer-body НЕ даёт Rust-style double-panic-abort — композируется в
  `MultiError` как suppressed ([D161](../../spec/decisions/03-syntax.md#d161),
  `emit_c.rs:17651`); все N cleanup'ов выполняются.
- `exit(code)` — единственный, кто НЕ разворачивает стек: `defer`/`on_exit` пропускаются.

## panic НЕ ловится `Fail`-handler'ом (но есть баг)

По [D13](../../spec/decisions/08-runtime.md#d13) `panic` должен пройти СКВОЗЬ
`with Fail[E]`-handler до границы fiber'а — handler ловит только `throw` (USER).

> ⚠️ **БАГ `[M-172-with-fail-swallows-panic]` (открыт, P1).** На практике `with Fail[E]`
> ГЛОТАЕТ панику: re-dispatch (`emit_c.rs:6650-6692`) ре-throw'ит только
> `NOVA_THROW_CANCEL`, а `NOVA_THROW_PANIC` проваливается в «USER path» → паника
> проглочена, выполнение продолжается (эмпирически подтверждено 2026-06-20, C-codegen).
> Нарушение D13. Фикс — симметричная PANIC-ветка перед USER-path. NB: `supervised{}`
> ловить panic ДОЛЖЕН (для рестарта) — это отдельная корректная граница.

## errdefer / okdefer / defer |result| — УДАЛЕНЫ ([D189](../../spec/decisions/03-syntax.md#d189))

Ретракнуты hard-cutover (D189, Plan 110.5.7); парсер реджектит (`parser/mod.rs:9835`).
Миграция:

| Было (ретракнуто) | Стало |
|---|---|
| `errdefer { rollback }` | `on_exit(o) match { Failure(_) / Panic(_) => rollback }`, либо флаг-паттерн `mut done=false; defer { if !done { rollback } }; …; done=true` |
| `okdefer { commit }` | `on_exit(o) match { Success => commit }` |
| `defer \|result\| { … }` | `on_exit(outcome ScopeOutcome)` |
| `defer { close }` (безусловный) | **остаётся** — `defer` жив |

Идиома cleanup'а ресурса — [consume-scope-cleanup.md](consume-scope-cleanup.md)
(Plan 110 / D188). Стиль написания — [nv-coding-style.md](../nv-coding-style.md) §20.4.

## Источники (авторитет)

- **[D13](../../spec/decisions/08-runtime.md#d13)** — panic = смерть fiber'а; три уровня; нет catch.
- **[D90](../../spec/decisions/03-syntax.md#d90)** — `defer` на любом exit (кроме `exit()`).
- **[D158](../../spec/decisions/03-syntax.md#d158)** — failable cleanup + `MultiError`.
- **[D161](../../spec/decisions/03-syntax.md#d161)** — panic-in-defer composition (нет double-abort).
- **[D188](../../spec/decisions/03-syntax.md#d188)** — `Consumable.on_exit(ScopeOutcome: Success/Failure/Panic)`.
- **[D189](../../spec/decisions/03-syntax.md#d189)** — `errdefer`/`okdefer`/`defer |result|` удалены.
- **[D194](../../spec/decisions/03-syntax.md#d194)** — `Consumable[Never]` infallible cleanup.
- **[D196](../../spec/decisions/03-syntax.md#d196)** R4b — body-panic доминирует, exactly-once on_exit.
- Код: `effects.h` (`nv_panic`, `NovaFailFrame`), `emit_c.rs` (defer/on_exit codegen, Fail-handler re-dispatch).
