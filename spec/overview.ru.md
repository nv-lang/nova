# Nova — обзор

## Центральная идея

Сеть, диск, время, случайность, лог, ошибка, мутация — в Nova это
всё **эффекты**. Функция объявляет в сигнатуре те эффекты, которые
использует сама; вызовы других функций не тащат свои эффекты вверх
(исключение — `Fail`, ошибки видны транзитивно). У каждого эффекта
есть **handler**, который перехватывает его операции.

Из одной абстракции (алгебраические эффекты в стиле Koka/Effekt,
доведённые до прикладного состояния) следует всё остальное в языке.
См. [revolutionary.md](revolutionary.md) для развёртки.

### `effect` vs `protocol`

В Nova два разных способа описать «что-то с операциями»:

- **«Как делать что-то»** — функция объявляет, что ей нужны
  такие-то операции, а какая реализация будет под ними — решает
  вызывающий код через `with`-блок (например, для прода —
  Postgres, для теста — in-memory). Это **эффект**, объявляется
  через `type X effect { ... }`.
- **«Что умеет значение»** — реализация жёстко привязана к типу:
  `int` хешируется так-то, `str` — так-то, и менять это нельзя.
  Это **протокол**, объявляется через `type X protocol { ... }`.

**Когда использовать эффект, а когда протокол в коде:** если
хочется при тестировании использовать другую реализацию — это
эффект. Если при тестировании мы просто работаем со значениями
типа, и подменять там нечего — это протокол.

## Killer use-case

**AI-first программирование.** Когда LLM пишет 50–80% кода, языку нужны:
- видимость побочных действий в сигнатуре (эффекты)
- compile-time гарантии вместо runtime-проверок (контракты, capabilities)
- локальность контекста (одна функция понятна без чтения 10 файлов)
- ошибки компилятора как обучающий сигнал для LLM
- стабильность синтаксиса (LLM учится на старых данных)

Все существующие языки спроектированы до AI-эпохи. Nova — первый
язык, явно оптимизированный под пару «LLM пишет, человек ревьюит».

## Поддерживающие решения

1. **Компилируется ТОЛЬКО через C-backend (AOT), как Go/Rust.** Ранняя
   идея «один исходник — три режима исполнения» (AOT/JIT/интерпретатор)
   **не реализуется**: `nova run file.nv` (tree-walking интерпретатор)
   ретрактирован — команда осталась в CLI только как заглушка,
   которая явно сообщает об этом и направляет на `nova build`/`nova
   test` (см. [`docs/dev/read-project.md`](../docs/dev/read-project.md)).
   Тестируется и шипится код тоже только через C-codegen — нет
   отдельного «интерпретируемого» пути с другой семантикой.
