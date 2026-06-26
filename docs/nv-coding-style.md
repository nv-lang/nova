<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova `.nv` — конвенция кодирования

> **Нормативный документ** — изменения и отклонения только по согласованию с владельцем; см. [conventions-governance.md](conventions-governance.md).

> **Директивный документ** для всех контрибьюторов (включая AI-агентов). Описывает,
> **как писать `.nv`-код** так, чтобы он совпадал по стилю с остальным `std/`. Дополняет
> [project-philosophy.md](project-philosophy.md) (как принимать решения) и
> [perf-conventions.md](perf-conventions.md) (модель стоимости).
>
> Правила выведены из паттернов, уже действующих в `std/runtime/string/` и `std/unicode/`.
> Каждое правило заземлено в реальном `file:line`. Правила 1, 2, 8 (специфичные для
> строк/Unicode) живут также в [strings.md](strings.md); правила 3–7, 9 — общеязыковые.
> Кросс-ссылки: правило 5 → [contracts.md](contracts.md); правило 6 → [parameters.md](parameters.md).
>
> §§11–22 (общеязыковые: эффекты/конкурентность 11–14, типы/композиция 15–17,
> мутабельность/владение 18–19, control-flow/ошибки/cleanup 20, именование 21,
> перегрузки 22) заземлены в `spec/decisions/` (D-блоки указаны в каждом правиле) и
> живом `std/`.

---

## 1. `as_*` / `to_*` / голое имя

- **`as_*` = O(1) zero-copy view или ленивый итератор**, заимствующий receiver.
  `str.@as_bytes()` возвращает реальную `ro []u8` реинтерпретацию (`core.nv:88`);
  `str.@as_chars()` строит `CharsIter` value-record за O(1) (`chars.nv:132`). `as_*` НИКОГДА
  не аллоцирует-и-копирует и НИКОГДА не потребляет (`consume`) receiver.
- **`to_*` = O(n) аллоцирующая владеющая копия.** `@to_bytes()`/`@to_chars()` аллоцируют
  свежий `[]u8`/`[]char` (`core.nv:61,255`). Имя на `to_` = вызывающий платит за аллокацию.
- **Ось `as_`/`to_` — это ВЛАДЕНИЕ (borrow vs copy), не стоимость.** Ленивые итераторы
  `as_chars`/`as_words`/`as_sentences` — O(1) создание. Если итератор по какой-то причине
  O(n) в создании, это нарушение — делайте его ленивым (как `WordsIter`/`SentencesIter` в
  Plan 91.18) либо переименовывайте в `to_*`. Не маскируйте O(n)-материализацию под `as_`.
- **Никакого голого `len`/индекса, прячущего O(n).** У `str` нет `s.len()` (три расходящиеся
  длины) и нет `s[i]` по codepoint — оба compile-error (`E_STR_NO_LEN`/`E_STR_NO_INT_INDEX`,
  `strings.md:71-82`). Голым остаётся только `byte_len()` (O(1)); codepoint/grapheme-счёт —
  через явный lens (`s.as_chars().count()`).
- **`consume`-финализаторы НЕ называются `as_*`.** Метод, потребляющий receiver и отдающий
  владение, — это `into_*` (Rust-идиома): `StringBuilder @into_str()`. `as_` зарезервирован
  за дешёвым non-consuming borrow.

## 2. ASCII vs Unicode в именах методов

- **Bare-имя = Unicode-семантика (под `import std.unicode`); `ascii` в имени = ASCII-only,
  table-free, из prelude.** `str @to_upper()` — Unicode full case (`case.nv`), `str
  @to_ascii_upper()` — ASCII A-Z (`transform.nv`). Без `import std.unicode` вызов bare-имени
  Unicode-метода — compile error (E7320), НЕ молчаливый ASCII.
- **На `char` ASCII-варианты — явные `@is_ascii_*`/`@to_ascii_*`** (`defaults.nv:58-81`),
  т.к. unqualified `@is_alphabetic`/`@to_uppercase` — Unicode (`core.nv:466,493`).
- **`eq_ignore_ascii_case` (ASCII, table-free, core) vs `eq_ignore_case` (Unicode fold,
  std.unicode)** (`core.nv:344` / `case.nv:136`). Правило: когда оба слоя дают один предикат
  на одном типе, ASCII-вариант несёт `ascii` в имени, Unicode владеет голым именем.
- **Резолюция одноимённых str-методов из двух stdlib-модулей сейчас НЕ диагностируется**
  (`check_extension_method_policy` early-return для всех stdlib-типов — std/prelude/runtime,
  str в их числе, types/mod.rs:5927) — избегайте коллизий именованием и документируйте
  правило резолюции.

## 3. Методы предпочтительнее свободных функций — и паттерн «фасад»

- **Публичный surface — методы** (`fn T @m()`). Предикаты/трансформы, вызываемые на значении:
  `char @is_alphabetic`, `str @eq_ignore_case`, `str @as_words`.
- **Свободные функции — слой реализации / cross-type**, не пользовательский surface.
  `char @method` — тонкий фасад, делегирующий в free fn над сырым скаляром:
  `char @is_alphabetic() => is_alphabetic(@ as int)` (`core.nv:466` → `category.nv:130`).
  Free fn уместна, когда (a) она работает над сырым `cp int`/`s str` до появления receiver'а,
  или (b) это module-private плумбинг (`is_cont`, `validate_utf8` — bare `fn`, без `export`).
- **N-арные чистые текст-трансформы, ПРОИЗВОДЯЩИЕ новое значение, могут оставаться free fn** —
  `collate_compare(a, b)` бинарна (receiver был бы произволен). Это **намеренный carve-out**
  (D254 для коллации: она НЕ должна выглядеть методом на str, чтобы не путаться с дефолтным
  byte-Ord). Документируйте такой выбор, чтобы он не читался как недосмотр. Lens-продюсеры
  и предикаты — всегда методы; нормализация/case (одноарные, дают новый str) — тоже методы.

## 4. panic / throw / Option / Result

- **panic = нарушение инварианта / баг, не recoverable.** Срез по не-границе cp `s[a..b]`
  паникует (ломать R-UTF8 — баг); non-panicking сосед — `s.get(a..b) -> Option[str]`.
  Несработавший `requires` → паника.
- **Option = genuine absence (отсутствие это норма).** `@find -> Option[int]`, `@strip_prefix -> Option[str]`,
  `CharsIter @nth -> Option[char]`, `@split_once -> Option[(str,str)]`, `env`/`parent`. **`Result → Option` через `.ok()`** — никаких `_opt`-имён.
- **Result = любая падающая операция (D325).** `str.parse_int -> Result[int, ParseIntError]`,
  `str.from_utf16 -> Result[str, Utf16Error]`, `str.try_from_codepoint`. Один структурный `XError` на домен.
