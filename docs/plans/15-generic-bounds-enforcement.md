# План 15: Generic bounds enforcement (D72)

**Статус:** активный, в работе с 2026-05-09.
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

### Ф.4 — Тесты

`nova_tests/types/generic_bounds.nv` (новый):

- `dedup[Hashable]` на типе с/без `@hash()` + `@eq()`.
- Multi-bound: `[T Hashable, K Ord]`.
- Forward dependency: `[K Hashable, V From[K]]`.
- Anonymous protocol bound: `[T protocol { @lt(o Self) -> bool }]`
  (если spec позволяет inline).
- Negative tests (manual через `check`-mode): ожидаемый compile-error
  при несоответствии.

**Объём:** ~10 тестов.

### Ф.5 — Spec-уточнение если нужно

После реализации могут всплыть углы (multi-arity overload методов,
default-методы protocol'а, parameterized protocol bounds). Записать в
spec / open-questions если найдём.

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
