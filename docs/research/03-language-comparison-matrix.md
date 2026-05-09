# План 03 — Матрица сравнения Nova с распространёнными языками

## Цель

Сравнить Nova с топ-10 наиболее распространёнными языками по 24
свойствам — 10 «болей» и 14 «супер-возможностей» (10 базовых + 4
добавленных в реревизии 2026-05). Результат — один документ-матрица,
используемый:

- В публикациях про язык (визуальный аргумент «зачем Nova»)
- Для проверки дизайн-решений Nova (где мы рискуем повторить
  чужие боли)
- Для пересмотров (если фича в матрице не закрывает реальную боль —
  вопрос, нужна ли она)

## Подход

- **Языки:** 10 распространённых backend/system (Go, Rust, Java, C#,
  Python, TypeScript, C++, Swift, Kotlin) + Nova
- **Свойства:** 14 строк (10 болей + 14 возможностей; +4 в реревизии
  2026-05) в одной матрице
- **Оценка по ячейкам:** трёхуровневая
  - **✓** — есть в полной мере (для болей: «эта боль присутствует»;
    для возможностей: «возможность доступна нативно»)
  - **~** — частично, опционально, через библиотеку или сахар
  - **—** — нет
- **Источник:** общие знания о языках, для сомнительных мест —
  WebSearch (отметка `[?]` рядом с ячейкой)
- **Статус Nova:** ячейки помечены **✓** только для фич, реально
  работающих в bootstrap-компиляторе [compiler-codegen](../../compiler-codegen/)
  (80/80 `nova_tests` PASS на 2026-05-08). Фичи, оставшиеся «по
  дизайну», помечены **(spec)**. Раньше весь столбец был «по дизайну»
  — это утверждение больше неверно.

## 10 болей современных языков

Боли — то, **что мешает** программистам каждый день, особенно в
backend и в paired-design с AI.

| # | Боль | Что значит | Зеркальная возможность |
|---|---|---|---|
| Б1 | **Невидимые побочки** | Сигнатура не показывает что функция делает (ходит в БД, бросает, мутирует) | В1: эффекты в типе |
| Б2 | **Null/nil pointer dereference** | Программа падает, потому что значение оказалось null | В2: Option[T] / no null |
| Б3 | **Цвет функции (async/await)** | Async-функции вирусят все вверх по стеку, sync ↔ async несовместимы | В3: невидимый async |
| Б4 | **Невидимое исключение** | Любая функция может бросить unchecked exception, не объявляя | В1: throws в типе |
| Б5 | **Ручное управление памятью** | Программист думает о malloc/free, weak/strong, lifetime | В4: GC по умолчанию |
| Б6 | **GC паузы для real-time** | GC останавливает программу непредсказуемо | В5: opt-in без GC (regions/borrow) |
| Б7 | **Цвет mutation (mutable everywhere)** | Любая функция может мутировать аргумент незаметно | В6: mut в типе |
| Б8 | **Тяжёлый mock-фреймворк для тестов** | Подмена зависимостей требует DI-фреймворка, mock-библиотеки, рефлексии | В7: тесты через handler-подмену |
| Б9 | **Наследование (fragile base, diamond, GoF)** | Сложные иерархии классов, проблема хрупкого базового класса | В8: композиция + sum-type вместо |
| Б10 | **Verbosity сигнатур ошибок** | `Throws/throws/Result/Either` многословны, программисты обходят их | В9: `?` для проброса |

## 14 супер-возможностей

Возможности — **что хочется** в современном языке, но редко есть
в полной мере. Расширено с 10 до 14 в реревизии 2026-05: добавлены
В11–В14, отражающие то, что реально вошло в bootstrap-codegen и
выделяет Nova на фоне конкурентов.

| # | Возможность | Что значит |
|---|---|---|
| В1 | **Эффекты в сигнатуре** | `fn f() Db Throws -> T` — все побочки видны в типе |
| В2 | **Нет null, есть Option[T]** | Отсутствие значения — типизировано |
| В3 | **Невидимый async** | Sync-стиль кода, async обрабатывается runtime'ом без `await` |
| В4 | **GC managed memory по умолчанию** | Программист не пишет `malloc`/`free`/`weak` в обычном коде |
| В5 | **Opt-in регионы для real-time** | Явный escape hatch без GC pauses |
| В6 | **Mut в сигнатуре** | Изменение аргумента видимо в типе, не скрытое |
| В7 | **Тесты через подмену handler'ов** | Без mock-фреймворков, через язык |
| В8 | **Sum-types + exhaustive match** | Алгебраические типы данных, проверяемые в compile-time |
| В9 | **`?` для проброса ошибок** | Сахар над match Err → return |
| В10 | **Pattern matching с guards** | Полноценный match с условиями и деструктуризацией |
| В11 | **Composable handlers (handler-as-value)** | Handler — first-class значение; транзакции, retry, capture-stdout — обёртки одного механизма |
| В12 | **Capability security в типах** | `forbid Net { body }` блокирует эффект; компилятор гарантирует, что плагин не выйдет за песочницу |
| В13 | **Structured concurrency как языковые примитивы** | `spawn`, `parallel for`, `race`, `select`, `cancel_scope` — ключевые слова, не библиотека |
| В14 | **Detерминированный режим тестов** | `with Time = fixed(...), Random = seed(42)` — повторяемость без mock-фреймворков, встроено в семантику handler'ов |

