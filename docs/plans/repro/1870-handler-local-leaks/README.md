<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# №1870 — локал тела одной операции обработчика-литерала перетипизирует параметр следующей

Найдено задачей #50 (план 294 Ф.4) 2026-10-08/09, при переводе PTY-операций `Proc` в форму D456.

**Что.** В литерале обработчика `effect X { a(..) { mut s = 0 ... } b(s Signal) => f(s) }` чекер видит
локал `s` из тела `a` и в теле `b`: параметр `s` считается `int`, при передаче в `f(s Signal)` вставляется
сумм-подъём, и C получается `f(nova_make_Signal_Other((nova_int)((Nova_Signal*)s)->tag))`. Ошибки
компиляции нет; в рантайме `Signal.Kill` приходит как `Signal.Other(2)` (тег варианта).

**Как запускать** (`.nv.txt` — улика, вне раннера; копия без `.txt` в любой каталог):

```sh
cp leak_test.nv.txt    <dir>/leak_test.nv     && nova test <dir>   --toolchain clang -v
cp control_test.nv.txt <dir2>/control_test.nv && nova test <dir2>  --toolchain clang -v
```

**Замер (Windows, clang).** `leak_test` — `RUN-FAIL … assert failed: via(Signal::Kill) == 9`, в C
`return …n_of(nova_make_Signal_Other(((nova_int)(((Nova_Signal*)(s))->tag))));`. `control_test` (те же две
операции и то же имя параметра, локал в первом теле назван `t`) — PASS. Отдельные контроли той же сессии:
одно имя параметра с разными типами в двух операциях без локала — PASS; передача `sig int` во внешнюю
функцию из соседних тел — PASS. Значит, течёт именно ЛОКАЛ тела.

**Носитель в std.** `real_proc` (`std/src/os/proc.nv`, ветка #50 на базе t44): тела `child_wait` /
`child_try_wait` / `child_wait_ms` объявляют `mut sig = 0`, ниже `pty_kill(pty PtyChild, sig Signal)` →
`Signal.Kill` доходил до `proc_pty_kill` как 2 → `Err(Unsupported)`, PTY-F красный. Обход в #50 —
параметр назван `signal` (комментарий со ссылкой на №1870). Чинится чекер отдельной задачей класса.
