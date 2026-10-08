<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# План 294 — std: потоки дочернего процесса, управление процессом, PTY

**Статус:** 📝 ЗАПЛАНИРОВАН 2026-10-08 — этап 1 (изучение и план), реализация НЕ начата. Слово владельца
2026-10-08: сервис запуска кодовых агентов переносится с Go на Nova, ему нужны эти три вещи. Номер плана — 294
(последний на `main` — [293](293-consume-pattern-auto-move.md)); номер D-блока не выбран, см. «Открытые вопросы».

**Родитель по теме:** [176](176-io-fs-os.md) (umbrella io/fs/os); продолжает D453 (Plan 265 Ф.1) — тот самый
«DEFERRABLE следующей волной», где D453 прямо оставил `spawn()` + стриминг. Реализации пока нет: ниже — карта,
предложение API, этапы, риски, приёмка.

## 1. Карта текущего состояния (всё прочитано в дереве, не по памяти)

| что | где | что даёт | чего не хватает |
|---|---|---|---|
| Запуск процесса | `Os.process_run` — [std/src/os/effect.nv](../../std/src/os/effect.nv), последняя операция эффекта | «спавн + дождаться + код»; `(rc, exit_code)`; `rc == PROCESS_CANCELLED` при отмене scope | ни дескриптора, ни pid, ни потоков: «одним вызовом, паркуя волокно» |
| Обёртки | `Command` / `ExitStatus` — [std/src/os/os.nv](../../std/src/os/os.nv) (`Command.new/arg/args/env/env_clear/dir/run`, `ExitStatus.code/success`) | builder-значение, дедупликация env, `Err(IoError)` для запуска/отмены | `ExitStatus` хранит только `code`: сигнал смерти теряется (`128+signal` — склейка в C); нет `stdin/stdout/stderr`, `spawn`, `kill`, `wait` |
| C-основа | [compiler-codegen/nova_rt/process.c](../../compiler-codegen/nova_rt/process.c), [process.h](../../compiler-codegen/nova_rt/process.h): `os_process_run` на `uv_spawn` | паттерн park/wake + `nova_sched_register_pending` + stop_cb (убить, `NOVA_STOP_ASYNC`); `stdio_count = 0` → потомок получает null-устройство; `req->killing` как сигнал отмены для прямого тела `supervised` | `uv_pipe_t` / `uv_tty_t` в `nova_rt` НЕ используются вовсе (`grep uv_pipe` по `*.c`/`*.h` — пусто); убивается только сам процесс (`uv_process_kill(SIGKILL)`), не дерево |
| Образец долгоживущего дескриптора | `TcpStream` — [std/src/net/tcp.nv](../../std/src/net/tcp.nv) + `net.c` (`net_tcp_read`, `net_tcp_write`) | read/write паркуют волокно, **независимые слоты** чтения и записи (дуплекс), читаем прямо в буфер вызывающего (zero-copy, alloc_cb), `share()`/`split()`/`consume close`/`Cleanup` | это сокет, а не пайп/PTY; повторять приём, не изобретать |
| Протоколы ввода-вывода | `Read`/`Write` — [std/src/io/core.nv](../../std/src/io/core.nv); `read_to_end`, `copy`, `write_all` | `Ok(0)` = EOF только при непустом буфере; частичная запись легальна | — готовы, потоки процесса должны им соответствовать структурно |
| Ошибки | `IoError` + открытый `ErrorKind` — [std/src/io/error.nv](../../std/src/io/error.nv) | `NotFound`, `PermissionDenied`, `NotADirectory`, `BrokenPipe`, `Interrupted`, `TimedOut`, `Unsupported`, `Other(int)` + `raw_os` | вариантов достаточно, новых не нужно (см. таблицу отказов в п. 6) |
| Консоль самого процесса | [std/src/io/console.nv](../../std/src/io/console.nv): эффект `Io`, `stdin()/stdout()/stderr()` | подменяемый `mock_io` | к потомку отношения не имеет; не путать |
| Отмена | `supervised(timeout:/deadline:/cancel:)` (D93, D439, №165) — [std/src/concurrency/supervised_deadline_test.nv](../../std/src/concurrency/supervised_deadline_test.nv) | срок и отмена на уровне scope; `Duration` — [std/src/time/duration/core.nv](../../std/src/time/duration/core.nv) | отмена убивает ОДИН процесс, внуки живут |
| Спека | D453 — [spec/decisions/04-effects.md](../../spec/decisions/04-effects.md), раздел «D453» | «сигнатура `process_run` задним числом не расширяется; `spawn()` — отдельная волна, нужен D-амендмент» | это и есть решение п. 2 |
| SIGPIPE | `nova_driver_init` в [compiler-codegen/nova_rt/driver.c](../../compiler-codegen/nova_rt/driver.c) (№664) | `SIGPIPE` игнорируется → запись в закрытый пайп даёт `EPIPE`, а не смерть | на Windows аналога нет — свой отказ (`ERROR_BROKEN_PIPE`) |
| CI | `.github/workflows/*.yml` | все задания на `ubuntu-latest` / self-hosted Linux; **Windows-задания нет** (`grep windows` — только `contracts-z3.yml`), macOS нет | проверка Windows-половины не получает автоматического вердикта — риск R9 |

