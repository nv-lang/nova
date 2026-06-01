# Plan 88 — generic static-method dispatch на type-параметре

> **Статус:** ✅ ЗАКРЫТ 2026-05-22 (Ф.0-Ф.4; ветка `plan-88`)
> **Приоритет:** P2 (idiom `T.from(v)` в generic-коде; пока обходится
> конверсией на call-site, но это spec-обещание D72)
> **Оценка:** ~1.5–2 dev-day (включая обязательный аудит Ф.0)
> **Зависимости:** Plan 48 (мономорфизация) ✅ partial; Plan 54 / 63
> (mono followups) ✅ — Plan 88 продолжает эту линию; Plan 15 (D72
> bound enforcement) ✅
> **Источник:** Plan 85.3 — маркер `[M-generic-static-method-on-typevar]`
> в `docs/simplifications.md`

## Зачем

Вызов **static-метода на type-параметре** внутри тела generic-функции
не мономорфизируется (перепроверено 2026-05-22):

```nova
fn wrap[T From[str]](s str) -> T => T.from(s)
ro n = wrap[Name]("alice")     // ← lld-link: undefined symbol nova_fn_T_from
```

`wrap` инстанцируется в `wrap____Nova_Name_p` (T→Name **в сигнатуре**),
но внутри тела `T.from(s)` эмитится как литеральный `nova_fn_T_from` —
`T` не подставляется в obj-позиции static-вызова → undefined symbol.
Аналогично `Result[T, E]` с type-параметром в return-позиции
generic-функции даёт unsubstituted `Nova_Result_...` (CC-FAIL,
`unknown type name`).

**Контраст:** instance-метод на type-параметре (`it.next()` для
`[T Iter[U]]`) мономорфизируется корректно (Plan 62
`protocol_param_generic_bound.nv`). Дыра — именно static-вызовы
`T.method(...)` и type-параметр в return-позиции.

**Это не «новая фича», а spec-долг.** Spec D72 приводит ровно этот
паттерн как канонический пример:

> `fn func[K, T From[K]](v K) -> T => T.from(v)` — spec/decisions/
> 02-types.md §D72, «Порядок параметров — слева направо».

Plan 88 = довести реализацию до того, что D72 уже обещает.

Линия mono-followup'ов: **Plan 48** (mono core) → **Plan 54**
(return-path) → **Plan 63** (cross-module dispatch) → **Plan 88**
(static-dispatch на typevar). Отмечалось как bootstrap-ограничение в
Plan 62.E (`nova_tests/plan62/tryfrom_tryinto_from_prelude.nv` —
bound-функции «не вызываются»).

## Сравнение с Go / Rust / TS

| Язык | Static-/assoc-метод type-параметра внутри generic |
|---|---|
| **Rust** | ✅ **полностью.** `fn wrap<T: From<S>>(s: S) -> T { T::from(s) }` — assoc-функция type-параметра через trait-bound + мономорфизация. Ядро Rust. |
| **Go** | ⚠️ **структурно нельзя.** Go-constraint'ы — интерфейсы (методы на *значениях*); у type-параметра нет «type-level функций». Конструктор передают отдельным параметром-функцией. |
| **TS** | ⚠️ **структурно нельзя.** Generics стираются; `T` — не значение, `T.from()` невозможно. Передают класс/factory. |
| **Nova (цель)** | ✅ **Rust-паритет** — `T.from(v)` через bound `[T From[K]]` + мономорфизация. |

Вывод: **планка — Rust.** Go и TS этого не умеют by-design (разные
модели generics). Nova по дизайну (D72 «universal через
мономорфизацию», Rust-grade) уже выбрала Rust-модель — Plan 88 лишь
доводит реализацию. Достижение Rust-паритета здесь **автоматически
ставит Nova выше Go и TS** по выразительности generic-кода.

## Scope

- `emit_c.rs` static-call resolution: при `obj = Ident(n)`, где `n` —
  type-параметр в активном mono-контексте (`current_type_subst`) —
  резолвить `n` в concrete Nova-тип и далее обычный static-dispatch.