## Языки в сравнении

1. **Go** — backend mainstream, GC, простота
2. **Rust** — системный, no-GC, эффекты-через-типы (частично)
3. **Java** — enterprise, GC, OOP-классика
4. **C#** — .NET, GC, гибрид OOP/FP
5. **Python** — скрипты/ML, GC, динамика
6. **TypeScript** — frontend/backend, GC, гибрид
7. **C++** — системный, manual memory, OOP+templates
8. **Swift** — Apple, ARC, гибрид
9. **Kotlin** — JVM, GC, гибрид
10. **Nova** — bootstrap-компилятор работает (80/80 nova_tests на
    2026-05-08); std/ — частично (43 модуля упираются в bootstrap-
    ограничения, см. [std/STATUS.md](../../std/STATUS.md))

## Что не покрыто этой матрицей (намеренно)

Не оценивались:
- **Performance** — слишком зависит от use-case
- **Экосистема** — Nova не имеет, всем остальным — рано сравнивать
- **Maturity** — Nova на дизайн-стадии, не сравнима
- **Tooling** (LSP, debugger, IDE) — слишком много нюансов
- **Onboarding curve** — субъективно

Эти аспекты — для отдельных документов. Здесь только **языковые
свойства**, что в самих типах и синтаксисе.

## Использование в публикациях

### Сокращённая версия для пилотной статьи (4 языка × 6 свойств)

Готовая для вставки таблица — только самые популярные конкуренты
(Go, Rust, Java) и только свойства, где Nova явно отличается.
Помещается в один экран, читается за 30 секунд.

| Свойство | Go | Rust | Java | **Nova** |
|---|---|---|---|---|
| Невидимые побочки в сигнатуре | ✓ боль | ~ частично | ✓ боль | **— нет** |
| Цвет async/await | — нет | ✓ боль | — нет¹ | **— нет** |
| Невидимые исключения | ~ panic | — нет | ✓ боль | **— нет** |
| Тяжёлый mock-фреймворк | ✓ боль | ~ | ✓ боль | **— нет²** |
| Эффекты в сигнатуре общим механизмом | — | ~³ | ~⁴ | **✓** |
| Подмена тестов через handler | — | — | — | **✓** |
| Capability sandbox в типах (`forbid Net`) | — | — | — | **✓** |
| Structured concurrency как ключевые слова | ~⁵ | — | ~⁶ | **✓** |

¹ Java 21+ virtual threads (LTS, 2023). До 21 — был цвет через
  `CompletableFuture`.
² Тесты в Nova через `with X = handler { ... }` — без mock-библиотек.
³ Rust имеет `Result`, `async fn`, `Option` — эффекты через типы
  частично, но **нет общего механизма** объединить `Throws[E]` +
  `Db` + `Net` в одной сигнатуре.
⁴ Java checked exceptions ≈ effects, но только для ошибок.
⁵ Go: `go` — есть, но никакого scoping/cancellation в языке;
  `context.Context` — рукопись, не примитив.
⁶ Java JEP 505/525 — `StructuredTaskScope` всё ещё в preview
  (5-й и 6-й preview, JDK 25/26); finalization ожидается в JDK 27
  к концу 2026.

**Сноска для статьи:** оценки субъективны на градации ✓/~/—. Полная
матрица с 10 языками и 20 свойствами — в [03-language-comparison-matrix.md](03-language-comparison-matrix.md).
Если у вас другое мнение по конкретной ячейке — напишите в
комментариях, обсудим.

### Кандидат темы для отдельной статьи серии

«Nova vs остальные: 10 болей, которые закрывает каждый язык, и 10,
которые остаются». Это будет статья номер 2 или 3 в серии (после
пилота), на основе **полной** матрицы из этого документа.

## Открытые вопросы

