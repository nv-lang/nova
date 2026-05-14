# Plan 47: `supervised(cancel:)` — удаление keyword `cancel_scope`

> **Создан 2026-05-14.**
>
> **СТАТУС:** план, не начат.
>
> **Реализует:** ревизию [D75](../../spec/decisions/06-concurrency.md#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
> (2026-05-14) + закрытие [Q-cancel_scope-lambda-syntax](../../spec/open-questions.md).
>
> **Зависит от:** [Plan 46](46-named-parameters.md) (именованные
> аргументы — `supervised(cancel: tok)` использует `cancel:` как
> именованный аргумент).
>
> **Приоритет:** P1 — убирает лишний keyword, чинит dangling-token
> ограничение bootstrap'а.

---

## Зачем

Ревизия D75: keyword `cancel_scope { tok => body }` **удаляется**.
Внешняя отмена выражается именованным аргументом `cancel:` у
`supervised`:

```nova
let tok = CancelToken.new()
supervised(cancel: tok) {
    spawn { fetch(url) }
}
// извне:
spawn { Time.sleep(5_000); tok.cancel() }
```

Причины (см. ревизию D75 и закрытый Q-cancel_scope-lambda-syntax):

1. **Минус один keyword.** `cancel_scope` — это `supervised` + токен;
   схлопывается в именованный аргумент без потери выразительности.
2. **Нет уникального синтаксиса.** `cancel_scope { tok => }` —
   scope-introduced `tok =>` binding, которого больше нигде в языке нет
   (ср. отменённую форму `f(args) { x => body }` в D43).
3. **Caller-owned токен чинит dangling.** Старая модель: токен хранил
   указатель на queue-frame, после выхода из scope'а — dangling
   (известное ограничение bootstrap-реализации). Новая модель: токен
   создаётся вызывающим, переживает scope, `cancel()` на завершённом
   scope'е — no-op.

---

## Архитектурное решение

### Модель токена — caller-owned handle

`CancelToken` — GC-managed объект, создаётся `CancelToken.new()`,
живёт сколько нужно вызывающему коду. `supervised(cancel: tok)` при
входе **привязывает** токен к scope'у, при выходе — **отвязывает**.

**Bind-check:** один `CancelToken` нельзя привязать к двум живым
scope'ам одновременно — повторный bind = runtime panic «token already
bound to a live scope». Это **runtime-проверка** одного поля, не
compile-time (affine/linear-типы несоразмерны — см. D75 §«Почему
runtime-check»).

`tok.cancel()`:
- токен привязан к живому scope'у → scope отменяется (механизм
  `cancel_requested` из D71);
- токен не привязан / scope завершён → **no-op** (безвредно).

### `supervised` остаётся keyword'ом

`supervised` — неустранимая магия (точка регистрации `spawn`-fiber'ов,
D14/D50). `cancel:` — именованный аргумент keyword-конструкции; парсер
keyword-специфичен, но синтаксис консистентен с D102.

### `race` / `with_timeout` — stdlib, не keyword'ы

Строятся на `supervised(cancel:)` + `spawn` + `Channel` + `Time.after`.
См. D75 §«`race` / `with_timeout` — stdlib».

---

## Фазы

### Ф.0 — AST

- `ExprKind::Supervised { body }` → `Supervised { body, cancel: Option<Expr> }`.
- Удалить вариант `ExprKind::CancelScope { token_name, body }`.

### Ф.1 — Лексер + парсер

- Удалить keyword `cancel_scope` (`KwCancelScope`) из лексера.
- Удалить `parse_cancel_scope`.
- `parse_supervised`: после `supervised` — опциональный `( cancel : expr )`,
  затем `{ body }`. V1: единственный допустимый именованный аргумент —
  `cancel:`; прочие имена — diagnostic.
- `cancel_scope` теперь — обычный идентификатор (больше не keyword);
  убрать из keyword-списка в `03-syntax.md` уже сделано.

### Ф.2 — Runtime (`nova_rt/fibers.h`)

- `NovaCancelToken` — heap-объект: `cancel_requested bool`,
  `bound_scope NovaFiberQueue*` (nullable), `linked[]` + `linked_count`.
- `nova_cancel_token_new()` — аллокация GC-managed токена.
- `nova_cancel_token_bind(tok, scope)` — проверка `bound_scope == NULL`,
  иначе panic «token already bound to a live scope»; ставит `bound_scope`.
