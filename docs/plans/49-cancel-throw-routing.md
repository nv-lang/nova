// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 49: Cancellation semantics — kinded throws + cancel reason

> **Создан 2026-05-14. Расширен 2026-05-15** (kinded throws → полная
> модель отмены: reason/cause, USER-precedence, defer-on-cancel — паритет
> с Go `context` / Rust cancellation / TS `AbortSignal`, местами лучше).
>
> **СТАТУС:** план, не начат.
>
> **Приоритет:** P1 — закрывает `[M-cancel-throw-routing]` и
> `[M-within-error-conflation]`; делает отмену first-class: «отмена ≠
> ошибка», с причиной, без потери реальных ошибок.
>
> **Предшественники:** [Plan 47](47-supervised-cancel.md) ✅. **Независим
> от Plan 48** — Plan 49 чинит runtime-семантику; чистый stdlib
> `within`/`race` требует обоих (48 — компиляция, 49 — семантика).

---

## Зачем и сравнение с индустрией

Сейчас брошенная ошибка в Nova — просто `nova_str` без «вида»
(`NovaFailFrame.error_msg`). Отмена scope'а (`"scope cancelled"`)
неотличима от реальной ошибки кроме как строковым сравнением. Следствия:
отмена «убегает» из `supervised(cancel:)` наружу как ошибка; `within`
вынужден ловить её `with Fail`-обёрткой, которая неотличимо глотает и
реальные ошибки (`[M-within-error-conflation]`).

Как сделано в индустрии (с чем просили сравнивать):

| | Механизм отмены | Отмена — это ошибка? | Причина отмены | Реальная ошибка vs отмена |
|---|---|---|---|---|
| **Go** | `context.Context` + `ctx.Done()` канал | **Нет** — значение (`ctx.Err()`) | `context.Cause()` (1.20+) — произвольная | errgroup: first-error-wins |
| **Rust** | future-drop / `CancellationToken` | **Нет** — структурно отдельно от `Result` | токен может нести reason | независимы по конструкции |
| **TypeScript** | `AbortController`/`AbortSignal` | **Полу** — `AbortError` в reject-канале | `signal.reason` (произвольная) | различимы по типу `AbortError` |
| **Nova сейчас** | cancel-throw (`nova_str`) | **Да** — неотличимо от Fail | нет | **не различимы** (строковый хак) |

Nova сейчас хуже всех: отмена = ошибка, без причины, неотличима.
Plan 49 выводит на паритет и местами лучше:
- **vs Go:** отмена не ошибка (паритет); причина (`Context.Cause`
  паритет); **лучше** — USER-ошибка имеет приоритет над отменой (Go
  errgroup теряет реальную ошибку при first-wins, мы — нет).
- **vs Rust:** отмена структурно отдельна от ошибок (паритет); reason
  на токене (паритет).
- **vs TS:** отмена **не** всплывает в caller'а как ошибка (**лучше** TS,
  где `AbortError` всё же в reject-канале — у нас отмена не доходит до
  caller'а `supervised` вообще); причина (паритет `signal.reason`).

---

## Корень проблемы

`nova_throw(msg)` кладёт только `nova_str` — нет поля, по которому
catch-site понял бы природу throw'а. `nova_supervised_run_impl` делает
`if (err) nova_throw(err)` — re-throw'ит ВСЁ, включая отмену.
`with Fail` `setjmp`-frame ловит ЛЮБОЙ throw, не отличая отмену от Fail.

---

## Архитектурное решение

### 1. Kinded throws — у throw'а есть «вид»

```c
typedef enum {
    NOVA_THROW_USER   = 0,   /* user `throw`, `?`, `!!`, assert — обычная ошибка */
    NOVA_THROW_CANCEL = 1,   /* кооперативная отмена scope'а */
} NovaThrowKind;
```

`NovaFailFrame` получает `error_kind`. `nova_throw(msg)` — `USER`
по умолчанию (вся существующая семантика не меняется). Новый
`nova_throw_cancel(msg)` — `CANCEL`. Kind переживает `longjmp`.

