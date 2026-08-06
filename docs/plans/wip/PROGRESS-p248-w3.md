<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# PROGRESS p248-w3 — 11 атомиков на «значение внутри» + TcpStream.rc указателем (волна 3 из трёх)

Модель: sonnet. Волна 3 план 248: перевод 11 атомиков (`std/src/runtime/
sync.nv`) с `type X(*())` (pointer-newtype, куча) на `#no_copy type X value
priv { v T }` (значение внутри, D447) + `TcpStream.rc` (`std/src/net/tcp.nv`)
на указатель `*mut AtomicInt` (единственное место в std, которому реально
нужна общая ячейка).

Worktree `d:/Sources/nv-lang/nova-p248w3` (ветка `p248-w3`). Бинарь
`d:/Sources/nv-lang/nova-p248w3/nova-cli/target/release/nova.exe`.

---

## Объём — что переведено

### 1. 11 атомиков — представление

`std/src/runtime/sync.nv`: `AtomicI64/I32/I16/I8`, `AtomicU64/U32/U16/U8`,
`AtomicInt`, `AtomicUint`, `AtomicBool` — все 11:

```nova
#stable(since = "0.1")
#share
#no_copy
export type AtomicI64 value priv { v i64 }
```

(поле `v` — ширина по типу: `i64/i32/i16/i8/u64/u32/u16/u8/int/uint/bool`).
`#share` сохранён (D415 §0 применим к value-record kind — record — так же,
как к newtype). Сигнатуры extern-методов (`load`/`store`/`swap`/`fetch_*`/
`compare_exchange[_weak]`) — **не тронуты**, публичный API байт-в-байт
совместим по вызову (только представление изменилось).

Остальные 8 примитивов (`Mutex`/`RwLock`/`ReentrantMutex`/`WaitGroup`/
`Once`/`OnceCell`/`Lazy`/`Barrier`/`Condvar`/`CountDownLatch`/`Semaphore`) —
**не тронуты** (вне объёма волны, per брифу). `CancelToken` — не тронут.

### 2. C-рантайм