- mono type-параметра в **return-позиции** generic-функции — в объёме,
  **необходимом для замыкания** static-dispatch: как минимум
  `Result[T, E]` и `Option[T]` (их возвращает `T.try_from`/`T.from`
  идиоматически). Без этого `T.try_from` нельзя объявить закрытым.

## Декомпозиция (фазы и шаги)

> **Привязка к коду** (из работы Plan 85.3, сверить при старте):
> static-call резолвится в `emit_c.rs` ~line 14987 — `recv_type_name`
> из `obj = Ident(n)`: ветка `method_overloads.keys().any(t==n)` →
> static, иначе → instance через `obj_ty`. Для type-параметра `n` ни
> одна ветка не срабатывает → fallback `nova_fn_<n>_<method>`.
> Mono-контекст: `current_type_subst: HashMap<typeparam → C-тип>`
> активен во время эмиссии мономорфизированной функции.

### Ф.0 — Аудит кластера mono-static-dispatch (~0.3 д) — GATE

Static-метод на typevar — почти наверняка не единственная дыра. План
обязан закрыть **семейство**, а не один симптом. Без упрощений: аудит
ищет в т.ч. **silent-wrong** случаи, не только loud CC-FAIL.

- **Ф.0.1** Probe static-методов на type-параметре в generic-теле:
  `T.from`, `T.try_from`, `T.new`, `T.with_capacity`, user `T.make`
  — временные фикстуры; зафиксировать симптом каждого
  (CC-FAIL / undefined-symbol / **silent-wrong**).
- **Ф.0.2** Probe type-параметра в return-позиции: `Result[T,E]`,
  `Option[T]`, `[]T`, generic record `Box[T]` — generic-функция,
  возвращающая такой тип с unresolved `T`.
- **Ф.0.3** Probe вложенности: `T.from` чей результат идёт в `Result`
  / в другой generic-вызов; `[U, T From[U]]` (двойной typevar).
- **Ф.0.4** Soundness-check: убедиться, что нет случая, где
  static-dispatch на typevar **молча** компилируется в неверный код
  (не link-error, а wrong-behavior) — такой случай повышается в
  приоритете.
- **Ф.0.5** Свести симптомы в таблицу «Кластер по итогам Ф.0» (ниже).
- **Ф.0.6** **Decision point:** что закрывает Plan 88, что (если
  есть несвязанное с static-dispatch — напр. чисто `[]T`-return)
  выносится в Plan 54-lineage. Финализировать объём Ф.1/Ф.2.

### Ф.1 — Static-call на typevar в mono-контексте (~0.5 д)

- **Ф.1.1** `emit_c.rs` static-call resolution (~14987): при
  `obj = Ident(n)`, если `n ∈ current_type_subst` — резолвить `n` →
  concrete Nova-тип (`nova_type_name_from_c(current_type_subst[n])`),
  далее обычный static-dispatch вместо fallback `nova_fn_<n>_<m>`.
- **Ф.1.2** Синхронно поправить `want_instance` (тот же блок): для
  резолвленного typevar это static-вызов (`want_instance = false`).
- **Ф.1.3** Покрыть **все** static-методы (`from`/`try_from`/`new`/
  `with_capacity`/user-defined), не только `from` — обобщённый путь,
  не спец-кейс под один метод.
- **Ф.1.4** Учесть overload (Plan 85.3 фикс `.into()`): если у
  резолвленного типа `from` перегружен — выбрать mangled-имя по типу
  аргумента (тот же `method_overloads`-механизм).
- **Ф.1.5** Targeted-verify: probe-фикстуры из Ф.0.1 → PASS.

### Ф.2 — Type-параметр в return-позиции (~0.5 д)

- **Ф.2.1** mono `Result[T, E]` / `Option[T]` в return-позиции
  инстанцируемой generic-функции — `T` подставляется в return-тип,
  C-имя резолвится в concrete (`Nova_Result____...`). **В scope
  безусловно** — без этого `T.try_from` (возвращает `Result[T,E]`)
  не закрывается, а это половина мотивации плана.