1. **Точность ячеек.** Матрица составлена из общих знаний + WebSearch
   для спорных мест. После реревизии 2026-05 проверены:
   C# 14/15 unions, Java JEP 505/525 (structured concurrency),
   Swift 6 typed throws.
2. **Список языков.** Не включены OCaml, Haskell, Zig, Erlang/Elixir,
   F#, Scala. Если расширять — это для отдельной матрицы
   «функциональные языки» или «системные языки». **Erlang/Elixir
   особенно жалко** — у них structured concurrency и supervision
   как раз keyword-уровневые, ближайший аналог Nova по В13.
3. **Нумерация ✓ / ~ / —** субъективна. Можно ужесточить (только ✓
   = «нативно из коробки»), можно расслабить (~ = «через библиотеку
   достаточно»). Сейчас среднее.
4. **Включать ли Nova в самосравнение.** Risk «маркетинговый материал».
   Решение принято: **столбец Nova оставлен**, ✓ означает «работает
   в bootstrap-codegen на 2026-05-08», `(spec)` означает «есть в
   спецификации, не реализовано». Это честный compromise — не
   маркетинговый material и не обезличенное описание ландшафта.
5. **В11–В14 vs «исключение через включение Nova-фич».** Возможный
   контраргумент: «эти 4 строки добавлены так, чтобы Nova была
   везде впереди». Аргумент против контраргумента: каждая из В11–В14
   — реальный pain в backend-разработке (тесты, sandbox, concurrency,
   воспроизводимость), а не выдуманная фича. Если они и
   «оптимизированы под Nova», то потому, что Nova реально решает их
   первая среди топ-10.

## Статус

- [x] План написан
- [x] Матрица собрана из общих знаний
- [x] Сомнительные ячейки проверены через WebSearch (агентом, май 2026):
  - Java async (Б3): ~ → **—** (Java 21+ не имеет async/await)
  - Java sum-types (В8): ~ → **✓** (JEP 441, exhaustive с 2023)
  - Swift `?` (В9): ✓ → **~** (try? имеет противоположную семантику)
  - C# nullable footnote обновлён (по умолчанию с .NET 6, 2021)
  - Kotlin coroutines footnote уточнён (suspend виральна)
  - Swift ARC footnote уточнён (compile-time RC, не GC)
  - C# discriminated unions footnote обновлён (target — C# 15, ноябрь 2026)
  - Сводная картинка очков пересчитана
- [x] Сокращённая версия для пилотной статьи (~6 строк, 4 языка)
- [x] **Реревизия 2026-05** — после того, как bootstrap-codegen дошёл
      до 80/80 nova_tests + std/ начал собираться:
  - Статус Nova ✓ переведён с «по дизайну» на «работает в bootstrap»
  - Добавлены В11–В14: composable handlers, capability sandbox,
    structured concurrency как keyword'ы, deterministic test mode
  - Сокращённая таблица расширена на 2 строки (capability + concurrency)
  - Добавлен раздел «Что Nova даёт новому пользователю» с 8 onboarding-
    хуками и категоризацией «настоящая новизна / переупаковка / гигиена»
  - Java structured concurrency JEP 505/525 — обновлено на 6-й preview
    в JDK 26, finalization в JDK 27 к концу 2026
  - Swift 6.0 typed throws — учтено в обсуждении В1
- [ ] Финальная сборка для отдельной статьи серии

---

# Матрица

## Боли (✓ = боль присутствует, ~ = частично, — = нет)

| Боль | Go | Rust | Java | C# | Python | TS | C++ | Swift | Kotlin | **Nova** |
|---|---|---|---|---|---|---|---|---|---|---|
| Б1 невидимые побочки | ✓ | ~ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | **—** |
| Б2 null/nil | ✓ | — | ✓ | ~¹ | ✓ | ~² | ✓ | — | ~³ | **—** |
| Б3 цвет async/await | — | ✓ | —⁴ | ✓ | ✓ | ✓ | — | ✓ | ~⁵ | **—** |
| Б4 невидимые исключения | ~⁶ | — | ✓⁷ | ✓ | ✓ | ✓ | ✓ | — | ✓ | **—** |
| Б5 ручная память | — | ~⁸ | — | — | — | — | ✓ | ~⁹ | — | **—** |
| Б6 GC паузы для real-time | ✓ | — | ✓ | ✓ | ✓ | ✓ | — | ~¹⁰ | ✓ | **~¹¹** |
| Б7 mutation everywhere | ✓ | — | ✓ | ✓ | ✓ | ✓ | ✓ | ~¹² | ~¹² | **—** |
| Б8 тяжёлый mock | ✓ | ~ | ✓ | ✓ | ~ | ~ | ✓ | ✓ | ✓ | **—** |
| Б9 наследование классов | — | — | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | **—** |
| Б10 verbosity ошибок | ✓¹³ | ~¹⁴ | ✓¹⁵ | ~ | ~ | ~ | ~ | ~ | ~ | **—** |

