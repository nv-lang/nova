// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 49: Cancellation semantics — kinded throws + typed cancel reason

> **Создан 2026-05-14. Расширен 2026-05-15** дважды: (1) kinded throws →
> полная модель отмены (reason/cause, USER-precedence, defer-on-cancel);
> (2) `CancelToken` → **`CancelToken[T]`** — типизированная причина отмены,
> строго лучше Go (`error`) и TS (`any`).
>
> **СТАТУС:** план, не начат.
>
> **Приоритет:** P1 — закрывает `[M-cancel-throw-routing]` и
> `[M-within-error-conflation]`; делает отмену first-class: «отмена ≠
> ошибка», с **типизированной** причиной, без потери реальных ошибок.
>
> **Предшественники:** [Plan 47](47-supervised-cancel.md) ✅.
> **Staging относительно Plan 48:**
> - Ф.0–Ф.5 + Ф.7 (kinded throws, routing, `CancelToken[str]`,
>   USER-precedence, defer-on-cancel, M:N) — **независимы от Plan 48**,
>   шипятся как есть.
> - Ф.6 (обобщение `CancelToken[str]` → `CancelToken[T]`) **требует
>   Plan 48** (мономорфизация generic-типа). Дизайн `[T]` полностью
>   зафиксирован здесь — это не «упрощение до str», а явная стадия:
>   `CancelToken` == `CancelToken[str]`, `CancelToken[T]` — общая форма,
>   landing после Plan 48. Backward-compatible эволюция, никто не
>   переписывает существующий код.

---

## Зачем и сравнение с индустрией

Сейчас брошенная ошибка в Nova — просто `nova_str` без «вида»
(`NovaFailFrame.error_msg`). Отмена scope'а (`"scope cancelled"`)
неотличима от реальной ошибки кроме как строковым сравнением. Следствия:
отмена «убегает» из `supervised(cancel:)` наружу как ошибка; `within`
вынужден ловить её `with Fail`-обёрткой, которая неотличимо глотает и
реальные ошибки (`[M-within-error-conflation]`).

Как сделано в индустрии (с чем просили сравнивать):

| | Механизм отмены | Отмена — это ошибка? | Тип причины отмены | Реальная ошибка vs отмена |
|---|---|---|---|---|
| **Go** | `context.Context` + `ctx.Done()` канал | **Нет** — значение (`ctx.Err()`) | `context.Cause()` → `error` (интерфейс, не типизирован конкретно) | errgroup: first-error-wins |
| **Rust** | future-drop / `CancellationToken` | **Нет** — структурно отдельно от `Result` | токен может нести reason (часто типизирован) | независимы по конструкции |
| **TypeScript** | `AbortController`/`AbortSignal` | **Полу** — `AbortError` в reject-канале | `signal.reason` → `any` (не типизирован) | различимы по типу `AbortError` |
| **Nova сейчас** | cancel-throw (`nova_str`) | **Да** — неотличимо от Fail | нет | **не различимы** (строковый хак) |
| **Nova (Plan 49)** | kinded cancel-throw | **Нет** — `CANCEL`-kind, не убегает из scope'а | **`CancelToken[T]` — конкретный тип `T`** | **USER-precedence** — реальная ошибка перетирает отмену |

Nova сейчас хуже всех: отмена = ошибка, без причины, неотличима.
Plan 49 выводит на паритет и **по двум осям строго лучше**:
- **vs Go:** отмена не ошибка (паритет); **лучше** причина — `CancelToken[T]`
  даёт **конкретный тип** `T`, а Go `Cause()` это `error`-интерфейс (надо
  type-assert'ить); **лучше** — USER-ошибка имеет приоритет над отменой
  (Go errgroup теряет реальную ошибку при first-wins, мы — нет).
- **vs Rust:** отмена структурно отдельна от ошибок (паритет);
  типизированный reason (паритет, у обоих типизирован).
- **vs TS:** отмена **не** всплывает в caller'а как ошибка (**лучше** TS,
  где `AbortError` всё же в reject-канале); **лучше** причина — `[T]`
  против `any` у `signal.reason`.

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

