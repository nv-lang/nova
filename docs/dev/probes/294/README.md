<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# План 294, Ф.0 — пробы: записка с замерами (2026-10-08)

Четыре маленьких C-пробы против libuv. Собираются так: Windows — `sh build-probe-win.sh <проба>.c <out>.exe`
(clang + `libuv.lib` из кэша сборки); Linux — `gcc <проба>.c -luv -lutil`. Выводы ниже — дословные, с машины
исполнителя (Windows 11, clang/MSVC ABI; Linux — WSL2 Ubuntu, gcc, libuv 1.x).

## (а) `probe_a_pipe.c` — `uv_spawn` + `UV_CREATE_PIPE`, давление назад

Потомок (сам бинарь) пишет 8 МиБ блоками по 4 КиБ и после каждого блока переписывает файл-счётчик. Родитель
сначала НЕ читает 1 с, потом тянет по одному куску (`uv_read_start` → `uv_read_stop`).

```
Windows:  PHASE1 unread 1s: child progress=65536 bytes (of 8388608)
          PHASE2 after one chunk + uv_read_stop: progress 69632 -> 69632 (stable=yes), got=4096
          PHASE3 drained: got=8388608 sum_ok=yes exit_code=0 chunks=2048
Linux:    PHASE1 unread 1s: child progress=110592 bytes (of 8388608)
          PHASE2 after one chunk + uv_read_stop: progress 110592 -> 110592 (stable=yes), got=4096
          PHASE3 drained: got=8388608 sum_ok=yes exit_code=0 chunks=2048
```

**Вывод:** допущение плана подтверждено на обеих ОС. Нечитаемый пайп останавливает потомка (≈ 64–108 КиБ),
`uv_read_stop` держит остановку, 8 МиБ приходят целыми. Урок пробы: дочерний `stdout` на Windows по умолчанию в
текстовом режиме CRT (`\n` → `\r\n`) — это свойство потомка, а не рантайма.

## (б) Дерево процессов

`probe_b_job.c` (Windows), 30 прогонов каждого варианта; потомок сразу порождает внука.

```
V1 uv_spawn, delay   0 ms, затем Assign: runs=30 grandchild_escaped=0   assign_failed(nested job)=0
V1 uv_spawn, delay 100 ms, затем Assign: runs=30 grandchild_escaped=30  assign_failed(nested job)=0
V2 CREATE_SUSPENDED + Assign + Resume:   runs=30 grandchild_escaped=0
```

**Вывод:** окно гонки настоящее (при задержке 100 мс между `uv_spawn` и `AssignProcessToJobObject` внук уходит
из задания в 30 случаях из 30; без задержки не уходит, но это удача планировщика, а не гарантия).
Вложенное задание назначается без ошибки (у процессов libuv свой job). **Решение для Ф.2:** `Tree.Group` на
Windows — `CreateProcess(CREATE_SUSPENDED)` + `Assign` + `ResumeThread` (0 побегов из 30), а не вставка после
`uv_spawn`.

`probe_c_posix.c`, T1/T2 (Linux): 

```
T1_detached: child=472 child_pgid=472 (parent pgid=468) grandchild=473 / after kill: grandchild alive=no
T2_plain:    child=474 child_pgid=468 (parent pgid=468) grandchild=475 / after kill: grandchild alive=YES
```

**Вывод:** `UV_PROCESS_DETACHED` (`setsid`) + `kill(-pid)` убивает внука; обычный `kill(pid)` оставляет жить.
Подтверждает «Group — явный, Single — умолчание». Побочное следствие `setsid` (потомок не получает Ctrl-C
консоли родителя) остаётся рискoм R4 и фиксируется в документации Ф.2.

## (в) PTY

`probe_c_posix.c`, T3/T4 (Linux):

```
T3 forkpty pid=476 master_fd=11
T3 uv_pipe_open(master) rc=0 (ok)
T3/T4 master output (39 bytes): abc<0d><0a>24 80<0d><0a>ABC<0d><0a><1b>[31mred<1b>[0m<0d><0a>40 120<0d><0a>
T3 read loop ended with err=-5 (i/o error); EIO=-5 EOF=-4095; child exit=0
```

**Вывод:** мастер `forkpty` открывается `uv_pipe_open` и читается `uv_read_start`; эхо и `\n`→`\r\n` делает
ядро; управляющие последовательности приходят неизменными; `TIOCSWINSZ` виден потомку (`40 120`); конец —
`EIO` (−5), а не `EOF` — переводится в `Ok(0)` (как в плане). Допущения R5 (свой `fork` до `exec`) проба не
проверяла — потомок вызывает `execl` сразу; это Ф.3.

`probe_d_conpty.c` (Windows):

```
CreatePseudoConsole hr=0x00000000
child exited code=3
bytes available before ClosePseudoConsole: 16
ReadFile ended after ClosePseudoConsole, err=109 (109 = ERROR_BROKEN_PIPE = EOF)
raw output (86 bytes): <1b>[?9001h<1b>[?1004h<1b>[?25l<1b>[?9001l<1b>[?1004l<1b>[2J<1b>[m<1b>[H<1b>]0;C:\WINDOWS\SYSTEM32\cmd.exe<07><1b>[?25h
```

**Вывод:** ConPTY работает; код выхода сохраняется; выходной пайп закрывается только после
`ClosePseudoConsole`; перед текстом идут служебные последовательности (R8 подтверждён). **Не объяснено:**
строка `hello` ушла в stdout РОДИТЕЛЯ, а не в псевдоконсоль (родитель запущен без консоли, из оболочки
агента) — выяснить в Ф.4 до реализации (вероятно, нужен явный `STARTF_USESTDHANDLES` с `INVALID_HANDLE_VALUE`
или консоль у родителя).

**Ответ Ф.4 (задача #50, 2026-10-08).** Причина — наследование: без `STARTF_USESTDHANDLES` `CreateProcess` отдаёт
потомку std-дескрипторы родителя, а у родителя, запущенного с перенаправленным выводом, это пайпы, а не консоль.
Та же проба (`cmd /c echo hello& exit 3`), stdout родителя перенаправлен в пайп (`| cat`), три режима
`STARTUPINFO`:

```
без STARTF_USESTDHANDLES:            hello            <- в stdout родителя
                                     raw (86): ...ESC[2J ESC[m ESC[H ESC]0;C:\WINDOWS\SYSTEM32\cmd.exe BEL ESC[?25h
STARTF_USESTDHANDLES, NULL:          raw (93): ...ESC[2J ESC[m ESC[H hello<0d><0a> ESC]0;...cmd.exe BEL ...
STARTF_USESTDHANDLES, INVALID_HANDLE_VALUE:  raw (93): то же, hello в псевдоконсоли
```

Рантайм (`proc_pty_spawn` в `nova_rt/process.c`) ставит `STARTF_USESTDHANDLES` с пустыми дескрипторами. Прочие
замеры Ф.4 (перерисовка вывода ConPTY, Enter = `\r`, наследуемый флаг «игнорировать Ctrl-C») — в плане 294,
раздел Ф.4, и в D492.

## Не проверялось

macOS (нет машины) — не заявляется поддержанным. R5 (async-signal-safe между `fork` и `exec` в процессе с
Boehm GC) — Ф.3.