- **🎯 Единый fallible-контракт std (D325, Plan 181) — Result-everywhere.** Любая падающая публичная операция → `Result`:
  - **(R1)** → `Result[T, <Domain>Error]`. Нет bare-throws-близнецов, нет `try_`-дублей, нет `_opt`.
  - **(R2)** Имя обычное, без префикса: `parse_int -> Result`, `read_u32 -> Result`, `open -> Result` (как Rust `str::parse`).
  - **(R3)** Префикс `try_` — **только** чтобы отличить fallible-вариант одноимённого **infallible**: `from`/`try_from`, `into`/`try_into` (D77). В одиночных fallible-операциях (нет infallible-сиблинга) префикса НЕТ.
  - **(R4)** `Option` — только genuine absence (`find`/`get`/`env`/`parent`), НЕ fallibility; `Result → Option` через `.ok()`.
  - **(R5)** Эффект `Fail[E]` в публичной std-сигнатуре запрещён для **собственных** ошибок (→ `Result`), но разрешён для прозрачного **проброса** `Fail[E]` из closure-параметра (effect-polymorphic forwarding: `retry`/`parallel`/`in_transaction` над телом пользователя).
  - Throw сохранён операторами (D85): `expr!!` (throw), `expr?` (проброс), `expr.ok()` (→Option), `match`. Эффект `Fail[E]` остаётся в языке (D25) — для пользовательского кода и внутренних хелперов; std им свои ошибки наружу не отдаёт. **Эталон:** `std/net` (Result-everywhere, 0 `Fail[`) — норма, не исключение.
  - **Миграция SHIPPED-форм** (`@try_parse_int`→`@parse_int`, удаление bare `@parse_int`/`@parse_int_opt`, ~20 `read_X`/`try_read_X` пар) — Plan 181 Ф.2 (compiler-gated части — Ф.2b).
- **lossy-FFFD (четвёртая категория) — ТОЛЬКО для функций, чьё имя это говорит** (`*_lossy`)
  или чей контракт best-effort (`cps_to_str`): подставляют U+FFFD. **Никогда не подставляйте
  пустую строку** как «успех» при невалидном входе — это потеря данных под видом успеха.

## 5. Максимизируйте контракты (Z3 их элидирует)

Доказанный `requires` — **zero-cost** (элидируется в compile-time, даже в debug,
`contracts.md:8`). Заявляйте каждое предусловие, которое можете.

- Границы и валидность — `requires` на каждом index/offset-параметре: `requires 0 <= i && i
  < @byte_len()` (только `&&`, никогда chain `0 <= i < n` → `E_CMP_CHAIN_UNSUPPORTED`). В
  методах ссылайтесь на состояние receiver через `@field`/`@byte_len()`.
- **Параметр-only `requires` особенно ценны** — Z3 снимает их на литеральных аргументах
  (`requires radix>=2 && radix<=36` исчезает на `parse_int_opt(16)`, `parse.nv:63-64`).
  `requires len>=0` на `@truncate(len)`/`@append_repeat(s,n)`.
- **НЕ добавляйте `ensures result >= 0` на size-accessors** (`byte_len`/`len`/`cap`/`count`) —
  non-negativity уже встроена в SMT-бэкенд как аксиома (`z3.rs:547-558`, План 33.6); это
  no-op, ничего не доказывает downstream. Аналогично Vec `@len()` намеренно без такого ensures.
- **НЕ пишите `ensures` с вызовами произвольных non-`#pure` user-функций/методов**
  (`types/mod.rs:18585`) и **tuple-выражениями в `ensures`** (E2401). Встроенные
  size-аксессоры `@len()`/`@cap()`/`@byte_len()`/`@is_empty()` (и форма на параметре
  `s.byte_len()`) — РАЗРЕШЕНЫ (хардкод-вайтлист, `encode.rs:230`). `ensures` — для отношений,
  выразимых через чистые термы.
- Помечайте side-effect-free helper'ы `#pure`, ЕСЛИ хотите звать их **внутри других
  контрактов** (`contracts.md:314`); напр. `is_ascii_ws`/`is_cont` сейчас НЕ аннотированы —
  пометь их `#pure`, прежде чем использовать в `requires`/`ensures`.
- Контракты стоят **между сигнатурой и `{`**, не внутри тела.

## 6. Компактный стиль

- **Tuple-binding для параллельных чтений:** `ro (sn, pn) = (@byte_len(), prefix.byte_len())`
  (`search.nv:20`); `ro (cp, step) = decode_at(s, i)`. Без длинных строк.
- **`+=`/`-=` вместо `a = a + 1`** для счётчиков/курсоров.
- **Операторы вместо `.compare()`/`.equal()`.** `str` синтезирует `< <= > >= == !=` из
  `@compare`/`@equal` — пишите `a < b`, не `a.compare(b) < 0`.
- **Цепочки вызовов / fluent `-> @`.** Чейньте через self-returning мутаторы:
  `[]u8.with_capacity(n).append(@as_bytes())`; `StringBuilder.with_capacity(n).append(x)
  .append(y).into_str()`. Метод, имеющий смысл в цепочке, объявляйте `-> @`, не `-> ()`.
- **Без `;` как разделителя** и **без нескольких операторов на строке через пробел**
  (`term = 11 phase = 0` — запрещено так же, как `;`). Один оператор — одна строка.

## 7. RawMem / bulk-copy вместо push-циклов

- **Стройте владеющие буферы одной аллокацией + bulk `append`, не push-циклом.**
  `@to_bytes` = `with_capacity(byte_len()).append(@as_bytes())` — один `RawMem.copy`
  (`memmove`), без цикла (`core.nv:61-67`).
- **Сравнение/скан через `RawMem.compare` (`memcmp`), не ручной byte-loop.** `@compare`,
  `@equal` (`core.nv`); `@starts_with`, `@ends_with`, `@contains`, `@find`, `@split`
  (`search.nv`).
- **Всегда пре-сайзьте.** `[]u8.with_capacity(n)`/`StringBuilder.with_capacity(n)` до любого
  заполнения; не растите инкрементально, если финальный размер известен. Для таблиц-карт
  (case/decomp) парсите один раз в lazy-static `HashMap[int, str]` (packed-str значение
  декодируется лениво через `parse_cp_list`), не per-cp.
- Остаточный push-цикл допустим только на честно нерегулярном пути (lossy UTF-8 FFFD-замена,
  `core.nv:207-219`) — и документируется как таковой.

## 8. Имена итераторов/view — единый суффикс `*Iter`

- **Все ленивые/потоковые декодеры и адаптеры заканчиваются на `Iter`**: `CharsIter`,
  `CharIndicesIter`, `VecIter`, `RangeIter`, `MapIter`/`FilterIter`/`TakeIter`/
  `EnumerateIter`/`StepByIter`, и сегментаторы Unicode `GraphemesIter`/`WordsIter`/
  `SentencesIter`. **`*View` НЕ используем** — суффикс `View` ошибочно намекает на
  random-access окно (Rust никогда не говорит `GraphemeView`). Сегментаторы — это стримы с
  `next()`/`count()`/`is_empty()`, ровно как `CharsIter`.