### 2. Typed cancel reason — `CancelToken[T]` (лучше Go/TS)

`CancelToken` становится **generic** — `CancelToken[T]`, где `T` — тип
причины отмены. `CancelToken` без параметра — сахар для `CancelToken[str]`
(самый частый случай: причина-сообщение). API:

- `CancelToken[T].new()` / `CancelToken.new()` (= `[str]`).
- `tok.cancel(reason: T)` — отмена с типизированной причиной. Для
  `CancelToken[str]` — `tok.cancel()` без аргумента, дефолт `"cancelled"`.
  Для `CancelToken[T]`, `T ≠ str` — `reason` обязателен (нет дефолта для
  произвольного `T`); пропуск → compile error.
- `tok.is_cancelled() -> bool` — был ли вызван `cancel()` (как
  `ctx.Err() != nil` / `signal.aborted`).
- `tok.reason() -> Option[T]` — типизированная причина; `None` если не
  отменён. Это `ctx.Cause()` / `signal.reason`, но **с конкретным типом**
  — можно `match` по структурированной причине, без type-assert (Go) и
  без `any` (TS):

```nova
type TaskCancel { Timeout { after_ms int }, UserAborted, ParentDied }
let tok = CancelToken[TaskCancel].new()
// ...
match tok.reason() {
    Some(Timeout { after_ms }) => retry_longer(after_ms)
    Some(UserAborted)          => show_cancelled_ui()
    Some(ParentDied)           => propagate_shutdown()
    None                       => /* завершился штатно */
}
```

**Runtime-представление.** `NovaCancelToken` хранит `reason` как `void*`
(box'нутое значение `T`) + флаг наличия. Nova-уровень `CancelToken[T]` —
типизированная обёртка над этим runtime-struct'ом; мономорфизация
(Plan 48) даёт типобезопасный `T` на Nova-уровне без рантайм-RTTI.
Ф.0–Ф.5 работают с `CancelToken[str]` (T = `str`, `reason` = `nova_str`)
— **без зависимости от Plan 48**; Ф.6 обобщает до `CancelToken[T]`.

**cancel-throw** несёт reason: для `[str]` — прямо `error_msg`; для
`[T]` — box'нутый `T` в отдельном слоте frame'а (`error_reason_ptr`).
`within` отменяет watchdog'ом с reason `"deadline exceeded"` — это
естественно даёт Go-различие `Canceled` vs `DeadlineExceeded` без
отдельного enum'а, а с `CancelToken[T]` — любую структурированную причину.

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

### Ф.1 — Cancel reason на `CancelToken` (str-форма; независимо от Plan 48)

- `nova_rt/fibers.h`: поле `void* reason` + `bool has_reason` в
  `NovaCancelToken` (caller-owned, переживает scope). В str-форме
  `reason` указывает на box'нутый `nova_str`.
- `nova_cancel_token_cancel(tok)` → `nova_cancel_token_cancel_reason(tok, reason_ptr)`;
  старый — wrapper с дефолтом `"cancelled"`.
- `nova_cancel_token_reason(tok) -> void*` (NULL если не отменён).
- Codegen: `tok.cancel()` / `tok.cancel(reason)` (опц. аргумент для
  `CancelToken` = `[str]`), `tok.reason()` → `Option[str]`. Dispatch на
  `NovaCancelToken*`.
- Runtime-struct сразу проектируется под `void*`-reason (а не `nova_str`)
  — чтобы Ф.6 (`[T]`) был чистым обобщением без переделки struct'а.
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

### Ф.6 — Обобщение `CancelToken[str]` → `CancelToken[T]` (требует Plan 48)

После Plan 48 (мономорфизация generic-типов) `CancelToken` становится
полноценным `CancelToken[T]`:
- `CancelToken` без параметра → сахар `CancelToken[str]` (existing-код
  не меняется — backward-compatible).
- `CancelToken[T].new()`, `tok.cancel(reason: T)`, `tok.reason() -> Option[T]`.
  Runtime `reason` уже `void*` (Ф.1) — box'ит `T` (мономорфизация даёт
  типобезопасный un-box на Nova-уровне, без RTTI).
