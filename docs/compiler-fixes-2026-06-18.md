# Исправления компилятора 2026-06-18

Все изменения — исправления багов (не изменение спеки). Объяснение ниже.

---

## 1. imports.rs: пиры с REQUIRES_SMT_BACKEND теперь пропускаются

**Файл:** `compiler-codegen/src/imports.rs`  
**Строки:** добавлен блок после чтения `src` в цикле `for sp in sib_paths`

**Проблема:**  
Когда test runner запускает файл напрямую, он читает `// REQUIRES_SMT_BACKEND z3v2`
из комментария и делает SKIP до парсинга файла. Но когда тот же файл попадает в
папочный модуль как со-файл (entry-folder peer) другого теста, эта проверка НЕ
делалась. Парсер пытался распарсить файлы с синтаксисом, который сам же
комментарий помечает как "parser сейчас не принимает". Это давало PARSE-ERROR,
который превращался в CODEGEN-FAIL для ОСНОВНОГО теста (не связанного с z3v2).

**Почему это баг, а не изменение спеки:**  
Аналогичная проверка для `#cfg` пиров уже существовала (`cfg_active` в том же
коде). Пропуск `REQUIRES_SMT_BACKEND` был явным отсутствием симметрии. Тест,
который проходил при прямом запуске (`SKIP`), фейлил при компиляции как пир.

**Исправление:**  
Перед парсингом каждого пира проверяем `// REQUIRES_SMT_BACKEND` в первых 30
строках (через `crate::test_runner::parse_smt_backend_requirement`). Если
активный backend не совпадает — пропускаем пир (аналогично `cfg_active`).

---

## 2. types/mod.rs: pure_views — мультимапа вместо одной записи

**Файл:** `compiler-codegen/src/types/mod.rs`  
**Структура:** `ContractCtx`  
**Поле:** `pure_views: HashMap<String, (String, usize)>` → `HashMap<String, Vec<(String, usize)>>`

**Проблема:**  
`ContractCtx::build` заполнял карту `pure_views`, маппируя имя метода
(`balance`) → `(имя эффекта, арность)`. Когда несколько эффектных типов объявляли
`#pure`-метод с одинаковым именем (например, `Db5`, `Db7`, `Db8`, `Db9` —
каждый имел `#pure balance(id int) -> int`), в карте оставалась только ПОСЛЕДНЯЯ
запись. При компиляции с folder-module пирами (все тесты `nova_tests/contracts/`
разделяют один модуль) все определения оказывались в одном compilation unit.

Затем `ContractCtx::check_fn` проверял функцию `withdraw2(id int, amount int) Db7 -> int
requires balance(id) >= amount` — она использует `balance` в контракте, и `Db7`
есть в её сигнатуре. Но `pure_views["balance"]` указывал на `Db9` (последний
записанный). Чекер видел: `Db9` отсутствует в сигнатуре `withdraw2` → ошибка
"pure_view `Db9.balance` referenced in contract of `withdraw2`, but effect `Db9`
is not in this function's signature". Хотя код был абсолютно корректен.

**Почему это баг, а не изменение спеки:**  
Контракт-проверка D142/Plan 33.3 Ф.9.3 гласит: pure_view можно использовать в
контракте если соответствующий эффект есть в сигнатуре функции. Это правило
выполнялось — `Db7.balance` был в сигнатуре. Баг был в реализации: хэш-карта
с одной записью на имя метода ломала проверку при нескольких эффектах с
одинаковым именем pure_view.

**Исправление:**  
Изменили тип на `HashMap<String, Vec<(String, usize)>>`. При заполнении
используем `entry().or_default().push(...)`. При проверке: ошибка выдаётся
только если НИ ОДИН эффект из списка не присутствует в сигнатуре функции.

---

## 3. std/runtime: @buf.capacity() → @buf.cap() в StringBuilder и WriteBuffer

**Файлы:** `std/runtime/string_builder.nv`, `std/runtime/write_buffer.nv`

