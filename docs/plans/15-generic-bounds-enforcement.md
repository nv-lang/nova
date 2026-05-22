# План 15: Generic bounds enforcement (D72)

**Статус:** ✅ ЗАКРЫТ (2026-05-11). Все фазы Ф.1-Ф.5 реализованы; Ф.4
тесты — 6 позитивных + 3 негативных покрывают satisfied bounds,
multi-bound, mixed, forward-dependency и compile-error на use-site при
несоответствии (включая D53 effect-as-bound rejection).
**Дата создания:** 2026-05-08.
**Зависимости:** [D72](../../spec/decisions/02-types.md#d72) уже описывает
`[T Protocol]` синтаксис.

> **Reality-check 2026-05-09:** изначальный план утверждал что «парсер
> принимает `[T Hashable]` синтаксис, type-checker не проверяет». На
> практике **парсер не принимает** этот синтаксис вообще
> (`error: expected ']', got identifier`). `FnDecl.generics` хранится
> как `Vec<String>` без места под bound. Поэтому первая фаза — **AST +
> parser**, а enforcement — поверх. Объём вырос с 250 до ~500 строк.

---

## Проблема

[D72](../../spec/decisions/02-types.md#d72) формализует generic bounds
через protocol:

```nova
fn dedup[T Hashable](xs []T) -> []T => ...
fn fold[T, Acc Numeric](xs Iter[T], init Acc) -> Acc => ...
```

Сейчас (post-Plan 14):
- Парсер **отвергает** этот синтаксис (`expected ']', got identifier`).
- `FnDecl.generics` / `TypeDecl.generics` — `Vec<String>` (только имена).
- Type-checker bounds **не проверяет на use-site**.

Результат:

```nova
type User { id u64, name str }   // у User нет @hash() / @eq()

let xs []User = [...]
let unique = dedup(xs)            // ✅ компилируется
                                  // ❌ позже падает с непонятным error
                                  //    "Nova_User has no method @hash"
```

Это — **direct AI-first hit**. Spec говорит «structural по умолчанию,
bounds — контракт», LLM полагается на сигнатуру; компилятор молчит до
самой глубокой точки expansion'а. Hourglass debugging.

---

## Что нужно

На use-site generic-вызова (`dedup[User](xs)` или после inference) —
проверить, что concrete type `T` структурно удовлетворяет bound'у:

1. Извлечь `Protocol` из bound'а (например `Hashable`).
2. Из protocol-декларации получить список required-методов с
   сигнатурами.
3. Для concrete type'а `T` — проверить наличие каждого метода с
   совпадающей сигнатурой (после substitution `Self → T`).
4. При отсутствии — **внятная ошибка на месте вызова**:
   ```
   error E0143: type `User` does not satisfy `Hashable` bound
     in call to `dedup[T Hashable]` at src/main.nv:42

     `Hashable` requires:
       hash() -> u64
       eq(other Self) -> bool

     `User` is missing: hash(), eq()

     fix: add `fn User @hash() -> u64 => ...`
          and `fn User @eq(other User) -> bool => ...`
          to your User type. See spec/decisions/02-types.md#d72.
   ```

---

## Фазы

### Ф.1 — AST + parser: bound на generic-параметре

**Корень:** `FnDecl.generics: Vec<String>`, `TypeDecl.generics:
Vec<String>`, `Receiver.generics: Vec<TypeRef>` — нет места под bound.

**AST:**
```rust
pub struct GenericParam {
    pub name: String,
    pub bound: Option<TypeRef>,    // None для `[T]`, Some(Hashable) для `[T Hashable]`
    pub span: Span,
}
```

`generics: Vec<String>` → `generics: Vec<GenericParam>` в:
- `FnDecl`
- `TypeDecl`
- effect/protocol method-signatures (если есть generics)

**Parser:** `parse_generic_param` — читает `Ident` (имя), затем optional
type (bound). Mirroring `parse_param` (`name type` правило). Forward-
references запрещены ([D72](../../spec/decisions/02-types.md#d72)) —
имя в bound'е должно быть в текущем списке слева или в окружающем
type-context.

**Mechanical refactor**: все consumers `Vec<String>` (codegen
`generic_fns`, monomorphization, generic_fn_tuple_arity, etc.)
адаптируются на `.iter().map(|g| &g.name)` или `.name`.

**Объём:** ~150 строк AST/parser + ~30 sites mechanical.

### Ф.2 — ProtocolSpec registry в type-checker

В type-checker'е protocol-методы парсятся как `TypeDeclKind::Effect`
(D62 — protocol/effect единая форма) но не агрегируются в
структуру доступную из generic-resolver'а.

- Добавить `protocol_specs: HashMap<String, ProtocolSpec>` в
  `ModuleEnv` или отдельный checker-state.
- `ProtocolSpec { methods: Vec<MethodSig> }` где `MethodSig` —
  `{ name, params: Vec<Ty>, return_ty: Ty }` после `Self → T`
  substitution готов.

**Объём:** ~80 строк.

### Ф.3 — Use-site проверка bound'ов + R5.3 diagnostic

При резолве вызова generic-функции `f[T1, T2, ...]` (или после
inference):

1. Для каждого bound'а `Ti Protocol` — взять `protocol_specs[Protocol]`.
2. Для concrete `Ti` — собрать `method_overloads[(Ti, _)]` (codegen
   registry уже знает методы).
3. Сверить required methods с available — match по name + arity +
   substituted signature (`Self → Ti`).
4. На mismatch — структурированный diagnostic
   ([R5.3](../../spec/revolutionary.md#r5-3)):
   ```
   error E0143: type `User` does not satisfy `Hashable` bound
     in call to `dedup[T Hashable]` at src/main.nv:42

     `Hashable` requires:
       hash() -> u64
       eq(other Self) -> bool

     `User` is missing: hash(), eq()

     fix: add `fn User @hash() -> u64 => ...`
          and `fn User @eq(other User) -> bool => ...`
          See spec/decisions/02-types.md#d72.
   ```

**Объём:** ~150 строк + diagnostic templates.

### Ф.4 — Тесты ✅ ЗАКРЫТ (2026-05-11)

`nova_tests/types/generic_bounds.nv` — 6 позитивных тестов:

- bound satisfied — `GbUser` имеет `@hash()` + `@eq()` → OK
- multi-bound — `[A GbHashable, B GbHashable]` оба satisfied
- no-bound generic — `[T]` без bound'а компилируется как раньше
- mixed bound — `[K GbHashable, V]` один с bound'ом, другой без
- different bound — `GbCountable` vs `GbHashable` на одном типе
- forward-dependency — `[K GbHashable, V GbFromKey[K]]` парсер
  принимает (через `parse_type`); чекер для параметризованных bound'ов
  permissive (early-return для non-single-name path) — это закрывается
  отдельной задачей.

`nova_tests/negative_capability/` — 3 negative-теста (через D89
`EXPECT_COMPILE_ERROR` маркер):

- `bound_not_satisfied_rejected.nv` — тип без `@hash()`/`@eq()`
  передаётся в `[T BnsHashable]` → «type X does not satisfy P bound».
- `bound_missing_method_rejected.nv` — метод с тем же именем но другой
  арностью (`count(int)` vs `count()`) → «does not satisfy» (BoundCtx
  матчит по name + arity; полная sig-сверка с `Self → T` — будущая
  фаза).
- `bound_effect_not_protocol_rejected.nv` — effect-kind тип
  (`type BefLogger effect { ... }`) используется как bound `[T BefLogger]`
  → R5.3 «type X is an effect, not a protocol» (D53 strict, Ф.5).

**Не покрыто (не поддерживается парсером bootstrap):**

- ~~Anonymous protocol bound `[T protocol { @lt(o Self) -> bool }]` —
  `parse_type` не принимает `protocol` keyword в позиции типа (matches
  только Named/Array/Tuple/Func). Отложено до отдельной задачи
  (требует расширения грамматики типов; D53 §628 анонимные
  protocol-литералы).~~
- **CLOSED Plan 97 Ф.2 (2026-05-22, D142).** `parse_type` теперь
  принимает `protocol { method-sig* }` как 4-ю форму типа. AST
  получил `TypeRef::Protocol { methods, span }`,
  `check_satisfaction_against_methods` обобщён на named + anonymous.
  Тесты: `nova_tests/plan97/pos_anon_protocol_bound.nv` (pos),
  `neg_anon_protocol_missing_method.nv` (neg).

**Итого:** 6 позитивных + 3 негативных = 9 тестов покрытия Plan 15.

### Ф.5 — D53 strict-mode: split Protocol vs Effect ✅ ЗАКРЫТ (2026-05-09)

После Ф.1-Ф.4 BoundCtx был permissive — принимал любой method-bag
тип как potential bound, что нарушало D72. Ф.5 закрыла эту дыру
через split AST variants.

**Реализация:**

- AST: `TypeDeclKind::Effect(...)` остался для `effect`-keyword;
  новый `TypeDeclKind::Protocol(Vec<EffectMethod>)` для
  `protocol`-keyword.
- Parser (`parse_type_decl`): split arm `KwEffect | KwProtocol` в
  две отдельные ветки.
- Codegen (`emit_type_decl`): Effect — emit vtable как было; Protocol
  — пропускаем эмиссию (compile-time-only). Бонус: попутно фиксит
  pre-existing **Self-bug** — `Self` в protocol-методе раньше ломал
  vtable (искал `Nova_Self*`); без vtable-emit'а type_ref_to_c для
  protocol-методов не вызывается.
- BoundCtx: новый `effect_decls: HashMap<String, &TypeDecl>` —
  для дифференциированного error-сообщения. `protocol_specs`
  регистрирует **только** `TypeDeclKind::Protocol`. В
  `check_satisfaction` — bound-name в `effect_decls` → R5.3 error.
- Lints (`collect_protocol_names`): scan через
  `TypeDeclKind::Protocol(_)`.
- `walk_module` — теперь обходит и `Item::Test(_)`.

**Тесты:**
- `nova_tests/types/generic_bounds.nv` — bonus: восстановлен `eq(other
  Self) -> bool` в `GbHashable`.
- Negative: ручная проверка `[T MyEffect]` → R5.3 error.

**Объём:** ~70 строк.

**Не покрыто:** анонимные protocol-литералы в позиции типа (D53 §628)
— отдельная задача.

---

## Что НЕ делаем в этом плане

- **`From[T]`/`Into[T]` bound enforcement** — это [Plan 08 Ф.6](08-from-into-conversions.md),
  отложенный. Параллельный план; механизм похож, но имеет свою
  специфику auto-derive (D77).
- **Default-методы protocol'а** — пока spec их не разрешает; если
  введём, доработать.
- **Bound на ассоциированном типе** (`fn f[T Iter[E]](xs T) ...`) —
  открытый вопрос, не сейчас.

---

## Оценка

- Ф.1 (AST + parser + mechanical refactor): ~150 + ~30 sites = ~250 строк.
- Ф.2 (ProtocolSpec registry): ~80 строк.
- Ф.3 (use-site enforcement + diagnostic): ~150 строк.
- Ф.4 (тесты): ~10 файлов.
- **Итого: ~500 строк**, 3-4 дня.

(Изначальная оценка 250 строк недооценила Ф.1 — план полагал что парсер
уже принимает синтаксис, что не так.)

---

## Связь с другими планами

- [Plan 08 Ф.6](08-from-into-conversions.md) — From/Into bounds, отложен.
- [Plan 11](11-method-values-and-overload.md) — overload по типу
  использует похожий механизм structural matching.
- [Plan 14](14-stdlib-codegen-gaps.md) — параллельный план, не зависит.

---

## Ссылки

- [spec/decisions/02-types.md → D72](../../spec/decisions/02-types.md#d72) —
  generic bounds.
- [spec/decisions/02-types.md → D53](../../spec/decisions/02-types.md#d53) —
  protocol declarations.
- [spec/revolutionary.md → R5.3](../../spec/revolutionary.md#r5-3) —
  AI-first error format.
- `compiler-codegen/src/types/` — type-checker source.