Вывод: Nova сегодня умеет «запустить и подождать». Нет ни одного из трёх: потоков (ни pipe-обёртки в рантайме,
ни `Child`), управления (pid, сигнал, дерево), PTY (нужен свой fork/ConPTY, `uv_spawn` его не умеет).

## 2. Совместимость с D453 — решение плана

**Расширять, не заменять.** `Command.run()` и `Os.process_run` остаются как есть (одним вызовом, для скриптов и
для `mock_os()`-тестов обёрток над `git`). Всё новое — **новый подключаемый эффект `Proc`** (plumbing-эффект по
образцу `Net`/`Os`: пользователь зовёт `Command.spawn`, а не операции эффекта; в тестах ставится `mock_proc(...)`).
Причина не вкус: D453 запретил расширять `process_run` задним числом, а дескриптор (`*()`) в `Os` нести нельзя —
`Os` «тонкий», без хендлов. `Command` получает новые методы-настройки и три точки входа
(`spawn`/`output`/`spawn_pty`), остальные его методы не меняются. `ExitStatus` дополняется полем `signal` (аддитивно,
конструируется только в `os.nv`).

## 3. Предложение API (сигнатуры — набросок формы; точная проверка `nova check` — первым шагом Ф.1)

Принципы: ошибки значениями (`Result[_, IoError]`), никакого глобального состояния (всё в дескрипторе и в
эффекте `Proc`), волокно паркуется, поток OS не блокируется, одна форма на Windows и POSIX; различия — в п. 3.4.

### 3.1. Часть 1 — потоки дочернего процесса

```nova
module std.os

export type Stdio enum | Null | Inherit | Piped         // по умолчанию: stdin Null, stdout/stderr Null (как D453)

// новые настройки Command (value-builder, как OpenOptions: каждый вызов — копия)
export fn Command mut @stdin(s Stdio) -> @
export fn Command mut @stdout(s Stdio) -> @
export fn Command mut @stderr(s Stdio) -> @
export fn Command mut @stderr_to_stdout() -> @          // один общий пайп, порядок байтов сохраняется

export type Child consume value priv { handle *(), rc *mut AtomicInt }
export type ChildStdin  consume value priv { handle *() }   // io.Write
export type ChildStdout consume value priv { handle *() }   // io.Read
export type ChildStderr consume value priv { handle *() }   // io.Read

export fn Command @spawn() Proc -> Result[Child, IoError]
export fn Child @pid() -> int
export fn Child mut @take_stdin()  -> Option[ChildStdin]    // Some один раз; None, если не Piped
export fn Child mut @take_stdout() -> Option[ChildStdout]
export fn Child mut @take_stderr() -> Option[ChildStderr]
export fn Child mut @wait() Proc -> Result[ExitStatus, IoError]           // паркует волокно
export fn Child mut @try_wait() Proc -> Result[Option[ExitStatus], IoError]
export fn Child consume @cleanup(outcome ScopeOutcome) -> ()             // D188: не ждёт — гасит (см. 3.2)

export fn ChildStdin  mut @write(data []u8) Proc -> Result[int, IoError]  // io.Write; частичная запись легальна
export fn ChildStdin  mut @flush() -> Result[(), IoError]                  // no-op, как у TcpStream
export fn ChildStdin  consume @close() Proc                               // закрыть = EOF потомку
export fn ChildStdout mut @read(mut buf []u8) Proc -> Result[int, IoError] // io.Read; Ok(0) = EOF
export fn ChildStdout consume @close() Proc                               // (то же у ChildStderr)

export type Output value { ro status ExitStatus, ro stdout []u8, ro stderr []u8 }
export fn Command @output() Proc -> Result[Output, IoError]               // читает оба потока ПАРАЛЛЕЛЬНО (два волокна)
```

