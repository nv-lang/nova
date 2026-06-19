<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova `.nv` — конвенция кодирования

> **Директивный документ** для всех контрибьюторов (включая AI-агентов). Описывает,
> **как писать `.nv`-код** так, чтобы он совпадал по стилю с остальным `std/`. Дополняет
> [project-philosophy.md](project-philosophy.md) (как принимать решения) и
> [perf-conventions.md](perf-conventions.md) (модель стоимости).
>
> Правила выведены из паттернов, уже действующих в `std/runtime/string/` и `std/unicode/`.
> Каждое правило заземлено в реальном `file:line`. Правила 1, 2, 8 (специфичные для
> строк/Unicode) живут также в [strings.md](strings.md); правила 3–7, 9 — общеязыковые.
> Кросс-ссылки: правило 5 → [contracts.md](contracts.md); правило 6 → [parameters.md](parameters.md).

---

## 1. `as_*` / `to_*` / голое имя

- **`as_*` = O(1) zero-copy view или ленивый итератор**, заимствующий receiver.
  `str.@as_bytes()` возвращает реальную `ro []u8` реинтерпретацию (`core.nv:88`);
  `str.@as_chars()` строит `CharsIter` value-record за O(1) (`chars.nv:149`). `as_*` НИКОГДА
  не аллоцирует-и-копирует и НИКОГДА не потребляет (`consume`) receiver.
- **`to_*` = O(n) аллоцирующая владеющая копия.** `@to_bytes()`/`@to_chars()` аллоцируют
  свежий `[]u8`/`[]char` (`core.nv:61,256`). Имя на `to_` = вызывающий платит за аллокацию.
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
- **На `char` ASCII-варианты — явные `@is_ascii_*`/`@to_ascii_*`** (`defaults.nv:59-85`),
  т.к. unqualified `@is_alphabetic`/`@to_uppercase` — Unicode (`core.nv:466,493`).
- **`eq_ignore_ascii_case` (ASCII, table-free, core) vs `eq_ignore_case` (Unicode fold,
  std.unicode)** (`core.nv:347` / `case.nv:121`). Правило: когда оба слоя дают один предикат
  на одном типе, ASCII-вариант несёт `ascii` в имени, Unicode владеет голым именем.
- **Резолюция одноимённых str-методов из двух stdlib-модулей сейчас НЕ диагностируется**
  (`check_extension_method_policy` early-return для str, types/mod.rs:5927) — избегайте
  коллизий именованием и документируйте правило резолюции.

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
- **Option = отсутствие это норма.** `@find -> Option[int]`, `@strip_prefix -> Option[str]`,
  `CharsIter @nth -> Option[char]`, `@split_once -> Option[(str,str)]`, `@parse_int_opt`.
- **Result = recoverable с payload для вызывающего.** `@try_parse_int -> Result[int,
  ParseIntError]`, `str.from_utf16 -> Result[str, Utf16Error]`, `str.try_from_codepoint`.
- **Дуальный API: bare(throw) + `try_`(Result) + `_opt`(Option).** Конвенция (D77/D25,
  protocols.nv:126-128): `from`/bare даёт значение или throw (требует эффекта `Fail[E]` в
  сигнатуре); `try_*` возвращает Result; `_opt` — Option. `try_from` — fallible-конвертер;
  `from` — total/infallible направление.
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
  (`requires radix>=2 && radix<=36` исчезает на `parse_int(16)`). `requires len>=0` на
  `@truncate(len)`/`@append_repeat(s,n)`.
- **НЕ добавляйте `ensures result >= 0` на size-accessors** (`byte_len`/`len`/`cap`/`count`) —
  non-negativity уже встроена в SMT-бэкенд как аксиома (`z3.rs:547-558`, План 33.6); это
  no-op, ничего не доказывает downstream. Аналогично Vec `@len()` намеренно без такого ensures.
- **НЕ пишите `ensures` с вызовами non-`#pure` методов** (`@len()`/`s.byte_len()` не pure →
  не скомпилируется) и **tuple-выражениями в `ensures`** (E2401). `ensures` — для отношений,
  выразимых через чистые термы.
- Помечайте side-effect-free helper'ы `#pure`, чтобы их можно было звать **внутри других
  контрактов** (`contracts.md:314`) — `is_ascii_ws`, `is_cont`.
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
  `@equal`, `@starts_with`, `@ends_with`, `@contains`, `@find`, `@split` (`search.nv`).
- **Всегда пре-сайзьте.** `[]u8.with_capacity(n)`/`StringBuilder.with_capacity(n)` до любого
  заполнения; не растите инкрементально, если финальный размер известен. Для таблиц-карт
  (case/decomp) парсите один раз в lazy-static `HashMap[int, Vec[u32]]`, не per-cp.
- Остаточный push-цикл допустим только на честно нерегулярном пути (lossy UTF-8 FFFD-замена,
  `core.nv:207-219`) — и документируется как таковой.

## 8. Имена итераторов/view — единый суффикс `*Iter`

- **Все ленивые/потоковые декодеры и адаптеры заканчиваются на `Iter`**: `CharsIter`,
  `CharIndicesIter`, `VecIter`, `RangeIter`, `MapIter`/`FilterIter`/`TakeIter`/
  `EnumerateIter`/`StepByIter`, и сегментаторы Unicode `GraphemesIter`/`WordsIter`/
  `SentencesIter`. **`*View` НЕ используем** — суффикс `View` ошибочно намекает на
  random-access окно (Rust никогда не говорит `GraphemeView`). Сегментаторы — это стримы с
  `next()`/`count()`/`is_empty()`, ровно как `CharsIter`.
- **Все такие типы — `value` записи** (стек, без per-iteration heap-churn); внутренний
  курсор `priv(type)`. Итератор — `value priv(type)`, никогда не heap-класс.
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
- **Нужен индекс — `enumerate()`/`CharIndicesIter`,** не ручной счётчик:
  `for (i, x) in xs.enumerate() { … }`; `for (i, c) in s.char_indices() { … }`.
- **Шаг/реверс — адаптеры,** не арифметика в `while`: `0..n step_by 2` /
  `(0..n).rev()` вместо `i += 2` / `i -= 1` в `while`.
- **`while` оставляйте ТОЛЬКО для честно не-итераторных циклов** — условие не
  сводится к проходу по диапазону/коллекции: координационные циклы (ожидание
  события/флага), парсеры с переменным шагом курсора, fixpoint до сходимости.
  Такой `while` документируйте, если неочевидно, почему это не `for in`.
- Существующий код с индексным `while` (`canonical_order`/`compose` в
  `unicode/normalize.nv` — insertion-sort/fold с нелинейным курсором) —
  пограничный: переписывать только если `for in` не теряет ясность.

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