- **Все такие типы — `value` записи** (стек, без per-iteration heap-churn), никогда не
  heap-класс. String/unicode-итераторы (CharsIter, CharIndicesIter, Graphemes/Words/
  SentencesIter) — `value priv(type)` (курсор закрыт type-privacy); коллекционные (VecIter,
  RangeIter, Map/Filter/Take/Enumerate/StepByIter) — field-level `priv`/`_`-курсор.
- **Каждый такой тип — свой `@iter()` self-iterator** (`CharsIter @iter() => @`), чтобы
  `for x in it` и `it.iter()` шли одним for-in-путём (D58).

## 9. Peer-модули для общих helper'ов

- **Папка = ОДИН модуль; peer `.nv` делят все декларации** без intra-module import.
  `std/runtime/string/{core,chars,search,transform,parse,slice}.nv` — все `module
  runtime.string`, свободно зовут helper'ы друг друга. Большой модуль дробите на peer'ы
  **по роли**, заявленной в header-комментарии файла.
- **Генерируемые таблицы — peer-файл, не import.** `case.nv` читает `FOLD_DATA` напрямую из
  со-равного `case_data.nv`. Держите машинно-генерируемые `*_data.nv` как same-module peer'ы.
- **Общий private helper живёт ОДИН раз, используется всеми peer'ами.** `decode_at`,
  `range_lookup3`, `parse_flat`, `cps_to_str` — объявлены в одном unicode-peer и переиспользуются.
  Не дублируйте helper между peer'ами; объявляйте в наиболее релевантном файле и зовите.
  Шарьте общие helper'ы через выделенные peer-файлы (`ranges.nv`, `cp_utils.nv`), а не
  размазывая по семантическим.
- **Дублирование decode/парсеров между РАЗНЫМИ модулями неизбежно** (граница folder-модуля +
  `#no_prelude` запрещают шарить private fn) — но внутри ОДНОГО модуля копия должна быть ровно
  одна. Если копия вынужденная (cross-module), документируйте это комментарием, чтобы будущий
  читатель не ввёл нелегальное ребро между модулями.
- **Разрывайте import-циклы через `#no_prelude`**, не инлайнингом. Весь `runtime.string` —
  `#no_prelude` (разрыв цикла prelude→string→prelude), импортирует только точные нужные
  prelude-элементы.

## 10. `for in` вместо `while`, где это возможно

- **Итерация по диапазону/коллекции — всегда `for in`, не `while` со счётчиком.**
  `for i in 0..n { … }`, не `mut i = 0` + `while i < n { …; i += 1 }`.
  `for x in xs { … }`, не индексный `while` с `xs[i]`. for-in выражает намерение,
  исключает off-by-one и забытый инкремент, идёт единым D58-путём
  (`@iter()` → `next()`).
- **Нужен индекс — `.iter().enumerate()` / `.as_chars().indices()`,** не ручной счётчик:
  `for (i, x) in xs.iter().enumerate() { … }`; `for (i, c) in s.as_chars().indices() { … }`.
  (`@enumerate` есть только на iterator-адаптерах, не на голом Vec — `vec/iter.nv:32`;
  `char_indices()` не существует, канон — `as_chars().indices()`, `chars.nv:164,178`.)
- **Шаг/реверс — методы Range,** не арифметика в `while`: `(0..n).step_by(2)` /
  `(1..=n).reverse()` вместо `i += 2` / `i -= 1` в `while` (range в скобках +
  метод; `std/collections/range.nv:143,179`).
- **`while` оставляйте ТОЛЬКО для честно не-итераторных циклов** — условие не
  сводится к проходу по диапазону/коллекции: координационные циклы (ожидание
  события/флага), парсеры с переменным шагом курсора, fixpoint до сходимости.
  Такой `while` документируйте, если неочевидно, почему это не `for in`.
- Существующий код с индексным `while` (`canonical_order`/`compose` в
  `unicode/normalize.nv` — insertion-sort/fold с нелинейным курсором) —
  пограничный: переписывать только если `for in` не теряет ясность.

## 11. Эффекты — контракт сигнатуры: явно у `export`, выводимо у private

- **В публичных (`export`) функциях перечисляй ВСЕ прямые эффекты явно** —
  PascalCase-типы между `)` и `->`. Сигнатура есть полный контракт побочных эффектов
  для читателя/LLM: одна строка заменяет чтение тела и весь набор DI-полей (D3,
  `spec/decisions/04-effects.md:191-224`). Реально в stdlib:
  `std/net/tcp.nv:97` — `export fn TcpListener.bind(addr SocketAddr) TcpNet -> Result[…]`;
  `examples/effect_density/service.nv:45-47` — 10 эффектов в одной сигнатуре.
- **В private-функциях эффекты МОЖНО опустить** — компилятор выведет их из тела (D28,
  `04-effects.md:1251-1351`). Писать их явно тоже допустимо и в stdlib частая практика:
  `std/encoding/base64.nv:191` — `fn decode_with(…) Fail[Base64Error] -> []u8`. Опускай
  ради краткости, пиши явно ради читаемости — обе формы легальны.
- **Пустой effect-set у `export`-функции — это доказанный факт, но узкий.** Компилятор
  доказал отсутствие ПРЯМЫХ обращений к эффект-операциям (D62, `04-effects.md:1253-1262`).
  Он НЕ доказывает транзитивную чистоту: функция может через вложенный вызов всё же
  затронуть эффект — это даёт лишь warning, не ошибку (`04-effects.md:1304-1316`). Жёсткую
  гарантию полной чистоты (мемоизируемость, любой поток) даёт ТОЛЬКО `forbid` (D63, см. §14).

```nv
// public — ВСЕ прямые эффекты явно (контракт, D3/D28)
export fn transfer_money(
    req TransferRequest
) Db Cache Logger Clock IdGen AuthContext Metrics Trace Idempotency Fail[TransferError] -> Transfer =>
    Trace.span("transfer_money") {
        if !AuthContext.has_role("transfer") { throw TransferError.Forbidden }
        do_transfer(req)
    }

// private — эффекты МОЖНО опустить, компилятор выведет
// (Db Cache Logger Clock IdGen Metrics Fail[TransferError], D28);
// явно тоже легально (ср. base64.nv:191).
fn do_transfer(req TransferRequest) -> Transfer {
    validate_amount(req.amount)
    Db.in_transaction(|| { /* ... */ })
}

// чистая public-функция: пустой effect-set = нет ПРЯМЫХ эффект-операций (D62).
// полная гарантия чистоты — через forbid (D63, §14), не сам по себе пустой список.
export fn double(x int) -> int => x * 2
```

## 12. Suspension невидима — пиши последовательный код, без async-цвета