`compiler-codegen/nova_rt/sync_primitives.h`, блок строк 192-826 (все 11
атомиков): struct-typedef переименован `Nova_AtomicXXX` → `NovaValue_
AtomicXXX` (поле `value` → `v`, для совпадения с `.nv`-стороны именем поля,
на которое лоуэрится `@v`), конструктор (`_static_new`) — с `nova_alloc` +
возврат указателя на возврат СТРУКТУРЫ ПО ЗНАЧЕНИЮ (стековая локальная,
без аллокации). **Имена функций не менялись** (`Nova_AtomicXXX_static_new`/
`Nova_AtomicXXX_method_*` — конвенция кодогена держит имя функции по
Nova-имени типа, не по C-typedef'у структуры, byte-identical).

Переименование `Nova_X` → `NovaValue_X` (не `Nova_X` как раньше) —
**обязательное**, не косметика: вся инфраструктура value-record'ов в
кодогене (`is_value_struct`/receiver-ABI/generic Option-Result wrapping,
§0 единый источник `emit_c.rs`) распознаёт ИМЕННО префикс `NovaValue_` —
рукописная структура должна называться так же, иначе кодоген для value-
kind типов её не узнаёт (см. §3 ниже — реальная находка, не гипотеза).

### 3. Компиляторные фиксы (НЕ в брифе буквально, но обязательные — иначе
### представление не работает вообще)

Брифом предполагалось «меняется только размещение и привязки extern» —
на практике вскрылся класс компиляторных пробелов: **RUNTIME_DEFINED_TYPES
+ Record+Value одновременно** — комбинация, которой в корпусе не было
никогда (RUNTIME_DEFINED_TYPES раньше — либо pointer-newtype, либо sum).
Всё — в чекер/кодоген-канале, `emit_c.rs` не трогался бы, если бы не эта
комбинация. Правки (`compiler-codegen/src/codegen/emit_c.rs`):

1. **`RUNTIME_DEFINED_TYPES` не содержал `"AtomicBool"`** — пред-
   существующая дыра (Newtype-кинд гейтился отдельным списком
   `debt_is_runtime_backed_newtype`, который `AtomicBool` уже содержал;
   этот, более общий, список — нет, пока тип не стал Record-кинд).
   Добавлено.
2. **Ранняя регистрация `type_aliases`/`value_record_names`** в fwd-decl-
   loop'е (до `RUNTIME_DEFINED_TYPES`-`continue`) — без неё
   `emit_fn_forward_decl` (метод-декларации ДРУГИХ модулей) падал в
   generic-фоллбэк `Nova_<Name>*` (репро: `compare_exchange`'s `nova_self`
   объявлялся `Nova_AtomicI64*`, необъявленный тип).
3. **Новая ветка в `emit_type_decl`'s RUNTIME_DEFINED_TYPES-гейте** —
   Record+Value подслучай, зеркалит уже существующий NamedTuple-подслучай
   рядом (`record_schemas`/`record_field_order`/`value_struct_field_tys`/
   `type_aliases`/`value_record_names`, без struct-body emit).
4. **Хардкод-имя в `resolved_named_to_c`** (11 имён → `NovaValue_<name>`,
   порядконезависимый дубль п.2 — нужен, когда МОДУЛЬ, ссылающийся на тип
   ПО УКАЗАТЕЛЮ (`std/net/tcp.nv`'s `TcpStream.rc *mut AtomicInt`),
   обрабатывается раньше sync.nv в CU — без него `*mut AtomicInt` лоуэрился
   в `Nova_AtomicInt**`).
5. **Тот же хардкод в `receiver_c_type`'s `other =>`** (по тому же
   порядковому основанию).
6. **`debt_is_guaranteed_struct_tag` exclusion** — 11 имён hand-written
   АНОНИМНЫХ структур исключены из generic `starts_with("NovaValue_")`-
   детекции «pointee = гарантированный struct-тег» (иначе forward-decl
   `typedef struct NovaValue_AtomicI64 NovaValue_AtomicI64;` конфликтует
   с анонимным `typedef struct { ... } NovaValue_AtomicI64;` из хедера —
   "typedef redefinition with different types", поймано реальной сборкой
   `std/net`, `TcpStream.rc`).

**`arch-ratchet` вырос 64171→64305 (+134)** — baseline обновлён В ЭТОМ ЖЕ
слиянии с обоснованием (`scripts/guards/arch-ratchet.baseline`), ПУТЬ B
(поднято исполнителем, ждёт ревью интегратора). `infer` не сдвинулся (348).

### 4. Отдельный найденный и починённый баг — `callnorm.rs` (не в
### `emit_c.rs`, не в ratchet-метрике)

AST-нормализация default/named-arg вызовов (`compiler-codegen/src/
callnorm.rs`) хойстила MUTATING receiver в `let __nova_recv = obj` перед
переписыванием в pointer-ABI вызов. На старой pointer-newtype форме
атомиков копия УКАЗАТЕЛЯ была безобидным алиасом; на новой value-inside
форме копия физически независима — мутация (`compare_exchange`) молча
терялась для оригинальной переменной caller'а (первый в корпусе value-
record с `mut`-ресивером И default-параметрами, реально вызванный без
всех аргументов). Фикс: `is_addressable_receiver` (зеркало `emit_c.rs`'s
`is_lvalue_receiver`, тот же предикат) — bare-lvalue-ресивер переиспользу-
ется напрямую, hoist остаётся только для side-effecting (rvalue) ресиверов.

### 5. `TcpStream.rc` → указатель

`std/src/net/tcp.nv`:
```nova
export type TcpStream consume value { priv handle *(), priv rc *mut AtomicInt }

fn TcpStream.from_raw(h *()) -> TcpStream {
    mut counter = AtomicInt.new(1)
    TcpStream { handle: h, rc: &counter }
}
fn share_copy(h *(), rc *mut AtomicInt) -> TcpStream => { handle: h, rc }
```