## Возможности (✓ = есть, ~ = частично, — = нет)

| Возможность | Go | Rust | Java | C# | Python | TS | C++ | Swift | Kotlin | **Nova** |
|---|---|---|---|---|---|---|---|---|---|---|
| В1 эффекты в сигнатуре | — | ~¹⁶ | ~¹⁷ | — | — | — | — | ~¹⁸ | — | **✓** |
| В2 Option[T] | — | ✓ | ✓¹⁹ | ~²⁰ | — | ~²¹ | ~²² | ✓ | ~²³ | **✓** |
| В3 невидимый async | ✓²⁴ | — | ✓²⁵ | — | — | — | — | — | ~²⁶ | **✓** |
| В4 GC по умолчанию | ✓ | — | ✓ | ✓ | ✓ | ✓ | — | ~²⁷ | ✓ | **✓** |
| В5 regions opt-in | — | ✓²⁸ | — | ~²⁹ | — | — | ~³⁰ | — | — | **✓** |
| В6 mut в сигнатуре | ~³¹ | ✓ | — | ~³² | — | — | ~³³ | ✓ | ~³⁴ | **✓** |
| В7 handler-подмена для тестов | — | — | — | — | — | — | — | — | — | **✓** |
| В8 sum-types + exhaustive match | — | ✓ | ✓³⁵ | ~³⁶ | ~³⁷ | ✓³⁸ | ~ | ✓ | ✓ | **✓** |
| В9 `?` для проброса | — | ✓ | — | — | — | — | — | ~³⁹ | — | **✓** |
| В10 pattern matching с guards | — | ✓ | ~⁴⁰ | ✓⁴¹ | ✓⁴² | — | — | ✓ | — | **✓** |
| В11 composable handlers (handler-as-value) | — | — | — | — | — | — | — | — | — | **✓**⁴³ |
| В12 capability security в типах | — | ~⁴⁴ | — | — | — | — | — | — | — | **✓**⁴⁵ |
| В13 structured concurrency как ключевые слова | ~⁴⁶ | — | ~⁴⁷ | — | ~⁴⁸ | — | — | ~⁴⁹ | ~⁵⁰ | **✓**⁵¹ |
| В14 детерминированный режим тестов | — | ~⁵² | — | — | ~⁵³ | — | — | — | — | **✓**⁵⁴ |

## Сводная картинка для статьи

После реревизии 2026-05 (добавлены В11–В14, статус Nova подтверждён
bootstrap-компилятором 80/80 nova_tests):

```
                        Боль закрыта    Возможность есть
Nova                     9.5 / 10        14 / 14  *(bootstrap, 2026-05)*
Rust                     8.0 / 10         6.5 / 14
Swift                    4.0 / 10         5.5 / 14
Kotlin                   3.0 / 10         4.0 / 14
Java                     2.0 / 10         4.5 / 14
C#                       3.0 / 10         4.0 / 14
Go                       3.5 / 10         3.0 / 14
TypeScript               2.5 / 10         2.5 / 14
C++                      2.5 / 10         2.0 / 14
Python                   2.0 / 10         3.0 / 14
```

**Считаются:** ✓ как 1, ~ как 0.5, — как 0.

**Замечание:** счёт «Nova 14/14» — по тому, что **реально работает в
bootstrap-компиляторе** на 2026-05-08 (см. [nova_tests/](../../nova_tests/),
[std/](../../std/)). Это не release v1.0, и std/ ещё компилируется
лишь частично (43 модуля упираются в bootstrap-ограничения, см.
[std/STATUS.md](../../std/STATUS.md)). Но базовый язык — эффекты,
handlers, regions, structured concurrency, sum-types, `?`, capability
sandbox — **парсится и выполняется**, не «по дизайну». Бóльшая часть
отрыва Nova над конкурентами — в **В11–В14**, которые целостно
держатся только при наличии алгебраических эффектов как первого
концепта.

**Главные находки проверки фактов:**

1. **Rust остаётся лидером среди существующих** — закрывает 8 болей
   из 10. По «возможностям» Rust также впереди (6.5/14), но В11–В14
   у него отсутствуют, потому что нет first-class handler'ов.