- **Никаких `await`/`Future[T]`/async-аннотаций.** Suspending-функция вызывается и
  связывается ТОЧНО как синхронная: `ro r = stream.read_bytes(n)` даёт `Result[…]`, не
  `Future[…]`. Один синтаксис вызова для sync и suspending — рефакторинг и LLM-генерация
  не воюют с function-color (D14 REVISED, `06-concurrency.md:45-48`; таблица сравнения с
  Rust `06-concurrency.md:145-153`: цвет функции «нет», await «не нужен», тип возврата «T»).
- **`Async` — не эффект и не часть системы типов.** Он убран из набора эффектов и стал
  ambient runtime-инфраструктурой (D62, `04-effects.md:2690-2708`: «Async — ambient
  capability, не эффект»). Suspension существует как runtime-факт, просто невидима в типах;
  отвергнутые альтернативы названы прямо (`06-concurrency.md:176-180`: `Future<T>` в
  возврате, `async`/`await` keywords, `Async` как эффект — все отвергнуты).
- **Единственный способ объявить «здесь НЕ должно быть приостановки» — `#realtime fn` /
  `realtime { }`** (D64), а НЕ маркер на call-site. Park в `#realtime`-контексте —
  `E_REALTIME_SYNC_PARK`.

```nv
// suspending net-вызовы связываются ТОЧНО как sync — никакого await, тип = T (не Future[T]).
// реальные вызовы из examples/net/echo_server.nv:31-48.
fn handle(lst TcpListener, mut stream TcpStream) TcpNet -> () {
    ro acc_r = lst.accept()             // suspending, синтаксис как у sync
    ro rd_r = stream.read_bytes(4096)   // тип Result[…], не Future[…]
    ro wr_r = stream.write("echo_ok\n") // тот же `ro x = call()`, что и для чистой fn
    ()
}

// гарантия НЕ-приостановки даётся блоком/атрибутом, а не маркером на call-site (D64):
#realtime fn rt_only(latch mut CountDownLatch) {
    latch.await()                       // E_REALTIME_SYNC_PARK: park запрещён в #realtime
}
```

## 13. Structured concurrency: `spawn` только в scope, `parallel for` как fan-out-выражение

- **`spawn` допустим лишь внутри structured-scope** (supervised / `parallel for` / select)
  и возвращает unit — связать его результат нельзя (`06-concurrency.md:294-348`;
  `ro r = spawn fetch_a()` помечено compile-error на `:307`). Результаты бери по форме задачи:
  последовательность → прямой вызов; гомогенный fan-out → `parallel for`; гетерогенное →
  общие `mut`-захваты в `supervised`.
- **Гомогенный fan-out — `parallel for x in iter { f(x) }` как выражение типа `[]T`**
  (ждёт всех, отменяет хвост при ошибке; D71, `06-concurrency.md:87`, тест
  `nova_tests/concurrency/parallel_for_array.nv:24`). Не нужен ручной join.
- **Внешняя отмена — `supervised(cancel: tok)`**; наличие `cancel:` самодокументирует
  отменяемость (`std/concurrency/cancellation.nv:117`).
- **`detach { }` — fire-and-forget orphan.** Spec-intent — эффект `Detach` в сигнатуре, но
  в bootstrap он компилятором пока НЕ требуется (D71, `06-concurrency.md:729`).
- **first-wins / таймаут** в bootstrap — это stdlib-ФУНКЦИИ `race2(a, b)` и
  `with_timeout(ms, body)` (`std/concurrency/cancellation.nv:113,156`), НЕ block-синтаксис.
  Блок-формы `race { }` / `with_timeout(d) { }` / `select { }` числятся нереализованными
  (D71, `06-concurrency.md:258-259`) — подавай их как будущий spec-intent, не готовый синтаксис.

```nv
// spawn возвращает unit и живёт только в structured-scope.
// гомогенный fan-out — parallel for как []T-выражение:
ro responses []Response = parallel for url in urls { fetch(url) }

// гетерогенная параллельность — общие mut-захваты в supervised:
mut a = 0
mut b = 0
supervised(cancel: tok) {
    spawn { a = compute_a() }   // ✓ spawn внутри scope
    spawn { b = compute_b() }
}
use_both(a, b)

// ✗ compile-error: spawn возвращает unit, связать нельзя
//   ro r = spawn fetch_a()
// ✗ compile-error: spawn вне structured-scope запрещён
```

## 14. `forbid X { body }` — capability-sandbox для доказуемого отсутствия эффектов

- **Когда подсистема ОБЯЗАНА не трогать определённые эффекты** (детерминированные
  вычисления, плагины, недоверенный код) — оборачивай в `forbid X, Y { body }` (D63,
  `04-effects.md:3599`; грамматика `:3771`). `forbid` — value-producing expression, его
  результат можно связать (`ro r = forbid … { … }`).
- **Запрет непреодолим compile-time** (использование forbid-эффекта или установка handler'а
  внутри → ошибка компиляции; enforcement в `compiler-codegen/src/types/mod.rs` через
  `CapabilityCtx`/`forbidden_stack`); runtime-sentinel описан в D63 (`04-effects.md:3680`).
- **`forbid` принимает только effect-типы**, не протоколы/записи (`forbid Hash` → ошибка,
  `:3751`). **`forbid Async` запрещён** — `Async` не type-эффект (`:3738`); для запрета
  приостановки используй `#realtime` (см. §12).

```nv
// детерминированное вычисление: провабельно без часов/случайности.
// Time и Random — реальные prelude-эффекты (std/prelude/effects.nv:137,
// std/testing/handlers.nv:64). тест: nova_tests/effects/forbid_realtime.nv.
ro result = forbid Time, Random {
    pure_double(21)        // вызов Time.now()/Random.next() внутри = compile error (D63)
}
assert(result == 42)

// запрет throw'а (Fail — реальный prelude-эффект):
ro r = forbid Fail {
    10 + 20                // любой throw здесь = compile error
}

// forbid Async — НЕЛЬЗЯ (Async не type-эффект); для запрета приостановки — #realtime (§12).
```

## 15. `value`/tuple vs heap record — форма объявления кодирует размещение

- **`type X(...)` (tuple, D215) и `type X value { ... }` (value record, D228) = стек-value,
  копия при передаче.** Для hot-path-математики, FFI-возвратов, состояния итератора (см. §8),
  мелких короткоживущих агрегатов. Bracket choice явно кодирует размещение
  (`02-types.md:2466`: `()` = stack, `{}` = heap; `value` — contextual keyword, парсер
  `parser/mod.rs:3546`). В stdlib: `std/collections/range.nv:58` `export type Range value {…}`,
  `std/net/addr.nv:34`, `std/prelude/protocols.nv:278`.
- **`type X { ... }` = heap-reference**, указатель при передаче + GC-tracking. Для
  domain-сущностей с identity/шарингом и крупных агрегатов (`02-types.md:2446-2454`).