- `nova_cancel_token_unbind(tok)` — `bound_scope = NULL` (на выходе из
  scope'а).
- `nova_cancel_token_cancel(tok)` — если `bound_scope != NULL`: ставит
  `bound_scope->cancel_requested = true` + walk `linked[]`; иначе no-op.
  Idempotent.
- `nova_cancel_token_is_cancelled(tok)` — `bound_scope ? bound_scope->cancel_requested : false`.
- `nova_cancel_token_bind_cascade(tok, other)` — каскад (бывший `bind`):
  `other.cancel()` вызывает и `tok.cancel()`.

### Ф.3 — Codegen (`emit_c.rs`)

- `emit_supervised`: если `cancel` присутствует — эмитить
  `nova_cancel_token_bind(<expr>, <scope-queue>)` перед телом scope'а,
  `nova_cancel_token_unbind` после (на всех путях выхода, включая
  throw — через тот же механизм, что cleanup'ы scope'а).
- `CancelToken.new()` → `nova_cancel_token_new()`.
- Method-dispatch на receiver-типе `NovaCancelToken*`:
  `tok.cancel()` / `tok.is_cancelled()` / `tok.bind(other)`.
- Удалить `emit_cancel_scope`.

### Ф.4 — Миграция существующих тестов

- `nova_tests/concurrency/cancel_scope_test.nv` →
  `supervised_cancel_test.nv`, переписать на `supervised(cancel: tok)`:
  no-cancel ≡ supervised, `is_cancelled` false по умолчанию, internal
  cancel + peer-non-execute, double-cancel idempotent, bind-cascade.
- `nova_tests/concurrency/cancel_stress_test.nv` — миграция.
- Обновить `07-modules.md` пример был на `select` (уже сделано) — никаких
  `cancel_scope_test.nv` ссылок не остаётся.

### Ф.5 — Stdlib `race` / `with_timeout` / `within`

- `std/.../concurrency` (или соответствующий модуль): `race[T]`,
  `with_timeout[T]` / `within[T]` как обычные функции на
  `supervised(cancel:)` + `spawn` + `Channel` + `Time.after`.
- Зависит от: именованных аргументов (Plan 46 — `Channel[T].new(capacity:)`),
  trailing-формы D43 (статус реализации D43 проверить; если trailing
  ещё не реализован — временно явная closure-форма
  `within(dur, || { body })`).
- Тесты: `race` возвращает первый результат + проигравшие отменены;
  `within` истекает по таймеру + возвращает `Cancelled`.

### Ф.6 — Negative-тесты

`EXPECT_COMPILE_ERROR` / runtime-panic тесты:
- `supervised(cancel: tok)` с тем же `tok` вложенно → runtime panic
  «token already bound to a live scope»;
- `supervised` с неизвестным именованным аргументом (`supervised(foo: x)`)
  → compile error;
- `tok.cancel()` на завершённом scope'е → no-op (positive-проверка:
  не паникует, не throws).

### Ф.7 — Spec sync + docs

- D75 ревизия — уже в `06-concurrency.md` (готово).
- Q-cancel_scope-lambda-syntax — закрыт (готово).
- Перенести старый текст D75 в `spec/decisions/history/` (упомянуто в
  REVISED-блоке D75).
- Обновить `docs/project-creation.txt` + `docs/simplifications.md`
  (bootstrap-ограничения как `[M*]` — в частности наследуемое
  cancel-throw-routing ограничение).
- Запись в discussion-log private-репы.

---

## Что НЕ входит

- **Унификация прочих keyword-scope'ов** (`parallel for`, `select`,
  `forbid`) в trailing-fn функции — Q-cancel_scope-lambda-syntax закрыт
  с решением «не делать». `supervised` остаётся keyword'ом.
- **Compile-time token-scope enforcement** (affine/linear-типы) —
  отвергнуто в ревизии D75.
- **Fix cancel-throw routing через `Nova_Fail_fail`/handler-vtable** —
  известное ограничение bootstrap'а, наследуется, отдельная задача
  (требует различать fiber-throw-from-handler vs cooperative-cancel-throw).
- **Реализация именованных аргументов** — Plan 46.

---

## Size estimate

| Компонент | LOC |
|---|---|
| AST + лексер + парсер (Ф.0-1) | ~120 |
| Runtime `NovaCancelToken` caller-owned (Ф.2) | ~200 |
| Codegen `emit_supervised` + dispatch (Ф.3) | ~200 |
| Миграция тестов (Ф.4) | ~150 |
| Stdlib `race`/`with_timeout` + тесты (Ф.5) | ~250 |
| Negative-тесты (Ф.6) | ~120 |
| **Итого** | **~1040** |

---

## Acceptance criteria

- [ ] Keyword `cancel_scope` удалён (лексер/парсер/AST/codegen);
      `cancel_scope` — обычный идентификатор.
- [ ] `supervised(cancel: tok) { body }` парсится и компилируется.
- [ ] `CancelToken` — caller-owned: создаётся вне scope'а, переживает
      его; `cancel()` на завершённом scope'е — no-op.
- [ ] Bind-check: повторный `supervised(cancel: tok)` с привязанным
      токеном → runtime panic.
- [ ] Мигрированные `supervised_cancel_test.nv` + `cancel_stress_test.nv`
      PASS.
- [ ] Stdlib `race` / `with_timeout` реализованы и протестированы
      (победитель отменяет проигравших; таймаут возвращает `Cancelled`).
- [ ] Negative-тесты Ф.6 PASS.
- [ ] Полный regression `nova test` без новых FAIL (release-сборка).

---

## Связь

- [D75](../../spec/decisions/06-concurrency.md#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
  — ревизованная спецификация.
- [D102](../../spec/decisions/03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)
  — именованные аргументы; `cancel:` — именованный аргумент.
- [D50](../../spec/decisions/06-concurrency.md#d50) — concurrency model.
- [D71](../../spec/decisions/06-concurrency.md#d71) — `cancel_requested`
  flag, cooperative cancellation propagation.
- [D43](../../spec/decisions/03-syntax.md#d43) — trailing-форма для
  stdlib `race`/`within`.
- [Plan 46](46-named-parameters.md) — prerequisite.
- [Plan 44](44-mn-runtime-roadmap.md) — M:N runtime; ортогонален
  (синтаксис отмены vs реализация планировщика).