Правила, которые фиксирует D-блок:

* `ChildStdout/Stderr/Stdin` — `consume`-значения с `Cleanup` (образец `TcpStream`): закрытие идемпотентно по
  владению, утечка дескриптора невозможна в `consume`-блоке. Чтение и запись разных потоков идут из разных
  волокон (независимые слоты парковки, как у `TcpReadHalf/WriteHalf`), поэтому «пишу в stdin, читаю stdout»
  не требует `select`.
* `read` кладёт байты прямо в буфер вызывающего (`uv_read_start` + alloc_cb, как `net_tcp_read`), и
  `uv_read_stop` между вызовами: читатель ТЯНЕТ, пока он не пришёл за данными, потомок упирается в полный пайп —
  это и есть **давление назад** на стороне чтения без собственных очередей.
* `write` паркуется до завершения `uv_write`; медленный потомок тормозит ПИСАТЕЛЯ, не планировщик.
* `Command.output()` — единственное место, где библиотека сама читает два пайпа; делает это параллельными
  волокнами внутри `supervised`, иначе большой `stderr` при читаемом `stdout` — взаимная блокировка (R3).
* `wait()` после отмены scope: потомок убит (по правилам п. 3.2), `Err(IoError{Interrupted})` — тот же
  выбор, что у D453.

### 3.2. Часть 2 — управление процессом (дерево, таймаут, сигнал)

```nova
export type Signal enum | Interrupt | Terminate | Kill | Hangup | Other(int)

export type Tree enum | Single | Group       // Group: потомок — лидер своей группы/задания; kill бьёт по всему дереву
export fn Command mut @tree(t Tree) -> @      // по умолчанию Single (поведение D453)

export fn Child mut @kill(sig Signal) Proc -> Result[(), IoError]         // Ok и для уже завершившегося (идемпотентно)
export fn Child mut @stop(grace Duration) Proc -> Result[ExitStatus, IoError]   // Terminate → ждать grace → Kill
export fn Child mut @wait_timeout(d Duration) Proc -> Result[Option[ExitStatus], IoError]   // None = срок вышел, процесс жив

export fn kill_pid(pid int, sig Signal, tree bool) Proc -> Result[(), IoError]   // по голому pid, без Child
```

* Таймаут «на дерево»: `supervised(timeout: d) { child.wait() }` — отмена scope вызывает stop_cb, а stop_cb
  теперь убивает **дерево**, если процесс запущен с `Tree.Group` (иначе — как в D453, один процесс). Отдельный
  `wait_timeout` — удобство для кода без `supervised`; он НЕ убивает сам (решает вызывающий).
* `Child.cleanup(outcome)` — при выходе из scope живой потомок гасится как `stop(grace)` с короткой дефолтной
  паузой (константа в D-блоке, не настройка в рантайме); `wait()` уже вызванный — ничего не делает. Причина:
  не оставлять осиротевших агентов при `panic`/отмене — главное требование сервиса запуска агентов.
* `kill_pid(pid, sig, tree)` — для pid, которым владеет не этот `Child` (перезапуск сервиса, pid из файла):
  best-effort, `NotFound` при отсутствии, **риск повторного использования pid** описан в D-блоке (R6).
* Сигнал `Kill` без `Group` убивает один процесс; `Group` обязателен для «дерева» — решение явное, не магия.

### 3.3. Часть 3 — PTY