- **АНОНИМНЫЙ мономорфизированный tuple >5 полей ИЛИ >128 байт → W-warning** «переведи в
  named record» (D123, `02-types.md:3909`). Lint про анонимные mono'd tuple, не про
  именованные `value`-record.
- **Поля value-record — на отдельных строках** (stdlib-стиль, как `Vec3`/`Range`); inline
  `value { x f64, y f64 }` тоже валиден, но на одной строке запятая обязательна (D215 amend).
- Cross-ref §8: типы итераторов обязаны быть `value` — частный случай этого общего правила.

```nv
// value record — стек, копия при передаче, zero GC (hot-path math, FFI, iterator state):
#impl(Clone)
type Vec3 value {
    x f64
    y f64
    z f64
}
// positional/named tuple — тоже стек-value (D215):
type Complex(re f64 = 0.0, im f64 = 0.0)

// heap record — указатель, GC-tracked, reference-семантика (domain identity / sharing):
type AccountId u64                         // newtype для domain-safety
type Money { amount i64, currency str }    // минорные единицы
type Account {
    id       AccountId
    balance  Money
}
// lint (D123): АНОНИМНЫЙ mono'd tuple >5 полей ИЛИ >128 байт → W-warning «переведи в record».
```

## 16. Композиция через `protocol`/`use` — без наследования и orphan-rule

- **Поведение крепи структурными `protocol`** — соответствие автоматическое: любой тип
  с подходящими сигнатурами удовлетворяет, без impl-блоков и без orphan-rule
  (`std/prelude/protocols.nv:81,199,460`). Эффекты в сигнатурах протокола разрешены и делают
  их строже Go-интерфейсов (D122 amended, `protocols.nv:135-138`).
- **Общее поведение встраивай через `use name Type`** — делегация с авто-прокси, компилятор
  инлайнит (zero-cost, никакого vtable), это НЕ подтип (D39, `02-types.md:1878,1920-1942`;
  реально `std/collections/set.nv:43` `use map HashMap[T, ()]`). Каждый `use name Type` — на
  ОТДЕЛЬНОЙ строке, без запятой (record-поля newline-separated, D39:1896,1930). Alias —
  нейтральный snake_case (`account`), не `base` (D39 строго anti-subtyping).
- **Не переопределяй чужой метод с той же сигнатурой на том же receiver** —
  `E_METHOD_REDEFINITION` (`nova_tests/plan154/neg_override_str_to_lower.nv:1`). Расширяй
  новым именем / перегрузкой / newtype: own-метод на newtype, совпадающий по имени со
  встроенным, легален, т.к. ключ метода включает receiver-тип (`Locale.to_lower ≠ str.to_lower`).

```nv
// структурный protocol: любой тип с подходящими сигнатурами удовлетворяет
// автоматически — без impl-блока и без orphan-rule.
type Logger protocol {
    @log(msg str) -> ()
}

type Account { balance int }
fn Account @log(msg str) -> () => println(msg)   // Account satisfies Logger structurally

// делегация (НЕ наследование): use name Type — авто-прокси, zero-cost inline, не подтип.
type AuditedAccount {
    use account Account
    audit []str
}

fn main() -> () {
    ro aa = AuditedAccount { account: Account { balance: 100 }, audit: [] }
    aa.log("opened")        // auto-proxy → aa.account.log("opened")
    println(aa.balance)     // auto-proxy → aa.account.balance
}

// не переопределяй чужой метод с той же сигнатурой на том же receiver:
//   fn str @to_lower() -> str => "X"   // → E_METHOD_REDEFINITION
// вместо этого — newtype + own-метод (ключ Locale.to_lower ≠ str.to_lower):
type Locale { s str }
fn Locale @to_lower() -> str => "custom"
```

## 17. Universal generics на hot-path, экзистенциальные — для гетерогенности

- **`fn f[T Hash](x T)` = универсальный параметр** → статический мономорфный диспетч,
  zero-cost, инлайнинг (горячий путь). Используется в stdlib: `[T Compare]`
  (`std/sort.nv:184`), `[K Hash + Equal]` (`std/collections/hashmap.nv:46`).
- **`fn f(x Hash)` = экзистенциальный** → динамический vtable-dispatch (реализовано,
  Plan 72 P3-B; тест `nova_tests/plan72/p3b_vtable_dispatch_pos.nv`). Различие только в
  позиции параметра (D72, `02-types.md:3460-3467`). Гетерогенный `[]Protocol` стоит heap-box
  (~16 байт) на элемент (`docs/simplifications.md:28027`) — бери ТОЛЬКО при реальной
  разнородности runtime-значений.
- **Bounds без двоеточия** (`[T Hash + Equal]`, multi-bound `+` — Plan 101.3); параметры
  объявляй слева-направо, **forward-ссылки запрещены** (`02-types.md:3354-3360,3423-3441`).

```nv
// universal: статический моно-диспетч, zero-cost, hot-path (как std/sort.nv:184):
fn[T Compare] []T @min_of() -> Option[T] {
    if @is_empty() { return None }
    mut m = @get(0).unwrap()
    for x in @ { if x.compare(m) < 0 { m = x } }
    Some(m)
}

// multi-bound через '+' (Plan 101.3), как std/collections/hashmap.nv:46:
export type HashMap[K Hash + Equal, V] { /* ... */ }

// existential: динамический vtable-dispatch — только для разнородных
// runtime-значений (heap-box ~16 байт per элемент). Как p3b_vtable_dispatch_pos.nv:
fn consume_iter(mut x Iterable[int]) -> int {
    mut count = 0
    loop {
        ro r = x.next()
        if r.is_none() { break }
        count += 1
    }
    count
}
```

## 18. Views по умолчанию, `mut` — opt-in, `consume` — для владения (3-осевая мутабельность)

- **Параметры/локали read-only по умолчанию** (D32 + D246, `02-types.md:3030-3039`): для
  чтения просто передавай значение (объект идёт по managed-ссылке); `mut` добавляй ТОЛЬКО при
  видимой вызывающему мутации (`std/unicode/collate.nv:46` `fn push_one_ce(mut acc Vec[u32], …)`);
  `consume` — только для передачи владения (`consume ⇒ mut` неявно).
- **Никаких `&T`/lifetimes/redundant-модификаторов.** `*ro T` → `E_REDUNDANT_POINTER_RO` (пиши
  `*T`, ведь `*T ≡ *ro T`, D246); `mut consume`/`consume mut` → `E_PARAM_MOD_CONFLICT`
  (parser-level). Сигнатура = контракт мутации.
- **Мутация bare-параметра — два разных stable-кода** (D176): вызов mut-метода
  (`b.push(1)`) → `E_PARAM_NOT_MUT` (`02-types.md:3022`); запись в индекс/содержимое
  (`v[0]=x`) → `E_READONLY_CONTENT` (`nova_tests/plan147/f7_neg2_ro_param_index_write.nv`).

