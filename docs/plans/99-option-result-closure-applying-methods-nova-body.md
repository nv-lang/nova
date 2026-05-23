# Plan 99 — Closure-applying Option/Result методы на Nova-body

> **Статус:** 🔵 Ф.0 ВЫПОЛНЕН — **re-scope 2026-05-23** (worktree
> `nova-p99`). Probe `Option[T] @my_map[U](f fn(T)->U) -> Option[U]`
> (`nova_tests/plan99_probe/my_map_probe.nv`) подтвердил:
>
> - Block 1 (closure-codegen) — **уже работает** через существующую
>   `NovaClosBase` + `fn_param_sigs` + cast machinery
>   (`emit_c.rs:13983+`). `emit_monomorphized_method` корректно
>   регистрирует fn-typed params в `fn_param_sigs` с mono'd
>   сигнатурой. Никакой новой closure-codegen инфры не требуется.
> - **Block 2** (method-level generic `[U]` в DeclaredBody-dispatch) —
>   текущий Option DeclaredBody (`emit_c.rs:14910+`) **не запускает**
>   `method_extra_subst` inference; логика закодирована только для
>   user-generic dispatch (`emit_c.rs:~16058`). Probe-output показал
>   unresolved `Nova_U` placeholder в типах match-веток и cast-
>   сигнатуре закрытой closure. mono_name = `_nova_int` (без
>   U-суффикса) → name collision risk.
> - **Block 3** (контекстный variant-constructor): зависит от Block 2.
>
> **Re-scope:** для production-grade Plan 99 нужно:
> 1. Extract `method_extra_subst` логику в helper и intergrate в
>    Option/Result DeclaredBody dispatch (2 места).
> 2. Расширить `mono_name` с `Nova_Option_method_<m>_<T>` на
>    `Nova_Option_method_<m>_<T>_<U>_<F>...` (включая method-level
>    generics).
> 3. `infer_expr_c_type` для Option/Result-методов (`emit_c.rs:23215+`,
>    `:23227+`) — хардкодит без method-level inference; нужно
>    учитывать U/F/E для `map`/`map_err`/`ok_or`.
> 4. `register_novaopt_decl(sani(U), U)` lazy-emit для return-Option-
>    типа при mono-эмиссии метода.
> 5. Удалить **6 inline emit-блоков** (`Option.unwrap_or_else`/`map`/
>    `ok_or`, `Result.unwrap_or_else`/`map`/`map_err`) в emit_c.rs
>    **одним коммитом** с переносом тел в core.nv (C-redefinition
>    collision на mono'd C-имени).
>
> Реалистичная оценка — **3.5–4 dev-day** с тестами и регрессией
> (proposal'овые 2.5–3 — недооценка). Ф.1–Ф.6 **не выполнены**;
> требуется отдельная плановая сессия. Plan 99 остаётся **GATED**:
> Plan 98 ✅ (закрыт 2026-05-23) разблокировал inference, но Block 2/3
> implementation = отдельная инициатива. Маркер
> `[M-option-result-closure-methods-deferred]` в `docs/simplifications.md`.
> **Приоритет:** P3 (de-magic / single-source, как Plan 95 / 95.bis).
> **Оценка:** ~2.5–3 dev-day (3 разных инфра-блокера; больше Plan 95.bis).
> **Зависимости (HARD):**
> - [Plan 95](95-builtin-sum-method-mono.md) ✅ (канал «method-only mono» для builtin sum-типов);
> - [Plan 95.bis](95.bis-option-result-pure-methods-nova-body.md) ✅
>   (расширение скоупа на match-only методы);
> - **[Plan 98](98-free-fn-generic-type-param-inference.md) — БЛОКЕР**
>   (infer_type_param_binding должен выводить `T` из `Option[T]`/
>   `Result[T,E]`-параметров, иначе return-type `Option[U]`/`Result[U,E]`
>   не резолвится).
> **Источник:** обсуждение 2026-05-23 после Plan 95.bis — «остальные 5
> closure-applying методов».

## Зачем

Plan 95 + Plan 95.bis перенесли **9 из 14** Option/Result-методов
на Nova-body. Остаётся **5 closure-applying методов**, которые
выразимы на Nova одной строкой, но требуют связки трёх инфра-блокеров:

| Метод | Тело на Nova |
|---|---|
| `Option.unwrap_or_else(f fn()->T)` | `=> match @ { Some(v) => v, None => f() }` |
| `Option.map[U](f fn(T)->U) -> Option[U]` | `=> match @ { Some(v) => Some(f(v)), None => None }` |
| `Option.ok_or[E](err E) -> Result[T, E]` | `=> match @ { Some(v) => Ok(v), None => Err(err) }` |
| `Result.unwrap_or_else(f fn(E)->T) -> T` | `=> match @ { Ok(v) => v, Err(e) => f(e) }` |
| `Result.map[U](f fn(T)->U) -> Result[U, E]` | `=> match @ { Ok(v) => Ok(f(v)), Err(e) => Err(e) }` |
| `Result.map_err[F](f fn(E)->F) -> Result[T, F]` | `=> match @ { Ok(v) => Ok(v), Err(e) => Err(f(e)) }` |

После Plan 99 на Nova-body будет **14 из 14** Option/Result-методов
(остаётся только `unwrap` — Fail-handler, см. Plan 61).

## Не входит в Plan 99

- **`Option.unwrap`** / **`Result.unwrap`** — требуют `fail()` в обычном
  Nova (effect handler-dispatch, Plan 61 lineage). Тело
  `=> match @ { Some(v) => v, None => fail("called unwrap on None") }`
  возможно только когда `fail()` лоуэрит на handler-dispatch без
  компилятор-магии. Отдельный план (Plan 100/Plan 61.bis).

## Три блокера (decision points)

### Блокер 1 — Closure-applying codegen

В теле метода: `f(v)` где `f: fn(T) -> U` — параметр функционального
типа. Currently `emit_c.rs:14080+` (`Option.map` inline) использует
**hardcoded** cast:

```c
((U(*)(void*, T))(((NovaClos_ii*)(f))->fn))(((NovaClos_ii*)(f))->env, v)
```

`NovaClos_ii` — фиксированный layout `{void* fn; void* env}` для одинаковых-
T-параметров (bootstrap-mono). Для произвольной сигнатуры `fn(T)->U` —
нет надёжной маршрутизации; работает только когда `T==U` примитивный.

**Что нужно:** sound closure-invoke механизм:
- **(A)** Универсальный shape `NovaClos_<sig>` для каждой mono'd
  сигнатуры (хешированный mangling). Codegen для `f(v)` подбирает
  shape по статическому типу `f`.
- **(B)** Vtable-style: `f` всегда `void*` указатель на heap-allocated
  `{fn_ptr, env_ptr, sig_descriptor}`; invoke через runtime helper
  `nova_closure_call_1(f, v)` с тип-чеком sig_descriptor.

Plan 99 Ф.0 решает A vs B. Recommended **A** (zero-cost, аналог Rust
`FnOnce`/`Fn`-моно). B — fallback если A непропорционально сложно.

### Блокер 2 — Method-level generic в DeclaredBody dispatch

`Option.map[U](f fn(T)->U) -> Option[U]` — метод имеет **собственный**
type-параметр `[U]` (не от receiver'а). Текущий
`DeclaredBody`-dispatch (Plan 95 Ф.3 / Plan 95.bis) формирует
`type_subst` только для **receiver type-params** (T, или T+E для Result).

**Что нужно:** расширить dispatch чтобы:
1. Обнаружить method-level generics в `fn_decl.generics`.
2. Вывести `U` из call-site: `opt.map(f)` где `f : fn(int)->str` →
   `U = str` (через `infer_type_param_binding` на `fn`-typed param).
   → Зависит от **Plan 98** (currently `infer_type_param_binding`
   работает только для голого T и []T).
3. Добавить `(U, str)` в `type_subst` рядом с `(T, int)`.
4. mono-name: `Nova_Option_method_map_<T>_<U>` (новая схема —
   нужно сверить с существующим naming для user-generic-methods,
   Plan 48 `emit_monomorphized_method` уже поддерживает method-level
   generics в mono-name).

### Блокер 3 — Контекстный return-type для variant constructors

В теле `=> Some(f(v))` Some-variant конструируется. Currently
`Some(x)` inference смотрит на тип `x`. Для `Option.map[U]` →
`Some(f(v))` — `f(v) : U`, конструктор должен быть для `Option[U]`,
не `Option[T]` (receiver).

**Что нужно:** контекстная инференция от return-type аннотации
`fn Option[T] @map[U](...) -> Option[U]`. Plan 95 mono'd method body
имеет return-type `NovaOpt_<U_resolved>`. Конструктор `Some(...)` в
body должен использовать этот же тип.

Похожее уже есть для `Result.ok()` (Plan 95.bis Ф.5) — там
`Some(v) -> Option[T]` где T — receiver. Для `map` — `Some(f(v)) ->
Option[U]` где U — method-level. Расширение существующего механизма.

## Декомпозиция

### Ф.0 — Audit + decision A/B closure-codegen + Plan 98 gate (~0.4 д) — GATE

- **Ф.0.1** Проверить, что Plan 98 ✅ ЗАКРЫТ (turbofish gap).
  Если нет — Plan 99 GATED.
- **Ф.0.2** Decision: closure-codegen подход A (per-sig mono shape)
  vs B (runtime vtable). Probe минимальным патчем — `fn Option[T]
  @my_map[U](f fn(T)->U) -> Option[U] => match @ { Some(v) => Some(f(v)),
  None => None }` на `Option[int].my_map(|x| str.from(x))`.
- **Ф.0.3** Метод-level generic в DeclaredBody dispatch — проверить,
  что Plan 48 `method_extra_subst` логика переиспользуется для
  builtin sum-типов; если нет — оценить delta.
- **Ф.0.4** Контекстный variant-constructor для `Some(f(v))` /
  `Ok(f(v))` / `Err(f(e))` — проверить, что Plan 95.bis механизм
  расширяется на method-level U/F/E.

### Ф.1 — Closure-codegen инфра (~1.0 д)

- Реализация по подходу A или B из Ф.0.2.
- Sound `f(v)` для произвольной mono'd сигнатуры `fn(T)->U`.
- Тесты: closure-call в обычной user-generic-функции (выделенные
  фикстуры).

### Ф.2 — Method-level generic в DeclaredBody (~0.5 д)

- Расширение dispatch в перехватах `NovaOpt_`/`is_result_like`:
  обнаружение `fn_decl.generics`, инференция через
  `infer_type_param_binding` (Plan 98), расширение `type_subst`.
- Mono-name схема: `Nova_<Sum>_method_<m>_<recv_T>_<method_U>` —
  сверить с naming для user-generic-methods.

### Ф.3 — Контекстный return-constructor (~0.3 д)

- Variant-construct в body учитывает return-type аннотацию метода.
- `register_novaopt_decl(U)` / `register_novares_decl(U, E)` для
  return-типов с method-level type-params.

### Ф.4 — Перенос 6 методов в core.nv (~0.2 д)

- `Option.unwrap_or_else`/`map`/`ok_or` → Nova-body.
- `Result.unwrap_or_else`/`map`/`map_err` → Nova-body.
- Удалены: inline emit в `emit_c.rs` (`unwrap_or_else`/`map`/etc
  блоки), `method_routing` entries (если есть — большинство уже
  `<inline>`-sentinel, тоже убрать).

### Ф.5 — Тесты позитив + негатив (~0.3 д)

- `nova_tests/plan99/`:
  - `option_map_migrated.nv` — `Option[int].map(|x| x*2)`,
    `Option[int].map(|x| str.from(x))` (T≠U), на str/char/record.
  - `option_ok_or_migrated.nv` — `Option → Result` projection с
    разными E типами.
  - `option_unwrap_or_else_migrated.nv` — lazy default.
  - `result_map_migrated.nv` — `Result[T,E].map(|x| ...)`.
  - `result_map_err_migrated.nv` — Err-side transform.
  - `result_unwrap_or_else_migrated.nv`.
  - Negative: closure-arg с неверной сигнатурой (`|x| x` для
    `f fn(int)->str`) → loud compile error (type-check, не CC).

### Ф.6 — Регрессия + spec/docs (~0.3 д)

- Полный `nova test` — 0 новых FAIL.
- spec `08-runtime.md` — расширить Plan 95.bis блок до Plan 99 (14/14
  методов).
- Plan 78 amend — расширить (теперь весь builtin Option/Result в
  Nova, кроме `unwrap`).
- README + project-creation + discussion-log.

## Acceptance criteria

- [ ] **Ф.0**: Plan 98 ✅; closure-codegen подход утверждён (A рек.);
      probe `my_map` компилируется и работает.
- [ ] Closure-applying codegen sound для произвольной `fn(T)->U`.
- [ ] Method-level generic в DeclaredBody — `Option.map[U]` mono'd
      per-(T, U).
- [ ] Контекстный variant-constructor — `Some(f(v))` в `map[U]` body
      → `Option[U]`, не `Option[T]`.
- [ ] 6 методов — Nova-body в `core.nv`; inline emits в codegen
      удалены.
- [ ] Тесты позитив + негатив; полный `nova test` — 0 новых FAIL.
- [ ] **14 из 14** Option/Result-методов на Nova (остаётся только
      `unwrap` — Plan 61 lineage).

## Non-scope

- **`Option.unwrap`** / **`Result.unwrap`** — Plan 61 (Fail-handler).
- **Универсальный closure-runtime** (D75 full) — Plan 99 берёт
  минимально достаточный механизм для Option/Result, не закрывает
  D75 целиком. Если Ф.0.2 → подход A, многое из D75 может
  переиспользоваться/откладываться.

## Связь с другими планами

- **Plan 95** ✅ — фундамент (method-only mono для builtin sum-типов).
- **Plan 95.bis** ✅ — расширение на match-only методы.
- **Plan 98** — БЛОКЕР (turbofish/inference для generic-typed params
  необходим для Plan 99 Ф.0.4 и Ф.2).
- **Plan 61** — параллельный (Fail-handler для `unwrap`); закроет
  последний 14-й метод.
- **Plan 78** — узкий пересмотр Ф.1: Plan 99 расширяет до 14/14
  builtin Option/Result методов; реестр C-routing остаётся только
  для `unwrap` после Plan 61.
- **D75** (closure ABI) — Ф.0.2 решает: Plan 99 берёт подмножество
  или строит свой минимум.
