# Runtime — режимы запуска, panic, prelude, статическое состояние

Решения этой группы определяют, как программа Nova **исполняется**:
поддерживаемые режимы компиляции, что считается panic'ом и как он
обрабатывается, что предоставляет prelude и почему в языке нет
static-состояния.

| # | Решение |
|---|---|
| [D7](#d7-один-язык--три-режима-компиляции) | Один язык — три режима компиляции |
| [D13](#d13-panic-vs-эффекты-что-не-является-эффектом) | Panic vs эффекты: что НЕ является эффектом |
| [D26](#d26-базовая-stdlib-и-prelude) | Базовая stdlib и prelude |
| [D41](#d41-static-функции-есть-static-состояния-нет) | Static-функции есть, static-состояния нет |
| [D70](#d70-tostr-protocol--replaced--d73) | ⚠️ REPLACED → D73 (migration map only) |
| [D73](#d73-from--into-protocol-пара-с-авто-выводом) | `From` / `Into` protocol-пара с авто-выводом |
| [D74](#d74-математические-операции-на-числовых-типах--instance-методы) | Математические операции на числовых типах — instance-методы |
| [D77](#d77-tryfrom--tryinto-protocol-пара-расширение-d73-для-fallible-конверсий) | `TryFrom` / `TryInto` — расширение D73 для fallible-конверсий |
| [D76](#d76-mem-эффект--runtime-introspection-для-leakgrowth-тестов) | `Mem` эффект — runtime introspection для leak/growth тестов |
| [D81](#d81-assertcond-vs-debug_assertcond--build-mode-семантика) | `assert(cond)` vs `debug_assert(cond)` — build-mode семантика |
| [D141](#d141-примитивы-доступа-к-памяти--byte_at--bulk-slice-операции) | Примитивы доступа к памяти — `byte_at` / bulk slice-операции |
| [D177](#d177-str-nova-body-dispatch--plan-54-ф2-extension) | `str` Nova-body dispatch — Plan 54 Ф.2 extension |
| [D178](#d178-str-api-cleanup-и-расширения--plan-91-ф26) | `str` API cleanup и расширения — Plan 91 Ф.2.6 |
| [D179](#d179-stringbuilder--pure-nova-consume-type--plan-91-ф26) | `StringBuilder` — pure Nova consume type — Plan 91 Ф.2.6 |

---

## D7. Один язык — три режима компиляции

### Что
Один и тот же исходник Nova поддерживает три режима исполнения:
**AOT** (бинарь, как Go), **JIT** (как .NET) и **интерпретатор**
(как Python). Скрипт за 1 строку и сервер на 100k строк — это
разные режимы запуска одного языка, а не разные языки.

### Правило

```bash
nova run script.nv          # интерпретатор / JIT (быстрый старт)
nova build app.nv           # AOT-бинарь, как `go build`
nova jit-server             # долгоиграющий процесс с JIT-компиляцией
```

Один и тот же `script.nv` без модификации работает во всех трёх
режимах. Эффекты, типы, контракты, handler'ы — везде ведут себя
одинаково.

### Почему

- **Скрипт vs сервер — это режимы запуска.** Не разные языки.
  Программисту не нужно «переписывать» под другой режим.
- **Прецедент Julia** — тот же подход (JIT по умолчанию + AOT через
  `PackageCompiler.jl`) работает на масштабе data-science.
- **AI-first** — LLM может генерировать код и запускать через
  интерпретатор для быстрой проверки, а тот же код собирать в бинарь
  для production.
- **Эффекты ортогональны runtime'у** — handler'ы перехватываются и в
  JIT, и в AOT, и в интерпретаторе одинаково.

### Что отвергнуто

- **Только AOT** (Rust/Go-стиль) — медленный feedback loop, плохо
  для скриптов и REPL.
- **Только интерпретатор** (Python) — производительность недостаточна
  для backend.
- **Транспиляция в чужой язык** (TypeScript → JS) — теряется
  возможность контроля runtime, привязка к чужой экосистеме.

### Связь

- [01-philosophy.md → D9](01-philosophy.md#d9-честная-оценка-новизны) —
  «три режима компиляции в строго типизированном языке» — одна из двух
  потенциальных уникальных заявок Nova.
- [01-philosophy.md → D10](01-philosophy.md#d10) — три режима следуют
  из «всё — эффект»: handler'ы абстрагируют runtime.

### Открытые вопросы

- Конкретные технологии: LLVM для AOT? Cranelift для JIT? Tree-walking
  для интерпретатора? — выбор реализации.
- Совместимость артефактов между режимами — пока считаем, что один
  исходник, разные бинарные форматы.

---

## D13. Panic vs эффекты: что НЕ является эффектом

### Что
**Не каждое прерывание вычисления — эффект.** Аппаратные/математические
сбои (деление на ноль, выход за границы массива, переполнение, OOM,
переполнение стека) **не указываются в сигнатуре** функции. Они
образуют общую категорию `Panic` — runtime-сбоев, перехватываемых
runtime'ом на границе fiber'а, не программистом в коде.

### Правило

#### Граница

| | Видимое (в сигнатуре) | Универсальное (не в сигнатуре) |
|---|---|---|
| **Что** | эффекты, описывающие **намерение** | сбои, описывающие **невозможность вычисления** |
| **Примеры** | `Net`, `Db`, `Time`, `Log`, `Fail[BusinessError]` | деление на ноль, переполнение, выход за границы, OOM, переполнение стека |
| **Где ловится** | handler'ом в коде | runtime'ом на границе fiber'а |
| **Как создаётся** | `throw` | `panic(msg)` или сам runtime |

#### Перехват — на границе fiber'а runtime'ом

`panic` означает **смерть текущего fiber'а**, не процесса. Что это
значит для процесса в целом — зависит от runtime-окружения
([06-concurrency.md → D14](06-concurrency.md#d14)):

- **HTTP-handler** — fiber на запрос. Panic = смерть fiber'а, runtime
  возвращает 500, остальные запросы продолжают.
- **Worker очереди** — fiber. Panic = задача упала, scheduler берёт
  следующую.
- **Supervised group** — supervisor видит «fiber завершился panic'ом»,
  рестартует по своей стратегии.
- **Синхронная программа без fiber-runtime** (CLI-скрипт): fiber один
  и совпадает с процессом, panic эффективно гасит процесс — но это
  **следствие топологии**, не семантика panic'а. Если нужно гарантированно
  убить процесс независимо от окружения — отдельная функция `exit`.

```nova
fn handle_request(r Request) Db Log -> Response =>
    process(r)             // если panic — fiber умирает, runtime вернёт 500
                            // если throw — handler выше ловит обычно

fn server() Net Fail -> () {
    supervised {
        spawn handle_requests()
        spawn periodic_cleanup()
    } strategy = one_for_one, max_restarts = 3
    // supervisor рестартует упавшие fiber'ы
}
```

**Никакого `try_panic`/`catch` в коде.** Программист **не ловит**
panic в обычной функции — это работа runtime'а на границе fiber'а.
Если программист хочет управляемую ошибку — пишет `throw` +
`Fail[E]`, ловит обычным handler'ом.

#### Три уровня катастрофы

| Уровень | Конструкция | Что убивает | Перехват |
|---|---|---|---|
| Управляемая ошибка | `throw err` + `Fail[E]` | ничего, передаётся handler'у | handler'ом в коде ([04-effects.md → D25](04-effects.md#d25)) |
| Сбой fiber'а | `panic(msg)` | текущий fiber | runtime'ом на границе fiber'а; supervisor может рестартовать |
| Смерть процесса | `exit(code, msg)` | весь процесс | не перехватывается — процесс гасится с указанным exit code |

Никаких `try_panic { ... } catch p { ... }` или
`panic_boundary { ... } recover (p) => { ... }` в языке. `exit`
тем более не перехватывается — это финальная точка.

##### Когда какой использовать

- **`throw err`** — контролируемая ошибка с информацией о причине.
  Всё, что вызывающий может осмысленно обработать. Дефолт.
- **`panic(msg)`** — поломан **локальный** инвариант, текущему
  вычислению дальше не жить, но процесс/сервер продолжают. Пример:
  «не должно случиться» в коде, который часть большого приложения.
- **`exit(code, msg)`** — поломан **глобальный** инвариант стартапа
  или операционной среды, продолжать процесс бессмысленно. Пример:
  битый конфиг при загрузке, нет доступа к критическим ресурсам, CLI
  завершает работу с конкретным exit code для скриптов.

```nova
// throw — обычная управляемая ошибка
fn parse(s str) Fail[ParseError] -> int =>
    if !valid(s) { throw ParseError.BadFormat } else { ... }

// panic — поломан локальный инвариант
fn pop_nonempty(mut stack []int) -> int {
    if stack.is_empty() { panic("pop_nonempty called on empty stack") }
    stack.pop()
}

// exit — нечего продолжать
fn main() Io -> () {
    ro cfg = load_config("/etc/app.toml")
              ?? exit(1, "config not found at /etc/app.toml")
    run(cfg)
}
```

##### `exit` — детали

- **Сигнатура:** `fn exit(code int, msg str) -> never`. `code` —
  exit code для процесса (по конвенции 0 = успех, ≥1 = ошибка). `msg`
  выводится в stderr перед завершением; пустая строка — без сообщения.
- **Не вызывает defer'ы / handler'ы.** Процесс гасится, стек не
  разворачивается. Если нужен cleanup — программист пишет его до
  `exit`.
- **В тестах** runtime тестов перехватывает `exit` и превращает в
  fail теста (иначе один тест убил бы всю прогонку). Это деталь
  test-runner'а, не часть языкового контракта.
- **Прецеденты:** C `exit(code)`, Go `os.Exit(code)`, Rust
  `std::process::exit(code)`, Python `sys.exit(code)` — везде
  отдельная функция от panic-аналога, везде не вызывает destructor'ы /
  defer'ы.

#### Опция: строгий режим `#strict_total`

Для критичного кода (медицина, финансы, авионика):

```nova
#strict_total
fn critical(...) -> Result =>
    // деление на ноль здесь — compile error
    // обязаны checked-операции: safe_div(a, b)?, arr.get(i)?
```

Превращает функцию в тотальную (всегда завершается). Цена — больше
кода, но для 1% случаев это окупается.

### Почему

Если бы `Fail[DivByZero]` был обязателен, он бы появился в **каждой
второй сигнатуре** (любая функция со средним арифметическим,
дисперсией, делением). К нему присоединились бы `Fail[IntegerOverflow]`,
`Fail[ArrayBounds]`. Это **синдром Java checked exceptions** —
информативность сигнатуры исчезает, потому что эффекты везде.

Сознательный компромисс: **строгая теория эффектов уступает
читабельности** в зоне аппаратных сбоев.

#### Что НЕ Panic, а обычный эффект

- Бизнес-ошибки парсинга, валидации, аутентификации → `Fail[E]`.
- Network failure, DB connection refused → `Fail[NetError]`,
  `Fail[DbError]` внутри эффекта `Net` / `Db`.
- Любая ошибка, которую программа **намерена обрабатывать**, —
  это не Panic.

**Принцип:** «обработать никак нельзя, надо умереть» → Panic;
«обработать можно и нужно» → Fail.

### Что отвергнуто

- **`Fail[DivByZero]` для каждой функции** — спам в сигнатурах.
- **`try_panic`/`catch` в обычном коде** — путает с `Fail`,
  усложняет reasoning о потоке управления.
- **Panic как обычное Throwable** (Java RuntimeException) — приводит
  к ловле «всего» через `catch (Exception e)`, антипаттерн.

### Связь

- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Fail[E]`.
- [06-concurrency.md → D14](06-concurrency.md#d14) — supervisor, fiber'ы.
- [01-philosophy.md → D10](01-philosophy.md#d10) — «всё — эффект» с
  оговоркой про runtime panics.

---

## D26. Базовая stdlib и prelude

### Что
Базовые типы (`Option[T]`, `Result[T, E]`, `Error`, `never`,
`Ordering`) и их конструкторы (`Some`, `None`, `Ok`, `Err`) живут в
**prelude** — автоматически в скоупе любого модуля, без `import`.
Список prelude **явно зафиксирован** в одном месте, не «магия».

> **Bootstrap-расширение (Plan 35 sub-plan 35.A R27, 2026-05-12):**
> большая часть prelude (`Option`/`Result`/`Some`/`None`/`Ok`/`Err`/
> `Error`/`never`/`print`/`println`/`panic`) реализована **hardcoded**
> в type-checker'е и codegen'е. Параллельно `compiler-codegen::imports`
> auto-импортирует `std/prelude.nv` если файл существует — это
> opt-in mechanism для расширения prelude из пользовательского кода
> (или для миграции hardcoded items в file-based form). Bootstrap MVP:
> `std/prelude.nv` содержит placeholder `PRELUDE_VERSION = 1`.
>
> **Plan 62 (закрыт 2026-05-18, `PRELUDE_VERSION = 3`):** большая часть
> prelude мигрирована в file-based декларации `std/prelude/*.nv`:
> - `std/prelude/core.nv` — `Option`/`Result`/`Some`/`None`/`Ok`/`Err`/
>   `Error`/`Ordering`. Bottom-тип `never` — строчный встроенный
>   примитив (Plan 76), в prelude не объявляется (как `int`/`bool`).
> - `std/prelude/runtime.nv` — `panic`/`exit`/`assert`/`debug_assert`
>   (`print`/`println` migrated в Plan 62.B.bis — `PRELUDE_VERSION = 7`,
>   2026-05-18).
> - `std/prelude/errors.nv` — `RuntimeError` (6 variants) +
>   `ReadBufferError` (`RuntimeNoneError` deferred — bootstrap parser
>   не поддерживает empty-body sum syntax).
> - `std/prelude/collections.nv` — `Iter[T]` formal protocol declaration.
> - `std/prelude/protocols.nv` — `From`/`Into`/`Hashable`/`Equatable`/
>   `Comparable`/`Display` (6 formal protocols; `TryFrom`/`TryInto`
>   deferred — Plan 56 Ф.2.7 effect-row enforcement).
> - `std/prelude/effects.nv` — `Fail[E]` formal effect declaration.
>
> **Plan 62.D bis-1 (закрыт 2026-05-18, `PRELUDE_VERSION = 4`):**
> `Range` / `RangeIter` re-export через prelude facade из
> `std.collections.range`. Раньше эта строка триггерила 4 latent
> codegen bugs (закрыты в bis-1).
>
> **Plan 62.F.bis (закрыт 2026-05-18, `PRELUDE_VERSION = 5`):**
> - **Edition versioning** (D124): `[package].edition = "2026.05"` в
>   `nova.toml` → resolver auto-импортирует `std/prelude/e2026_05.nv`
>   вместо rolling facade. Mirror Rust's `edition = "2021"`. См.
>   [D124](#d124-edition-versioned-prelude-resolver).
> - **Structured W_PRELUDE_SHADOW lint** (D125): user-declaration
>   shadowing prelude-imported имени → structured lint warning через
>   `lints::lint_prelude_shadow`. Suppress: `module X
>   allow_prelude_shadow` clause. См. [D125](#d125-prelude-shadow-warning-lint).
> - **`Time`/`Mem` formal effect declarations** добавлены в
>   `std/prelude/effects.nv` (codegen dispatch неизменен через
>   pre-registered `effect_schemas`).
>
> **Plan 62.D.bis (закрыт 2026-05-18, `PRELUDE_VERSION = 6`):**
> StringBuilder/WriteBuffer/ReadBuffer formally declared через
> `external type` (D126) в `std/prelude/collections.nv`. Закрывает
> последний known-by-name hole в D26 visible prelude. Methods
> остаются в `std/runtime/<name>.nv` через `external fn` (D82) —
> связь по receiver-type name. См. [D126](03-syntax.md#d126-external-type--opaque-типы-без-body).
>
> **Plan 62.B.bis (закрыт 2026-05-18, `PRELUDE_VERSION = 7`):**
> `print`/`println` formally declared в `std/prelude/runtime.nv` через
> D69 variadic + `[]any` (canonical D26 signature `fn print(...items
> []any) Io -> ()`). Plan 67 hotfix (silent-wrong-output bug в
> `infer_print_helper` для `println(str.from(int))` паттерна) absorbed
> как Ф.0 — refactor через unified `infer_expr_c_type` dispatch.
> Codegen special-case (emit_c.rs:11270) fires ДО variadic routing
> (Ф.1 reorder) — preserves per-arg type info, synthesized `[]any`
> array никогда не строится; per-arg `nova_print_<type>` dispatch
> сохраняется через `infer_print_helper` → unified inference.
> Builtins HashSet shrink: `"print"`, `"println"` removed (Ф.5).
> Cross-file resolve через R26+R27 находит declarations.
> См. [Plan 62.B.bis](../../docs/plans/62.B.bis-print-println-migration.md).
>
> **Plan 62.A.bis (закрыт 2026-05-20):** введён layered schema registry
> для sum-types в codegen (`SumSchemaRegistry` —
> `compiler-codegen/src/codegen/sum_schema_registry.rs`). Registry
> работает в трёх слоях с убывающим приоритетом:
> `DeclaredFromPrelude > DeclaredFromUser > HardcodedBaseline`.
> Hardcoded entries (Option/Result/Error/RuntimeError) остаются в
> качестве ABI-compat fallback для runtime-хелперов в `nova_rt/array.h`.
> File-based декларации в `std/prelude/core.nv` (через `external fn
> Option[T] @method`) получают приоритет и маршрутизируют вызовы через
> `MethodRouting` registry (HardcodedRuntimeFn / ExternalFn /
> DeclaredBody). Unblocked: 7 из 8 методов Option (is_some, is_none,
> unwrap, unwrap_or, unwrap_or_else, map, ok_or) + 4 из 9 методов
> Result (is_ok, is_err, ok, err) — задекларированы в `std/prelude/core.nv`.
> Deferred в core: 5 Result-методов возвращающих `T` (unwrap_or и др.)
> — blocker: type-checker выводит generic `T`, codegen возвращает
> `nova_int`, `==` после вызова ломается (Plan 62.B+). `Option.or` —
> trampoline в `nova_rt/array.h` отсутствует (Plan 62.B+). Phase 4
> (удаление legacy `sum_schemas`) deferred до Plan 59 sum-mono.
>
> **Remaining deferred:** `RuntimeNoneError` (bootstrap parser
> empty-sum syntax), `TryFrom`/`TryInto` (Plan 62.E.bis — требует
> Plan 56 Ф.2.7 effect-row enforcement). Bottom-тип `never` — закрыт
> Plan 76 (строчный встроенный примитив, не требует prelude-декларации).
>
> **Plan 99 (закрыт 2026-05-23):** последние 6 closure-applying
> Option/Result-методов перенесены на Nova-body в `std/prelude/core.nv`:
> `Option.map[U]`, `Option.unwrap_or_else`, `Option.ok_or[E]`,
> `Result.map[U]`, `Result.map_err[F]`, `Result.unwrap_or_else`.
> **15 / 17 Option/Result методов на Nova-body** (7 Option + 8
> Result), C-routed остаются только `Option.unwrap` и
> `Result.unwrap` (Plan 61 lineage — typed `Fail[E]` effect).
> Декомпозирован на 4 sub-plan'а:
> Plan 99.1 (foundation — method-level generic в DeclaredBody:
> extract `resolve_method_level_subst` helper, mono_name с
> method-level suffix, `register_novaopt_decl(U)` lazy-emit,
> `infer_method_level_return_for_sum` для `infer_expr_c_type`);
> Plan 99.2 (contextual variant constructors — bare `None` использует
> `current_fn_return_ty`; `Ok(v)`/`Err(e)` берут (T,E) из rt; bare
> `Some(v)` использует ARG-type через `infer_expr_c_type(arg)`
> чтобы sub-expr контексты — `s.char_at(i) == Some('/')` в
> `Option[int]`-fn — не строили `NovaOpt_<rt's_X>` для arg иного
> типа); Plan 99.3 (atomic per-method migration — 6 commits с
> regression-gate); Plan 99.4 (comprehensive tests + spec + close).
> Closure invoke через `NovaClosBase` + explicit cast — паритет
> Rust `FnOnce`-mono. Param-naming: closure-параметры
> `default_fn`/`map_fn`/`err_fn` (не `f`) — избегаем shadowing
> user-функций (см. `contracts/trivial_congruence_positive`
> регрессию). Полный nova test: 1141 PASS / 0 FAIL / 56 SKIP.
>
> **Plan 95.bis (закрыт 2026-05-23):** расширение Plan 95 — ещё 5
> «чистых» Option/Result-методов перенесены на Nova-body в
> `std/prelude/core.nv`: `Option.unwrap_or`, `Option.or`,
> `Result.unwrap_or`, `Result.ok`, `Result.err`. Удалены все
> соответствующие C-трамплины из `nova_rt/array.h` (включая
> `NOVA_ARRAY_IMPL`-macro entry `Nova_Option_method_or_<T>` + explicit
> `_nova_str` специализация, `Nova_Result_method_unwrap_or_<n>`,
> `Nova_Result_method_ok_<n>` + back-compat `#define`-алиасы) и
> lazy-emit в `register_novaopt_decl`/`register_novares_decl`. Также
> удалён inline emit `Result.err()` в codegen (Plan 59 Ф.7.5 D3 —
> теперь Nova-body эмитит boxed payload сам через mono'd
> `register_novaopt_decl` path). Result `DeclaredBody`-dispatch
> доработан: mono-имя **всегда суффиксированный**
> (`Nova_Result_method_<m>_<n>`), даже для legacy `Nova_Result*`
> obj_ty, чтобы избежать C-redefinition. Граница не изменилась:
> `unwrap` (Fail-handler, Plan 61), `unwrap_or_else`/`map`/`map_err`/
> `ok_or` (closure-applying + method-level generic + Plan 98
> inference) — остаются C-routed.
>

> **Plan 95 (закрыт 2026-05-23):** builtin sum-типы `Option`/`Result`
> участвуют в method-monomorphization через канал «method-only mono»
> — без регистрации в `generic_type_templates` (представление
> `NovaOpt_<T>` / `NovaRes_<ok>_<err>*` не трогается). Pre-existing
> `MethodRouting::DeclaredBody` (scaffold-only до Plan 95) теперь
> реально конструируется в `init_prelude_decls_from_items` для
> non-external методов на `Option`/`Result`, потребляется в перехватах
> вызова `NovaOpt_` (#6 в [emit_c.rs:14160](../../compiler-codegen/src/codegen/emit_c.rs#L14160))
> и `is_result_like` (#7). `receiver_c_type` спец-кейсит
> `Option`/`Result` → value-тип через `current_type_subst` + сохранённые
> `builtin_sum_type_params`. Mono-имя совпадает с формой бывшего
> C-трамплина (`Nova_Option_method_<m>_<T_sani>` /
> `Nova_Result_method_<m>_<n>`) → call-site mangling не меняется.
> **Перенесены на Nova-body:** `Option.is_some`/`is_none`,
> `Result.is_ok`/`is_err` (`=> match @ { ... }` в
> `std/prelude/core.nv`); C-трамплины удалены из `nova_rt/array.h`,
> lazy-emit в `register_novaopt_decl`/`register_novares_decl`, и
> baseline-entries в `init_hardcoded_baseline`. Граница: `unwrap`
> (Fail-dispatch), `unwrap_or`/`unwrap_or_else`/`map`/`ok_or`/`map_err`
> (closure-applying) — **остаются** C-routed. Закрыт маркер
> `[M-option-methods-not-mono-able]`. Plan 93 (узкий вариант
> «is_some-Nova-body») superseded by Plan 95 — целиком поглощён Ф.4.
> Plan 78 (prelude-codegen single-source) — узкий санкционированный
> пересмотр Ф.1 только для чистых тег-предикатов; реестр C-routing
> в силе.

### Правило

#### Что в prelude (v1.0)

**Типы:**

```nova
type Option[T] | Some(T) | None
type Result[T, E] | Ok(T) | Err(E)
type Ordering | Less | Equal | Greater
// `never` — bottom-тип (uninhabited): строчный встроенный примитив,
// НЕ объявляется (как `int`/`bool`). См. «`never` — bottom-тип» ниже.
type any protocol { }                            // top-type через пустой protocol (D53)
```

**Базовые методы `Option[T]`:**

```nova
fn Option[T] @is_some() -> bool
fn Option[T] @is_none() -> bool
fn Option[T] @unwrap() Fail[Error] -> T              // throw "called unwrap on None"
fn Option[T] @unwrap_or(default T) -> T              // None → default
fn Option[T] @unwrap_or_else(f fn() -> T) -> T       // None → f() (lazy default)
fn Option[T] @map[U](f fn(T) -> U) -> Option[U]
fn Option[T] @ok_or[E](err E) -> Result[T, E]        // None → Err(err)
fn Option[T] @or(other Option[T]) -> Option[T]
```

**Базовые методы `Result[T, E]`:**

```nova
fn Result[T, E] @is_ok() -> bool
fn Result[T, E] @is_err() -> bool
fn Result[T, E] @ok() -> Option[T]                   // Ok(v) → Some(v); Err → None
fn Result[T, E] @err() -> Option[E]                  // Err(e) → Some(e); Ok → None
fn Result[T, E] @unwrap() Fail[E] -> T               // Err(e) → throw e
fn Result[T, E] @unwrap_or(default T) -> T           // Err → default
fn Result[T, E] @unwrap_or_else(f fn(E) -> T) -> T   // Err → f(e) (lazy)
fn Result[T, E] @map[U](f fn(T) -> U) -> Result[U, E]
fn Result[T, E] @map_err[F](f fn(E) -> F) -> Result[T, F]
```

`unwrap_or` / `unwrap_or_else` — основной идиоматический путь
безопасного доступа к значению с fallback. Прецеденты — Rust
`Option::unwrap_or`, Swift `??` оператор, TypeScript `??`.

```nova
ro n int = parse_int(s).unwrap_or(0)               // на ошибке — 0
ro cfg = config.unwrap_or_else(|| default_config())  // lazy default

// Идиома: цепочка через map / unwrap_or:
ro port int = env.get("PORT").map(parse_int).unwrap_or(8080)
```

`@unwrap()` — assertion-style: throw'ает Fail если None/Err. Идиома
для случаев когда программист **гарантирует** что значение есть
(prove'ил выше через `if let` / `match`). Caller-side либо ловит
через `with Fail = ...`, либо позволяет распространиться (паника
на границе fiber'а — D13).

#### Bootstrap status (2026-05-08)

| Метод | Codegen | Тесты |
|---|---|---|
| `Option.is_some` / `is_none` | ✅ | ✅ |
| `Option.unwrap` (Fail на None) | ✅ inline | ✅ runtime/unwrap_or.nv |
| `Option.unwrap_or(default)` | ✅ runtime helper | ✅ |
| `Option.unwrap_or_else(f)` | ✅ inline (closure call) | ✅ runtime/result_methods.nv |
| `Option.map(f)` | ✅ inline | ✅ |
| `Option.ok_or(e)` | ✅ inline | ✅ |
| `Option.or(other)` | ✅ per-T trampoline `Nova_Option_method_or_<T>` | ✅ plan62/option_or_from_prelude.nv |
| `Result.is_ok` / `is_err` | ✅ | ✅ |
| `Result.ok()` → Option[T] | ✅ runtime helper | ✅ |
| `Result.err()` → Option[E] | ✅ inline (boxed nova_str) | ✅ |
| `Result.unwrap` (Fail на Err) | ✅ inline | ✅ |
| `Result.unwrap_or(default)` | ✅ runtime helper | ✅ |
| `Result.unwrap_or_else(f)` | ✅ inline (closure call) | ✅ |
| `Result.map(f)` | ✅ inline | ✅ |
| `Result.map_err(f)` | ✅ inline | ✅ |
| `Error.new(msg)` | ✅ runtime helper | ✅ runtime/error_runtime_error.nv |
| `Error.msg` (field) | ✅ direct field access | ✅ |
| `RuntimeError.DivByZero` | ✅ unit-variant constructor | ✅ |
| `RuntimeError.Overflow` | ✅ unit-variant constructor | ✅ |
| `RuntimeError.IndexOutOfBounds {i, n}` | ✅ record-variant constructor | ✅ |
| `RuntimeError.TypeMismatch(s)` | ✅ tuple-variant constructor | ✅ |
| `RuntimeError.AssertFailed(s)` | ✅ tuple-variant constructor | ✅ |
| `RuntimeError.NoHandler(s)` | ✅ tuple-variant constructor | ✅ |

> **Plan 62.B (2026-05-20):** `Option.or` реализован — per-T trampoline
> `Nova_Option_method_or_<T>`. Все 17 Option/Result методов из §283-306
> теперь задекларированы в `std/prelude/core.nv` через `external fn`
> (раньше 5 Result-методов — `unwrap`/`unwrap_or`/`unwrap_or_else`/`map`/
> `map_err` — оставались hardcoded-only из-за generic-стаб блокера в
> type inference, см. plan-doc 62 §«Status update 2026-05-20»). Починен
> pre-existing баг `Result.map` для `bool`/`char`-typed closure
> (хардкод `NOVA_CLOS_CALL_ii` int-layout → calling-convention mismatch).

**Bootstrap-ограничения**:
- ~~`Result[T, E]` зашит на `(nova_int Ok, nova_str Err)`. Generic
  monomorphization для произвольных T/E — отдельная задача
  (Q-result-monomorphization).~~ **✅ ЗАКРЫТО (Plan 59 Ф.7.5
  increment 2, 2026-05-21):** `Result[T, E]` полностью
  мономорфизирован — per-(T,E) C-тип `NovaRes_<ok>_<err>*` (аналог
  `NovaOpt_<T>`), реальные типы в Ok/Err payload'е. Legacy единый
  `Nova_Result` устранён.
- Lambda-параметры с не-`int` типом (например `fn(e str) -> str => ...`
  для `map_err`) требуют **явной аннотации** через closure-full
  (`fn(...)`). Closure-light (`|x|`) полагается на context-inference;
  если method-sig недостаточен — переключайся на closure-full.
  Codegen в bootstrap не делает inference closure-параметра по
  сигнатуре method'а (Q-closure-param-inference).
- Zero-arg closure для `unwrap_or_else` — `|| expr` (closure-light)
  или `fn() -> T => expr` (closure-full). Парсер различает
  `||`-closure-start от `||`-binary OR по позиции.
- `Error` имеет поле `msg`. По D26 spec'у должно быть `readonly msg`,
  но bootstrap не enforce'ит readonly — поле модифицируется как
  обычное (bootstrap-grade compromise).
- `RuntimeError` варианты создаются и matchаются user-кодом, но
  **встроенные операции** (`a/b` на 0, `arr[i]` out-of-bounds,
  unhandled effects) пока бросают `nova_str` через `Nova_Fail_fail`,
  не структурированный `Nova_RuntimeError*`. Конверсия throw-points
  в RuntimeError-payload — отдельная задача (требует расширения
  fail-frame mechanism с `nova_str` на `void*` payload).

**Прочие prelude-типы:**

```nova
// Error — record для quick-and-dirty ошибок с сообщением (D65)
type Error {
    ro msg str
}
fn Error.new(msg str) -> Error => { msg }

// RuntimeError — sum-тип встроенных runtime-сбоев (D65)
// Бросается встроенными операциями: a/b на 0, arr[i] на out-of-bounds, etc.
// StackOverflow и OutOfMemory не входят — они panic, не Fail (D13).
type RuntimeError
    | DivByZero
    | Overflow
    | IndexOutOfBounds { index int, length int }
    | TypeMismatch(str)
    | AssertFailed(str)
    | NoHandler(str)

// RuntimeNoneError — unit-тип, бросается через `expr!!` на Option (D85).
// Отдельный от RuntimeError — это категория «отсутствие значения», не
// аппаратный сбой.
type RuntimeNoneError

// Iterator protocol (D58)
type Iter[T] protocol {
    mut next() -> Option[T]
}

// Range — литерал `a..b` / `a..=b` (D58)
type Range {
    ro start int
    ro end int
    ro inclusive bool
}
type RangeIter {
    end       int
    inclusive bool
    mut cur   int
}

// Built-in opaque accumulator/buffer типы (Plan 04, D82, D126).
// Formal declarations — std/prelude/collections.nv через `external type`
// (D126, Plan 62.D.bis, 2026-05-18). Methods — std/runtime/string_builder.nv,
// std/runtime/write_buffer.nv, std/runtime/read_buffer.nv через `external fn`
// (D82, Plan 13 Ф.8; раньше были в едином std/runtime/builtins.nv —
// REMOVED 2026-05-08). До 62.D.bis типы существовали как «known-by-name»
// (без formal Nova-side declaration) — теперь canonical source в prelude.
// `[]u8` — canonical byte-slice (Plan 69, byte→u8 migration).
external type StringBuilder    // UTF-8 string accumulator, @into() -> str (infallible)
external type WriteBuffer      // binary write buffer, @into() -> []u8
external type ReadBuffer       // cursor-style binary reader, view над []u8

// Ошибка ReadBuffer — недостаточно байт для read-операции.
type ReadBufferError
    | UnexpectedEnd { wanted int, available int }
```

**Базовые числовые и строковые типы** (`int`, `i8`-`i64`, `u8`-`u64`,
`f32`, `f64`, `str`, `bool`, `char`, `()`) — встроены в язык,
не stdlib, но упомянуты для полноты.

**Size-accessor методы для built-in `[]T` и `str`** (Plan 60 / [D117](03-syntax.md#d117-size-like-accessors-require-call-syntax)):

```nova
fn []T @len() -> int                // O(1), zero-cost lowering arr->len
fn []T @capacity() -> int           // O(1), zero-cost lowering arr->cap
fn []T @is_empty() -> bool          // O(1), len() == 0
fn str @len() -> int                // O(1) — байты (Plan 108 D26 rev)
fn str @char_len() -> int           // O(n) — codepoints (UTF-8 walk)
fn str @byte_len() -> int           // O(1) — deprecated alias для @len()
fn str @is_empty() -> bool          // O(1) — len() == 0
```

Field-access form (`arr.len`, `s.byte_len`, etc.) запрещён в
user-language — D117 enforce'ит method-only. Internal C-поля
`arr->len` / `arr->cap` сохраняются как implementation detail.

**Built-in opaque-типы для аккумуляции** (`StringBuilder`,
`WriteBuffer`, `ReadBuffer`) — расширяют примитивы D26. **Type
declarations** — в `std/prelude/collections.nv` через `external type`
([D126](03-syntax.md#d126-external-type--opaque-типы-без-body),
Plan 62.D.bis, 2026-05-18). **Methods** — в `std/runtime/string_builder.nv`,
`std/runtime/write_buffer.nv`, `std/runtime/read_buffer.nv` (auto-generated
через Plan 13 Ф.8) — `external fn` декларации ([D82](#d82-external-fn--функции-с-runtime-implementation)).
Программист **не пишет** `type StringBuilder { ... }` body — `external
type` — это opaque marker, реализация в runtime (`nova_rt/`).

| Тип | Глагол | Финализация | Use-case |
|---|---|---|---|
| `StringBuilder` | `@append` | `@into() -> str` infallible | string concat в hot loop |
| `WriteBuffer` | `@write_*` | `@into() -> []u8` | binary serialize |
| `ReadBuffer` | `@read_*` / `@try_read_*` | view, no into | binary parse |

Эти три типа **заменяют** старый унифицированный `Buffer` (Q-buffer
закрыт REPLACED 2026-05-08). Причина split: text+binary mixed
ломает `@into() -> str` infallible-семантику. См. Plan 04.

#### `@clone()` — shallow по умолчанию (Plan 17 Ф.1)

Конвенция в Nova:

> **`@clone() -> Self` — shallow copy.** Возвращает новый экземпляр с
> тем же набором полей; managed-references (другие record'ы, массивы,
> вложенные коллекции) после clone **разделяются** между оригиналом и
> копией. Для глубокой копии — `@deep_clone()` (не в prelude,
> определяется по необходимости вручную).

**Что значит «shallow» для разных категорий:**

- **Примитивы** (`int`, `f64`, `bool`, `char`, `u8`) — value semantics,
  clone = тривиальная копия.
- **`str`** — immutable, `s.clone()` возвращает тот же ptr (равноценно
  присваиванию). Семантически независимая копия не нужна.
- **Record** — копируются поля; managed-поля (вложенные record'ы,
  массивы) — по ссылке.
- **`[]T`** — копируется внутренний `(ptr, len, cap)`-storage в свежий
  buffer (O(n) поверхностно), но элементы `T` — managed-references
  share'аются если `T` сам не примитив.
- **HashMap / Vec / Set / Queue (stdlib)** — копируется внутренний
  storage, элементы и ключи — по ссылке.
- **`StringBuilder`, `WriteBuffer`** — `@clone()` тут **deep** для
  внутреннего byte-buffer'а, потому что **сам тип определён как
  mutable accumulator с уникальным storage'ом** — shared buffer между
  clone'ами = data race по семантике D26. Это **исключение из общего
  shallow-правила**, обоснованное mutability-семантикой типа.

**Когда писать `@deep_clone()`** — когда нужно гарантировать, что
после clone никакая мутация одной копии не видна другой. Stdlib не
вводит общий `@deep_clone()`-protocol; программист реализует на
конкретном типе:

```nova
fn HashMap[K, V] @deep_clone() -> HashMap[str, []int] {
    mut out = HashMap[str, []int].new()
    for (k, v) in @ {
        out.insert(k, v.clone())     // элементы клонируются shallow
    }
    out
}
```

Прецедент: Rust `Clone` shallow по умолчанию, deep — руками. Java
`Object.clone()` shallow, override для deep. Go — value semantics на
структурах + reference semantics на slice/map (=shallow на assign).

**Bootstrap status (2026-05-08):** только `StringBuilder.@clone()` и
`WriteBuffer.@clone()` зарегистрированы как built-in (deep, через
`Nova_*_clone` C-функции). Для record/коллекций программист пишет
clone вручную.

Подробно — Plan 17 Ф.1, [Q-clone-semantics](../open-questions.md#q-clone-semantics)
(closed).

`StringBuilder.@into() -> str` — **infallible** (UTF-8 invariant
поддерживается каждым `@append`, который принимает только `str` или
`char`). `WriteBuffer.@into() -> []u8` — infallible (произвольные
байты валидны как `[]u8`). `ReadBuffer` — view, `@into()`
**не определён** (явный throw блокирует D73 auto-derive).

`ReadBuffer` пара `@read_*` (Fail-form) / `@try_read_*` (Result-form)
— **обе формы явно** в `runtime_registry.rs` и в `std/runtime/read_buffer.nv`.
Каждая Fail-форма имеет независимую C-функцию `Nova_ReadBuffer_method_read_X`,
а Result-форма — `Nova_ReadBuffer_method_try_read_X`. Автоматический
синтез одной из другой **отменён** (Plan 13 Ф.9.5; ранее Plan 12 Ф.4.5
предлагал такое правило, но было отменено для соблюдения D82 single-source-
of-truth — всё что компилятор знает, должно быть в registry явно).

**`char` — Unicode codepoint, НЕ UTF-8 byte sequence.** `char` хранит
**одно скалярное значение Unicode** (диапазон 0..0x10FFFF, исключая
surrogate pairs 0xD800..0xDFFF). Размер в памяти — 4 байта (как Rust
`char`, Go `rune`, Swift `Unicode.Scalar`).

`str` хранит UTF-8 байты, `char` — codepoint. Конверсии:
- `char → str` или `char → []u8` — UTF-8 encode (1-4 байта в
  зависимости от значения; см. `Buffer.add_char` в Q-buffer).
- `str.chars() -> Iter[char]` — UTF-8 decode по ходу итерации.

Это разделение типичное для современных языков (Rust, Swift). Go
использует `rune` = `int32` по тому же принципу. C `char` это byte —
**не** аналог Nova `char`.

Bootstrap-status: `char` зарезервирован как тип, но синтаксис
char-литералов (`'a'`) — ещё открытый вопрос (Q-char-literals).
В коде сейчас используется `nova_int` напрямую (передаём codepoint
как число) — это будет заменено на нормальный `char` при закрытии
Q-char-literals.

**`str` — Unicode-string.** Внутреннее представление — UTF-8 байты
`(ptr, byte_len)`, но **все public operations работают на уровне
codepoint'ов** (Unicode scalar values). Содержимое — валидный UTF-8
по конвенции: литералы, конкатенация и `str.from(...)` гарантируют
валидность; FFI-код должен сам проверять при создании `str` из
чужого буфера.

**Длина и индексация (codepoint-indexed, школа Python/Swift):**

- `s.len` — длина в **codepoint'ах**, O(n) (требует обхода UTF-8).
  Это **базовая** «длина строки» с точки зрения программиста.
- `s.byte_len()` — длина в байтах, O(1). Для FFI и буферных операций.
- **`s[a..b]` (slice, bracket-form)** — принимает **codepoint-индексы**,
  O(b) (нужен обход до byte-offset'ов). Boundary всегда корректные —
  невозможно попасть в середину multi-byte sequence. **Panic при OOB**
  (consistent с `arr[a..b]`, [D144](02-types.md#d144)). Также 5 форм
  Range: `s[a..b]`/`s[a..=b]`/`s[a..]`/`s[..b]`/`s[..]`.
- `s[i]` (codepoint indexing) — `Option[char]`, O(i). `None` если
  `i >= s.len`. См. также Q-string-indexing.
- `s.chars() -> Iter[char]` — ленивый обход codepoint за codepoint.

> **Plan 96.1 (2026-05-23):** метод `s.slice(a, b)` **удалён** в пользу
> bracket-формы `s[a..b]` (D9 «один очевидный путь»; convergence Rust/Go/
> Swift/Python — bracket-only). Старая clamp-семантика метода (OOB →
> обрезка до длины) удалена; bracket-form всегда panic'ит на OOB —
> симметрично с `arr[a..b]` (D144). Closes `[P-str-slice-clamp-vs-panic]`.

**Поиск, сравнение, конверсия** (все индексы — **codepoint-offset**):

```nova
fn str @find(needle str) -> Option[int]          // codepoint-offset
fn str @rfind(needle str) -> Option[int]         // последний codepoint-offset
fn str @contains(needle str) -> bool
fn str @starts_with(prefix str) -> bool
fn str @ends_with(suffix str) -> bool
fn str @split(sep str) -> Iter[str]
fn str @trim() -> str
fn str @to_lower() -> str
fn str @to_upper() -> str
```

`s.find(":") -> Option[int]` возвращает **codepoint-индекс** ":".
Это передаётся напрямую в bracket-slice `s[0..i]`:

```nova
ro s = "Привет:мир"           // 10 codepoints, 19 bytes
ro i = s.find(":").unwrap_or(0)  // i == 6 (codepoints)
ro key = s[0..i]              // "Привет"
ro val = s[i + 1..]           // "мир" (open-end)
assert(s.len() == 10)            // codepoints
assert(key.len() == 6)
```

**Почему codepoint-indexing (школа B) выбрана для Nova:**

1. **AI-friendly.** LLM генерирует код где `s.len` интуитивно
   «количество символов». Byte-уровень (Rust/Go) — источник bug'ов
   у новичков и AI: `"Привет".len == 12` нелогично.
2. **Безопасность boundary.** Невозможно попасть в середину UTF-8
   sequence — все индексы codepoint-выровнены.
3. **Consistency.** `find` / `s[a..b]` / `s[i]` — все codepoint-уровень,
   не нужно мысленно переключаться между byte и codepoint.
4. **Прецеденты:** Python (codepoints), Swift (graphemes — ещё выше),
   Java (UTF-16 code units, близко к codepoint для BMP). Все
   современные языки кроме system-low-level (Rust, Go, C) выбирают
   codepoint-or-grapheme уровень.

**Цена:**

- O(n) для `s.len`, O(b) для `s[a..b]` — обходы UTF-8.
  Внутреннее byte-хранилище неизбежно: альтернатива (UTF-32 4-byte
  per char) утроит память для ASCII-heavy кода.
- Hot-path работа с byte-уровнем — через explicit `s.bytes()`
  → `[]u8` или через `Buffer` (Q-buffer).
- В Nova принципе AI-генерация важнее микро-perf для primitive ops;
  программист может явно перейти на byte-уровень там где надо.

**FFI / byte-уровень доступен через:**

```nova
fn str @byte_len() -> int                    // O(1) — для C-interop размеров
fn str @bytes() -> []u8                    // copy (D73 []u8.from(s))
```

**Конверсия в `[]u8` через D73:**
- `[]u8.from(s str) -> []u8` — infallible (всегда работает,
  `str` гарантированно валидный UTF-8). **Копирует**
  `s.ptr..s.ptr+s.len` в свежий `[]u8`. D73 авто-синтезирует
  `s.into()` для `let b []u8 = s.into()`.
- Копирует, не view: Nova не имеет readonly-меток (D6 — managed
  heap без borrow-checker), а `[]u8` mutable — без копии mutate
  испортил бы immutability `str`. Стоимость O(n) — приемлемо для
  границы str↔bytes; для in-place аккумуляции использовать `Buffer`
  (Q-buffer).
- `str.from(b []u8) Fail[Utf8Error] -> str` — fallible-форма
  (D73 + Fail-effect). Валидирует UTF-8; на ошибке throw'ает.
  Auto-derived: `b.into()` тоже декларирует `Fail[Utf8Error]`.
  Result-форма (`str.try_from(b)` → `Result[str, Utf8Error]`)
  доступна через D77 как convenience sugar.

**Nul-termination (C-interop):** `nova_str_concat` сейчас аллоцирует
`len + 1` байт и кладёт `\0` после данных, чтобы `s.ptr` можно было
передать в C-функции. Литералы тоже nul-terminated (`.rodata` C-string).
Slice — **НЕ** добавляет `\0` (просто view). Это значит
`nova_str.ptr` — **не** гарантированно cstring; зависит от того как
строка построена. **Открытый вопрос (Q-cstring):** либо унифицировать
("все `nova_str` всегда nul-terminated, slice копирует") ценой
аллокаций, либо отказаться от частичной гарантии и ввести явный
`s.as_cstr() -> *const char` (с копированием при необходимости).
В bootstrap'е действует текущее inconsistent поведение.

**Дедупликация / interning:** `str` **не интернируется автоматически**.
Одинаковые runtime-строки — разные инстансы. `==` сравнивает контент
(memcmp), O(min). Compile-time литералы deduplicate-аются C-компилятором
через стандартное string-literal pooling в `.rodata`. Для opt-in
interning — **открытый вопрос (Q-string-interning):** Atom-тип или
`Sym[T]` (Erlang-style); прецеденты — Rust не интернирует, Java/C#
имеют пул для литералов + opt-in `intern()`.

**Конкатенация:** `s1 + s2` — O(a+b), новая аллокация каждый раз.
В hot loop `s = s + x` × N → O(N²). Для аккумуляции использовать
**`Buffer`** (Q-buffer; финализация через `@try_into() -> Result[str,
Utf8Error]` для UTF-8 или `@into() -> []u8` для сырых данных).
Nova унифицирует string-builder и byte-buffer в один тип — отличается
от Go (`bytes.Buffer` + `strings.Builder`) и Rust (`Vec<u8>` +
`String`).

См. также [Q-char-literals](../open-questions.md) (синтаксис
char-литералов) и [D54](03-syntax.md#d54) (`as`/`is` для конверсий).

**Математические операции на числовых типах** объявлены как
**instance-методы** через `@` ([D74](#d74-математические-операции-на-числовых-типах--instance-методы)):
`x.sqrt()`, `theta.cos()`, `y.atan2(x)`, `a.hypot(b)`, `n.abs()`,
`x.is_finite()`, etc. Static-функции — только для констант
(`f64.PI`, `f64.NAN`) и парсинга (`f64.try_parse(s)`).

**`any`** — пустой protocol-тип (D53). Любой тип удовлетворяет
пустому контракту, поэтому `any` — top-type (универсальный супертип).
Имя lowercase — исключение в [03-syntax.md → D30](03-syntax.md#d30)
naming convention, по аналогии с примитивами. Использование:
`fn dump(x any) Io -> ()`, `Logger.log_event(level, fields []any)`
для гетерогенных структурных логов.

**`Iter[T]`** — структурный protocol для итераторов (D58). Любой
тип с методом `mut next() -> Option[T]` автоматически удовлетворяет.
`for x in collection`-синтаксис вызывает `collection.iter().next()` в
цикле; коллекции реализуют `iter()` возвращая собственный iterator-тип.

**`Range`** — runtime-представление range-литерала `a..b` (exclusive)
и `a..=b` (inclusive) (D58). Range — обычное значение, можно
передавать как аргумент, хранить в переменной, использовать в `for`.

**Стандартные эффекты** в prelude — после [D62](04-effects.md#d62)
делятся на **две категории** по влиянию на семантику программы:

#### Semantic effects — влияют на результат

Программист **обязан** объявить в сигнатуре, если функция их
использует. Caller получает информацию что зависит от resource'а.

| Эффект | Resource | Тестовый handler |
|---|---|---|
| `Fail[E]` | error reporter | `with Fail[E] = \|e\| ...` |
| `Io` | stdout/stderr | mock-stdout |
| `Net` | сеть (HTTP/socket) | recorded responses |
| `Db` | соединение к БД | in-memory db |
| `Fs` | файловая система | virtual-fs |
| `Time` | clock | `fixed_ms(ms u64)` / `mut_clock(start_ms u64)` |
| `Random` | RNG | `seeded(seed u64)` |
| `Log` | logger | capture-log |
| `Ask[T]` | контекстный read (Reader) | fixed value |
| `Alloc[R]` | region аллокация | (для real-time, [D6](05-memory.md#d6)) |
| `Detach` | background scheduler | `SyncDetach` |
| `Blocking` | OS-thread pool | mock |

#### Instrumental effects — observability, ambient

`Mem` ([D76](#d76)) и `Trace` — **не влияют** на результат программы,
только на наблюдаемость. Программист **не декларирует** их в
сигнатуре; компилятор не лифтит через D28-inference.

```nova
// Программист пишет:
fn parse_data(s str) -> Data { ... }

// Внутри может быть Trace.span("parse"), Mem.alloc_count() — это
// implementation detail, в сигнатуру НЕ лифтится.
```

**Ambient capability — прецедент `Async` (D14/D62).** Если в скоупе
нет active handler для instrumental эффекта — runtime-panic
(`RuntimeError.NoHandler("Mem")` через [D65](04-effects.md#d65)),
**не compile error**.

| Эффект | Категория |
|---|---|
| `Mem` | instrumental, ambient |
| `Trace` | instrumental, ambient |

**Зачем разделять:**

1. **Сигнатуры остаются чистыми.** Если бы `Trace` был semantic, то
   почти **каждая** функция бы содержала его — observability обычно
   pervasive. Шум в типах.
2. **AI-friendly.** LLM не должна писать `Mem` в сигнатуре —
   instrumental detail имплементации.
3. **Интуитивно.** `Time` в сигнатуре говорит "функция зависит от
   времени, тестируй с fixed clock". `Trace` в сигнатуре ничего
   полезного не говорит.

#### Не существуют как эффекты

| Имя | Почему |
|---|---|
| `Async` | runtime mechanic (suspension, [D14 (REVISED)](06-concurrency.md#d14)) |
| `Par` | runtime mechanic (parallelism через `parallel for`) |
| `Mut` | удалён ([D62](04-effects.md#d62)) — `mut` поля/параметры |

**Базовые функции:**

```nova
fn print(...items []any) Io -> ()           // variadic, см. D69
fn println(...items []any) Io -> ()         // variadic + newline
fn panic(msg str) -> never                  // смерть текущего fiber'а (D13)
fn exit(code int, msg str) -> never         // смерть всего процесса (D13)

// Assertions — обычные fn-call, обязательно со скобками
fn assert(cond bool) -> ()                  // always runtime; failure → panic (D13)
fn debug_assert(cond bool) -> ()            // debug-only; no-op в release (D81)
```

`print`/`println` — **variadic** ([D69](03-syntax.md#d69)),
принимают любое число аргументов любого типа (`any` —
[D54](03-syntax.md#d54)). Каждый аргумент конвертируется в строку
через `str.from(v)` ([D73](#d73-from--into-protocol-пара-с-авто-выводом)).
Spread разрешён: `print(...parts)`.

`assert`/`debug_assert` — **обычные функции, не keyword'ы**. Вызываются
со скобками как любой fn-call: `assert(x > 0)`. Build-mode семантика —
[D81](#d81). Failure любого assert'а — panic ([D13](#d13)), не Fail.

#### `never` — bottom-тип (uninhabited)

`never` — **bottom-тип** языка: строчный встроенный примитив, в одном
ряду с `int`/`bool`/`f64`. **Не объявляется** ни в prelude, ни через
`type` — компилятор знает его напрямую (как и остальные примитивы).
Имя строчное по конвенции примитивов (Plan 76).

**Свойства:**

- **Uninhabited** — значений типа `never` не существует (0 значений).
- **`never` — подтип любого типа** (bottom type ⊥). Любой контекст,
  ожидающий `T`, может принять `never`-выражение.
- **Используется в типах не-возвращающих выражений** — `throw expr`,
  `return expr`, `panic(...)`, `exit(...)`, бесконечный `loop`. Все
  имеют тип `never`, поэтому совместимы с любым контекстом.

Аналоги: Rust `!` (`never`-RFC), Haskell `Void`, Kotlin/Scala
`Nothing`, TypeScript `never`. Не уникальная фича Nova.

#### Эффекты как обычные типы — `Fail[E]` не магия

`Fail[E]` объявляется в prelude как любой другой эффект — через
kind-токен `effect` ([04-effects.md → D18 (REVISED)](04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type),
[D61](04-effects.md#d61)):

```nova
type Fail[E] effect {
    fail(value E) -> never
}
```

`throw expr` — сахар для `Fail[E].fail(expr)` (вызов операции
активного handler'а), как `Db.query(...)`. Никакой специальной
обработки. См. [04-effects.md → D25](04-effects.md#d25),
[04-effects.md → D61](04-effects.md#d61).

#### Что НЕ в prelude

Коллекции (`String`, `HashMap`, `HashSet`, `LinkedList`), I/O API (`File`, `Http`),
JSON, SQL, время как библиотека — **обычные модули**, требующие
явного импорта:

```nova
import std.io.{File, read_all}
import std.collections.HashMap
```

### Почему

#### Зачем нужен prelude

Без prelude каждый файл начинается с:

```nova
import std.option.{Option, Some, None}
import std.result.{Result, Ok, Err}
```

Это шум на 90% файлов. Прецедент — Rust, Haskell, Swift, Kotlin: все
имеют prelude. AI-first: LLM не должен генерировать boilerplate-импорты
базовых типов.

#### Не противоречит «локальности контекста»

Prelude **документирован**, его содержимое — фиксированный список,
не магия. LLM знает, что доступно везде. Всё остальное — явный импорт
([07-modules.md → D29](07-modules.md#d29)).

### Что отвергнуто

- **Никакого prelude, всё через явный import** — шум, не выигрыш.
- **Prelude определяется компилятором, без документации** — магия,
  ломает AI-first тезис.
- **Prelude настраивается per-project** — усложнение без выгоды; LLM
  должен знать фиксированный набор.
- **`Void`** — отвергнут, тип «без значения» это `()` (unit). См.
  [03-syntax.md → D20](03-syntax.md#d20).

### Связь

- [01-philosophy.md → D10](01-philosophy.md#d10) — AI-first,
  локальность через документированный prelude.
- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Fail[Error]`.
- [04-effects.md → D18](04-effects.md#d18) — эффекты как обычные типы.
- [02-types.md → D17](02-types.md#d17) — sum-type, `never` как пустой.
- [03-syntax.md → D20](03-syntax.md#d20) — `()` вместо `void`.
- [07-modules.md → D29](07-modules.md#d29) — prelude и явные импорты.

### Открытые вопросы

- ~~Полный API `Option`/`Result`~~ — **частично закрыт (2026-05-07):**
  базовые методы (`is_some`/`is_none`/`unwrap`/`unwrap_or`/`unwrap_or_else`/
  `map`/`ok_or`/`or` для Option; `is_ok`/`is_err`/`ok`/`err`/`unwrap`/
  `unwrap_or`/`unwrap_or_else`/`map`/`map_err` для Result) описаны в
  prelude выше. Расширенный API (`and_then`, `flatten`, etc.) —
  отдельная задача (Q-monadic-api).
- ~~Семантика `?` для `Option`~~ — закрыто
  [D67](04-effects.md#d67): ранний `return None` из текущей функции.
- `Error` как универсальный тип — что в нём (поддержка `str.from(e)`,
  цепочка причин)? Похоже на Rust `std::error::Error`.

### Цена

1. **Список prelude нужно поддерживать.** Любое добавление в prelude —
   breaking change после v1.0 (имя становится «зарезервированным» в
   модулях). Поэтому prelude **минимален**.
2. **Импорт-конфликты.** Если программист объявит свой `type Option`,
   будет конфликт с prelude — компилятор предупредит.

### Runtime stdlib проекция (Plan 13)

Все методы str / f64 / f32 которые знает компилятор объявлены в
[`std/runtime/string.nv`](../../std/runtime/string.nv) и
[`std/runtime/math.nv`](../../std/runtime/math.nv) — **auto-generated**
из `compiler-codegen/src/codegen/runtime_registry.rs` через команду
`nova-codegen emit-runtime-stubs`.

**Эти модули НЕ требуют import** — методы доступны через обычный
method-call синтаксис (`s.find`, `x.sin`), потому что str / f64 /
f32 — built-in типы из prelude. `std/runtime/*.nv` — read-only artefact
для:

1. **Code-review:** разработчик видит формальные сигнатуры всех
   runtime-функций в одном месте.
2. **Type-check без полной компиляции:** `nova-codegen check` загружает
   декларации и валидирует user-код против них.
3. **Single source of truth:** runtime_registry.rs (Rust) — driver,
   `.nv`-файлы — проекция. Изменение реестра → регенерация → diff
   видно в `.nv`.

**Manual edits запрещены** — pre-commit/CI guard через
`emit-runtime-stubs --check` (Plan 13 Ф.6).

См. [docs/plans/13-runtime-stdlib-and-autogen.md](../../docs/plans/13-runtime-stdlib-and-autogen.md).

### GC introspection — `std.runtime.gc` (Plan 32)

Namespace `gc.*` доступен для runtime-инспекции и явного управления GC:

```nova
ro h = gc.heap_size()       // bytes; 0 если backend без introspection
ro n = gc.live_count()      // приблизительное число live-объектов
ro a = gc.alloc_count()     // монотонный счётчик с старта
gc.collect()                 // принудительный сбор (no-op под malloc)
gc.reset_stats()             // сброс счётчиков
```

**Без import** — `gc` — встроенный namespace (как `panic` / `exit`).
Документация в [`std/runtime/gc.nv`](../../std/runtime/gc.nv); фактический
dispatch — hard-coded в `compiler-codegen/src/codegen/emit_c.rs` (special-
case для `gc.<method>()` member-call'ов).

**Semantics per backend:**

| API | malloc | boehm |
|---|---|---|
| `heap_size()` | 0 (honest «не поддерживается») | `GC_get_heap_size()` |
| `live_count()` | `alloc - free` | `alloc_count` (upper bound) |
| `alloc_count()` | counter | counter |
| `collect()` | no-op | `GC_gcollect()` |
| `reset_stats()` | zero counters | zero counters |

`heap_size() == 0` — honest sentinel; differential-тесты могут
использовать `if gc.heap_size() == 0 { ... skip ... }`.

**Прецеденты:** Go `runtime.GC()` / `runtime.ReadMemStats`, Java
`System.gc()` / `Runtime.totalMemory()`, Python `gc.collect()` /
`gc.get_stats()`, .NET `GC.Collect()` / `GC.GetTotalMemory()`. Nova
следует convention.

См. [docs/plans/32-gc-introspection.md](../../docs/plans/32-gc-introspection.md).

---

## D41. Static-функции есть, static-состояния нет

### Что
У типа есть **static-функции** (`fn Type.name(...)`), но **нет
static-полей**, **нет static-переменных**, **нет static initializer'ов**.
Если нужны константы, ассоциированные с типом, — это `const` в том же
модуле. Если нужно «глобальное» изменяемое состояние — это **handler**
(эффект-capability), не static.

### Правило

#### Static-функции — обычные функции в namespace типа

Внутри одной static-функции другие static-функции того же типа
вызываются **через полное имя**, без сокращений:

```nova
fn Account.new(owner str) -> Account =>
    Account { _balance: 0, owner }

fn Account.from_balance(owner str, initial money) -> Account {
    ro acc = Account.new(owner)             // явное Account.new, не self.new
    Account.deposit_static(acc, initial)     // тоже явно
    acc
}
```

Никакого `Self::new` (Rust) или просто `new` (Java/C#). Один способ
вызова static-функции — через имя типа, что внутри типа, что снаружи.

#### Константы рядом с типом — `const` в модуле

```nova
const ACCOUNT_MIN_BALANCE money = 0
const ACCOUNT_MAX_OVERDRAFT money = 1000

fn Account.new(owner str) -> Account =>
    Account { _balance: ACCOUNT_MIN_BALANCE, owner }
```

Если нужна группировка — отдельный модуль:

```nova
module account_limits

export const MIN_BALANCE money = 0
export const MAX_OVERDRAFT money = 1000

// использование:
import account_limits
ro acc = Account.new_with(account_limits.MIN_BALANCE)
```

#### Глобальное изменяемое — через handler

Вместо static counter / static config — handler, передаваемый через
`with`-блок:

```nova
// Эффект ([04-effects.md → D61](04-effects.md#d61))
type IdGen effect {
    fresh() -> u64
}

// Handler — обычная функция, возвращающая handler-литерал
fn counter_id_gen(c mut Counter) -> Effect[IdGen] =>
    effect IdGen {
        fresh() {
            c.count += 1
            c.count
        }
    }

// в main:
fn main() {
    mut counter = Counter { count: 0 }
    with IdGen = counter_id_gen(counter) {
        run_app()
    }
}
```

> Это пример **closure-capture** паттерна по [D68](04-effects.md#d68).
> Альтернатива — `@as_handler` метод на record'е `Counter` —
> рассмотрена в D68 для случаев, когда state нужно проинспектировать
> снаружи. Выбор между паттернами детерминирован сценарием
> (нужен ли state наружу), не вкусом.

Тестируется тривиально — другой handler в `with`-блоке.

### Почему

- **Static state — главный источник скрытых багов.** Глобальный
  изменяемый стейт не виден в сигнатурах, ломает параллельность,
  невозможно тестировать без хаков.
- **Тесты.** Static-поле = разделяемое состояние между тестами.
  Каждый тест должен либо ресетить его (хрупко), либо запускаться
  изолированно (медленно). Handler — `with`-блок изолирует
  автоматически.
- **Параллелизм.** Несколько fiber'ов на одном static-поле = data race
  по умолчанию. Handler-state живёт в scope и не делится случайно.
- **DI is the language.** Передача зависимостей — это handler. Не
  нужен отдельный фреймворк для DI, не нужны static-singleton'ы как
  замена.
- **Единственный путь.** Нет «иногда static, иногда handler» —
  всегда handler. Меньше способов сделать неправильно.

### Что отвергнуто

- **Static mutable поля** (Java `static int counter`, Python class
  variable) — мешают тестам и параллелизму.
- **Static immutable поля как `const`** на типе (`const Account.MIN`)
  — технически безопасно, но добавляет второй способ объявить
  константу. Один способ — `const` в модуле.
- **Companion-object** (Kotlin) — то же что и static, просто в
  обёртке. Не нужен.
- **Lazy static** (Rust `lazy_static!`) — скрытое глобальное состояние
  с инициализацией. Если нужна ленивость — handler с lazy полем.

### Связь

- [05-memory.md → D6](05-memory.md#d6) — глобального mutable state не
  предусмотрено в модели памяти; всё живёт в fiber-scope или
  handler-scope.
- [04-effects.md → D11](04-effects.md#d11),
  [04-effects.md → D31](04-effects.md#d31) — handler-механизм для
  «глобальных» состояний.
- [04-effects.md → D18](04-effects.md#d18) — эффекты это обычные `type`,
  не keyword `effect`.
- [03-syntax.md → D33](03-syntax.md#d33) — `const` — единственный
  способ объявить immutable «глобальную» константу.

### Цена

1. **Привычка из Java/C#/Python ломается.** Нет `Account.MAX_BALANCE`
   как поля, есть `MAX_BALANCE` как `const` в модуле. Чуть длиннее,
   но единообразнее.
2. **Singleton'ы переписываются как handler.** Это не цена, а фича —
   но мигрирующий код придётся переделать.
3. **Counter / cache / pool** требуют явного создания и проброса в
   `with`-блок. Не «само работает», а явный жизненный цикл.

### Эволюция

В исходной формулировке D41 пример использовал устаревшие keyword'ы
`effect IdGen { ... }` и `handler counter_id_gen(...) IdGen { ... }` —
оба отменены ([04-effects.md → D18](04-effects.md#d18) — эффект это
обычный `type`; слово `handler` не зарезервировано).
В текущем тексте пример переписан как `type IdGen { ... }` +
обычная функция, возвращающая handler-литерал.

---

## D70. `ToStr` protocol — REPLACED → D73

> ⚠️ **REPLACED → [D73](#d73-from--into-protocol-пара-с-авто-выводом)
> (2026-05-06).** Полное содержание D70 (ToStr protocol, @to_str() метод,
> free function to_str(v), auto-derive по структуре) удалено для устранения
> дублирования. Историческая запись об эволюции — в
> [decisions/history/evolution.md](history/evolution.md) →
> «`ToStr` protocol: D70 формализует to_str()».

### Migration map (D70 → D73)

| Старая форма (D70) | Новая форма (D73) |
|---|---|
| `type ToStr protocol { to_str() -> str }` | удалено — protocol больше не нужен |
| `fn UserId @to_str() -> str => ...` | `fn str.from(u UserId) -> Self => ...` |
| `to_str(user)` | `str.from(user)` |
| `user.@to_str()` | `user.into()` (Into[str] авто-выведен из From) |
| `"${user}"` (через to_str) | `"${user}"` (через str.from, без изменения синтаксиса) |
| `fn f[T: ToStr](v T)` | `fn f[T Into[str]](v T)` (если bound нужен) |

**Auto-derive для встроенных типов и record/sum** перенесён из D70 на
`str.from`: stdlib pre-registers `str.from(int)`, `str.from(bool)`,
`str.from(f64)`, `str.from(<any record>)`, `str.from(<any sum>)`. Newtype
без override делегирует к underlying-типу.

**Почему замена:** D70 + D73 решали одну задачу разными способами.
Конверсия в `str` — частный случай конверсии в любой тип. Принцип
«один очевидный путь» (D9) требует единого механизма. См. также D40
(philosophy «один способ»).

<!-- BEGIN: legacy D70 body REMOVED 2026-05-09 — see history/evolution.md -->
<!-- Удалены устаревшие примеры: type ToStr protocol declaration, builtin
     auto-derive table, override examples, evolution prose. Migration map
     выше + ссылка на evolution.md покрывают всю нужную информацию. -->
<!-- END: legacy D70 body REMOVED -->

---

## D73. `From` / `Into` protocol-пара с авто-выводом

> **Уточнение (2026-05-07):** `from`/`into` могут декларировать
> `Fail[E]` если конверсия fallible. Это **унифицирует** infallible и
> fallible конверсии под одной формой `from`/`into` — нет нужды в
> отдельном `try_from`/`try_into` (D77 теперь convenience-sugar,
> см. там).

### Что
Универсальный механизм нетривиальной конверсии значения между типами:

1. **`From[T]`** — protocol со static-методом `from(v T) -> Self`.
   «Целевой тип знает, как сделать себя из источника».
2. **`Into[T]`** — protocol с instance-методом `@into() -> T`.
   «Источник знает, как превратиться в целевой».
3. **Авто-вывод одного из другого** — компилятор знает про симметрию.
   Если задан только `From[X]` для типа `T`, компилятор автоматически
   удовлетворяет `Into[T]` для `X` (и наоборот). Программист пишет
   **одну** реализацию из пары.
4. **Fallible конверсии** объявляются эффектом `Fail[E]` в сигнатуре —
   та же `from`/`into` форма; effect-aware auto-derive переносит
   эффект на парную форму.

Программисту доступны **две формы вызова** из одной реализации:

```nova
T.from(v X)             // static, на целевом типе
v.into()               // instance, на источнике (тип цели — из контекста)
```

Для fallible (с `Fail[E]`) семантика та же; ошибка распространяется
через стандартный effect-механизм — `with Fail = handler { ... }` /
`?` оператор / propagation наружу.

В отличие от `as` (D54) — compile-time numeric/newtype/sum cast без
runtime-кода, — `From`/`Into` для **семантически нетривиальных**
конверсий (парсинг, единицы измерения, формат-обмен, представление
в строку — последнее заменяет old D70 `ToStr`).

### Правило

#### Декларация protocol'ов в prelude

```nova
type From[T] protocol {
    from(v T) -> Self           // static, на целевом типе
}

type Into[T] protocol {
    @into() -> T                 // instance, на источнике
}
```

`Self` (D66) — тип, реализующий protocol. `From.from` — static-метод,
вызывается через точку (D35): `Fahrenheit.from(celsius)`. `Into.@into`
— instance-метод, через `@`-нотацию: `c.into()`.

**Программист пишет одну сторону пары** — компилятор автоматически
выводит другую. Подробности — секция «`Into[T]` protocol и
автоматический вывод» ниже.

#### Реализация на пользовательском типе

Программист пишет обычный static-метод (D35):

```nova
type Celsius f64
type Fahrenheit f64

fn Fahrenheit.from(c Celsius) -> Self =>
    Self((c as f64) * 9.0 / 5.0 + 32.0)

ro f = Fahrenheit.from(Celsius(100.0))   // Fahrenheit(212.0)
```

Структурно `Fahrenheit` теперь удовлетворяет `From[Celsius]` (D53 +
D72) — никаких явных `impl` блоков.

**Несколько `From[X]` на одном типе** через overloading по
параметру ([D84](10-overloading.md#d84)):

```nova
fn Fahrenheit.from(c Celsius) -> Self => ...
fn Fahrenheit.from(k Kelvin) -> Self => ...

ro f1 = Fahrenheit.from(Celsius(100.0))
ro f2 = Fahrenheit.from(Kelvin(373.15))
```

#### Generic-функции с `From`-bound

```nova
fn parse_typed[U From[str]](s str) -> U => U.from(s)

ro n int = parse_typed("42")     // если int реализует From[str]
```

Bound `[U From[X]]` в generic-сигнатуре требует чтобы конкретный
тип `U` реализовывал `From[X]` — структурно, через D72 bound check.

#### Fallible конверсии через `Fail[E]`

Если конверсия может **не получиться** (валидация, парсинг, проверка
диапазона), `from`/`into` декларируют `Fail[E]` в сигнатуре:

```nova
type Utf8Error | InvalidByte | UnexpectedEnd

fn str.from(b []u8) Fail[Utf8Error] -> Self {
    if !is_valid_utf8(b) {
        throw Utf8Error.InvalidByte
    }
    // ...
}

// Caller-side — три варианта:

// (1) Propagate via Fail в сигнатуре caller'а:
fn parse_message(b []u8) Fail[Utf8Error] -> Message {
    ro s = str.from(b)              // ошибка пробрасывается
    parse_inner(s)
}

// (2) Catch handler'ом — Result-стиль через with-handler:
ro r Result[str, Utf8Error] =
    with Fail[Utf8Error] = |e| interrupt Err(e) {
        Ok(str.from(b))
    }

// (3) Default-fallback через with-handler:
ro s str = with Fail[Utf8Error] = |_| interrupt "[invalid utf-8]" {
    str.from(b)
}
```

**Effect-aware auto-derive:** если `T.from(v V) Fail[E] -> Self`,
компилятор авто-синтезирует `v.into() Fail[E] -> T`. Эффект
наследуется, видим в сигнатуре auto-derived формы.

#### Auto-derive 4-way (D73 + D77 unified)

**Программист пишет ОДНУ форму** из четырёх; компилятор синтезирует
остальные. Это объединяет D73 (`from`/`into`) и D77 (`try_from`/`try_into`)
в один механизм.

**Разделение «реализовать» vs «использовать»:**

| Природа конверсии | Программисту реализовать | Программисту использовать |
|---|---|---|
| **Fallible** | `T.try_from(v) -> Result[T, E]` | `T.from(v)` или `v.into()` (короче, throws Fail) |
| **Infallible** | `T.from(v) -> T` | `T.from(v)` или `v.into()` |

То есть **писать богатую форму** (`try_from` для fallible — Result-стиль
явный, error type first-class), а **использовать в обычном коде**
короткую (`from` / `into`).

**Compiler синтезирует все 4 формы из одной:**

| Программист написал | Compiler даёт |
|---|---|
| `try_from(v) -> Result[T, E]` (fallible) | `from() Fail[E]`, `into() Fail[E]`, `try_into() -> Result[T, E]` |
| `from(v) -> T` (infallible) | `into() -> T`. (try-формы НЕ синтезируются — не имеют смысла без error type.) |

**Почему `try_from` — самое богатое для имплементации:**
1. **Result в типе явный.** `Result[T, E]` показывает error type как
   first-class signature element — IDE / AI читают это сразу. Через
   `Fail[E]` нужен ещё шаг effect-rezolution.
2. **Compiler легко синтезирует throwing-форму** из Result — простое
   `match { Ok(v) => v, Err(e) => throw e }`. Обратное (Result из
   throwing) требует with-handler инфраструктуры.
3. **Boilerplate Ok(...) — это feature имплементации.** `Ok(value)`
   явно говорит «вот success-path», `Err(...)` — «вот failure-path».
   Программист читает контракт без неявных throw'ов в теле функции.

**Почему `from`/`into` — для использования в коде:**
1. **Короче** — `T.from(v)` против `T.try_from(v)?` или
   `T.try_from(v).unwrap()`.
2. **Идиоматичнее** — `v.into()` через context-driven dispatch
   читается как «преобразовать v к ожидаемому типу».
3. **Throws пропагируются естественно** — caller или handle через
   `with Fail`, или эффект уходит наружу. Программист не пишет
   `?`-цепочки руками.

**Когда использовать `try_from`/`try_into` в коде:**
- Когда нужен **explicit branching** на error type через `match`.
- Когда нужно **map error** в другой тип (`r.map_err(|e| MyError::Wrap(e))`).
- Когда нужен **default fallback** через `unwrap_or` без handler-блока.

В остальных случаях — `from`/`into` через эффекты.

**Прецедент Rust:** `TryFrom` каноническая форма для fallible
конверсий; сообщество выработало этот стиль.

**Алгоритм синтеза (программист пишет `try_from`):**

```nova
// Программист написал:
fn u64.try_from(s str) -> Result[Self, ParseIntError] => ...

// Компилятор синтезирует автоматически:
// (1) throwing-from через D73:
fn u64.from(s str) Fail[ParseIntError] -> Self =>
    match try_from(s) { Ok(n) => n, Err(e) => throw e }

// (2) instance try_into через D77:
fn str @try_into() -> Result[u64, ParseIntError] =>
    u64.try_from(@)

// (3) instance into через D73:
fn str @into() Fail[ParseIntError] -> u64 =>
    u64.from(@)

// Программист может вызвать любую из 4-х форм:
ro n = u64.try_from(s)?           // → Result, propagate с ?
ro n = u64.from(s)                // → throws Fail (caller handles)
ro n: u64 = s.try_into()?         // → instance Result
ro n: u64 = s.into()              // → instance throws
ro n = u64.try_from(s).unwrap_or(0)  // → fallback default
```

**Когда писать `from` вместо `try_from`:**
- Конверсия математически не может failure'ить: numeric upcast
  (`f64.from(int)`), unit ↔ unit (`Fahrenheit.from(Celsius)`),
  newtype unwrap (`int.from(UserId)`).
- Программист может сам убедиться что параметр валиден prerequisite'ом
  (например `from(s str)` где `s` уже валидирован выше) — но это
  опасно, лучше fallible форма.

**Тонкости:**
1. **Если программист пишет ОБЕ формы** (`from` без Fail и `try_from`
   с `Result[T, !]`) — compile-error: ambiguity, какая основная.
   Программист выбирает одну.
2. **Compiler не синтезирует try-формы из infallible `from()`** —
   нет error-type для Result. Если нужно (например, generic-bound
   требует `TryFrom`), программист пишет explicit
   `T.try_from(v) -> Result[T, never]` (never = uninhabited error).
3. **`Result[T, never]`** automatically converts to `T` через unwrap
   — never-type не имеет значений, `Err` ветка unreachable.

**Когда писать `Fail`, когда нет:**
- `Fahrenheit.from(c Celsius)` — без Fail (всегда успех).
- `int.from(s str) Fail[ParseIntError]` — с Fail (может не парситься).
- `Buffer.into() Fail[Utf8Error] -> str` — с Fail (валидация UTF-8).

Это **унифицирует** API: одна форма `from`/`into` для всех конверсий.
Не нужно решать «infallible или try_»; effect-аннотация в сигнатуре
сама описывает контракт. Согласовано с D2/D10/D25/D62/D65 («всё —
эффект», throw — операция Fail).

#### Соотношение с `as` (D54)

**`as` — compile-time, без runtime-кода:**

```nova
ro n = 100 as u32                 // numeric cast
ro u = 42 as UserId                // newtype ↔ underlying
ro code = NotFound as int          // sum → int
```

**`From` — нетривиальная конверсия с runtime-логикой:**

```nova
ro f = Fahrenheit.from(c)         // арифметика
ro u = User.from(json_value)      // парсинг
ro m = Money.from(("USD", 100))    // конструирование с валидацией
```

Граница чёткая: если конверсия выражается одним bit-level/tag-уровнем —
`as`. Если требует логики или может бросить — `from`.

#### Соотношение с D55 record-coercion

D55 — automatic coercion в позиции с известным целевым типом для
**record-литералов** и **sum-конструкторов**:

```nova
ro u User = { id: 2, name: "Bob" }     // D55: anonymous record → User
ro m Maybe[int] = 42                    // D55: 42 → Just(42)
```

D73 — **explicit** конверсия через method call для произвольных типов.
D55 срабатывает раньше на синтаксическом уровне; `From.from` — обычный
вызов. Не конфликтуют:

```nova
ro f Fahrenheit = Celsius(100.0)        // ОШИБКА: D55 не работает —
                                          // Fahrenheit не sum с unary Celsius
ro f = Fahrenheit.from(Celsius(100.0))  // ok: D73
ro f = into[Fahrenheit](Celsius(100.0)) // ok: через free function
```

#### `Into[T]` protocol и автоматический вывод

`Into[T]` — protocol с instance-методом, симметричный к `From[T]`:

```nova
type From[T] protocol {
    from(v T) -> Self          // static — на целевом типе
}

type Into[T] protocol {
    @into() -> T                // instance — на источнике
}
```

**Компилятор знает про симметрию `From`/`Into` и выводит одно из
другого автоматически.** Программист пишет **одну** реализацию из
пары, вторая выводится без блан­ket-impl и orphan-rule:

```nova
// Программист пишет From — Into выводится автоматически.
type Celsius f64
type Fahrenheit f64

fn Fahrenheit.from(c Celsius) -> Self =>
    Self((c as f64) * 9.0 / 5.0 + 32.0)

// Компилятор автоматически синтезирует:
//   fn Celsius @into() -> Fahrenheit => Fahrenheit.from(@)
// → Celsius структурно удовлетворяет Into[Fahrenheit].

ro f1 = Fahrenheit.from(Celsius(100.0))    // явная from-форма
ro f2 = Celsius(100.0).into()              // авто-выведенная into-форма
ro f3 = into[Fahrenheit](Celsius(100.0))   // free function
ro f4 Fahrenheit = into(Celsius(100.0))    // через context (D55)
```

Симметрично, если программист пишет `@into`, компилятор синтезирует
`from`:

```nova
// Программист пишет Into — From выводится автоматически.
type Json record { ... }
type User { id u64, name str }

fn Json @into() -> User =>
    User { id: @get_u64("id"), name: @get_str("name") }

// Компилятор автоматически синтезирует:
//   fn User.from(v Json) -> Self => v.into()
// → User структурно удовлетворяет From[Json].

ro u1 = json.into()                        // явная into-форма
ro u2 = User.from(json)                     // авто-выведенная from-форма
```

**Если написаны обе** — обе используются как написаны, авто-вывод
не применяется. **Несовпадение результатов** между руками
написанными `from` и `into` — ответственность программиста (типичный
лит-чек предупреждает, но не запрещает: бывают legitimate случаи
типа explicit-from-bytes vs implicit-into-bytes).

**Запрет циклов авто-вывода.** Авто-вывод одноуровневый: из `From[X]`
для `T` синтезируется `Into[T]` для `X`. Не наоборот в той же
итерации (это создало бы цикл). Это значит:

- Программист пишет `From[X]` или `Into[X]` — оба триггерят авто-вывод парного.
- Компилятор не пытается «найти transitively From[Y] через From[X] и From[X→Y]».

Если нужна транзитивность (`A → B → C` через две промежуточные
конверсии) — программист пишет explicit:

```nova
fn C.from(a A) -> Self =>
    ro b = B.from(a)
    Self.from(b)
```

#### Две формы вызова

Конверсия доступна в **двух формах**, обе из одной реализации:

```nova
Fahrenheit.from(Celsius(100.0))       // 1. static method (From[T] protocol)
Celsius(100.0).into()                // 2. instance method (Into[T] protocol)
```

Обе формы эквивалентны. Выбирай по читаемости:

- **`T.from(v)`** — целевой тип выделен в начале, читается как
  «build a Fahrenheit from this Celsius». Хорош в выражениях,
  где тип цели — главная информация.
- **`v.into()`** — короче в method-chains: `c.into().log()`.
  Тип цели берётся из контекста (`let s str = v.into()`,
  параметр функции, return-type). Без context — компилятор
  попросит указать тип цели через аннотацию.

Free function `into[T, U From[T]](v T) -> U` **не вводится** —
третья форма создавала бы лишний выбор для программиста и LLM
(нарушение D9 «один очевидный путь»). Static `T.from` уже
покрывает explicit-type case, instance `.into()` — context-driven.

#### Throwing-варианты

`From.from` может throw'ить через `Fail[E]`:

```nova
type ParseError | InvalidFormat | OutOfRange

fn UserId.from(s str) Fail[ParseError] -> Self =>
    match parse_int(s) {
        Some(n) if n >= 0 => Self(n as u64)
        Some(_)            => throw OutOfRange
        None               => throw InvalidFormat
    }

ro id UserId = UserId.from("42")        // throws Fail[ParseError]
```

Это обычная сигнатура с эффектом, никаких специальных правил.
`?` после такого вызова — нарушение D67 (`from` возвращает T через
Fail, не Result/Option):

```nova
ro id = UserId.from(s)?       // ОШИБКА D67
ro id = UserId.from(s)         // ok, throw сам пробрасывается
```

### Почему

1. **Нетривиальные конверсии — частая нужда.** Единицы измерения
   (`Celsius` ↔ `Fahrenheit`), парсинг (`str` → `UserId`), формат-обмен
   (`Json` → `User`). Без `From` каждый тип придумывает своё имя
   (`Celsius.to_fahrenheit`, `User.parse_json`). Единый protocol даёт
   общий контракт.

2. **Замещает старый `ToStr` (D70 REPLACED → D73).** D70 использовал ту же форму
   (protocol с одним методом + free function в prelude), но только для
   конверсии в `str`. D73 обобщает паттерн на любые конверсии: `From` +
   `into`. Конверсия в `str` — частный случай D73, не отдельный механизм.

3. **`Self` универсален (D66).** `Self` в protocol-методе делает
   объявление коротким — не нужно повторять имя типа. До D66 `From[T]`
   потребовал бы typeclass-механизм; с D66 это обычный protocol.

4. **Bounds (D72) разблокируют generic-функции.** `fn parse[U From[str]]`
   до D72 было невозможно. Теперь — естественно.

5. **Прецедент Rust.** `From`/`Into` — самый используемый паттерн в
   Rust ecosystem. Nova берёт идею (явные конверсии через protocol),
   адаптирует под свою систему (структурная типизация, без orphan
   rule, free function вместо blanket-impl).

6. **AI-friendly.** LLM генерирует `Fahrenheit.from(celsius)` без
   обдумывания имени метода. Структурный bound `[U From[T]]`
   проверяется compile-time с понятной ошибкой («`Bar` не реализует
   `From[Foo]`: missing static method `from(v Foo)`»).

### Что отвергнуто

- **Free function `into[T, U From[T]](v T) -> U`.** Раньше была
  предложена как третья форма вызова (`into[Target](value)`).
  Отвергнута: дублирует `T.from(v)` (ровно та же ширина и информация),
  создаёт три формы для одной операции — нарушение D9. `T.from`
  для explicit-type, `v.into()` для context-driven — этих двух
  достаточно.
- **Только `From[T]` без `Into[T]`** (как было в первой редакции D73).
  Без `Into` method-form `c.into()` была недоступна. Теперь
  `Into[T]` — first-class protocol; method-form работает; компилятор
  выводит парность из `From[T]` автоматически.
- **Blanket-impl типа Rust `T: From<U> ⇒ U: Into<T>`.** В Nova нет
  orphan rule и нет `impl` блоков (D42/D53), классический blanket-impl
  негде. **Решение Nova** — компилятор синтезирует парный protocol
  на уровне type-checker'а: если у типа есть `from`, считается что
  есть и `@into` (и наоборот). Это сохраняет преимущество Rust
  (одна реализация → две формы вызова) без orphan-механики.
- **`From` как trait с default-методами.** Без `impl` блоков и orphan
  rule концептуально неприменимо. Авто-синтез symmetric'а заменяет.
- **Implicit conversion в позиции аргумента** (Scala 3 `Conversion`,
  C++ implicit constructors). Nova: все конверсии явные (`as`, `from`,
  D55 — но D55 only для sum/record-литералов, без method call).
- **`@from(v T) -> Self` instance-метод вместо static.** `from` это
  фабрика — у неё нет существующего инстанса для `@`. По D35
  `fn Type.method` для конструкторов / static, что соответствует
  семантике.
- **`as` для нетривиальных конверсий** (`celsius as Fahrenheit`).
  D54 явно ограничивает `as` — compile-time numeric/newtype/sum.
  Расширять — теряется граница между cheap-cast и expensive-conversion.
- **Отдельный `ToStr` protocol для конверсии в строку (старая D70).**
  Конверсия в `str` — частный случай `From[X]`-механизма. Иметь два
  механизма для одной задачи нарушает D9. См. D70 v3 «REPLACED → D73»
  про переход.

### Цена

1. **Без context требуется явный целевой тип.** `v.into()` на
   bare-line-position не компилируется — нужно либо `let x T = v.into()`,
   либо `T.from(v)` с явным типом-prefix'ом.
2. **Multiple `From[X]` через overloading по типу параметра**
   ([D84](10-overloading.md#d84)) — четыре оси перегрузки и правила
   ambiguity описаны в D84.
3. **`From` от типа из чужого модуля.** Без orphan rule — добавляешь
   `fn MyType.from(v ForeignType)` где угодно, **но** реализация
   живёт в модуле, владеющем `MyType` (по D47 visibility). Если ни
   один из типов не «твой» — добавить `From` нельзя без обёртки
   (newtype). Это сознательное ограничение: предотвращает duplicate
   conflicting implementations.

### Связь

- [02-types.md → D53](02-types.md#d53) — protocol = тип, основа.
- [02-types.md → D66](02-types.md#d66) — `Self` в protocol.
- [02-types.md → D72](02-types.md#d72) — bounds для `[U From[T]]`.
- [03-syntax.md → D35](03-syntax.md#d35) — static / instance методы;
  receiver — любой тип, включая примитивы (`fn str.from(...)`).
- [03-syntax.md → D54](03-syntax.md#d54) — `as` для тривиальных
  cast'ов; D73 покрывает остальное.
- [02-types.md → D55](02-types.md#d55) — record/sum coercion;
  D73 для остальных типов.
- [04-effects.md → D67](04-effects.md#d67) — `from` с throw через
  `Fail` следует общим правилам `?`.
- [08-runtime.md → D70](#d70-tostr-protocol--replaced--d73)
  — REPLACED → D73; конверсия в `str` это частный случай D73.
- [D26](#d26-базовая-stdlib-и-prelude) — `From`, `Into` в prelude.

### Открытые вопросы

- **`From` для базовых типов.** Stdlib pre-registers `str.from(int)`,
  `str.from(bool)`, `str.from(f64)` (D70-replacement). Должны ли
  `int.from(bool)`, `f64.from(int)` etc. — сейчас open вопрос
  Q-from-builtins.
- **`TryFrom`** — отдельный protocol для **fallible** конверсий
  с явным `Result`/`Fail` в сигнатуре? Сейчас обычный `from` с
  `Fail[E]` достаточен. Q-tryfrom.
- **Auto-derive `From`** — для newtype можно автоматически (`type
  UserId u64` ⇒ `UserId.from(n u64) -> Self`)? Сейчас программист
  пишет вручную. Q-auto-from.
- **`From`-цепочки.** Если `B: From[A]` и `C: From[B]`, можно ли
  одно вызовом перейти `A → C`? В Rust — нет (single-step). Nova —
  пока тоже нет, программист пишет `C.from(B.from(a))`. Q-from-chain.

### Эволюция

**v1 (первая редакция D73):** только `From[T]` protocol + free function
`into[T, U From[T]](v T) -> U`. `Into` отвергнут как «Rust-style
blanket-impl нет, не нужен отдельный protocol». Method-form
`value.into()` не работала.

**v2:** добавлен `Into[T]` protocol с instance-методом `@into() -> T`.
Компилятор автоматически синтезирует парный protocol — `T.from(v X)`
written → `X.into() -> T` synthesized (и наоборот). Три эквивалентные
формы вызова из одной реализации: `into[T](v)`, `v.into()`,
`T.from(v)`.

**v3 (текущая, 2026-05-06):** убрана free function `into[T, U](v)`.
Три формы — это нарушение D9. Остались две: `T.from(v)` (static,
explicit-type) и `v.into()` (instance, context-driven). Также:

- D70 `ToStr` помечен как REPLACED → D73 — конверсия в строку
  выражается через `str.from(v)` / `v.into()` (с context = str).
- D35 явно расширен: receiver-тип может быть примитивом
  (`fn str.from(int)`, `fn int @to_hex() -> str` и т.п.).

**Что было невозможно до этого:** D73 как механизм требует bound'ы
(D72). До D72 (Q-bounds открыт) `From`/`Into` пара была заблокирована.
С D72 разблокирована.

---

## D74. Математические операции на числовых типах — instance-методы

### Что
Стандартные математические функции (`sin`, `cos`, `sqrt`, `atan2`,
`hypot`, `abs`, `pow`, `floor`, `is_finite`, и др.) объявляются как
**instance-методы** через `@` на числовых типах (`f64`, `f32`, `int`,
i8-i64, u8-u64), а не как static `Math.fn(...)` или free function
`sin(x)`. Static-функции остаются только для **констант**
(`f64.PI`, `f64.NAN`) и **парсинга** (`f64.try_parse(s)`).

```nova
ro r = (x * x + y * y).sqrt()
ro phi = im.atan2(re)
ro dist = a.hypot(b)
ro s = (theta + offset).sin()
ro n = magnitude.abs()
```

### Правило

#### Полный набор на `f64` (prelude)

| Категория | Методы |
|---|---|
| Корни и степени | `@sqrt()`, `@cbrt()`, `@sqr()`, `@pow(exp f64)`, `@powi(n int)` |
| Тригонометрия | `@sin()`, `@cos()`, `@tan()`, `@asin()`, `@acos()`, `@atan()` |
| `atan2` (двух-арг) | `@atan2(other f64) -> f64` (`y.atan2(x)`) |
| Гиперболические | `@sinh()`, `@cosh()`, `@tanh()` |
| Экспонента / лог | `@exp()`, `@ln()`, `@log10()`, `@log2()`, `@log(base f64)` |
| Норма / расстояние | `@abs()`, `@hypot(other f64)` |
| Округление | `@floor()`, `@ceil()`, `@round()`, `@trunc()`, `@fract()` |
| Знак / минимум | `@signum()`, `@min(other f64)`, `@max(other f64)` |
| Предикаты | `@is_finite()`, `@is_nan()`, `@is_infinite()` |

Аналогичный набор на `int` (где математически осмысленно):
`@abs()`, `@pow(n int)`, `@signum()`, `@min(other)`, `@max(other)`,
`@is_negative()`, `@is_positive()`. Тригонометрия и логарифмы — только
на float-типах.

#### Static-функции на типе (не методы)

Для констант и операций без естественного receiver'а — обычные
static через точку (D35):

```nova
f64.PI                                    // константа π
f64.E                                     // константа e
f64.NAN                                   // тихий NaN
f64.INFINITY                              // +∞
f64.NEG_INFINITY                          // -∞
f64.MAX                                   // максимальное конечное
f64.MIN_POSITIVE                          // минимальное положительное
f64.EPSILON                               // машинная точность

f64.try_parse(s str) -> Option[f64]      // парсинг с возможной ошибкой
```

Парсинг через `f64.try_parse(s)` дополнен `From[str]` через D73 —
доступна обе формы:

```nova
ro x = f64.try_parse("3.14")            // Option[f64]
ro y f64 = f64.from("3.14")              // throws Fail[ParseError]
ro z f64 = "2.71".into()                 // через D73 авто-Into
```

#### Двух-аргументные функции

`atan2`, `hypot`, `min`, `max`, `pow`, `log` принимают два аргумента.
Receiver — первый по математической / физической конвенции:

```nova
y.atan2(x)        // arctangent of y/x — y первый
a.hypot(b)        // √(a² + b²) — симметрично, но a первый
base.log(other)   // log_base(other)
x.pow(n)          // x^n
```

Это даёт chain-style: `dy.atan2(dx).abs() < tolerance`.

#### Соответствующее имя `@sqr()`

`@sqr()` — квадрат (`x*x`). Имя из Pascal (`Sqr(x)`), короче
`squared`, согласовано с одноимённым методом на других типах
(например, `Complex @sqr()`). Для нецелых степеней — `@pow(2.0)`
или `@powi(2)`.

### Почему

1. **Согласовано с D35** ([03-syntax.md → D35](03-syntax.md#d35)).
   `@`-методы — основной механизм для type-bound функций. Числовые
   операции — type-bound по определению (зависят от типа: `i32.abs()`
   ≠ `f64.abs()` в реализации). Использовать static-стиль для одних
   операций и `@` для других — нарушение D40 «один способ».

2. **Chain-friendly формулы.** Длинные математические выражения
   читаются слева направо в «pipeline»-стиле:
   ```nova
   ro result = (a*a + b*b).sqrt().abs().min(MAX_VALUE)
   ```
   В static-стиле было бы:
   ```nova
   ro result = f64.min(f64.abs(f64.sqrt(a*a + b*b)), MAX_VALUE)
   ```
   Вложенность растёт справа налево, читать тяжелее.

3. **Прецедент Rust / Kotlin / Swift.** Все три используют instance-
   методы для математики (`(2.0_f64).sqrt()`, `theta.cos()`).
   Java/JS/Python со static-стилем (`Math.sin(x)`) — наследие старой
   эпохи без object-методов на примитивах.

4. **Free functions конфликтуют с user-кодом.** `sin(x)` как глобальная
   функция занимает имя `sin` — пользователь не может назвать так
   свою функцию без shadowing prelude. `@sin()` живёт в namespace
   типа, не глобально.

5. **AI-friendly.** LLM пишет `theta.cos()` без раздумий «math.cos
   или Math.cos или просто cos». Один паттерн — один способ
   вызова.

### Что отвергнуто

- **Static `Math.sin(x)`** (Java, JavaScript). Менее читаемо для
  длинных формул, не chain-friendly, и в Nova нет объекта-namespace
  `Math` (нет static-namespace объектов как в Java).
- **Free function `sin(x)`** (C, Python). Захватывает короткие имена
  в глобальном scope, конфликтует с пользовательскими функциями.
- **Trait-style `Float` protocol с `sin/cos/...`** (Haskell `Floating`,
  Rust `num_traits::Float`). Лишняя indirection, generics с bounds
  для каждой математической функции усложняют сигнатуры. В Nova
  `f64`/`f32` — отдельные типы, дублирование методов на оба
  допустимо (как в Rust).
- **Разные имена для разных размеров** (`sinf` для f32, `sin` для f64
  как в C). Перегрузка по типу receiver'а ([D84](10-overloading.md#d84))
  даёт одно имя, разные реализации — естественно для языка с типами.
- **`@squared()` вместо `@sqr()`.** Длиннее без выгоды; `sqr` имеет
  Pascal-прецедент и согласовано со стилем коротких имён в Nova
  (`@neg`, `@inv`, `@conj`, `@arg`, `@rem`, `@shl`).
- **Только static-функции для констант + instance для операций
  через `@`** (mixed). Принято: константы — static (`f64.PI` — у
  значения нет receiver'а), операции — `@`. Это два разных рода
  имён (decleration site), не конфликт.

### Цена

1. **Дублирование методов между f32/f64**, потенциально int.
   Реализация — обычно одна (через builtin / FFI к libm), но
   объявления повторяются. Это цена отсутствия Float-protocol;
   терпимо для prelude, который пишется один раз.

2. **`x.sqrt()` для `x < 0`** возвращает `NaN` (IEEE 754) — runtime-
   surprise. Strict-режим (`Fail[NaN]`) — отдельная функция
   `@try_sqrt()` если понадобится; в base — IEEE без проверок.

3. **Нет namespace `math`.** Если пользователь хочет
   `import math; math.sin(x)` — придётся писать `x.sin()`. Часть
   программистов из Python/Java будут удивлены поначалу.

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — prelude содержит математику
  как часть числовых типов; D74 уточняет форму объявления.
- [03-syntax.md → D35](03-syntax.md#d35) — `@`-методы как механизм.
- [03-syntax.md → D46](03-syntax.md#d46) — operator overloading
  (`@plus`, `@times`, ...) дополняет D74 для арифметики.
- [`std/runtime/math.nv`](../../std/runtime/math.nv) — auto-generated
  external-fn декларации всех f64/f32 math методов (Plan 13).
- [03-syntax.md → D40](03-syntax.md#d40) — «один способ» — выбор
  между static и instance не остаётся на усмотрение программиста.
- [D73](#d73-from--into-protocol-пара-с-авто-выводом) — парсинг
  чисел через `f64.from(s)` / `s.into()`, согласовано с from/into.
- [std/math/complex.nv](../../std/math/complex.nv) —
  использует instance-стиль (`theta.cos()`, `im.atan2(re)`,
  `a.hypot(b)`) как канонический пример.

### Эволюция

Изначально черновик `complex.nv` (2026-05) использовал static-стиль
`f64.cos(theta)`, `f64.atan2(im, re)` по аналогии с Java `Math.sin`.
При обсуждении выявлено что это противоречит D35 (методы — основной
механизм) и плохо читается для математических формул. Все вызовы
переписаны в instance-стиль, и паттерн зафиксирован формальным
D-решением D74.

`Math` namespace отвергнут (нет static-namespace в Nova, имя `Math`
конфликтовало бы с пользовательскими типами `Math` для предметных
областей).

---

## D77. `TryFrom` / `TryInto` — protocol-пара, расширение D73 для fallible-конверсий

> **Уточнение (2026-05-07):** D73 теперь сам поддерживает fallible
> через `Fail[E]` в сигнатуре `from`/`into` — единый механизм.
> Программист пишет **одну** из 4-х форм (`from` / `into` / `try_from` /
> `try_into`), компилятор синтезирует остальные. **Рекомендуется
> писать `try_from`** для fallible (Result-стиль явный, error type
> first-class в signature) и `from` для infallible (без boilerplate
> `Ok(...)`). Подробности в D73 «Auto-derive 4-way».
>
> Этот документ (D77) описывает Result-форму (`try_from` / `try_into`)
> как **рекомендуемую implementation form** для fallible конверсий
> (вопреки названию «convenience sugar» в раннем описании).

### Что
Парный механизм к [D73](#d73-from--into-protocol-пара-с-авто-выводом)
для **fallible-конверсий**: когда конверсия может не получиться,
программист может выбрать одну из двух эквивалентных форм:

1. **Throwing-форма** через `Fail[E]` — `T.from(v) Fail[E] -> Self`
   (D73, основная форма).
2. **Result-форма** — `T.try_from(v) -> Result[Self, E]` (D77,
   convenience sugar).

Семантически **эквивалентны** (одна задача — конверсия с возможной
ошибкой), различаются **формой возврата ошибки**. D73 forma — Nova-
канонический путь («всё — эффект», D2/D10), D77 — для error-aware
веток с explicit Result.

**Компилятор синтезирует одну из другой.** Программист пишет одну
сторону, другая выводится — точно так же как `From` ↔ `Into` в D73.

```nova
// Программист пишет — одну форму:
fn u64.try_from(s str) -> Result[Self, ParseIntError] => ...

// Компилятор автоматически даёт обе формы вызова:
ro n = u64.from("42")             // throws Fail[ParseIntError]
ro r = u64.try_from("42")          // Result[u64, ParseIntError]
ro opt = u64.try_from("42").ok()   // Option[u64] через Result.ok()
```

`Option`-вариант **не** требует отдельного метода — `Result.ok()`
из prelude превращает Result в Option. Один универсальный путь.

### Правило

#### Декларация protocol'ов в prelude

```nova
type TryFrom[T, E] protocol {
    try_from(v T) -> Result[Self, E]
}

type TryInto[T, E] protocol {
    @try_into() -> Result[T, E]
}
```

`Self` (D66) — реализующий тип. `try_from` — static-метод (как
обычный `from`), `try_into` — instance-метод.

#### Авто-синтез четырёхугольника

Если программист пишет любую **одну** форму из четырёх, компилятор
выводит остальные три:

```nova
       T.from(v X)              ← throws Fail[E]
       T.try_from(v X)          ← Result[Self, E]
       v.into() -> T            ← throws Fail[E]
       v.try_into() -> T        ← Result[T, E]
```

**Правила синтеза:**

1. **`from` → `try_from`:** оборачивает throw в Result.
   ```nova
   // Если написано:
   fn u64.from(s str) Fail[ParseIntError] -> Self => ...
   // Синтезируется:
   fn u64.try_from(s str) -> Result[Self, ParseIntError] =>
       with Fail[ParseIntError] = |e| interrupt Err(e) {
           Ok(Self.from(s))
       }
   ```

2. **`try_from` → `from`:** разворачивает Result в throw.
   ```nova
   // Если написано:
   fn u64.try_from(s str) -> Result[Self, ParseIntError] => ...
   // Синтезируется:
   fn u64.from(s str) Fail[ParseIntError] -> Self =>
       match Self.try_from(s) {
           Ok(v)  => v
           Err(e) => throw e
       }
   ```

3. **`from` ↔ `into` / `try_from` ↔ `try_into`:** через D73-механизм
   на каждой из форм отдельно. То есть если написано `u64.from(s)`,
   синтезируются:
   - `u64.try_from(s)` (D77)
   - `s.into()` для типа `u64` (D73)
   - `s.try_into()` для типа `u64` (D77)

**Если написаны обе** (например, `from` и `try_from` обе вручную) —
обе используются как написаны, авто-синтез не применяется. Как в D73,
программист отвечает за consistency.

#### Какую форму писать?

Рекомендация — **писать `try_from`**, для парсинга / валидации:

```nova
fn u64.try_from(s str) -> Result[Self, ParseIntError] =>
    if !is_all_digits(s) {
        Err(InvalidDigit { position: 0 })
    } else {
        // ... основная логика
        Ok(parsed_value)
    }
```

Причины:
- **Result-возврат явный** — программисту не нужно держать в голове
  активный handler `Fail[E]`.
- **Тип ошибки виден в сигнатуре** (`Result[Self, ParseIntError]`),
  а не пробрасывается через эффект-row (где может теряться).
- **Pattern matching** на Result удобен внутри парсера для composition.

`from` остаётся для случаев когда программист **уверен** в успехе и
не хочет писать `match`:

```nova
fn UserId.from(n u64) -> Self => Self(n)         // infallible
fn Greeting.from(name str) -> Self =>
    Self("Hello, ${name}!")                       // тоже infallible
```

Если конверсия **infallible** — `from` достаточно, `try_from` не
синтезируется (нет `E`).

#### Семантика равенства

`from(s)` и `try_from(s).unwrap()` — поведенческое равенство (с
учётом разной формы ошибки). Компилятор гарантирует:
- `try_from(v) == Ok(x)` ⇒ `from(v) == x`
- `try_from(v) == Err(e)` ⇒ `from(v)` бросает `throw e`

#### `D67` ?-оператор

- `let v = u64.try_from(s)?` — **валидно**, Result оборачивается
  через [D67](04-effects.md#d67) `?` на Result.
- `let v = u64.from(s)?` — **ошибка** (D67), `from` возвращает T
  через `Fail`, не Result. Throw сам пробрасывается без `?`.

```nova
// Функция возвращает Fail[ParseIntError]:
fn parse_pair(s str) Fail[ParseIntError] -> (u64, u64) {
    ro parts = s.split(",")
    ro a = u64.from(parts[0])              // throws через Fail (без ?)
    ro b = u64.from(parts[1])              // throws через Fail (без ?)
    (a, b)
}

// Функция возвращает Result, использует try_from + ?:
fn parse_pair_r(s str) -> Result[(u64, u64), ParseIntError] {
    ro parts = s.split(",")
    ro a = u64.try_from(parts[0])?         // ? на Result ([D85](04-effects.md#d85))
    ro b = u64.try_from(parts[1])?
    Ok((a, b))
}
```

#### Option через `Result.ok()`

Отдельный `try_parse` / `from_str_or_null` / similar **не вводится**.
Если нужен Option — `Result.ok()` в prelude:

```nova
fn Result[T, E] @ok() -> Option[T] => match @ {
    Ok(v)  => Some(v)
    Err(_) => None
}

// Использование:
ro opt = u64.try_from(s).ok()          // Option[u64]
match u64.try_from(s).ok() {
    Some(n) => n
    None    => default_value
}
```

Прецедент Rust: `s.parse::<u64>().ok()` → `Option<u64>`. Один
универсальный путь, не требует отдельного именования.

### Почему

1. **Согласовано с D73.** Тот же auto-pair-механизм. Программист
   видит ровно один паттерн «пишу одну сторону — компилятор даёт
   все формы вызова». Не нужно помнить «for fallible — другая система».

2. **Закрывает три формы вызова через одну реализацию.** Парсинг —
   частый use case. Без D77 программисту нужно либо:
   - Писать `try_X` отдельно (Kotlin-style `toIntOrNull`, размножение
     имён), или
   - Всегда `match { Some => ... None => throw }` обёртку.

3. **Стандартизованное имя `try_from`.** До D77 разные библиотеки
   могли использовать `try_parse`, `parse_or_err`, `validate`, и
   т.д. — каждая со своим именем. С D77 — единое имя как `from`
   стандартно для конверсии.

4. **Прецедент Rust:** `From` / `TryFrom` — стандарт `std`. Auto-blanket
   реализация (`Into ↔ From`) делается компилятором. Nova повторяет
   паттерн.

5. **Option получается бесплатно** через `Result.ok()`. Не нужны
   `_or_null`-suffix имена (Kotlin), `init?` (Swift), `*OrNull`
   (Java fluent helpers). Один Result — три формы (`from`, `try_from`,
   `try_from(...).ok()`).

6. **AI-friendly.** LLM пишет `Version.from(s)` и работает; пишет
   `Version.try_from(s)?` для propagation через Result — тоже
   работает. Не нужно помнить какая форма реализована — всегда обе
   доступны.

### Что отвергнуто

- **`u64.try_parse(s) -> Option[u64]`** — отдельный Option-вариант
  как метод. Конфликтует с принципом «один способ» (D9): `try_parse`
  vs `try_from(...).ok()` делают одно и то же. Result.ok() универсальнее.
- **`u64.parse(s)`** — отдельное имя для парсинга. Парсинг — это
  частный случай конверсии (`str → u64`), общий механизм через
  `from`/`try_from` лучше.
- **`OrNull`-suffix имена** (Kotlin): `toIntOrNull`. Размножение
  имён, не масштабируется (`fromOrNull`, `intoOrNull`, `parseOrNull`).
- **Java-style overloading throwing/non-throwing с одинаковым именем**
  (`int.parse(s) -> int` vs `int.parse(s) -> int` через флаг).
  Тип-ambiguity, нечитаемо.
- **Failable initializer как в Swift** (`init?`). Специальный
  синтаксис конструктора — лишняя категория. У Nova `from`/`try_from`
  обычные функции.

### Цена

1. **Расширение compiler-логики.** D73 уже синтезирует пару From/Into,
   D77 удваивает: from/try_from + into/try_into = 4 формы из одной
   написанной. Компилятор должен:
   - Распознать одну из четырёх форм
   - Сгенерировать остальные три
   - Применять одни и те же правила structural-conformance.
   Цена — реализация в type-checker'е, не run-time.

2. **Semantic equivalence требует доверия.** Компилятор гарантирует
   что `from(v)` и `try_from(v).unwrap()` поведенчески одинаковы.
   Если программист пишет **обе вручную** и они расходятся —
   ответственность программиста (как в D73).

3. **Ambiguity при нескольких `try_from`.** Если у `u64` есть
   `try_from(str)` и `try_from(f64)` (через overloading
   [D84](10-overloading.md#d84)) — `u64.try_from(x)` резолвится по
   типу аргумента. Стандартный overloading.

4. **`Self` в Result.** `Result[Self, E]` корректно по D66 (Self
   валиден в method-контексте). Generic-параметр `E` свободен —
   не привязан к Self.

### Связь

- [D73](#d73-from--into-protocol-пара-с-авто-выводом) — базовая
  пара From/Into, D77 расширяет на fallible-форму.
- [D67](04-effects.md#d67) — `?`-оператор; работает на Result
  (`try_from(s)?`), не работает на throwing `from`.
- [D72](02-types.md#d72) — bounds: `[U TryFrom[T, E]]` для
  generic-функций fallible-конверсии.
- [D26](#d26-базовая-stdlib-и-prelude) — `TryFrom`, `TryInto`,
  `Result`, `Option` в prelude. `Result.ok() -> Option[T]` — стандартный
  метод для перевода.
- [D30](03-syntax.md#d30) — конвенция имён ошибок
  (`Parse<TypeName>Error`); не меняется.
- [std/data/semver.nv](../../std/data/semver.nv) —
  использует `u64.try_parse` (legacy имя) — должно мигрировать на
  `u64.try_from` после принятия D77.

### Открытые вопросы

- **Auto-derive для newtype?** `type UserId u64` — должны ли
  автоматически быть `UserId.from(n u64)` и `UserId.try_from(s str)`?
  Сейчас — программист пишет вручную. Q-auto-from осталось открытым
  из D73, расширяется на D77.
- **`from` цепочки** (`A → B → C`) — ни D73, ни D77 не вводят
  транзитивность. Программист пишет `C.from(B.from(a))`. Q-from-chain.
- **`TryFrom` для одного и того же `T` с разными `E`?** Пример:
  `u64.try_from(s str) -> Result[Self, ParseIntError]` и
  `u64.try_from(s str) -> Result[Self, ValidateError]` — отличаются
  только `E`. По [D84](10-overloading.md#d84) ось 3 (overloading по
  типу результата) формально это поддерживает, но требует context
  для дисамбигуации (`let r Result[u64, ParseIntError] = u64.try_from(s)`).
  Если контекста нет — compile error «cannot resolve overload».
  Альтернатива на call-site без контекста — `enum`-объединение
  ошибок (`type AnyError | A | B`) или разные имена.
  Q-tryfrom-multi-error.

### Эволюция

До D77 в первой реализации `std/data/semver.nv` использовался
`u64.try_parse(s) -> Option[u64]` — отдельное имя для Option-варианта
парсинга. При обсуждении выявилось три проблемы:

1. **Ad-hoc имя** — каждая stdlib-либа могла использовать своё
   (`try_parse`, `parse_opt`, `from_str_or_null`).
2. **Дублирование с `from`** — `try_parse` это «`from` минус throw,
   плюс Option». Семантически избыточно.
3. **Прецедент Rust** — `TryFrom` парный к `From` решает ту же
   задачу унифицированно.

D77 формализует: **одно имя `try_from`** для Result-варианта, авто-
синтез четырёх форм вызова из одной реализации. Option получается
через `Result.ok()`. `try_parse` отвергается как избыточное.

Backward-compat: `try_parse` в существующих файлах (semver.nv) —
переименовывается на `try_from`. Общая семантика не меняется.

---

## D76. `Mem` эффект — runtime introspection для leak/growth тестов

> **Status:** active. **Реализовано** в bootstrap'е (2026-05-06).
> Тесты: `nova_tests/runtime/memory_growth.nv`.

### Что

Built-in эффект `Mem` даёт Nova-коду доступ к runtime-счётчикам
аллокаций. Цель — **regression detection**: тест запоминает
`Mem.alloc_count()` до и после горячего кода и assert'ит, что прирост
остался в разумном бюджете. Если codegen начнёт генерировать в N раз
больше аллокаций (баг типа "alloc-per-iter увеличился на порядок"),
тест поймает это сразу.

### Операции

```nova
Mem.alloc_count() -> int   // total nova_alloc since gc_init/reset
Mem.free_count()  -> int   // total frees (plain malloc backend → 0)
Mem.live()        -> int   // alloc_count - free_count
Mem.reset()       -> ()    // zero stats counters (for per-test isolation)
```

Числа — это **счётчики вызовов**, не байты. Этого достаточно для
поимки регрессий "1 alloc на итерацию стало 10".

### Семантика

- `Mem` pre-registered как built-in эффект (как `Time`, `Fail`).
  Compiler не требует `Mem` в сигнатуре функции — это ambient
  capability (D11 / D62-style).
- **Нет user-handler'а:** в отличие от `Time` и `Fail`, операции
  `Mem` не имеют vtable; они эмитируются прямо в `Nova_Mem_*`
  inline-функции, которые ходят к runtime-counters.
  *Причина:* эти операции должны быть **наблюдаемыми с очень
  низкими накладными расходами** — vtable добавляет лишний indirect
  call который сам бы изменил alloc-pattern. И смысла переопределять
  их нет (это не business effect — это runtime-факт).

### Реализация

- **`compiler-codegen/nova_rt/alloc.h`** — runtime-функции
  `nova_gc_alloc_count`, `nova_gc_free_count`, `nova_gc_live_count`,
  `nova_gc_reset_stats`. Доступны во всех allocator-backend'ах.
- **`compiler-codegen/nova_rt/alloc.c`** (Phase-0 plain malloc) —
  считает `nova_alloc` calls; `free_count` всегда 0 (`release`
  no-op). Достаточно для growth-rate тестов.
- **`compiler-codegen/nova_rt/effects.h`** — `Nova_Mem_*` inline-
  обёртки.
- **`compiler-codegen/src/codegen/emit_c.rs`** — `effect_schemas`
  pre-populated с `Mem` schema; standard effect-call dispatch
  работает (`Mem.live()` → `Nova_Mem_live()`).

### Bootstrap-ограничения

1. **Plain-malloc backend (default):** `free_count` всегда 0,
   `live` == `alloc_count`. Это значит leak-тесты могут только
   измерять **growth rate**, не "осталось ли что-то живое". Когда
   подключим Boehm GC (alloc_boehm.c) или RC (alloc_rc.c) —
   free_count станет осмысленным, тесты можно расширить.
2. **Нет per-allocation type info.** `alloc_count` — счётчик всех
   `nova_alloc` calls без разбивки по типам. Production-runtime
   возможно даст breakdown (records, arrays, fiber stacks).
3. **Не thread-safe** в multi-threaded backend'е (счётчики не
   atomic). На bootstrap single-threaded fiber-runtime это OK.

### Связь

- [D7](#d7-один-язык--три-режима-компиляции) — runtime modes;
  `Mem` доступен во всех режимах.
- [D11](04-effects.md#d11) — pre-registered effects pattern.
- [05-memory.md → D6](05-memory.md#d6) — managed-heap design;
  `Mem` — observability над ним.

### Что отвергнуто

- **Free function `mem_alloc_count()`** — нарушает D9 («одна
  идиома для одной задачи»). Effect-форма даёт ровно столько же
  выразительности и согласована с Time.
- **Bytes-tracking** в bootstrap — требует instrumentированного
  allocator (overhead). Counts достаточно для regression-detection.

---

## D81. `assert(cond)` vs `debug_assert(cond)` — build-mode семантика

### Что

Два уровня assertion'ов в prelude:

- **`assert(cond)`** — **always runtime**, проверяется во всех
  режимах сборки (debug/release/JIT/AOT). Failure → panic
  ([D13](#d13)).
- **`debug_assert(cond)`** — **debug-only**, в release-сборке
  полностью отбрасывается компилятором (zero cost).

Третий уровень — формальные контракты `requires`/`ensures`
([D24](09-tooling.md#d24)) — отдельный механизм, не путать.

### Правило

#### Декларация в prelude

```nova
// always runtime — production invariants
fn assert(cond bool) -> ()

// debug-only — hot-path / sanity checks
fn debug_assert(cond bool) -> ()
```

Сигнатуры идентичны на уровне типов; разница — в семантике релиза.
Обе — обычные prelude-функции (не keyword'ы), вызываются со скобками
как любой fn-call (см. также [syntax.md секция «Тестирование без
моков»](../syntax.md)).

#### Семантика по build-mode

| Form | Compile-time check | Debug runtime | Release runtime | Use-case |
|---|---|---|---|---|
| `assert(cond)` | нет | check | **check** | production invariants |
| `debug_assert(cond)` | нет | check | **no-op** | hot-path / sanity |
| `requires`/`ensures` (D24) | SMT где возможно | check rest | **no-op** | formal contracts |

#### Примеры использования

```nova
// Production invariant — всегда проверяется
fn divide(a int, b int) -> int {
    assert(b != 0)            // ВСЕГДА runtime, даже в release
    a / b
}

// Hot-path — release не платит за проверку
fn fast_lookup(arr []int, idx int) -> int {
    debug_assert(idx >= 0 && idx < arr.len())   // только в debug
    arr[idx]                                    // unchecked в release
}

// Формальный контракт — compile-time где возможно, runtime fallback
fn sqrt(x f64) -> f64
    requires x >= 0.0
    ensures result >= 0.0
=> ...
```

#### Build-mode mechanics в bootstrap

Bootstrap (D71) **не различает** debug/release — все три режима
([D7](#d7-один-язык--три-режима-компиляции)) одинаковы, всегда
checked. `debug_assert` в bootstrap'е — **синоним `assert`** (тот же
runtime check, готовность к production-семантике).

Production-runtime добавит:
- preprocessor-style `#ifdef NOVA_DEBUG` для C-backend, или
- codegen-флаг для no-op generation в release-сборке.

Build-mode влияет на **performance**, не на **семантику** программы:
`assert` всегда работает; `debug_assert` — только performance в release.
Это согласовано с D7 принципом «один язык — три режима».

### Почему `assert` = always runtime (не Java/C-style no-op)

1. **AI-friendly: одна семантика.** LLM генерирует `assert(...)`
   ожидая, что invariant держится. Если в release он silent — это
   **тихий bug class** (Java pre-1.4 classic).

2. **Безопасность.** «Production runs without your invariants» —
   известная проблема C/Java/Python: программист в курсе своих
   asserts только в debug, в release они **исчезают** без следа.

3. **Прецедент Rust/Swift.** `assert!` в Rust always runtime;
   `debug_assert!` для debug-only. Swift аналогично: `assert`
   debug-only, `precondition` always runtime — но Nova инвертирует
   defaults (более безопасный — короткое имя).

4. **Согласовано с D24.** Если программист хочет zero-cost проверку
   с compile-time гарантией — пишет `requires` (D24 contract). Если
   просто debug-time hint — `debug_assert`. `assert` — strong
   invariant, всегда работает.

5. **D13 (panic vs effects).** `assert` failure = panic = fiber dies.
   Это «hardware/math сбой» класс, не business error. По D13 такое
   **не должно зависеть от build-mode**.

### Что отвергнуто

- **`assert` no-op в release** (C/Java/Python style). Тихие bug'и в
  production — главная причина отказа.
- **`assert` как keyword без скобок** (Rust macro / Java `assert`
  expression). Закрыто в spec sweep 2026-05-07: assert — обычная
  fn-call, со скобками. Один способ для одной задачи (D40).
- **Только один уровень (`assert` always runtime).** Hot-path
  use-case реален; без `debug_assert` программисты пишут
  `if (DEBUG) { ... }` ручками. Лучше дать canonical-форму.
- **Только один уровень (`assert` debug-only).** Невозможно выразить
  production invariant. Java pre-1.4 опыт показывает что это
  anti-pattern.

### Связь

- [D7](#d7-один-язык--три-режима-компиляции) — три режима компиляции;
  D81 уточняет, как build-mode влияет на assert-семантику.
- [D13](#d13-panic-vs-эффекты-что-не-является-эффектом) — assert
  failure = panic, не Fail-эффект.
- [D24](09-tooling.md#d24) — `requires`/`ensures` контракты;
  D81 определяет три уровня safety: `assert` < `debug_assert` <
  `contracts`.
- [D26](#d26) — prelude содержит обе функции (`assert`,
  `debug_assert`).
- spec/syntax.md — секция «Тестирование без моков» уточняет, что
  `assert(cond)` обязательно со скобками (fn-call).

### Эволюция

До 2026-05-07 spec упоминал `assert` неявно — в `syntax.md` как
«встроенный оператор» (без скобок), в D26 prelude как функцию (со
скобками). Bootstrap-парсер принимал только со скобками.
spec-assert-syntax sweep 2026-05-07 канонизировал форму
`assert(cond)` — функция из prelude, обязательно со скобками.

D81 закрывает оставшийся вопрос — **семантика в release**.
Принята модель Rust (`assert!` always runtime + `debug_assert!`
debug-only). До D81 spec не различал `assert`/`debug_assert`,
bootstrap имел только always-runtime `nova_assert` без build-mode
разделения. После D81: prelude содержит обе функции; production-
runtime реализует zero-cost `debug_assert` в release; bootstrap
оставляет `debug_assert` как alias `assert` до production.

---

## D82. `external fn` — функции с runtime-implementation

### Что

`external fn` — модификатор функции-декларации, означающий что **тело
функции реализовано в runtime (C-коде `nova_rt/`)**, а не на Nova.
Декларация даёт сигнатуру и имя; codegen lookup'ит C-функцию по
имени в hard-coded таблице.

`external` применяется к **функциям** (этот D-block) и к **типам**
(D126, Plan 62.D.bis, 2026-05-18). Один и тот же keyword, два валидных
позиционирования. Built-in opaque-типы (`StringBuilder`, `WriteBuffer`,
`ReadBuffer`) теперь имеют formal Nova-side declaration через
`external type` в `std/prelude/collections.nv` — раньше (до 62.D.bis)
существовали как «known-by-name» (без formal declaration).

### Правило

#### Грамматика

```
fn-decl = ['export'] ['external'] 'fn' [receiver] name [generic-params]
          [params] [effects] ['->' return-type] [body | ';']
```

Порядок modifiers строгий: `export` первым, `external` вторым. Body
у `external fn` **должен отсутствовать** (никакого `=>` или `{ ... }`),
иначе compile error «external function cannot have a body».

#### Примеры

```nova
// Public external static
export external fn StringBuilder.new() -> Self

// Public external instance, mutating
export external fn StringBuilder mut @append(s str) -> ()

// Private external (используется внутри runtime/builtins.nv module'а)
external fn Nova_intrinsic_unreachable() -> never
```

#### Связь с D26 prelude

Built-in opaque-типы из D26 (`StringBuilder`, `WriteBuffer`,
`ReadBuffer`) имеют **type declaration** через `external type`
([D126](03-syntax.md#d126-external-type--opaque-типы-без-body),
`std/prelude/collections.nv`) + **methods** через `external fn`
(этот D-block, `std/runtime/<name>.nv`). Связь декларация ↔ methods
— по receiver-type name.

```nova
// std/prelude/collections.nv (Plan 62.D.bis, 2026-05-18)
module std.prelude.collections

export external type StringBuilder    // D126
export external type WriteBuffer      // D126
export external type ReadBuffer       // D126

// std/runtime/string_builder.nv (auto-generated, Plan 13 Ф.8)
module std.runtime.string_builder

export external fn StringBuilder.new() -> Self
export external fn StringBuilder.with_capacity(n int) -> Self
export external fn StringBuilder mut @append(s str) -> Self
// ... остальные методы
```

`Self` в receiver-context для external — `StringBuilder` (имя
содержащего receiver-type'а). Те же правила, что для обычных
fn-декл.

#### Связь с D5/D47 видимостью

`export external fn` — публичная: имя видно из других модулей.
`external fn` без `export` — модуль-private. Те же правила, что для
обычных fn-декл. `external` ортогонален `export`.

#### Связь с будущим FFI

`external fn` — для функций, реализованных **в Nova-runtime**
(`nova_rt/*.h`/`.c`). Для функций, импортируемых из **сторонних
C-библиотек** (libc, OS-libs), будет отдельный keyword
`extern("C")` (Q-ffi, не реализуется сейчас). Семантика разная:

| Keyword | Реализация | C-name | Разрешён программисту |
|---|---|---|---|
| `external fn` | Nova-runtime (`nova_rt/`) | `Nova_<Type>_<...>` mangled | **нет** (только в `std.runtime.*`) |
| `extern("C") fn` (TBD) | сторонний C/lib | as-is | да (FFI) |

Программистский Nova-код **не пишет** `external fn`. Этот keyword —
**экспозиционный**: только модули в `std.runtime.*` имеют право его
использовать. Компилятор **отклоняет** `external fn` в любом другом
namespace'е.

#### Mangling и dispatch

Codegen **не хранит** список external-функций. Source of truth — это
`std/runtime/builtins.nv`. Codegen знает **только правила mangling**
и для каждой `external fn` декларации выводит C-name детерминированно:

| Nova-form | C-name |
|---|---|
| `T.method(...)` static | `Nova_T_static_method(...)` |
| `t.method(...)` instance | `Nova_T_method_method(t, ...)` |
| `t.method(...)` mut instance | `Nova_T_method_method(t, ...)` (тот же mangling) |

Имена параметров в C-сигнатуре генерируются из позиций (`arg0`,
`arg1`, ...); типы маппятся по canonical Nova→C таблице (`int` →
`nova_int`, `str` → `nova_str`, `u8` → `uint8_t`, `u32` →
`uint32_t`, `&T` → `Nova_T*`, `mut T` → `Nova_T*`, ...).

Этот mapping **архитектурно идентичен** registry built-in conversions
(D73 + Plan 08 Ф.2). Один механизм lookup'а.

#### Validation: builtins.nv — single source of truth

Подписи external-функций живут **только** в `std/runtime/builtins.nv`.
Никакой дублирующей таблицы в Rust-коде codegen'а быть не должно;
если есть — это bug, и расхождение между .nv-декларацией и Rust-
таблицей приведёт к runtime-крашу или silent UB.

**Сигнатура** в этом разделе понимается полно — это весь contract
вызова, не только имя и типы параметров:

| Компонент | Используется для |
|---|---|
| Имя метода (`write_u32_be`) | C-name через mangling |
| Receiver-type + `mut`-флаг (`WriteBuffer mut`) | Первый параметр C-функции (`Nova_WriteBuffer*`), prefix mangling |
| Параметры (имена + типы, в порядке) | Остальные параметры C-функции; для overload — также часть mangling (Plan 11 Ф.3) |
| **Return-type** | C-return type; для auto-derive — целевой тип synthesized обёртки |
| Effects (`Fail[E]`, etc.) | Дополнительный `*err`-параметр в C-сигнатуре + control-flow эмиссии |

Любой из этих компонентов, если расходится между .nv-декларацией и
runtime-реализацией компилятора, отлавливается **самим Nova-
компилятором** при загрузке builtins.nv (раздел Diagnostics ниже),
не на стадии C-toolchain'а. В частности **return-type входит в
проверку**: если в builtins.nv `... -> u32`, а компилятор знает
что runtime возвращает `uint64_t` — Nova-error «signature
mismatch».

**Pipeline:**

1. Компилятор парсит `std/runtime/builtins.nv` как обычный Nova-
   модуль. Каждая `export external fn ...`-декларация даёт AST-узел
   с полной сигнатурой (имя, receiver, params, return, effects).
2. Codegen применяет mangling rules → C-name + C-prototype:
   ```c
   void Nova_WriteBuffer_method_write_u32_be(Nova_WriteBuffer*, uint32_t);
   ```
3. Codegen сверяет каждую декларацию со своим внутренним реестром
   реализованных runtime-функций (компилятор и runtime — один
   версионируемый артефакт, см. Diagnostics ниже).
4. Если совпадает — codegen эмитит C-prototype в сгенерированный
   header для линковки с `nova_rt/`.
5. Если не совпадает (нет реализации, расходится сигнатура) →
   **Nova compile error** до запуска C-toolchain'а.

**Что это даёт:**

- Программист добавляет `export external fn WriteBuffer mut
  @write_u64_le(v u64) -> ()` в builtins.nv → если компилятор уже
  поддерживает `Nova_WriteBuffer_method_write_u64_le` (в bundled
  runtime), декларация принимается; иначе — Nova-error с понятной
  диагностикой.
- AI-генерируемый код для расширения runtime API — два места правки:
  builtins.nv (Nova-side) + nova_rt/*.c (C-side). Компилятор
  валидирует, что они согласованы.

**Что это запрещает:**

- Hard-coded **списки методов конкретных opaque-типов** в codegen'е
  (сейчас `record_schemas.insert("StringBuilder", ...)` + method
  dispatch таблицы) — должны быть удалены или сведены к чтению
  AST builtins.nv. Q-codegen-builtins-cleanup, Plan 12 Ф.5.
- «Скрытые» external-функции, известные только codegen'у, без
  декларации в builtins.nv. Если codegen эмитит вызов
  `Nova_X_method_y` — соответствующая `external fn X.@y(...)`
  декларация **обязана** существовать в builtins.nv (или другом
  модуле в `std.runtime.*`).

#### Diagnostics: компилятор сам валидирует, без C-toolchain

Nova компилируется в C, который потом обрабатывается C-toolchain
(`cc`/`clang`/`MSVC`). У C-toolchain есть свой линкер, но мы **не
полагаемся** на его ошибки для пользовательской диагностики:
mangled C-имя в `undefined reference to Nova_WriteBuffer_method_X`
не понятно тому, кто пишет на Nova.

Вместо этого Nova-компилятор сам знает, какие external-функции
реализованы в bundled runtime (`nova_rt/`). Runtime версионируется
**вместе с компилятором**; компилятор всегда знает свой runtime.
builtins.nv — **проекция** этого знания в Nova: декларации, которые
компилятор валидирует против собственного внутреннего реестра.

Расхождение выдаётся как **Nova compile error** до запуска `cc`.
Таксономия:

| Случай | Когда | Диагностика |
|---|---|---|
| User вызывает несуществующий метод opaque-типа (`sb.unknown()`) | type-check | Nova: `no method 'unknown' on StringBuilder. Available: append, len, capacity, ...` |
| `external fn X.@y` в builtins.nv ссылается на функцию, не реализованную в runtime | при загрузке builtins.nv в codegen | Nova: `external fn 'StringBuilder.@y' not implemented in runtime. Either remove from std/runtime/builtins.nv or add Nova_StringBuilder_method_y to nova_rt/string_builder.c` |
| Сигнатура в builtins.nv не совпадает с реализацией компилятора (тип параметра, return-type, effects) | при загрузке builtins.nv | Nova: `signature mismatch for 'StringBuilder.@append': declared 'fn (s str) -> ()', runtime expects 'fn (s str) -> int'` |
| Codegen эмитит вызов внешней функции, не объявленной в builtins.nv | bug в компиляторе | internal compile error: `compiler bug: emitted call to undeclared external 'X.@y'`. Не должно случаться у пользователя; если случилось — bug-report |
| User объявил auto-derived форму (`@try_read_X` рядом с `@read_X`) | при загрузке builtins.nv | Nova: `'@try_read_X' is auto-derived from '@read_X' (D77 Fail↔Result); remove from std/runtime/builtins.nv` |

C-toolchain никогда не должен быть первым, кто заметит проблему.
Если он всё-таки выдаёт `undefined reference` — это **bug в Nova-
компиляторе**: либо реестр был неполным, либо валидация не сработала.

**Что не валидируется на этом уровне:**

- Семантика реализации (правильно ли `write_u32_be` пишет big-endian
  байты) — runtime tests, не compile-time check.
- Memory ownership / lifetime / aliasing — это контракт типа (mut,
  &T), линкер его не видит.

### Почему

#### Зачем нужен `external` keyword

1. **Документация stdlib API.** Программист (и AI) видя
   `external fn StringBuilder.new()` понимает: тело реализовано
   runtime'ом, не Nova. Не нужно искать в `nova_rt/` где определён.
2. **Compile-time validation.** Без `external` компилятор не знает,
   что функция без тела должна искаться в C-runtime — попытается
   эмитить empty body и упадёт. С `external` — явный contract.
3. **AI-friendly.** LLM-генерируемый код для stdlib имеет canonical
   форму: `export external fn ...`. Шаблонная подстановка тривиальна.
4. **Будущая совместимость с FFI.** Когда появится `extern("C")` для
   сторонних libs, два keyword'а различаются однозначно.

#### Почему не `intrinsic` или `builtin`

- `intrinsic` — занят понятием compile-time intrinsic (Rust-style
  `intrinsics::transmute`). Для Nova таких пока нет, но имя зарезервируем.
- `builtin` — слишком общее. `int`/`str` тоже builtin (D26), но они
  **типы**, не функции.
- `external` — точное слово: «реализация во **внешнем** (по отношению
  к Nova-source) контексте — runtime/C». Прецеденты: OCaml `external`,
  Dart `external`, Kotlin `external`.

#### Почему не `extern`

D30 фиксирует «полные слова, не сокращения». `external` — full word.
`extern` — сокращение (как в C/Rust). Мы выбираем full form.

### Что отвергнуто

- **Без keyword'а — компилятор сам решает по имени модуля.** Магия:
  программист не видит чего ожидать, AI генерирует boilerplate-`type`
  декларации.
- **`builtin fn`** — конфликт с понятием built-in типа.
- **`@external` атрибут вместо keyword'а.** Атрибуты в Nova
  зарезервированы для тестов / dev-tools (Q-attributes). Modifier-форма
  единообразна с `export`/`mut`.
- **`external type`** — закрыто 2026-05-18 в [D126](03-syntax.md#d126-external-type--opaque-типы-без-body).
  Изначально для три built-in (StringBuilder/WriteBuffer/ReadBuffer);
  future user-defined opaque типы (Channel, mmap'ed Region) — тот же
  D126 mechanism + relaxation whitelist'а. Plan 62.D.bis (Ф.1–Ф.6,
  2026-05-18) — реализация в bootstrap.
- **Codegen — single source (вариант A).** Сигнатуры жили бы в
  Rust-таблицах; builtins.nv был бы только документацией, а codegen
  cross-check'ал бы при чтении. Отвергнуто: дублирование (два места
  правки на каждую новую runtime-функцию), риск тихого расхождения
  если cross-check где-то пропущен, недружелюбно к AI (надо править
  Rust-код codegen'а).
- **Hybrid: builtins.nv для типов + codegen хранит mangling.** Тоже
  отвергнуто — оставляет Rust-таблицу как «второй источник», даже
  если меньшего объёма. Принят чистый вариант B: builtins.nv —
  единый источник; codegen знает только правила mangling.

### Связь

- [D5 / D47](07-modules.md#d47) — `export` modifier; `external` —
  ортогональный второй modifier.
- [D26](#d26) — prelude содержит StringBuilder/WriteBuffer/ReadBuffer
  как built-in opaque-типы; декларации API — через `external fn`.
- [D30](03-syntax.md#d30) — naming convention; `external` — full word.
- [D52](02-types.md#d52) — kind-tokens (`type`/`effect`/`protocol`);
  D82 **не** добавляет нового kind-token'а.
- [D54](03-syntax.md#d54) — `as`/`is` для конверсий; не пересекается.
- [D73](#d73-from--into-protocol-пара-с-авто-выводом) — From/Into
  registry; D82 использует тот же dispatch-механизм для external-функций.
- [D126](03-syntax.md#d126-external-type--opaque-типы-без-body) —
  type-аналог D82 (`external type` для opaque-типов с runtime
  backing). Один keyword `external`, два валидных позиционирования.

### Эволюция

До 2026-05-08 spec фиксировал `Buffer` как единый тип (Q-buffer) —
text+binary mixed. В разговоре про endianness-методы выявилось
семантическое смешение: `add_str` рядом с `add_u32_le` несогласовано.

Plan 04 (зафиксирован 2026-05-08) — split на три типа
(`StringBuilder` / `WriteBuffer` / `ReadBuffer`) + новый keyword
`external` для документирования stdlib runtime-функций. До D82 такие
функции декларировались как обычные `fn` без тела (компилятор
special-case'ил по имени receiver'а — fragile).

### Bootstrap status (2026-05-08)

- ✅ Спека: D82 закрыт (этот блок). Validation rule (builtins.nv —
  single source of truth) добавлен 2026-05-08 после обсуждения
  signature mismatch для `WriteBuffer.@write_u32_be`.
- ⏳ Lexer: `KwExternal` token — TBD (Plan 04 Этап 2).
- ⏳ Parser: `external` modifier в `parse_fn_decl` — TBD.
- ⏳ AST: `is_external: bool` flag — TBD.
- ⏳ Codegen: чтение external-деклараций из AST builtins.nv,
  применение mangling rules, эмиссия C-prototype'ов в header — TBD
  (Plan 04 Этап 2).
- ⏳ Codegen cleanup: удалить hard-coded `record_schemas.insert(...)`
  и method dispatch-таблицы для StringBuilder/WriteBuffer/ReadBuffer.
  Должны замениться чтением builtins.nv. Это **ломает** silent
  расхождения, которые сейчас существуют (Q-codegen-builtins-cleanup).
- ⏳ Runtime: `nova_rt/string_builder.h` / `write_buffer.h` /
  `read_buffer.h` — TBD. Реализации обязаны матчить builtins.nv по
  C-name + сигнатуре; иначе linker error.

### Plan 13: расширение projection на str/math + декомпозиция (2026-05-08)

После Plan 13 Ф.8 **в `std/runtime/` нет ни одного handwritten файла**.
`builtins.nv` ❌ REMOVED — декомпозирован на per-type auto-generated файлы:

| Что | Файл (auto-gen) |
|---|---|
| str API (UTF-8 операции) | `std/runtime/string.nv` |
| f64/f32 math (D74 instance-методы) | `std/runtime/math.nv` |
| char/str interop (`str.from(c char)`) | `std/runtime/char.nv` |
| StringBuilder API | `std/runtime/string_builder.nv` |
| WriteBuffer API | `std/runtime/write_buffer.nv` |
| ReadBuffer API | `std/runtime/read_buffer.nv` |

Источник истины — `compiler-codegen/src/codegen/runtime_registry.rs` (Rust):
~157 entries (~17 str + ~50 math f64+f32 + ~50 ReadBuffer fail+try
форм + ~20 WriteBuffer numeric × LE/BE + StringBuilder + char).

Команда `regen_runtime.bat` (или `.\regen_runtime.ps1`, или прямой
`nova-codegen emit-runtime-stubs`) генерирует все 6 `.nv` файлов;
manual edit запрещён (CI guard через `--check`).

ExternalRegistry в codegen загружает 4 .nv файла через `include_str!`
(string_builder, write_buffer, read_buffer, char) — единый registry для
opaque-types dispatch (Plan 12). string.nv/math.nv пока загружаются
emit-runtime-stubs только; codegen-side dispatch для str/math остаётся
через legacy special-cases (Plan 13 Ф.4 deferred).

См. [docs/plans/13-runtime-stdlib-and-autogen.md](../../docs/plans/13-runtime-stdlib-and-autogen.md).

## D109. Встроенные методы примитивных типов — hash, eq, ord

### Что

Компилятор автоматически предоставляет следующие методы для стандартных
примитивных типов без явных деклараций в `.nv` файлах:

| Метод | Возврат | Применимо |
|---|---|---|
| `hash() -> u64` | беззнаковый 64-bit хеш | int, bool, f64, char, u8, str |
| `eq(Self) -> bool` | равенство | int, bool, f64, char, u8, str |
| `lt(Self) -> bool` | строго меньше | int, f64, char, u8, str |
| `le(Self) -> bool` | меньше или равно | int, f64, char, u8, str |
| `gt(Self) -> bool` | строго больше | int, f64, char, u8, str |
| `ge(Self) -> bool` | больше или равно | int, f64, char, u8, str |

Эти методы нужны для использования примитивов как ключей в
`HashMap[K, V Hashable]` и других коллекциях с protocol bounds (D72).

### Семантика

**hash:**
- `int`/`char`/`u8` — FNV-1a по 8 байтам значения (`nova_int_hash`).
- `bool` — 0 или 1 (`nova_bool_hash`).
- `f64` — FNV-1a по битовому представлению (`nova_f64_hash`; -0.0 и 0.0
  хешируются по-разному — bootstrap ограничение, production fix V2).
- `str` — FNV-1a по байтам контента (`nova_str_hash`; уже реализован,
  объявлен явно в `std/runtime/string.nv`).

**eq:** сравнение по значению. `f64.eq` использует `==` (NaN != NaN по IEEE 754).

**lt/le/gt/ge:** лексикографически для `str`, по значению для остальных.
Для `bool` эти методы не предоставляются (нет естественного порядка).

### Как реализовано

C-функции в `nova_rt.h`:
- `nova_int_hash(nova_int) -> nova_int`
- `nova_bool_hash(nova_bool) -> nova_int`
- `nova_f64_hash(nova_f64) -> nova_int`
  (возврат `nova_int` = int64_t, хранит битовое значение u64)

`eq/lt/le/gt/ge` для `nova_int`/`nova_bool`/`nova_f64` — inline C-операторы
`==`, `<`, `<=`, `>`, `>=` (без отдельных C-функций).

Codegen: `prim_builtin_method(c_ty, method)` в `emit_c.rs` перехватывает
метод-вызов до общего resolver'а и эмитит нужный код.

### Что отвергнуто

- **Явные декларации в prelude.nv** — лишний boilerplate, нет спасения от
  расхождения между .nv и runtime impl. Codegen-уровень: единый источник
  правды.
- **`Ord` protocol bound** — структурный bound (`lt/le/gt/ge` методы) V2;
  для D109 достаточно auto-dispatch без формального `Ord` protocol.
- **Хеш для пользовательских типов** — авто-derive (аналог Rust
  `#[derive(Hash)]`) V2; требует рекурсивного обхода полей.

### Связь

- [D72 — Generic bounds](02-types.md#d72) — `Hashable` требует `hash() -> u64`.
- [D26 — stdlib](08-runtime.md#d26) — примитивные методы часть runtime stdlib.
- [docs/plans/48-closures-in-generics.md → Ф.8](../../docs/plans/48-closures-in-generics.md)
  — реализация.

## D124. Edition-versioned prelude resolver

### Что

`[package].edition = "<X.Y>"` в `nova.toml` — pin prelude content на
конкретный snapshot. Resolver выбирает `std/prelude/<sanitized(<X.Y>)>.nv`
вместо rolling `std/prelude.nv` facade.

**Sanitization rules** (`manifest::sanitize_edition`):
- Не-alphanumeric ASCII → `_` (e.g. `2026.05` → `2026_05`).
- Digit-leading prefix → `e` (e.g. `2026_05` → `e2026_05`), потому что
  Nova-identifier должен начинаться с буквы / `_`.
- Empty input → empty output (caller-side responsibility).

**Examples:**
- `edition = "2026.05"` → `std/prelude/e2026_05.nv`
- `edition = "nightly"` → `std/prelude/nightly.nv`
- `edition = "v1-beta"` → `std/prelude/v1_beta.nv`

**Fallback chain** (resolver-side):
1. Edition pin: `std/prelude/<sanitized>.nv` — если файл существует,
   import path = `["std", "prelude", "<sanitized>"]`.
2. Rolling facade: `std/prelude.nv` — backward-compat default (нет
   edition в манифесте, или edition pin не найден).

**Soft-fail:** edition specified, но файла нет → silently fall back на
rolling facade (не блокируем build, user может указать pin без файла
для будущего расширения).

### Правило

```toml
# nova.toml
[package]
name = "myapp"
edition = "2026.05"
```

→ Все модули в `myapp` auto-импортируют `std/prelude/e2026_05.nv`
вместо rolling `std/prelude.nv`. Будущие изменения rolling facade
(новые re-export'ы, signature drift) НЕ затрагивают packages с
pinned edition — они видят фиксированный snapshot.

### Зачем

- **Industry-standard pinning.** Rust `edition = "2021"`, Go `go 1.21`,
  Swift package `swift-tools-version` — stability через explicit pin.
- **Migration safety.** Maintainer'ы prelude могут add'ить re-export'ы
  в rolling facade без breaking changes для users с pinned edition.
- **AI-friendly.** LLM-генерируемый код с stable edition → reproducible.

### Что отвергнуто

- **Universal pin через one global rolling.** Без edition future
  изменения prelude (например new re-export shadowing user-type) ломают
  существующие packages. Edition pin даёт opt-out из rolling.
- **Multi-edition support в одном workspace.** Каждый package имеет
  одну edition; transitive deps могут иметь свои edition'ы независимо.
- **Auto-migrate workflow.** Edition bump — explicit decision package
  owner'а (как Rust `cargo fix --edition`). Tooling может предложить,
  но не auto-apply.

### Связь

- [D26 — stdlib и prelude](#d26) — base prelude content.
- [D78 — package tooling](07-modules.md#d78) — `nova.toml` schema.
- [Plan 62.F.bis Ф.1](../../docs/plans/62.F.bis-edition-shadow-and-runtime-effects.md)
  — implementation.

## D125. Prelude shadow warning lint

### Что

`W_PRELUDE_SHADOW` — structured lint warning эмитимый когда
user-declaration top-level имени shadow'ит prelude-imported name
(D26, D29). User-declaration wins (silent shadow), warning сигнализирует
о потенциальной AI/training confusion.

**Эмиттер:** `lints::lint_prelude_shadow` (lints.rs::lint_module
включает его в общий проход). LintWarning имеет:
- `rule = "W_PRELUDE_SHADOW"` (grep'абельно из CLI и для
  `EXPECT_COMPILE_WARNING` matching в `nova test`).
- `diag.message` начинается с `[W_PRELUDE_SHADOW]` tag (для rendering
  через `diag.render` — `rule` поле не leak'ит в текст автоматически).
- Actionable hint: `qualify as std.prelude.<sub>.<name>` или
  `add allow_prelude_shadow / no_prelude / partial_prelude(...)`.

**Visibility detection:** `lints::collect_prelude_visibility` — shared
helper между `types::check_module` (silent classify duplicates как
W_PRELUDE_SHADOW vs codegen-only merge) и `lint_prelude_shadow`
(structured warning emission). 2-pass:
1. Names declared directly в `std/prelude/*.nv` peer files (включая
   `std/prelude.nv` facade себя).
2. Names re-exported через `export import X.{A, B as C}` selective list.

**Suppress mechanisms:**
- **Module-level clause** `module X allow_prelude_shadow` — silences
  ALL W_PRELUDE_SHADOW warnings в модуле. См.
  [07-modules.md → Allow prelude shadow](07-modules.md#allow-prelude-shadow-plan-62fbis-2026-05-18).
- **Prelude self-modules** (`std.prelude.*`, `<pkg>.prelude.*`) —
  automatically skipped (они LEGITIMATELY declare prelude names).
- **Item-level suppress** (`#[allow(prelude_shadow)] type Foo`) —
  DEFERRED (требует generic attribute parser; пока не приоритет).

### Правило

```nova
module myapp.dsl

// Conflict: PRELUDE_VERSION auto-imported via std/prelude.nv;
// user-decl wins (codegen skips merged duplicate via Const-skip path),
// W_PRELUDE_SHADOW emitted.
const PRELUDE_VERSION int = 42  // → warning
```

```nova
module myapp.dsl allow_prelude_shadow

// Same conflict, suppress'нут (warning не эмитится).
const PRELUDE_VERSION int = 42  // → silent
```

### Зачем

- **AI/training clarity.** LLM-generated code часто случайно shadow'ит
  prelude names (e.g. local `type Result { ... }`). Warning catches it
  early; explicit suppress сигнализирует intentional override.
- **Migration safety.** Если будущий prelude bump добавит новое имя
  (e.g. `From`/`Into` в Plan 62.E), existing user-decl с тем же именем
  получит warning — обнаружение early-stage.
- **Не error.** Sometimes shadowing намеренно (DSL слой, embedded);
  warning + suppress даёт user-выбор vs hard block.

### Что отвергнуто

- **Hard error.** Per D5 / D26: user wins на conflict — shadowing
  допустим как backward-compat механизм. Error блокировал бы legitimate
  DSL use-cases.
- **Codegen-only merge как warning.** Когда prelude impl-merge подтягивает
  type не visible в user код (e.g. internal struct prelude'а), и user
  re-declares то же имя — это НЕ shadow, потому что user не "видел"
  prelude name. Lint фильтрует через `prelude_visible_names` vs
  `merged_from_imports_names`.
- **Per-name allowlist.** `allow_prelude_shadow = ["Option"]` — слишком
  fine-grained, добавляет complexity без явного use-case. Module-level
  bool clause достаточен.

### Связь

- [D26 — stdlib и prelude](#d26) — prelude scope rules.
- [D29 — модули и импорты](07-modules.md#d29) — name resolution.
- [07-modules.md → Allow prelude shadow](07-modules.md#allow-prelude-shadow-plan-62fbis-2026-05-18)
  — clause syntax.
- [Plan 62.F.bis Ф.2](../../docs/plans/62.F.bis-edition-shadow-and-runtime-effects.md)
  — implementation.

---

## D141. Примитивы доступа к памяти — `byte_at` / bulk slice-операции

> **Plan 90.** Принято 2026-05-22.
> **Plan 90.1 amend.** 2026-05-27 — extend-family API + `copy_from` hardening.
> **Plan 91 amend (2026-05-30).** Rename for semantic clarity:
> `extend_from → append`, `insert_from → insert`. Старые имена удалены —
> breaking change. Family по `_from`-суффиксу распался: append/insert —
> semantic verbs из append/insert classes; `copy_from` остаётся (Rust-style
> `copy_from_slice`, теперь isolated имя). Migration: replace_all для всех
> call-sites. Lint `W_VIEW_EXTEND_DETACH` сохранён (детектит grow-методы
> `append`/`insert`/`reserve`).
> **Plan 90 followup amend (2026-06-01).** Два уточнения runtime:
> (1) `fill(v)` — memset fast-path для single-byte `T` (`u8`/`i8`):
> `sizeof(T) == 1` ветка → `memset(data, v, len)`. Per-instantiation
> compile-time DCE: для wider T (`int`/`f64`/`ptr`) остаётся scalar loop
> с auto-vectorization potential под `-O2`.
> (2) `append_zero(n int)` — extend by `n` zero-initialized elements.
> Полиморфно по `T` через `memset(_, 0, n*sizeof(T))` — zero bytes =
> valid zero-init для всех primitives + `nova_ptr` (NULL) + `bool`
> (false). Use-case: encoders/framers (reserve write-window под
> length-prefix или padding с последующим патчингом через
> index-assignment `arr[i] = v`). `W_VIEW_EXTEND_DETACH` lint срабатывает
> на `append_zero` (grow-метод).

### Что

Минимальный набор **безопасных** примитивов доступа к памяти, чтобы
алгоритмы рантайма и stdlib (str-методы, буферы, парсеры) выражались на
Nova без лишних аллокаций и без ухода в `external fn`. Сырые указатели и
`unsafe`-режим **не вводятся** — Nova остаётся языком без указателей
([D6](05-memory.md#d6)).

### Правило

**`str.byte_at`** — O(1) доступ к байту строки:

```nova
fn str @byte_at(i int) -> u8
```

Byte-indexed (не codepoint). Выход за границы (`i < 0 || i >= byte_len`)
— `panic` ([D13](#d13-panic-vs-эффекты-что-не-является-эффектом)).
Неустранимый примитив для data-dependent байтовых алгоритмов (лексер,
`find`, `trim`).

**Bulk slice-операции `[]T`:**

```nova
fn []T mut @copy_from(src []T)                               // memmove (overlap-safe)
fn []T mut @copy_within(src_from int, dst_from int, len int) // memmove (overlap-safe)
fn []T mut @fill(v T)                                        // заполнение
```

- `copy_from` — строгое копирование: `src.len != dst.len` → `panic`
  «length mismatch». **Всегда memmove** (overlap-safe, паритет Go;
  см. «Overlap safety» ниже). Truncation use-case — через slicing:
  `dst[..n].copy_from(src[..n])` (D144).
  *Breaking change (Plan 90.1): прежняя молчаливая truncation (`src` короче
  `dst` → хвост не тронут) заменена на panic. Migration: `dst[..n].copy_from(src[..n])`.*
- `copy_within` — копирование внутри одного среза, **корректно при
  перекрытии** диапазонов (семантика `memmove`); диапазон вне границ →
  `panic`.
- `fill` — записывает `v` во все элементы. **Perf (Plan 90 followup
  2026-06-01):** single-byte `T` (`u8`/`i8`) → memset fast-path; wider
  `T` → scalar loop (auto-vectorizable). Compile-time selection через
  `sizeof(T)` constant per macro instantiation, DCE убирает мёртвую
  ветку из output.
- Определены для **любого** `T` (копирование element-storage корректно
  при non-moving GC, [D6](05-memory.md#d6)).

### Append/insert/reserve API (Plan 90.1, renamed Plan 91 2026-05-30)

```nova
fn []T mut @append(src []T)                  // bulk add to end, grows
fn []T mut @insert(i int, src []T)           // bulk insert at position, grows
fn []T mut @reserve(extra int)               // preallocate hint
fn []T mut @append_zero(n int)               // extend by n zero-init elements, grows (Plan 90 followup 2026-06-01)
```

**`append(src)`** — bulk append элементов `src` в конец `dst`, с ростом:
- Рост: если `dst.len + src.len > dst.cap` → new_cap = max(2 × dst.cap, needed).
  Паритет `push` (2x doubling, [D27]).
- **memmove**: safe для self-append (`dst.append(dst)`) — `src.len`
  снапшотится до realloc; после realloc memmove работает со старым буфером
  (Boehm GC удерживает до сборки). Test: `append_self.nv`.
- View detach: при realloc существующие slice-view'ы от `dst` становятся
  dangling. Lint `W_VIEW_EXTEND_DETACH` предупреждает; suppress через
  `#allow(view_extend_detach)`.

**`insert(i, src)`** — bulk вставка `src` в позицию `i` (элемент, не байт):
- Диапазон `i`: `[0, dst.len]` — включая `dst.len` (append-at-end ≡ `append`).
  `i < 0 || i > dst.len` → panic.
- Рост: та же стратегия, что `append`.
- In-place path (без realloc): memmove хвоста `[i, len)` вправо на `src.len` слотов;
  затем memmove `src` в образовавшуюся дыру (обрабатывает overlap).
- Alloc path: prefix `[0, i)` + дыра + tail `[i, len)` — три memcpy без overlap.

**`reserve(extra)`** — hint на preallocate `extra` дополнительных слотов:
- `extra < 0` → panic. `extra == 0` → no-op.
- `dst.len + extra ≤ dst.cap` → no-op O(1). Иначе рост ≥ `dst.len + extra`.
- `dst.len` не изменяется.
- View detach: при realloc — тот же lint.

**`append_zero(n)` (Plan 90 followup, 2026-06-01)** — extend by `n`
zero-init элементов:
- `n < 0` → panic `«append_zero: n must be >= 0»`. `n == 0` → no-op.
- Рост: та же 2x стратегия что у `append`/`insert`/`reserve`
  (`new_cap = max(2 × cap, new_len)`).
- Tail новой памяти инициализируется через `memset(data+old_len, 0,
  n*sizeof(T))` — полиморфно по `T`, в отличие от `fill` (где memset
  только для single-byte `T`).
- Zero bytes — **valid zero-init** для primitives (`int`/`u8`/`f64` → 0),
  `nova_ptr` (NULL), `bool` (false). Для compound value types
  (Plan 120 named tuples) zero-init соответствует семантике "все поля
  zero" — допустимое начальное состояние при условии что все поля
  имеют zero-default representation (IEEE 754 +0.0 для floats, NULL
  для optional). За пределами этого набора (например, type с
  invariant'ом «non-null» полем) — UB: zero-init нарушит invariant.
- `dst.len += n`. View detach: при realloc — `W_VIEW_EXTEND_DETACH`.
- **Use-case** — encoders/framers: reserve write-window для
  length-prefix или padding, затем patch через index-assignment:
  ```nova
  mut buf []u8 = []
  buf.append_zero(2)              // reserve 2 байта под length-prefix
  buf.append(payload)
  buf[0] = (payload.len() >> 8) as u8
  buf[1] = (payload.len() & 0xff) as u8
  ```
  До followup'а аналог был `buf.append([0, 0])` (литерал-аллокация)
  или explicit loop — оба неэргономично.

### Naming rationale (Plan 91 rename, 2026-05-30)

Старые имена `extend_from`/`insert_from` уродовали family-pattern: глагол не
обнажал semantic class. После rename:
- **append-family:** `push(v)` + `append(src)` + `append_zero(n)` —
  единая семантика "add to end" (`v` единичный, `src` срез, `n`
  zero-init слотов)
- **insert-family:** `insert(i, src)` — overload с будущим `insert(i, v T)`
- **overwrite-family:** `copy_from(src)` + `fill(v)` — equal-len mutation
- **move-family:** `copy_within(...)` — internal copy

`_from`-суффикс изначально склеивал ops по dispatch-детали ("берёт source array"),
а не по semantic class. Распад family + переход на semantic verbs — net win для
discoverability и API symmetry.

Migration было mechanical replace_all: ~30 файлов (compiler + std + 12 fixtures).

### Truncation idiom

```nova
// Новая строгая семантика copy_from:
dst.copy_from(src)  // panic если src.len != dst.len

// Idiom для частичного копирования (была старая silent-truncation):
dst[..n].copy_from(src[..n])  // explicit prefix slice — Plan 96 D144
```

`dst[..n]` — slice `NovaArray_T` с `len = cap = n` (D-cap-len, D144);
`copy_from` на нём требует `src[..n].len == n` → panic-safe.

### Overlap safety

Nova всегда использует `memmove` для array bulk-операций (не `memcpy`):
- **`copy_from`**: memmove → safe если dst и src overlap (через view в тот же буфер).
- **`copy_within`**: явно memmove, документировано.
- **`extend_from` / `insert_from`**: memmove для `src`-копирования → safe при
  view-аргументе.

Паритет Go (`copy()` + `append()` — memmove/safe). Отличие от Rust
`copy_from_slice` (UB при overlap, нет borrow-check): Nova overlap-safe
by default без lifetime annotations.

### W_VIEW_EXTEND_DETACH lint (Plan 90.1, names updated Plan 91)

```nova
ro view = parent[1..4]
parent.append([5, 6, 7])  // W_VIEW_EXTEND_DETACH: view may dangle after realloc
```

Lint срабатывает если в той же функции после `let view = parent[a..b]`
вызывается grow-метод на `parent` (`append` / `insert` / `reserve` /
`append_zero`).
После realloc `view.data` указывает на стёртую память (Boehm GC удерживает
до сборки, но lifetime семантически опасен).

Lint-name остался `W_VIEW_EXTEND_DETACH` (концепт "view extend → detach"
остаётся valid term-of-art независимо от method-naming).

Suppress через `#allow(view_extend_detach)` перед `module`-декларацией.
Параллельный lint — `W_VIEW_PUSH_DETACH` (Plan 96.1, D144).

**`compare` — один примитив сравнения `[]u8`:**

```nova
fn []u8 @compare(other []u8) -> int   // <0 / 0 / >0, лексикографически
```

memcmp-класс (byte-wise, word/SIMD-скорость). **Равенство — частный
случай:** `a == b` ⇔ `a.compare(b) == 0`; оператор `==` и
`lt`/`le`/`gt`/`ge` выводятся из `compare`. Отдельного `bytes_equal`
нет. Определён только для `[]u8`: для multi-byte `T` побайтовое
сравнение endianness-зависимо.

### Почему

- **Self-hosting и stdlib на Nova.** Без примитивов доступа к памяти
  str-методы и буферы вынужденно остаются C-кодом либо аллоцируют
  (`slice`/`bytes`). Примитивы переносят *алгоритмы* в Nova, оставляя в
  C лишь неустранимый минимум.
- **Безопасность сохранена.** Все примитивы bounds-checked; нет сырых
  указателей, нет `unsafe`-keyword. Паритет с Go (`copy()`/`bytes` —
  safe, без `unsafe`), Rust (`slice::copy_*`/`[u8]::cmp` — safe),
  TS (typed arrays — указателей нет вовсе). FFI-граница закрыта
  `external fn` ([D82](#d82-external-fn--функции-с-runtime-implementation))
  и `external type` (D126) — сырой указатель в систему типов Nova не
  попадает.
- **`compare` — один примитив.** memcmp возвращает порядок; равенство —
  его zero-case. Дублировать в два примитива (`equal` + `compare`)
  преждевременно (если профайл покажет — fast-path добавится позже,
  модель Go `bytes.Equal`).
- **Extend-family** (Plan 90.1): паритет с Go `append(dst, src...)`, Rust
  `extend_from_slice` / `Vec::reserve`, TS `push(...arr)` / `splice`,
  Kotlin `addAll`, Java `ArrayList.addAll`. Единственный grow-path до 90.1
  — `for x in src { dst.push(x) }` (O(N) virtual calls); `extend_from` —
  bulk memmove, намного быстрее для primitive `[]T`.
- **`copy_from` hardening** (Plan 90.1): молчаливая truncation —
  silent bug factory. Ни один из 5 эталонных языков не имеет такой гибрид
  «panic на длинный + silent на короткий». Strict equal-only + memmove —
  лучший баланс корректности и overlap-safety.

### Связь

- [D6 — память managed, без указателей](05-memory.md#d6).
- [D13 — panic](#d13-panic-vs-эффекты-что-не-является-эффектом) —
  семантика выхода за границы.
- [D27 §1659 — `[]T` push cap-growth](03-syntax.md) —
  та же 2x стратегия, что `extend_from`/`insert_from`/`reserve`.
- [D82 — `external fn`](#d82-external-fn--функции-с-runtime-implementation),
  D126 — `external type`: FFI-граница без сырых указателей.
- [D117 — size-accessors `[]T`/`str`](03-syntax.md#d117-size-like-accessors-require-call-syntax)
  — соседняя группа методов built-in-типов.
- [D144 — slices `arr[a..b]`](08-runtime.md) — truncation idiom через `dst[..n].copy_from(...)`.
- [Plan 90](../../docs/plans/90-memory-access-primitives.md) — baseline реализация.
- [Plan 90.1](../../docs/plans/90.1-array-extend-family.md) — extend-family + copy_from hardening.
- [Plan 96.1](../../docs/plans/96.1-array-slices-followup.md) — W_VIEW_PUSH_DETACH (параллельный lint).
- Ориентиры: Go `copy()`/`bytes`/`append`, Rust `slice::copy_*`/`extend_from_slice`/`Vec::reserve`,
  TS typed arrays/`splice`, Kotlin `copyInto`/`addAll`, Java `arraycopy`/`ArrayList.addAll`.

---

## D173. `std/net` — Async TCP/UDP socket stdlib via libuv

> **Status:** ✅ implemented (Plan 83.12, 2026-05-27). Merge `05f7e77592c`.

### Что

Nova предоставляет **async-transparent** сетевой stdlib `std/net/` на базе
libuv (`uv_tcp_t`, `uv_udp_t`). Все операции блокируют **fiber** (не OS thread)
через park/wake D93, выглядят синхронно в коде пользователя.

Модуль состоит из четырёх файлов:

| Файл | Содержимое |
|---|---|
| `std/net/addr.nv` | `IpAddr`, `SocketAddr` |
| `std/net/error.nv` | `NetError` — типизированные сетевые ошибки |
| `std/net/tcp.nv` | `TcpListener`, `TcpStream` |
| `std/net/udp.nv` | `UdpSocket` |

### Правила

#### 1. Типы адресов

```nova
type IpAddr = | V4(u8, u8, u8, u8) | V6(str)

namespace SocketAddr {
    fn new(ip IpAddr, port u16) -> SocketAddr
    fn loopback(port u16) -> SocketAddr   // 127.0.0.1:port
    fn any(port u16) -> SocketAddr        // 0.0.0.0:port
    fn parse(s str) -> Result[SocketAddr, NetError]
}
```

`str.from(SocketAddr)` возвращает `"ip:port"` (human-readable).

#### 2. TcpListener

```nova
namespace TcpListener {
    fn bind(addr SocketAddr) -> Result[TcpListener, NetError]
}

type TcpListener {
    fn accept(self) -> Result[TcpStream, NetError]   // parks fiber until connection
    fn local_port(self) -> u16
    fn close(self)
}
```

`bind(addr)` — OS TCP bind + listen. `local_port()` корректен после bind
(для `port=0` возвращает OS-assigned port).

`accept()` использует `nova_sched_park_until(pred: pending_conns > 0)` —
spurious wake безопасен, re-checks predicate (см. D93).

#### 3. TcpStream — lifecycle state machine

```
IDLE ──connect──▶ CONNECTING ──cb──▶ CONNECTED
                                          │
                                       close()
                                          ▼
                                      CLOSING ──close_cb──▶ CLOSED
```

Состояния: `IDLE=0 / CONNECTING=1 / CONNECTED=2 / CLOSING=3 / CLOSED=4`.
CAS-переходы атомарны. `write()` и `read_bytes()` проверяют stage ≥ CLOSING
перед операцией → возвращают `Err("stream closing")`.

```nova
namespace TcpStream {
    fn connect(addr SocketAddr) -> Result[TcpStream, NetError]  // parks fiber
}

type TcpStream {
    fn write(self, data str) -> Result[(), NetError]
    fn read_bytes(self, max_len int) -> Result[str, NetError]
    fn local_addr(self) -> SocketAddr
    fn remote_addr(self) -> SocketAddr
    fn close(self)
}
```

**EOF semantics:** `uv_read_cb` с `nread == UV_EOF` → `read_bytes()` возвращает
`Ok("")` (пустая строка). Чистое закрытие соединения = success, не error.

#### 4. UdpSocket

```nova
namespace UdpSocket {
    fn bind(addr SocketAddr) -> Result[UdpSocket, NetError]
}

type UdpSocket {
    fn send_to(self, data str, addr SocketAddr) -> Result[(), NetError]
    fn recv_from(self, max_len int) -> Result[(str, SocketAddr), NetError]
    fn local_port(self) -> u16
    fn close(self)
}
```

`recv_from` — parks fiber до получения датаграммы; возвращает `(data, sender_addr)`.

#### 5. NetError

```nova
type NetError =
    | ConnectionRefused
    | ConnectionReset
    | TimedOut
    | AddrInUse
    | AddrNotAvailable
    | Other(str)
```

Все `Result[T, NetError]` возвращаемые типы используют typed errors —
match-exhaustive на стороне пользователя.

#### 6. Thread-affinity invariant

libuv handles (`uv_tcp_t`, `uv_udp_t`) должны закрываться на том же OS thread,
на котором они созданы. В M:N режиме fiber может мигрировать между workers.

**Решение:** `nova_loop_defer_close(handle)` — enqueue request в
`NovaDeferredCloseQueue` текущего loop; worker деqueue и вызывает `uv_close`
на своём thread. В AUTOARM=0 (single thread) — direct `uv_close`.

#### 7. Park/wake контракт (D93-compliant)

1. Caller fiber: `nova_sched_register_pending(scope, slot)` → `nova_sched_park(scope, slot)`
2. libuv callback (`_tcp_connect_cb`, `_tcp_read_cb`, `_tcp_write_cb`, ...):
   устанавливает result поля → `nova_sched_wake(scope, slot)`
3. Fiber resume: читает result, возвращает `Ok(...)` или `Err(...)`

Stop callback для cancel: `uv_read_stop` + deferred `uv_close` → close_cb → wake.

### Почему

- **Fiber-transparent async** — пользователь пишет последовательный код (как Go),
  без `async/await` ключевых слов (в отличие от Rust/Tokio).
- **libuv** — уже в runtime (Plan 22), cross-platform (Linux/Windows/macOS),
  production-grade event loop.
- **D93 park/wake** — единый контракт для всех блокирующих операций (Time.sleep,
  Channel, net). Не дублируется логика.
- **Typed errors** — `NetError` sum type vs stringly-typed (Go `err.Error()`)
  позволяет exhaustive match.

### Связь

- [D93 — park/wake contract](06-concurrency.md#d93-park--wake-контракт-для-async-io) — основа impl.
- [D91 — Channel](06-concurrency.md#d91) — аналогичный park/wake pattern.
- [Plan 83.12](../../docs/plans/83.12-async-net-stdlib.md) — реализация.
- [Plan 83.3](../../docs/plans/83.3-blocking-effect-threadpool.md) — Blocking effect для DNS/sync IO.
- [Plan 91](../../docs/plans/91-stdlib-mvp-for-0.1.md) — std/net co-planned в 0.1.
- Ориентиры: Go `net.Listen`/`net.Dial`, Rust `tokio::net::TcpListener`.

---

## D177. `str` Nova-body dispatch — Plan 54 Ф.2 extension

*Plan 91 Ф.2.5 — 2026-05-28*

### Что

Пять методов `str` (`parse_int_radix`, `pad_left`, `pad_right`, `repeat`,
`replace`) реализованы как **Nova-body методы** в `std/runtime/string.nv` и
диспатчатся через механизм **Plan 54 Ф.2** (Nova method dispatch) вместо
C bootstrap shim'ов. Auto-available через `std.prelude` re-export — явный
`import std.runtime.string.{pad_right}` не требуется.

### Правило

#### 1. Nova-body декларации (std/runtime/string.nv)

```nova
// Parse int с указанной base (2..36). None при ошибке.
export fn str @parse_int_radix(radix int) -> Option[int] { ... }

// Pad до width codepoints слева символом fill.
export fn str @pad_left(width int, fill char) -> str { ... }

// Pad до width codepoints справа символом fill.
export fn str @pad_right(width int, fill char) -> str { ... }

// Повторить строку n раз (n ≤ 0 → "").
export fn str @repeat(n int) -> str { ... }

// Заменить все вхождения from на to.
export fn str @replace(from str, to str) -> str { ... }
```

Модуль `std/runtime/string.nv` использует `#no_prelude` для разрыва
циклического импорта `prelude → string → prelude`.

#### 2. Prelude auto-availability

```nova
// std/prelude.nv
export import std.runtime.string.{parse_int_radix, pad_left, pad_right, repeat, replace}
```

Все пять методов доступны в любом пользовательском модуле без явного
`import` — аналогично остальным prelude items (D26).

#### 3. Dispatch mechanism (Plan 54 Ф.2)

Codegen диспатчит `obj.method(...)` для `obj: str` через Plan 54 Ф.2:

1. `obj_ty = "nova_str"` → `prim_nova_name = "str"`
2. Look up `method_overloads[("str", method)]`
3. Фильтр `!is_external` — Nova-body методы получают `is_external = false`
4. Генерируется вызов `Nova_str_method_<name>(obj, args...)`

External fn методы (`@len`, `@eq`, `@split`, ...) имеют `is_external = true`
и **не перехватываются** Plan 54 Ф.2 — они продолжают диспатчиться через
`str_method_to_rt` → прямые C функции (без изменения поведения).

#### 4. Generated C names

| Nova method | C function |
|---|---|
| `str @parse_int_radix(radix int)` | `Nova_str_method_parse_int_radix` |
| `str @pad_left(width int, fill char)` | `Nova_str_method_pad_left` |
| `str @pad_right(width int, fill char)` | `Nova_str_method_pad_right` |
| `str @repeat(n int)` | `Nova_str_method_repeat` |
| `str @replace(from str, to str)` | `Nova_str_method_replace` |

Функции генерируются при каждой компиляции — встроены в выходной `.c` файл
как `static` функции (аналогично всем Nova-body методам).

#### 5. Removed C shims

`nova_str_parse_int_radix` удалён из `nova_rt/array.h`.
`nova_str_pad_left`, `nova_str_pad_right`, `nova_str_repeat`, `nova_str_replace`
оставлены в `nova_rt/string_builder.h` для внешних потребителей,
но codegen их больше не вызывает.

#### 6. consume-method alias (nova_rt/string_builder.h)

Nova-body методы `pad_left`, `pad_right`, `repeat` вызывают
`StringBuilder.into()` — consume-метод (`export external fn StringBuilder consume @into()`).
Codegen генерирует `Nova_StringBuilder_consume_into(sb)` (D164 ABI, Plan 100.6).
Добавлен inline alias в `string_builder.h`:

```c
static inline nova_str Nova_StringBuilder_consume_into(Nova_StringBuilder* b) {
    return Nova_StringBuilder_method_into(b);
}
```

### Почему

- **Единый механизм** — аналогично `fn int @seconds() -> Duration` (Plan 91
  Ф.1) Nova-body методы на примитивных типах позволяют писать стандартную
  библиотеку на Nova, а не на C.
- **Cycle-safe** — `#no_prelude` в `std/runtime/string.nv` + explicit imports
  `std.prelude.core.{Option, None, Some}` и
  `std.prelude.collections.{StringBuilder}` разрывают цикл `prelude → string → prelude`.
- **Single source of truth** — логика `replace` (concat-loop вместо
  `[]str.join`) написана один раз на Nova; C bootstrap shim'ы удалены.
- **Backward compatible** — external fn методы (`@len`, `@eq`, `@split`, ...)
  продолжают использовать `str_method_to_rt` без изменений. Фильтр
  `!is_external` в Plan 54 Ф.2 гарантирует, что только Nova-body методы
  перехватываются.

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — prelude auto-availability.
- [D82](08-runtime.md#d82) — external fn декларации (str external методы).
- [D176](02-types.md#d176-readonly-t--тип-модификатор) — `str.as_bytes() -> readonly []u8`
  используется в `parse_int_radix` body.
- [Plan 91.4](../../docs/plans/91.4-str-nova-body-dispatch.md) — sub-plan Ф.2.5 D177.
- [Plan 54](../../docs/plans/54-nova-body-methods.md) — Ф.2 dispatch mechanism.

---

## D178. `str` API cleanup и расширения — Plan 91 Ф.2.6

### Что

Комплекс из шести взаимосвязанных изменений `str` API, закрывающих Plan 91
Ф.2.6:

1. **`@bytes()` → `@to_bytes()`** — allocating copy; `@as_bytes()` (D176,
   zero-copy `readonly []u8`) остаётся без изменений.
2. **`@chars()` → `@to_chars()`** — allocating codepoint slice.
3. **`@split(sep str) -> []str` → `-> readonly []str`** — возвращает
   zero-copy views в оригинальный буфер; тип сигнализирует об этом.
4. **`@parse_int_radix(radix int)` + `@parse_int()` → `@parse_int(radix int = 10)`**
   — одна Nova-body функция с keyword-only default-параметром (D102).
   Вызов без аргументов: `"42".parse_int()` (radix=10). С явным radix:
   `"ff".parse_int(radix: 16)`. Позиционная передача default-параметра
   запрещена D102.
5. **`@compare(other str) -> int`** — новый C-примитив; возвращает
   отрицательное/ноль/положительное, как C `strcmp`. Реализован как
   `nova_str_compare` через `__builtin_memcmp`.
6. **`readonly bytes` parameter syntax** — параметр `from_bytes_lossy` и
   `from_bytes_unchecked` переписан в форму `readonly bytes []u8` (modifier
   перед именем параметра, а не перед типом). Оба варианта теперь
   поддерживаются парсером.

### Правило

```nova
// D178 итоговый str API (bootstrap):
export external fn str @to_bytes() -> []u8              // allocating copy
export external fn str @as_bytes() -> ro []u8     // D176: zero-copy
export external fn str @to_chars() -> []char            // allocating codepoints
export external fn str @split(sep str) -> ro []str
export external fn str @compare(other str) -> int       // <0 / 0 / >0

// from_bytes: `readonly` перед именем параметра (новая форма, D178)
export external fn str.from_bytes_lossy(ro bytes []u8) -> str
export external fn str.from_bytes_unchecked(ro bytes []u8) -> str

// parse_int: единственный метод с keyword-only default (D102)
export fn str @parse_int(radix int = 10) -> Option[int] {
    if radix < 2 || radix > 36 { return None }
    // ... тело на Nova (Plan 54 Ф.2)
}
```

**Prelude auto-import (std.prelude v11):**

```nova
export import std.runtime.string.{
    parse_int, pad_left, pad_right, repeat, replace,
    compare, to_bytes, to_chars, as_bytes
}
```

**Эквивалентность типов `readonly []u8`:**

```nova
ro []u8  ≡  ro [] ro u8
```

Оба варианта стриппируют recursive `readonly` до `NovaArray_nova_byte*` в
C codegen. Различие семантическое — первый «readonly array of u8», второй
«readonly array of readonly u8» — но в bootstrap-реализации оба ведут
себя идентично (нет изменяющих операций на байтах).

**Default-параметры и keyword-only вызов (D102):**

Параметр с дефолтным значением — всегда keyword-only (Nova D102). Попытка
передать позиционно вызывает ошибку компилятора. Для `parse_int`:

```nova
"ff".parse_int()          // ✓ radix=10 (default)
"ff".parse_int(radix: 16) // ✓ явно radix=16
"ff".parse_int(16)        // ✗ CODEGEN-FAIL: D102 keyword-only
```

**Codegen: default-arg fill-in для Nova-body dispatch (Plan 54 Ф.2):**

Когда вызов `str.method(fewer_args_than_params)` проходит через Plan 54
Ф.2 dispatch (`method_overloads[("str", m)]`, `!is_external` filter),
codegen заполняет пропущенные trailing аргументы из `MethodSig.param_defaults`.
Поле `param_defaults: Vec<Option<String>>` добавлено в `MethodSig`; при
регистрации методов из `FnDecl` — populate через `simple_literal_c` (конвертирует
литеральные default-expressions в C-строку без вызова `emit_expr`).

### Почему

- **Консистентность `to_*` prefix** — `to_bytes` / `to_chars` семантически
  аналогичны Rust `to_vec()` / `to_string()`: allocating copy. Без `to_`-prefix
  неясно, zero-copy или нет. `as_bytes()` остаётся как zero-copy аналог Rust
  `as_bytes()`.
- **`readonly []str` из `split`** — zero-copy views в оригинальный буфер;
  тип это выражает явно. Изменять элементы результата нельзя.
- **Единый `parse_int`** — вместо двух методов (`parse_int()` и
  `parse_int_radix(r)`) один с default-параметром. Упрощает API; radix=10
  — наиболее частый случай.
- **`compare` как примитив** — лексикографическое сравнение через `memcmp`;
  будущий `PartialOrd` auto-derive для `str` может опираться на него.

### C codegen mapping

| Nova method | C function |
|---|---|
| `str @to_bytes()` | `nova_str_to_bytes` |
| `str @to_chars()` | `nova_str_to_chars` |
| `str @compare(other)` | `nova_str_compare` |
| `str @split(sep)` | `nova_str_split` (unchanged) |
| `str @as_bytes()` | `nova_str_as_bytes` (D176) |

Legacy C aliases сохранены для совместимости кода, написанного до D178:
`nova_str_bytes` → `nova_str_to_bytes`, `nova_str_chars` → `nova_str_to_chars`.

### Связь

- [D102](02-types.md#d102-keyword-only-default-параметры) — keyword-only default params.
- [D176](02-types.md#d176-readonly-t--тип-модификатор) — `readonly` type modifier; `as_bytes()`.
- [D177](#d177-str-nova-body-dispatch--plan-54-ф2-extension) — Nova-body dispatch механизм.
- [Plan 91.5](../../docs/plans/91.5-str-api-cleanup.md) — sub-plan Ф.2.6 D178.

---

## D179. `StringBuilder` — pure Nova consume type — Plan 91 Ф.2.6

**Статус:** закрыт (Plan 91 Ф.2.6 sub-phase, 2026-05-28).

### Суть

`StringBuilder` перенесён из внешней реализации (C runtime / Rust String) в
чистый Nova-тип:

```nova
type StringBuilder consume {
    mut buf []u8
}
```

Все методы реализованы на Nova; единственный внешний примитив —
`buf.push(byte u8)` (добавление байта в backing array), UTF-8 encoding
реализован через Nova bitwise ops.

### API (финал D179)

```nova
// Конструкторы
StringBuilder.new()              -> Self   // pre-alloc 16 байт
StringBuilder.with_capacity(n)   -> Self   // pre-alloc n байт
StringBuilder.from(s str)        -> Self   // copy UTF-8 bytes
StringBuilder.from(c char)       -> Self   // UTF-8 encode одного codepoint

// Query
@len()       -> int   // байты O(1); аналог str.len (D26 school B)
@char_len()  -> int   // codepoints O(n) UTF-8 walk; новый метод
@capacity()  -> int   // allocated байты
@is_empty()  -> bool
@clone()     -> Self  // deep copy buffer

// Prefix/suffix check
@starts_with(prefix str) -> bool
@ends_with(suffix str)   -> bool

// Мутирующие (-> @, consume-тип — см. D131)
@append(s str)               -> @   // append UTF-8 bytes из str
@append(c char)              -> @   // append codepoint как UTF-8 (1-4 байта)
@append_bytes(ro arr []u8) -> @  // raw bytes; caller обеспечивает UTF-8
@append_repeat(s str, n int) -> @   // append s ровно n раз
@truncate(len int)           -> @   // обрезать буфер до len байт

// Операторы
@plus(s str) -> @   // sb + "text" → @append(s) (D46)
@plus(c char) -> @  // sb + c    → @append(c) (D46)

// Consume (финализация)
@to_str() -> str    // consume StringBuilder → str; infallible (UTF-8 invariant)
```

### Изменения относительно pre-109

| Было (до D179) | Стало (D179) |
|---|---|
| `external type StringBuilder` | `type StringBuilder consume { mut buf []u8 }` |
| `@byte_len() -> int` | удалён (дублировал `@len()`) |
| `@peek() -> str` | удалён (unsound: pointer aliasing с realloc) |
| `@into() -> str` | `@to_str() -> str` (consume) |
| `@append_bytes(arr []u8)` | `@append_bytes(readonly arr []u8)` |
| внешняя реализация C/Rust | чистый Nova-код |

### Инфраструктура

- `std/runtime/string_builder.nv` — Nova-реализация всех методов.
- `compiler-codegen/nova_rt/string_builder.h` — только UTF-8 helpers:
  `nova_str_from_bytes_unchecked`, `nova_str_from_bytes_lossy`,
  `Nova_str_static_try_from_bytes`, `Nova_str_static_from_char`,
  `nova_str_replace`. Старые `Nova_StringBuilder_*` функции удалены.
- `std/prelude/collections.nv` — `export import std.runtime.string_builder.{StringBuilder}`
  (было `external type StringBuilder`).
- `compiler-codegen/src/codegen/runtime_registry.rs` — `RUNTIME_DEFINED_TYPES` includes `"StringBuilder"`.
- `emit_c.rs` — `lhs_is_nova_ptr` guard: `sb + "str"` → `@plus` dispatch, не `nova_str_concat`.

### Связь

- [D131](03-syntax.md#d131-consume-types-и-fluent-api) — consume types и `-> @` fluent API.
- [D133](02-types.md#d133-consume-static-analysis) — consume static analysis.
- [D176](02-types.md#d176-readonly-t--тип-модификатор) — `readonly` parameter modifier.
- [D178](#d178-str-api-cleanup-и-расширения--plan-91-ф26) — `str.from_bytes_*` helpers.
- [Plan 91.6](../../docs/plans/91.6-stringbuilder-nova-type.md) — sub-plan Ф.2.6 sub-phase D179.

## D217. Method-local receiver field caching — Plan 123.1

**Source:** [Plan 123 umbrella](../../docs/plans/123-receiver-field-cse.md)
+ [Plan 123.1](../../docs/plans/123.1-core-cse.md). Implementation:
`compiler-codegen/src/field_cache.rs`.

### 1. Семантика — formal property

Для каждого input AST `A` и output AST `T(A)` — observable behavior
running `T(A)` **identical** to running `A`. «Observable» включает
stdout / panic / exit code / file system effects / network effects /
GC behavior. Pass — pure AST→AST трансформация без I/O и global
state.

**Пример** — типичная конверсия:

```nova
// До D217 преобразования (source AST):
fn ReadBuffer @try_read_u32_le() -> Result[u32, BufferError] {
    if @pos + 4 > @data.len() { return Err(...) }
    ro b0 = @data[@pos] as u32
    ro b1 = @data[@pos + 1] as u32
    ro b2 = @data[@pos + 2] as u32
    ro b3 = @data[@pos + 3] as u32
    @pos = @pos + 4
    Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
}

// После D217 преобразования (transformed AST):
fn ReadBuffer @try_read_u32_le() -> Result[u32, BufferError] {
    ro _at_data = @data    // ro → unconditional cache (D175 freeze).
    ro _at_pos = @pos      // mut → first-region cache (валидна до
                           //   первой write/call boundary).
    if _at_pos + 4 > _at_data.len() { return Err(...) }
    ro b0 = _at_data[_at_pos] as u32
    ro b1 = _at_data[_at_pos + 1] as u32
    ro b2 = _at_data[_at_pos + 2] as u32
    ro b3 = _at_data[_at_pos + 3] as u32
    @pos = _at_pos + 4   // write — _at_pos undefined after this point.
    Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
}
```

`.c` output до — `nova_self->data` × 5 + `nova_self->pos` × 5;
после — оба cached в локалы один раз, регистр-аллокатор C-компилятора
тривиально hoisted. **Net result:** `-O0` build стабильно быстрее на
hot-path methods (15-30% reduction в pointer derefs).

### 2. Heuristics

#### 2.1 Threshold N

Default `N=2`: cache emit'ится только если field accessed **≥2** раз
в method body. `N=0` → feature OFF (escape hatch). Tunable через env
vars (см. §5).

#### 2.2 ro field — unconditional cache

`RecordField.readonly == true` (D175 `ro` modifier):
- Frozen post-construction, no mutation возможно.
- Cache valid **across entire method body** — unaffected by calls,
  asserts, loops, branches.
- Single prefix `let _at_<F> = @<F>` emitted в body block start.
- All reads of `@<F>` replaced с `_at_<F>`.

#### 2.3 mut field — straight-line first-region cache

`RecordField.mutable == true` (или default без `ro`/`mut` modifier):
- Cache valid **от body start до first barrier**.
- **Barrier** = first top-level Stmt syntactically containing:
  - **Write to `@<F>`:** `Assign { target: Member{SelfAccess, F},
    op: AssignOp::*, ... }`. Includes compound (`+=`, `-=`, `*=`,
    `/=`).
  - **Any Call expression:** `ExprKind::Call`, `Spawn`, `Supervised`,
    `Detach`, `Blocking`, `With` (handler invoke), `Select` (channel
    op). V1 conservative — IPA / `#nofield_mut` annotations refine
    в Plan 123.7.
- Cache emitted при count reads-in-prefix-region ≥ threshold.
- Reads после boundary stay as direct `@<F>` (no re-cache в V1).
  Full multi-region recache — `[M-123.1-mut-region-recache]` P2
  followup.

### 3. Safety constraints

#### 3.1 Closure capture

Если **ANY closure body** (`ClosureLight` / `ClosureFull` / `Lambda`
/ `HandlerLit` / `ProtocolLit`) **syntactically references** `@<F>`
в method body — caching `F` skipped полностью. Closure может outlive
scope (stored handler, spawned fiber) и mutate `self`-pointer'ом
field через alias.

#### 3.2 Protocol / Effect / Opaque receivers

`Receiver.type_name` указывающий на `TypeDeclKind::Protocol`,
`Effect`, `Opaque`, `Alias`, `Newtype`, `Sum` → method skipped
entirely. Protocol — vtable dispatch (concrete impl unknown). Effect
/ Opaque — no record fields. Sum — variants accessed через pattern-
match. `NamedTuple` (D215) receivers — fields treated как ro
(stack value type, immutable post-construction).

#### 3.3 Generic monomorphization

Pass runs **после** type-check. Receiver type known + `RecordField`
classification из TypeDecl level (generic-agnostic). Codegen mono
pipeline downstream видит уже cached AST per instantiation.

#### 3.4 Consume / embed fields skipped

`RecordField.consume == true` (D131 linearity) — separate ownership
semantics. `is_embed == true` (`use _ Type` — D39) — auto-proxy
methods. Both skipped from registry.

#### 3.5 Static-method receivers + External fn skipped

`Receiver.kind == ReceiverKind::Static` — `fn Type.method(...)` без
`@self`. `FnDecl.is_external == true` — no body. Both skipped
explicitly.

### 4. Mangling — naming convention

Cache local = **`_at_<field>`** (D217 §4 baseline). При collision с
existing user local в fn scope — numeric suffix `_<N>`, где N — fn-
local counter (incremented only при actual collision; default case
keeps `_at_<F>` bit-stable across builds).

Examples:
- No collision: `ro _at_pos = @pos`.
- User has `ro _at_pos = 99` → cache renamed `ro _at_pos_1 = @pos`.
  User local untouched.

Collision detection — pre-pass scan всех `Stmt::Let` patterns +
`Pattern::Ident` / `Pattern::Binding` bindings во всём fn body +
params + closure-light/full params.

### 5. Escape hatch + tunables

Three environment variables (CLI-flag wiring через `--field-cache-*`
запланировано Plan 123.6 telemetry):

| Var | Default | Semantics |
|---|---|---|
| `NOVA_FIELD_CACHE` | (unset) | `0`/`off`/`false` → pass disabled. |
| `NOVA_FIELD_CACHE_THRESHOLD` | `2` | Min reads to cache. `0` → disabled. |
| `NOVA_FIELD_CACHE_MAX` | `8` | Cap cache locals per fn. |

Disabling — bypass точно identical к baseline AST output без pass.
**Verified differential testing** — full nova_tests/plan123_1 PASS
identically под ON и OFF (18/18 PASS обе configurations).

### 6. Debug-info preservation

`Span` каждого generated `let _at_F = @F` binding клонируется от
**first occurrence** of `@F` access в method body. DWARF / PDB emit
reflects это — debugger показывает `_at_F` local mapped к source
`@F` expression position.

V2 (Plan 123.5) — LSP code-lens над method header «N caches inserted»
+ hover «cached as `_at_F` from line X».

### 7. Edition compatibility

V1 (Plan 123.1) — enable **unconditionally** (semantic equivalence
guarantee достаточная; verified via 5 verification methods §1).
Future versions могут require edition opt-in если выявится unexpected
regression в production telemetry (Plan 123.6).

### 8. Cross-platform determinism

Pass deterministic by construction:
- Field names alphabetically sorted в `cache_fn` (HashMap iteration
  leak prevented).
- Per-fn counter reset → bit-stable cache local names across runs.
- No timestamp / random / system call в pass.

Same input AST → same output AST → same `.c` file (modulo platform-
specific runtime references).

### 9. Cross-references

- **D32** (semantics передачи параметров) — receiver semantics —
  Self pointer не aliased под managed-heap rule.
- **D52** (объявление типов) — `RecordField` declaration source.
- **D120** (`#pure` views + axioms) — pure annotation infrastructure
  для Plan 123.3 pure-call caching V3 future.
- **D131** (consume types) — linearity hints; consume fields skipped
  в V1, могут быть aggressive-cached в V2.
- **D175** (readonly field freeze) — ro field invariant — единственный
  unconditional-cache eligibility источник.
- **D176** (`readonly T` modifier) — orthogonal к D217 (parameter-
  level), но D175 + D176 вместе формируют immutability semantics.
- **D215** (named tuple) — `NamedTuple` fields treated как ro.

### 10. Implementation milestones — Plan 123 umbrella

| Version | Sub-plan | D-block | Status |
|---|---|---|---|
| **V1** | 123.1 (Core CSE) | **D217 (this)** | ✅ V1 active |
| V2 | 123.2 (LICM) | D218 (planned) | gate'нут на 123.1 ✅ |
| V3 | 123.3 (`#pure` cache) | D219 (planned) | gate'нут на 123.1 ✅ + D120 |
| V4 | 123.4 (chain) | D217 amend | gate'нут на 123.1 ✅ |
| V5 | 123.5 (LSP/diag) | D217 §6 amend | gate'нут на 123.1 + 104.x |
| V6 | 123.6 (telemetry) | (impl-only) | gate'нут на 123.1 ✅ |
| V7 | 123.7 (IPA) | D217 amend | gate'нут на all above |

### 11. Open Q resolution

- `Q-codegen-cse-semantics` (если откроется): → D217 (этот блок).
- `Q-debug-info-cache`: → D217 §6.
- `Q-cache-edition-gating`: → D217 §7 (V1 — no edition gate).

## D218. LICM — Loop-Invariant Code Motion для receiver fields — Plan 123.2

**Source:** [Plan 123.2](../../docs/plans/123.2-licm.md) sub-plan #2
Plan 123 umbrella. Implementation: `compiler-codegen/src/field_cache.rs`
LICM phase. Composes с D217 (Plan 123.1 V1 baseline).

### 1. Семантика — formal property

Для каждого input AST `A` содержащего loops L₁, L₂, ..., output
`T(A)` — observable behavior identical to `A`. Loop-invariant
`@<F>` reads inside loop body перемещены **immediately before**
the loop в enclosing Block.scope, с replacement reads inside body
на cache local ident `_at_<F>_loop` (or `_at_<F>_loop_<N>` при
collision).

**Пример:**

```nova
// До D218 (source):
fn Image @sum_pixels_for(n int) -> int {
    mut total = 0
    mut i = 0
    while i < n {
        total = total + @pixels + @pixels    // 2 reads of mut @pixels
        i = i + 1
    }
    total
}

// После D218 (LICM-hoisted):
fn Image @sum_pixels_for(n int) -> int {
    mut total = 0
    mut i = 0
    ro _at_pixels_loop = @pixels             // hoist immediately before loop
    while i < n {
        total = total + _at_pixels_loop + _at_pixels_loop
        i = i + 1
    }
    total
}
```

LICM benefit visible when D217 (Plan 123.1) **cannot** cache the
field at method-body prefix:
- Mut field accessed only inside loop; method body has Call before
  loop → D217 mut-prefix region empty → no cache. D218 LICM hoists
  immediately before loop scope.
- Method body has mixed access patterns where D217's first-region
  bailout leaves loop body uncached.

### 2. Composition с D217 (Plan 123.1)

**Order:** D218 LICM phase runs **BEFORE** D217 per-fn ro/mut caching
(см. `cache_module` в field_cache.rs).

Rationale:
- LICM hoists invariant reads из loops; replaces reads inside loop
  с `_at_<F>_loop` ident.
- D217 then walks the body; `@<F>` reads inside loop are already
  replaced — counted only reads OUTSIDE loops для method-body prefix
  cache decision.
- No double-cache: if D217 also caches `@<F>` at method-body prefix,
  the loop body reads are already `_at_<F>_loop` ident (don't match
  `@F` pattern). Both cache locals coexist.

**Result:** для ro fields с reads only inside loop, the hoisted
`_at_<F>_loop` typically suffices (D217 sees zero `@<F>` accesses
outside loop — below threshold). Для ro fields с reads inside AND
outside loop, two separate caches emitted — one at method-body
prefix (D217) and one immediately before loop (D218). Stack-frame
growth bounded by `max_per_fn=8` total cap.

### 3. Eligibility rules per loop body

For each field `F` and each loop body Block:

1. **Read count:** `count_field_reads_in_block(body, F) ≥
   licm_threshold` (default 2).
2. **No mutation:** `!block_contains_write_to(body, F)` —
   no `Assign { target: Member{SelfAccess, F}, ... }` anywhere in
   body. Includes compound assigns (`+=`, etc.) и nested control flow.
3. **No closure capture:** `!collect_closures_captures_in_block(body, F)`
   — no closure body inside loop body references `@F`. (Closure body
   syntactic detection; conservative.)
4. **No spawn / supervised / detach / blocking / parallel-for:**
   loop body must not contain concurrent constructs (aliasing
   safety with concurrent fibers).
5. **For mut fields:** loop body must NOT contain any Call (V2
   conservative — IPA refines в Plan 123.7).
6. **For ro fields:** Call в body OK (frozen — no aliasing).

### 4. Loop forms supported

- `for pattern in iter { body }` — D-foreach standard for.
- `while cond { body }`.
- `loop { body }` — D-loop infinite + break.
- `while let pat = expr { body }` — D34.

**Excluded:**
- `parallel for pat in iter { body }` — D14, concurrent body.
- Loops nested внутри `Spawn` / `Supervised` / `Detach` / `Blocking`
  / `Select` channel-op contexts.

### 5. Hoisting placement

Hoisted `ro _at_<F>_loop = @<F>` Stmt::Let inserted **immediately
before** the loop expression в enclosing Block.stmts. Placement
rules:

- **Loop as `Stmt::Expr` в Block.stmts:** hoist inserted at same
  index, loop stmt pushed after.
- **Loop as Block.trailing:** hoist appended к Block.stmts (loop
  remains trailing).
- **Loop as FnBody::Expr (whole-body loop):** body coerced к
  Block-with-trailing, hoist inserted at start.
- **Loop nested внутри expression** (e.g. `if cond { for ... }`):
  hoist inserted into innermost enclosing Block.

### 6. Naming convention

Cache local = **`_at_<field>_loop`** (D218 §6 baseline). Distinct
от D217 `_at_<field>` для clarity в debug-info — каждое имя
объясняет cache origin: `_at_X` = method-body cache; `_at_X_loop`
= LICM hoist.

Collision avoidance: numeric suffix `_<N>` если имя уже occupied
user-local OR другим LICM hoist в same fn scope.

### 7. Escape hatch + tunables

| Var | Default | Semantics |
|---|---|---|
| `NOVA_FIELD_CACHE_LICM` | (unset) | `0`/`off`/`false` → LICM disabled. D217 unaffected. |
| `NOVA_FIELD_CACHE_LICM_THRESHOLD` | `2` | Min reads inside loop body. `0` → LICM disabled. |
| `NOVA_FIELD_CACHE_LICM_MAX` | `4` | Cap hoists per loop. |
| `NOVA_FIELD_CACHE` (D217) | (unset) | `0` → disables BOTH D217 and D218 (umbrella escape hatch). |

Verified differential testing: 14/14 plan123_2 PASS identically под
`NOVA_FIELD_CACHE_LICM=0` и default ON. Semantic equivalence
guaranteed.

### 8. Debug-info preservation

`Span` каждого hoisted let клонируется от **first occurrence** of
`@F` access в loop body. DWARF/PDB emit reflects это — debugger
показывает `_at_<F>_loop` local mapped к source `@<F>` expression
position inside loop body.

### 9. Cross-references

- **D217** (Plan 123.1, V1) — baseline CSE pass; composition
  partner.
- **D14** (ParallelFor) — concurrent body, LICM skip.
- **D50** (structured concurrency — Spawn/Supervised) — LICM skip
  bodies that contain these.
- **D131** (consume types) — consume fields skipped (D217 §3.4
  inherits).
- **D175** (readonly field freeze) — ro semantics; aliasing-safe
  invariance.

### 10. Implementation milestones

- **V2** Plan 123.2 ✅ (this block).
- **V3-V7** — Plan 123.3 (pure-call cache) / 123.4 (chain) / 123.5
  (LSP) / 123.6 (telemetry) / 123.7 (IPA) — orthogonal extensions.

### 11. Open Q resolution

- `Q-licm-correctness`: → D218 §1 + §3 (formal property + eligibility
  rules ensure correctness).
- `Q-licm-composition-with-D217`: → D218 §2 (order LICM → D217).

## D219. Pure-call result caching (effect-aware, Nova edge) — Plan 123.3

**Source:** [Plan 123.3](../../docs/plans/123.3-pure-call-cache.md)
sub-plan #3 Plan 123 umbrella. Implementation: `field_cache.rs`
pure-cache phase. Composes с D217 (V1) + D218 (V2).

### 1. Семантика — formal property

Для каждого input AST `A` содержащего multiple invocations of
`@<pure_method>()` (where method has `Purity::Pure` per D24
infrastructure), output `T(A)` — observable behavior identical
to `A`. Pure-call result evaluated once per method body, cached в
local `_at_<method>_call`, replaced на cache ident в subsequent
call sites.

Nova-edge — leverages effect system: `#pure` annotation guarantees
no effects, no side effects, deterministic result (depends only
on self's state).

**Пример:**

```nova
// До D219 (source):
#pure
fn Vec3 @magnitude_sq() -> int => @x * @x + @y * @y + @z * @z

fn Vec3 @double_test() -> int {
    @magnitude_sq() + @magnitude_sq()   // 2 pure-method calls
}

// После D219 (cached):
fn Vec3 @double_test() -> int {
    ro _at_magnitude_sq_call = @magnitude_sq()  // single eval
    _at_magnitude_sq_call + _at_magnitude_sq_call
}
```

`.c` output до — `Nova_Vec3_method_magnitude_sq(nova_self)` × 2;
после — single call, register-reuse через cache local.

### 2. Composition с D217 + D218

**Order:** D218 LICM → **D219 pure-cache** → D217 per-fn cache.

Rationale:
- D218 LICM hoists loop-invariant @F reads. Pure-call args (V3
  args-less) — none, so LICM unaffected.
- D219 caches @<pure_method>() result; replaces calls с Ident.
- D217 sees pure-cache locals as Idents (not @F pattern). Continues
  to cache @F reads outside pure-call args.

No double-cache risk — three layers operate on distinct AST
patterns (LICM: @F in loop; D219: @M() pure call; D217: @F method-
body prefix).

### 3. Eligibility rules per fn body

For each fn body (instance method with non-protocol receiver):

1. **Body must NOT have any `@F = ...` write** (conservative
   invalidation). Refined V3.1 с D24 `f.reads` frame info.
2. **Body must NOT contain concurrent constructs:** Spawn,
   Supervised, Detach, Blocking, ParallelFor — skip whole pure-
   cache for safety.
3. **Method registry lookup:** `(receiver_type, method_name)` must
   be в pure_methods registry. Registry includes only methods с
   `purity == Purity::Pure` (D24 — annotated `#pure` OR inferred
   через SCC) AND `Instance` receiver AND `params.is_empty()` (V3
   scope; V3.1 — args-with-literals).
4. **Count occurrences:** call count ≥ `pure_threshold` (default 2).
5. **Closure capture exclusion:** if `@<method>()` appears inside
   nested closure body, that call counts toward closure_captured
   set, excluded from caching.

### 4. V3 scope (DECISION-A.3 / E.3)

**Included:**
- `Call { func: Member{SelfAccess, name: M}, args: [] }` — args-less
  self-method calls.

**Excluded (Plan 123.4/V3.1/Plan 123.7 territory):**
- Pure calls с arguments (V3.1).
- Pure calls на `@field.method()` chains (Plan 123.4 chain cache).
- Pure calls на parameters / locals.
- Effectful methods (`Purity::Effectful` / `Purity::Unknown`).
- Consume-returning pure methods (D131 linearity — skip).

### 5. Naming convention

Cache local = **`_at_<method>_call`** baseline. Distinct от:
- D217: `_at_<field>` (per-fn field cache).
- D218: `_at_<field>_loop` (LICM hoist).

Collision avoidance: numeric suffix `_<N>`.

### 6. Escape hatch + tunables

| Var | Default | Semantics |
|---|---|---|
| `NOVA_FIELD_CACHE_PURE` | (unset) | `0`/`off`/`false` → V3 disabled. D217/D218 unaffected. |
| `NOVA_FIELD_CACHE_PURE_THRESHOLD` | `2` | Min pure-call count. `0` → disabled. |
| `NOVA_FIELD_CACHE` (D217) | (unset) | `0` → disables all 3 layers. |

Verified differential testing: 12/12 plan123_3 PASS identically
под `NOVA_FIELD_CACHE_PURE=0` и default ON.

### 7. Conservative invalidation rationale

V3 simple rule: ANY `@F = ...` write anywhere в method body skips
all pure-cache. Rationale: without frame info, mutation may affect
pure method's result transitively. Pure method reads `@F` (typical
pure method depends on fields); mutating any field may change
result.

**V3.1 refinement (followup `[M-123.3-frame-based-invalidation]`):**
Use D24 `f.reads` frame information to determine which fields
each pure method reads. Cache valid until any of those fields
written. Allows mut field's write to NOT invalidate unrelated
pure methods. Marked P2 followup.

### 8. Debug-info preservation

Span каждого generated `let _at_<method>_call = @<method>()`
binding клонируется от **first occurrence** of `@<method>()` call.
Debugger показывает cache origin maps к source position.

### 9. Cross-references

- **D24** (Plan 33.1 + 33.2) — Purity infrastructure (`#pure`
  annotation + SCC inference). V3 leverages `FnDecl.purity` field.
- **D217** (Plan 123.1, V1) — per-fn ro/mut field cache. Composition
  partner; D219 runs between D218 LICM и D217.
- **D218** (Plan 123.2, V2) — LICM hoisting. Composition partner.
- **D120** (`#pure` views + axioms) — semantic foundation; pure
  methods have no effects, no mutation, deterministic.
- **D131** (consume types) — consume-returning pure methods skipped.

### 10. Implementation milestones

- **V3** Plan 123.3 ✅ (this block) — args-less self-pure-call.
- **V3.1** followups: `[M-123.3-args-literals]` (cache calls с
  literal args), `[M-123.3-frame-based-invalidation]` (D24 reads
  frame).
- **V4** Plan 123.4 chain cache — orthogonal extension.

### 11. Open Q resolution

- `Q-pure-call-cache`: → D219 (this block).
- `Q-pure-call-mutation-invalidation`: → D219 §3 + §7 (V3
  conservative; V3.1 refined).

## D217 amend V4 — Chain caching `@a.b.c` (Plan 123.4)

**Source:** [Plan 123.4](../../docs/plans/123.4-chain-cache.md)
sub-plan #4 Plan 123 umbrella. Extends D217 caching infrastructure
to nested chain access patterns. Implementation: `field_cache.rs`
chain-cache phase (D217 amend rather than NEW D-block because
chain extension preserves D217 semantic foundation, just extends
to multi-segment paths).

### 1. Семантика — V4 extension

For chain access pattern `Member { ... Member { obj: SelfAccess,
name: A }, name: B }` (= `@a.b`) with chain length 2..=
`chain_max_depth` (default 4), cache emitted при ≥
`chain_threshold` (default 2) occurrences. Replaces all matching
chain expressions с cache local Ident.

**Пример:**
```nova
// До D217 V4 (source):
fn Outer @sum_with_chain() -> int {
    @inner.value + @inner.value + @inner.value + @mark
}

// После D217 V4:
fn Outer @sum_with_chain() -> int {
    ro _at_inner_value_chain = @inner.value
    _at_inner_value_chain + _at_inner_value_chain + _at_inner_value_chain + @mark
}
```

### 2. Composition order — three-layer + V4

Order в `cache_module`:
1. D218 LICM phase.
2. **D217 V4 chain phase** (NEW).
3. D219 pure-call phase.
4. D217 V1 per-fn cache phase.

Rationale:
- LICM hoists single-field @F reads из loops first.
- Chain caching emits multi-segment chain locals; replaces chain
  expressions с Idents.
- D219 pure-cache then handles `@<pure_method>()` calls (chains
  inside pure-method receivers already cached).
- D217 V1 final fills in remaining @F single-field caches.

All four layers emit distinct cache local naming, no shadowing risk:
- D217 V1: `_at_<F>`
- D218 LICM: `_at_<F>_loop`
- D219 pure: `_at_<M>_call`
- **D217 V4 chain: `_at_<a>_<b>[_<c>[_<d>]]_chain`** (NEW)

### 3. Eligibility (V4)

1. **Chain length:** 2 ≤ depth ≤ `chain_max_depth` (default 4).
   Single-field (depth 1) handled by D217 V1 baseline; deeper than
   4 → skip (stack-frame bloat protection).
2. **Occurrence count:** identical canonical path ≥ `chain_threshold`.
3. **No top-level @F write anywhere в body** (V4 conservative;
   future V4.1 may refine per-segment).
4. **No concurrent body:** Spawn/Supervised/Detach/Blocking/
   ParallelFor → skip.
5. **No closure capture:** chain in closure body → excluded from
   caching.
6. **Receiver type known:** not Protocol/Effect/Opaque/etc.

### 4. Critical detection rule — method dispatch ≠ chain

`@a.b.method()` — Member{obj: @a.b, name: "method"} is NOT a chain
of length 3 (`method` is method-dispatch name, not field).

Implementation detail: when traversing `ExprKind::Call`, recurse
into `func.obj` (the receiver) not into `func` itself. Same fix
applied in both `count_chains_in_expr` и `rewrite_chains_in_expr`.

Verified through fixture failure during V4 implementation —
StringBuilder.append__nova_char attempted to chain-cache
`@buf.push` (push is array method); fix corrected immediately.

### 5. Naming

Cache local = **`_at_<a>_<b>[_<c>[_<d>]]_chain`** для path components
`[a, b, c?, d?]`. Joined with underscores. Suffix `_<N>` при
collision.

Examples:
- `@inner.value` → `_at_inner_value_chain`.
- `@parent.inner.cfg.limit` → `_at_parent_inner_cfg_limit_chain`.

### 6. Escape hatch + tunables

| Var | Default | Semantics |
|---|---|---|
| `NOVA_FIELD_CACHE_CHAIN` | (unset) | `0`/`off`/`false` → V4 disabled. |
| `NOVA_FIELD_CACHE_CHAIN_THRESHOLD` | `2` | Min chain occurrences. |
| `NOVA_FIELD_CACHE_CHAIN_DEPTH` | `4` | Max chain depth (≥2 enforced). |
| `NOVA_FIELD_CACHE` | (unset) | `0` → disables all 4 layers. |

Verified: 10/10 plan123_4 PASS identically под default и
`NOVA_FIELD_CACHE_CHAIN=0`.

### 7. Risk register (V4-specific)

- **R-4.1:** stack-frame bloat (many chain caches per fn).
  Mitigation: `max_per_fn=8` cap shared across all 4 layers.
- **R-4.2:** chain через mut intermediate field could theoretically
  invalidate. Mitigation: V4 conservative — any @F write in body
  skips all chain caching.
- **R-4.3:** method-dispatch confusion. Mitigation: detection rule
  §4.

### 8. Future extensions

- **V4.1 followups:**
  - `[M-123.4-per-segment-invalidation]` — refine invalidation
    via D24 `f.reads` frame info.
  - `[M-123.4-chain-prefix-sharing]` — cache shared prefixes
    (e.g. `@a.b` + `@a.b.c` share `@a.b` intermediate).
- **V7 IPA (Plan 123.7)** — enables cross-method analysis для
  chain invalidation precision.

### 9. Cross-references

- **D217 V1** (Plan 123.1) — baseline single-field cache.
- **D218** (Plan 123.2) — LICM. Chain caching composes after LICM.
- **D219** (Plan 123.3) — pure-call. Chain caching composes before
  pure-call (chain ID resolution must complete before pure-call's
  self.method() detection).
- **D52** (record types) — chain fields must be record-typed at
  each level.