`counter` — heap-escape-promoted (D216 §4, тот же механизм, что уже
проверен для `mut`-захвата в `spawn`) обычным путём `&counter`; `@share()`/
`@close()` не менялись (`@rc.fetch_add(1)`/`@rc.fetch_sub(1)` — вызов метода
через указатель авто-разыменовывается, без `unsafe`, как и было обещано
разведкой p248-sharedcell).

---

## Прогоны — вердикты дословно

### `cargo build --release`

Чисто, `Finished \`release\` profile [optimized] target(s)`, без новых
warning/error (несколько пересборок за окно).

### `nova check std/src` — канон

```
===== SUMMARY =====
PASS: 148  FAIL: 26  WARN: 61
```
Байт-в-байт неизменно на протяжении всего окна (включая финальное
состояние).

### `arch-ratchet`

```
arch-ratchet ok: lines=64305 <= 64305
arch-ratchet ok: infer=348 <= 348
```
(baseline обновлён этим же слиянием, обоснование — в самом файле).

### `nova test std/src/runtime`

```
===== SUMMARY =====
PASS: 5  FAIL: 0  SKIP: 13 (skipped)
```
`sync_test.nv` — PASS (все существующие value-проверяющие тесты для
AtomicInt/AtomicI64/AtomicI32/AtomicUint/AtomicBool, включая compare_
exchange retry-loop идиому из Plan 207).

### `nova check std/src/net`

`ok` (весь `net`-модуль, включая `tcp.nv`, одна co-equal единица —
подтверждает `TcpStream.rc`-изменение компилируется). 3 неотносящихся к
волне `FAIL` в `neg/` (double_close/host_str_removed/split_after_use) —
байт-в-байт совпадают с прогоном на немодифицированном `main` (baseline).

### `nova test std/src/net`

**Заблокирован пред-существующим, НЕ относящимся к волне багом**:
`write_all`'s локальная `_nv_scr_*` объявляется `nova_unit` вместо
`NovaRes_nova_int_NovaValue_IoError*` (CC-FAIL). Байт-в-байт (тот же текст
ошибки, тот же файл `addr.c`, только смещённые номера строк) воспроизведён
на **немодифицированном `main`** — не регрессия этого окна. Из-за этого
рантайм-подтверждение `tcp_share_test.nv` (`@share()`/`@close()` refcount)
средствами `nova test` в ЭТОМ окне невозможно — статически подтверждено
(п. «TcpStream» выше + `nova check` чисто + прямой разбор порождённого C
для `from_raw`/`share_copy`, корректная адресация).

### Пакетные репы (свой бинарь `nova-p248w3`, `NOVA_STD_PATH` → свой `std/src`)

```
nova-polaris: check src --strict-effects → PASS: 55  FAIL: 0  WARN: 3134
nova-http:    check src --strict-effects → PASS: 4   FAIL: 3  WARN: 210   (3 FAIL — байт-в-байт baseline, неотносящиеся neg-фикстуры)
nova-tls:     check src --strict-effects → PASS: 1   FAIL: 1  WARN: 3    (1 FAIL — байт-в-байт baseline, неотносящаяся neg-фикстура)
nova-bignum:  грепом — Atomic* не используется вовсе (заражения НОЛЬ, как и предсказывала разведка)
```

`nova-polaris` **изначально давал 3 FAIL** (реальная регрессия волны —
см. находки B/C ниже), починено ДВУМЯ путями: (1) компиляторный фикс
(borrow-правило теперь пускает `mut`-параметр), (2) корпус-фикс
(`middleware/log.nv`: `fresh_id` инлайнен в `request_id_of` — снимает
многошаговую borrow-цепочку, которую чекер пока не умеет проверять
транзитивно). Итог — 0 FAIL.

### `nova lint`

```
std/src/runtime/sync.nv, std/src/net/tcp.nv, spec_tests/conformance/p248w3_atomic_value_inside_spawn.nv:
  lint: 3 file(s), 0 finding(s)
nova-polaris/src/middleware/log.nv:
  lint: 1 file(s), 1 finding(s)  — W_MANUAL_SLICE_TO_END, строка 169, ПРЕД-СУЩЕСТВУЮЩАЯ, вне участков этого окна
```

