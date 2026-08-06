# PROGRESS — окно p375-ptr2 (№375 + №367 + №368 + №377)

Модель: sonnet. Ветка `p375-ptr2`, worktree `d:/Sources/nv-lang/nova-p375`.
Пункт 12 К1 плана 221 (`docs/plans/221-release-v0-1.md`); записи `| 375 |`,
`| 367 |`, `| 368 |` в `docs/plans/221.1-bug-sweep.md`; №377 добавлен
координатором в ходе окна (та же указательная связка, решение владельца
«до релиза»).

Все фиксы — ОДНИМ каналом: `compiler-codegen/src/types/mod.rs` (checker).
`emit_c.rs`/легаси НЕ тронут (кроме исследования №377, см. ниже) —
arch-ratchet ровно на базовой линии (`lines=64171<=64171`,
`infer=348<=348`, скрипт `scripts/guards/arch-ratchet.sh`).

## №375 — `&x` на readonly-биндинге легализовало запись через `*mut T` — ЗАКРЫТ

### Важное изменение мандата в процессе окна

Изначальный бриф фиксировал правило вывода `&x` = D246 «всегда `*T`»
(п.12 плана 221) и запрещал мне реализовывать альтернативу. **В ходе окна
координатор передал решение владельца (вариант Б, спека уже поправлена
на main координатором, коммит `96100421e`, D216 §4 AMEND + D246
«Восстановление §V2.6 — частично отменено»):**

1. Вывод `&x` ВОССТАНОВЛЕН к Plan 118.6: от `mut`-биндинга → `*mut T`
   автоматически; от `ro`-биндинга → `*T`.
2. Аннотированный тип держится: `*T` в аннотации — всегда ro-pointee
   (`mut p *T` записи не даёт); `ro p *mut T = &mut_x` легальна.
3. От ro-источника `*mut T` НЕ достижим НИКАКИМ путём — ни выводом
   (закрывается само правилом 1), ни аннотацией, ни параметром/полем/
   возвратом, ни кастом.
4. Каст `(&x) as *mut T` (тот же pointee `T`) РЕТРАКТИРОВАН ЦЕЛИКОМ —
   даже от `mut`-источника (где он теперь просто избыточен: `&mut_x` сам
   даёт `*mut T`).

Я реализовал ОБА мандата за одно окно (координатор явно снял запрет на
вариант Б); правило-источник (п.3 ниже) построено ИНВАРИАНТНО к
выбору А/Б, как и просили изначально.

### Корень (проверено интегратором до старта)

```nova
type Counter { v int }
fn main() {
    ro b = Counter { v: 7 }
    ro q *mut Counter = &b
    unsafe { q.write(Counter { v: 8 }) }
    println("${b.v}")     // 8 — ro-значение изменено
}
```

`nova check` PASS, `build` built, `run` печатает 8 — ro-гарантия обходится
одной строкой. Корень: инференс `&x` (было — всегда `*T`, ЛИБО теперь —
зависит от биндинга) НЕ проверялся против ОЖИДАЕМОГО типа в точке
материализации (аннотация/параметр/поле/возврат/каст) — структурное
сравнение типов (`resolved_cat_of`/`cat_compatible_rt`) схлопывает ЛЮБОЙ
`Pointer(..)` в одну категорию `Ptr`, не различая `Pointer(Mut(T))` vs
голый `Pointer(T)`.

### Фикс — общее место (как и просил бриф)

Новая пара функций в `TypeCheckCtx` (`types/mod.rs`, рядом с
`is_through_ro_binding`/`assign_root_ident`):

- `addrof_mut_ro_source_root(&self, expr) -> Option<&str>` — распознаёт
  ПРЯМОЙ `&place`/`raw &place`, чей КОРЕНЬ (`assign_root_ident`) —
  `ro`-биндинг (`is_through_ro_binding`). НЕ рекурсирует через `As` —
  каст-путь имеет собственную отдельную проверку (см. ниже), чтобы не
  задваивать диагностику.
