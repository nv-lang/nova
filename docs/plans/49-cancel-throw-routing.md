// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 49: Cancel-throw routing — kinded throws

> **Создан 2026-05-14.**
>
> **СТАТУС:** план, не начат.
>
> **Приоритет:** P1 — закрывает `[M-cancel-throw-routing]` и
> `[M-within-error-conflation]`; делает `supervised(cancel:)` /
> `within` / `race` семантически честными (отмена ≠ ошибка).
>
> **Предшественники:** [Plan 47](47-supervised-cancel.md) ✅ —
> `supervised(cancel:)` + `CancelToken`. **Независим от Plan 48** —
> Plan 49 чинит runtime-семантику; Plan 48 чинит codegen для closures.
> Чистый stdlib `within`/`race` требует ОБА.

---

## Проблема

Брошенная ошибка в Nova — это просто `nova_str` без «вида»
(`NovaFailFrame.error_msg`, effects.h:26-30). На месте перехвата
невозможно отличить:

- реальную ошибку пользователя (`throw "db connection failed"`),
- кооперативную отмену (`"scope cancelled"` из `nova_fiber_yield`).

Текущий обход — строковое сравнение с `"scope cancelled"`. Хрупко.

### Последствие 1 — отмена «убегает» из `supervised(cancel:)`

`nova_supervised_run_impl` (fibers.h) делает `if (err) nova_throw(err)` —
re-throw'ит ВСЁ, включая `"scope cancelled"`. То есть когда снаружи
вызвали `tok.cancel()` — ожидаемое, штатное событие — `supervised`
выкидывает это наружу как ошибку. Вызывающий обязан оборачивать scope в
`with Fail` handler просто чтобы поймать собственную отмену.

### Последствие 2 — `within` не может быть честным

`within`/`race` (Plan 47 Ф.5) вынуждены оборачивать `supervised(cancel:)`
в `with Fail[any]` handler, который ловит cancel-throw. Но тот же handler
неотличимо ловит и **реальную** ошибку из `body()` — обе сводятся к
`None`/проигрышу. Это `[M-within-error-conflation]`: timeout и настоящий
баг в коде неразличимы.

### Корень — throw не несёт «kind»

`nova_throw(msg)` кладёт только `nova_str`. Нет поля, по которому
catch-site понял бы природу throw'а.

---

## Архитектурное решение: kinded throws

**Добавить «вид» брошенному значению.** Минимальный enum:

```c
typedef enum {
    NOVA_THROW_USER   = 0,   /* user `throw`, `?`, `!!`, assert — обычная ошибка */
    NOVA_THROW_CANCEL = 1,   /* кооперативная отмена scope'а ("scope cancelled") */
} NovaThrowKind;
```

`NovaFailFrame` получает поле `error_kind`. `nova_throw(msg)` — kind по
умолчанию `USER` (вся существующая семантика не меняется). Новый
`nova_throw_cancel(msg)` — kind `CANCEL`. Kind переживает `longjmp`
(хранится в frame, ставится до longjmp, читается после).

**`NovaFiberQueue`** получает `first_error_kind` рядом с `first_error` —
spawn-entry при перехвате throw'а fiber'а записывает И сообщение, И kind.

**`nova_supervised_run_impl` становится kind-aware:**

- `first_error_kind == CANCEL` → scope был отменён (штатно, через
  `tok.cancel()`). **Возврат БЕЗ re-throw** — отмена сделала свою работу,
  fiber'ы остановлены, наружу ничего не летит.
- `first_error_kind == USER` → реальная ошибка fiber'а. Re-throw —
  причём через `Nova_Fail_fail` (а не plain `nova_throw`), чтобы
  внешний `with Fail` handler пользователя был вызван (закрывает вторую
  половину `[M-cancel-throw-routing]`: «user with Fail handler не
  вызывается»).

**`with` блок становится kind-aware:** его `setjmp`-frame ловит ЛЮБОЙ
throw, включая cancel. Codegen `emit_with`: если frame поймал
`CANCEL`-kind — это не Fail, handler НЕ запускается; throw
**пробрасывается дальше** (`nova_throw_cancel` снова) к ближайшему
`supervised`. `with Fail` никогда не «обрабатывает» отмену — отмена
структурна, не ошибка.