```nova
export type PtySize value { ro rows int, ro cols int }    // пиксельные размеры не нужны сервису; можно добавить позже без слома

export fn Command @spawn_pty(size PtySize) Proc -> Result[PtyChild, IoError]

export type PtyChild consume value priv { handle *(), rc *mut AtomicInt }
export fn PtyChild mut @read(mut buf []u8) Proc -> Result[int, IoError]    // io.Read: объединённый вывод; Ok(0) = потомок закрыл терминал
export fn PtyChild mut @write(data []u8) Proc -> Result[int, IoError]      // io.Write: ввод «с клавиатуры»
export fn PtyChild mut @flush() -> Result[(), IoError]
export fn PtyChild mut @resize(size PtySize) Proc -> Result[(), IoError]   // TIOCSWINSZ / ResizePseudoConsole; потомок получает SIGWINCH / событие размера
export fn PtyChild @pid() -> int
export fn PtyChild mut @wait() Proc -> Result[ExitStatus, IoError]
export fn PtyChild mut @kill(sig Signal) Proc -> Result[(), IoError]       // как у Child, дерево всегда Group
export fn PtyChild consume @close() Proc                                   // закрыть терминал: SIGHUP потомку / ClosePseudoConsole
export fn PtyChild consume @cleanup(outcome ScopeOutcome) -> ()
```

Единый смысл, одинаковый на обеих ОС: **один дуплексный байтовый канал + размер окна + жизнь потомка**.
`PtyChild.read`/`write` — независимые слоты парковки (читатель и писатель в разных волокнах).

### 3.4. Различия Windows / POSIX (в D-блок и в доки — явной таблицей)

| вопрос | POSIX (Linux, macOS) | Windows |
|---|---|---|
| запуск с потоками | `uv_spawn` + `uv_pipe_t` (`UV_CREATE_PIPE`) | то же (libuv именованные пайпы); 4 КБ-буфер пайпа — быстрее упирается, давление назад заметнее |
| дерево | `setsid`/`setpgid` + `kill(-pgid, sig)` | Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), `TerminateJobObject` |
| `Interrupt`/`Terminate` | настоящие `SIGINT`/`SIGTERM` | нет сигналов: `CTRL_BREAK_EVENT` потомку в своей группе процессов (best-effort), иначе `Err(Unsupported)`; `Terminate` без этого эквивалентно `Kill` — **явно названо**, не маскируется |
| код смерти по сигналу | `ExitStatus.signal = Some(n)`, `code` = 128+n | `signal = None`; `code` = код завершения (в т.ч. `0xC000013A` при Ctrl-C) |
| PTY | `openpty`/`forkpty`: `setsid` + `TIOCSCTTY` в потомке, мастер — `uv_pipe_open` | ConPTY: `CreatePseudoConsole` + `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`; Windows 10 1809+; на старшем — `Err(Unsupported)` |
| EOF PTY | чтение мастера после выхода потомка даёт `EIO` (Linux) → переводится в `Ok(0)` | выходной пайп закрывается после `ClosePseudoConsole` + выхода потомка |
| эхо и перевод строк | дисциплина линии ядра: эхо, `ICRNL`, `ONLCR` (`\n` → `\r\n`) | ConPTY делает своё: эхо и `\r\n` аналогично, но с лишними служебными последовательностями (R8) |
| `Ctrl-C` | байт `0x03` в мастер → `SIGINT` группе переднего плана | `0x03` в ввод → `CTRL_C_EVENT` процессам консоли |

## 4. Этапы

Все этапы — на двух ОС; «Windows» в проверке этапа = прогон у владельца/в окне на Windows-машине до появления
Windows-задания CI (риск R9). Каждый этап — один слитый кусок с фикстурами; зелёный CI — вердикт.

**Ф.0. Разведка-прототипы и спека (до кода std).** Три маленьких C-пробы вне дерева std (в `scratch`/пробном каталоге
окна): (а) `uv_spawn` с `UV_CREATE_PIPE` на stdout, большой вывод, `uv_read_stop` — давление назад на Linux и
Windows; (б) дерево: `setsid` + `kill(-pgid)` на Linux; Job Object **поверх libuv** на Windows — важный вопрос,
вставка `AssignProcessToJobObject` после `uv_spawn` имеет окно гонки (внук успевает появиться) → если проба это
показывает, Windows-спавн с `Tree.Group` идёт своим `CreateProcess(CREATE_SUSPENDED)` и `uv_pipe_open`; (в) PTY:
`forkpty` на Linux, `uv_pipe_open` мастера и чтение через `uv_read_start`; ConPTY на Windows. Плюс D-блок
(`spec/decisions/`, черновик п. 7) и страница обзора — **спека до кода**.
*Приёмка Ф.0:* записка в `docs/dev/` с замерами проб (какие допущения плана подтверждены, какие нет); D-блок
влит с номером интегратора. **Приёмка глазами, потому что** это решение по дизайну (путь Job Object при гонке; macOS).