### 2. Cancel reason — причина отмены (Go `Context.Cause` паритет)

`CancelToken` получает поле `reason` (`nova_str`, nullable). API:
- `tok.cancel()` — отмена без причины (reason = дефолт `"cancelled"`).
- `tok.cancel(reason str)` — отмена с причиной (`"deadline exceeded"`,
  `"user aborted"`, ...). Перегрузка / опциональный аргумент.
- `tok.is_cancelled() -> bool` — был ли вызван `cancel()` (как
  `ctx.Err() != nil`).
- `tok.reason() -> str?` — причина, если отменён (как `ctx.Cause()` /
  `signal.reason`). `None` если не отменён.

`cancel-throw` несёт reason как `error_msg` (не фиксированную строку).
`within` отменяет watchdog'ом с reason `"deadline exceeded"` — это
естественно даёт Go-различие `Canceled` vs `DeadlineExceeded` без
отдельного enum'а.

### 3. `supervised(cancel:)` — отмена не убегает наружу

`nova_supervised_run_impl` становится kind-aware. После цикла, при
`first_error != NULL`:
- `first_error_kind == CANCEL` → scope отменён штатно. **Возврат БЕЗ
  re-throw.** Отмена сделала работу, наружу ничего не летит. (Как в Go:
  `ctx` отменён → функция просто возвращается.)
- `first_error_kind == USER` → реальная ошибка fiber'а. Re-throw через
  `Nova_Fail_fail(err)` (не plain `nova_throw`) — внешний `with Fail`
  handler пользователя вызывается.

### 4. USER-precedence — реальная ошибка не теряется (лучше Go)

`nova_fiber_report_error` при записи в `first_error` соблюдает приоритет:

| current ↓ \ incoming → | (пусто) | CANCEL | USER |
|---|---|---|---|
| **(пусто)** | — | запись CANCEL | запись USER |
| **CANCEL** | — | keep | **overwrite USER** |
| **USER** | — | keep | keep (first USER wins) |

То есть: реальная ошибка **всегда** перетирает отмену, даже если отмена
записана раньше. Go errgroup делает чистый first-wins и **теряет**
реальную ошибку, случившуюся после отмены — у нас она surface'ится.
(Edge: ошибка в cancellation-teardown тоже surface'ится — это честнее,
см. Риск R6.)

### 5. `with Fail` не «обрабатывает» отмену

Codegen `emit_with`: `setjmp`-frame поймал throw → проверить
`frame.error_kind`. `CANCEL` → handler НЕ запускается, throw
пробрасывается дальше (`nova_throw_cancel`) к ближайшему `supervised`.
`with Fail` ловит только `USER`. Отмена структурна, не ошибка.

### 6. defer/errdefer выполняются при отмене (Rust Drop / Go defer паритет)