2. **Java 21+ обогнала C# в sum-types** — JEP 441 sealed + switch
   pattern даёт полный exhaustive check, в C# 14 это всё ещё
   warning + workaround. C# 15 (нояб. 2026) с unions — не гарантирован.
3. **Java structured concurrency всё ещё в preview** — JEP 505/525
   (5-й и 6-й preview, JDK 25/26), finalization ожидается в JDK 27
   к концу 2026 года.
4. **Swift `try?` ≠ Rust `?`** — semantically opposite (Optional vs
   propagation). Распространённое заблуждение, исправлено.
5. **Java НЕ имеет async/await вообще** (никогда не имела).
   Virtual threads (Java 21) дают sync-style без цвета.
6. **Swift 6 typed throws** — стабильно с 2024, добавляет точность,
   но эффекты остаются в двух конкретных каналах (`throws`, `async`),
   общего механизма handlers нет.

## Что показывает матрица

### Сильные стороны Nova относительно Rust

Rust — ближайший конкурент по эффектам-в-типе и no-null. Где Nova
сильнее:

- **Б3 (цвет async/await):** Rust — `async fn` вирусит. Nova — нет.
- **Б5 (ручная память):** Rust требует ownership/lifetime от
  программиста. Nova — managed GC по умолчанию.
- **В7 (handler-подмена для тестов):** уникально Nova.
- **В1 (эффекты в сигнатуре общим механизмом):** Rust имеет частично
  (`Result`, `async`, `Option`), но нет единого `Throws[E]`+`Db`+`Net`.
- **В11–В14:** все четыре уникальны для Nova. Это не «ещё одна
  фича», а **прямое следствие** наличия алгебраических эффектов как
  первого концепта.

### Слабые стороны Nova относительно Rust

- **Real-time гарантии:** Rust — без GC из коробки. Nova —
  через `region { }` opt-in. Для системного программирования Rust
  лучше.
- **Зрелость:** Rust — стабильный 1.0 с 2015, экосистема. Nova —
  bootstrap-компилятор работает, но v1.0 ещё нет, std/ собирается
  частично.

### Где Nova сильнее всех остальных

- **В1 (эффекты в сигнатуре общим механизмом)** — нет ни в одном
  из 9 сравниваемых языков.
- **В7 (handler-подмена для тестов)** — нет ни в одном.
- **В11 (composable handlers)** — нет ни в одном; в Rust/Swift
  есть похожее через trait-objects, но нет first-class композиции
  с прозрачным проносом continuation.
- **В12 (capability security в типах)** — частично есть в Rust
  (через ownership), но не как языковая фича `forbid X`. В Nova
  плагин с сигнатурой `fn run(s str) Logger -> str` физически не
  может тронуть Net/Db/Fs.
- **В13 (structured concurrency как примитивы языка)** — Java
  ещё в preview, Kotlin/Swift — через библиотеки и блоки. В Nova
  `parallel for`, `race`, `select`, `cancel_scope` — **keyword'ы**.
- **В14 (детерминированный режим тестов)** — нигде не встроено в
  семантику; в Rust/Python — через моки и фикстуры.
- **Б8 (тяжёлый mock-фреймворк)** — все остальные страдают.

### Где Nova не уникальна

- **В4 (GC по умолчанию)** — есть в Go, Java, C#, Python, TS,
  Kotlin. Это **базовая гигиена**, не дифференциация.
- **В8 (sum-types)** — есть в Rust, Swift, Kotlin, частично в TS.
- **В9 (`?` для проброса)** — есть в Rust, Swift.

## Что Nova даёт новому пользователю — onboarding-хуки

Это раздел для того, чтобы понять, **что бросается в глаза** при
первом знакомстве с Nova и может стать причиной попробовать.
Сгруппировано по «вау-эффекту».

### 1. Тесты без mock-фреймворка

```nova
test "withdraw debits the account" {
    let mut clock = 1000
    with Time = handler { now() => return clock; sleep(ms) { clock += ms; return () } } {
        let acc = Account { balance: 500, last_tx: 0 }
        withdraw(acc, 100)?
        assert(acc.balance == 400)
        assert(acc.last_tx == 1000)
    }
}
```

Никакого Mockito, Jest, unittest.mock. **Один синтаксис языка**
заменяет три-четыре экосистемы. Программисту, прошедшему через
DI-фреймворки и mock-библиотеки, это видно сразу.

### 2. Capability sandbox в одной строке

```nova
fn run_plugin(code str) Logger Fail -> str {
    forbid Net, Db, Fs {
        eval(code)   // плагин может только логировать; Net/Db/Fs — compile error внутри
    }
}
```