**Проблема:**  
Методы `StringBuilder.@capacity()` и `WriteBuffer.@capacity()` вызывали
`@buf.capacity()` на внутреннем `[]u8` (`Vec[u8]`). Но у `Vec[T]` метод
называется `@cap()`, а не `@capacity()`. Codegen пытался диспатчить
`capacity()` на Vec — не найдя метода, резолвился к методу самого типа
(WriteBuffer/StringBuilder) — что создавало бесконечную рекурсию → segfault.

**Почему это баг:**  
Wrong method name в stdlib теле метода. Компилятор корректно диспатчит по
имени метода, ошибка в исходнике — `@buf.capacity()` должно быть `@buf.cap()`.

**Исправление:**  
`@buf.capacity()` → `@buf.cap()` в обоих файлах.

---

## 4. types/mod.rs: удалены external_sources из LinearityRegistry и ConsumeRegistry

**Файл:** `compiler-codegen/src/types/mod.rs`

**Проблема:**  
`LinearityRegistry::build()` и `ConsumeRegistry::build()` содержали
`external_sources` — массив `.nv` файлов стандартной библиотеки, которые
парсились заново прямо внутри проверяющих passes. Это нарушало принцип:
компилятор не должен «запекать» пользовательские типы внутрь себя; он узнаёт
о них при компиляции `.nv` файлов. Типы из stdlib (StringBuilder, WriteBuffer,
ReadBuffer — через prelude; Mutex, TcpStream — через явные импорты) уже
присутствуют в `module.items` после `resolve_imports_inline_ex`, поэтому
повторный парсинг их исходников был избыточным и семантически неправильным.

**Почему это баг:**  
Нарушение архитектурного принципа: компилятор должен знать о типах из
компилируемых `.nv` файлов, а не из захардкоженного списка исходников.

**Исправление:**  
Убраны оба блока `external_sources` (в `LinearityRegistry::build` и
`ConsumeRegistry::build`). Типы теперь распознаются только через `module.items`,
заполняемый при `resolve_imports_inline_ex` до вызова `check_module`.

---

## 5. emit_c.rs: closure-typed local var затеняет одноимённую free fn в return-type инференсе

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`  
**Метод:** `infer_expr_c_type`, ветка `ExprKind::Call` с `func = Ident(name)`

**Проблема:**  
В `nova_tests/syntax/closure_corner_cases.nv` есть:
```nova
ro first = || n
ro second = || n + 1
if use_first { first() } else { second() }
```
`first()` инферился как `NovaOpt_nova_int`, а `second()` — правильно как `nova_int`.
Из-за этого `if`-выражение получало тип `NovaOpt_nova_int`, и C-codegen
присваивал `nova_int` в слот `NovaOpt_nova_int` → CC-FAIL.

Корень: имя локального замыкания `first` совпало с именем stdlib-итератор-
адаптера `first() -> Option[T]`. При компиляции folder-модуля все `.nv`
делят один `var_types`, где висит `fn_ret_first = "NovaOpt_nova_int"`.
В `infer_expr_c_type` lookup `fn_ret_<name>` стоял ВЫШЕ проверки «name —
локальная переменная closure-типа», поэтому перехватывал инференс и
возвращал return-тип чужой функции. `second` коллизии не имел → работал.

**Почему это баг, а не изменение спеки:**  
Локальный биндинг (`ro first = ...`) ОБЯЗАН затенять free fn с тем же именем
— это базовая семантика scope'а. `first()` здесь — вызов захваченного
замыкания, а не stdlib-функции. Инференс нарушал shadowing.

**Исправление:**  
Перенёс проверку closure-типизированной локальной переменной
(`var_types[name]` → `clos_struct_ret_type`) ВЫШЕ lookup'а `fn_ret_<name>`.
Теперь локальный closure-биндинг резолвится первым.

---

## 6. emit_c.rs: дизамбигуация одноимённых вариантов суммы по return-типу функции

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`  
**Метод:** `emit_record_lit` (резолв `variant_lookup`)

