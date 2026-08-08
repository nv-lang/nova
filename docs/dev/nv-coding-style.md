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
> строк/Unicode) живут также в [strings.md](../guide/strings.md); правила 3–7, 9 — общеязыковые.
> Кросс-ссылки: правило 5 → [contracts.md](../guide/contracts.md); правило 6 → [parameters.md](../guide/parameters.md).
>
> §§11–22 (общеязыковые: эффекты/конкурентность 11–14, типы/композиция 15–17,
> мутабельность/владение 18–19, control-flow/ошибки/cleanup 20, именование 21,
> перегрузки 22) заземлены в `spec/decisions/` (D-блоки указаны в каждом правиле) и
> живом `std/`.

---

## 0а. Язык документации (· согласовано 2026-07-07)

- **Doc-comments (`///`, `//!`) std, ПАКЕТНЫХ реп (bigint/polaris/http/compress/tls) и examples — английский; сообщения линта (lints.rs) — английский** (владелец 2026-07-31, повторно). И: в `///` НИКАКИХ внутренних ссылок — номеров планов, маркеров [M-...], D-номеров, №N, «что закрывали и по каким дефектам»: язык ещё не в релизе, пользователю это не интересно; дока = смысл API. Автопроверка: `scripts/guards/check-doc-hygiene.sh` (храповик, шаг gate.sh): это пользовательский артефакт
  будущего релиза.
- **Спека (`spec/decisions/`), планы, журналы, конвенции — русский** (голос автора,
  единственное число).
- Внутренние `//`-комментарии реализации — любой из двух, не смешивать языки в одном блоке.

## 1. Голое имя-вид / `to_*`-трансформация / `into_*` (D410, решение владельца 2026-07-06)

> Прежнее правило `as_*`/`to_*`-пар РЕТРАКТИРОВАНО (D410): близнецы-копии удалены,
> префикс `as_` упразднён. Миграция — `[M-d410-as-to-migration]`.

- **Голое существительное = O(1) вид/линза**: заём, zero-copy, лениво. `s.bytes()` — `ro []u8`
  реинтерпретация; `s.chars()`/`s.words()`/`s.sentences()` — ленивые линзы; `v.slice()`,
  `v.ptr()`. Вид НИКОГДА не аллоцирует-и-копирует и НИКОГДА не потребляет receiver. Если
  «вид» по какой-то причине O(n) в создании — это нарушение: делайте ленивым или признавайте
  трансформацией (`to_*`).
- **Копия — явно на месте вызова**: `s.bytes().clone()`, `s.chars().collect()`. Отдельных
  методов-близнецов (`to_bytes`, `to_chars`) НЕТ — плата за аллокацию видна глазами там,
  где она платится.
- **`to_*` = трансформация в новое владеющее значение** — операция, у которой вида не
  существует в принципе: `to_upper()`, `to_lower()`, `to_ascii_upper()`, `int.to_str()`.
- **`into_*` = потребляющий финализатор** (ось владения): `StringBuilder @into_str()`,
  `Vec @into_raw()`. Receiver — `consume`; голым именем и `to_*` потребление не выражается.
- **Никакого голого `len`/индекса, прячущего O(n).** У `str` нет `s.len()` (три расходящиеся
  длины) и нет `s[i]` по codepoint — оба compile-error (`E_STR_NO_LEN`/`E_STR_NO_INT_INDEX`,
  `strings.md:71-82`). Голым остаётся только `byte_len()` (O(1)); codepoint/grapheme-счёт —
  через явный lens (`s.chars().count()`).

### 1а. Четыре направления конверсий (система, владелец 2026-07-09)

| Операция | Форма | Пример |
|---|---|---|
| Вид/переинтерпретация (zero-copy) | голое существительное | `s.bytes()`, `s.chars()` |
| Конверсия с копией/валидацией из ro-источника | `x.to_*()` (→ Result где fallible) | `cp.to_char()`, `s.to_int()`, `b.to_str()` |
| Конверсия с передачей владения | `consume x.into_*()` | `sb.into_str()`, `b.into_str_unchecked()` |
| Конструктор без источника-носителя | `Type.new(...)` / `of` вариадик / композитные | `DeError.new(k)`, `Vec.of(1,2)`, `SocketAddr.v4(...)` |

Статики-конверсии `T.from(x)` / `T.parse(s)` — ЗАПРЕЩЁННАЯ пятая дверь (дубль
`to_*`; ломает цепочки — симптом: `.ok()`-прослойки в match). Ретрактированы
2026-07-09: char.from→to_char, str.parse_*→to_int-семья, str.from_bytes*→
[]u8.to_str*/into_str_unchecked. `from` уместен ТОЛЬКО когда источник — не
значение-ресивер, а концепт (from_polar, embed). Проверка (185): W_STATIC_CONVERSION.

**Имя типа внутри `to_*` — snake_case по границам CamelCase** (· согласовано 2026-07-30):
`SocketAddr` → `to_socket_addr`, `ErrorKind` → `to_error_kind`, `IoError` → `to_io_error` —
Rust-парити (`ToSocketAddrs::to_socket_addrs`); сплющивание (`to_socketaddr`) стирает границы
слов. **Исключение** — типы, чьё имя лексикализовано индустрией как ОДНО слово: `datetime`,
`bigint`, `bigdecimal`, `bigfloat` (Python-модуль `datetime`, Java `BigDecimal`) — пишутся
слитно: `to_datetime`, `to_bigint`. Дрейф `to_versionreq` мигрирован на `to_version_req`
2026-07-30 (`std/data/semver_range.nv`, `[M-from-str-static-conversion-lint-gap]`-волна).

**Голое имя-вид ЗАПРЕЩЕНО на `consume`-receiver'е.** Потребление владения
выражается ТОЛЬКО `into_*`; голое существительное — только zero-copy вид,
НЕ потребляющий receiver (первая строка таблицы). Смешение — нарушение оси:
`consume`-метод, конвертирующий в другой тип, но названный как вид, читается
как дешёвая переинтерпретация, а на деле потребляет и часто аллоцирует.
Пример нарушения: `Response consume @bytes() -> Result[[]u8, HttpError]` →
канон `Response consume @into_bytes() -> Result[[]u8, HttpError]`.
Исключения (отдельные оси, не нарушение): `with_*` (D117 wither — не
финализатор), `to_*` (ro-источник, отдельный класс), возврат ТОГО ЖЕ типа
receiver'а (builder-шаг, не конверсия), Unit-подобный возврат
(`-> ()`/без `->`/`Result[(), E]` — RAII-финализатор `close`/`cleanup`/
`commit`/`release`, не конверсия в значение). Проверка (185): W_CONSUME_NAKED_NAME.

### 1б. Порядок кортежа при расщеплении на два конца (владелец 2026-07-24)

Два разных случая — два разных правила (записано, чтобы вопрос «почему порядок
противоположный?» не поднимался заново):

| API | Возврат | Правило порядка |
|---|---|---|
| `Channel.new(N)` | `(tx, rx)` | **Направленный конвейер:** данные текут `tx → rx`; кортеж читается слева направо ПО НАПРАВЛЕНИЮ ПОТОКА (сообщение сперва отправлено, потом принято). Бонус: совпадает с Rust `mpsc`. |
| `TcpStream consume @into_split()` | `(TcpReadHalf, TcpWriteHalf)` | **Дуплекс-расщепление:** два независимых направления одного сокета, причинного порядка нет — правит устоявшаяся идиома пары `read/write`. |