### Новая фикстура (`spec_tests/conformance/p248w3_atomic_value_inside_spawn.nv`)

Изолированный прогон (`nova test`, копия в свою директорию):
```
PASS: 1  FAIL: 0
```
3 test-блока: `AtomicInt.fetch_add` через `parallel for` (10 фиберов →
10), `AtomicI64.fetch_add` через 2 явных `spawn` (100+100 → 200),
`AtomicBool.swap` через захват в `spawn`. Все — значение-проверяющие
(`assert` на итоговое значение, не просто «компилируется»).

**compare_exchange НЕ включён** в фикстуру — см. находку D ниже (пред-
существующий баг, не связан с волной, репродуцирован и на `main`).

Существующие 8 фикстур D447 (волна 2, `d447_no_copy_second_name.nv` +
7 `neg/n_d447_*.nv`) — прогнаны изолированно ДО и ПОСЛЕ компиляторных
правок этого окна, поведение не изменилось (1 позитив PASS, 7 негативов
FAIL с ожидаемым текстом).

---

## Изменения публичного API (список)

- `std/src/runtime/sync.nv`: `AtomicI64`/`AtomicI32`/`AtomicI16`/
  `AtomicI8`/`AtomicU64`/`AtomicU32`/`AtomicU16`/`AtomicU8`/`AtomicInt`/
  `AtomicUint`/`AtomicBool` — декларация `type X(*())` → `#no_copy type X
  value priv { v T }`. Сигнатуры методов НЕ изменились. **Поведенческое
  отличие**: значение больше нельзя связать вторым именем (`ro b = a`,
  чтение в поле, передача НЕ-borrowing-параметром, встраивание в литерал)
  — компилируется в ошибку `E_NO_COPY_SECOND_NAME` там, где раньше молча
  копировался УКАЗАТЕЛЬ (тот же счётчик под двумя именами) либо, для
  cross-module-конструированных значений, гейт пока не видит (находка A).
- `std/src/net/tcp.nv`: приватное поле `TcpStream.rc` — `AtomicInt` →
  `*mut AtomicInt` (не публичный API, `rc` всегда был `priv`).
- Диагностика `E_NO_COPY_SECOND_NAME` — текст дополнен («не `ro`/`mut`-
  параметр» вместо «не `ro`-параметр», отражает находку B).
- `nova-polaris/src/middleware/log.nv`: удалена module-private функция
  `fresh_id` (инлайнена в `request_id_of`) — не публичный API пакета.

---

## Находки — №TBD (нумерация за интегратором)

**№TBD-A. `check_no_copy_second_name` (D447 wave 2) слеп к типам,
сконструированным CROSS-MODULE вызовом.** `NoCopyIndex` (`compiler-
codegen/src/types/mod.rs`) индексирует только `module.items` + `module.
peer_files` — типы/функции ТЕКУЩЕГО модуля. Для `mut a = AtomicInt.new(0)`
в модуле-потребителе (не `std.runtime.sync`) тип `a` не резолвится в scope
вообще (ни `resolve_path_type` — путь-only, ни `record_lit_type` —
RecordLit-only не покрывают `Type.new(...)`-вызов) → последующий `ro b =
a` НЕ ловится (проверено пробой: `nova check` — чисто там, где должна быть
ошибка). Это значит: Rule 1 (голый алиас) / Rule 2 (чтение поля) ПРАКТИЧЕСКИ
никогда не срабатывают для реального использования всех 11 атомиков (они
ВСЕГДА cross-module с точки зрения потребителя) — работают только когда
тип биндинга приходит из явной аннотации или ПАРАМЕТРА функции (тип
известен локально, без cross-module lookup).
Частично закрыто этим окном: `call_result_type` — новый хелпер в
`NoCopyWalk`, резолвит `Type.method(...)`-вызов через `idx.fns` (ТОЛЬКО
same-module — `ReceiverKind::Static` ключуется `(Some(type_name), method)`,
подтверждено чтением `Receiver`/`NoCopyIndex::build`). Закрывает узкий
same-module случай; НЕ закрывает атомики (по определению cross-module).
Полный фикс — threading `resolved_types`/аналогичного канала ГЛАВНОГО
чекера в `check_no_copy_second_name` (сейчас — свободная функция,
получает только `&Module`, без доступа к результатам основного прохода) —
отдельная волна, за пределы бюджета этого окна.