**Ф.1. Потоки: `Proc`, `Stdio`, `Child`, `ChildStdin/Stdout/Stderr`, `spawn`, `wait`, `output`.**
`nova_rt/process.c` (или новый `proc.c`, тот же libuv-гейт и сборочные списки, что у `process.c` в
`compiler-codegen/src/test_runner.rs`): структура дескриптора с `uv_process_t` + до трёх `uv_pipe_t`, независимые
слоты чтения/записи, `exit_cb` ждётся отдельно от закрытия потоков (порядок «процесс завершился, но stdout ещё не
дочитан» — штатный). `std/src/os/proc.nv` + `proc_ffi.nv` + `mock_proc`. `ExitStatus.signal`.
*Тесты:* `std/src/os/proc_*_test.nv` рядом с модулем; потомок — сам `nova`-собранный помощник-бинарь в тестовом
каталоге (не `cat`/`sh`: их нет на Windows) — программа печатает по аргументу: эхо stdin, N байт вывода, код
возврата, «спать и писать частями», «закрыть stdout и жить». *Проверка:* Linux — CI; Windows — прогон вручную.

**Ф.2. Управление: `kill`, `stop`, `wait_timeout`, `Tree.Group`, `kill_pid`, `Cleanup`.**
POSIX: группа процессов. Windows: Job Object по результату Ф.0. stop_cb, учитывающий `Tree`.
*Тесты:* потомок порождает внука, оба пишут в файл-метку; убитое дерево — оба мертвы; таймаут `supervised` убивает
оба; `kill_pid` по pid без `Child`. Windows — тот же набор.

**Ф.3. PTY на POSIX** (`forkpty`, `resize`, EOF, `close`). Первым — POSIX: CI целиком на Linux, значит единственный
этап, где PTY-семантика получает автоматический вердикт; она же задаёт форму API, под которую подгоняется ConPTY, а
не наоборот. Свой `fork`+`exec` в многопоточном процессе (Boehm GC, M:N-планировщик): в потомке до `exec` — только
async-signal-safe вызовы, всё подготовлено заранее (R5).
*Тесты:* п. 5, сценарии PTY-1…PTY-4 на Linux (macOS — вручную, если появится машина).

**Ф.4. ConPTY на Windows** — та же поверхность, тот же набор сценариев PTY-1…PTY-5. Живёт в отдельном файле
(`proc_conpty.c`) за `#ifdef _WIN32`; Windows 10 1809+; иначе `Err(Unsupported)`.

**Ф.5. Закрытие:** доки (`docs/guide/`), строка в `docs/plans/README.md`-навигации (через
`gen-plan-status.sh`, руками не пишется), реестр дефектов (если нашлись), закрытие `[M-176.1-process]`-хвостов,
пример `examples/` «запуск агента с таймаутом и деревом». Перенос сервиса-потребителя не входит.

## 5. Критерии приёмки и сценарии

**Машинные критерии.** Все сценарии — фикстуры-пары: `std/src/os/proc_*_test.nv` (Ф.1–Ф.2, оба ОС) и
`spec_tests/conformance/standalone/` с маркерами `EXPECT_STDOUT` / `EXPECT_RUNTIME_PANIC` для языковых форм (если
D-блок потребует). Каждая фикстура утверждает наблюдаемое: байты, счётчик, код возврата. «Проба в обе стороны» из
`test-conventions.md`: в каждой фикстуре — сабот условия (убрать `uv_read_stop`; не назначить Job Object; не дождаться
`exit_cb`) краснит ровно её.