- **Ф.2.2** generic record `Box[T]` / `[]T` в return-позиции — **в
  scope, если** Ф.0 покажет, что это часть того же дефекта подстановки.
  Если Ф.0 покажет, что чистый `[]T`-return — несвязанное отдельное
  семейство (своя машинерия) — обоснованно вынести в Plan 54-lineage
  (это **граница задач**, а не упрощение: критерий — связан ли дефект
  с подстановкой typevar или это другой механизм; решение и
  обоснование фиксируются в «Кластер по итогам Ф.0»).
- **Ф.2.3** Targeted-verify: probe-фикстуры из Ф.0.2 → PASS.

### Ф.3 — Тесты (~0.3 д)

- **Ф.3.1** `nova_tests/plan88/` позитив — generic-функция с
  настоящим `T.from(...)` / `T.try_from(...)` / `T.new()` внутри
  тела, реально вызывается с turbofish и работает; несколько
  инстансов (`wrap[A]`, `wrap[B]`).
- **Ф.3.2** `nova_tests/plan88/` — D72-канонический пример
  `fn func[K, T From[K]](v K) -> T => T.from(v)` end-to-end.
- **Ф.3.3** `nova_tests/plan88/` негатив (EXPECT_COMPILE_ERROR) —
  bound-violation остаётся ошибкой (не сломать Plan 15).
- **Ф.3.4** Переписать `nova_tests/protocols/conversion/
  generic_from_bound.nv` + `generic_try_bound.nv` — с probe-формы
  (T как value-тип, конверсия на call-site) на прямой `T.from` /
  `T.try_from` внутри generic-тела; снять комментарии-ограничения и
  ссылки на `[M-generic-static-method-on-typevar]`.
- **Ф.3.5** Полный `nova test` — 0 новых FAIL.

### Ф.4 — Spec / docs (~0.1 д)

- **Ф.4.1** `docs/simplifications.md` —
  `[M-generic-static-method-on-typevar]` → ✅ ЗАКРЫТО; родственные
  маркеры, найденные в Ф.0, — обновить.
- **Ф.4.2** D72/D73 — аменд **только** если меняется наблюдаемая
  семантика; ожидаемо не требуется (фикс приводит реализацию в
  соответствие с уже существующим D72-примером `T.from(v)`). Если
  D72-пример был помечен как «недоступно в bootstrap» где-либо —
  снять пометку.
- **Ф.4.3** `docs/plans/README.md` — Plan 88 → статус-апдейт.
- **Ф.4.4** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Кластер по итогам Ф.0

Аудит проведён 2026-05-22 — 8 probe-фикстур (`_audit/*.nv`, временные),
прогон через `nova build`/`nova test`. Результаты:

| Probe | Случай | Симптом | Стадия | Решение |
|---|---|---|---|---|
| `pa_from` | `T.from(s)` в generic-теле | `lld-link: undefined symbol nova_fn_T_from` | **link** | Plan 88 Ф.1 |
| `pa_usermake` | user `T.make()` (protocol со static-методом) | `lld-link: undefined symbol nova_fn_T_make` | **link** | Plan 88 Ф.1 |
| `pa_tryfrom` | `T.try_from(n)` + return `Result[T,str]` | `unknown type name 'Nova_Result____Nova_Pos_p__nova_str'` (Result-имя падает первым, до link) | **codegen** | Plan 88 Ф.1 + Ф.2 |
| `pc_nested` | двойной typevar `[K, T From[K]]`, `T.from(v)` | `lld-link: undefined symbol nova_fn_T_from` | **link** | Plan 88 Ф.1 |
| `pb_result_ret` | `Result[T,E]` в return-позиции | `unknown type name 'Nova_Result____nova_int__nova_str'` (объявлен `NovaRes_nova_int_nova_str`) | **codegen** | Plan 88 Ф.2 |
| `pb_option_ret` | `Option[T]` в return-позиции | ✅ компилируется, линкуется, runtime-assert проходит | OK | — (уже работает) |
| `pb_arr_ret` | `[]T` в return-позиции | ✅ компилируется, линкуется, runtime-assert проходит | OK | — (уже работает) |
| `pb_box_ret` | generic record `Box[T]` в return-позиции | ✅ компилируется, линкуется, runtime-assert проходит | OK | — (уже работает) |

