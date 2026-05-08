# План 12 — builtins.nv-driven external dispatch

**Статус:** ⏳ pending (после Plan 04 Этап 6).
**Связь:** [D82](../../spec/decisions/08-runtime.md#d82) (extended
2026-05-08), Q-codegen-builtins-cleanup
([open-questions.md](../../spec/open-questions.md#q-codegen-builtins-cleanup)),
Plan 04 Этап 6.

## Цель

Удалить hard-coded таблицы external-функций из codegen'а. После
плана единственный источник истины для StringBuilder / WriteBuffer /
ReadBuffer / `str.from(char)` — `std/runtime/builtins.nv`. Codegen
читает декларации из AST builtins.nv и автоматически выводит C-name
+ C-prototype через mangling и Nova→C type mapping.

Расхождение между .nv-декларацией и runtime-реализацией ловится
**линкером** (undefined reference / type mismatch).

## Не цели

- **Mangling/type-mapping для user-defined типов и обычных функций**
  — не трогаем. Сейчас они тоже hard-coded местами в `type_ref_to_c`
  и `mangle_fn`, но это отдельный refactor.
- **Поддержка `external fn` за пределами `std.runtime.*`** — D82
  whitelist сохраняется.
- **Удаление Buffer** — это Plan 04 Этап 6, идёт **до** Plan 12.

## Текущее состояние (2026-05-08)

Hard-coded в `compiler-codegen/src/codegen/emit_c.rs`:

| Что | Локация |
|---|---|
| `record_schemas.insert("StringBuilder", ...)` (empty schema, opaque) | строки 411-413 |
| `record_schemas.insert("WriteBuffer", ...)` | 411-413 |
| `record_schemas.insert("ReadBuffer", ...)` | 411-413 |
| Method dispatch: `StringBuilder` (len/capacity/clone/into/append) | 4850-4877 |
| Method dispatch: `WriteBuffer` (len/capacity/clone/into/write_*) | 4879-4900 |
| Method dispatch: `ReadBuffer` (position/remaining/.../read_*/try_read_*) | 4902-4928 |
| Static-форма (`Type.factory(...)`) для всех трёх типов | 4948-5023 |
| Mangling instance: `format!("Nova_{}_method_{}", ...)` | 548, 658, 2830-2831 |
| Mangling static: `format!("Nova_{}_static_{}", ...)` | 5208 и др. |
| Type mapping `type_ref_to_c` | 886-985 |
| Парсинг builtins.nv | парсится, но `FnBody::External => {}` игнорируется codegen'ом (строки 1935, 2987, 3218) |

## Архитектура целевого решения

```
            ┌──────────────────────────┐
            │ std/runtime/builtins.nv  │  ← single source of truth
            └────────────┬─────────────┘
                         │ parse (existing parser)
                         ▼
            ┌──────────────────────────┐
            │ AST: ExternalFnDecl[]    │  ← name, receiver, params, return, effects
            └────────────┬─────────────┘
                         │ codegen passes:
                         ▼
            ┌──────────────────────────┐
            │ ExternalRegistry         │  ← keyed by (recv_type, fn_name, arity_kind)
            │  — name + C-name         │
            │  — params: Vec<C-type>   │
            │  — return: C-type        │
            │  — receiver: by-ptr/none │
            └────────────┬─────────────┘
                         │ used at:
              ┌──────────┴──────────┐
              ▼                     ▼
    emit C-prototype     emit_call resolves
    в header             dispatch через registry
                         (вместо hard-coded match)
```

`ExternalRegistry` строится **один раз** при загрузке модуля
`std.runtime.builtins`. После — только читается.

## Этапы

### Ф.1 — Сбор external деклараций из AST (~1-2ч)

1. Добавить `external_decls: Vec<ExternalFnDecl>` в codegen-state
   (или метод `collect_external_fns(&Module) -> Vec<...>`).
2. Walk module AST. Для каждого `FnDecl` где `body == FnBody::External`
   и `is_export == true` (D82 whitelist уже валидирован types/mod.rs):
   - Извлечь имя, receiver-type (если есть), `is_mut_receiver`,
     params (имена + типы), return-type, effects (для
     `Fail[ReadBufferError]`).
   - Сложить в `external_decls`.
3. Для каждой декларации применить mangling rules → `c_name: String`:
   - static: `Nova_<RecvType>_static_<fn_name>`
   - instance: `Nova_<RecvType>_method_<fn_name>`
   - free: `Nova_<module_path>_<fn_name>` (для `str.from(char)` —
     спец-case, см. Ф.4)
4. Sanity check: дубликаты mangled name? Сейчас разрешено через
   overload registry (Plan 11), нужно использовать
   parameter-type-mangling extension (Plan 11 Ф.3) при коллизии.

**Тест:** загрузить builtins.nv, распечатать registry, сравнить с
hand-rolled таблицей в emit_c.rs (текущей). Должно совпадать 1:1
после миграции — это проверка корректности парсинга.

### Ф.2 — Применить registry для record_schemas (~30мин)

В `emit_module` (строки 411-413):
```rust
// БЫЛО:
record_schemas.insert("StringBuilder".to_string(), HashMap::new());
record_schemas.insert("WriteBuffer".to_string(), HashMap::new());
record_schemas.insert("ReadBuffer".to_string(), HashMap::new());

// СТАЛО:
for decl in &external_decls {
    if let Some(recv_ty) = &decl.receiver_type {
        record_schemas.entry(recv_ty.clone())
            .or_insert_with(HashMap::new);  // opaque, schema empty
    }
}
```

Опаковые типы (StringBuilder/WriteBuffer/ReadBuffer) автоматически
регистрируются. Buffer уже удалён (Plan 04 Этап 6).

### Ф.3 — Применить registry для emit_call dispatch (~2-3ч)

Это **главный refactor**. Hard-coded match'и (строки 4850-4928,
4948-5023) заменяются на lookup в `ExternalRegistry`.

**До:**
```rust
match (recv_ty, method_name) {
    ("StringBuilder", "append") => { /* hand-rolled emit */ }
    ("StringBuilder", "len")    => { /* hand-rolled emit */ }
    ...
}
```

**После:**
```rust
if let Some(decl) = external_registry.lookup(recv_ty, method_name, &arg_types) {
    emit_external_call(&decl, &args, &mut output);
    return;
}
// fallback на user-defined record методы (без изменений)
```

`emit_external_call` — общая функция, которая на основе
`decl.params: Vec<CType>` + `decl.return_ty: CType` генерирует:
- C-syntax вызова: `Nova_X_method_y(receiver_ptr, arg0, arg1, ...)`
- Нужные приведения: `as nova_int` / `as uint32_t` etc.
- Обработку Fail-эффектов (sets `*err` параметр, как сейчас в
  ReadBuffer).

**Edge cases:**
- Overloaded методы (`StringBuilder.@append(s str)` vs `@append(c char)`):
  registry хранит обе декларации, lookup выбирает по типам args
  (Plan 11 Ф.3 mangling).
- `@into()` overloads (`StringBuilder @into() -> str` vs
  `WriteBuffer @into() -> []byte`): разные receiver'ы — нет
  коллизии.
- Static-форма (`StringBuilder.with_capacity(n)`): registry знает
  `is_static`, mangling `Nova_StringBuilder_static_with_capacity`.
- Self в return-type (`StringBuilder.new() -> Self`): уже работает
  через `current_receiver_type`; не требует изменений.
- Fail-effects: декларация несёт `effects: [Fail[ReadBufferError]]`,
  emit_external_call добавляет `*err` параметр и unwind.

### Ф.4 — Free-функции (`str.from(char)`) (~30мин)

`export external fn str.from(c char) -> Self` — это "method on `str`".
Mangling: `Nova_str_static_from`. Сейчас обрабатывается отдельным
branch в emit_call.

После Ф.3 — единый путь через registry. Receiver-type = `"str"`,
is_static = true.

**Особенность:** `str` не record, не opaque-тип, а primitive
(`nova_str` = struct в runtime). C-функция возвращает `nova_str`
по value, не `Nova_str*`. Убедиться что `type_ref_to_c("Self")` в
контексте receiver=str даёт `nova_str` корректно.

### Ф.5 — Удалить hard-coded таблицы (~30мин)

После того как Ф.1-Ф.4 работают и проходят тесты:

Удалить из `emit_c.rs`:
- Method dispatch для StringBuilder (4850-4877).
- Method dispatch для WriteBuffer (4879-4900).
- Method dispatch для ReadBuffer (4902-4928).
- Static-форма Buffer/StringBuilder/WriteBuffer/ReadBuffer
  (4948-5023; Buffer уже удалён в Plan 04 Этап 6).
- Hard-coded `record_schemas.insert(...)` для трёх типов
  (заменено в Ф.2).

Mangling helper'ы (`format!("Nova_{}_method_{}", ...)`) остаются —
теперь вызываются из registry-builder, а не из emit_call.

### Ф.6 — Compile-time gate против stale references (~1ч)

Сейчас если кто-то напишет в .nv:
```nova
let mut sb = StringBuilder.new()
sb.@no_such_method()
```
codegen эмитит `Nova_StringBuilder_method_no_such_method(...)` —
linker error. Это уже compile-time gate, но late-stage (linker).

**Earlier check:** type-checker должен валидировать что метод
существует в `external_registry` для своего receiver'а. Если
StringBuilder не имеет метода `no_such_method` — ошибка на
type-checking, не на линковке.

Реализация: types/mod.rs при resolve method-call'а проверяет
`external_registry.has_method(recv_ty, method_name, &arg_types)`.
Сейчас типчекер уже знает методы через AST — добавляется только
проверка для receiver-types которые не имеют user-defined record
(opaque types). Раньше эту дыру закрывал hard-coded match в
codegen — теперь нужно явно.

**Тест:** добавить failing test в `nova_tests/errors/`:
```nova
test "StringBuilder unknown method" {
    let mut sb = StringBuilder.new()
    sb.unknown_method()  // expected error: no method `unknown_method` on StringBuilder
}
```

### Ф.7 — Документация и discussion-log (~30мин)

1. `docs/project-creation.txt` — описать что builtins.nv теперь
   driver, hard-coded таблицы удалены.
2. `docs/simplifications.md` — записать как simplification (1 source
   вместо 2).
3. `nova-lang-private/discussion-log.md` — резюме эволюции D82 →
   Plan 12.
4. В `compiler-codegen/README.md` (если есть) — раздел "Adding new
   external fn": один edit в builtins.nv + импл в `nova_rt/`, codegen
   не трогается.

## Тесты

- **Все существующие тесты должны продолжать проходить** (это refactor,
  не feature). 42/42 codegen pass rate сохраняется.
- **Новый negative test** (Ф.6): unknown method on opaque type —
  type-error, не linker-error.
- **Sanity test:** добавить в builtins.nv новую external fn (например
  `WriteBuffer mut @write_u128_le(v u128) -> ()`), реализовать в
  runtime — должна работать без правки emit_c.rs. Это **acceptance
  criterion** Plan'а 12.

## Зависимости

- ✅ Plan 04 Этапы 1-5 (типы StringBuilder/WriteBuffer/ReadBuffer
  существуют в runtime).
- ⏳ Plan 04 Этап 6 (Buffer удалён) — Plan 12 идёт **после**.
- Parsing `external fn` — уже работает (lexer/parser/types подтверждены
  в инвентаре).

## Риски

1. **Overload-collision с user-defined methods.** Если кто-то напишет
   `fn StringBuilder mut @custom() -> ()` (не external) — должен ли
   это работать? D26 говорит StringBuilder — built-in opaque, нельзя
   расширять методами в user-коде. Type-checker должен запретить
   `fn <opaque-type> @<method>` за пределами `std.runtime.*`.
   Проверить, что это уже валидируется (если нет — добавить).
2. **Bootstrap circular.** Codegen зависит от builtins.nv → builtins.nv
   парсится codegen'ом. Но: парсинг — это lexer+parser+types, не
   codegen. Codegen только **читает** AST, что приходит из types.
   Цикла нет; порядок: types module процессит builtins.nv первым (он
   зависит только от prelude), затем все остальные модули видят
   external_registry.
3. **Performance.** Lookup в HashMap на каждый method call vs
   hard-coded match. Insignificant: registry небольшая (~70 entries),
   match'и тоже линейные. Не оптимизируем.

## Acceptance criteria

- [ ] Ф.1: registry строится из builtins.nv AST, печатается, совпадает
      с hand-rolled таблицей.
- [ ] Ф.2: record_schemas наполняется из registry.
- [ ] Ф.3: emit_call использует registry для StringBuilder/WriteBuffer/
      ReadBuffer; все 42/42 codegen теста проходят.
- [ ] Ф.4: `str.from(char)` работает через registry.
- [ ] Ф.5: hard-coded таблицы удалены из emit_c.rs (4850-5023);
      diff в LoC негативный на ~200 строк.
- [ ] Ф.6: type-checker отвергает unknown method на opaque-типе.
- [ ] Ф.7: docs обновлены.
- [ ] **Sanity:** добавление новой `external fn` в builtins.nv +
      runtime-impl работает без правки Rust-codegen'а.

## Open questions внутри плана

1. **Effect signatures в registry.** `Fail[ReadBufferError]` нужно
   сохранять как часть декларации, чтобы emit_external_call знал что
   добавить `*err` параметр. Простая реализация: `effects: Vec<EffectId>`
   в `ExternalFnDecl`. Уже есть в AST.
2. **Generic external fns.** Сейчас не нужны (все external —
   monomorphic). Если когда-то понадобится `external fn collect[T]()`
   — refactor. Не делаем.