| № | сценарий | наблюдаемое | этап |
|---|---|---|---|
| S1 | **stdin → stdout → код 0**: `spawn` помощника-эхо, `write_all("привет\n")`, `close`, `read_to_end` stdout, `wait` | байты stdout == поданным; `status.code == 0` | Ф.1 |
| S2 | **долгий вывод частями без блокировки волокон**: потомок печатает 20 порций с паузой 50 мс; параллельно в том же `supervised` крутится волокно-счётчик | порции приходят по одной; счётчик успел сделать ≥ N тиков МЕЖДУ порциями (планировщик жив) | Ф.1 |
| S3 | **остановка по pid с деревом**: потомок → внук (оба пишут метку раз в 50 мс); `kill_pid(child.pid(), Kill, tree: true)` | через 1 с обе метки перестали расти; `wait` возвращает; процессов с этим родителем нет | Ф.2 |
| S4 | **закрытый stdout при живом процессе**: читатель вызывает `ChildStdout.close()`, потомок продолжает писать | потомок получает `EPIPE`/`BROKEN_PIPE` и завершается с ненулевым кодом ИЛИ продолжает (по ветке «игнорирует»); наша сторона: `wait` возвращает, дескрипторов не утекло; нет зависания | Ф.1 |
| S5 | **большой вывод при медленном чтении**: потомок пишет 64 МБ, читатель берёт по 4 КБ с паузой | потомок блокируется на записи (давление назад: его прогресс ≈ прогресс читателя, а не «успел всё сразу»); все 64 МБ приходят целиком, контрольная сумма сходится; память не растёт линейно с объёмом | Ф.1 |
| S6 | **`output()` без взаимной блокировки**: потомок пишет 1 МБ в stderr и 1 МБ в stdout вперемешку | `Output` получен, размеры 1 МБ/1 МБ; без параллельного чтения — тест зависал бы | Ф.1 |
| S7 | **таймаут дерева**: `supervised(timeout: 1.to_seconds()) { child.wait() }` с потомком-спящим и внуком | `Interrupted`/`TimeoutError` за ≈1 с; дерево мертво | Ф.2 |
| S8 | **`stop(grace)`**: потомок игнорирует `Terminate` (POSIX) | `Terminate` → пауза → `Kill`, код смерти по сигналу: `status.signal == Some(9)` (POSIX); Windows: сразу `Kill`, пауза не нужна (названо в таблице) | Ф.2 |
| S9 | **утечки при отмене**: `consume`-блок с `Child`, внутри `panic`/отмена | `cleanup` погасил потомка; повторный `kill_pid(pid, Kill, false)` → `NotFound` | Ф.2 |
| PTY-1 | **ввод**: потомок читает строку и печатает её в верхнем регистре | написанное `write("abc\n")` → в выводе есть `ABC` | Ф.3/Ф.4 |
| PTY-2 | **вывод с управляющими последовательностями**: потомок печатает `ESC[31mred ESC[0m` и `ESC[2J` | байты приходят НЕИЗМЕНЁННЫМИ (проверка вхождения подстрок; префикс/суффикс ConPTY допустим, R8) | Ф.3/Ф.4 |
| PTY-3 | **resize**: потомок печатает размер окна по запросу | до `resize` — 24×80 (заданные при `spawn_pty`), после `resize(40, 120)` — 40×120 | Ф.3/Ф.4 |
| PTY-4 | **закрытие**: `close()` живого потомка / потомок завершился сам | `read` отдаёт `Ok(0)` после последнего байта; `wait` возвращает код; потомок мёртв (SIGHUP/ClosePseudoConsole) | Ф.3/Ф.4 |
| PTY-5 | **`Ctrl-C` через PTY**: `write([0x03])` | потомок получает `SIGINT` / `CTRL_C_EVENT`; `status.signal == Some(2)` на POSIX | Ф.3/Ф.4 |

**Отказы (по одной фикстуре на ветку, обе ОС):**

| отказ | как создаётся | ожидаемое |
|---|---|---|
| нет программы | `Command.new("net-nonexistent-xyz").spawn()` | `Err(IoError{kind: NotFound})`, `raw_os != 0` |
| нет доступа | файл без бита `x` (POSIX) / каталог вместо exe (Windows) | `Err(PermissionDenied)` (Windows: допустим `InvalidInput`, фикстура принимает оба, названо) |
| неверная папка | `.dir("несуществующая")` | `Err(NotFound)` или `NotADirectory` (по ОС; фикстура принимает пару, названо) |
| прерванный процесс | потомок убит снаружи (`kill_pid`), пока родитель в `wait` | `Ok(ExitStatus{code: 137, signal: Some(9)})` (POSIX) / `code != 0` (Windows) — не `Err` |
| запись после смерти потомка | `write` в `ChildStdin` после выхода потомка | `Err(BrokenPipe)`, паники нет |
| PTY на старой Windows | `spawn_pty` на ОС без ConPTY | `Err(Unsupported)` |