Когда fiber отменён (cancel-throw разворачивает его стек), его `defer`-
блоки **обязаны** выполниться — это cleanup-семантика, паритет с Rust
(Drop при отмене future) и Go (`defer` при выходе goroutine).
`errdefer` — тоже срабатывает (отмена это «выход с ошибкой» с точки
зрения fiber'а). Проверяется явно в Ф.4.

### Почему throw, а не Go-style чисто-значение

Go-горутина проверяет `ctx.Done()` в `select` и **возвращается сама**.
Nova-fiber может быть запаркован глубоко внутри `Time.sleep` /
`Channel.recv` — его нельзя «вернуть», его надо **развернуть** со стека.
Throw (longjmp) — и есть механизм разворота. Ключевое: throw **kinded**
(различим) и **не убегает** из `supervised` (наружу — как в Go,
невидим). Внутри — kinded unwind; снаружи — отмена не ошибка.

Кооперативная проверка `tok.is_cancelled()` остаётся **предпочтительным**
путём для CPU-bound fiber'ов (как Go `select { case <-ctx.Done() }`) —
`if tok.is_cancelled() { return }` выходит чисто, без throw'а. Throw —
fallback для fiber'ов, запаркованных в блокирующих операциях, которые
не могут опрашивать флаг.

---

## Фазы

### Ф.0 — `NovaThrowKind` + `NovaFailFrame.error_kind` + `nova_throw_cancel`

- `nova_rt/effects.h`: enum `NovaThrowKind`; поле `error_kind` в
  `NovaFailFrame`; `nova_throw` ставит `USER`; `nova_throw_cancel` —
  `CANCEL`. `NOVA_TRY`/`NOVA_CATCH` экспонируют `error_kind`.
- Чистый рефактор + новое API — поведение не меняется (никто ещё не
  бросает `CANCEL`).

### Ф.1 — Cancel reason на `CancelToken`

- `nova_rt/fibers.h`: поле `nova_str reason` в `NovaCancelToken`
  (caller-owned, переживает scope).
- `nova_cancel_token_cancel(tok)` → `nova_cancel_token_cancel_reason(tok, reason)`;
  старый — wrapper с дефолтом `"cancelled"`.
- `nova_cancel_token_reason(tok) -> nova_str` (пустая если не отменён).
- Codegen: `tok.cancel()` / `tok.cancel(reason)` (опц. аргумент),
  `tok.reason()` → `Option[str]`. Dispatch на `NovaCancelToken*`.
- Spec D75 §«Модель токена» — добавить `reason()` в capabilities.

### Ф.2 — Кооперативная отмена бросает `CANCEL` + reason

- `nova_fiber_yield` (`cancel_requested` ветка): `nova_throw_cancel`
  с reason из токена scope'а (а не фикс. `"scope cancelled"`).
  → нужен путь scope → bound token → reason. Либо scope несёт
  `cancel_reason` (копируется из токена в `nova_cancel_token_bind` /
  при `cancel()`).
- **Аудит всех точек cancel-throw'а:** `fibers.h` (yield),
  `channels.h` (cancel-during-recv/send), `nova_sleep` (cancel-during-park),
  `select`. Каждая — на `nova_throw_cancel`. Пропуск = регрессия (Риск R1).
- `NovaFiberQueue`: `first_error_kind` + `first_error_reason`.
  `nova_scope_init` инициализирует.
- `nova_fiber_report_error` → kinded + USER-precedence таблица (раздел 4).

### Ф.3 — `nova_supervised_run_impl` + `emit_with` kind-aware

- `nova_supervised_run_impl`: `CANCEL` → возврат без re-throw; `USER` →
  re-throw через `Nova_Fail_fail`.
- `emit_with` (emit_c.rs): frame поймал `CANCEL` → re-throw
  `nova_throw_cancel` (с reason), handler не запускать; `USER` →
  существующее поведение. Аккуратный pop/re-throw порядок (Риск R3).

### Ф.4 — Тесты семантики (без Plan 48)

Прямые `supervised(cancel: tok)` тесты, НЕ требующие closures-in-generics:
- **отмена не убегает**: `supervised(cancel: tok)` без `with Fail` —
  после scope'а код продолжается, throw наружу нет.
- **реальная ошибка убегает**: fiber `throw "boom"` → `supervised`
  re-throw'ит, внешний `with Fail` ловит именно `"boom"`.
- **USER-precedence**: один fiber `tok.cancel()`, другой позже
  `throw "boom"` → наружу летит `"boom"`, не отмена.
- **reason**: `tok.cancel("deadline")` → после scope'а
  `tok.reason() == Some("deadline")`; `tok.is_cancelled() == true`.
- **`with Fail` не глотает отмену**: handler не вызывается на cancel-throw.
- **defer при отмене**: fiber с `defer { cleanup() }` отменён →
  `cleanup()` выполнился. `errdefer` — тоже.
- **кооперативный `is_cancelled()`**: CPU-bound fiber проверяет флаг,
  выходит `return` без throw'а — scope чист.
- Чистка: убрать `with Fail`-обёртки из Plan-47-тестов, которые были
  нужны ТОЛЬКО чтобы ловить убегавшую отмену.

### Ф.5 — M:N: kind + reason в cross-worker propagation

- Plan 44.5 Layer 5: cross-worker ошибка через `first_error_atomic` (CAS).
  Добавить `first_error_kind` + `first_error_reason` рядом; reader читает
  их только увидев non-NULL msg (release/acquire ordering, как
  существующий atomic-путь).
- USER-precedence под M:N: CAS-петля должна уметь overwrite CANCEL→USER
  (не строгий compare-NULL-and-swap, а compare-kind).
- Тест: `runtime.init(N)` + `supervised(cancel:)` + cross-worker cancel
  + cross-worker real error — поведение совпадает с single-thread.

### Ф.6 — Regression + docs + spec

- Полный `nova test` (release) — без новых FAIL.
- spec `06-concurrency.md`: D-decision (~D104) — «cancellation semantics:
  kinded throws, отмена ≠ ошибка, reason, USER-precedence». Обновить D75
  §«Семантика отмены», §«Модель токена» (reason), §«История» (ограничение
  снято).
- `project-creation.txt` + `simplifications.md`: закрытие; снять
  `[M-cancel-throw-routing]` + `[M-within-error-conflation]`.
- discussion-log.

---

## Что НЕ входит

- **D65 typed errors / `NovaError` с payload + RTTI** — богатая модель
  ошибок отдельная ось. Plan 49 — минимальный kind-enum + reason-строка,
  не типизированные ошибки. (Reason — строка, не произвольное значение
  как Go `Cause() error`; типизированный cause — future work с D65.)
- **`PANIC`-kind** для unrecoverable (channel cap=0, GC OOM) — сейчас
  `abort()` напрямую. Отдельный kind при необходимости — under-план.
- **Cancellation propagation через каналы** (Go `ctx` в struct, передача
  по каналу) — в Nova явный `tok.bind()` каскад (Plan 47, уже есть).
- **Deadline как keyword** — `within` остаётся stdlib-функцией (как Go
  `context.WithTimeout` — тоже «stdlib»). Не keyword.
- **Closures-in-generics** — Plan 48. `within`/`race` как код требуют
  Plan 48; Plan 49 даёт им семантику.

---

## Риски

**R1 — пропущенная точка cancel-throw'а.** Путь, бросающий отмену через
старый `nova_throw` (не `_cancel`) → останется `USER` → re-throw'ится.
Митигация: Ф.2 явно аудитит ВСЕ места throw'а по `cancel_requested`
(fibers/channels/sleep/select). Тест Ф.4 «отмена не убегает» ловит регресс.

**R2 — `interrupt`-frame vs cancel.** `with`-блок имеет `NovaFailFrame`
И `NovaInterruptFrame`. Cancel — через fail-frame; `interrupt` — через
interrupt-frame. Митигация: kind живёт только в `NovaFailFrame`,
`interrupt` не трогаем; integration-тест Ф.4 `with` + `supervised(cancel:)`.

**R3 — двойной unwind в `emit_with`.** `CANCEL` → re-throw = второй
longjmp из else-ветки setjmp'а. Frame должен быть снят (`nova_fail_pop`)
до re-throw'а. Митигация: порядок pop/re-throw как в существующем
re-throw из handler-body (D65 правило 3 — аналог уже работает).

**R4 — M:N atomic kind+reason.** Три поля (msg/kind/reason) вместо
одного atomic'а. Митигация: kind+reason пишутся обычным store ПОСЛЕ
успешного CAS на msg-указатель; reader читает их увидев non-NULL msg
(release/acquire). USER-precedence требует compare-kind в CAS-петле —
аккуратно, integration-тест Ф.5.

**R5 — defer при cancel-throw.** Если cancel-throw НЕ прогоняет defer'ы
fiber'а — leak/missed-cleanup. Митигация: cancel-throw идёт через тот же
`longjmp`-механизм что и обычный throw, а defer-cleanup уже привязан к
fail-frame unwinding'у; Ф.4 явно тестирует defer+errdefer при отмене.
Если окажется что defer не срабатывает на cancel-path — это баг к
исправлению в рамках Ф.3, не «отложить».

**R6 — USER-precedence surface'ит teardown-ошибки.** После `tok.cancel()`
ошибка в cleanup-коде fiber'а теперь всплывёт (раньше маскировалась
отменой). Это **честнее** (cleanup-failure — реальный баг), но может
быть «шумнее». Митигация: это by-design, документируется; если на
практике мешает — отдельная under-фича «suppress errors during teardown»,
не сейчас.