2. **Память: managed по умолчанию (current: Boehm conservative GC; v1.0+:
   concurrent GC), regions opt-in для real-time.** Программист пишет код без
   префиксов памяти — циклы освобождаются автоматически. **Текущее состояние
   bootstrap-runtime'а** ([Plan 27](../docs/plans/27-gc-switch.md), default
   с 2026-05-11): Boehm GC, measured pauses (см.
   `nova_tests/concurrency/gc_pause_bench.nv`) на x86_64-v3 Windows debug-build:
   - 10k objects × 20 rounds: max < 16ms, p99 ≈ avg ≈ 0ms (внутри тика
     GetTickCount64 — Windows timer gran 15.6ms).
   - 100k objects × 10 rounds: max < 16ms.
   - 1M objects × 3 rounds: max < 16ms.

   Это **upper bounds через low-res timer**; реальные pauses скорее всего
   меньше. Hi-res measurement (uv_hrtime) — отдельная задача после bootstrap.

   **Дизайн-цель v1.0+:** concurrent GC, p99 < 1ms на типичных workloads
   ([decisions/05-memory.md#d6](decisions/05-memory.md#d6),
   [Plan 25 G3b](../docs/plans/25-production-readiness-roadmap.md#g3-memory-management--главное-упрощение-runtimeа)).

   Escape analysis оставляет на стеке всё, что не утекает (без GC overhead).
   Для real-time зон (звук, торговля, embedded) — атрибут `#realtime nogc fn`
   ([D172 §7](decisions/06-concurrency.md#d172-realtimeblocking-sync-class-annotation-system-plan-1036);
   исторически блок `realtime nogc { }`, [D64](decisions/04-effects.md#d64),
   retracted Plan 113), сочетаемый с `region { }` для arena-allocations
   (⚠ `region` в текущем компиляторе не реализован).

   **Introspection API** ([Plan 32](../docs/plans/32-gc-introspection.md)):
   `gc.heap_size()`, `gc.collect()`, `gc.live_count()` доступны без import.
3. **Структурная типизация + вывод типов везде.**
4. **Protocols + data вместо классов.** Никакого наследования. Структурные
   контракты через `protocol` (см. [decisions/01-philosophy.md#d1](decisions/01-philosophy.md#d1), [decisions/02-types.md#d42](decisions/02-types.md#d42)).
5. **Контракты в сигнатуре.** `requires`/`ensures`/`invariant` —
   опциональны, но проверяются статически где можно.
6. **Структурированная конкурентность поверх M:N-планировщика (кодовое имя рантайма —
   Vela).** `spawn`/`supervised`/`detach`/cancel-token'ы — те же fiber'ы
   (mco-coroutines), что несут async/await-инфраструктуру из раздела выше.
   **`main()` сам исполняется как файбер** ([D92](decisions/06-concurrency.md#d92),
   ретракция Правила 6, 2026-07-25) — блокирующие park/wake-операции
   (`Time.sleep`, `TcpListener.accept()`, `Channel.recv()`) легальны
   **прямо в `main()`**, без обёртки в `supervised { spawn { … } }`.
   То же самое верно и **прямо в теле `supervised { … }`** без
   промежуточного `spawn` ([D439](decisions/06-concurrency.md#d439),
   2026-07-30) — но такая прямая блокирующая операция НЕ защищена
   `timeout:`/`cancel:` этого scope (enforcement живёт только в join-цикле,
   который стартует после statement'ов тела); для защиты дедлайном/токеном
   нужен `spawn { … }`.
   Супервизия падений — обычный эффект `Supervisor`
   ([D416](decisions/06-concurrency.md#d416)): готовые политики
   `escalate()`/`stop()`, пользовательские — handler-литерал
   `on_child_fail(idx, err) -> Decision`. Внимание: документированная в D416§2
   сериализация `on_child_fail` на drive-файбере рантаймом пока НЕ
   обеспечивается (опровергнуто измерением 2026-07-31, реестр №173) —
   mut-захваты в таком обработчике проверяются энфорсом D441 как у любого
   другого, исключения нет. Naming-конвенции для этого слоя —
   [`docs/dev/mn-coding-conventions.md`](../docs/dev/mn-coding-conventions.md).
   **Модель памяти между файберами** ([D415](decisions/06-concurrency.md#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733),
   [D441](decisions/06-concurrency.md#d441), 2026-07-31): `mut`-захват — линейный ресурс одного файбера, пересекать
   границу (`spawn`/`detach`/`parallel for`/канал/`with`-обработчик вокруг
   fiber-содержащего тела) может только явным move (`consume`), `ro`-видом
   или значением из белого списка синхронизированных типов (`Atomic*`,
   `Mutex`, концы канала, `#share`-типы) — проверяется транзитивно, в том
   числе когда замыкание пересекает границу как ДАННЫЕ (параметром/каналом),
   не только при прямом синтаксическом захвате. То же покрывает
   precomputed-обработчик (`ro h = |..| {..}` затем `with X = h { … }`) и
   транзитивную установку handler'а параметром (A-V10, D441 §5, 2026-07-31).
   Явный move в ребёнка (`spawn`/`detach consume a, b { … }`, D415 §4) отдаёт
   владение телу ребёнка целиком: авто-`@cleanup` срабатывает на выходе тела
   ребёнка — а если тело потребило биндинг ЯВНО (`a.close()`, передача в
   `consume`-параметр), не срабатывает вовсе (амендмент №456, 2026-08-08: те
   же дизарм-точки и рантайм drop-флаг, что у
   [D432](decisions/02-types.md#d432) §4/§5; повторная очистка потреблённого
   значения нарушала бы exactly-once D131/D133).
   Отдельная ось — **`#thread_affine extern fn`** (A-V10, D441 §5): маркирует
   M:N-небезопасный C-side лист (thread-local состояние), транзитивно
   поднимается по графу вызовов, гейтится на границе
   `spawn`/`detach`/`parallel for`.

## Что заимствует у кого

| Фича | Источник |
|------|----------|
| Алгебраические эффекты + handler'ы | Koka, Effekt, Eff |
| Скорость компиляции, простой синтаксис | Go |
| Производительность, traits, мономорфизация | Rust |
| Concurrent GC, простота памяти для backend | Go, Java ZGC |
| Pattern matching, ADT, sum-types | OCaml/Rust |
| Регионы памяти | Zig, Odin |
| Структурированная конкурентность, супервизия | Erlang/OTP, Swift |
| Контракты, refinement-types | Eiffel, Dafny, F* |
| Capability security | E, Pony |

## Tooling из коробки

**Сегодня** — реализовано в `nova` CLI ([nova-cli/](../nova-cli/)):

- `nova build file.nv` — статический бинарь через C-backend (единственный
  путь исполнения, см. «Поддерживающие решения» п.1)
- `nova check [paths]` — типечек + lint без сборки (`--strict-effects` —
  Plan 197, транзитивные эффекты как hard error; `--lint` — те же
  convention-правила, что `nova lint`)
- `nova test [filter]` — discovery + parallel прогон `.nv` тестов
  (C-codegen pipeline; структурированные ошибки с EXPECT-маркерами для
  negative-тестов, D89)
- `nova lint` — реестр конвенционных `W_*`-правил (Plan 185),
  info-режим по умолчанию, `--deny` — CI-гейт
- `nova doc file.nv [--format markdown|json|html]` — генератор документации:
  doc-tests (`--test`), покрытие (`--coverage[-threshold]`), watch-режим,
  mutation-testing для контрактов (`--mutate-contracts`) — Plan 45, ЗАКРЫТ
- `nova bench file.nv` — прогон бенчмарков (release-mode, samples,
  regression-гейт) — Plan 57
- `nova add`/`nova update`/`nova info` — управление зависимостями
  (git/path-зависимости + `nova.lock.toml`; `nova info --diff` —
  effect-surface diff публичного API пакета как supply-chain-гейт) —
  Plan 03.1-03.4. Прокси для скачивания (`NOVA_PKG_PROXY` /
  `nova.override.toml` `[net] proxy` / `~/.nova/config.toml`) — Plan 233
- `nova regen-runtime [--check]` — регенерация `std/runtime/*.nv`
  stubs из `runtime_registry.rs` (Plan 13)
- `nova daemon start/stop/status` — резидентный build-daemon (только
  latency-оптимизация повторных `nova build`, поведение байт-идентично
  без него) — Plan 219
- **LSP** (`nova-lsp/`) — completion/hover/diagnostics/goto/rename,
  выполнен целиком (Plan 104.10, «V2 production», ЗАКРЫТ 2026-07-04);
  конвенции разработки — [`docs/dev/lsp-conventions.md`](../docs/dev/lsp-conventions.md)
- `nova run file.nv` — **НЕ поддерживается**: команда осталась в CLI
  только как понятная ошибка («используйте `nova build`/`nova test`»),
  сам интерпретатор (treewalk) не обслуживается

**Roadmap** (не реализовано):

- `nova fmt`
- `nova check --fragment '...'` — типечекинг одной функции без проекта
- Пакетный менеджер, content-addressed (как Deno + Nix) — сегодняшний
  `nova add`/`update` устроен проще (git/path-зависимости + lockfile),
  content-addressed-хранилище не строилось
- Hot reload в dev-режиме
- AI-friendly патчи в diagnostic'ах (для LLM)
     интерпретатор, который ретрактирован; актуальная форма этой идеи
     (если она ещё жива) не описана ни в одном известном плане на
     момент этой ревизии — не изобретаю новую команду. -->

## Экосистема (отдельные репозитории)

Ядро языка (этот репозиторий) целенаправленно узкое; прикладные слои
живут отдельными пакетами/репозиториями поверх него:

- **`nova-http`** — байтовый HTTP-транспорт (клиент/сервер поверх `std/net`).
- **Polaris** (`nova-polaris`) — веб-фреймворк поверх `nova-http`,
  Axum/FastAPI-модель (Router/Handler/Middleware/extractors); в разработке,
  полная EN+RU документация — отдельным планом (229).
- **`nova-tls`** — TLS поверх `std/net`, vendored C + `.nv`-фасад, без Rust
  в рантайм-пути.

<!-- TODO(232): точный публичный статус/зрелость Polaris (что уже стабильно
     для внешнего пользователя, а не только для разработки) — за пределами
     этого репозитория, сверить с nova-polaris при следующей ревизии. -->

## Что выкинуто из обычных языков

- **Заголовочные файлы, namespaces, modules-vs-packages** — один файл = модуль
- **Null** — только `Option[T]`
- **Исключения как невидимое control flow** — только эффект `Fail[E]`
- **`async`/`await` ключевые слова** — suspension это ambient runtime
  ([D62](decisions/04-effects.md#d62)), эффекты в типах: `Net`, `Io`, `Db`
- **Перегрузка операторов на произвольные типы**
- **Макросы как препроцессор** — только typed comptime (как Zig)
- **Глобальное изменяемое состояние** — `mut` поля/параметры
  (локально) или специализированные state-эффекты (Counter, Cache)
- **DI через рефлексию** — зависимости в эффектах или параметрах
- **Mock-библиотеки** — handler'ы из языка
- **Скрытые импорты** — каждый идентификатор виден откуда

## Зарезервированные identifier'ы

Помимо grammar-keyword'ов (`fn`, `type`, `effect`, `handler`, `let`,
`if`, `match`, `return`, ... — около 38 слов), Nova имеет
**identifier'ы с зарезервированной семантикой**. Они парсятся как
обычные имена, но компилятор знает их специальное значение в
определённых контекстах.

| Identifier | Категория | Где валиден | См. |
|---|---|---|---|
| `Self` | referential type | в любом type-контексте — refers к receiver-типу метода / типу удовлетворяющему protocol'у | [D66](decisions/02-types.md#d66) |
| `any` | top-type | везде; runtime type-tag для downcast'а | [D54](decisions/03-syntax.md#d54) |
| `never` | bottom-type | return type не-возвращающих функций (`throw`, `panic`, `loop`) | [D26](decisions/08-runtime.md#d26) |
| `Option[T]`, `Some`, `None` | sum-тип в prelude | везде | [D26](decisions/08-runtime.md#d26) |
| `Result[T, E]`, `Ok`, `Err` | sum-тип в prelude | везде | [D26](decisions/08-runtime.md#d26) |
| `Error` | record-тип в prelude | для `throw err` | [D26](decisions/08-runtime.md#d26) |
| `RuntimeError` | sum-тип в prelude | bottom-уровневые runtime-ошибки | [D26](decisions/08-runtime.md#d26) |
| `RuntimeNoneError` | unit-тип в prelude | бросается через `expr!!` на `Option` | [D85](decisions/04-effects.md#d85) |
| `Effect[E, IRT]` | first-class тип handler'а эффекта `E` с типом interrupt-VAL `IRT` (default `never` через D88); sugar `Effect[E]` ≡ `Effect[E, never]` | везде | [D61](decisions/04-effects.md#d61), [D87](decisions/04-effects.md#d87), [D88](decisions/03-syntax.md#d88) |
| `Fail[E]`, `Fail` | стандартный эффект | в effect-row сигнатуры | [D25](decisions/04-effects.md#d25), [D65](decisions/04-effects.md#d65) |
| `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace`, `Ask[T]`, `Alloc[R]`, `Detach`, `Blocking` | стандартные эффекты | в effect-row сигнатуры | [D2 (REVISED)](decisions/04-effects.md#d2), [D50](decisions/06-concurrency.md#d50) |
| `int`, `i8`-`i64`, `u8`-`u64`, `f32`, `f64`, `str`, `bool`, `byte` | примитивные типы | везде | [D44](decisions/03-syntax.md#d44), [D27](decisions/03-syntax.md#d27) |

Эти identifier'ы можно **переопределить локально** (например, тип
`Net` пользовательской библиотеки), но это — анти-паттерн. Линтер
выдаст warning.

## Что замораживает выпуск

Тег замораживает ровно то, что несёт `#stable(since = "...")`, — и ничего
сверх ([D460](decisions/01-philosophy.md#d460)).

| интерфейс | обещание |
|---|---|
| помечен `#stable(since = "0.1")` | форма и наблюдаемое поведение зафиксированы; изменение — только через новый мажорный выпуск и запись в спеке |
| пометки нет | работает, но не заморожен: может измениться в следующей версии без миграции |

Правило одинаково для `std`, для пакетов и для флагов компилятора:
обещание даёт пометка, а не факт, что вещь существует и на вид работает.

Это не разрешает поставлять сломанное. Незамороженное обязано быть либо
рабочим, либо ЯВНО названным нерабочим в заметках к выпуску. Молча
неверное поведение остаётся блокером выпуска независимо от пометки —
именно потому, что обманывает того, кто ни на какую стабильность и не
рассчитывал.

## Главные trade-offs

1. **Algebraic effects сложны в реализации** — это передовой край PL,
   Koka работает 10+ лет и всё ещё академический.
2. **Понимание эффектов — порог входа** — решается **только** качеством
   сообщений компилятора. Если они академически точны и человечески
   непонятны — язык мёртв.
3. **Performance эффектов** требует агрессивной оптимизации (статический
   handler-резолюшн, инлайнинг).
4. **Ставка на AI-кодинг** как доминирующий тренд — статистически вероятна,
   но не гарантирована.
5. **9 из 10 таких проектов проваливаются.** Это нормальный риск
   революционной попытки. Альтернатива — гарантированный «ещё один Nim».
