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

## Чего в этой волне нет

Убийство дерева процессов, сигналы, `stop(grace)`, `wait_timeout` (план 294, фаза 2), PTY (фазы
3–4), `stderr_to_stdout`. macOS не проверялся. Сегодня уборка `Child` убивает только самого потомка.

## Как тестировать код, который запускает процессы

`Proc` — plumbing-эффект, как `Os` и `Net`: рабочий код получает `real_proc()` сам
(`#default_handler`). Фикстуры `std/src/os/proc_streams_test.nv` показывают переносимых
потомков-помощников (`sh`/`cat` на POSIX, `powershell` на Windows).