**№TBD-B. D447 §«Заимствование» требовал ГОЛЫЙ `ro`-параметр для borrow-
легальности — противоречит D246/Plan 184 P10.** `mut x T` для value-record
кинда (в точности множество, на которое `#no_copy` применим) — ПОИНТЕР
in-out ABI, не копия (Plan 184 P10, подтверждено чтением `param_is_auto_
byref`/`receiver-ABI` того же файла). Найдено реальной регрессией на
`nova-http`/`nova-polaris` (`fresh_id(mut counter AtomicInt)`/
`request_id_of(mut counter AtomicInt, ...)` — оба ложно отклонялись).
**Исправлено этим окном** (`types/mod.rs`, borrow-критерий): `!p.is_mut &&
!p.consume && ...` → `!p.consume && ...`. Диагностика обновлена. Спека НЕ
правилась — ниже готовый текст амендмента к D447 §«Заимствование».

**№TBD-C. Многошаговая borrow-цепочка (borrow пробрасывается ДАЛЬШЕ
аргументом БЕЗОПАСНОГО вызова) всё ещё считается эскейпом.** `nc_scan_expr`
(`Call`-арм) флагует ЛЮБУЮ передачу идентичности значения дальше аргументом
— даже если ПРИЁМНИК тоже только заимствует и не эскейпит (было бы видно
рекурсивным применением `nc_param_escapes` к ВЛОЖЕННОМУ callee). Найдено на
`nova-polaris`'s `request_id_of(counter, ...)` → `fresh_id(counter)` (обе —
безопасные `mut`-заимствования по отдельности, но вторая ступень эскейпит
по правилу текущей волны). НЕ исправлено в компиляторе (транзитивный
анализ — заметно больше объёма, риск глубины рекурсии/взаимной рекурсии) —
корпус переписан вместо этого (`fresh_id` инлайнен). Задокументировано как
сознательное консервативное ограничение v1 (симметрично остальным пунктам
«Что НЕ покрыто» волны 2).

**№TBD-D. `compare_exchange`/`compare_exchange_weak` CC-FAIL в реальных
идиомах использования — ПРЕД-СУЩЕСТВУЮЩИЙ баг, НЕ вызван этой волной.**
Единственные два НЕ-`extern` (обычное `.nv`-тело), `mut`-ресивер, много-
перегруженных-по-типу-ресивера (11 деклараций с одним именем) метода среди
атомиков. CC-FAIL «no member named 'compare_exchange'» (кодоген порождает
буквальный C `.compare_exchange(...)` — struct-member-access синтаксис,
не method-dispatch call) воспроизведён в ТРЁХ независимых обстоятельствах:
(1) вызов внутри тела `while`-цикла — репродуцировано ДАЖЕ на точной форме,
которую `std/runtime/sync_test.nv` УЖЕ успешно использует (explicit
`success:`/`failure:`), просто обёрнутой в тривиальный `while i < 1 { … }`;
(2) 2-позиционно-аргументная форма (оба ordering-параметра — дефолт);
(3) ресивер, захваченный (`mut`) в `spawn`-замыкание. **Все три
воспроизведены БАЙТ-В-БАЙТ идентично на немодифицированном `main`**
(старая pointer-newtype форма — тот же C-симптом, `cell->compare_exchange`)
— подтверждает: представление (`value`/pointer-newtype) ни при чём, баг
живёт где-то в диспетче перегруженных-по-receiver-типу Nova-body методов
(вероятный кандидат по документации `callnorm.rs`: `Sigs.instance_by_name`
намеренно «пропускает» нормализацию для неоднозначных ПО ИМЕНИ методов —
11 одноимённых `compare_exchange` на разных типах — но тогда КАК ИМЕННО это
приводит к сломанной C-эмиссии, а не к пропуску нормализации, не
прослежено до конца в рамках бюджета окна). Блокирует документированную
«retry-loop» идиому (собственный doc-comment типа называет её «the
motivating use case») — реальный, важный, но НЕ атомик-специфичный дефект.
Не включён в новую фикстуру (не работает надёжно ни в какой опробованной
форме внутри цикла/spawn — не выдавать за рабочее).

