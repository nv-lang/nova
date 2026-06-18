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
