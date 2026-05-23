# Plan 99 — Closure-applying Option/Result методы на Nova-body (master)

> **Статус:** 📋 RE-AUDIT 2026-05-23 — план переписан после clean-slate
> аудита; декомпозирован на 4 sub-plan'а (99.1–99.4) для атомарной
> verifiability. Каждый sub-plan = verifiable shipment unit с
> регрессионной чисткой.
> **Приоритет:** P3 (de-magic / single-source — паритет Plan 95/95.bis).
> **Оценка (re-audit):** ~3 dev-day (1.5+0.5+0.5+0.5). Прежняя оценка
> 2.5–3 была близка, но без декомпозиции — non-atomic shipping risk.
> **Зависимости (HARD):**
> - [Plan 95](95-builtin-sum-method-mono.md) ✅ — канал `DeclaredBody`-
>   mono для builtin sum-типов.
> - [Plan 95.bis](95.bis-option-result-pure-methods-nova-body.md) ✅ —
>   расширение скоупа на match-only методы.
> - [Plan 98](98-free-fn-generic-type-param-inference.md) ✅ —
>   `infer_type_param_binding` рекурсирует сквозь `Option[T]`/
>   `Result[T,E]`/user-generics (нужно для inference U из closure-arg).
> **Источник:** Plan 95.bis closing + clean-slate re-audit 2026-05-23.

## Цель

После Plan 99: **14/14 Option/Result-методов на Nova-body**
в `std/prelude/core.nv`. Остаётся только `unwrap` (Fail-handler,
Plan 61 lineage).

| Метод | Тело на Nova |
|---|---|
| `Option.unwrap_or_else(f fn()->T) -> T` | `=> match @ { Some(v) => v, None => f() }` |
| `Option.map[U](f fn(T)->U) -> Option[U]` | `=> match @ { Some(v) => Some(f(v)), None => None }` |
| `Option.ok_or[E](err E) -> Result[T, E]` | `=> match @ { Some(v) => Ok(v), None => Err(err) }` |
| `Result.unwrap_or_else(f fn(E)->T) -> T` | `=> match @ { Ok(v) => v, Err(e) => f(e) }` |
| `Result.map[U](f fn(T)->U) -> Result[U, E]` | `=> match @ { Ok(v) => Ok(f(v)), Err(e) => Err(e) }` |
| `Result.map_err[F](f fn(E)->F) -> Result[T, F]` | `=> match @ { Ok(v) => Ok(v), Err(e) => Err(f(e)) }` |

## Clean-slate аудит (2026-05-23) — что есть, чего нет

### ✅ Что УЖЕ работает (re-audit находки)

| Инфра | Где | Готовность |
|---|---|---|
| Closure invoke для arbitrary `fn(T)->U` | `emit_c.rs:14321-14336` — NovaClosBase + explicit cast `(ret(*)(void*, params))((NovaClosBase*)f)->fn)(env, args)` | ✅ Production. Используется для всех non-shortcut signature combos. |
| `fn_param_sigs` registration в mono методе | `emit_monomorphized_method` инсертит fn-typed params с mono'd sig | ✅ Работает (Plan 48). |
| `method_extra_subst` inference (T,U) из closure-args | `emit_c.rs:16386-16494` — user-generic dispatch | ✅ Robust + mature, но **только для user-generic** dispatch. |
| Contextual return-type | `current_fn_return_ty` set в `emit_monomorphized_method`. Использует `Option.None` (path-form, `:17264`) | ✅ Mechanism exists, **extend нужно** на bare `Some`/`Ok`/`Err`. |
| `register_novaopt_decl` для arbitrary T | Plan 14 Ф.1 + Plan 95.bis. Вызывается из `Option.Some(v)` path form (`:17256`). | ✅ Готово. |
| Plan 98 inference | `Option[T]`/`Result[T,E]`/user-generics в позиции param | ✅ Закрыт. |

### ❌ Что НЕ работает (real gaps)