**Два дефекта:**

- **D1 — static-call на typevar.** `obj = Ident(T)`, где `T` —
  type-параметр активного mono-контекста: codegen эмитит литеральный
  `nova_fn_<T>_<method>` → undefined symbol на линковке. Затрагивает
  `T.from` / `T.try_from` / `T.make` (и любой static-метод). Корень —
  `recv_type_name` в `emit_c.rs` не резолвит `T ∈ current_type_subst`.
  → **Ф.1.**
- **D2 — `Result[T,E]` в return-позиции.** `T` подставляется корректно
  (C-имена содержат `nova_int`/`Nova_Pos_p`), но return-тип мангл'ится
  `Nova_Result____<ok>__<err>` (generic-user-type схема), тогда как
  mono'd Result-структура объявлена `NovaRes_<ok>_<err>` (Plan 59
  Ф.7.5). Имя-рассинхрон → CC-FAIL «unknown type name». Корень —
  `apply_type_subst_to_ref` имеет спец-ветку для `Option`, но не для
  `Result`. → **Ф.2.1.**

**Soundness (Ф.0.4):** все 5 отказов — **loud** (3× link-error, 2×
CC-FAIL). Ни одного silent-wrong: 3 рабочих случая (`Option[T]`/`[]T`/
`Box[T]` return) дают корректный runtime (assert'ы проходят). Дыры
soundness нет.

**Decision point (Ф.0.6):** Plan 88 закрывает D1 (Ф.1) + D2 (Ф.2.1).
`Option[T]` return — **уже работает**, делать нечего. `[]T`-return и
generic-record `Box[T]`-return — **уже работают** (probe'ы PASS) →
выносить в Plan 54-lineage **нечего**: Ф.2.2 неприменима (нет дефекта).
Объём Ф.1/Ф.2 финализирован: Ф.1 = static-call typevar resolution;
Ф.2 = `apply_type_subst_to_ref` спец-ветка `Result`.

## Acceptance criteria

- [x] D72-канонический пример `fn func[K, T From[K]](v K) -> T =>
      T.from(v)` + вызов с turbofish — компилируется, линкуется,
      корректно работает (Rust-паритет). _(plan88/d72_canonical.nv)_
- [x] `T.from` / `T.try_from` / `T.new` на type-параметре внутри
      generic-тела — все работают (обобщённый путь через `recv_seg`,
      не спец-кейс). _(plan88/static_from.nv, static_methods.nv)_
- [x] generic-функция с `Result[T,E]` / `Option[T]` return-типом
      мономорфизируется. _(plan88/result_return.nv)_
- [x] overload `from` на резолвленном typevar выбирает верный
      mangled-symbol — через `method_overloads` strict-match (Ф.1.4).
- [x] `generic_from_bound.nv` / `generic_try_bound.nv` переписаны на
      прямой `T.from` / `T.try_from` внутри generic-тела — probe-форма
      устранена.
- [x] Soundness: нет silent-wrong кодогенерации (Ф.0.4 — все 5
      отказов были loud: link / CC-FAIL).
- [x] Полный `nova test` — 0 новых FAIL.
- [x] `[M-generic-static-method-on-typevar]` закрыт; D72-пример
      работает в реализации.

## Non-scope

- **Existential / dynamic dispatch** — `x From[str]` как тип параметра
  (не bound). `From` как existential семантически бессмыслен (`from`
  производит `Self`); Plan 88 — только universal/mono путь (D72).
- **Полная type-erasure → vtable fat-pointer dispatch** — Plan 72
  P3-B territory; Nova для bound'ов использует мономорфизацию, не
  erasure (D72).
- **Чистый `[]T` / generic-record return**, **не связанный** с
  подстановкой typevar в static-dispatch — остаётся в Plan 54-lineage,
  **если** Ф.0 докажет, что это другой механизм (решение фиксируется
  в «Кластер по итогам Ф.0» с обоснованием — это граница задач, а не
  тихое упрощение).