- **Cross-type cascade — конвертация причины через `From` (D73/D77).**
  `child.cancelled_by(parent)` где `child: CancelToken[A]`,
  `parent: CancelToken[B]` — типы причин разные. Родитель может быть
  отменён по РАЗНЫМ `B`-причинам, и мы хотим, чтобы причина ребёнка их
  **отражала**, а не была фиксированной константой. Решение — каскад
  конвертирует причину через уже существующий протокол `From`:
  - **Same-type** (`A == B`): причина пробрасывается как есть, конвертация
    не нужна. `child.cancelled_by(parent)`.
  - **Cross-type** (`A != B`): требуется `A: From[B]` — конвертация
    `B → A`. При отмене `parent` с `b_reason` ребёнок отменяется с
    `A.from(b_reason)`. Compile-time проверка: нет `From[B] for A` →
    понятная ошибка «no conversion from B to A — define `A.from(B)` or
    use a shared reason type».
  - **«Не важна причина родителя»** — частный случай: пишешь `A.from(B)`,
    игнорирующий вход и возвращающий константу. Это покрывает старый
    `on_cancel`-вариант, но не навязывает его.
  Почему `From`, а не bespoke `on_cancel`-аргумент: (1) `From`/`TryFrom`
  — **идиоматичный** механизм конверсий в Nova, не надо изобретать;
  (2) **сохраняет «почему»** — причина ребёнка проекция реальной причины
  родителя, а не фиксированная заглушка; (3) compile-time типобезопасно.
  Это **строго лучше Go**: Go `context.Cause` пробрасывает причину
  предка как `error`-интерфейс (надо type-assert'ить и причина одного
  типа на всё дерево); у нас — типизированная проекция per-уровень.
- Тесты: `CancelToken[CustomEnum]` — `match` по структурированной
  причине; cross-type каскад с `From`-конвертацией; `within`/`race`
  (Plan 48) с типизированной причиной таймаута.
- Снять любые str-only оговорки; обновить spec D75 на `[T]`.

### Ф.7 — Regression + docs + spec

- Полный `nova test` (release) — без новых FAIL.
- spec `06-concurrency.md`: D-decision (~D104) — «cancellation semantics:
  kinded throws, отмена ≠ ошибка, typed reason `CancelToken[T]`,
  USER-precedence, cross-type cascade через `From`-конвертацию». Обновить
  D75 §«Семантика отмены», §«Модель токена» (typed reason), §«История»
  (ограничение снято).
- `project-creation.txt` + `simplifications.md`: закрытие; снять
  `[M-cancel-throw-routing]` + `[M-within-error-conflation]`.
- discussion-log.

---

## Что НЕ входит

- **D65 typed errors / `NovaError` с payload + RTTI** — богатая модель
  *ошибок* отдельная ось. Plan 49 типизирует *причину отмены*
  (`CancelToken[T]`), а не Fail-канал. `CANCEL`-kind остаётся
  enum-флагом; типизация самой ошибки (`USER`-канал) — future work с D65.
- **`PANIC`-kind** для unrecoverable (channel cap=0, GC OOM) — сейчас
  `abort()` напрямую. Отдельный kind при необходимости — under-план.
- **Cancellation propagation через каналы** (Go `ctx` в struct, передача
  по каналу) — в Nova явный `child.cancelled_by(parent)` каскад (Plan 47,
  уже есть).
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
проходит по Plan-47-тестам, убирает лишние обёртки; Ф.7 regression
ловит остальное.

**R8 — cross-type cascade (`CancelToken[A].cancelled_by(CancelToken[B])`).**
Типы причин разные. Митигация (раздел Ф.6): каскад конвертирует причину
через протокол `From` — same-type пробрасывает как есть, cross-type
требует `A: From[B]` (compile-time проверка). Сохраняет «почему»
(причина ребёнка — проекция реальной причины родителя), использует
идиоматичный Nova-механизм конверсий, никакого `A|B`-union. Решение
зафиксировано в дизайне.