### Что это даёт — `within` без хака

После Plan 49 (+ Plan 48 для closures) `within` становится тривиальным
и **корректным**, без `with Fail`-обёртки:

```nova
export fn within[T](ms int, body fn() -> T) -> Option[T] {
    let tok = CancelToken.new()
    let mut result Option[T] = None
    supervised(cancel: tok) {
        spawn { Time.sleep(ms); tok.cancel() }
        spawn { result = Some(body()); tok.cancel() }
    }
    result    // timeout → None; body завершился → Some(...);
              // реальная ошибка body() → re-throw наружу, НЕ None
}
```

- Watchdog сработал → work отменён (`CANCEL`) → `supervised` вышел чисто
  → `result == None`. Timeout честно = `None`.
- `body()` бросил реальную ошибку (`USER`) → `first_error_kind == USER`
  → `supervised` re-throw'ит → ошибка летит наружу, НЕ маскируется под
  `None`. Конфляция устранена.

### Почему enum, а не богатый ErrorBox

Богатый `NovaError { kind; msg; payload; type_info }` — это D65 typed
errors, отдельная большая ось. Plan 49 — минимальный incremental шаг:
одно enum-поле в уже существующих структурах. Существующий код
(`nova_throw`, `Nova_Fail_fail`, `with Fail`) продолжает работать —
`USER` это дефолт.

---

## Фазы

### Ф.0 — `NovaThrowKind` + `NovaFailFrame.error_kind` + `nova_throw_cancel`

- `nova_rt/effects.h`: enum `NovaThrowKind { USER, CANCEL }`. Поле
  `NovaThrowKind error_kind` в `NovaFailFrame`. `nova_throw(msg)` ставит
  `error_kind = USER` перед longjmp. Новый `nova_throw_cancel(msg)`
  ставит `CANCEL`.
- Все существующие места `setjmp(frame.jmp)` читают `frame.error_kind`
  при ненулевом возврате — но на Ф.0 ещё никто не ставит `CANCEL`, так
  что поведение не меняется (чистый рефактор + новое API).
- `NOVA_TRY`/`NOVA_CATCH` макросы — `error_kind` доступен через frame.

### Ф.1 — Кооперативная отмена бросает `CANCEL`

- `nova_rt/fibers.h`: `nova_fiber_yield` — `cancel_requested` ветка
  меняет `nova_throw(...)` → `nova_throw_cancel(nova_str_from_cstr("scope cancelled"))`.
- Аналогично — все остальные точки cooperative-cancel-throw'а
  (`channels.h` cancel-during-recv, `nova_sleep` cancel-during-park —
  проверить все места где fiber бросает из-за `cancel_requested`).
- `NovaFiberQueue`: поле `NovaThrowKind first_error_kind`. `nova_scope_init`
  инициализирует `USER`.
- `nova_fiber_report_error(msg)` → `nova_fiber_report_error_kinded(msg, kind)`;
  spawn-entry catch-блок (codegen) передаёт `_ff.error_kind`.

### Ф.2 — `nova_supervised_run_impl` kind-aware

- После цикла, при `err != NULL`:
  - `first_error_kind == CANCEL` → НЕ re-throw; нормальный возврат.
    (Token отвязан, scope чист — отмена отработала.)
  - `first_error_kind == USER` → re-throw через `Nova_Fail_fail(err)`
    (не plain `nova_throw`) — внешний `with Fail` handler вызывается.
    Если handler'а нет — `Nova_Fail_fail` сам падает в `nova_throw` →
    abort с сообщением. Поведение для USER-ошибок без handler'а
    сохраняется.
- `interrupt`-ветка (`pending`) не трогается — interrupt ортогонален.
- Граница: если первым записан `CANCEL`, но позже fiber бросил `USER` —
  «first wins», реальная ошибка в отменяемом scope'е теряется. Для V1
  приемлемо (scope всё равно валится); зафиксировать как
  `[M-cancel-masks-late-error]` в simplifications.md.

### Ф.3 — `emit_with` kind-aware

