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

## Pseudo terminal (PTY)

Some programs behave differently when nobody is "at the keyboard" (no colours, no prompts, block
buffering). `Command.start_pty(size)` runs the child on a pseudo terminal instead of pipes: one
duplex byte channel that is its keyboard and its screen, plus a window size.

```nova
import std.os.{Command, PtySize}
import std.io.{write_all}

consume pty = Command.new("cmd").start_pty(PtySize { rows: 24, cols: 80 })?
write_all(pty, "echo hello\r".bytes())?    // typing; Enter is "\r"
pty.resize(PtySize { rows: 40, cols: 120 })?
mut buf []u8 = []u8.new()
buf.resize(4096, 0 as u8)
ro n = pty.read(buf)?                      // the screen; Ok(0) = the terminal is gone
ro status = pty.wait()?
```

| `PtyChild` | does |
|---|---|
| `read(buf)` / `write(data)` / `flush()` | screen / keyboard (`io.Read` / `io.Write`) |
| `resize(size)` | new window size; the child sees it (SIGWINCH / a console resize event) |
| `pid()`, `wait()`, `try_wait()` | as `Child` |
| `kill(sig)` | the child's whole tree, as `Child` with `Tree.Group` |
| `close()` / leaving the scope | hang up, 500 ms, kill the tree, return once it is gone |

* **What you read is the terminal's, not the child's bytes.** Typing is echoed; `\n` becomes
  `\r\n`. On Windows ConPTY renders the child's output into a screen and sends *that* — the same
  picture, not the same bytes: `ESC[0m` arrives as `ESC[m`, a clear repaints, a start-up prefix
  comes first. Match what the program shows (substrings), never exact byte strings.
* **Enter is `\r`**, as a real keyboard sends (ConPTY does not end a line on `\n`).
* **Ctrl-C** is the byte 3: `write([3])`. The child gets SIGINT / `CTRL_C_EVENT` (Windows: it
  usually exits with `0xC000013A`). A PTY child starts with Ctrl-C enabled even if your own process
  ignores it.
* **The tree and the terminal do not outlive the child.** When it exits, whatever it started is
  killed and `read` returns `Ok(0)` after the last byte.
* **Systems.** Windows 10 1809+ (ConPTY); older Windows: `Err(Unsupported)`. POSIX (`forkpty`) is
  the next phase of plan 294: until then `start_pty` returns `Err(Unsupported)` there.
* **One fiber.** `PtyChild` is one `consume` value: read and write from the same fiber (type, then
  read). Splitting it into a reader and a writer for two fibers is not there yet.

Full example: `examples/os/pty_session.nv`.

## Not in this wave

`stderr_to_stdout`; PTY on POSIX (plan 294 phase 3); a reader/writer split of `PtyChild`. macOS is
not verified.

## Testing code that spawns

`Proc` is a plumbing effect like `Os` and `Net`: production code gets `real_proc()` automatically
(`#default_handler`). The fixtures in `std/src/os/proc_streams/proc_streams_test.nv` show portable helper
children (`sh`/`cat` on POSIX, `powershell` on Windows).