**R9 — Ф.6 зависит от Plan 48.** `CancelToken[T]` — generic-тип, требует
мономорфизации. Митигация: Ф.0–Ф.5 спроектированы на `CancelToken[str]`
с `void*`-reason в runtime-struct'е — шипятся независимо; Ф.6 —
обобщение без переделки struct'а. Если Plan 48 задержится — Plan 49
полезен и без Ф.6 (str-причина уже лучше текущего «нет причины»).

---

## Size estimate

| Компонент | LOC | Зависит от Plan 48 |
|---|---|---|
| Ф.0 — NovaThrowKind + frame field + nova_throw_cancel | ~60 | нет |
| Ф.1 — cancel reason (`void*`) на токене + str-API | ~140 | нет |
| Ф.2 — cooperative-cancel → CANCEL+reason + аудит + USER-precedence | ~180 | нет |
| Ф.3 — supervised_run_impl + emit_with kind-aware | ~120 | нет |
| Ф.4 — тесты семантики + чистка Plan-47 обёрток | ~220 | нет |
| Ф.5 — M:N atomic kind+reason | ~90 | нет |
| Ф.6 — обобщение `CancelToken[str]` → `CancelToken[T]` + cascade | ~200 | **да** |
| Ф.7 — regression + docs + spec | ~90 | нет |
| **Итого** | **~1100** | Ф.0–Ф.5+Ф.7 (~900) независимы |

---

## Acceptance criteria

- [ ] `nova_throw_cancel` → `CANCEL`-kind; `nova_throw` → `USER`; kind
      переживает longjmp.
- [ ] Кооперативная отмена (yield/channels/sleep/select) бросает `CANCEL`
      с reason из токена.
- [ ] `CancelToken` (= `[str]`): `cancel()` / `cancel(reason)` /
      `is_cancelled()` / `reason() -> str?` — caller-owned, reason
      переживает scope.
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

**После Plan 48 (Ф.6):**

- [ ] `CancelToken[T].new()`, `cancel(reason: T)`, `reason() -> Option[T]`
      — типизированная причина; `match` по структурированной причине
      работает.
- [ ] `CancelToken` без параметра == `CancelToken[str]` (existing-код
      компилируется без изменений).
- [ ] Cross-type каскад: `child.cancelled_by(parent)` где
      `child: CancelToken[A]`, `parent: CancelToken[B]` — требует
      `A: From[B]`; при отмене `parent` с `b_reason` ребёнок отменяется
      с `A.from(b_reason)`. Нет `From[B] for A` → понятная compile-error.
- [ ] Same-type каскад (`A == B`) — причина родителя пробрасывается
      ребёнку как есть, без конвертации.

---

## Связь

- [Plan 47](47-supervised-cancel.md) — `supervised(cancel:)` +
  `CancelToken`; Plan 49 чинит семантику отмены, вынесенную Plan 47
  «вне scope».
- [Plan 48](48-closures-in-generics.md) — Ф.0–Ф.5+Ф.7 **независимы** от
  Plan 48; Ф.6 (`CancelToken[T]`) **требует** мономорфизацию из Plan 48.
  Оба нужны для чистого stdlib `within`/`race` (48 — компиляция,
  49 — семантика).
- spec D75 §«Семантика отмены» / §«Модель токена» / §«История» —
  обновляются в Ф.7 (typed `reason()`, cross-type cascade через
  `From`-конвертацию, снимается ограничение).
- D65 (Fail-strict, typed errors) — Plan 49 минимально совместим:
  `USER`-kind = текущая D65-семантика; `CANCEL` — ортогональный вид.
  Plan 49 типизирует *причину отмены* (`CancelToken[T]`); типизация
  *Fail-канала* — future work с D65.
- D73/D77 (`From` / `TryFrom` протоколы) — cross-type каскад
  (`CancelToken[A].cancelled_by(CancelToken[B])`, Ф.6) использует
  `A: From[B]` для конвертации причины. Переиспользование существующего
  механизма конверсий, не bespoke API.