**Проблема:**  
В `nova_tests/syntax/` два типа объявляют вариант с одним именем `Circle`:
`Shape1 | Circle { radius f64 }` (is_sum.nv) и
`Shape2 | Circle { r int }` (record_literal_type_once.nv).
Функция `fn mk_circle(r int) -> Shape2 => Circle { r }` генерировала
`nova_make_Shape1_Circle()` (чужой тип, 0 аргументов) вместо
`nova_make_Shape2_Circle(r)`. Причина: `find_variant_compat("Circle")`
возвращает ПЕРВЫЙ зарегистрированный одноимённый вариант (Shape1), без учёта
контекста — оба варианта одного `SchemaSource`, одинаковой длины имени типа.

**Почему это баг:**  
sum-coercion (D-правило «тип литерала ≠ return-тип») гласит: литерал
`Circle { r }` в функции `-> Shape2` строит вариант ИМЕННО `Shape2.Circle`.
Контекст возврата — авторитетный источник. Codegen игнорировал его.

**Исправление:**  
Перед `find_variant_compat` добавлен контекстный lookup: если
`current_fn_return_ty == "Nova_<Sum>*"` и у `<Sum>` есть вариант с искомым
именем — берём именно его. Fallback на `find_variant_compat` сохранён для
случаев без контекста (let-биндинг без аннотации и т.п.).

---

## 7. emit_c.rs: tuple-of-closures — индекс tuple теряет тип элемента

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`  
**Метод:** `infer_expr_c_type`, ветка `Member { name = "0"/"1"/… }` (tuple index)

**Проблема:**  
`fn make_pair1() -> (fn()->int, fn()->int)` возвращает tuple замыканий. При
`ro inc = pair.0` codegen объявлял `nova_int inc` вместо `void*` (closure-
storage), потому что инференс `pair.0` читал side-table `tuple_element_types`
(содержащий устаревшие `["nova_int","nova_int"]`, записанные при биндинге
`pair`), а не декодировал реальный C-mono-name `_NovaTuple_2_6_void_p_6_void_p`.

**Исправление:**  
Декодирование mono-имени (`parse_mono_tuple_elements(&obj_ty)`) теперь
АВТОРИТЕТНО — оно отражает реальный C-struct. Side-table `tuple_element_types`
используется только как fallback, когда obj_ty не self-describing.

---

## 8. emit_c.rs: closure-вызов через fn_param_sigs в инференсе

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`  
**Метод:** `infer_expr_c_type`, ветка `Call { func = Ident(name) }`

**Проблема:**  
После фикса №7 `get` (элемент tuple) хранится как opaque `void*`. Вызов
`get()` инферился неправильно (str), потому что `void*` не распознаётся как
closure-struct, и срабатывала коллизия имени с stdlib-методом `get`.
Emit-сторона при этом корректно роутит `get()` через `fn_param_sigs` →
`NOVA_CLOS_CALL_vi`.

**Исправление:**  
Инференс `name()` теперь читает return-тип из `fn_param_sigs[name]` (тот же
реестр, что использует emit) ПЕРЕД lookup'ом `fn_ret_<name>`. Симметрия
инференса и эмиссии.

---