- `check_addrof_mut_from_ro_source(&self, value, expected, errors)` —
  если `pointee_is_writable(expected) == Some(true)` (ожидаемый тип —
  писабельный `*mut T`) и `addrof_mut_ro_source_root(value)` даёт корень —
  `[E_POINTER_MUT_FROM_RO_SOURCE]`.

Вызвана в ОДНОМ месте на каждый путь материализации:
1. `f1_check_assign_let` — let-аннотация.
2. Общий цикл call-arg (`f1_check_call`, ~L14994+) — параметр.
3. Generic-instance overload arg-check (~L14140) — параметр через
   receiver-generic путь (этот сайт свою `Compat::CoerceConflict`
   гейтит на `generic_param`; source-check вызван БЕЗ этого гейта).
4. `check_fn_value_call` (~L14249) — параметр через fn-value биндинг.
5. `ExprKind::RecordLit` арм `f1_expr_inner` — поле-литерал (тип поля —
   через `record_fields_for`).
6. `Stmt::Return` — возврат (тип — `current_fn_return_ty`).

### Каст — отдельная ретракция (координатор, п.4)

`ExprKind::As(inner, cast_ty)` арм `f1_expr_inner`: если `cast_ty`
писабелен И `inner` — ПРЯМОЙ `&`/`raw &` (не любое указательное
выражение вообще) над ТЕМ ЖЕ pointee `T` — `[E_POINTER_OP_USE_METHOD]`
(та же семья кодов, что и остальные D216/174.5-ретракции). Текст:
ретракция + fix-it «возьми адрес от mut-биндинга — вывод даст *mut сам».

**Почему scoped к ПРЯМОМУ `&`/`raw &`, а не к «любому уже-указателю»:**
широкая формулировка ломает легитимный `std/src/collections/vec/
core.nv:119` (`Vec[T].new(ptr *T, len) -> ro Self`, VIEW-конструктор,
`unsafe { ptr as *mut T }` — `ptr` ЧУЖОЙ параметр, не адрес локального
биндинга, `unsafe fn` с документированным «обязательство на вызывающем»,
и переписать это через инференс/аннотацию НЕЛЬЗЯ — нет биндинга, чей
адрес взять). Проверено эмпирически — до сужения именно этот файл
ловился ретракцией; после сужения — чист. Гайд `docs/guide/typed-
pointers.md:194,316` (`(&a) as *mut Counter` / `(&x) as *mut int`) —
ОБА примера ПРЯМОЙ адрес-каст → ловятся моей ретракцией корректно (не
трогал файл — чинится параллельной сессией синхронно).

### Инференс `&x` (вариант Б, восстановленная 118.6)

`infer_expr_type`, арм `UnOp::AddrOf | UnOp::RawAddrOf`: корень операнда
через `assign_root_ident` + `ro_binding_names` → `mut`-источник даёт
`Pointer(Mut(T))`, иначе — голый `Pointer(T)`.

### Корпус — миграция с `as *mut` (тот же pointee)

Проверено `grep -rn "as \*mut"` по `spec_tests`, `nova_tests`, `examples`,
`std`, плюс `../nova-tls`/`../nova-http`/`../nova-polaris`/`../nova-bignum`
(доступные working directories). Формы `(&x) as *mut T` (ретрактированная)
нашлись ТОЛЬКО в 4 файлах spec_tests/conformance — все смигрированы на
голый `&x`/`&mut_x` (кас убран, инференс сам даёт `*mut T`):

- `spec_tests/conformance/d141_ptr_read_write_unaligned.nv` — `(&y) as *mut int` → `&y`.
- `spec_tests/conformance/d216_ptr_methods_174_5.nv` — 2 места (`(&x) as *mut int`, `(&dst) as *mut int` + `(&src) as *int`) → `&x`/`&dst`/`&src`.
- `spec_tests/conformance/d216_unused_unsafe_pos.nv` — `(&x) as *mut int` → `&x`.
- `spec_tests/conformance/neg/d216_ptr_deref_write_neg.nv` — `(&x) as *mut int` → `&x`.