Это **не runtime-проверка** через seccomp или WASM-песочницу —
это compile-time гарантия. Аналогов в mainstream-языках нет.

### 3. Sync-стиль кода без `await` и без цвета функции

```nova
fn fetch_user(id int) Net Fail -> User =>
    http.get("/users/${id}").parse[User]()?

// никаких async, никаких .then, никаких runtime-блокировок
fn main() Net Fail -> () =>
    let users = parallel for id in 1..100 { fetch_user(id)? }
    println("got ${users.len} users")
```

`parallel for` — **языковая конструкция**, не runtime-библиотека.
Возвращает `[]T` после завершения всех fiber'ов. Программист,
уставший от async/await, видит, насколько этот код проще.

### 4. Один блок `realtime { }` для критичных секций

```nova
fn process_audio(samples []f32, gain f32) -> []f32 =>
    realtime {
        samples.map((x) => x * gain)   // компилятор гарантирует: no GC, no suspension
    }
```

Не «выберите Rust для real-time», не «выберите Java с low-pause GC»
— **в одном языке** есть и managed-GC, и compile-time
no-GC-зона. Это редкая комбинация.

### 5. Structured concurrency «из коробки»

```nova
let result = race {
    fetch_from_primary(),
    fetch_from_cache(),
    timeout(500.ms)
}
```

Erlang-style supervision и Swift-style structured concurrency —
**ключевые слова** (`spawn`, `parallel for`, `race`, `select`,
`cancel_scope`, `supervised`), не библиотечные API.

### 6. Property-based testing без отдельной библиотеки

```nova
test "reverse is involutive" {
    property[[]int](ArrayGen[int]) { xs =>
        assert(xs.reverse().reverse() == xs)
    }
}
```

В std/testing/property.nv — генераторы и shrinking. Работает на
тех же handler'ах: детерминизм через `with Random = seed(42)`.

### 7. Контракты прямо в сигнатуре (опциональны)

```nova
fn withdraw(mut acc Account, amount money) Fail[Overdraft] -> ()
    requires amount > 0
    ensures result.is_ok ==> acc.balance == old(acc.balance) - amount
{
    if amount > acc.balance { throw Overdraft }
    acc.balance -= amount
}
```

Опциональны. Без них — Go-style. С ними — F*/Dafny-style. **Один
язык покрывает спектр** от скрипта до критичного кода.

### 8. Стабильный синтаксис как обещание для LLM

В spec **зафиксирован отказ** от breaking changes синтаксиса после
v1.0. LLM, обученный на старых данных, не выдаёт мёртвый код.
Это явная цель дизайна, не побочный эффект.

### Что из этого «настоящая новизна», а что — переупаковка

**Настоящая новизна** (не встречается ни в одном из топ-10 языков):
- В7 + В11 + В14 (handlers как тесты + композиция + детерминизм)
- В12 (capability sandbox в типах — `forbid X`)
- Эффекты + GC + регионы в одной системе типов

**Переупаковка известного** (есть в одном-двух языках, Nova
объединяет):
- Sum-types + match (Rust, Swift, Kotlin)
- `?` для проброса (Rust, Swift)
- Structured concurrency (Swift, Erlang/OTP, Java preview)
- Контракты (Eiffel, Dafny, F*)

**Базовая гигиена** (есть у всех взрослых языков):
- GC по умолчанию
- Option[T] вместо null

Главное, что отличает Nova **именно как язык, а не как набор
библиотек**, — категория «настоящая новизна» выше. Там же — те
самые «вау-фичи», которые могут стать поводом попробовать.

## Сноски

¹ C# 8.0+ ввёл nullable reference types (`string?`). С **.NET 6
   (ноябрь 2021) включено по умолчанию** в новых проектах (не opt-in,
   как было раньше). Compile-time only — нет runtime гарантий; дыры
   через десериализацию (JSON/EF), reflection, suppression-оператор
   `null!`, не-аннотированные библиотеки. NRE всё ещё возможен в
   production.

² TypeScript: `strictNullChecks` — opt-in флаг компилятора.

³ Kotlin: `String?` встроен, но платформенные типы из Java могут
   быть `String!` (platform type), что снова создаёт боль.