Это НЕ несогласованность: направленная труба упорядочена потоком, дуплекс — идиомой.
Терминология тоже доменная: message-passing = `tx`/`rx` (Channel), byte-stream I/O =
`read`/`write` (сокеты). `into_rxtx` и подобные переносы словаря между доменами — отклонены.

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
- **🎯 Единый fallible-контракт std (D325, Plan 177 · согласовано 2026-06-25) — Result-everywhere.** Любая падающая публичная операция → `Result`:
  - **(R1)** → `Result[T, <Domain>Error]`. Нет bare-throws-близнецов, нет `try_`-дублей, нет `_opt`.
  - **(R2)** Имя обычное, без префикса: `parse_int -> Result`, `read_u32 -> Result`, `open -> Result` (как Rust `str::parse`).
  - **(R3)** Префикс `try_` — **только** чтобы отличить fallible-вариант одноимённого **infallible**: `from`/`try_from`, `into`/`try_into` (D77). В одиночных fallible-операциях (нет infallible-сиблинга) префикса НЕТ.
  - **(R4)** `Option` — только genuine absence (`find`/`get`/`env`/`parent`), НЕ fallibility; `Result → Option` через `.ok()`.
  - **(R3а) Трёхсловарная таблица «checked/try» (владелец 2026-07-21 — роли ЧЁТКИЕ, не смешивать):**
    · **`checked_*` (префикс)** — ТОЛЬКО арифметическая политика переполнения, четвёрка
      `checked_/wrapping_/saturating_/overflowing_` (D423/D427; Rust-словарь) — не применять вне арифметики;
    · **`try_*`** — неблокирующая попытка «удалось ли сейчас» → `bool` (sync: `try_lock`/`try_acquire`,
      Rust-парити) ПЛЮС конверсии-сиблинги D77 из (R3) (`from`/`try_from`);
    · **`*_checked` (суффикс)** — Option/Result-близнец ПАНИКУЮЩЕГО/unchecked одноимённого метода,
      проверка предусловия (`split_at` ↔ `split_at_checked`, `into_str` ↔ `into_str_checked`;
      Rust-парити `split_at_checked`) — суффикс намеренно держит пару рядом в доке/автокомплите.
    Выглядит разнобоем, но это три РАЗНЫЕ семантики (политика ≠ попытка ≠ предусловие) — все три
    Rust-парити; выравнивание «под одну» ломало бы словарь без выгоды.
  - **(R5)** Эффект `Fail[E]` в публичной std-сигнатуре запрещён для **собственных** ошибок (→ `Result`), но разрешён для прозрачного **проброса** `Fail[E]` из closure-параметра (effect-polymorphic forwarding: `retry`/`parallel`/`in_transaction` над телом пользователя).
  - Throw сохранён операторами (D85): `expr!!` (throw), `expr?` (проброс), `expr.ok()` (→Option), `match`. Эффект `Fail[E]` остаётся в языке (D25) — для пользовательского кода и внутренних хелперов; std им свои ошибки наружу не отдаёт. **Эталон:** `std/net` (Result-everywhere, 0 `Fail[`) — норма, не исключение.
  - **Миграция SHIPPED-форм** (`@try_parse_int`→`@parse_int`, удаление bare `@parse_int`/`@parse_int_opt`, 22 `read_X`/`try_read_X` пары) — Plan 177 Ф.2 (compiler-gated части — Ф.2b).
- **lossy-FFFD (четвёртая категория) — ТОЛЬКО для функций, чьё имя это говорит** (`*_lossy`)
  или чей контракт best-effort (`cps_to_str`): подставляют U+FFFD. **Никогда не подставляйте
  пустую строку** как «успех» при невалидном входе — это потеря данных под видом успеха.
- **Тихое глотание `Result` запрещено (· согласовано 2026-07-07).** `match r { Ok(_) => (),
  Err(_) => () }`, `ro _ = fallible()`, голый вызов-statement с отброшенным `Result` — всё
  это потеря ошибки под видом успеха. Допустимое потребление: `?` (проброс), `!!` (нарушение
  инварианта = паника; канон внутри тотальных сеттеров `-> @`, куда вход пишет программист),
  `match`/`.ok()` с ОСМЫСЛЕННОЙ обработкой обеих ветвей. Динамический (внешний) вход идёт
  через fallible-путь с `Result`, не через тотальный сеттер. Проверка: `W_RESULT_DISCARDED`
  (план 185).

## 5. Максимизируйте контракты (Z3 их элидирует)

Доказанный `requires` — **zero-cost** (элидируется в compile-time, даже в debug,
`contracts.md:8`). Заявляйте каждое предусловие, которое можете.

- **ОБЯЗАТЕЛЬСТВО (· согласовано 2026-07-07): каждый index/offset/len/count-параметр публичной
  std-поверхности ОБЯЗАН нести `requires`** — это не «по возможности», а норма приёмки
  (Z3 элидирует, стоимость ноль). Проверка: греп публичных сигнатур с параметрами
  `i/idx/pos/offset/len/n/cap/count int` без `requires` — кандидаты в нарушение.
- Границы и валидность — `requires` на каждом index/offset-параметре: `requires 0 <= i && i
  < @byte_len()` (только `&&`, никогда chain `0 <= i < n` → `E_CMP_CHAIN_UNSUPPORTED`). В
  методах ссылайтесь на состояние receiver через `@field`/`@byte_len()`.
- **Параметр-only `requires` особенно ценны** — Z3 снимает их на литеральных аргументах
  (`requires radix>=2 && radix<=36` исчезает на `parse_int(s, 16)`, `parse.nv`; форма D325).
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

## 5а. Канон `[]T` (D239 amend, vec-sweep 2026-07-06)

- **`[]T` — каноническая запись везде**: аннотации типов, возвраты, вложенные
  формы (`[][]u8`), кортежи (`[](str, str)`), статические
  вызовы-конструкторы (`[]u8.new()`, `[]int.of(1,2,3)`, `[]int.from(...)`).
- **`Vec[...]` — ТОЛЬКО definition-site**, внутри `std/collections/vec/`
  (реализация самого `Vec[T]`). За пределами этого модуля пишите `[]T`.
- **Исключение** — известный compiler-gap `[M-153.x-array-new-not-vec]`:
  bracket-spelling `[]T` в value-позиции конструктора иногда лоуэрится через
  legacy erased-array путь вместо типизированного `Vec`-пути и может дать
  RUN-FAIL (см. `std/collections/hashmap.nv::new_buckets` для примера и
  маркера на месте). В таких точечных местах `Vec[...]` остаётся исключением
  с явным комментарием-маркером — не переписывайте их бездумно на `[]T`.
- **`cap(n)` (D117 setter, ex-`with_capacity`)** — см. §6 ниже: НЕ чейньте
  `X.new().cap(n)` в одном выражении, если результат бинедится через
  `consume`/сохраняется в переменную, которую дальше используют другие
  вызовы в той же compile unit — известные gaps
  `[M-vec-spelling-array-value-position-cap-collision]` /
  `[M-vec-spelling-consume-chain-cap-collision]` (D372 amend).

## 6. Компактный стиль

- **Tuple-binding для параллельных чтений:** `ro (sn, pn) = (@byte_len(), prefix.byte_len())`
  (`search.nv:20`); `ro (cp, step) = decode_at(s, i)`. Без длинных строк.
- **`+=`/`-=` вместо `a = a + 1`** для счётчиков/курсоров.
- **Операторы вместо `.compare()`/`.equal()`.** `str` синтезирует `< <= > >= == !=` из
  `@compare`/`@equal` — пишите `a < b`, не `a.compare(b) < 0`.
- **Цепочки вызовов / fluent `-> @`.** Чейньте через self-returning мутаторы:
  `[]u8.new().append(@as_bytes())`; `StringBuilder.new().append(x)
  .append(y).into_str()`. Метод, имеющий смысл в цепочке, объявляйте `-> @`, не `-> ()`.
  **Исключение — `cap(n)` (D117 setter, ex-`with_capacity`, амендмент D372
  2026-07-06): НЕ чейньте `X.new().cap(n)` в одном выражении** — известный
  compiler-gap (`[M-vec-spelling-array-value-position-cap-collision]` /
  `[M-vec-spelling-consume-chain-cap-collision]`, см. D372) мис-dispatch'ит
  или ломает consume-tracking. Разбивайте на два оператора:
  `mut x []u8 = []u8.new(); x.cap(n)` (или `consume sb = StringBuilder.new(); sb.cap(n)`),
  и только ПОСЛЕ этого продолжайте цепочку остальных методов.
- **Без `;` как разделителя** и **без нескольких операторов на строке через пробел**
  (`term = 11 phase = 0` — запрещено так же, как `;`). Один оператор — одна строка.

## 7. RawMem / bulk-copy вместо push-циклов

- **Стройте владеющие буферы одной аллокацией + bulk `append`, не push-циклом.**
  `@to_bytes` = `new()` + `.cap(byte_len())` + `.append(@as_bytes())` — один `RawMem.copy`
  (`memmove`), без цикла (`core.nv:61-67`).
- **Сравнение/скан через `RawMem.compare` (`memcmp`), не ручной byte-loop.** `@compare`,
  `@equal` (`core.nv`); `@starts_with`, `@ends_with`, `@contains`, `@find`, `@split`
  (`search.nv`).
- **Всегда пре-сайзьте.** `[]u8.new()`+`.cap(n)` / `StringBuilder.new()`+`.cap(n)`
  (D372 amend 2026-07-06, ex-`with_capacity(n)`) до любого
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
  heap-класс. **Все — field-level `priv`/`_`-курсор**: String/unicode-итераторы (CharsIter,
  CharIndicesIter, Graphemes/Words/SentencesIter) наравне с коллекционными (VecIter,
  RangeIter, Map/Filter/Take/Enumerate/StepByIter). `value priv(type)` (тип-privacy) для
  итераторов — ИСПРАВЛЕНО (Plan 200 п.2): было излишне строго — по D267 любой модуль может
  писать extension-методы для типа → `priv(type)`-поля всё равно обходимы; реальная внешняя
  граница — module-`priv` (D281), не type-privacy.
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

