# Аудит дизайна Nova по «100 Go Mistakes and How to Avoid Them»

Сверка дизайна языка Nova с каталогом ошибок из книги Teiva Harsanyi
«100 Go Mistakes and How to Avoid Them». Книга описывает грабли
*использования* Go; для Nova вопрос ставится иначе — **заставляет ли
дизайн наступать на ту же ошибку, разрешает её или запрещает**.

Дата аудита: 2026-05-20. Источники — `spec/overview.md`, `spec/effects.md`,
`spec/conversions.md`, все 10 файлов `spec/decisions/`, планы 19/20/21/30/31/47/49/61,
`std/time/duration.nv`.

---

## TL;DR

Nova спроектирована как «анти-Go»: в тексте решений (D33, D71, D79, D90)
есть прямые ссылки на конкретные грабли Go. Дизайн **осознанно устраняет
большинство ошибок книги**.

| Категория | Кол-во | Примеры |
|---|---|---|
| ✅ Закрыто дизайном | ~60 | nil → Option, loop-var capture, defer в цикле, exhaustive match, structured concurrency, typed Duration |
| ➖ Неприменимо (нет фичи) | ~25 | nil receiver, named result params, WaitGroup, sentinel errors |
| ⚠️ Свой риск / частично | ~12 | narrowing `as`-cast, `any`, оборачивание ошибок, закрытие ресурсов |
| ❌ Воспроизводится | ~3 | **гонки данных**, двойное логирование ошибок, IEEE `NaN != NaN` |

**Главный незакрытый риск — гонки данных.** Убрав мьютексы, язык не дал
взамен ни статической, ни рантайм-защиты.

---

## Глава 2. Организация кода и проекта

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 1 | Unintended variable shadowing | ⚠️ Частично |
| 2 | Unnecessary nested code | ✅ Закрыто |
| 3 | Misusing init functions | ✅ Закрыто |
| 4 | Overusing getters/setters | ✅ Закрыто |
| 5–7 | Interface pollution / producer-side / returning interfaces | ✅ Закрыто |
| 8 | `any` says nothing | ❌ Воспроизводится |
| 9 | Confusion about generics | ✅ Закрыто |
| 10 | Type embedding problems | ✅ Закрыто |
| 12–14 | Project misorganization / utility packages / package name collisions | ✅ / ⚠️ |
| 15–16 | Missing documentation / not using linters | ✅ Закрыто |