```nv
// ro по умолчанию: bare-параметр = read-only view (объект по managed-ссылке)
fn show(acc Account) -> str => acc.summary()        // acc.id=… → E_READONLY_FIELD/CONTENT

// mut виден вызывающему; второй параметр bare = ro
fn deposit(mut acc Account, m Money) -> () {
    acc.balance += m.amount                          // ✓ mut binding
}

// consume = передача владения (consume ⇒ mut неявно); plan62:65, plan108_1
fn finish(consume sb StringBuilder) -> str => sb.into_str()

// запреты (stable error codes):
//   fn f(v []int) { v[0] = x }   → E_READONLY_CONTENT   (запись в content bare-param)
//   fn f(b []int) { b.push(1) }  → E_PARAM_NOT_MUT       (mut-метод на bare-param)
//   fn g(p *ro Acc)              → E_REDUNDANT_POINTER_RO (пиши *Acc; *T ≡ *ro T, D246)
//   fn h(consume mut x T)        → E_PARAM_MOD_CONFLICT   (parser-level)
```

## 19. `consume` — видимая линейная передача на каждом binding-site

- **`consume` — логический linear-qualifier**, память остаётся под GC (это НЕ
  Rust-ownership; как explicit `move` в Rust, но без lifetimes — D180 Industry comparison).
  Если RHS обязан к передаче владения, биндинг ОБЯЗАН быть `consume X = expr`:
  `ro X = consume-obligated-ctor` → `E_CONSUME_KEYWORD_MISSING` (D180 Rule 1,
  `05-memory.md:400-520`). В stdlib 40+ usages, напр. `std/runtime/string/transform.nv:192`.
- **`StringBuilder` affine**: потребить ≤1 раз, повторное использование после consume —
  ошибка компиляции (use-after-consume, flow-sensitive D131, `05-memory.md:290-396`; забыть
  OK — `:392`). Must-consume (≥1, забыть → error) — отдельная ось `type T consume` (D133).
- **Внутри тела нельзя alias-связать consume-переменную**: `ro twin = sb` →
  `E_VIEW_BINDING_FORBIDDEN` (D180 Rule 2) — это ДРУГОЙ код, чем Rule 1. Чтобы поделить
  владельца — передай как view-параметр на время вызова (D180 Rule 5).
- Финализатор `into_str()` именно consuming с buffer-steal (`std/runtime/string_builder.nv:173`
  `consume @into_str() -> str => str.from_bytes_unchecked_steal(@buf)`) — buffer-steal вместо
  второй копии. Дополняет §1 (имя `into_*`) механикой биндинга.

```nv
consume sb = StringBuilder.with_capacity(n)
sb.append(a).append(b)
ro s = sb.into_str()   // sb после into_str мёртв (use-after = E_CONSUME error)

// поделить владельца нельзя алиасом:
// ro twin = sb        // ❌ E_VIEW_BINDING_FORBIDDEN (Rule 2)
// только через view-параметр на время вызова (Rule 5):
fn used_len(view sb StringBuilder) -> int => sb.len()
ro k = used_len(sb)    // sb остаётся Live; view живёт только в вызове
```

## 20. Ветвление-как-выражение; три оператора пропагации ошибки; cleanup без RAII

### 20.1 `match`/`if`-выражение вместо early-return для результата (D19/D40)

- **Ветвление, дающее значение — это выражение** (тело через `=>`, ветви через `=> …` без
  `return`). Ранний `return`/`throw` оставляй ТОЛЬКО для guard-ов в начале функции
  (`03-syntax.md:2046-2056`; guard-clauses `std/runtime/string/transform.nv:155`).

```nv
fn classify(n int) -> str =>
    match n { 0 => "zero", n if n > 0 => "pos", _ => "neg" }

fn abs(x int) -> int => if x < 0 { -x } else { x }

fn left_pad(s str, w int) -> str {
    if s.byte_len() >= w { return s }   // guard-clause: ранний выход
    StringBuilder.with_capacity(w)
        .append_repeat(" ", w - s.byte_len())   // первый арг — str, второй — count (sb.nv:144)
        .append(s)
        .into_str()
}
```

### 20.2 Exhaustive `match` по sum-типам; избегай `_`-catch-all на доменных sum-типах

- **Доменный sum-тип — `match` исчерпывающе, БЕЗ `_`** (`nova_tests/plan103_1/ordering_enum_match.nv:10-16`).
  Spec предписывает exhaustive (`spec/syntax.md:655`); hard compile-error fires в const-fn
  (`E_CONST_FN_MATCH_EXHAUSTIVE`, `const_fn_eval.rs`). Для runtime-`match` общего
  exhaustiveness-gate в текущей реализации НЕТ — это конвенция стиля + ревью, поэтому
  `_`-catch-all на домене опасен вдвойне (компилятор не подстрахует при добавлении варианта).
- **Извлечение — `if Variant(x) = e`** (НЕ `if let` — Rust-форма retracted Plan 114,
  `03-syntax.md:1459`; реально `std/collections/hashmap.nv:417` `if Some(v) = @get(key)`).
- **Дешёвое да/нет — `is`** (переиспользует sum-discriminant, без глобального RTTI,
  `03-syntax.md:3215-3333`). **Не пиши предикат-метод `@is_X()`** — для этого есть `is`
  (`spec/syntax.md:410-418`).

```nv
// доменный sum-тип: match исчерпывающе, БЕЗ `_`
type Slot | Empty | Tombstone | Occupied(Entry)

fn handle(slot Slot) => match slot {
    Empty       => insert()
    Tombstone   => reuse()
    Occupied(e) => update(e)
    // нет `_`: добавишь вариант — здесь явно дотронешься (ревью; в const-fn — compile-error)
}

if Some(v) = lookup(k) { touch(v) }   // извлечение (форма hashmap.nv, НЕ `if let`)
if slot is Empty { skip() }           // дешёвое да/нет, без RTTI
// НЕ пиши slot.@is_empty() — для этого есть `is`
```

### 20.3 Три явных оператора пропагации: `?` / `!!` / `??` (D85/D86)

- **`expr?` — return-style**: ранний возврат обёртки (`return Err`/`None`). Работает ТОЛЬКО
  на Option/Result-форме (`try_*`), НЕ на Fail (`04-effects.md:4693`); enclosing-fn должна
  возвращать совместимый Result/Option.
- **`expr!!` — throw-style**: throw через эффект `Fail[E]` в сигнатуре (для Option →
  `RuntimeNoneError`, требует `Fail[RuntimeNoneError]`); канонически `!!` на `try_`-форме
  (`std/runtime/read_buffer.nv:428` `@read_byte() … => @try_read_byte()!!`).
- **Bare-throw callee НЕ требует оператора** — throw авто-пробрасывается, если у вызывающей
  fn уже есть `Fail[E]` в сигнатуре.