## 9. emit_c.rs: match block-arm — локальные let'ы засеиваются в инференсе результата

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`  
**Методы:** `emit_match` (`infer_arm` closure) + `infer_expr_c_type`
(`ExprKind::Match`)

**Проблема:**  
`match xs { [] => 0; [_, ..rest] => { mut s = 0; for x in rest { s += x }; s } }`
— результат `rest_sum` инферился как `nova_str` вместо `nova_int`. Тело arm
НЕ эмитится во время инференса, поэтому trailing-`s` подхватывал устаревший
`var_types["s"]` (от arm/файла в том же folder-модуле, где `s` был str).
Аналогично tuple-pattern `(1, _, c) => c` подхватывал `c: Nova_Color*`.

**Исправление:**  
Два места:
1. `collect_pattern_inner_bindings` получил ветку `Pattern::Tuple` — декодирует
   element-types из mono-имени scrutinee и рекурсивно биндит под-паттерны.
2. Оба `infer_arm` (emit_match через `var_types`, infer_expr_c_type через
   `pattern_binding_overrides`) теперь засеивают block-local `let`-биндинги
   их инференс-типами перед инференсом trailing, с save/restore.

---

## 10. emit_c.rs: bare-call free fn затеняется одноимённым методом в fn_ret

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`  
**Метод:** `infer_expr_c_type`, ветка `Call { func = Ident(name) }`

**Проблема:**  
`fn scale(p Point1) -> Point1` (free fn) и метод `Point7.scale(k) -> Point7`
делят ключ `fn_ret_scale` (записывается last-wins под голым именем и для
free fn, и для методов). При вызове `ro p = scale(...)` инференс брал
`fn_ret_scale = Nova_Point7*` (последний записанный метод), и `p` объявлялся
`Nova_Point7*` вместо `Nova_Point1*` → `p.x` читался через чужой layout →
assert `p.x == 10.0` падал в рантайме.

**Исправление:**  
Bare-вызов (`func` = `Ident`, не `Member`) — это вызов FREE fn. Инференс
теперь сначала читает return-тип из `user_fn_sigs[name]` (реестр заполняется
ТОЛЬКО для non-generic free fn без receiver — авторитетен для bare-call)
перед lookup'ом загрязнённого `fn_ret_<name>`.

---

## 11. test_runner.rs: RUN-FAIL detail показывает строки с FAIL/assert/panic

**Файл:** `compiler-codegen/src/test_runner.rs`  
**Место:** формирование `Stage::Run { error }` при `exit != 0` без content-marker

**Проблема (DX, не баг компилятора):**  
In-binary тест-харнесс печатает много `PASS:`-строк, затем summary
(`351/352 passed`). При фейле раннер показывал «последние 3 строки» — это
trailing PASS + счётчик, скрывая КАКОЙ тест упал.

**Исправление:**  
detail теперь предпочитает строки, содержащие `fail`/`assert`/`panic`
(до 4 шт.); fallback на last-3, если таких нет. Диагностика, поведение
тестов не меняется.

---

## 12. emit_c.rs: const-ссылка-на-const манглит имя референса

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`
**Метод:** `emit_const_expr`, ветка `ExprKind::Ident`

**Проблема:**
`const BASE int = 100` + `const DERIVED int = BASE + 10`. Module-private const
`BASE` мангляется в C как `Nova_const_<modpath>_BASE`, но при эмиссии
инициализатора `DERIVED` ссылка на `BASE` выдавалась как сырой `BASE` →
`use of undeclared identifier 'BASE'` (CC-FAIL).

**Почему это баг:**
На use-site `Ident(name)` обычного кода манглинг уже делался через
`private_const_c_names`, но в const-инициализаторе (`emit_const_expr`) ветка
Ident возвращала `name.clone()` без манглинга.

**Исправление:**
`emit_const_expr(Ident)` теперь резолвит mangled C-name через
`private_const_c_names` по `expr.span.file_id`; fallback на сырое имя для
exported const'ов (эмитятся под собственным именем).

---

## 13. emit_c.rs: bare unit-variant дизамбигуируется по target-типу (аннотации)

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`
**Метод:** `emit_expr_with_target_type`, ветка `ExprKind::Ident`

**Проблема:**
`Empty` объявлен и в `Node | Leaf(Point) | Empty`, и в `Slot` (другой файл
folder-модуля). `ro e Node = Empty` давал `Nova_Node* e = nova_make_Slot_Empty()`
— тип переменной правильный (по аннотации), но КОНСТРУКТОР резолвился через
first-wins `find_variant_compat("Empty")` → `Slot.Empty`. Сравнение `a != e`
обращалось к чужому layout → RUN-FAIL.