Остальные 19 `as *mut`-вхождений в `std/` (`RawMem.alloc(...) as *mut T`,
`@data.offset(i) as *mut u8`, и т.п.) — casts РАЗНЫХ pointee-типов
(reinterpret, не mutability-upgrade того же `T`) — вне scope ретракции,
не тронуты (правильно — иначе сломались бы `Vec`/`RawMem`/`string_builder`
целиком). Пакетные репы (`nova-tls`/`nova-http`/`nova-polaris`/
`nova-bignum`) — 0 находок формы `(&x) as *mut` (единственное совпадение
в `nova-tls/src/stream.nv:355` — `0 as *mut u8`, литерал-каст, не
адрес-каст, не задет).

### Фикстуры

**Neg** (`spec_tests/conformance/neg/`, каждый — свой compile unit):
- `d375_ptr_mut_from_ro_let_neg.nv` — let-аннотация.
- `d375_ptr_mut_from_ro_param_neg.nv` — параметр.
- `d375_ptr_mut_from_ro_field_neg.nv` — поле record-литерала.
- `d375_ptr_mut_from_ro_return_neg.nv` — возврат.
- `d375_ptr_mut_cast_from_ro_neg.nv` — каст, ro-источник.
- `d375_ptr_mut_cast_from_mut_neg.nv` — каст, mut-источник (форма снята
  ЦЕЛИКОМ, не только для ro).

**Pos** (`spec_tests/conformance/d375_ptr_mut_from_mut_source_pos.nv`,
часть единого folder-module, тип `PtrSrcCounter` — уникальное имя,
проверено на коллизии): 3 value-checking теста — голый `&mut_x` даёт
`*mut T` и запись видна на оригинале; явная аннотация `*mut T` над
mut-источником легальна; `&mut_x` в параметр `*mut T` легален.

### Вердикты (`nova check`/`nova test`, ветка `p375-ptr2`)

Все 6 neg + 1 pos — `nova test <файл>` (полная сборка+codegen+run, НЕ
только check): **PASS 3+3+1 = 7/7** (два батча из-за таймаута
whole-folder-module сборки на батч >3 файлов; по одному/по три — все
зелёные). Pos-файл реально СОБРАЛСЯ и ЗАПУСТИЛСЯ — assert'ы прошли
(значение реально видно на оригинале, не только «компилируется»).

## №367 — паритет чтения — ЗАКРЫТ

Снятые формы чтения `x = *p` (Deref) и `y = p[i]` (Index) на raw pointer
всё ещё проходили `nova check` (запись закрыта №353 в
`check_target_readonly`; это — та же ретракция, но со стороны READ, в
ДРУГОМ узле — `f1_expr_inner`, общий рекурсивный обход выражений).

### Фикс

Тот же узел, что предлагал бриф («check_target_readonly Deref/Index-армы —
образец места») НЕ подошёл напрямую — он вызывается ТОЛЬКО для
assignment-target. Общий READ-обход живёт в `f1_expr_inner`'s
`ExprKind::Unary`/`ExprKind::Index` армах — туда и добавлена симметричная
проверка `pointee_is_writable(operand_ty).is_some()` → любой raw pointer
(ro или mut) → `[E_POINTER_OP_USE_METHOD]` (`.read()`/`.read_at(i)`).