**R7 — регрессия тестов, полагавшихся на «отмена = Fail».** Тесты,
оборачивающие `supervised(cancel:)` в `with Fail` чтобы поймать
убегавшую отмену — после Ф.3 обёртка становится no-op. Митигация: Ф.4
проходит по Plan-47-тестам, убирает лишние обёртки; Ф.6 regression
ловит остальное.

---

## Size estimate

| Компонент | LOC |
|---|---|
| Ф.0 — NovaThrowKind + frame field + nova_throw_cancel | ~60 |
| Ф.1 — cancel reason на токене + API | ~130 |
| Ф.2 — cooperative-cancel → CANCEL+reason + аудит точек + USER-precedence | ~180 |
| Ф.3 — supervised_run_impl + emit_with kind-aware | ~120 |
| Ф.4 — тесты семантики + чистка Plan-47 обёрток | ~220 |
| Ф.5 — M:N atomic kind+reason | ~90 |
| Ф.6 — regression + docs + spec | ~90 |
| **Итого** | **~890** |

---

## Acceptance criteria

- [ ] `nova_throw_cancel` → `CANCEL`-kind; `nova_throw` → `USER`; kind
      переживает longjmp.
- [ ] Кооперативная отмена (yield/channels/sleep/select) бросает `CANCEL`
      с reason из токена.