**Приёмка глазами, потому что** не получает машинного вердикта: (1) Windows-половина, пока нет Windows-задания CI
(R9) — исполнитель прикладывает вывод прогона на Windows дословно; (2) совпадение п. 3.4 с фактическим
поведением на macOS, если его проверяют руками.

## 6. Риски

| № | риск | мера |
|---|---|---|
| R1 | **Неблокирующее чтение паркует волокно**: `uv_read_start` на пайпе — события в петле libuv; при cancel scope читатель обязан проснуться, а буфер — не освободиться до `read_cb` | образец `net_tcp_read`: «operation-in-flight»-счётчик, stop_cb, закрытие дескриптора из другого волокна будит читателя (`UV_ECANCELED`); те же стресс-фикстуры, что `net/stress_test.nv` |
| R2 | **Давление назад**: либо читатель не читает (потомок блокируется — это верное поведение), либо библиотека сама буферизует без предела (OOM) | дизайн «тянущего» чтения (`uv_read_stop` между вызовами), буферов в рантайме нет; сценарий S5 меряет память |
| R3 | **Взаимная блокировка двух пайпов**: stderr полон, пока читаем stdout | `Command.output()` читает параллельно; в доках `Child` — предупреждение с образцом на `supervised` |
| R4 | **Дерево процессов**: на POSIX `setsid` ломает «управляющий терминал» родителя (процесс перестаёт получать Ctrl-C из консоли); на Windows `AssignProcessToJobObject` ПОСЛЕ `uv_spawn` — окно гонки, а у libuv свой глобальный job с `KILL_ON_JOB_CLOSE` (вложенные задания — Windows 8+) | Ф.0 проба; `Tree.Single` остаётся умолчанием; решение по гонке — путь `CREATE_SUSPENDED` |
| R5 | **PTY на POSIX требует свой `fork`** (`uv_spawn` не делает `setsid` + `TIOCSCTTY`): `fork` в многопоточном процессе с Boehm GC и M:N-планировщиком — между `fork` и `exec` только async-signal-safe | всё (argv, env, пути) готовится до `fork`; в потомке — `setsid`, `ioctl`, `dup2`, `chdir`, `execve`; GC-потоки в потомке не нужны (замена образа) — проверяется в Ф.0; запасной путь — `posix_spawn` + `POSIX_SPAWN_SETSID` там, где он есть |
| R6 | **Повторное использование pid** в `kill_pid` | документируется; `Child`-путь (по дескриптору) pid не использует; для `kill_pid` с `tree: true` — обход потомков одним снимком + проверка времени старта процесса там, где ОС даёт |
| R7 | **macOS и PTY**: `kqueue` исторически капризен к мастеру PTY (libuv обходит это в `uv_tty`, но `uv_pipe_open` над мастером — не гарантирован) | Ф.0 проба на macOS, если машина есть; если нет — macOS помечен «не проверен» в доках, а не молча заявлен |
| R8 | **ConPTY добавляет свои последовательности** (инициализация, запрос позиции курсора `ESC[6n`, на ранних версиях требуется ответ), и **deadlock при `ClosePseudoConsole`, пока выход не вычитан** | сценарии PTY-2 принимают префикс/суффикс; `close` сначала выводит читателя в дренаж, потом закрывает консоль; описано в п. 3.4 |
| R9 | **Windows-задания в CI нет** — половина приёмки без машинного вердикта | исполнитель обязан приложить прогон на Windows; предложение интегратору: добавить `windows-latest`-задание `nova test std --filter os/proc` (вопрос ниже) |
| R10 | **Расхождение `mock_proc` и реальности** | тот же набор фикстур прогоняется на обоих обработчиках там, где можно (stdin→stdout), как `mock_os` |
| R11 | **Широкий диапазон `ErrorKind` у Windows**: коды `ERROR_*` проецируются хуже errno | таблица отказов принимает пары вариантов, `raw_os` авторитетен (правило `IoError`) |