- **`expr ?? fallback` — coalesce**: дефолт/throw/panic/return без затаскивания `Fail[E]` в
  сигнатуру (`nova_tests/effects/throws.nv:79` `None ?? 10`). **Force-unwrap-оператора НЕТ**:
  краш только явно — `opt ?? panic(…)` (`04-effects.md:4738-4741`).

```nv
fn pipeline(s str) -> Result[int, ParseIntError] {
    ro n = s.try_parse_int()?          // на Err → return Err(e)  (try_-форма = Result)
    Ok(n * 2)
}

fn read_header(buf ReadBuffer) Fail[ReadBufferError] -> u8 {
    ro b = buf.try_read_byte()!!       // на Err → throw e  (!! на Result-форме try_)
    b
}

fn read_header2(buf ReadBuffer) Fail[ReadBufferError] -> u8 {
    buf.read_byte()                    // bare-форма уже Fail — throw авто-пробрасывается
}

fn port(cfg Config) -> int {
    cfg.get("port") ?? 8080            // None → 8080, без Fail[E] в сигнатуре
}

ro v = opt ?? panic("expected Some")   // краш ТОЛЬКО явно — force-unwrap-оператора нет (D85)
```

### 20.4 Детерминированный cleanup без RAII: `defer` + `consume X = init() { body }`

- **У Nova нет RAII-деструкторов** — освобождай ресурсы явно. `errdefer`/`okdefer`/
  `defer |result|` УДАЛЕНЫ (D189, hard cutover; парсер отклоняет с `[D189-removed-errdefer]`).
- **`defer` — безусловное освобождение** (close/unlock) на ЛЮБОМ выходе, несколько — LIFO
  (D90, `03-syntax.md`; тест `nova_tests/syntax/defer_basic.nv`). Аргументы вычисляются на
  месте, тело — отложенно; `mut` захватываются по ссылке. Тело `defer` МОЖЕТ иметь
  `Fail[E]`/suspend (D158/D159 amend D90 §4/§5) — тогда enclosing fn-sig обязан declare его.
- **Exactly-once cleanup ресурса — `consume X = expr { body }`** (Consumable[E].on_exit,
  D188 — официальная замена errdefer; status active, Plan 110; тесты `nova_tests/plan110/`).
  Для error-only-отката без Consumable-ресурса — паттерн escape hatch:
  `mut done = false; defer { if !done { rollback } }; …; done = true`.

```nv
// 1) defer — безусловное освобождение, LIFO, любой exit-путь (D90):
fn read_config(path str) Fs Fail -> Config {
    consume file = Fs.open(path)  // File линейный (must-consume, D133) → consume; `ro` = E_CONSUME_KEYWORD_MISSING (D180 Rule 1)
    defer file.close()            // consume @close — разряжает обязательство на любом выходе
    ro raw = file.read_all()
    Config.parse(raw)
}

// 2) error-only-откат без Consumable — паттерн-флаг (официальная замена errdefer, D189):
fn create_user(req UserReq) Db Fail -> User {
    ro user = Db.insert_user(req)
    mut ok = false
    defer { if !ok { Db.delete_user(user.id) } }   // откат только при throw ниже
    Db.insert_profile(user.id, req.profile)
    ok = true
    user
}

// 3) exactly-once cleanup ресурса — consume-scope (Consumable.on_exit, D188):
consume tx = Db.begin() {
    Db.insert(tx, data)?
}   // выход из блока → on_exit (commit при успехе, rollback при throw) — exactly-once

// УДАЛЕНО (D189, parser отклоняет): errdefer { … }, okdefer { … }, defer |result| { … }
```

## 21. Имена несут семантику: PascalCase-типы, полные слова, домен-квалифицированные ошибки

- **PascalCase** для типов/вариантов/эффектов/протоколов (акронимы тоже PascalCase: `Db`/`Io`/
  `JsonParser`, не `DB`/`IO`); **snake_case** для функций/полей/локалей; **SCREAMING_SNAKE_CASE**
  для констант; модули — snake_case через точку (D30, `03-syntax.md:827-850`).
- **Полные слова, не аббревиатуры** (`@capacity()` не `@cap()`, `destination` не `dest`);
  ровно 3 исключения — `len`/`iter`/`idx` локали (`03-syntax.md:893-922`).
- **Имена типов ≥2 символов** (`E_TYPE_NAME_TOO_SHORT`, `nova_tests/plan167/`).
  **Error-типы домен-квалифицированы** (`ParseUrlError`/`DbError`, не голый `Error`,
  `03-syntax.md:972-1000`). **`_`-префикс = намеренно неиспользуемое**, запрещён на
  public-экспортах (`03-syntax.md:941-970`).

```nv
export type ParseUrlError | EmptyHost | BadPort | InvalidScheme(str)
export fn parse_url(s str) Fail[ParseUrlError] -> Url {
    if s.is_empty() { throw EmptyHost }
    // ...
}
const MAX_RETRIES int = 5
```

## 22. Перегрузки одного имени — все в одном модуле; различай — называй по-разному

- **Все перегрузки имени ОБЯЗАНЫ жить в одном модуле** — читая модуль, видишь весь набор
  перегрузок `f` (D84 «LLM-критерий», `10-overloading.md:304-316`). Резолв = most-specific
  (concrete > generic, non-variadic > variadic), **без неявных конверсий** в матчинге
  (`10-overloading.md:202-205`); неоднозначность → compile-error со списком кандидатов.
- **Turbofish не обходит concrete-перегрузку**: `f[u8](7) ≡ f(7 as u8)` (D84,
  `10-overloading.md:113-130`).
- **Перегружай только same-task/different-type**; для реально РАЗНЫХ операций — разные имена
  (D40). Прецедент same-module overload: `std/runtime/string_builder.nv:99-139`
  (`@append(str)`/`@append(char)`/`@append(f64)`…).

```nv
// ✅ same-task/different-type → одно имя, ОБА определения в ОДНОМ модуле:
fn area(c Circle) -> f64 => 3.14159 * c.r * c.r
fn area(r Rect)   -> f64 => r.w * r.h

area(circle)   // → area(Circle), most-specific резолв, без неявных конверсий
// f[u8](7) ≡ f(7 as u8): turbofish НЕ обходит concrete-перегрузку (D84)

// ❌ реально РАЗНЫЕ операции — НЕ перегружай, бери разные имена (D40):
//   fn area(c Circle) -> f64    // площадь
//   fn area(s str)    -> int    // длина текста — другая задача → назови length/byte_len
```

## 23. Тип без тела как namespace для static-функций

- **Группу связанных static-функций без общего состояния оформляй как тип-неймспейс:
  `type Name` (БЕЗ тела) + `fn Name.method(...)`** (static-метод через `.`, НЕ instance-`@`).
  Это аналог Rust associated functions без полей / namespace-модуля внутри типа — даёт
  квалифицированный вызов `Name.method()` вместо россыпи свободных функций. Реально в stdlib:
  `std/runtime/raw_mem.nv:31` `export type RawMem` (без тела) + `:43`
  `export extern "nova" unsafe fn RawMem.copy(src *u8, dst *mut u8, n int) -> ()`,
  `RawMem.fill`/`compare`/`alloc`/… — все `fn RawMem.<name>`.