- Codegen `emit_with` (emit_c.rs:1578): в else-ветке `setjmp` (frame
  поймал throw) — проверять `frame.error_kind`:
  - `CANCEL` → НЕ запускать handler-логику with-блока; re-throw
    `nova_throw_cancel(frame.error_msg)` — отмена пробрасывается к
    ближайшему `supervised`. `with Fail` не «обрабатывает» отмену.
  - `USER` → существующее поведение (handler уже отработал внутри
    `Nova_Fail_fail`; frame-catch — точка приземления unwind'а).
- Проверить взаимодействие с `interrupt`-frame'ом (отдельный
  `NovaInterruptFrame`) — cancel не должен путаться с `interrupt`.

### Ф.4 — Тесты runtime-семантики (без Plan 48)

`nova_tests/concurrency/` — прямые `supervised(cancel: tok)` тесты,
НЕ требующие closures-in-generics (Plan 48):

- **cancel не убегает**: `supervised(cancel: tok) { spawn{...}; spawn{ tok.cancel() } }`
  без обёртки `with Fail` — после scope'а код продолжается нормально,
  никакого throw наружу.
- **реальная ошибка убегает**: fiber бросает реальный `throw "boom"` —
  `supervised` re-throw'ит, внешний `with Fail` handler ловит именно
  `"boom"` (не `"scope cancelled"`).
- **cancel + real error**: один fiber `tok.cancel()`, другой
  `throw "boom"` — `first wins`; проверяем детерминированный исход в
  cooperative FIFO.
- **`with Fail` не глотает отмену**: `with Fail = handler {...} {
  supervised(cancel: tok) { ... tok.cancel() ... } }` — handler НЕ
  вызывается на cancel-throw; cancel доходит до `supervised` и
  гасится там.
- Negative: убрать из мигрированных Plan-47-тестов `with Fail`-обёртки,
  которые были нужны ТОЛЬКО чтобы ловить убегавшую отмену
  (`supervised_cancel_test.nv` тест «tok.cancel() изнутри» — упрощается).

### Ф.5 — M:N: kind в cross-worker error propagation

- Plan 44.5 Layer 5: cross-worker ошибка идёт через
  `NovaFiberQueue.first_error_atomic` (CAS). Добавить
  `first_error_kind_atomic` (или упаковать kind в старший бит/рядом).
- `nova_supervised_run_impl` под M:N читает атомарный kind так же.
- Тест: `runtime.init(N)` + `supervised(cancel:)` + cross-worker cancel
  — отмена не убегает и под M:N.

### Ф.6 — Regression + docs + spec

- Полный `nova test` (release) — без новых FAIL.
- spec `06-concurrency.md`: новая D-decision (следующий свободный номер,
  ~D104) — «kinded throws: cancellation ≠ error». Обновить D75 §«Семантика
  отмены» п.3 (throw + cancel) и §«История» (ограничение снято).
- `docs/project-creation.txt` + `docs/simplifications.md`: закрытие
  плана; снять `[M-cancel-throw-routing]` и `[M-within-error-conflation]`;
  добавить `[M-cancel-masks-late-error]` (Ф.2 граница).
- discussion-log private-репы.

---

## Что НЕ входит

- **D65 typed errors / `NovaError` с payload + type-info** — богатая
  модель ошибок отдельная ось. Plan 49 — минимальный enum-kind, не
  типизированные ошибки.
- **`PANIC`-kind** для unrecoverable (channel cap=0, GC OOM) — сейчас
  такие делают `abort()` напрямую, не `nova_throw`. Если понадобится
  отдельный kind — under-план. V1: `USER` / `CANCEL`.
- **Closures-in-generics** — Plan 48. `within`/`race` как stdlib-код
  требуют Plan 48; Plan 49 даёт им семантику, Plan 48 — компиляцию.
- **`[M-cancel-masks-late-error]`** — «first error wins» когда cancel
  записан раньше реальной ошибки. Осознанная граница V1 (см. Ф.2).

---

## Риски

**R1 — пропущенная точка cancel-throw'а.** Если какой-то путь бросает
`"scope cancelled"` через старый `nova_throw` (не `nova_throw_cancel`) —
он останется `USER`-kind и будет re-throw'иться. Митигация: Ф.1 явно
аудитит ВСЕ места throw'а по `cancel_requested` (fibers.h, channels.h,
sleep). Тест Ф.4 «cancel не убегает» поймает регрессию.