| Гэп | Где | Что делать |
|---|---|---|
| **G1.** Option DeclaredBody dispatch не запускает `method_extra_subst` | `emit_c.rs:14910-14964` | Integrate (Plan 99.1). |
| **G2.** Result DeclaredBody dispatch — same | `emit_c.rs:15148+` | Integrate (Plan 99.1). |
| **G3.** mono_name не включает method-level type-args | `format!("Nova_Option_method_{}_{}", m, T)` (без U) | Расширить: `_{T}_{U}_{F}...` (Plan 99.1). |
| **G4.** `register_novaopt_decl(U)` / `register_novares_decl` не вызывается для return-types с method-level generics | DeclaredBody dispatch | Trigger в dispatch (Plan 99.1). |
| **G5.** Bare `Some(v)` Ident-form всегда возвращает `NovaOpt_nova_int` | `emit_c.rs:14060` `find_variant_compat` → `nova_make_Option_Some` (hardcoded `nova_int → NovaOpt_nova_int` в `array.h:276`) | Use `current_fn_return_ty` если `NovaOpt_<X>` (Plan 99.2). |
| **G6.** Bare `Ok(v)`/`Err(e)` defaults к `nova_int/nova_str` | `emit_c.rs:14362-14378` | Use `is_result_like(current_fn_return_ty)` (Plan 99.2). |
| **G7.** `infer_expr_c_type` для Option/Result methods хардкодит без method-level | `emit_c.rs:23215+`/`:23227+` — `"map" \| "or" => NovaOpt_<elem_ty>` | Учитывать U через method-level inference (Plan 99.1). |
| **G8.** 6 inline emit-блоков в `emit_c.rs` (`Option.map`/`unwrap_or_else`/`ok_or`, `Result.map`/`map_err`/`unwrap_or_else`) | `:15008`, `:15249`, etc. | Удалить atomic с миграцией (Plan 99.3). |

## Сравнение с Go / Rust / TS