- **`Name.fn()` (static, точка) vs `value.@m()` (instance, `@`).** static-метод не имеет
  receiver-значения — это просто функция в пространстве имён типа. Instance-метод (`@`)
  работает на значении. Не путай: namespace-тип НЕ инстанцируется (тела/полей нет).
- **Когда так делать:** связанный набор операций над внешними данными (raw-указатели,
  байты, FFI-интринсики), где receiver был бы искусственным, но плоские свободные функции
  теряют группировку. `RawMem.copy(src, dst, n)` читается как «операция memory-namespace'а»,
  а не как метод на чём-то. Заголовок-комментарий файла называет тип «namespace»
  (`raw_mem.nv:6` «**RawMem namespace** (`type RawMem`) groups raw memory operations»).
- **Не злоупотребляй:** если у операций ЕСТЬ естественный receiver-значение — это
  instance-методы (`@`, §3), а не static-namespace. Тип-неймспейс — для функций БЕЗ
  носителя-значения.

```nv
// тип-неймспейс: type без тела + static-методы через `.`
// (как std/runtime/raw_mem.nv — RawMem.copy/fill/compare/alloc).
export type RawMem

export extern "nova" unsafe fn RawMem.copy(src *u8, dst *mut u8, n int) -> ()
export extern "nova" unsafe fn RawMem.fill(dst *mut u8, val u8, n int) -> ()

fn use_it(src *u8, dst *mut u8, n int) -> () {
    unsafe { RawMem.copy(src, dst, n) }   // квалифицированный static-вызов Name.fn()
}

// ✗ не делай namespace-тип, если есть носитель-значение — это instance-метод (@, §3):
//   type Vec3 value { x f64, y f64, z f64 }
//   fn Vec3 @length() -> f64 => …        // @ — работает на значении, НЕ Vec3.length()
```

---

## 24. Числовые типы: `int` для индексов/размеров/offset; `u*` — только где ширина семантична

- **Индекс / длина / размер / offset / позиция / счётчик — `int`** (i64), **не `u64`/`usize`** (в отличие от Rust). Так во
  всём stdlib: `Vec[T].@len()->int`, `str.@byte_len()->int`, `WriteBuffer.@len()/@capacity()->int`, `ReadBuffer.@position()/@remaining()->int`,
  `SeekFrom.Start(int)`. i64 покрывает любой реальный размер/offset (±8 EiB).
- **`u8`/`u16`/`u32`/`u64` — только когда значение *само по себе* этой ширины:** байт-данные — `u8`/`[]u8`; UTF-16 code units —
  `u16`/`[]u16`; **Unicode codepoint (scalar value) — `u32`** (см. ниже); типизированные атомики (`AtomicU64.@fetch_add(v u64) -> u64`);
  фиксированный bitmask по необходимости. Там ширина семантична, а не «беззнаковость ради порядка».
- **Codepoint = `u32`, НЕ `int`** · согласовано 2026-06-26. Кодпоинт — character-data интринсик-ширины 32 бит (применение правила выше,
  ср. UTF-16 code units → u16), ОТДЕЛЬНО от правила «index/len/offset → int»: кодпоинт — *значение-идентификатор*, не мера. Хранилище
  последовательностей — `Vec[u32]` (4 байта, как Rust `Vec<char>` / Go `[]rune`=`[]int32`); поток и арифметика внутри unicode-движков — `u32`.
  `char` (= `u32`, [D128](../spec/decisions/02-types.md#d128)) — на границе `str` (`as_chars()`→`char`, `char.try_from(u32)`) и в char-методах
  (`'a'.is_alphabetic()`). **Публичные cp-функции принимают `u32`** (`general_category(cp u32)`); целочисленные литералы адаптируются к
  u32-контексту, поэтому `general_category(0x41)` остаётся валидным. **Fallible-функции, выдающие кодпоинт, → `Option[u32]`** ([D77](../spec/decisions/02-types.md), а не `-1`-сентинел).
  **Bit-packing** нескольких кодпоинтов в один ключ (`(a<<21)|b`, > 32 бит) → явный `as int` (packed key — не кодпоинт). Обоснование:
  [D327](../spec/decisions/02-types.md#d327).
- **Анти-паттерн:** `u64`/`usize` для offset/len «чтобы было ≥0» + россыпь `as u64`-кастов (литералы Nova — `int`). Знак не кодируем
  типом — отрицательный индекс/offset → доменная ошибка (`InvalidInput` / контракт `requires i >= 0`), как `SeekFrom.Start(int)` (Start < 0 → ошибка).
- **Почему signed, а не unsigned (обоснование/research):**
  [research/08-int-width-and-literal-inference.md §1](research/08-int-width-and-literal-inference.md) (3 раунда обсуждения, 2026-06-03)
  → формализовано в [D226 «Signed indexing convention»](../spec/decisions/02-types.md#d226) (§Почему); `usize`/`isize` удалены
  [Plan 133](plans/133-remove-usize-isize.md). Ключевое: **industry 7:3 за signed** (Go/Swift/Java/Kotlin/C#/Python/TS signed;
  Rust `usize`/C++ `size_t`/Zig — unsigned, причём **Stroustrup: «I regret using unsigned for size in STL»** + vocal Rust-regrets);
  **нет underflow-trap** (`xs.len() - 1` на пустом vec даёт `-1`, не паника как Rust `0usize-1`); sentinel `-1` для find; разности/diff
  естественно signed; mixed-arith без `as`-ceremony (**AI-first**: LLM пишет signed-индексацию вернее); bit-width-аргумент мёртв на
  64-bit (i64 = 9.2×10¹⁸ элементов).

---

## Известные расхождения для будущего sweep'а

1. **`docs/idioms/size-accessors.md:41-42`** документирует `s.len()` как O(n) codepoint-count,
   но `strings.md:71` / `core.nv:22` — это compile-error `E_STR_NO_LEN` (Plan 152.1/D249).
   Idiom-doc устарел; `strings.md` авторитетен.
2. **Стиль-дрейф:** часть строкового кода ещё пишет `i = i + 1` вместо `i += 1` (`search.nv`,
   `core.nv`) и имеет инлайн-`;` (`parse.nv:64`). Новый код — `+=`/без `;`; механический sweep
   приведёт существующее в соответствие (Plan 91.18 Ф.8).
3. **Контракт-разрыв (высокоценный followup):** строковые методы почти без `requires`/`ensures`
   при идеальных целях (offset'ы, radix-диапазон). Правило 5 + elidable-bounds модель
   (`contracts.md:204`) — добавление бесплатно в runtime при доказуемости.