**Развилка write/read на ОДНОМ узле:** `Stmt::Assign` прогоняет `target`
через ТОТ ЖЕ `f1_expr` (для типовой аннотации канала), поэтому `*p = v`/
`p[i] = v` попадали бы в Unary/Index армы ДВАЖДЫ — один раз как READ (мой
новый чек), один раз как WRITE (`check_target_readonly`). Добавлено новое
поле `assign_target_top: Cell<bool>` (тот же set-before/
consume-on-entry-`replace(false)` протокол, что уже существующий
`in_call_func`): `Stmt::Assign` ставит `true` ПЕРЕД `f1_expr(target,..)`;
Unary/Index армы читают-и-сбрасывают на ВХОДЕ (до рекурсии в
`operand`/`obj`) — так вложенный `**p = v`/`a[i][j] = v` тоже верно
классифицирует внешний узел как WRITE, внутренний — как READ.

### Фикстуры

- `neg/d367_ptr_deref_read_neg.nv` — `mut y = *p`.
- `neg/d367_ptr_index_read_neg.nv` — `mut y = p[1]`.
- Pos — объединён с №368 (см. ниже, `.read()`/`.read_at()` legit).

Регресс write-форм (`p.write(v)`) проверен вручную — легален (см. ниже).

## №368 — пробел вывода `mut p = buf.ptr()` — ЗАКРЫТ

### Корень