**Почему это баг:**
Явная аннотация `ro e Node = …` — авторитетный target-тип. Конструктор bare
unit-variant обязан строить вариант ИМЕННО этого sum-типа.

**Исправление:**
`emit_expr_with_target_type(Ident)`: если target = `Nova_<Sum>*` и у `<Sum>`
есть unit-вариант с этим именем — эмитим `nova_make_<Sum>_<Variant>()`.
Симметрично существующей дизамбигуации `None` по `NovaOpt_<T>` target.

Известное ограничение (НЕ фикс): без аннотации (`ro e = Empty`) и при
коллизии имён компилятор по-прежнему берёт first-wins — bidirectional-инференс
из последующего сравнения требует доработки type-checker'а. Аннотация — рабочий
обход; см. `nova_tests/plan141/t5_sum_record_payload.nv`.

NOTE §11: лимит детали RUN-FAIL поднят 150 → 400 символов (имена тестов на
кириллице длиннее; полезнее для диагностики).

---

## 14. emit_c.rs: pattern_cond тег unit-варианта берёт sum из типа scrutinee

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`
**Метод:** `pattern_cond`, ветка `Pattern::Variant` (резолв `type_name` при `path.len()==1`)

**Проблема:**
`type Tier | Low | Mid | Hi` + другой sum-тип с вариантом `Hi` (`Tag1`) в том
же folder-модуле. В `match t { … Hi => "hi" }` для arm `Hi` (bare, без `Tier.`)
тег резолвился через first-wins `find_variant_compat("Hi")` → `NOVA_TAG_Tag1_Hi`
вместо `NOVA_TAG_Tier_Hi`. Scrutinee `Nova_Tier*` никогда не матчил этот tag →
`_nv_matched` оставался 0 → результат match неинициализирован → assert падал.

**Исправление:**
При `path.len()==1` тип scrutinee (`var_types[scr]` → `Nova_<Sum>*`)
АВТОРИТЕТЕН: если этот sum объявляет искомый вариант, берём его имя. Guard
`scr_is_mono` (имя содержит `____`): для mono'd generic sum тег пишется как
`NOVA_TAG_Nova_<mono>_<V>`, поэтому там оставляем fallback на
find_variant_compat (базовое имя). Fallback также при unknown scrutinee.
plan125 5/5 PASS (без регрессии negative-тестов).

---

## 15. emit_c.rs: bodyless `-> T` инференс return-типа засеивает типы параметров

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`
**Методы:** forward-decl free fn (~9484) + `emit_fn` (~16643), перед `return_type_c(f)`

**Проблема:**
`#blocking fn _blk_notify(mut cv Condvar) { cv.notify_one() }` без `-> T`.
`return_type_c` инферит return из trailing-выражения `cv.notify_one()`. Но на
момент вызова (и в forward-decl, и в definition) типы параметров ещё НЕ
зарегистрированы в `var_types` (params-loop идёт ПОСЛЕ return_type_c). Поэтому
`cv` инферился как nova_int (fallback) → method-lookup `Condvar.notify_one`
промахивался → return мистипился `nova_int` вместо `nova_unit`.
Результат: `nova_int _blk_notify(...) { return <unit-expr>; }` (CC-FAIL
initializing), либо conflicting types forward-decl (unit) vs definition (int).

**Почему это баг:**
Тип параметра известен из сигнатуры (`mut cv Condvar`). Инференс return из
тела обязан видеть параметры в scope — это базовая семантика.

**Исправление:**
В обоих местах перед `return_type_c(f)` (только когда `f.return_type.is_none()`)
временно засеиваем `var_types[param] = type_ref_to_c(param.ty)`, после —
восстанавливаем (save/restore, без утечки между декларациями). Теперь
`cv.notify_one()` резолвится в `Nova_Condvar*` → method-lookup попадает →
return = nova_unit. plan103_6 14/14 PASS.