**Затенение переменных** (#1). Оператор `:=` сознательно выкинут — D33/D34
прямым текстом называют его «источником shadowing-багов Go», остался
только `let`/`let mut`. Но затенение обычной переменной во вложенном
блоке правилом не регламентировано: линтер ловит только затенение
prelude-имён (D83). Лёгкий пробел.

**Вложенность, `init`, геттеры** (#2–4). Early-return поощряется явно
(D23), линтер ругается на лишний `return`. Аналога Go `init()` нет вовсе —
убран целый класс багов с порядком инициализации и скрытыми эффектами при
импорте пакета. Геттеры есть, но вызов метода всегда со скобками
(`acc.balance()`); property-механизмы C#/Kotlin отвергнуты (D31/D35) — нет
невидимого «поле или вызов».

**Интерфейсы и `any`** (#5–8). `protocol` структурный, объявляется явным
keyword'ом — намерение видно с первого токена; эффекты входят в сигнатуру
метода, поэтому реализация не может привнести лишний эффект (контракт
строже Go-interface). Но `any` (top-тип, прецедент Go `interface{}`) так
же «слеп»: type-pattern-match для извлечения конкретного типа из `any`
(`match x { int(n) => ... }`) требует runtime-tag и **пока не реализован**
(D53). Здесь Nova повторяет Go-проблему «`any` says nothing».

**Generics и embedding** (#9–10). Мономорфизация + bounds через
`[T Protocol]` (D72). Встраивание `use name Type` (D39): alias
обязателен, конфликт имён методов даёт **compile-error ambiguous** (Go
молча затеняет) — заметно безопаснее. Риск: метод-обёртка, вызывающая
делегата без явного `@field.method()`, даёт бесконечную рекурсию (это
указано в спеке как известный острый угол).

**Организация, коллизии, документация** (#12–16). Сильная зона. D78 —
жёсткий path/module enforcement (имя директории = имя пакета),
циклические импорты запрещены с указанием полного цикла, wildcard-импорт
запрещён, `internal/` получает 3-сегментное имя против коллизий. Развитая
doc-инфраструктура (D101–D107), doc-тесты компилируются и запускаются —
защита от дрейфа примеров. Единственное: «утилитарные пакеты»
(`util`/`common`) прямо не запрещены, и пример `app.shared` для разрыва
циклов фактически поощряет такой пакет — линтер про это молчит.

---

## Глава 3. Типы данных

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 17 | Confusion with octal literals | ✅ Закрыто |
| 18 | Neglecting integer overflows | ✅ / ⚠️ |
| 19 | Not understanding floating points | ❌ IEEE-семантика сохранена |
| 20–28 | Slice length/cap, nil vs empty, copies, append side-effects, leaks; map init/leaks | ✅ / ➖ / ⚠️ не специфицировано |
| 29 | Comparing values incorrectly | ✅ Закрыто |

**Восьмеричные литералы** (#17). Только `0o755`; ведущий ноль не меняет
систему счисления (`decimal-int` не допускает leading-zero как octal).
Классическая Go/C-путаница убрана.

**Целочисленное переполнение** (#18). Overflow при обычной арифметике
(`+`, `*`) — это **паника** (D13), а не молчаливый wraparound как в Go.
Строже Go. **Но**: narrowing `as`-cast (`x as u8`) делает **молчаливый
wraparound** по модулю 2^N — здесь Nova повторяет Go-грабли «тихая потеря
данных при конверсии».

**Float** (#19). `f64.eq` / `==` сохраняет IEEE-семантику `NaN != NaN` —
ловушка не закрыта (она по сути неустранима без отказа от IEEE 754).
float→int через `as` — defined saturation (NaN→0, ±∞→границы), что лучше
C/C++ UB.

**Слайсы и мапы** (#20–28). `len()` и `capacity()` разделены (инвариант
`len() ≤ capacity()`). Различие «nil-слайс vs пустой слайс» неприменимо —
nil в языке нет. Обращение к мапе по отсутствующему ключу → `Option[V]`,
**не zero-value как в Go** — прямая защита от грабли #34. Темы
**копирования слайсов, side-effects от `append`, утечек памяти через
под-слайсы, роста мап** в спеке прямо не специфицированы — формально не
закрыты (managed GC и отсутствие shared-семантики снижают риск, но это не
гарантия дизайна). **Рекомендация:** зафиксировать семантику в
`02-types.md` / `05-memory.md`.

**Сравнение значений** (#29). Структурное сравнение через protocol
`Hashable`/`Eq`; «сравнение несравнимых типов с runtime-паникой» (Go)
неприменимо — типизация структурная.

---

## Глава 4. Управляющие конструкции

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 30–33 | Range loop: copy элемента, оценка аргументов, pointer-элементы, итерация мапы | ✅ Закрыто |
| 34 | Ignoring how break works | ⚠️ Меток циклов нет |
| 35 | Using defer inside a loop | ✅ Закрыто |

**Range-цикл и захват loop-переменной** (#30–33). Главный Go-баг закрыт.
Переменная цикла — свежий immutable binding на каждой итерации (D58); в
`parallel for` захватывается **по значению** (snapshot на момент spawn,
D71) — спека прямо приводит пример «иначе `parallel for x in [1,2,3]` дал
бы 9, не 6». Менять элемент массива в цикле — только явно через
`for mut x` или индекс.

**`defer` в цикле** (#35). В Nova `defer` — **scope-level, а не
function-level** как в Go (D90, Plan 20): внутри тела `for` срабатывает на
каждой итерации. Прямо чинит Go-грабли «defer копится до конца функции».
Аргументы вычисляются eager (как Go), тело — LIFO. Дополнительно тело
defer infallible и no-suspend — нельзя проглотить ошибку в cleanup.

**`match`**. Исчерпывающность обязательна, fallthrough отсутствует —
забытый `break` (C/Go) невозможен. `break`-меток для выхода из вложенных
циклов нет — мелкий пробел относительно Go.

---

## Глава 5. Строки

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 36–37 | Rune concept / inaccurate iteration | ⚠️ Не специфицировано |
| 38 | Misusing trim | ➖ Неприменимо |
| 39 | Under-optimized concatenation | ✅ Частично |
| 40 | Useless string conversions | ✅ Закрыто |
| 41 | Substrings and memory leaks | ⚠️ Не специфицировано |

`str` хранит UTF-8 байты, есть отдельные типы `char` (codepoint) и `byte`.
**Конкатенация** (#39): интерполяция `"${...}"` компилируется в
`StringBuilder` с pre-size estimate — одна аллокация, спека явно пишет
«нет O(N²) от цепочки `+`». Для конкатенации `+=` в явном цикле
рекомендуется `StringBuilder`. **Конверсии** (#40): `str as int` запрещён,
парсинг только через `T.try_from(s)?` — защита от тихих ошибок.
**Итерация строки** (по байтам vs рунам, #36–37) и **утечки от подстрок**
(#41) спекой не зафиксированы — `Q-string-patterns` открытый вопрос.

---

## Глава 6. Функции и методы

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 42 | Wrong receiver type | ✅ Закрыто (явный `mut @`) |
| 43–44 | Named result params + их side effects | ➖ Неприменимо |
| 45 | Returning a nil receiver | ➖ Неприменимо |
| 47 | Defer arguments/receivers evaluation | ✅ Закрыто |

Named result parameters в Nova нет, и `defer return X` запрещён (D90) —
Go-грабли «defer меняет именованный результат» структурно невозможна.
nil-receiver неприменим (nil нет). Receiver — через `@`-синтаксис,
мутация всегда явная (`fn T mut @method`), компилятор проверяет, что
не-мутирующий метод действительно не пишет.

---

## Глава 7. Обработка ошибок

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 48 | Panicking | ✅ Закрыто |
| 49 | Ignoring when to wrap an error | ⚠️ Цепочка причин не сохраняется |
| 50 | Comparing an error type inaccurately | ✅ Закрыто |
| 51 | Comparing an error value inaccurately | ✅ Закрыто (нет sentinel) |
| 52 | Handling an error twice | ❌ Воспроизводится |
| 53 | Not handling an error | ✅ Закрыто |
| 54 | Not handling defer errors | ✅ Закрыто |

**Паника** (#48). `panic` отделена от ошибки-значения (D13/D25), не ловится
в коде (нет `recover`-антипаттерна), убивает только текущий fiber;
supervisor рестартует. Граница чёткая: «обработать можно» → `throw` +
`Fail[E]`, «обработать нельзя» → `panic`.

**Игнорирование ошибки** (#53). Невозможно: `Fail[E]` strict-транзитивен —
компилятор требует либо объявить эффект (проброс), либо обработать через
`with`-handler. Force-unwrap (`.unwrap()`/`!`) сознательно отсутствует
(D85) — никаких скрытых panic'ов через короткий синтаксис.

**Оборачивание ошибок** (#49). **Пробел.** Аналога `fmt.Errorf("%w")` +
`errors.Is`/`errors.As` с автоматической цепочкой причин нет. Оборачивание
делается вручную через re-throw в handler'е; оригинальная ошибка доступна,
только если её явно положить полем в новый тип. Грабли «потеря контекста
при оборачивании» структурно не решена — auto-cascade через `From`
сознательно отложен (Plan 61.A).

**Проверка типа/значения** (#50–51). Варианты sum-type матчатся через
`match` внутри handler'а; lookup handler'а — по точному типу, subtype-aware
специально не делается (D65) — это точнее, чем Go `errors.As` по дереву
обёрток. Sentinel-ошибок (`var ErrX = errors.New(...)` + сравнение `==`)
нет — идентичность ошибки = её тип/вариант, грабли #51 неприменима.

**Двойная обработка** (#52). **Воспроизводится.** Защиты от «залогировал
и пробросил дальше» нет — паттерн log-and-rethrow в handler'е (D65)
легален и идиоматичен. Та же грабля, что в Go. **Рекомендация:** линт.

**Ошибки в `defer`** (#54). Закрыто радикально: в `defer`/`errdefer`
эффект `Fail` запрещён компилятором (`throw`/`?`/`!!` → compile-error),
cleanup физически не может породить теряемую ошибку.

---

## Главы 8–9. Конкурентность (foundations + practice)

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 55 | Mixing concurrency/parallelism | ✅ Закрыто |
| 56–59 | Concurrency faster / channels vs mutexes / race problems / workload type | ✅ / ❌ |
| 60–61 | Misunderstanding contexts / propagating wrong context | ✅ / ⚠️ |
| 62 | Goroutine without knowing when to stop | ✅ Закрыто |
| 63 | Goroutines and loop variables | ✅ Закрыто |
| 64 | Deterministic behavior with select | ✅ Закрыто |
| 65–67 | Notification / nil channels / channel size | ✅ Закрыто |
| 69–70 | Data races with append / mutexes with slices & maps | ❌ Воспроизводится |
| 71–74 | WaitGroup / Cond / errgroup / copying sync types | ✅ Закрыто |

**Concurrency vs parallelism** (#55). Разделены синтаксически: `for`
(statement, side-effects) vs `parallel for` (expression типа `[]T`).
Suspension — ambient runtime, нет «цвета функции» (D62).

**Запуск задачи без понимания, когда остановить** (#62). Закрыто: `spawn`
разрешён только внутри structured-scope (`supervised`/`parallel for`/
`select`), вне scope — compile-error. Fire-and-forget требует явного
`detach` + эффекта `Detach` в сигнатуре — фоновая задача видна в типе.
Спека прямо называет это «главной ошибкой Go fire-and-forget». Даже `main`
обёрнут в implicit supervised scope (D92).

**Захват loop-переменной** (#63). Закрыто (см. главу 4): immutable binding
+ capture-by-value snapshot на момент spawn.

**`select`** (#64). При нескольких готовых каналах — недетерминированный
выбор (Fisher-Yates shuffle), спека предупреждает не полагаться на
порядок. Есть `default`-ветка и arm-guards `if cond`. Все каналы закрыты +
нет default → **panic** «select: all channels closed», а не busy-spin на
zero-value как в Go.

**Каналы** (#65–67). Send в закрытый канал возвращает `false` (не паника
как в Go); nil-каналов нет; capability-split (D91) — `ChanWriter`/
`ChanReader` раздельны, writer не может читать (compile-error).
Notification-канал — идиома `Channel[()]`. Буфер всегда bounded
(unbounded отвергнут как antipattern).

**WaitGroup / errgroup / sync-типы** (#71–74). WaitGroup нет вообще —
join встроен в `supervised` scope; нечего рассинхронизировать, скопировать
или забыть `Wait`. Реальная ошибка всегда перетирает отмену в
`first_error` (Go errgroup при first-wins теряет реальную ошибку — Nova
нет).

**Контексты и отмена** (#60–61). Аналог `context.Context` — `CancelToken`,
несёт **типизированную** причину (`reason() -> Option[T]`) — лучше Go
`context.Cause()` (`error`). Известная незакрытая проблема: cancel-throw
сейчас конфлирует с `Fail` (`[M-cancel-throw-routing]`, Plan 49).
`with_timeout`/`race`/`within` спроектированы, но **не реализованы**
(Plan 47 Ф.5 упирается в closures-in-generics) — базового таймаута пока
нет.

### ⚠️ Гонки данных (#58, #69, #70) — главный риск

Мьютексы и атомики сознательно выкинуты («channel-only», D79), но язык
**разрешает** shared `mut` через захваты в `spawn`, и спека сама честно
называет это UB в preemptive runtime. При этом:

- нет borrow-checker (D75: «в Nova его нет — GC + эффекты»);
- нет аналога Rust `Send`/`Sync`;
- **нет race-детектора** (`go test -race`-аналога);
- memory model между fiber'ами не специфицирована (`Q-memory-model`).

Пока bootstrap single-threaded cooperative — гонок физически нет. Но с
включением M:N + preemption (Plan 44, D103) `let mut`, захваченный двумя
`spawn`, станет настоящей data race **без какой-либо диагностики**.
Go-ошибки про гонки здесь воспроизводимы — защита целиком возложена на
дисциплину «только каналы».

---

## Глава 10. Стандартная библиотека

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 75 | Wrong time duration | ✅ Закрыто |
| 76–77 | JSON / SQL mistakes | ⚠️ Не специфицировано |
| 78 | Not closing transient resources | ⚠️ Частично |
| 79–80 | HTTP return statement / default client & server | ⚠️ Частично |

**Длительности времени** (#75). Закрыто отлично. `Duration` — typed
newtype над `i64` nanos (`std/time/duration.nv`), конструкторы по единицам
(`30.seconds()`, `Duration.from_millis(n)`), полная арифметика и
сравнение. Более того — **отдельные типы `Timestamp` (wall-clock) и
`Monotonic`** (D124): смешивание даёт compile-error, что закрывает и
Go-грабли с монотонным временем. Go-ошибка `time.Sleep(10)` (10 нс вместо
10 секунд) здесь структурно невозможна — `Time.sleep` принимает
`Duration`, не голый `int`.

**Закрытие ресурсов** (#78). Частично: есть `defer file.close()`
(scope-level, надёжнее Go function-level), но RAII/Drop нет — авто-закрытие
язык не гарантирует. Грабли «забыл закрыть resource» возможна.

**JSON / SQL / HTTP** (#76–80). JSON и SQL — обычные модули/эффекты,
специфических гарантий (unknown fields, числовая точность, prepared
statements) в спеке нет. Дефолтные таймауты HTTP-клиента не зафиксированы —
Go-грабли «`http.Client` без таймаута висит вечно» формально не закрыта.
HTTP-handler работает как fiber-на-запрос: panic = смерть fiber'а,
runtime отдаёт 500, остальные запросы продолжают.

---

## Глава 11. Тестирование

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 81 | Not categorizing tests | ✅ Закрыто |
| 82 | Not enabling the race flag | ❌ Нет race-детектора |
| 83 | Test execution modes (parallel, shuffle) | ✅ Закрыто |
| 84 | Not using table-driven tests | ➖ Идиома не выделена |
| 85–86 | Sleeping in tests / time API | ✅ Закрыто |
| 87 | Testing utility packages | ✅ Закрыто (handlers) |
| 88 | Inaccurate benchmarks | ✅ Закрыто |

Моки — через подмену handler'ов, без мок-фреймворков (D41). **Детерминизм**
(#85–86): `Time` → `fixed_ms`/`mut_clock`, `Random` → `seeded(seed)` —
«sleeping in unit tests» и зависимость от реального времени структурно
устранены. Параллельный прогон (`--jobs`), per-test timeout, категории,
форматы JSON/TAP/JUnit, `--rerun-failed` (D89, Plan 26). Бенчмарк-DSL
с `bench.opaque` против constant-folding и drift-detection (D121).
**Race-флага нет** (#82) — прямое следствие пробела с гонками выше.

---

## Глава 12. Оптимизации

| # | Ошибка Go | Статус Nova |
|---|---|---|
| 91–94 | CPU caches / false sharing / ILP / data alignment | ⚠️ Не адресовано |
| 95 | Stack vs heap | ✅ Escape analysis |
| 96 | Reducing allocations | ✅ Частично |
| 97–99 | Inlining / diagnostics / GC | ⚠️ / ✅ roadmap |

Слабо покрыто на уровне дизайна — ожидаемо для спеки прикладного языка.
Escape analysis есть (стек vs heap, #95), `StringBuilder`/`WriteBuffer`
против O(N²)-конкатенации (#96), GC introspection (`std.runtime.gc`) и PGO
в roadmap. CPU-кеши, false sharing (#92), выравнивание полей (#94), ILP не
адресованы. Для backend-ниши приемлемо; при появлении real-time-амбиций —
пробел.

---

## Приоритеты для устранения

1. **Гонки данных** (критично). К моменту включения M:N/preemption нужен
   либо компиляторный запрет shared-`mut`-захватов в `spawn`, либо
   runtime race-детектор, либо `Send`/`Sync`-аналог. Сейчас дизайн
   обещает безопасность, но не обеспечивает её. Зафиксировать также
   memory model между fiber'ами (`Q-memory-model`).
2. **Оборачивание ошибок** (#49). Решить, нужна ли цепочка причин
   (`errors.Is`/`As`-аналог); сейчас контекст при re-throw теряется.
3. **Двойное логирование ошибок** (#52). Линт против log-and-rethrow.
4. **Narrowing `as`-cast** (#18). Молчаливый wraparound — сделать хотя бы
   линт-warning при потере данных.
5. **`any`** (#8). Без type-pattern-match остаётся «слепым», как
   `interface{}` — достроить runtime-tag downcast (D53).
6. **HTTP-таймауты по умолчанию** (#80) и **гарантии закрытия ресурсов**
   (#78) — зафиксировать в спеке stdlib.
7. Мелочи: метки циклов для выхода из вложенных (#34); регламент
   затенения обычных переменных; enforce `_`-приватности полей; явно
   специфицировать копирование/утечки слайсов и строк.

## Вывод

Дизайн Nova **осознанно и систематически бьёт по каталогу Go-ошибок** —
это видно по прямым ссылкам на Go в тексте решений (D33, D71, D79, D90).
Большинство граблей книги либо закрыты дизайном, либо неприменимы.
Главная незакрытая зона — **гонки данных**: убрав мьютексы ради
безопасности, язык не дал взамен ни статической, ни рантайм-защиты, и при
переходе на вытесняющий M:N-рантайм это станет реальной дырой.

## Связанные документы

- [spec/decisions/06-concurrency.md](../../spec/decisions/06-concurrency.md) — `spawn`, `select`, каналы, `CancelToken`
- [spec/decisions/04-effects.md](../../spec/decisions/04-effects.md) — `Fail[E]`, handler'ы, `defer`/`errdefer`
- [spec/decisions/02-types.md](../../spec/decisions/02-types.md) — типы, `protocol`, generics, `use`-делегация
- [docs/plans/47-supervised-cancel.md](../plans/47-supervised-cancel.md), [49-cancel-throw-routing.md](../plans/49-cancel-throw-routing.md) — отмена
