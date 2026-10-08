<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Потоки дочернего процесса (`std.os`)

[English](process-streams.md) | **Русский**

`Command.run()` (D453) запускает программу, ждёт и отдаёт код выхода — больше ничего.
`Command.start()` (план 294, [D492](../../spec/decisions/04-effects.md#d492)) отдаёт **живой
дескриптор** процесса и **байтовые потоки** его stdin, stdout и stderr. Всё паркует *волокно*,
а не поток ОС, и ведёт себя одинаково на Windows и POSIX.

> `spawn` в Nova — зарезервированное слово (запуск волокна), поэтому метод называется **`start`**.

Пример, который можно запустить: [`examples/os/agent_runner.nv`](../../examples/os/agent_runner.nv).

## Быстрый старт

```nova
import std.io.{read_to_end, write_all}

consume child = Command.new("sort").stdin(Stdio.Piped).stdout(Stdio.Piped).start()?
consume tx = child.take_stdin().unwrap()      // ChildStdin  - io.Write
consume rx = child.take_stdout().unwrap()     // ChildStdout - io.Read
write_all(tx, "b\na\n".bytes())?
tx.close()                                    // конец ввода
ro out = read_to_end(rx)?                     // "a\nb\n"
ro status = child.wait()?                     // паркует волокно
assert(status.success())
```

(`consume child = …` делает выход из области уборкой: ещё живой потомок будет **убит**, а не
забранные пайпы закрыты.)

## API

| вызов | что делает |
|---|---|
| `Command.stdin(s)` / `.stdout(s)` / `.stderr(s)` | `Stdio.Null` (умолчание), `Stdio.Inherit`, `Stdio.Piped` |
| `Command.start() Proc -> Result[Child, IoError]` | запустить, не ожидая |
| `Command.output() Proc -> Result[Output, IoError]` | довести до конца и собрать stdout + stderr (`Output { status, stdout, stderr }`) |
| `Child.pid()` | идентификатор процесса ОС |
| `Child.take_stdin()` / `take_stdout()` / `take_stderr()` | `Some(поток)` **один раз**, только если поток был `Piped` |
| `Child.wait()` | дождаться выхода → `ExitStatus` (`code()`, `success()`, `signal()`) |
| `Child.try_wait()` | `Ok(None)` пока идёт, `Ok(Some(status))` после |
| `ChildStdin.write(data)` | пишет **весь** буфер, `Ok(n)`; мёртвый читатель → `Err(BrokenPipe)` |
| `ChildStdin.close()` | потомок видит конец ввода |
| `ChildStdout.read(buf)` / `ChildStderr.read(buf)` | `Ok(n)` байт, **`Ok(0)` = конец потока** |
| `ChildStdout.close()` | перестать читать; сам процесс не затронут, `wait()` всё равно его ждёт |

Потоки — `io.Read` / `io.Write`, поэтому `read_to_end`, `write_all` и `copy` с ними работают.

## Что стоит знать

* **Давление назад — автоматически.** `read` берёт один кусок и останавливается; очередей внутри
  рантайма нет. Потомок, пишущий быстрее вашего чтения, упирается в пайп ОС (64 КиБ на Linux, пара
  КиБ на Windows): память не растёт, данные не теряются.
* **Правило взаимной блокировки.** Если потомок заполняет stderr, пока вы читаете только stdout,
  оба ждут вечно. Читайте оба из разных волокон, ненужный отправьте в `Stdio.Null` или берите
  `output()` — он вычитывает оба параллельно.
* **Байты, не текст.** Без перекодирования и без перевода строк.
* **Отмена.** `supervised(timeout: …)` вокруг `wait()` убивает потомка и даёт `Err(Interrupted)`.
  Отменённые чтение или запись закрывают свой поток.
* **Отказы типизированы.** Нет программы → `NotFound`; нельзя запустить → `PermissionDenied`
  (Windows: эквивалент платформы); нет `dir(...)` → `NotFound`/`NotADirectory`; код ОС — в `raw_os`.
* **Статус выхода.** Ненулевой код — это `Ok(status)`, не `Err`. Смерть от сигнала POSIX даёт в
  `code()` значение `128 + сигнал`, а в `signal()` — номер (на Windows всегда `None`).

## Псевдотерминал (PTY)

Некоторые программы ведут себя иначе, когда «за клавиатурой» никого нет (без цвета, без
приглашений, блочная буферизация). `Command.start_pty(size)` запускает потомка на псевдотерминале
вместо пайпов: один дуплексный байтовый канал — его клавиатура и его экран — и размер окна.

```nova
import std.os.{Command, PtySize}
import std.io.{write_all}

consume pty = Command.new("cmd").start_pty(PtySize { rows: 24, cols: 80 })?
write_all(pty, "echo hello\r".bytes())?    // набор с клавиатуры; Enter — "\r"
pty.resize(PtySize { rows: 40, cols: 120 })?
mut buf []u8 = []u8.new()
buf.resize(4096, 0 as u8)
ro n = pty.read(buf)?                      // экран; Ok(0) — терминала больше нет
ro status = pty.wait()?
```

| `PtyChild` | что делает |
|---|---|
| `read(buf)` / `write(data)` / `flush()` | экран / клавиатура (`io.Read` / `io.Write`) |
| `resize(size)` | новый размер окна; потомок его видит (SIGWINCH / событие изменения консоли) |
| `pid()`, `wait()`, `try_wait()` | как у `Child` |
| `kill(sig)` | всё дерево потомка, как `Child` с `Tree.Group` |
| `close()` / выход из scope | повесить трубку, 500 мс, убить дерево, вернуться, когда его нет |

* **Читаются байты терминала, а не потомка.** Набранное отражается эхом; `\n` становится `\r\n`. На
  Windows ConPTY рисует вывод потомка в экран и отдаёт *его* — та же картинка, не те же байты:
  `ESC[0m` приходит как `ESC[m`, очистка перерисовывает экран, впереди служебный префикс. Сверяйте то,
  что программа показывает (подстроки), а не точные байтовые строки.
* **Enter — `\r`**, как шлёт настоящая клавиатура (ConPTY по `\n` строку не завершает).
* **Ctrl-C** — байт 3: `write([3])`. Потомок получает SIGINT / `CTRL_C_EVENT` (Windows: обычно
  выходит с кодом `0xC000013A`). Потомок на PTY стартует с включённым Ctrl-C, даже если ваш процесс
  его игнорирует.
* **Дерево и терминал не переживают потомка.** Когда он выходит, всё, что он запустил, убивается, а
  `read` после последнего байта отдаёт `Ok(0)`.
* **Системы.** Windows 10 1809+ (ConPTY); старше — `Err(Unsupported)`. POSIX (`forkpty`) — следующая
  фаза плана 294: до неё `start_pty` там отвечает `Err(Unsupported)`.
* **Одно волокно.** `PtyChild` — одно `consume`-значение: читайте и пишите из одного волокна (набрали —
  прочитали). Разделения на читателя и писателя для двух волокон пока нет.

Полный пример — `examples/os/pty_session.nv`.

## Чего в этой волне нет

`stderr_to_stdout`; PTY на POSIX (план 294, фаза 3); разделение `PtyChild` на читателя и писателя.
macOS не проверялся.

## Как тестировать код, который запускает процессы

`Proc` — plumbing-эффект, как `Os` и `Net`: рабочий код получает `real_proc()` сам
(`#default_handler`). Фикстуры `std/src/os/proc_streams/proc_streams_test.nv` показывают переносимых
потомков-помощников (`sh`/`cat` на POSIX, `powershell` на Windows).