**№TBD-E (мелкая, не функциональный баг). `nova-polaris/src/net/serve.nv`'s
`RejectLog`-комментарий устарел.** Комментарий утверждает «every copy...
aliases the SAME two cells» — фактически копирования (второго имени) НЕТ
вовсе: `reject_log` объявлен ОДИН раз и захватывается (`mut`) в каждый
`detach{}`-блок (escape-promotion, безопасно, тот же механизм, что и
проверенный mut-capture-в-spawn). Код корректен, формулировка комментария
— нет (описывает механизм, которого не происходит). Не исправлено (не
функциональный дефект, вне приоритета времени окна).

---

## Амендмент к D447 §«Заимствование» (спека НЕ тронута — текст для интегратора)

Заменить:
> **Заимствование.** Передача `Affine`-значения в параметр — не копия,
> если параметр получателя `ro` (не `mut`, не `consume`) И тело получателя
> НЕ сохраняет его...

на:

> **Заимствование.** Передача `Affine`-значения в параметр — не копия,
> если параметр получателя `ro` ИЛИ `mut` (не `consume`) И тело получателя
> НЕ сохраняет его: не пишет в поле, не возвращает, не встраивает в
> литерал, не захватывает в замыкание/`spawn`/`detach`/`blocking`/
> `supervised`, не передаёт дальше аргументом. `mut`-параметр для
> `Affine`-типов (структурно — record/sum/named-tuple/newtype/opaque,
> то же множество, что применимо к `#no_copy` целиком) — указатель на
> слот вызывающего (D246 три оси мутабельности, Plan 184 §Р10 in-out ABI),
> НЕ копия — запрет второго имени касается копирования значения, а не
> заимствования по указателю независимо от `ro`/`mut`. Такая передача —
> заём, не перевязка, и остаётся законной. (Известное ограничение,
> задокументированное отдельно: борроу через ВТОРОЙ уровень вызова —
> «A заимствует у caller'а и передаёт то же заимствование B, который тоже
> только заимствует» — сегодня консервативно считается эскейпом; №TBD-C.)

---

## Файлы

- `std/src/runtime/sync.nv` — 11 деклараций атомиков (представление).
- `compiler-codegen/nova_rt/sync_primitives.h` — struct-рename + value-
  return конструкторы (11 типов, строки 192-826).
- `compiler-codegen/src/codegen/emit_c.rs` — RUNTIME_DEFINED_TYPES+Value
  регистрация (несколько точек, см. §3 выше); `arch-ratchet` +134,
  обоснование — в `scripts/guards/arch-ratchet.baseline`.
- `compiler-codegen/src/callnorm.rs` — `is_addressable_receiver` +
  receiver-hoist fix (default/named-arg normalization).
- `compiler-codegen/src/types/mod.rs` — borrow-правило (`mut` param) +
  `call_result_type` (same-module constructor-call scope inference).
- `std/src/net/tcp.nv` — `TcpStream.rc` → `*mut AtomicInt`.
- `nova-polaris/src/middleware/log.nv` — `fresh_id` инлайнен.
- `spec_tests/conformance/p248w3_atomic_value_inside_spawn.nv` — новая
  позитивная фикстура (3 значение-проверяющих теста, cross-fiber).
- `docs/plans/wip/PROGRESS-p248-w3.md` — этот отчёт.

---

## Что НЕ входило / оставлено интегратору

Мега-CU (`spec_tests/conformance`, единый compile-unit) и флагман
(`examples/flagship/aggregator --strict-effects`) — по брифу, зона
интегратора. Находки №TBD-A/B/C/D требуют решения владельца (объём
дальнейшей работы над D447-механизмом) — не решались за пределами
минимального фикса B (сама волна не могла оставаться регрессией на
корпусе) и документирования остальных.

