<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-187-docker-linux-runtime-hang] — Linux M:N server profile, чекпоинт

Worktree: `d:/Sources/nv-lang/nova-linuxsrv`, ветка `fix-linux-server-profile`.
В main НЕ мёржить. Runtime-фикс, не язык — D-амендмент не требуется.

## Слой 1 — fiber_arena guard-page mprotect storm (ПОЧИНЕНО, не провалидировано под docker)

Root cause подтверждён: каждый `mprotect(slot_base, GUARD_SIZE, PROT_NONE)`
в цикле guard-страниц (`nova_fiber_arena_init`,
`compiler-codegen/nova_rt/fiber_arena.c`) СПЛИТИТ единый mmap-регион на
guard(PROT_NONE)+usable(RW) VMA — 2 VMA/слот вместо 1. Linux ограничивает
общее число VMA процесса `/proc/sys/vm/max_map_count` (дефолт 65530).
NOVA_MAX_FIBERS дефолт 16384 × 2 VMA × N воркеров быстро выбивает лимит
(8 воркеров → 262144 VMA). Обход `NOVA_MAX_FIBERS=2048` (2048×2×8=32768 <
65530) подтвердил гипотезу.

Фикс (`compiler-codegen/nova_rt/fiber_arena.c`):
1. `_nova_vma_slot_budget()` (новая статик-функция, читает
   `/proc/sys/vm/max_map_count` через `_nova_read_max_map_count()`,
   кэш процесс-wide `_nova_maxmap_cache`) — считает безопасный потолок
   slot_count НА АРЕНУ с учётом `nova_runtime_maxprocs()+1` (воркеры +
   main), резервом `NOVA_ARENA_VMA_RESERVE=4096` под остальные VMA
   процесса и safety-фактором `NOVA_ARENA_VMA_SAFETY_PCT=75`. Если лимит
   не читается (macOS/sandboxed) → `SIZE_MAX` (без клэмпа, старое
   поведение).
2. `nova_fiber_arena_init()`: pre-clamp slot_count ДО mmap/mprotect по
   этому бюджету, warn ОДИН РАЗ на процесс (`_nova_arena_clamp_warned`,
   под мьютексом).
3. mprotect-цикл: если mprotect падает НА СЕРЕДИНЕ (после pre-clamp —
   защита от гонки с другими потоками/подсистемами), больше НЕ
   `abort()` всего процесса — усечь slot_count до того, что успело
   заматчиться, warn один раз, unmap хвост. `abort()` остаётся ТОЛЬКО
   если деградированная ёмкость падает ниже пола битмапа (64 слота) —
   там уже нет безопасного меньшего шага.
4. `#include "runtime.h"` добавлен в fiber_arena.c для
   `nova_runtime_maxprocs()` — циклической зависимости нет (runtime.h
   не инклудит fiber_arena.h).

Компиляция подтверждена: `nova build examples/flagship/aggregator/src/main.nv
--strict-effects` в WSL2 Ubuntu (нативно, вне docker) — чисто, линкуется,
бинарь запускается. **НЕ проверено**: реальное срабатывание клэмпа/деградации
под искусственно опущенным `vm.max_map_count` (нет root/sudo в этой WSL-сессии
— `sudo` требует пароль, недоступен неинтерактивно) И под docker (где сборка
ещё не пересобрана с этим фиксом).

## Слой 2 — "нежить" после bind+listen (В РАБОТЕ)

Диагностика без strace/gdb (недоступны: `strace` не установлен, `sudo`
требует пароль интерактивно — нет пути поставить/получить root в этой
WSL-сессии). План: `/proc/<pid>/status` (State), `/proc/<pid>/wchan`,
`/proc/<pid>/syscall`, `/proc/<pid>/task/*/wchan` — доступны без root
для процесса своего UID.

Промежуточное наблюدение (некорректный скрипт, НЕ финальный вывод):
нативный (не-docker) прогон в WSL2 — sever стартовал, слушал 8187,
ПЕРВЫЙ curl получил "Empty reply from server" (соединение закрыто без
ответа), и процесс исчез (ps -p <pid> пусто) вскоре после. Это может
быть (а) тот же exit-hang, который в итоге таки завершился, ИЛИ
(б) отдельный краш при обработке первого запроса — НЕ различено, нужен
чистый повтор с /proc-диагностикой на каждом шаге (до/во время/после
curl), а не смешанный с side-quest про `$!` (см. ниже).

Side-note (потеря времени, НЕ баг Nova): `$!` в этой sandboxed
Bash-tool-через-wsl.exe цепочке стабильно пустой ДАЖЕ в тривиальном
`bash -c 'sleep 30 & echo $!'` — окружение репортит "screen size is
bogus"; похоже на артефакт pty/conpty этого конкретного canvas, не
относится к Nova. Обход: `pgrep -x aggregator` вместо `$!`.

## WSL-окружение (nova-work, native fs, вне git — rsync-снапшот)

- `~/nova-work` — rsync-копия `nova-linuxsrv` (exclude .git/target/
  libuv/test/libuv/docs). ВАЖНО: **worktree сам по себе НЕ чекаутит
  submodule** — `compiler-codegen/nova_rt/libuv` был пуст в
  nova-linuxsrv до `git submodule update --init
  compiler-codegen/nova_rt/libuv` (сделано в этой волне).
- `~/nova-http` — sibling path-зависимость (examples/nova.toml
  `../../nova-http`), синхронизирован с `d:/Sources/nv-lang/nova-http`
  ветка `fix-compress-dep` коммит `250f4ab` (тот же, что Dockerfile
  ожидает).
- Cargo: `CARGO_TARGET_DIR=~/nova-target`, toolchain `~/.cargo/bin`
  (rustup 1.85.0, per docs/linux-build.md — дистро-rustc ICE'ит).
- Nova-бинарь: `~/nova-target/release/nova` (собран из МОЕГО
  worktree-кода, компилятор пересобирался дважды в этой волне после
  правок fiber_arena.c — линковка runtime.c C-объектов подхватывает
  правки без пересборки Rust-компилятора, только `nova build`
  перекомпилирует изменённый nova_rt/*.c).
- Аггрегатор: собран в `~/aggregator` (НЕ `/tmp` — WSL2 VM может
  рециклиться между отдельными `wsl.exe`-вызовами этой сессии, `/tmp`
  tmpfs теряется; `~/` на ext4 персистентен).
- `/proc/sys/vm/max_map_count` в ЭТОЙ WSL Ubuntu-сессии = 1048576 (уже
  поднят кем-то ДО этой волны) — слой-1 storm НЕ воспроизводится тут
  напрямую (нужен либо root для временного понижения, либо docker, где
  дефолт мог остаться 65530).

## Гейт (дословно из задания оркестратора)

1. WSL: бинарь aggregator запускается, curl / и /api/run demo → 200,
   15с idle → 200.
2. docker run образа (пересобрать с фиксом) → тот же smoke С ХОСТА +
   5 последовательных запросов + idle.
3. Windows-профиль НЕ регрессирует: loadtest.ps1 все блоки (кроме
   известной сетевой флаки health-live) зелёные.
4. Точечно `nova test std/concurrency`. Мега-CU НЕ гонять.

**Ничего из гейта ещё не пройдено** — слой 2 не диагностирован до
конца, поэтому фикс слоя 2 отсутствует, гейт рано гонять.