**Проверка (185, реализовано 2026-07-17):** `W_WHILE_COUNTER_FOR_RANGE`
(`compiler-codegen/src/lints.rs`, реестр `CONV_RULES`) — эвристика ловит
РОВНО счётный подслучай этого правила: `mut i = start` НЕПОСРЕДСТВЕННО
перед `while i < end { …; i += 1 }` (или `i = i + 1`), инкремент — ПОСЛЕДНИЙ
statement тела. Консервативно молчит, если нарушено любое из: `i`
присваивается ещё где-то в теле (any depth); условие сложнее строгого
`i < end`/`i <= end`; `end` — не простое место (ident/`@field`)/int-литерал,
или мутируется в теле (голый вызов/индекс как `end` переоценивался бы
КАЖДУЮ итерацию `while`, но ОДИН раз в `for`-range — реальная разница); в
теле есть `continue` (перепрыгнул бы инкремент); `i` используется ПОСЛЕ
`while` (не пережил бы конверсию в `for`-переменную, D58 scope); `while`
несёт `invariants`/`decreases` (потерялись бы при механической замене).
Не покрывает координационные/парсерные/fixpoint-циклы выше (SEMANTIC-
UPGRADE вне V1) — их и не нужно ловить, это НЕ счётный подслучай.

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
  (~16 байт) на элемент (`docs/dev/simplifications.md:28027`) — бери ТОЛЬКО при реальной
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

### 18а. Срезы-виды вместо ручного копирования (2026-07-08)

Поэлементная копия куска `[]T` в новый Vec — красный флаг: `[]T`-вид (D262)
даёт то же за O(1) без аллокации.

```nova
// ПЛОХО: O(n) копия на каждой итерации частичной записи (квадратичность)
mut rest []u8 = []u8.new()
mut i = done
while i < total { rest.push(@buf[i]); i += 1 }
@inner.write(rest)

// КАНОН: zero-copy срез-вид
@inner.write(@buf[done..total])
```

Копия легальна только когда нужно ВЛАДЕНИЕ отдельным буфером — и тогда она
пишется явно `.clone()` на виде, а не циклом (плата видна в точке вызова, §D410).
Вычищено 2026-07-08: io/buffered drain, fs write, path slice_from/slice_to
(вопрос владельца). Проверка (185): эвристика «push(x[i]) в счётном цикле».

### 18б. `mut`-параметр — позиция ПЕРЕД именем (канон, owner decision 2026-07-17)

Канон mut-параметров — ПРЕФИКСНАЯ форма `mut name Type`. Есть ДВЕ разные позиции,
куда исторически можно было поставить `mut`, и они означают РАЗНОЕ:

- **позиция ПЕРЕД именем** (`mut name Type`) — канон, D176/Plan 108.1: параметр
  read-only по умолчанию, `mut` — единственный opt-in на mut-методы/index-запись,
  видимую вызывающему (§18 выше);
- **позиция ПОСЛЕ имени, ПЕРЕД типом** (`name mut Type`) — D6 legacy-спеллинг того
  же самого: парсер принимает её как ПОЛНЫЙ поведенческий синоним префиксной формы
  (`i mut int` реассайнится в теле идентично `mut i int`) — она НЕ даёт другой,
  более узкой семантики, просто исторический альтернативный спеллинг.