---

## Приёмка интегратора (2026-08-06) — мега-CU 669/3, два регресса найдены и починены

Интегратор прогнал мега-CU на слиянии p248-w3: **PASS 669 FAIL 3**. Два
регресса вернулись на доработку, третий («№TBD-F» ниже) — пред-существующий,
задокументирован, не мой мандат.

### Регресс 1 — `a_q3_println_debug_record` CC-FAIL (2 места)

```
error: assigning to 'NovaValue_AtomicI64' from incompatible type 'NovaValue_AtomicI64 *'; dereference with *
error: passing 'NovaValue_AtomicI64' to parameter of incompatible type 'NovaValue_AtomicI64 *'; take the address with &
```

Файл в мега-CU объединяет ВЕСЬ `spec_tests/conformance` — реальный источник
(по номерам строк порождённого C) — `d172_realtime_blocking_attrs.nv`'s
`#blocking fn d172_blk_fetch_add(mut d172_a AtomicI64, delta int)`.

**Корень**: `emit_blocking_fn_call` (Plan 113/D172, `#blocking fn`
call-site → thread-pool offload, `compiler-codegen/src/codegen/emit_c.rs`)
вычисляла C-тип ctx-struct-поля для КАЖДОГО параметра через голый
`type_ref_to_c(&p.ty)` — НИКОГДА не применяя Plan 184 §Р10 (`mut x T` для
value/примитивного `T` — указатель `T*` в C, ЛЮБОЙ ДРУГОЙ call-путь уже это
делает: `synthesize_inout_refargs`, `emit_fn_forward_decl`). Невидимо ДО
этой волны: у старого pointer-newtype `AtomicI64` голый value-тип УЖЕ БЫЛ
указателем (`Nova_AtomicI64*`), отсутствие Р10-поправки ничего не меняло.
Теперь `AtomicI64` — настоящий value-record (`NovaValue_AtomicI64`) — поле
осталось VALUE-типизированным, а аргументы на call-site УЖЕ приходят
`RefArg`-обёрнутыми (Р10-обвязка апстрим в `emit_call`, ДО диспетча в
`emit_blocking_fn_call`) — то есть уже АДРЕСА. Несовпадение типа поля vs
типа значения — обе ошибки клэнга ровно про это.

**Фикс**: применить существующий хелпер `param_is_inout_ptr` к вычислению
типа ctx-поля (добавляет `*`, когда параметр `mut`+value/примитив). Аргумент-
заполняющий цикл НЕ трогался — он и так передаёт уже-адресное значение;
первая версия фикса добавляла ВТОРОЙ слой адресации там (`&(arg_c)`), что
давало `&(&(d172_la))` — поймано ДО коммита прогоном `d172_realtime_
blocking_attrs.nv` standalone (`initializing 'NovaValue_AtomicI64' with an
expression of incompatible type 'NovaValue_AtomicI64 *'`), откачено.

**Верификация**: `d172_realtime_blocking_attrs.nv` standalone — `PASS: 1
FAIL: 0`.

### Регресс 2 — `standalone/m240_detach_box_while_loop_read_after` RUN-FAIL

```
:107 assert failed: n == 3
:117 assert failed: n == 6
```

**Дифференциально проверено — НЕ вызвано `callnorm.rs`-фиксом** (отключение
куска не меняло результат; баг воспроизводится и с полностью откаченным
`is_addressable_receiver`).