| Язык | Closure-applying методы |
|---|---|
| **Rust** | `Option::<T>::map<U, F: FnOnce(T) -> U>(self, f: F) -> Option<U>` — full mono per (T, U). Zero-cost (FnOnce — unique type). `Some(x)` infers из return-type. **Gold standard.** |
| **Go** | `func Map[T, U any](o Option[T], f func(T) U) Option[U]` — free function (нет методов на generic-типах). Mono'd per (T,U). Closure — heap-allocated `{fn, env}` (как NovaClosBase). Inference unifies полностью. |
| **TS** | `function map<T, U>(o: T \| undefined, f: (x: T) => U): U \| undefined` — type-erased at runtime. Closure = JS function. |
| **Nova (сейчас)** | Bootstrap-mono: `Option.map` hardcode T==U primitive (NovaClos_ii). Для T≠U или non-primitive — broken. **Хуже Rust/Go**. |
| **Nova (цель Plan 99)** | Full mono per (T, U) — паритет Rust. Closure через NovaClosBase + explicit cast (одна indirection vs Rust's zero-cost — bootstrap-acceptable; Plan 11 Ф.x потом оптимизирует). Inference унифицирует closure-arg → U. |

## Декомпозиция — 4 sub-plan'а

### [Plan 99.1](99.1-method-level-generic-in-declared-body.md) — Method-level generic в DeclaredBody (foundation)

**Цель:** инфраструктура. НЕ трогает Nova code, НЕ мигрирует методы.
После 99.1: probe `fn Option[T] @my_map[U](f fn(T)->U) -> Option[U]
=> match @ { Some(v) => Some(f(v)), None => None }` компилируется и
работает.

- **Ф.1** Extract `method_extra_subst` (`:16386-16494`) в reusable
  helper `fn resolve_method_level_subst(&mut self, fn_decl, args,
  receiver_subst) -> Result<Vec<(String,String)>, String>`. Refactor
  user-generic dispatch использовать helper. Regression на
  `nova_tests/plan48_mpm/`, `generics/`.
- **Ф.2** Integrate в Option DeclaredBody (`:14910+`):
  - Call helper для method-level inference (если `fn_decl.generics`
    непуст).
  - Extend `type_subst` с method extras.
  - mono_name: `Nova_Option_method_<m>_<T_sani>` →
    `Nova_Option_method_<m>_<T_sani>[_<U_sani>...]`
    через `Self::compute_mono_name`.
  - `register_novaopt_decl(sani(U), U)` для U-typed return-Option.
  - Fix `infer_expr_c_type` для Option methods (`:23215+`) — учитывать
    method-level inferred U через preview-inference.
- **Ф.3** Integrate в Result DeclaredBody (`:15148+`) — параллельно
  Ф.2, с `register_novares_decl(U, err_c)`.
- **Ф.4** Probe `my_map[U]` + `my_result_map[U]` — PASS.
- **Ф.5** Регрессия + commit.

**Acceptance:** probe-методы (с другими именами, не сталкивающиеся с
inline emit) на Option/Result с method-level generic компилируются и
выполняются корректно для T≠U.

### [Plan 99.2](99.2-contextual-variant-constructors.md) — Contextual variant constructors

**Цель:** `Some(v)`/`None`/`Ok(v)`/`Err(e)` (bare Ident form) в
expression-position учитывают `current_fn_return_ty` для выбора
mono-репрезентации. Независимый sub-plan.

- **Ф.1** `Some(v)` bare (`:14060` chain): когда
  `current_fn_return_ty.starts_with("NovaOpt_")`, эмитить typed
  compound literal `(NovaOpt_<X>){.tag=Some, .value=v}` вместо
  `nova_make_Option_Some(v)`. Параллель `Option.Some(v)` path form
  на `:17251`.
- **Ф.2** `None` bare — extract type из `current_fn_return_ty`
  (analog path form на `:17264`).
- **Ф.3** `Ok(v)`/`Err(e)` bare (`:14362-14378`): когда
  `is_result_like(current_fn_return_ty)`, использовать
  `novares_ok_err(current_fn_return_ty)` для (T,E) типов вместо
  defaults `nova_int/nova_str`.
- **Ф.4** Тесты — small Nova fixtures с explicit `-> Option[U]` /
  `-> Result[U,E]` return annotation, bare `Some/None/Ok/Err` в body.
  Verify mono'd ctor.

**Acceptance:** non-receiver-typed variant constructor в body — паритет
с path form. `Some(f(v))` в `map[U] -> Option[U]` body эмитит mono'd
`NovaOpt_<U>` constructor.

### [Plan 99.3](99.3-migrate-6-closure-methods.md) — Migrate 6 closure-applying methods (consumer)

**Зависит:** 99.1 ✅ + 99.2 ✅.

**Atomic shipping (C-redefinition risk):** каждый метод =
**один коммит** с парой (core.nv migration + delete inline emit block
в emit_c.rs). Метод-за-методом с регрессией.

- **Ф.1** `Option.map` — core.nv тело + удалить `:15008-15034`. Regression
  plan62/89/95/95.bis/json/std.
- **Ф.2** `Option.unwrap_or_else` — analog.
- **Ф.3** `Option.ok_or` — analog (+ test Option→Result projection).
- **Ф.4** `Result.unwrap_or_else` — analog.
- **Ф.5** `Result.map` — analog.
- **Ф.6** `Result.map_err` — analog.

**GATE на каждой Ф:** если regression non-trivial — STOP, document,
don't ship broken.

### [Plan 99.4](99.4-tests-spec-docs.md) — Comprehensive tests + spec + docs + close

- Comprehensive positive + negative tests в `nova_tests/plan99/`:
  - `option_map_typed.nv` — int→str, str→char, User→int.
  - `option_unwrap_or_else_lazy.nv` — lazy default invocation.
  - `option_ok_or_to_result.nv` — Option→Result(T,E) с разными E.
  - `result_map_ok_transform.nv`.
  - `result_map_err_transform.nv`.
  - `result_unwrap_or_else_recovery.nv`.
  - **Negative:** wrong closure sig (type-check loud-fail), wrong U
    arg type, missing method-level type (turbofish needed message).
- Полный `nova test` — 0 новых FAIL.
- spec `08-runtime.md` — расширить Plan 95.bis блок до Plan 99 (14/14).
- Plan 78 amend — финальный (после Plan 99 реестр C-routing только
  для `unwrap`).
- Plan 61 lineage / `unwrap` — отдельный план (out-of-scope).
- README, project-creation, discussion-log.
- Маркер `[M-option-result-closure-methods-deferred]` в simplifications
  → ✅ ЗАКРЫТО.

## Acceptance criteria (master)

- [ ] **Plan 99.1** ✅ — method-level generic в DeclaredBody, probe
      `my_map[U]` работает.
- [ ] **Plan 99.2** ✅ — bare `Some`/`Ok`/`Err` используют
      `current_fn_return_ty`.
- [ ] **Plan 99.3** ✅ — 6 методов мигрированы atomic; inline emits
      удалены.
- [ ] **Plan 99.4** ✅ — full nova test 0 regressions; spec/docs/logs.
- [ ] **14 из 14** Option/Result-методов на Nova-body (только `unwrap`
      C-routed — Plan 61).
- [ ] Никакой деградации vs Rust: full mono per (T,U); closure через
      NovaClosBase + cast = одна indirection (Plan 11 follow-up
      optimize).

## Non-scope

- **`unwrap`** — Plan 61 (Fail-handler dispatch). Отдельная линия.
- **Universal D75 closure ABI** — Plan 99 берёт subset через
  NovaClosBase; не закрывает D75 целиком.
- **Zero-cost закрытий** (FnOnce trait unique-type) — Rust-level
  оптимизация; Plan 11 Ф.x follow-up.

## Связь с другими планами

- **Plan 95** ✅ — фундамент (DeclaredBody channel).
- **Plan 95.bis** ✅ — match-only методы (9/14).
- **Plan 98** ✅ — inference recursion (нужен для 99.1 Ф.2/Ф.3).
- **Plan 61** — `unwrap` Fail-handler (parallel, закроет 14-й метод).
- **Plan 78** — узкий пересмотр Ф.1; Plan 99 расширяет до 14/14
  builtin Option/Result; реестр C-routing → только `unwrap`.
- **D75** — closure ABI; Plan 99 берёт минимум, не закрывает целиком.
- **Plan 48** — фундамент `method_extra_subst` (helper extract в 99.1).

## Riski + mitigation

1. **C-redefinition collision** при удалении inline emit ≠ atomic
   с миграцией — mitigation: 99.3 = atomic per-method (core.nv +
   inline-delete в одном коммите).
2. **Bare ctor backward-compat** (99.2) — старый код вне Option/Result
   методов не должен сломаться; mitigation: change только когда
   `current_fn_return_ty` matches Option/Result; иначе legacy path.
3. **Method-level mono name collisions** (99.1) — `Option[int].map[str]`
   vs `Option[int].map[int]` — без U-suffix collide; mitigation:
   `compute_mono_name` правильно extends.
4. **fn_param_sigs scoping** — `f` в body должен быть зарегистрирован
   на entrance в body и снят на exit; `emit_monomorphized_method`
   уже корректно save/restore'ит (Plan 48). Mitigation: verification
   tests.

## Реалистичная оценка

- Plan 99.1: 1.5 dev-day (helper extract + 2 dispatch integrations
  + register_novaXX_decl + mono_name + infer_expr_c_type fix).
- Plan 99.2: 0.5 dev-day (4 контролируемых изменения + small tests).
- Plan 99.3: 0.5 dev-day (6 atomic per-method shipping).
- Plan 99.4: 0.5 dev-day (comprehensive tests + spec + docs).

**Total: ~3 dev-day** (с гранулярной регрессией на каждом шаге).

> Прежняя оценка 3.5–4 dev-day (Ф.0 re-scope) была pessimistic —
> clean-slate audit показал, что бо́льшая часть инфры уже есть
> (closure invoke + fn_param_sigs + method_extra_subst + contextual
> return-type + register_novaopt_decl). Реальная работа = integration
> + 4 точечных fix + atomic migration.