## 7. Черновик D-блока (НОМЕР НЕ ВЫБРАН: «D-?» — ждёт интегратора)

**D-? — os: потоки дочернего процесса, управление процессом, PTY (Plan 294)** · амендмент-продолжение D453.

1. Новый plumbing-эффект `Proc` (рядом с `Os`, не вместо): операции с дескрипторами `*()`; `Os.process_run` и
   `Command.run()` не меняются (D453, «сигнатура не расширяется задним числом» — соблюдено).
2. Типы: `Stdio`, `Child`, `ChildStdin/Stdout/Stderr`, `Signal`, `Tree`, `PtySize`, `PtyChild`, `Output`;
   `ExitStatus.signal`. Сигнатуры — п. 3 плана; нормативной станет таблица в D-блоке.
3. Поток процесса — структурно `io.Read`/`io.Write` (образец `TcpStream`), владение — `consume` + `Cleanup`.
4. Чтение тянущее; запись паркует; независимые слоты; дуплекс из двух волокон допустим.
5. Отмена scope во время `wait()` гасит потомка (при `Tree.Group` — дерево) и возвращает `Err(Interrupted)`
   (D453, тот же выбор).
6. `Child.cleanup` не оставляет живого потомка; дефолтная пауза `stop` — константа в D-блоке.
7. Платформенные различия — таблица п. 3.4 нормативна, `Unsupported` — единственный способ сказать «этой ОС нельзя».
8. PTY: один дуплексный канал, `resize`, EOF как `Ok(0)`; требования ОС (Windows 10 1809+).
9. **Не вводится:** новых вариантов `ErrorKind`, глобальных настроек, `select`.

Обзорная страница (`spec/effects.md` + `.ru.md`) правится тем же слиянием, что D-блок (правило `AGENTS.md`,
«Changing the language»: спека до реализации).

## 8. Не делаем

* Реализацию — в этой задаче только план и, при Ф.0, записка. `std/`, `nova_rt/`, компилятор, `spec/decisions/` — не
  трогаем до слияния Ф.0 и номера D-блока.
* Не меняем и не заменяем `Os.process_run`/`Command.run()` (D453 жив).
* Не пишем терминальный эмулятор (разбор escape-последовательностей, экранный буфер): PTY отдаёт байты как есть.
* Не делаем `select` по нескольким потокам и отдельного пула потоков под ввод-вывод: хватает волокон.
* Не вводим ограничения ресурсов потомка (rlimit, cgroup, `JOB_OBJECT_LIMIT_*` кроме KILL_ON_JOB_CLOSE).
* Не заявляем macOS/старые Windows поддержанными без проверки; пользовательский «шелл» (`sh -c`) — не часть API.
* Перенос сервиса запуска агентов — отдельная работа владельца.

## 9. Открытые вопросы

| вопрос | адресат | умолчание, если ответа нет | срок |
|---|---|---|---|
| Номер плана 294 свободен? | интегратор | 294 | до слияния плана |
| Номер D-блока | интегратор | в тексте остаётся «D-?», Ф.0 блокируется только на этом | до начала Ф.0 |
| PTY: сначала POSIX или ConPTY? | план решает, владелец может переиграть | **POSIX первым** (Ф.3), ConPTY — Ф.4: CI на Linux даёт машинный вердикт семантике, форма API определяется без выходящих на Windows 1809+ ограничений | при входе в Ф.3 |
| `ExitStatus` получает `signal` (аддитивное поле) — допустимо? | интегратор | да, конструируется только в `os.nv` | Ф.1 |
| Windows-задание в CI | интегратор/владелец | нет; Windows — вручную с приложенным выводом | Ф.1 |
| Нужен ли macOS (PTY)? | владелец | не заявляем, пока не проверен | Ф.3 |
| Целевая платформа сервиса запуска агентов (только Linux или и Windows)? | владелец | обе ОС, как в задаче | сейчас |

## 10. Followups

Плана-носителя пока нет; плавающие хвосты (если появятся) — строки
[backlog-followups.md](backlog-followups.md). Реестр дефектов — [221.1](221.1-bug-sweep.md) (№TBD).