`infer_expr_type`'s `ExprKind::Call` арм имел ТОЧЕЧНЫЙ спецкейс для
`[N]T @len()`/`@ptr()` (FixedArray, компилятор-синтезированные аксессоры,
Plan 200 П19) — но НЕ для настоящих `.nv`-объявленных
`Vec[T] @ptr() -> *T` / `Vec[T] mut @ptr() -> *mut T`
(`std/src/collections/vec/access.nv:258/266`). Общего пути «резолвь
return type ЛЮБОГО instance-метода через `method_overloads`» в
`infer_expr_type` НЕТ вообще (существует `resolve_instance_method_return`,
но это МЁРТВЫЙ код — `dead_code`-warning, ни одного вызывающего). Из-за
этого `Stmt::Let`'s scope-регистрация (`d.ty.or(chain_ty).or_else(infer_
expr_type).or_else(channel)`) для `mut p = buf.ptr()` БЕЗ аннотации не
находила НИЧЕГО → `scope.remove(&name)` → `p` вообще не типизирован в
scope. Подтверждено трассировкой (`NOVA_IDX_TRACE=1`): `[IDX-MISS] v=p
scope=false buf=false`.

Эффект — молчание ВСЕХ последующих проверок, ключующихся на
`infer_expr_type(p)`: `E_POINTER_RO_ASSIGN` (writability через
`.write()`), №349/№353 (запись через `p.field`/`*p`/`p[i]`), теперь и
№367 (чтение). Кодоген (отдельный, C-type-string based инференс) при
этом классифицировал `p` верно — ровно асимметрия, которую описывает
№368.

### Фикс — узкий, тем же каналом (НЕ новый проход, НЕ `resolve_instance_
method_return`)

Зеркало FixedArray-спецкейса, СВОИМ отдельным армом (не трогая
существующий): `ExprKind::Member{obj,name:"ptr"}`, нулевая арность,
`array_elem_type(obj_ty)` (уже существующая функция, нормализует ОБА
`[]T`-сахар и явный `Vec[T]` к элементу) → `is_mut =
!is_through_ro_binding(obj)` → `Pointer(Mut(T))` либо голый `Pointer(T)`.
Структурно, не через реестр методов — тот же стиль, что и FixedArray-сосед,
явно НЕ общий `resolve_instance_method_return` (риск шире, чем нужно
этому багу — эта функция мёртвая, непроверенная на всём корпусе).

### Фикстуры

- `neg/d368_ptr_unannotated_ro_source_write_neg.nv` — `ro buf`,
  неаннотированный `mut p = buf.ptr()` (пикает RO-перегрузку `@ptr()`) →
  `p.write(...)` теперь корректно `[E_POINTER_RO_ASSIGN]` (было — тихо
  проходило).
- Pos — `spec_tests/conformance/d367_d368_ptr_unannotated_method_forms_pos.nv`
  (часть folder-module): методные формы (`.write()`/`.write_at()`/
  `.read()`/`.read_at()`) остаются легальны на НЕаннотированном
  `buf.ptr()` для ОБОИХ источников (`ro`/`mut`), value-checking.

### Вердикты

`nova test` — все 3 neg (батч) + pos (отдельно, whole-folder-module
сборка ~неск. минут холодной сборки) — **PASS 3/3 + 1/1**, все с реальным
запуском/assert'ами.

## №377 — `p.method()` возвращает мусор — ИССЛЕДОВАНО, НЕ ФИКШЕНО (доклад владельцу)

### Подтверждение репро (build+run, ветка `p375-ptr2`)

```nova
type Counter { v int }
fn Counter @val() -> int => @v
fn main() {
    mut a = Counter { v: 21 }
    ro p = &a
    println("${p.val()} ${p.read().val()}")   // напечатало: 3020263542768 21
    println("field: ${p.v}")                   // напечатало: field: 21
}
```

Оба заявленных auto-deref оператора D216 §5 ПРОВЕРЕНЫ раздельно —
**`p.field` (чтение поля) КОРРЕКТЕН**, сломан ТОЛЬКО `p.method()`.

### Корень (найден разведкой, `compiler-codegen/src/codegen/emit_c.rs`)

Приёмник метод-вызова строится через общий helper `prepare_method_recv`
(`emit_c.rs:55275-55329`), используемый на большинстве instance-call
сайтов (`:41232`, `:42372`, `:43038`, `:43288`, `:43982`/`:44003`). Этот
helper обрабатывает спецкейс ТОЛЬКО для `NovaValue_*`/`NovaTuple_*`
(`is_value_struct_val`); для всего остального (включая `Nova_Counter**` —
C-представление `*Counter`, raw pointer НА record-значение, само значение
которого уже `Nova_Counter*`) — возвращает `obj_c` БЕЗ изменений
(`:55326-55328`, `else { obj_c.to_string() }`). Итог: в
`Nova_Counter_method_val(Nova_Counter* self)` передаётся СЫРОЕ `p` (тип
`Nova_Counter**`) — адрес переменной `p`, а не адрес `a`; поле `v`
(offset 0) читает первые 8 байт `&p`, там как раз лежит значение `p`
(= адрес `a`) — отсюда «похожий на адрес мусор».

Референс — путь `p.field`, КОТОРЫЙ работает верно: `ExprKind::Member` арм
`emit_expr_inner` (`:34944-34958`) явно проверяет
`obj_ty.ends_with("**")` (`is_double_ptr`) и оборачивает в `(*obj)->field`
— ИМЕННО той проверки нет в `prepare_method_recv`. Диспетчеризация ИМЕНИ
метода при этом резолвится верно — `debt_strip_recv_c_prefix`
(`:54825-54835`) использует `trim_end_matches('*')` (срезает ВСЕ звёзды
разом → верный C-символ), но `prepare_method_recv` теряет информацию об
уровне косвенности при построении САМОГО receiver-выражения.

### Почему НЕ фикшу сам

Фикс лежит ЦЕЛИКОМ в `emit_c.rs` (легаси codegen) — ни один канал чекера
(`resolved_types`/`resolved_callees`) тут не участвует, это ЧИСТО
C-эмиссия receiver-выражения, аналог которой (`is_double_ptr`) УЖЕ
существует в СОСЕДНЕМ member-арме того же файла. Правило окна («фиксы
чекерные, §0/196; в emit_c не заходить») и постоянное правило проекта
(`feedback-compiler-fixes-checker-channel-196.md`: «легаси emit_c НЕ
наращивать; минимальный фикс в легаси = нарушение даже при зелёных
гейтах») здесь буквально применимы — канального пути для этого класса
бага просто нет. Координатор явно попросил доложить оба варианта с
ценой, а не решать самому.

### Два варианта (цена каждого)

**A. Точечный фикс в `emit_c.rs`, `prepare_method_recv`.** По образцу
`is_double_ptr` из member-арма: если `obj_ty` (после trim `const`)
заканчивается на `**` и это `Nova_`-тип (raw pointer на record-значение)
— обернуть `obj_c` в `(*obj_c)` перед дальнейшей обработкой. Оценка: ~5-15
строк, один helper, низкий риск ИЗОЛИРОВАННО (тот же паттерн, что уже
доказанно работает в member-арме). НЕ архитектурно полный: несколько
соседних веток (`:43085`, `:43143`, `:43159`, `:42357` — generic/
external/примитив-ресивер диспетчеризация) строят receiver через
`emit_expr(obj)` НАПРЯМУЮ, минуя `prepare_method_recv` — но эти пути,
по всей видимости, нерелевантны для record-указателя (`Nova_X**`) класса
(примитивы/generic-хендлы не имеют такого же double-star representation).
Требует явного одобрения владельца/интегратора на рост `emit_c.rs` —
через `arch-ratchet.baseline` (см. прецедент п.137 в 221.1-bug-sweep.md,
«Path B» — baseline поднимается с обоснованием) — сам НЕ поднимал.

**B. Ретрактировать auto-deref для методов** (оставить ТОЛЬКО `p.field`;
`p.method()` требовать явного `p.read().method()` всегда). Это
LANGUAGE-MENYAющее решение — требует D-амендмента к D216 §5 («ОСТАЮТСЯ
операторами: … auto-deref p.field/p.method()» → сузить до одного
`p.field`) в ТОМ ЖЕ слиянии. Цена корпуса: **~0** — разведка не нашла
НИ ОДНОГО `p.method()` auto-deref на пользовательском типе нигде в
`spec_tests/conformance/*ptr*`; гайд (`docs/guide/typed-pointers.md:
208-212`) УЖЕ официально документирует баг как известный/tracked и
УЖЕ рекомендует `p.read().method()` как канон — т.е. корпус СЕЙЧАС живёт
так, будто auto-deref для методов уже не существует. Ретракция сделала бы
это явным (компайл-ошибка вместо тихого мусора в рантайме) без миграции
единого реального файла.

**Моя рекомендация (не решение):** B — дешевле, безопаснее (текущее
поведение — silent data corruption, ХУЖЕ compile error), корпус уже
живёт по правилам B де-факто. Но выбор — владельца.

### Фикстуры (value-checking, как просил координатор)

`spec_tests/conformance/d377_ptr_auto_deref_field_and_workaround_pos.nv`
(часть folder-module, тип `Ptr377Counter` — уникальное имя):
- `p.field` auto-deref read — value-checked (`p.v == 21`), **корректен**.
- `p.read().method()` (канонический workaround) — value-checked
  (`p.read().val() == 21`), **корректен**, регресс-guard на будущее.
- `p.method()` auto-deref (СЛОМАННЫЙ путь) — **сознательно НЕ добавлен**
  как фикстура: assert на ПРАВИЛЬНОЕ значение упал бы на текущем
  компиляторе (красный гейт из-за незакрытого бага — путает «фикстура
  сломана» с «баг не пофикшен»); assert на НЕПРАВИЛЬНОЕ значение
  канонизировал бы баг как ожидаемое поведение — оба варианта хуже, чем
  честно задокументировать в этом отчёте (репро выше) и оставить
  открытым до решения владельца по A/B.

### Вердикты

`nova check` на новой фикстуре — PASS 1/0 (часть folder-module). Полный
`nova test` (build+run) на whole-folder-module ЗАВИС/не уложился в 10 мин
дважды подряд (независимая проблема — heavy cold-build всей папки
`spec_tests/conformance`, ~200 файлов; НЕ регрессия этого фикса — те же
таймауты уже наблюдались на НЕ связанных с №377 фикстурах этого окна,
см. №375/№368 выше, где повторный прогон С БОЛЬШИМ таймаутом всё же
проходил). Value-checking для ОБЕИХ утверждённых фикстуры-строк уже
подтверждено НАПРЯМУЮ: repro-бинарь (`repro377.exe`, standalone build,
см. репро выше) реально собран/запущен и печатает именно `21`/`21` для
`p.v`/`p.read().val()` — это и есть runtime-подтверждение assert'ов
фикстуры, независимо от того, успеет ли доехать whole-folder мега-сборка
за отведённое время.

## Ужесточения — регресс-проверка

- `nova check std/src` — **148 PASS / 26 FAIL / 61 WARN** — БАЙТ-В-БАЙТ
  канон плана, НЕ сдвинулся (ни после №375/№367/№368-фиксов, ни после
  корпус-миграции каста).
- `nova check examples` — **46 PASS / 1 FAIL** — единственный FAIL
  (`examples/tls/echo_server.nv:59:50: undefined identifier session`) —
  ПРЕ-СУЩЕСТВУЮЩИЙ, НЕ связан с указателями (name-resolution баг,
  вероятно тот же класс, что №336 «лок не фиксирует версию» — nova-tls
  через lockfile); не трогал, вне scope окна.
- `../nova-tls`/`../nova-http`/`../nova-polaris`/`../nova-bignum` —
  грепнуты на `as *mut` (см. №375 выше), кода не менял (СВОИМ бинарём не
  гонял полный `test` по пакетным репам — вне explicit мандата окна,
  задачи 375/367/368 не language-changing для ЭТИХ репозиториев).
- `cargo build` (`compiler-codegen` + `nova-cli`, release) — чисто, 0
  новых warning'ов сверх baseline.
- `scripts/guards/arch-ratchet.sh` — `lines=64171<=64171 ok`,
  `infer=348<=348 ok` — БЕЗ сдвига (весь фикс — checker-канал, `emit_c.rs`
  не тронут).
- Мега-CU (672/0/69) и флагман (`examples/flagship/aggregator`
  `--strict-effects`) — интегратору (стоячее правило окна).

## Амендмент-текст (спека НЕ правилась мной)

Спека для №375 (D216 §4 AMEND, D246 «Восстановление §V2.6 — частично
отменено») уже дописана координатором на main (`96100421e`) ДО начала
моей реализации — привожу здесь для протокола, что РЕАЛИЗАЦИЯ покрывает
ВСЕ 4 сформулированных там пути (вывод/аннотация/параметр-поле-
возврат/каст) плюс регрессирует существующие №349/№353 write-проверки.

№367/№368 — НЕ language-changing амендменты: №367 закрывает
enforcement-пробел УЖЕ действующего правила (D216/174.5 retraction —
операторные read-формы были задуманы ретрактированными изначально,
просто чекер молчал); №368 — чистый compiler bug (inference gap), не
меняет ни синтаксис, ни семантику языка. Новый код `E_POINTER_MUT_FROM_
RO_SOURCE` (№375) и текст `read-parity` на существующем
`E_POINTER_OP_USE_METHOD` (№367) — оба в семье уже задокументированных в
`spec/decisions/02-types.md` кодов (D216/D246/Plan 174.5), отдельного
D-номера не требуют.

№377 — амендмент НЕ написан (не мой мандат): ЕСЛИ владелец выберет
вариант B (ретракция auto-deref для методов) — потребуется правка D216
§5 карты «ОСТАЮТСЯ операторами» (сузить `auto-deref p.field/p.method()`
до одного `p.field`) в ТОМ ЖЕ слиянии, что и фикс/ретракция. Если вариант
A (точечный emit_c.rs фикс) — амендмент не нужен, спека уже верна, чинится
только реализация.

## №TBD-находки

Нет новых находок сверх переданных координатором №375-доп/№377 (уже
интегрированы в фикс/раздел выше). Финальная регресс-проверка (std/src,
examples, arch-ratchet, cargo) — см. «Ужесточения» выше, без сюрпризов.