- [ ] `CancelToken`: `cancel()` / `cancel(reason)` / `is_cancelled()` /
      `reason() -> str?` — caller-owned, reason переживает scope.
- [ ] `supervised(cancel: tok)` + `tok.cancel()` БЕЗ `with Fail`-обёртки:
      scope выходит чисто, throw наружу не летит.
- [ ] Реальная ошибка fiber'а (`USER`) → `supervised` re-throw'ит,
      внешний `with Fail` handler вызывается с этим сообщением.
- [ ] USER-precedence: отмена + поздняя реальная ошибка → наружу летит
      ошибка, не отмена.
- [ ] `with Fail` НЕ перехватывает cancel-throw как Fail.
- [ ] `defer` / `errdefer` отменённого fiber'а выполняются.
- [ ] Кооперативный `is_cancelled()` → fiber выходит `return` без throw'а.
- [ ] M:N: cross-worker cancel не убегает; cross-worker USER-ошибка
      re-throw'ится; USER-precedence соблюдается.
- [ ] Полный `nova test` (release) — без новых FAIL.
- [ ] `[M-cancel-throw-routing]` + `[M-within-error-conflation]` сняты.

---

## Связь

- [Plan 47](47-supervised-cancel.md) — `supervised(cancel:)` +
  `CancelToken`; Plan 49 чинит семантику отмены, вынесенную Plan 47
  «вне scope».
- [Plan 48](48-closures-in-generics.md) — ортогонален; оба нужны для
  чистого stdlib `within`/`race` (48 — компиляция, 49 — семантика).
- spec D75 §«Семантика отмены» / §«Модель токена» / §«История» —
  обновляются в Ф.6 (добавляется `reason()`, снимается ограничение).
- D65 (Fail-strict, typed errors) — Plan 49 минимально совместим:
  `USER`-kind = текущая D65-семантика; `CANCEL` — ортогональный вид.
  Типизированный cause (Go `Cause() error`) — future work с D65.
