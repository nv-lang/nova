<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Child-process streams (`std.os`)

**English** | [Русский](process-streams.ru.md)

`Command.run()` (D453) starts a program, waits and gives you the exit code — nothing else.
`Command.start()` (Plan 294, [D492](../../spec/decisions/04-effects.md#d492)) gives you a **live
child** and **byte streams** over its stdin, stdout and stderr. Everything parks the *fiber*,
never the OS thread, and behaves the same on Windows and POSIX.

> `spawn` is a reserved word in Nova (it starts a fiber), so the method is **`start`**.

Runnable example: [`examples/os/agent_runner.nv`](../../examples/os/agent_runner.nv).

## Quickstart

```nova
import std.io.{read_to_end, write_all}

consume child = Command.new("sort").stdin(Stdio.Piped).stdout(Stdio.Piped).start()?
consume tx = child.take_stdin().unwrap()      // ChildStdin  - io.Write
consume rx = child.take_stdout().unwrap()     // ChildStdout - io.Read
write_all(tx, "b\na\n".bytes())?
tx.close()                                    // end of input
ro out = read_to_end(rx)?                     // "a\nb\n"
ro status = child.wait()?                     // parks the fiber
assert(status.success())
```

(`consume child = …` makes leaving the scope clean up: a child still running is **killed**, pipes
you never took are closed.)

## API

| call | what it does |
|---|---|
| `Command.stdin(s)` / `.stdout(s)` / `.stderr(s)` | `Stdio.Null` (default), `Stdio.Inherit`, `Stdio.Piped` |
| `Command.start() Proc -> Result[Child, IoError]` | start without waiting |
| `Command.output() Proc -> Result[Output, IoError]` | run to completion, collect stdout + stderr (`Output { status, stdout, stderr }`) |
| `Child.pid()` | OS process id |
| `Child.take_stdin()` / `take_stdout()` / `take_stderr()` | `Some(stream)` **once**, only if that stream was `Piped` |
| `Child.wait()` | wait for exit → `ExitStatus` (`code()`, `success()`, `signal()`) |
| `Child.try_wait()` | `Ok(None)` while running, `Ok(Some(status))` after |
| `ChildStdin.write(data)` | writes the **whole** buffer, `Ok(n)`; dead reader → `Err(BrokenPipe)` |
| `ChildStdin.close()` | the child sees end of input |
| `ChildStdout.read(buf)` / `ChildStderr.read(buf)` | `Ok(n)` bytes, **`Ok(0)` = end of stream** |
| `ChildStdout.close()` | stop reading; the process itself is unaffected, `wait()` still waits for it |

The streams are `io.Read` / `io.Write`, so `read_to_end`, `write_all` and `copy` work on them.

## Rules worth knowing

* **Back pressure is automatic.** `read` pulls one chunk and stops; nothing is queued inside the
  runtime. A child that writes faster than you read is blocked by the OS pipe (64 KiB on Linux,
  a few KiB on Windows) — no memory growth, no loss.
* **Deadlock rule.** If the child fills stderr while you read only stdout, both wait forever. Read
  the two from separate fibers, send the unused one to `Stdio.Null`, or use `output()`, which
  drains both in parallel.
* **Bytes, not text.** No re-encoding, no newline translation.
* **Cancellation.** `supervised(timeout: …)` around `wait()` kills the child and returns
  `Err(Interrupted)`. A cancelled read or write closes that stream.
* **Failures are typed.** No such program → `NotFound`; not runnable → `PermissionDenied` (Windows:
  the platform's equivalent); missing `dir(...)` → `NotFound`/`NotADirectory`; the raw OS code is
  in `raw_os`.
* **Exit status.** A non-zero exit is `Ok(status)`, not `Err`. A POSIX signal death reports
  `128 + signal` in `code()` and the number in `signal()` (always `None` on Windows).

## Not in this wave

Process-tree kill, signals, `stop(grace)`, `wait_timeout` (plan 294 phase 2), PTY (phases 3–4),
`stderr_to_stdout`. macOS is not verified. Today `Child` cleanup kills only the child itself.

## Testing code that spawns

`Proc` is a plumbing effect like `Os` and `Net`: production code gets `real_proc()` automatically
(`#default_handler`). The fixtures in `std/src/os/proc_streams_test.nv` show portable helper
children (`sh`/`cat` on POSIX, `powershell` on Windows).