Голая постфиксная форма (без `ro` перед именем) — footgun-спеллинг, под запретом
lint'а `W_PARAM_TYPE_POS_MUT` (unconditional pipeline, `lints.rs`) для **не-slice**
типов. Позиция ПОСЛЕ имени зарезервирована ИСКЛЮЧИТЕЛЬНО за view-слайсами и их
роднёй — `[]u8`/`[]T` (io-канон, `buf mut []u8`) и fixed-size массивами
`[N]u8`/`[N]T` (hash-digest out-буферы, `std/crypto/sha256.nv` `out mut [32]u8`);
для любого другого типа (`Mutex`, `StringBuilder`, `HashMap[K,V]`, generic-параметр
`R`/`W` бэки протокол-bound'ов и т.п.) — пиши mut ПЕРЕД именем.

Санкционированное ИСКЛЮЧЕНИЕ — explicit **R2-split** `ro name mut Type` (D246 P6,
Plan 118.5 V3 amend): `ro` явно снимает L1 (без реассайна имени), постфиксный
`mut` явно про L2 (запись в содержимое) — самодокументирующая, НЕ-каноническая, но
разрешённая строгая форма (`spec_tests/conformance/d246_param_ro_mut_view.nv`). Lint
её не флагует (parser не отмечает legacy-маркер, когда `ro` был явным).

```nv
// КАНОН — mut ПЕРЕД именем
fn bump(mut i int) { i = i + 1 }
fn wait(mut m Mutex) { m.lock() }

// ЗАПРЕЩЁННЫЙ синоним (postfix, non-slice) — W_PARAM_TYPE_POS_MUT
// fn bump(i mut int) { i = i + 1 }        // ⚠ мигрируй на `mut i int`
// fn wait(m mut Mutex) { m.lock() }       // ⚠ мигрируй на `mut m Mutex`

// ЛЕГИТИМНО (slice/fixed-array родня) — postfix остаётся
fn read(buf mut []u8) -> int => 0                // io-канон
fn hash(out mut [32]u8) { out[0] = 1 as u8 }      // digest out-buffer

// САНКЦИОНИРОВАННОЕ исключение (R2-split, НЕ канон, но разрешено)
fn touch(ro i mut int) { i = i + 1 }             // explicit ro + explicit mut
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
consume sb = StringBuilder.new()
sb.cap(n)   // D372 amend 2026-07-06: НЕ чейньте .cap(n) в ту же `consume`-строку — §6 исключение
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
    StringBuilder.new().cap(w)
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
- **Exactly-once cleanup ресурса — `consume X = expr { body }`** (Cleanup[E].@cleanup,
  D188 — официальная замена errdefer; status active, Plan 110; тесты `nova_tests/plan110/`).
  Для error-only-отката без Cleanup-ресурса — паттерн escape hatch:
  `mut done = false; defer { if !done { rollback } }; …; done = true`.
- **Discharge-глаголы ресурса — единая таксономия · согласовано 2026-06-27.** Не плодить
  синонимы для «освободить ресурс»: `@cleanup(o ScopeOutcome)` — **АВТО**-хук протокола
  `Cleanup[E]` (зовёт компилятор в конце `consume X = e {}`, outcome-aware; после
  [173](../plans/173-error-system-unify-harden.md) sign-off — бывш. `Consumable.@on_exit`);
  `@close()` — **РУЧНОЙ** no-arg teardown (net/channels/File/простой ресурс); `@drain()` —
  дочитать остаток → release (reusable, напр. HTTP-conn в пул); `@finish()` — завершить
  **ПИСАТЕЛЬ** (напр. chunked-terminator). `@cleanup` обычно делегирует в `@close()`. Глагол
  `@discard` и прочие синонимы release — НЕ вводить (= `@close`/`@drain` по семантике).
  Обоснование выбора — [Plan 178 §13.3](../plans/178-std-http.md).

```nv
// 1) defer — безусловное освобождение, LIFO, любой exit-путь (D90):
//    std fallible → Result (D325); `?` разворачивает/пробрасывает на call-site (D85).
fn read_config(path str) Fs -> Result[Config, IoError] {
    consume file = Fs.open(path)?  // Fs.open -> Result[File,IoError]; `?` разворачивает; File линейный (must-consume, D133) → consume; `ro` = E_CONSUME_KEYWORD_MISSING (D180 Rule 1)
    defer file.close()             // consume @close — разряжает обязательство на любом выходе
    ro raw = file.read_all()?      // read_all -> Result; `?` пробрасывает IoError
    Ok(Config.parse(raw))          // Config.parse инфаллибл → обернуть в Ok
}

// 2) error-only-откат без Cleanup — паттерн-флаг (официальная замена errdefer, D189):
fn create_user(req UserReq) Db Fail -> User {
    ro user = Db.insert_user(req)
    mut ok = false
    defer { if !ok { Db.delete_user(user.id) } }   // откат только при throw ниже
    Db.insert_profile(user.id, req.profile)
    ok = true
    user
}

// 3) exactly-once cleanup ресурса — consume-scope (Cleanup.@cleanup, D188):
consume tx = Db.begin() {
    Db.insert(tx, data)?
}   // выход из блока → @cleanup (commit при успехе, rollback при throw) — exactly-once

// УДАЛЕНО (D189, parser отклоняет): errdefer { … }, okdefer { … }, defer |result| { … }
```

## 21. Имена несут семантику: PascalCase-типы, полные слова, домен-квалифицированные ошибки

- **PascalCase** для типов/вариантов/эффектов/протоколов (акронимы тоже PascalCase: `Db`/`Io`/
  `JsonParser`, не `DB`/`IO`); **snake_case** для функций/полей/локалей; **SCREAMING_SNAKE_CASE**
  для констант; модули — snake_case через точку (D30, `03-syntax.md:827-850`).
- **Полные слова, не аббревиатуры** (`destination` не `dest`); исключения — `len`/`iter`/
  `idx` локали (`03-syntax.md:893-922`) и `cap` (канонизировано свойством `cap()`/`cap(n)`,
  D117 AMEND; прежний пример «capacity не cap» — до-амендментная эпоха, ретракция capacity
  в волне-2).
- **Имена типов ≥2 символов** (`E_TYPE_NAME_TOO_SHORT`, `nova_tests/plan167/`).
- **Никаких `_`-префиксов приватности (· согласовано 2026-07-08).** Приватность выражается
  СИСТЕМОЙ ВИДИМОСТИ (default-private без `export`; `priv` / `priv(file)` / `priv(type)`),
  не соглашением об именах: `fn addr_image()` без export, НЕ `fn _addr_image()`. `_`-имя
  дублирует модификатор и врёт читателю о видимости. Легальные употребления `_`:
  discard-биндинг `ro _ = ...`, неиспользуемый параметр `_x` (если язык требует имени),
  C-мир (`_pad`-поля C-образов, `_nova_*` rt-хелперы — не Nova-канон). Проверка (185):
  греп `fn _[a-z]|@_[a-z]` = 0 в std.
- **Меньшее/большее из двух — `a.min(b)` / `a.max(b)` (· согласовано 2026-07-07),**
  не `if a < b { a } else { b }`-тернарии: методы определены на числовом семействе
  (`defaults.nv`), читаются как намерение, не как ветвление. Исключение — сами
  определения `@min`/`@max` (их реализация). Проверка (185): греп
  `if [ident] [<>]=? [ident] { [ident] } else { [ident] }`.
- **Конвейер преобразований одного значения — ребиндинг одним именем (D347; · согласовано
  2026-07-07).** Смена типа по шагам конвейера — повторный `ro x = ...` (rebind = новая
  переменная, тип может отличаться), НЕ суффиксные цепочки имён:
  ```nova
  ro port = env("PORT") ?? "8080"    \ str
  ro port = int.parse(port)!!        \ int — то же логическое значение, новый тип
  ```
  НЕ `port_str`/`port_num`/`port_raw`. Разные СУЩНОСТИ — разные имена (ребиндинг только
  для конвейера одного логического значения). Проверка-эвристика (185): греп-кандидаты
  `[a-z]+_(raw|str|num|txt|parsed|val) =`.
  **Error-типы домен-квалифицированы** (`ParseUrlError`/`DbError`, не голый `Error`,
  `03-syntax.md:972-1000`). **`_`-префикс = намеренно неиспользуемое**, запрещён на
  public-экспортах (`03-syntax.md:941-970`).
- **Конструкторы: `.new(...)` — обычный конструктор типа из аргументов; `.of(...)` —
  ТОЛЬКО вариадик-конструктор коллекции «из перечисленных элементов» (`Vec[T].of(a,b,c)`,
  D259). Пустой `.of()` (0 аргументов) запрещён контрактом — пустая коллекция строится
  ТОЛЬКО через `.new()` (`Vec[T].new()`, D259 amend 2026-07-06).

```nv
export type ParseUrlError | EmptyHost | BadPort | InvalidScheme(str)
export fn parse_url(s str) Fail[ParseUrlError] -> Url {
    if s.is_empty() { throw EmptyHost }
    // ...
}
const MAX_RETRIES int = 5
```

### 21б. Конструкторы: `new` + дефолт-параметры; `of` только вариадик (2026-07-09)

Тривиальная установка полей записи — это `Type.new(...)`, и ровно ОДИН
конструктор: опциональные поля выражаются дефолт-параметрами (D102,
передаются по имени), а не парой функций:

```nova
// ПЛОХО: две двери + имя of у невариадика
export fn DeError.of(kind DeErrorKind) -> DeError => { kind, path: "" }
export fn DeError.at(kind DeErrorKind, path str) -> DeError => { kind, path }

// КАНОН
export fn DeError.new(kind DeErrorKind, path str = "") -> DeError => { kind, path }
// вызовы: DeError.new(k) · DeError.new(k, path: "$.field")
```

Имя `of` зарезервировано ЗА ВАРИАДИК-коллекциями (`Vec[T].of(a, b, c)`);
`from` — за конверсией из другого типа. Хелперы С СЕМАНТИКОЙ (не голая
установка полей: `DeError.unexpected(expected, found)`) остаются именованными.
Вычищено 2026-07-09: SerError/DeError (of/at→new), IoError/Metadata/DirEntry
(of→new), 45 вызовов + serde-derive генератор. Проверка (185): W_NONVARIADIC_OF.

## 22. Перегрузки одного имени — все в одном модуле; различай — называй по-разному

- **Одно имя = одна операция (· согласовано 2026-07-07).** Вариации по ТИПУ/арности
  аргумента — перегрузки под одним именем (`@append(str)`/`@append(char)`/`@append(int)` —
  канон, НЕ `append_str`/`append_num`: неявных коэрций в Nova нет, выбор перегрузки
  однозначен; Go/Rust плодят имена вынужденно — у них перегрузок нет). ДРУГАЯ операция —
  другое имя (`append_repeat(s, n)` — повтор, не вариация типа).
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
  `char` (= `u32`, [D128](../../spec/decisions/02-types.md#d128)) — на границе `str` (`as_chars()`→`char`, `char.try_from(u32)`) и в char-методах
  (`'a'.is_alphabetic()`). **Публичные cp-функции принимают `u32`** (`general_category(cp u32)`); целочисленные литералы адаптируются к
  u32-контексту, поэтому `general_category(0x41)` остаётся валидным. **Fallible-функции, выдающие кодпоинт, → `Option[u32]`** ([D77](../../spec/decisions/02-types.md), а не `-1`-сентинел).
  **Bit-packing** нескольких кодпоинтов в один ключ (`(a<<21)|b`, > 32 бит) → явный `as int` (packed key — не кодпоинт). Обоснование:
  [D327](../../spec/decisions/02-types.md#d327).
- **Анти-паттерн:** `u64`/`usize` для offset/len «чтобы было ≥0» + россыпь `as u64`-кастов (литералы Nova — `int`). Знак не кодируем
  типом — отрицательный индекс/offset → доменная ошибка (`InvalidInput` / контракт `requires i >= 0`), как `SeekFrom.Start(int)` (Start < 0 → ошибка).
- **Почему signed, а не unsigned (обоснование/research):**
  [research/08-int-width-and-literal-inference.md §1](research/08-int-width-and-literal-inference.md) (3 раунда обсуждения, 2026-06-03)
  → формализовано в [D226 «Signed indexing convention»](../../spec/decisions/02-types.md#d226) (§Почему); `usize`/`isize` удалены
  [Plan 133](../plans/133-remove-usize-isize.md). Ключевое: **industry 7:3 за signed** (Go/Swift/Java/Kotlin/C#/Python/TS signed;
  Rust `usize`/C++ `size_t`/Zig — unsigned, причём **Stroustrup: «I regret using unsigned for size in STL»** + vocal Rust-regrets);
  **нет underflow-trap** (`xs.len() - 1` на пустом vec даёт `-1`, не паника как Rust `0usize-1`); sentinel `-1` для find; разности/diff
  естественно signed; mixed-arith без `as`-ceremony (**AI-first**: LLM пишет signed-индексацию вернее); bit-width-аргумент мёртв на
  64-bit (i64 = 9.2×10¹⁸ элементов).

---

## 25. Same-scope re-binding: pipeline/unwrap под одним именем, но не для несвязанного (D347)

Повторное `ro`/`mut`/`consume x = …` в ОДНОМ scope — новая переменная того же имени
(тип может меняться). Идиоматично, когда новое значение — **трансформация старого**:

```nova
ro input = read_line()
ro input = input.trim()            \ pipeline: та же сущность, уточнённая
ro input = parse_request(input)?   \ str → Request: старое имя больше не нужно
mut work = work                    \ «разморозка» ro→mut перед мутацией
```

Когда идиоматично: unwrap-цепочки (`ro s = s.trim()`), смена типа при парсинге/валидации
(`ro cfg = parse(cfg)?`), разморозка `mut x = x`. Не злоупотребляй для **несвязанных**
значений под одним именем (`ro x = user; … ro x = socket`) — это `W_SHADOW_UNRELATED`
(warn): читатель ждёт, что `x` — «тот же x». Для другой сущности — новое имя. Rebind
биндинга с непотреблённым `consume`-обязательством — hard error `E_REBIND_LIVE_CONSUME`
(потреби или переименуй). RHS видит СТАРЫЙ биндинг (`ro x = x + 1` читает прежний `x`);
замыкания/`defer` захватывают значение на момент создания/регистрации.

---

## 26. Деструктуризация вместо повторных полевых снапшотов (D411, · согласовано 2026-07-10)

Два и более подряд идущих `ro`/`mut`-биндинга, каждый из которых снимает ОДНО поле с
ОДНОГО и того же источника (`ro a = src.a` за которым `ro b = src.b`) — стилевой дрейф:
источник читается по частям вручную там, где язык уже даёт для этого биндинг за одно
выражение. Канон — record-деструктуризация (D411).

```nova
// ПЛОХО: два подряд полевых снапшота одного источника
ro status = resp.status
ro headers = resp.headers

// КАНОН: деструктуризация одним биндингом
ro { status, headers, .. } = resp
```

Когда НЕ применять:
- **Одно поле.** Единичный снапшот (`ro status = resp.status`) не мигрируется — правило
  триггерится от ДВУХ и более подряд идущих полей одного источника.
- **Переименование смешанных источников.** Если соседние биндинги снимают поля с
  РАЗНЫХ источников (`ro a = x.a; ro b = y.b`) — это не деструктуризация, а два разных
  факта; не сворачивай в один биндинг.
- **Вызов метода, не поле.** `ro status = resp.status()` — это метод (значение
  вычисляется), не структурный доступ к полю; D411-паттерн деструктурит только поля,
  такие строки не мигрируются.

Проверка (185): `W_DESTRUCTURE_SNAPSHOT` — эвристика «2+ подряд идущих `ro`/`mut`-биндинга
вида `x = <тот же ident>.x` (совпадение имени биндинга с именем поля)».

## 27. Бинарный оператор многострочного выражения — TRAILING, не ведущий (· согласовано 2026-07-10)

Продолжая многострочное выражение, бинарный оператор (`||`, `&&`, сравнения, арифметика)
ставится в **конце** предыдущей строки, а не в начале строки-продолжения.

```nova
// ПЛОХО: ведущий `||` на continuation-строке
fn is_ascii_letter(c int) -> bool {
    (c >= 65 && c <= 90)
    || (c >= 97 && c <= 122)
}

// КАНОН: trailing-оператор в конце строки
fn is_ascii_letter(c int) -> bool {
    (c >= 65 && c <= 90) ||
    (c >= 97 && c <= 122)
}
```

**Почему это не просто стиль.** Внутри statement-sequence (`{ }`-блок с несколькими
statement'ами) парсер завершает предыдущий statement, ЕСЛИ он выглядит полным, и НЕ
заглядывает вперёд за продолжением на следующей строке. Ведущий `||` — худший случай:
`||`/`|x|` также валидный старт zero-arg closure-литерала, поэтому строка-продолжение
молча парсится как ОТДЕЛЬНЫЙ discarded statement, а не как продолжение булева выражения.
Реальный прецедент (2026-07-10): `needs_quoting`/`is_ascii_ident_char` были **always-true**
именно из-за этого — round-trip-тесты не ловили, потому что always-true не менял конечный
результат сериализации (test-conventions.md, «Пиннинг-тест для silent-wrong-value багов»).
Type-checker ловит СЛЕДСТВИЕ для скалярных return-позиций (`E_CLOSURE_SCALAR_RETURN`,
D417); линт ниже ловит ПРИЧИНУ раньше и во всех контекстах.

Проверка (185): `W_LEADING_BINOP_CONTINUATION` — эвристика «строка-продолжение начинается
с бинарного оператора после строки, похожей на завершённый statement» (permissive на
безопасных конструкциях — arrow-body, `if`/`while`/`for`/`match`-условие без скобок,
многострочный `[...]`-литерал, `calc {}`-блок, унарные `-`/`+`/`*`).

## 28. Конструирование коллекций: литерал `[...]` — канон; `.of`/`.new` — по нужде (· согласовано 2026-07-10)

**Принцип.** Конструируй значение в самой лёгкой форме, при которой тип однозначен.
Явный конструктор (`.of`/`.new`) — только когда он несёт информацию, которой нет в
литерале (фиксирует тип), а не шум. **Канон — литерал `[...]`.**

**1. Литерал `[a, b, c]` — по умолчанию.** Когда тип выводится из элементов или из
контекста (аннотация переменной, тип параметра, тип поля):
```nova
ro xs = [1, 2, 3]                  // Vec[int] — выведен из элементов
ro ys [][]u8 = [a.bytes()]         // тип из аннотации
ro ps = [x"00 11 ff"]              // байтовые данные — литерал + x"..." (D412)
send([1, 2, 3])                    // тип из сигнатуры send
```

**2. `Vec[T].of(a, b, c)` — когда тип НЕ выводится ИЛИ нужна явная фиксация.** `.of`
оправдан, только если несёт информацию, которой нет в литерале:
```nova
ro ws = Vec[u32].of(1, 2, 3)       // фиксируем u32 — литерал [1,2,3] дал бы int
ro os = Vec[Option[int]].of(None)  // None сам тип не задаёт
ro es = Vec[Shape].of()            // пусто + точный тип на границе API
```

**3. `Vec[T].new()` — пустой, наполняю потом.** Элементов нет → тип обязателен явно
(`[]` тип не выведет); `.new()` яснее говорит «начну пустым»:
```nova
mut v = Vec[int].new()
for x in src { v.push(x) }
mut buf = Vec[u8].new().cap(1024)  // + преаллокация
```

**4. НЕ `Vec[T].from([...])`** — двойная упаковка (литерал → Vec → from). Вариадик =
`.of`, коллекция = литерал. См. [[feedback-vec-of-not-from-in-tests]].

> **RETRACTED 2026-07-20 (Plan 200 П16):** `Vec[T].from` удалён из языка целиком
> (не только анти-паттерн на литерале). Same-T конверсия существующей коллекции —
> `existing.clone()`; литерал — `.of(...)` (правило выше не изменилось); width-
> конверсия — явный поэлементный цикл. См. D259 AMEND, spec/decisions/02-types.md.

**Rule of thumb:** можешь литералом → `[...]`; надо зафиксировать тип → `.of(...)`
(сужение, `None`, граница API); пусто-заполню-потом → `.new()` (+`.cap(n)`). Аналог
Rust: `vec![1,2,3]` (вывод) vs `Vec::<u32>::new()` (явный тип) vs `with_capacity`.

**Проверка (реализовано, 2026-07-11):** `W_REDUNDANT_OF` (`compiler-codegen/src/lints.rs`,
реестр `CONV_RULES`) — `Vec[T].of(...)`, когда литерал `[...]` дал бы ТОТ ЖЕ тип
(избыточный `.of`). V1 консервативно: флагует только голые примитивные литералы
(`int`/`str`/`bool`/`char`), у которых `T` буквально совпадает с default-типом
литерала. НЕ срабатывает, когда `.of` фиксирует тип, который литерал не даёт
(`Vec[u32].of(1,2,3)` легально), на `Option`-элементах, пустых `.of()` и
non-literal/`[]T`-аргументах (вне V1 — семантический анализ). Связано с
[[feedback-ctor-new-not-of]].

## 29. Компаунд-присваивание вместо `x = x OP e` (· согласовано 2026-07-17)

**Принцип.** Если правая часть присваивания — бинарная операция, у которой
левый операнд СИНТАКСИЧЕСКИ идентичен цели присваивания, используй
компаунд-форму. Она короче и не рискует рассинхроном LHS/RHS-аккумулятора
при копипасте (`a = b + 1`, где `b` должно было быть `a`).

```nova
// ПЛОХО: цель повторена в правой части
total = total + delta
@count = @count - 1

// КАНОН: компаунд-форма
total += delta
@count -= 1
```

**Поддержанные операторы — ТОЛЬКО `+=`/`-=`/`*=`/`/=`.** `AssignOp`
(`compiler-codegen/src/ast/mod.rs`) несёт варианты `Add`/`Sub`/`Mul`/`Div` —
`Mod` и битовые (`&`/`|`/`^`/`<<`/`>>`) НЕ имеют компаунд-формы в Nova (парсер
лексирует только четыре compound-токена `+=`/`-=`/`*=`/`/=`; `%=`/`&=`/`|=`/
`^=`/`<<=`/`>>=` в языке не существуют). `x = x % e` и битовые — не мигрируй,
предлагать нечего.

**LHS — простое место без побочных эффектов у receiver'а:** голый `ident`,
`@field` (self-поле) или цепочка полей поверх них (`config.limit`, `@a.b`).
**Index-места (`x[i] = x[i] + e`) НЕ мигрируй** — компаунд-присваивание по
индексу (`x[i] += e`) в кодогене идёт ДРУГИМ путём, чем `x[i] = v`
(bounds-checked Vec-write / struct-value memcpy-write / fixed-array-write
ветки `emit_c.rs` гейтятся буквально `if *op == AssignOp::Assign` —
компаунд-оператор падает в общий fallback, легальность для нескалярных
элементов не подтверждена).

**Проверка (185, реализовано 2026-07-17):** `W_NON_COMPOUND_ASSIGN`
(`compiler-codegen/src/lints.rs`, реестр `CONV_RULES`) — `x = x OP e` при
существующем компаунде; не дублирует `W_STR_CONCAT_LOOP` (строковая
конкатенация в цикле — там канон StringBuilder, не `+=`).

## 30. `@max`/`@min`/`@clamp` вместо ручного if/else (· согласовано 2026-07-20)

**Принцип.** Если `if`/`else` вычисляет большее/меньшее из двух значений
или ограничивает значение диапазоном `[lo, hi]`, используй встроенный
метод-свойство вместо ручного ветвления.

```nova
// ПЛОХО: ручной максимум/минимум
ro bigger = if a > b { a } else { b }
if x > hi { x = hi }

// КАНОН
ro bigger = a.max(b)
x = x.min(hi)

// ПЛОХО: ручной трёхветочный clamp
ro y = if x < lo { lo } else if x > hi { hi } else { x }

// КАНОН
ro y = x.clamp(lo, hi)
```

**`@max`/`@min`** доступны на всех числовых типах (`int`/`uint`/`i8`..`i64`/
`u8`..`u64`/`f32`/`f64`, `defaults.nv`) и на любом `value`-типе, реализующем
собственный `@max`/`@min` (например `Duration`, `std/time/duration/core.nv`).
**`@clamp`** — бланкет `fn[T Ints] T @clamp(lo T, hi T) -> T`
(`std/prelude/protocols.nv`, D74 amend Plan 200) для всех целых + отдельные
конкретные `f32 @clamp`/`f64 @clamp` (`defaults.nv`, float ∉ `Ints`).
**Для НЕ-числового Comparable-типа своего `@max`/`@min`/`@clamp` бланкета
НЕТ** — до расширения линт (см. ниже) на такие типы просто не сработает
(нечего предложить взамен), не силентно, а потому что паттерн ручного
if/else там встречается редко и решение — за отдельным заходом, не этой
волной.

**Проверка (185, реализовано 2026-07-20):**
`W_MANUAL_MIN_MAX`/`W_MANUAL_CLAMP` (`compiler-codegen/src/lints.rs`,
реестр `CONV_RULES`) — оба СИНТАКСИЧЕСКИЕ (без типов), консервативны:
- **W_MANUAL_MIN_MAX**: expr-форма `if a > b { a } else { b }` (и зеркала
  `<`/`>=`/`<=`, и обе ветви местами) — ОБЕ ветви обязаны буквально
  совпадать с операндами сравнения (голый `ident`/`@field`/цепочка полей
  ИЛИ int/float-литерал — никаких вызовов/индексаций/произвольных
  выражений, побочных эффектов быть не может). Statement-форма БЕЗ `else`
  (`if x > hi { x = hi }`, включая mirrored — цель присваивания на правой
  стороне условия, `if deadline_ms > current_ms { current_ms = deadline_ms
  }` → `current_ms.max(deadline_ms)`) — тот же критерий операндов, плюс
  цель присваивания обязана совпадать с одним из операндов сравнения.
- **W_MANUAL_CLAMP**: трёхветочный `if X op1 B1 { B1 } else if X op2 B2 {
  B2 } else { X }` (`else if`-сахар И буквальный вложенный `else { if ...
  }` — обе формы) — байт-в-байт та же форма, что каноническая реализация
  `@clamp`; финальная ветвь обязана вернуть НЕИЗМЕНЁННЫЙ исходный
  операнд, обе проверки — РАЗНЫЕ направления (не обе `<`/не обе `>`).
  **НЕ покрывает** цепочки `x.min(hi).max(lo)`/`x.max(lo).min(hi)`
  (упомянуты как альтернативная форма антипаттерна) — они РАСХОДЯТСЯ с
  `@clamp` на инвертированном диапазоне `lo > hi` (алгебраически: при
  `x < lo` `@clamp` вернёт `lo`, а обе цепочки — `hi`); синтаксический
  линт не может исключить `lo > hi` на этапе сборки, значит подсказка была
  бы поведенчески рискованной — сознательное сужение, ноль реальных
  сайтов такой цепочки в корпусе на момент волны.
- Оба правила МОЛЧАТ внутри fn с ИМЕНЕМ буквально `min`/`max`/`clamp`
  (не по receiver'у — гасит и свободные функции-реализации) — `@min`/
  `@max`/`@clamp` сами реализованы РОВНО этими паттернами
  (`defaults.nv`/`protocols.nv`), предложение заменить их собственное
  тело на вызов самих себя было бы рекурсией на себя либо (для `clamp`)
  трогало бы канон-референсную реализацию вне периметра волны.

Разкраснение волной: 23 находки (std/spec_tests/examples) — все
исправлены на `.max()`/`.min()`/`.clamp()` (два сайта `Vec[T]
@first_n`/`@last_n`, std/collections/vec/views.nv, объединены в один
`.clamp(0, @len)` вместо двух последовательных `if`, семантически
идентично). `nova lint --rule W_MANUAL_MIN_MAX,W_MANUAL_CLAMP std
spec_tests examples` = 0.

## 31. Время: `Duration`/`Monotonic` в домене; голые `_ms int` — только провод и сериализация (· согласовано 2026-07-20)

- **Доменные сигнатуры и публичные API: длительность = `Duration`, момент = `Monotonic`.**
  Канон — флагманская дверь: `fn aggregate(sources []Source, budget Duration)`,
  `supervised(deadline: t0 + budget)` (D408: абсолютная точка). Не `budget_ms int`.
- **Голые `_ms int` разрешены ровно в трёх местах:**
  1. **эффект-провод** — `Time.sleep(ms int)` / `now_unix_ms() -> int` (int-провод Ф.1,
     `std/src/prelude/effects.nv` §Time): опы пересекают FFI (`nova_rt/effects.h`),
     скаляр ABI-прост; типизированный слой ПОВЕРХ — `time.duration` (`sleep(Duration)`,
     `sleep_until(Monotonic)`);
  2. **границы сериализации** — JSON/API-поля с единицей в имени (`elapsed_ms`,
     `wall_ms`, `budget_ms` в снапшотах) — числа там канон;
  3. **тест-хендлеры** — `with Time = effect Time { sleep(ms) {} ... }` пишутся по
     сигнатурам опов.
- **Анти-паттерн:** конверсия на двери (`budget.as_millis() as int`) и дальше int-плавание
  по всем внутренним сигнатурам (`fetch_one(latency_ms int)`, `deadline_elapsed_ms -> int`).
  Единица уезжает в имена, компилятор её не видит, `ms`/`ns`-путаница ловится только глазами.
- **Известное расхождение:** внутренности `examples/flagship/aggregator` (историческое,
  §выше и есть его портрет) — миграция на `Duration` после закрытия трёх codegen-маркеров
  хрупкости Duration-путей: `[M-vr-binop-wrapper-decl-order-standalone-cu]`,
  `[M-p67-path-call-const-receiver-method-ice]`, `[M-flagship-monotonic-now-bare-binding-ice]`
  (все — backlog-followups). До их закрытия точечный обход допустим ТОЛЬКО с
  маркер-ссылкой в комментарии (образец — `examples/mini_aggregator.nv`).

## 32. Пустая строка между top-level декларациями (· согласовано 2026-07-23; РАСШИРЕНО 2026-07-26 по ревью владельца «речь про все файлы»)

> **Амендмент 2026-07-26 (владелец: план 225 исполнен узко — «склеенные тексты» остались).**
> Правило «ровно одна пустая строка» распространяется ЯВНО на все стыки вертикального ритма:
> 1. **любые** соседние top-level блоки деклараций — включая голые (без `///`/атрибутов)
>    `}`→`fn`, `=> expr`→`type` и т.п. (допуск §32 для голых аксессоров ОДНОГО поля вплотную —
>    сохраняется);
> 2. после строки `module …` — пустая; между import-блоком и первым не-import — пустая;
> 3. перед каждым `test "…" {` — пустая;
> 4. **двойные+ пустые строки запрещены** («ровно одна», не «хотя бы одна»);
> 5. внутри длинных тел — рекомендация (не машинная норма): пустая строка между логическими
>    шагами-абзацами.
> Исполнение по всем репам — план [225.1](../plans/225.1-blank-line-full-sweep.md); машинный
> страж от дрейфа — линт W_DECL_SPACING (линт-волна №70).

**Правило.** Соседние top-level декларации (`fn`, `type`, `const`, `module`-item) разделяются
**ровно одной пустой строкой.** Ведущий doc-комментарий (`///`) и атрибуты (`#stable`, `#coerce`,
`#impl` …) **принадлежат следующей декларации** — пустая строка ставится **перед** ними, не между
ними и `fn`.

**Единица оформления — «блок декларации»:**
```
<пустая строка>
/// Doc-комментарий (может быть многострочным).
#stable(since = "0.1")
export fn Part @name() -> str => @name
```

**Анти-паттерн (то, что часто дрейфует — 115 стыков в 28 файлах на 2026-07-23):** декларация
вплотную к doc-комментарию/атрибуту следующей — «всё в кучу», глаз не разделяет:
```nova
export fn Part @name() -> str => @name
/// The part's filename…                       // ✗ нет пустой строки — новый блок слит с предыдущим
#stable(since = "0.1")
export fn Part @filename() -> Option[str] => @filename
```
Канон:
```nova
export fn Part @name() -> str => @name

/// The part's filename…                       // ✓ пустая строка отделяет блок
#stable(since = "0.1")
export fn Part @filename() -> Option[str] => @filename
```

**Границы правила:**
- Внутри блока (между `///`, `#attr` и `fn`) пустых строк НЕТ — они одно целое.
- Не больше одной пустой строки подряд между блоками (двойные/тройные пустые — тоже дрейф).
- Секции-разделители (`// ─── Numeric types ───`) — легальны, это осознанная группировка;
  пустая строка вокруг них по тому же правилу.
- Пара беглых аксессоров ОДНОГО поля без doc-комментариев (`@x()` + `mut @x(v)`) как один
  логический блок-свойство — допускается вплотную; но как только на любом появляется `///` или
  `#attr`, они разделяются пустой строкой (становятся отдельными блоками).

**Проверка/правка** — чисто пробельная, codegen байт-идентичен (гейт sweep'а). План
[225](../plans/225-blank-line-between-decls.md) — механический проход по 28 файлам.

## 33. `.collect()` вместо ручного for-push (· согласовано 2026-07-24)

**Принцип.** Если пустая коллекция объявляется РОВНО чтобы тут же наполниться
итератором один-в-один (`push` голой loop-переменной), это ручной collect —
канон `it.collect()`.

```nova
// ПЛОХО: ручной collect
mut chars []char = []char.new()
for c in scheme.chars() {
    chars.push(c)
}

// КАНОН
mut chars = scheme.chars().collect()   // тип `[]char` выводится, аннотация не нужна
```

**Форма (семья «ручная форма vs канон», §30).** Дрейф — `mut v = <пустой ctor>`
(`[]T.new()` / `Vec[T].new()` / литерал `[]`) НЕПОСРЕДСТВЕННО перед
`for x in <iter> { v.push(x) }`, где тело цикла — РОВНО `push` голой
loop-переменной. Только **identity-collect**: `.map`/`.filter`-варианты
(`push(f(x))`, `if c { push(x) }`) в первую версию не входят — там канон
`it.map(f).collect()` / `it.filter(c).collect()`, вводится отдельно.

**Границы (что НЕ дрейф):** преаллокация `[]T.new(cap: n)` (это НЕ пустой ctor —
намеренная ёмкость; часто ещё и с `push` ПОСЛЕ цикла — не чистый collect);
`push(f(x))` / `push` под условием (не identity); push НЕ той переменной /
НЕ в свежий пустой буфер.

**Проверка (Пункт 22 плана 200, реализовано 2026-07-24):** `W_MANUAL_COLLECT`
(`compiler-codegen/src/lints.rs`, реестр `CONV_RULES`) — СИНТАКСИЧЕСКИЙ, machine-
applicable fix-it (`mut v = <iter>.collect()`; для Range/closure-итераторов
подсказка помечается `MaybeIncorrect` — round-trip печати не байт-точен).
Прецедент clippy `manual_collect`/`needless_collect`.

## 34. Открытые диапазоны `[a..]` / `[..b]` / `[..]` вместо длинных границ (· согласовано 2026-07-24)

**Принцип.** Конец, равный `len()`/`byte_len()` того же receiver'а, и старт,
равный `0`, подразумеваются автоматически — писать их явно многословно.

```nova
// ПЛОХО: избыточные границы
ro rest = after_scheme[2..after_scheme.byte_len()]
ro head = bytes[0..n]
ro all  = v[0..v.len()]

// КАНОН
ro rest = after_scheme[2..]
ro head = bytes[..n]
ro all  = v[..]
```

**Три редукции (семья §30):** (1) `recv[a..recv.len()]` / `recv[a..recv.byte_len()]`
→ `recv[a..]`; (2) `recv[0..b]` → `recv[..b]`; (3) `recv[0..recv.len()]` →
`recv[..]`. `len()` — для `Vec`/slice, `byte_len()` — для `str`.

**Границы (что НЕ дрейф):** end — АРИФМЕТИКА (`x[a..x.len() - 1]`, реальная
граница, не «до конца»); end — `len()` ДРУГОГО receiver'а (`x[a..y.len()]` — не
тот же срез); инклюзивный `..=` (редукция до `len()` была бы OOB); receiver с
вызовом (двойной eval мог бы отличаться — только «чистые места»: ident/`@`/поле/
индекс).

**Проверка (Пункт 22 плана 200, реализовано 2026-07-24):** `W_MANUAL_SLICE_TO_END`
(`compiler-codegen/src/lints.rs`, реестр `CONV_RULES`) — СИНТАКСИЧЕСКИЙ (тип не
нужен, матчит по факту вызова len-метода на том же receiver), machine-applicable
fix-it (удаление избыточной границы по точному span'у). Прецедент clippy
redundant-slicing.

## 35. Отступ продолжения цепочки: `.method` глубже базового объекта (· согласовано владельцем 2026-08-02)

**Принцип.** Когда цепочный вызов переносится на следующую строку (`.method(...)`
с точки), продолжение пишется с **дополнительным уровнем отступа** относительно
строки, на которой начинается базовое выражение. Продолжение на одном уровне с
базой визуально сливает «что строим» и «что навешиваем» — особенно в
builder-цепочках с многострочными лямбда-аргументами.

```nova
// ПЛОХО: продолжение на уровне базы
fn build_router() -> Router =>
    Router.new()
    .get("/hello/{name}", fn(req ServerRequest) -> ServerResponse {
        ro name = req.param("name") ?? "world"
        ServerResponse.text(StatusCode.OK, "hello, ${name}")
    })!!

// КАНОН: продолжение глубже базы на один уровень
fn build_router() -> Router =>
    Router.new()
        .get("/hello/{name}", fn(req ServerRequest) -> ServerResponse {
            ro name = req.param("name") ?? "world"
            ServerResponse.text(StatusCode.OK, "hello, ${name}")
        })!!
        .get("/", fn(req ServerRequest) -> ServerResponse =>
            ServerResponse.text(StatusCode.OK, "hello, world"))!!
```

Индустриальный паритет: rustfmt, Kotlin style guide, prettier — везде
продолжение цепочки индентируется глубже первой строки выражения. Отступ —
пробелы, один уровень (4), как всюду в Nova.

**Линт.** W_CHAIN_CONTINUATION_INDENT (очередь: линт-волна после операторного
196-окна): строка, начинающаяся с `.` (продолжение цепочки), обязана иметь
отступ строго больше отступа первой строки выражения. Существующий код
перекрашивается волной после появления линта.

## 36. Имя собранного роутера — `app` (· согласовано владельцем 2026-08-02)

Снаружи строителя собранный Router зовётся `app` (`ro app = build_router()`;
`serve_router(listener, app, policy)`) — собранный роутер с middleware и есть
приложение; индустриальный паритет Express/Flask/Axum. Внутри строителя —
локальное `mut r = Router.new()`: там это ещё заготовка, не приложение.

## 37. Конверсии = `to_*` на источнике; `From*`-протоколы = bound'ы generic-диспетча (· согласовано владельцем 2026-08-02)

Два разных инструмента, НЕ противоречащих друг другу:
- **Конверсия значения** (источник в руках, целевой тип известен) — метод на
  ИСТОЧНИКЕ: `s.to_int()`, `b.to_str()`. Статические конструкторы/`try_from`-формы
  на целевом типе ретрактированы (канон 174.1, D321-AMEND).
- **Протокол-контракт для generic-кода** (поведение выбирает ЦЕЛЕВОЙ типовой
  параметр T; источник один для всех T) — `From*`-протокол как bound:
  `[T FromRequest]` + `T.from_request(req)` внутри generic-обвязки. Rust-паритет:
  трейт FromStr (контракт) + `s.parse::<T>()` (фасад) сосуществуют так же.

Правило: обычный код с известными типами — только `to_*`; generic-машинерия —
`From*`-bound, а пользователю по возможности фасад-метод на источнике поверх него.

## 38. Конфиг и состояние времени выполнения — РАЗНЫЕ типы (· согласовано владельцем 2026-08-05)

**Правило:** тип, описывающий настройки, не должен содержать изменяемое
состояние времени выполнения. Счётчики, таблицы, кэши, накопители живут не в
конфиге, а в замыкании или структуре, которая ими владеет.

**Образец — как надо** (`nova-polaris/src/middleware/ratelimit.nv`): таблица
корзин создаётся ВНУТРИ функции и захватывается замыканием-middleware; в
параметрах остаются только настройки.

```nova
fn ratelimit(capacity int, per_sec f64, per_client bool = false) Time -> Middleware {
    mut table = BucketTable.new()          // состояние — здесь
    middleware(fn(req, next) => rl_apply(capacity, per_sec, per_client, table, next, req))
}
```

**Образец — как не надо** (`nova-polaris/src/middleware/log.nv`): `AccessLog`
держит `counter AtomicInt` рядом с настройками `request_id`/`real_ip`. Счётчик —
не настройка, он там оказался «заодно».

**Почему это не вкусовщина.** Сегодня смешение просто неаккуратно. После
ввода линейности разделяемых ручек (план 248) оно станет ДОРОГИМ: линейность
распространяется на всё, что содержит такой тип, — конфиг со счётчиком
потянет за собой каждый тип, который его держит, и так далее по цепочке.
Плохое разделение перестанет быть вопросом ревью и станет ошибкой
компиляции у пользователей этого типа.

**Как проверить себя:** если поле нельзя осмысленно записать в файл настроек
или передать по сети как часть конфигурации — оно не конфиг.

## 39. Без лишних скобок вокруг первичного выражения (реестр 221.1 №463, · согласовано владельцем)

**Правило (устное, повторено владельцем много раз).** Скобки вокруг
литерала, идентификатора или уже готового `(expr)` — лишние, когда
приоритет операторов их не требует. Живой пример из реального кода:

```nova
// ПЛОХО
(500).to_millis().sleep()
(x).to_str()
((a + b))

// КАНОН
500.to_millis().sleep()
x.to_str()
(a + b)
```

**Границы (что НЕ дрейф) — скобки здесь ОБЯЗАТЕЛЬНЫ:**
- приоритет: `(a + b) * c`, `(a ?? b).f()`, `!(a && b)`;
- отрицательный литерал перед method-call: `(-5).abs()` — унарный минус НЕ
  сворачивается лексером в токен-литерал, без скобок `abs()` привяжется
  раньше минуса;
- кортежи `(a, b)`, unit `()`, `match (a, b) { ... }`,
  `consume (r, w) = expr` — деструктуризация, не группировка;
- fn-указательный тип с одним типом-параметром `fn(int)` и closure-full
  параметры — обязательный синтаксис сигнатуры;
- `priv(file)` (D307) и `@(x)` (self напрямую вызываемый у fn-newtype,
  `type Mid fn(H) -> H`) — обязательные marker/call-скобки, не группировка.

**Проверка:** `W_REDUNDANT_PAREN` (`compiler-codegen/src/lints.rs`, реестр
`CONV_RULES`) — СИНТАКСИЧЕСКОЕ (по токен-потоку `lex()`, НЕ по AST: парсер
не заводит `ExprKind::Paren`, факт скобок в дереве не сохраняется),
machine-applicable fix-it (удаляет ровно внешнюю пару). Фикстуры:
`spec_tests/conformance/neg/lint_redundant_paren_warns.nv` (находки),
`spec_tests/conformance/lint_redundant_paren_legal.nv` (законные формы,
0 находок).

## Известные расхождения для будущего sweep'а

0. **`docs/dev/idioms/size-accessors.md:41-42`** документирует `s.len()` как O(n) codepoint-count,
   но `strings.md:71` / `core.nv:22` — это compile-error `E_STR_NO_LEN` (Plan 152.1/D249).
   Idiom-doc устарел; `strings.md` авторитетен.
1. **Стиль-дрейф:** часть строкового кода ещё пишет `i = i + 1` вместо `i += 1` (`search.nv`,
   `core.nv`) и имеет инлайн-`;` (`parse.nv:64`). Новый код — `+=`/без `;`; механический sweep
   приведёт существующее в соответствие (Plan 91.18 Ф.8).
2. **Контракт-разрыв (высокоценный followup):** строковые методы почти без `requires`/`ensures`
   при идеальных целях (offset'ы, radix-диапазон). Правило 5 + elidable-bounds модель
   (`contracts.md:204`) — добавление бесплатно в runtime при доказуемости.

## Именование: `with_*` и методы-свойства (решение владельца, 2026-07-06)

- **`with_*` никогда не мутирует** — всегда возвращает НОВОЕ значение.
  **УТОЧНЕНИЕ (владелец, 2026-07-06): with_* легален ТОЛЬКО там, где копия
  честная** — value-типы (копируются целиком) либо полная замена кучевых полей.
  Для кучевых записей поверхностная копия делит потроха со «старым» объектом —
  независимости нет, это самообман + лишняя аллокация. Канон для кучевых —
  мутирующее беглое свойство `mut @x(v) -> @` (см. ниже). Пример НЕПРАВИЛЬНОГО:
  `@header(n,v) -> ServerResponse` копией; ПРАВИЛЬНО: `mut @header(n,v) -> @`.
- **Методы-свойства одним именем** (перегрузка по арности, D84): чтение — `@x() -> T`,
  запись — `mut @x(v T) -> @` (беглая запись; с D409 возврат приёмника автоматический).
  Предпочтительный стиль доступа к полям вместо пар `get_x`/`set_x`.
- **Сеттер возвращает `@`** (D117 AMEND-2, решение владельца 2026-07-06): `-> @` у
  метода установки свойства — умолчание, не опция. Бесплатно (возврат автоматический,
  D409) и даёт цепочки `r.header("a","1").header("b","2")`. `-> ()` у сеттера — только
  с обоснованием; установка, которая может отказать, — не свойство, а операция
  (`Result`/эффект).
- **У `with_` — ДВА канонических смысла, различаются по СИГНАТУРЕ** (решение
  владельца 2026-07-17): `with_x(значение) -> НовоеЗначение` — копия (правило
  выше, mut-приёмник запрещён — это находка `W_WITH_MUTATOR`); `with_x(замыкание)
  fn(...) -> R) -> R` — scope-guard: выполнить `body` ПОД ресурсом (взять/
  освободить лок вокруг вызова), прецедент Kotlin `withLock { ... }`. У
  scope-guard-формы mut-приёмник ЛЕГИТИМЕН (лок/анлок — это и есть мутация),
  возврат — результат closure `R`, не «новая копия `Self`». Различитель для
  тула/ревьюера — параметр fn-типа в сигнатуре; `nova lint` (`W_WITH_MUTATOR`)
  распознаёт этот случай автоматически и молчит на нём (`Mutex.with_lock`,
  `RwLock.with_read`/`with_write`, `ReentrantMutex.with_lock`,
  `Semaphore.with_permit` — std/runtime/sync.nv).

## Эффекты: strict-режим (конвенция владельца 2026-07-13)

Внутренние модули (`std/**`) и программы (`examples/**`, включая flagship-демо) ОБЯЗАНЫ собираться
с `--strict-effects` (экспериментальный флаг nova-cli):
- функция, вызывающая эффектную функцию вне `with E = ...`-скоупа, обязана нести эффект в СВОЕЙ
  сигнатуре (транзитивность, `E_UNDECLARED_TRANSITIVE_EFFECT`);
- fn-значение не коэрсится в fn-тип с меньшим набором эффектов (`E_EFFECT_ERASED_IN_FN_TYPE`).
Долг миграции трекается `[M-strict-effects-conformance-sweep]` (docs/plans/wip/strict-effects-debt.txt).
Для пользовательского кода флаг опционален (флип в дефолт — только через D-амендмент).

## `priv` выносится на тип, если приватны ВСЕ поля

Правило владельца 2026-08-09: «в Nova принято `priv` выносить наружу, если все
`priv` (линт такое обязан ловить)».

```nova
// ✅ канон — все поля приватны, модификатор на типе
type TcpReadHalf consume value priv { handle *() }

// ❌ то же самое, но `priv` повторён у каждого поля
type TcpReadHalf consume value { priv handle *() }
```

Обе формы законны по [D281](../../spec/decisions/02-types.md) — решение задаёт
возможность, но не выбор. Выбор здесь: **если приватны все поля, модификатор
принадлежит типу**; если приватна часть — `priv` ставится у конкретных полей.

Замер на день записи: 12 носителей отклонения среди однострочных объявлений
`std/src/**` (многострочные не считались). Реестр — [№497](../plans/221.1-bug-sweep.md);
энфорс — правилом линта, волна [254](../plans/254-process-rules-without-mechanism.md) Ф.2.
