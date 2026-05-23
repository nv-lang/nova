# Plan 98 — Free-fn generic type-param inference на generic-типах (Option/Result/user-generics)

> **Статус:** ✅ **ЗАКРЫТ 2026-05-23** (worktree `nova-p98`, ветка
> `plan-98`). Ф.0–Ф.3 все выполнены. Подход A (метод `&self` + резолв
> через `novaopt_value_types`/`novares_value_types`/
> `generic_type_instance_info`). 16 call-sites обновлены
> (`Self:: → self.`). 5 фикстур в `nova_tests/plan98/` (4 pos + 1 neg)
> — PASS. Регрессия 10 крит. подпапок: 127/0. **Known limitation:**
> `[]Option[T]` / `[]Result[T,E]` не выводится (codegen эрейзит element
> type до inference — отдельный gap, не scope).
> **Приоритет:** P2 (UX-дыра: `fn check[T](o Option[T])` требует turbofish
> `check[int](a)` вместо естественного `check(a)`. Раздражает в каждом
> generic-helper'е; не блокер фич, но систематически портит читаемость
> и обнаруживается при каждом написании generic-helper'а).
> **Оценка:** ~1.5 dev-day (точечный fix infer_type_param_binding +
> рефакторинг 17 call-sites + тесты-фикстуры на каждый класс типа).
> **Зависимости:** Plan 95 ✅ (вскрыл gap в Ф.6); Plan 88 ✅
> (lineage — static-method-on-typevar mono-followup).
> **Источник:** Plan 95 Ф.6.1 — тест `option_in_generic_fn.nv` упал с
> «cannot infer type argument `T`» при `check_some(a)` где
> `a: Option[int]`, потребовался `check_some[int](a)`.

## Зачем

`infer_type_param_binding` ([emit_c.rs:8480](../../compiler-codegen/src/codegen/emit_c.rs#L8480))
умеет извлекать `T` из параметра только в **двух** случаях:

```rust
fn infer_type_param_binding(param_ty, concrete_c, subst) {
    match param_ty {
        TypeRef::Named { generics, .. } if generics.is_empty() => { ... }  // bare T
        TypeRef::Array(inner, _) => { ... }                                // []T
        _ => {}                                                            // ВСЁ ОСТАЛЬНОЕ
    }
}
```

**Всё остальное молча игнорируется** — и `Option[T]`, и `Result[T,E]`,
и пользовательские `Box[T]`/`HashMap[K,V]`/etc. Следствие: любой
generic-helper, принимающий generic-параметризованный тип, **обязан**
вызываться через turbofish:

```nova
fn check[T](o Option[T]) -> bool => o.is_some()

// Сейчас:
check[int](a)        // turbofish обязателен
// Хочется (Rust/Go-style):
check(a)             // вывести T = int из Option[int]
```

В Plan 95 Ф.6 это всплыло на тесте `option_in_generic_fn.nv` —
turbofish работает, но не должен быть обязательным для очевидно
выводимого case'а.

## Сравнение с Go / Rust / TS

| Язык | Поведение |
|---|---|
| **Rust** | `fn check<T>(o: Option<T>) -> bool { o.is_some() }` — `check(some_int_opt)` выводит `T = i32`. Стандартная unification. |
| **Go** | `func check[T any](o Option[T]) bool { ... }` (с 1.18+) — `check(someIntOpt)` выводит `T = int`. Type-inference engine ↔ ParamPair. |
| **TS** | `function check<T>(o: T | undefined): boolean { ... }` — `check(maybe)` выводит `T` структурно. |
| **Nova (сейчас)** | Только `T` и `[]T` выводятся. Любой другой generic-тип в параметре → **обязательный turbofish**. **Хуже Rust/Go/TS** в типичном случае. |
| **Nova (цель)** | Парfunc-параметра с любым generic-типом (Option/Result/user-generics) выводит type-params структурно через unification с C-типом аргумента. |

## Привязка к коду (сверено 2026-05-23, worktree `nova-p95`)

| # | Точка | Файл:строка | Роль |
|---|---|---|---|
| 1 | `infer_type_param_binding` | `emit_c.rs:8480` | Ассоциированная функция (`fn ...` без `&self`); 2 case'а, всё остальное no-op. |
| 2 | Call-sites | `emit_c.rs:8171, 8225, 8231, 8256, 8307, 8324, 8345, 8440, 8503, 15750, 15793, 20702, 20753, 22140, 22164, 22177` | 16 уникальных мест (drain mono-worklist, register_mono_method_instance, generic-fn dispatch, closure-param inference и т.п.). |
| 3 | Реверс-резолверы | `emit_c.rs:novaopt_value_types, novares_ok_err, generic_type_instance_info` | Map'ы sanitized→real (Option), C-type→(ok,err) (Result), mangled→base+args (user-generics). Доступны только на `&self`. |

## Архитектурное решение (Ф.0 утвердит)

Две опции:

- **(A) `infer_type_param_binding` → метод `&self`.** Резолвит реальные
  C-типы через `novaopt_value_types` / `novares_ok_err` /
  `generic_type_instance_info`. Полная корректность (включая
  не-примитивные T). **Минус:** 16 правок `Self:: → self.`, риск
  borrow-конфликтов в 1-2 местах (subst заполняется внутри loop'а на
  параметрах — `&self` shared borrow совместим, но нужно проверить).
- **(B) Оставить ассоциированной + sanitized-форма для Option/Result.**
  Bind `T → sanitized C-name`. Для примитивов (`nova_int`/`str`/...)
  sanitized == real → корректно. Для не-примитивов (`Nova_Foo_p`
  вместо `Nova_Foo*`) — **некорректно** если `T` используется в теле
  метода как value-тип. Для `is_some`/`is_none`/`is_ok`/`is_err`
  (Plan 95) — безопасно (T не используется), но это case-by-case
  костыль.

**Рекомендация — A.** Подход B — частичная заплатка с тихим
miscompilation-риском для будущих случаев (любой Nova-body метод
Option/Result/user-generic, использующий T в теле через value-тип).
Plan 79 «no silent fallback» этого не допускает.

User-generic case (`Box[T]`, `HashMap[K,V]`) — отдельный вход в той же
функции, тоже нуждается в реверс-резолве через
`generic_type_instance_info`. Естественно делать вместе с Option/Result —
один pass, один метод.

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит + decision A/B (~0.2 д) — GATE

- **Ф.0.1** Сверить карту 16 call-sites; проверить borrow-совместимость
  `&self` + mutable `subst` параметра во всех вызовах.
- **Ф.0.2 — decision point A vs B.** Утвердить **A** (или
  зафиксировать обоснование B). Probe: точечный prototype с `&self`
  +  Option-case → запустить `option_in_generic_fn.nv` **без**
  turbofish и убедиться, что компилируется.
- **Ф.0.3 — decision point: scope user-generics.** Включить
  ли `Box[T]`/`HashMap[K,V]` в эту же итерацию (рекомендуется — тот же
  механизм через `generic_type_instance_info`)? Если выявит
  непропорциональную сложность — re-scope: оставить только
  Option/Result в Ф.1, user-generics — Plan 98.bis.

### Ф.1 — Реализация (~0.7 д)

- **Ф.1.1** `infer_type_param_binding` → `fn infer_type_param_binding(
  &self, param_ty, concrete_c, subst)`. 16 call-sites: `Self::` →
  `self.`. Cargo-check, фикс возможных borrow-conflict'ов.
- **Ф.1.2** Добавить case `Named { path == ["Option"], generics: [T] }`:
  `strip_prefix("NovaOpt_") → sanitized → novaopt_value_types[sani]`
  → real T → recurse.
- **Ф.1.3** Case `Named { path == ["Result"], generics: [T, E] }`:
  `novares_ok_err(concrete_c) → (ok, err)` → recurse на T и E.
- **Ф.1.4** Case user-generic `Named { path: [base], generics: gs }`
  где `base ∈ generic_type_templates`: parse mangled `concrete_c` →
  base + type_args (через `generic_type_instance_info` или
  парсинг `Nova_<base>____<arg1>__<arg2>__...*`) → recurse на каждом
  generic.

### Ф.2 — Тесты позитив + негатив (~0.3 д)

- **Ф.2.1** `nova_tests/plan98/` позитив:
  - `fn check[T](o Option[T]) -> bool => o.is_some()` + `check(a)` без
    turbofish для int/str/char/user-record.
  - Аналог для Result, для user-generic'а (`Box[T]`,
    `HashMap[K,V]`).
  - Вложенные `Option[Option[T]]`, `Result[Option[T], E]`.
- **Ф.2.2** Негатив (`EXPECT_COMPILE_ERROR`):
  - Конфликтующие bindings: `fn f[T](a T, b Option[T])` с
    `a: int`, `b: Option[str]` → loud error «cannot unify T» (а не
    silent first-binding-wins).
  - Полностью неразрешимый T (только в return type) → loud error
    с подсказкой использовать turbofish (sanity — текущее поведение).
- **Ф.2.3** Регресс: `option_in_generic_fn.nv` (Plan 95) — переписать
  без turbofish, должно работать; turbofish-форма остаётся valid
  (back-compat). Полный `nova test` — 0 новых FAIL.

### Ф.3 — Spec / docs (~0.1 д)

- **Ф.3.1** D-блок: явно зафиксировать contract «type-param inference
  recurses через generic-параметризованные типы (Option/Result/user-
  generics); turbofish — fallback для неразрешимых case'ов».
- **Ф.3.2** `docs/simplifications.md` — отметить устранение skill-floor
  для generic-helper'ов; обновить tutorial-фрагменты.
- **Ф.3.3** Plan 88 lineage-note: дописать, что P98 закрыл свободно-
  функциональный аналог его static-method-on-typevar fix.
- **Ф.3.4** `docs/plans/README.md` + `project-creation.txt` +
  `nova-private/discussion-log.md`.

## Acceptance criteria

- [ ] **Ф.0**: подход (A рекомендован) утверждён; probe
      `option_in_generic_fn` без turbofish компилируется.
- [ ] `infer_type_param_binding` — метод `&self`, все 16 call-site'ов
      обновлены.
- [ ] `Option[T]`, `Result[T,E]`, user-generic'и (`Box[T]`/`HashMap`/...)
      в параметре free-fn выводят type-params без turbofish.
- [ ] Конфликтующие bindings → loud error (не silent first-wins).
- [ ] Тесты `nova_tests/plan98/` позитив + негатив; регресс
      (`option_in_generic_fn`) переписан без turbofish.
- [ ] Полный `nova test` — 0 новых FAIL.

## Non-scope

- **Method-level generics inference** (`fn obj.method[U](...)` где `U`
  нужно вывести из args) — другой code-path
  (`emit_c.rs:15460+ method_extra_subst`), уже работает (Plan 48).
  Plan 98 ограничен **free-fn** случаем.
- **Static-method-on-typevar** (`T.from(x)`) — закрыто Plan 88.
- **Higher-kinded inference** (`F[A]` → `F` как мета-генерик) — нет в
  Nova и не планируется.
- **Inference из return type** (Hindley-Milner full) — Nova держит
  локально-направленную inference (от args в обе стороны от param),
  глобальный backflow не входит. `check(a) -> T` где `T` только в
  return — turbofish остаётся обязательным (документируется как
  by-design).

## Связь с другими планами

- **Plan 95** — обнаружил gap; turbofish-workaround в Ф.6.1 тестах.
  После Plan 98 рекомендуется переписать `option_in_generic_fn.nv` без
  turbofish.
- **Plan 88** — родственная инициатива (static-method-on-typevar);
  Plan 98 — free-fn-параметр-аналог. Оба — mono-followup line.
- **Plan 48 / Plan 54 / Plan 63** — mono pipeline, на котором стоит
  Plan 98 (без них inference и эмиссия mono'd инстансов не работают).