**R2 — взаимодействие с `interrupt`-frame.** `with`-блок имеет и
`NovaFailFrame`, и `NovaInterruptFrame`. Cancel-throw'ит через
fail-frame; `interrupt` — через interrupt-frame. Нельзя перепутать.
Митигация: kind живёт только в `NovaFailFrame`; `interrupt` не трогаем.
Integration-тест Ф.4 с `with` + `supervised(cancel:)` вместе.

**R3 — двойной unwind.** `emit_with` при `CANCEL` делает re-throw —
второй longjmp из else-ветки setjmp'а. Нужно убедиться, что frame уже
снят (`nova_fail_pop` до re-throw'а) — иначе longjmp в себя.
Митигация: аккуратный порядок pop/re-throw, как в существующем
re-throw из handler-body (D65 правило 3 уже делает похожее).

**R4 — M:N atomic kind.** `first_error_atomic` — CAS на указатель.
Добавление kind атомарно — либо второй атомик (рассогласование msg/kind
в окне), либо упаковка. Митигация: упаковать kind в выделенный
`first_error_kind` обычным atomic-store ПОСЛЕ успешного CAS на msg —
reader читает kind только увидев non-NULL msg (release/acquire).

**R5 — поведение существующих тестов.** Многие тесты сейчас полагаются
на то, что отмена прилетает как Fail (оборачивают в `with Fail`). После
Ф.2 отмена не убегает — эти обёртки становятся no-op (handler не
вызывается). Митигация: Ф.4 явно проходит по мигрированным Plan-47
тестам и убирает лишние обёртки; regression Ф.6 ловит остальное.

---

## Size estimate

| Компонент | LOC |
|---|---|
| Ф.0 — NovaThrowKind + frame field + nova_throw_cancel | ~60 |
| Ф.1 — cooperative-cancel throws CANCEL + queue kind field | ~120 |
| Ф.2 — nova_supervised_run_impl kind-aware | ~50 |
| Ф.3 — emit_with kind-aware | ~80 |
| Ф.4 — runtime-семантика тесты + чистка Plan-47 обёрток | ~180 |
| Ф.5 — M:N atomic kind | ~70 |
| Ф.6 — regression + docs + spec | ~80 |
| **Итого** | **~640** |

---

## Acceptance criteria

- [ ] `nova_throw_cancel` ставит `CANCEL`-kind; `nova_throw` — `USER`;
      kind переживает longjmp.
- [ ] Кооперативная отмена (`nova_fiber_yield` по `cancel_requested`,
      channels, sleep) бросает `CANCEL`.
- [ ] `supervised(cancel: tok)` + `tok.cancel()` — БЕЗ `with Fail`
      обёртки: scope выходит чисто, наружу throw не летит.
- [ ] Реальная ошибка fiber'а (`USER`) — `supervised` re-throw'ит,
      внешний `with Fail` handler вызывается именно с этим сообщением.
- [ ] `with Fail` НЕ перехватывает cancel-throw как Fail (handler не
      вызывается; cancel пробрасывается к `supervised`).
- [ ] M:N: cross-worker cancel не убегает из scope'а.
- [ ] Полный `nova test` (release) — без новых FAIL.
- [ ] `[M-cancel-throw-routing]` + `[M-within-error-conflation]` сняты
      из simplifications.md.

---

## Связь

- [Plan 47](47-supervised-cancel.md) — `supervised(cancel:)` +
  `CancelToken`; Plan 49 чинит семантику отмены, которую Plan 47
  явно вынес «вне scope».
- [Plan 48](48-closures-in-generics.md) — ортогонален; оба нужны для
  чистого stdlib `within`/`race` (Plan 48 — компиляция, Plan 49 —
  семантика без error-conflation).
- spec D75 §«Семантика отмены» / §«История» — обновляются в Ф.6.
- D65 (Fail-strict, typed errors) — Plan 49 минимально совместим:
  `USER`-kind = текущая D65-семантика; `CANCEL` — новый ортогональный
  вид, не typed-error.
