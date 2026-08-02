// SPDX-License-Identifier: MIT OR Apache-2.0
# Error & cleanup runtime model — panic / fail / defer / @cleanup

> Сводный reference: как взаимодействуют `throw`/`Fail[E]`, `panic`, `exit`, `defer`
> и `Cleanup.@cleanup` в рантайме. Модель была разбросана по
> D13/D90/D158/D161/D188/D189/D194/D196/D314 (4+ файла спеки) + код — этот документ сводит
> её в одну карту. Авторитет — спека (D-ссылки) и реализация
> (`compiler-codegen/nova_rt/effects.h`, `compiler-codegen/src/codegen/emit_c.rs`).
> Создан 2026-06-20. Обновлён под Plan 173 Ф.1/Ф.2 (2026-07-03).
>
> **Статус редизайна ([Plan 173](../plans/173-error-system-unify-harden.md), D314 defer-kernel):**
> - **Ф.1 (soundness+hygiene) — ЗАКРЫТА:** with-Fail-глотает-panic ИСПРАВЛЕН (D13, см. ниже);
>   errdefer/okdefer/`defer |r|` ретрактнуты (D189); renames Consumable→`Cleanup[E]`, `@on_exit`→`@cleanup`,
>   effect Cleanup→`ResourceTrace` (R1/R2).
> - **Ф.2 (defer-kernel) — ЗАКРЫТА (2026-07-04, верифицировано Ф.5.4):** `defer(o ScopeOutcome) { … }` —
>   РЕАЛИЗОВАН (B1/B2): тело видит исход scope; errdefer/okdefer субсумированы. `consume` — реализован как
>   **consume-flavored defer-entry** (D314 §3, B3-merge): sharing единый `ScopeOutcome`-примитив и четыре
>   defer run-site'а (FAIL/LEAVE/EARLY/INTERRUPT) с `defer(o)`, плюс consume-policy
>   (cancel-shield/watchdog/ResourceTrace/exactly-once/partial-init/compose). Терминальный транспорт —
>   **единый `nova_scope_exit(frame, policy)`** (D314 §4, effects.h): CATCH (with-Fail) / TRANSPARENT
>   (defer/consume); класс «кадр забыл kind» исчез по построению.
> - **Ф.5 (hygiene, 2026-07-10):** D188 R2 exactly-once — РЕАЛЬНЫЙ runtime-счётчик (скрытое поле
>   `_consume_ccount` + пролог-guard в `Nova_<T>_consume_cleanup` → `D188-on-exit-double-invocation`);
>   D192-ретракт ПРИМЕНЁН: тип `CleanupTimeoutError` УДАЛЁН, watchdog армится только вокруг cleanup-вызова,
>   превышение порога = one-shot stderr-варн + `duration_ms`/`overrun` в `ResourceTrace.on_resource_exit`
>   (D185 amend; `on_resource_enter(label)` — timeout дропнут).
> - **Structured-concurrency (§3a/§3b owner-пересмотры 2026-06-26):** scope-выход = **completes-by-default**
>   (cleanup'ы добегают, не обрубаются); **force-timeout НЕ существует** (D192-ретракт — только watchdog-варн
>   при превышении порога, cleanup продолжается); **restart по умолчанию НЕТ** (no-restart-default —
>   child-fail → cancel-siblings / Escalate / Stop; per-fiber Restart — только для изолированных файберов,
>   гейт [173.3](../plans/173.3-data-race-freedom-share.md)). Полная concurrency-карта — Ф.3-семейство
>   ([173.0](../plans/173.0-concurrency-runtime-substrate.md)–173.3); полный rewrite хаба под неё — Ф.2/Ф.5.
> - **Structured error propagation (Ф.3, [D414](../../spec/decisions/06-concurrency.md#d414-structured-error-propagation--primary-selection-precedence-detach-policy-channel-closed-vs-value-plan-173-ф3)):**
>   (§1) primary при нескольких падениях детей — строгий ранг **PANIC > USER/USER_TYPED > CANCEL**
>   (panic не деградирует до ловимого USER; остальные — в suppressed-карман Ф.4); (§2) `detach { }`
>   требует эффект `Detach` в сигнатуре (`[E_DETACH_REQUIRES_EFFECT]`), error-policy = LogAndDrop (default);
>   (§3) `recv() -> Option` канон, select `None = rx` различает closed от value.

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
- `consume X = … { }` ловит исход и зовёт `@cleanup(Panic(msg))` (consume-монолит
  `emit_c.rs` `Stmt::ConsumeScope`, [D188](../../spec/decisions/03-syntax.md#d188)); при двойной
  панике body-panic доминирует над cleanup-panic ([D196](../../spec/decisions/03-syntax.md#d196) R4b).
  Исход строится единым примитивом `assign_scope_outcome_from_frame` (общий с `defer(o)`, D314).
- `panic` в defer-body НЕ даёт Rust-style double-panic-abort — композируется в
  `MultiError` как suppressed ([D161](../../spec/decisions/03-syntax.md#d161), defer-kernel
  FAIL/LEAVE run-site'ы); все N cleanup'ов выполняются.
- `exit(code)` — единственный, кто НЕ разворачивает стек: `defer`/`@cleanup` пропускаются.

## panic НЕ ловится `Fail`-handler'ом (баг ИСПРАВЛЕН)

По [D13](../../spec/decisions/08-runtime.md#d13) `panic` должен пройти СКВОЗЬ
`with Fail[E]`-handler до границы fiber'а — handler ловит только `throw` (USER).

> ✅ **`[M-172-with-fail-swallows-panic]` — ИСПРАВЛЕН (Plan 173 Ф.1, D13 soundness).** Ранее
> `with Fail[E]` ГЛОТАЛ панику: re-dispatch ре-throw'ил только `NOVA_THROW_CANCEL`, а
> `NOVA_THROW_PANIC` проваливался в «USER path». Теперь SITE A (with-Fail terminal,
> `emit_c.rs` `emit_with`) имеет симметричную PANIC-ветку ПЕРЕД USER-path: pop frame +
> restore handlers/interrupt + `nova_rethrow_with_suppressed` (сохраняет kind=PANIC + msg +
> suppressed-chain). CANCEL — тоже re-throw (`nova_throw_cancel_reason`). Гейт:
> `nova_tests/err173_0/rt/f1_with_fail_swallow_panic`. NB: `supervised{}` ловить panic
> ДОЛЖЕН (для рестарта) — отдельная корректная граница.
>
> **Планируемая унификация transport** (`nova_scope_exit(primary, {CATCH,TRANSPARENT})`, D314 §4) —
> ОТЛОЖЕНА: terminal-сайты (with-Fail CATCH / defer+consume TRANSPARENT) несогласованы в
> kind-dispatch (SITE A: PANIC→rethrow, CANCEL→cancel_reason; defer C1: все→rethrow; consume:
> PANIC→nv_panic) → единый helper требует behavior-normalization design (followup).

## errdefer / okdefer / defer |result| — УДАЛЕНЫ ([D189](../../spec/decisions/03-syntax.md#d189))

Ретракнуты hard-cutover (D189, Plan 110.5.7); парсер реджектит (`parser/mod.rs:9835`).
Миграция:

| Было (ретракнуто) | Стало ([D314](../../spec/decisions/03-syntax.md#d314) `defer(o)` — РЕАЛИЗОВАН) |
|---|---|
| `errdefer { rollback }` | `defer(o ScopeOutcome) { match o { Failure(_) \| Panic(_) => rollback, Success => () } }` |
| `okdefer { commit }` | `defer(o ScopeOutcome) { match o { Success => commit, _ => () } }` |
| `defer \|result\| { … }` | `defer(o ScopeOutcome) { … }` (Zig-парность: `Failure(e)` payload типизирован) |
| `defer { close }` (безусловный) | **остаётся** — плейн `defer` жив |

Идиома cleanup'а ресурса — [consume-scope-cleanup.md](consume-scope-cleanup.md)
(Plan 110 / D188). Стиль написания — [nv-coding-style.md](../dev/nv-coding-style.md) §20.4.

## Источники (авторитет)

- **[D13](../../spec/decisions/08-runtime.md#d13)** — panic = смерть fiber'а; три уровня; нет catch.
- **[D90](../../spec/decisions/03-syntax.md#d90)** — `defer` на любом exit (кроме `exit()`).
- **[D158](../../spec/decisions/03-syntax.md#d158)** — failable cleanup + `MultiError`.
- **[D161](../../spec/decisions/03-syntax.md#d161)** — panic-in-defer composition (нет double-abort).
- **[D188](../../spec/decisions/03-syntax.md#d188)** — `Cleanup.@cleanup(ScopeOutcome: Success/Failure/Panic)`.
- **[D189](../../spec/decisions/03-syntax.md#d189)** — `errdefer`/`okdefer`/`defer |result|` удалены.
- **[D194](../../spec/decisions/03-syntax.md#d194)** — `Cleanup[Never]` infallible cleanup (caller-relax жив; §perf-элизия НЕ реализована → followup).
- **[D196](../../spec/decisions/03-syntax.md#d196)** R4b — body-panic доминирует, exactly-once `@cleanup`.
- **[D314](../../spec/decisions/03-syntax.md#d314)** — defer-kernel: `defer(o ScopeOutcome)` примитив; `consume`/`@cleanup` = сахар над outcome-defer; единый `nova_scope_exit` (transport-унификация отложена).
- Код: `effects.h` (`nv_panic`, `NovaFailFrame`), `emit_c.rs` (defer(o)/consume codegen, `assign_scope_outcome_from_frame`, Fail-handler re-dispatch SITE A).