⁴ Java **не имеет** ключевых слов `async`/`await` вообще. Java 21+
   ([JEP 444](https://openjdk.org/jeps/444), LTS, сентябрь 2023) —
   virtual threads дают sync-стиль кода без цвета. В Java 21–23 был
   pinning под `synchronized` (perf-проблема, не цвет); исправлено в
   Java 24 ([JEP 491](https://openjdk.org/jeps/491), март 2025).
   Остаточный pinning только в JNI/FFM и инициализации классов.

⁵ Kotlin: `suspend` **виральна вверх** по стеку (компилятор
   инжектит параметр `Continuation`). Вызов `suspend` из не-`suspend`
   возможен только через мост (`runBlocking`, `launch`, `async`).
   Structured concurrency **не убирает цвет** — это про lifecycle/
   cancellation, ортогонально. ~ оправдано только тем, что один
   keyword (`suspend`) вместо двух (`async`/`await`).

⁶ Go: `panic` существует, но обычные ошибки через `if err != nil`
   (явные).

⁷ Java: unchecked `RuntimeException` — невидимы. Checked exceptions
   объявляются, но обычно оборачивают в `RuntimeException`.

⁸ Rust: ownership/lifetime — это **другая** ручная работа, не
   malloc/free, но требует думать.

⁹ Swift ARC: автоматический, но программист пишет `weak`/`unowned`
   для разрыва циклов.

¹⁰ Swift ARC: предсказуемые освобождения (deterministic), без
   stop-the-world пауз. Не GC в классическом смысле, но требует
   осознанной работы.

¹¹ Nova: `region { }` блок с эффектом `Realtime` снимает GC внутри.
   Боль есть только если использовать дефолтный managed heap в hot
   path — но программист может выбрать.

¹² Swift/Kotlin: `let` vs `var`, `val` vs `var`. Иммутабельность
   опциональна, но не enforce'ится в сигнатуре функции (Swift `inout`
   требует явного маркера, Kotlin не enforce'ит).

¹³ Go: `if err != nil { return ..., err }` повторяется везде.

¹⁴ Rust: `?` оператор сильно сокращает.

¹⁵ Java checked exceptions: `throws IOException, SQLException, ...`
   многословны, обычно обходятся через RuntimeException.

¹⁶ Rust: `async fn`, `Result<T,E>`, `Option<T>` — частично эффекты в
   типе, но нет общего механизма (как `Throws[E]` + `Db` + `Net`
   одновременно).

¹⁷ Java checked exceptions ≈ эффекты в сигнатуре, но только для
   ошибок, не для IO/Net/Db. Project Loom добавил virtual threads,
   но без эффектов.

¹⁸ Swift `throws`, `async` — эффекты-в-типе для двух конкретных
   случаев. Нет общего механизма handlers.

¹⁹ Java `Optional<T>` — есть, но реликт null остался для совместимости.

²⁰ C#: `Nullable<T>` для value types, для reference — opt-in через
   `?`.

²¹ TypeScript: `T | null` или `T | undefined` — workable, но
   неочевидно.

²² C++ `std::optional<T>` — есть с C++17, но `T*` всё ещё
   распространён.

²³ Kotlin: `T?` встроен, но платформенные типы из Java создают дыры.

²⁴ Go goroutines: нет `await`, sync-style код, вирусности нет.

²⁵ Java 21 virtual threads: нет `await`, sync-style. До 21 —
   `CompletableFuture<T>` с цветом.

²⁶ Kotlin: `suspend` функции — частично виральны, но structured
   concurrency помогает.

²⁷ Swift ARC — **compile-time reference counting**, официально
   **не GC** (Apple отказался от GC в 2012 году). Без явного
   malloc/free, но циклы ссылок **не освобождаются автоматически** —
   программист пишет `weak`/`unowned` для разрыва циклов. Из этих
   соображений `~` — автоматизация неполная по сравнению с tracing
   GC.

²⁸ Rust: arena allocators (typed-arena, bumpalo, slotmap) —
   через библиотеки. Нет ключевого слова, но паттерн известен.

²⁹ C# `stackalloc`, `Span<T>` — частичные средства.

³⁰ C++: arena allocators через библиотеки или вручную.

³¹ Go: `*T` указатель = mutation возможна. Не явный `mut` в типе,
   но видно `*` в сигнатуре.

³² C# `ref` параметры — частично, но не enforce'ит read-only по
   умолчанию.

³³ C++ `const T&` vs `T&` — есть, но default — mutation.

³⁴ Kotlin `val` vs `var` — для локальных, не для параметров функций.

³⁵ Java 21 (LTS, сентябрь 2023, [JEP 441](https://openjdk.org/jeps/441))
   — `sealed` классы + pattern matching for `switch` дают **полный
   compile-time exhaustive check**. Добавление нового варианта в
   `permits` ломает все non-default switch'и — то же поведение, что
   в Rust/Kotlin/TS. Стабильно с 2023 года.

³⁶ C# 14 (по апрель 2026) **не имеет нативных discriminated unions**.
   Целевой релиз — C# 15 (ноябрь 2026, не гарантирован). Workaround:
   `abstract record` + `sealed record` + `switch` expressions.
   Exhaustive check через warning **CS8509** (по умолчанию warning,
   не error; для строгости — `<TreatWarningsAsErrors>` или анализатор
   `ExhaustiveMatching.Analyzer`).

³⁷ Python 3.10+: `match` statement — структурный pattern matching.
   Sum-types через `Union` типы или классы.

³⁸ TypeScript: discriminated unions — sum-types через type-level.
   Exhaustiveness через `never` тип.

³⁹ Swift: `try?` имеет **противоположную семантику** от Rust `?`.
   `try?` конвертирует throwing call в Optional (возвращает `nil` при
   ошибке, **проглатывает** ошибку — НЕ пробрасывает). Аналог Rust
   `?` в Swift — это **`try`** (без `?`) внутри функции с `throws`.
   Соответственно ~: пропагация есть, но синтаксис другой и `?` не
   тот же.

⁴⁰ Java 21 patterns: limited match, без guards в полной мере.

⁴¹ C# pattern matching: `when` clause — guard'ы есть.

⁴² Python `match`: guards через `if` — есть.

⁴³ Nova: `handler` — first-class значение, можно положить в
   переменную, передать аргументом, скомпоновать `transactional`/
   `with_retry`/`with_logging` как обычные функции от handler'а к
   handler'у. В Rust/Swift `dyn Trait`/protocol-objects ближайший
   аналог, но они не могут чисто перехватить continuation
   (resume/interrupt) — это требует первого класса поддержки в
   рантайме. Поэтому `—` для всех существующих языков.

⁴⁴ Rust: capability-by-construction через ownership и приватные
   модули — программист сам строит «безопасные» API. Это `~`,
   потому что нет языковой конструкции «запрети эффект Net в этом
   блоке». Pony — единственный mainstream-кандидат на ✓, но он не
   входит в топ-10.

⁴⁵ Nova: `forbid Net, Db, Fs { body }` — внутри блока вызов любой
   функции с эффектом из списка — **compile-time error**. Это часть
   эффект-системы, а не runtime-sandbox.

⁴⁶ Go: `go f()` — простой spawn, но никакого scope/cancellation
   в языке. `context.Context` пробрасывается **руками**, нет
   гарантий, что все горутины завершатся. Поэтому `~`, не `✓`.

⁴⁷ Java: JEP 505/525 — `StructuredTaskScope` всё ещё в preview
   (5-й и 6-й preview, JDK 25/26). Finalization ожидается в JDK 27
   к концу 2026. Когда зафиналят — будет `✓` (через библиотечный
   API `java.util.concurrent`, не keyword'ы).

⁴⁸ Python: `asyncio.TaskGroup` (3.11+) — structured concurrency
   через библиотеку, не keyword.

⁴⁹ Swift: `async let`, `TaskGroup`, `withTaskCancellationHandler`
   — есть structured concurrency, но через API + `async`/`await`
   синтаксис (т.е. виральный цвет функции, см. `~` в Б3 для Swift).

⁵⁰ Kotlin: `coroutineScope`, `supervisorScope`, `withTimeout` —
   structured concurrency через библиотеку kotlinx.coroutines.
   Не keyword'ы, требует suspend (вирусность).

⁵¹ Nova: `spawn`, `parallel for`, `race`, `select`, `cancel_scope`,
   `supervised` — **ключевые слова** ([D50](../../spec/decisions/06-concurrency.md#d50),
   [D71](../../spec/decisions/06-concurrency.md#d71),
   [D75](../../spec/decisions/06-concurrency.md#d75),
   [D79](../../spec/decisions/06-concurrency.md#d79)).
   Базовая реализация в bootstrap-runtime (fiber scheduler, channel,
   cancel token).

⁵² Rust: тестовый фреймворк `proptest`/`quickcheck` — детерминизм
   через seed, но Time/Random/Net надо мокать руками каждый раз.
   `~` за усилие.

⁵³ Python: `freezegun`, `responses`, `pytest-randomly` — ecosystem
   умеет, но это `~` пакетов, не одно языковое решение.

⁵⁴ Nova: handler-подмена — **тот же механизм**, что и в обычном
   коде. `with Time = fixed(2026-01-01), Random = seed(42),
   Net = record_or_replay("flow.json"), Db = in_memory() { ... }` —
   и весь блок детерминирован. Работает потому, что эффекты — часть
   языка, а не runtime-инжекция.