**Реальный корень**: `emit_detach.rs`'s heap-boxing мутабельно-захваченных
в `detach{}` локалов (Fix №240, ДРУГОЕ окно, дочерний модуль `compiler-
codegen/src/codegen/emit_c/emit_detach.rs` — ratchet его не измеряет).
alloc+copy-в-бокс (`*bv = counter;`) стоит на C-позиции самого `detach{}` —
если эта позиция внутри `while`-тела, код ИСПОЛНЯЕТСЯ на каждой итерации. Для
СТАРОГО pointer-kind захвата (`Nova_AtomicI64*`) повторное копирование —
безобидно: копируется указатель, все копии алиасят ОДИН shared-объект. Для
value-inside `AtomicInt` (эта волна) `*bv = counter` КОПИРУЕТ ЗНАЧЕНИЕ —
каждая итерация тихо заводит НЕЗАВИСИМЫЙ свежий счётчик (снимок вечно-
неизменной `counter`, которая никогда не пишется обратно), более ранние
итерации мутируют СИРОТСКИЕ, более недостижимые через текущий указатель
бокса, ячейки. `runtime.drain_orphans()` + чтение после цикла видит только
ПОСЛЕДНИЙ, почти всегда нулевой, бокс.

**Фикс**: hoisted box-указатель инициализируется `NULL`
(`hoist_box_decl`); alloc+copy обёрнут в `if (!bv) { ... }` — boxing
происходит НЕ БОЛЕЕ ОДНОГО РАЗА за вызов содержащей функции (hoisted
declaration — обычная C-локальная, свежий `NULL` на каждом новом
стек-фрейме), все дальнейшие итерации И все прочие `detach{}`-сайты,
делящие тот же `var_boxed`-реюз, переиспользуют ТУ ЖЕ ячейку — ровно то же
поведение, что уже было у pointer-kind типов, теперь верно и для
value-kind.

**Верификация**: `standalone/m240_detach_box_while_loop_read_after.nv`
standalone — `PASS: 1 FAIL: 0` (все 3 под-теста: `n==3`/`n==1`/`n==6`).

### №TBD-F. `neg/neg_str_from_retracted` — NEG-NO-ERROR, ПРЕД-СУЩЕСТВУЮЩИЙ, не мой мандат

Третья строка мега-CU-фейла (669/3 = a_q3 + m240 + этот): `str.from(5)`
(Plan 174.2 ретракция) ожидает `EXPECT_COMPILE_ERROR`, но codegen проходит
без ошибки **только в мега-CU контексте** — изолированный прогон (`nova
check` на копии в свою директорию) даёт `ok` (нет ошибки) И на этой ветке,
И на немодифицированном `main` — байт-в-байт то же поведение. Не связано с
`Atomic*`/D447/этой волной вообще (файл не ссылается ни на что из моих
изменений); похоже на порядко-зависимый мега-CU-артефакт (какая-то другая
декларация в CU конкурирует за имя/резолв `str.from`). Не чинилось —
не моя область, не регрессия этой волны.

### Финальные каноны (последний синхронный прогон, ПОСЛЕ обоих фиксов)

```
nova check std/src:            PASS: 148  FAIL: 26  WARN: 61   (байт-в-байт)
nova-polaris check --strict-effects (свой бинарь): PASS: 55  FAIL: 0  WARN: 3134
cargo build --release:         чисто
```

### Ratchet

`lines` поднят ДО 64349 (+44 от моей предыдущей 64305, обоснование дописано
в `scripts/guards/arch-ratchet.baseline`) — ТОЛЬКО фикс регресса 1
(`emit_blocking_fn_call`, в самом `emit_c.rs`); фикс регресса 2 — в дочернем
модуле `emit_detach.rs`, ratchet-метрику не трогает. **По указанию
интегратора базу дальше НЕ трогаю** — у интегратора своя рабочая копия уже
на 64306 (+1 от параллельного слияния "pk2" поверх моей 64305); сведение
64306+44 — на интеграторе при мёрже.

### Файлы (дополнение к списку выше)

- `compiler-codegen/src/codegen/emit_c.rs` — `emit_blocking_fn_call`
  Р10-поправка ctx-struct-поля (регресс 1).
- `compiler-codegen/src/codegen/emit_c/emit_detach.rs` — box-once-guard
  (`hoist_box_decl` init `NULL` + `if (!bv)` вокруг alloc+copy, регресс 2).
- `scripts/guards/arch-ratchet.baseline` — новая запись обоснования
  (64305→64349), база НЕ финализирована (см. выше).
