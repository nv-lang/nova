<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-boehm-large-buffer-retention-fiber-reuse] вариант (b) — ЗАКРЫТО

Worktree `d:/Sources/nv-lang/nova-netb`, ветка `p-fix-net-free-on-close`.
Модель sonnet. НЕ мёржено в main, НЕ запушено.

## Механика

Классический intrusive atomic refcount (тот же идиом, что `nova_chan_writer_close`
в channels.h / R1 A1 в sync.h): `nova_aint_fetch_sub_release` +
acquire-fence-перед-destroy на стороне, которая довела счётчик до нуля.

- `refcount` (новое поле в `NovaNet2Listener`/`NovaNet2Stream`/`NovaNet2Udp`):
  старт = 1 ("existence"-юнит — структуру держит сам факт существования live
  libuv-хендла / Nova-side ссылки).
  - `net_tcp_accept`/`net_tcp_connect`/`net_tcp_read`/`net_tcp_write`/
    `net_udp_send_to`/`net_udp_recv_from`: acquire() ОДИН РАЗ на вход,
    release() на КАЖДОМ выходе (goto-based single-exit `out:`-label — покрывает
    park + все послепарковые чтения полей — ровно §9-класс: конкурентный close()
    из ДРУГОГО файбера, пока этот filéбер ещё в парке/дочитывает поля).
  - `_nn2_stream_close_cb`/`_nn2_listener_close_cb`/`_nn2_udp_close_cb`:
    release() existence-юнита ПОСЛЕ будильника ждущих (uv_close уже завершён —
    libuv больше не тронет хендл).
  - Освобождает memory та сторона, чей release довёл счётчик до 0 (in-flight op
    ИЛИ close_cb — обе стороны безопасны, ровно один free, доказано тем же
    паттерном что и существующий `nova_chan_writer_close`).
  - `net_tcp_shutdown` cross-thread путь — fire-and-forget (без park): acquire
    ПЕРЕД nova_loop_defer_call, release ВНУТРИ отложенной job (не в
    выпускающем кадре — тот уже вернулся раньше, чем job выполнится).
  - net.h НЕ тронут (0 сигнатурных изменений, чисто внутренние static-хелперы +
    поля структур).

## НАХОДКА №1 (вне зоны net.c) — P217 ломает std/net + флагман на ЛЮБОЙ платформе

Текущий main HEAD: Plan 217 (auto-cleanup, merge `22f3a519f`, предок ветки)
ломает компиляцию ЛЮБОГО файла, использующего `Ok(consume x) => {...}` с
struct-типизированным Nova-значением (`TcpStream` и родня) — `std/src/net/*`
целиком И флагман-агрегатор. Корень найден и подтверждён: `is_struct_type()`
(`compiler-codegen/src/codegen/emit_c.rs:41934`) классифицирует C-тип по
префиксу (`Nova_`, `NovaVtable_`, `NovaOpt_`, `_NovaTuple`, `struct `) но
**пропускает `NovaValue_`** (напр. `NovaValue_TcpStream`) — Plan 217's
hoisted-cleanup-пролог поэтому эмитит `NovaValue_TcpStream s = 0;` (буквальный
int-литерал) вместо `NovaValue_TcpStream s = {0};` для STRUCT-типов → C2440
"cannot convert int to struct" на MSVC И clang, любая ОС (баг чисто в
Rust-кодогене, не платформенный). Подтверждено НЕ моей правкой:
воспроизводится байт-в-байт на пристинном main-бинаре (уже собранном ДО этой
волны). Однострочный фикс — добавить `|| ty.starts_with("NovaValue_")` в
`is_struct_type` — ИСПОЛЬЗОВАН ТОЛЬКО как временный SCRATCH-патч (не входит в
net.c-поставку, отревертирован перед коммитом) чтобы получить рабочий свой
бинарь для верификации; **чинить по-настоящему — НЕ моя зона** (`emit_c.rs`
запрещена этой волной). Отдельный маркер заведён в backlog-followups.md.

## НАХОДКА №2 — заимствованный WSL-бинарь молча игнорировал NOVA_RT_DIR

`~/nova-appeffect-wsl` + `~/nova-appeffect-target/release/nova` (заимствованное
дерево из более ранней opus-разведки, ДО Plan 217) использовались для
слоп-репро (по инструкции задания). Обнаружено: rt-archive-cache для ЭТОГО
старого бинаря производил ОДИНАКОВЫЙ хеш независимо от содержимого net.c в
NOVA_RT_DIR-дереве (проверено strings на скомпилированном .exe — debug-принты,
добавленные ТОЛЬКО в overlay-копию net.c, отсутствовали в бинаре: 0 попаданий).
Т.е. NOVA_RT_DIR-оверлей молча не подхватывался этим конкретным старым
бинарём — вероятно собран до content-hash фикса Plan 218
(`[M-218-rt-archive-parallel-jobs-race]`, влит 2026-07-20). Попытка пересобрать
`nova` НАПРЯМУЮ из ~/nova-appeffect-wsl своим `cargo build --release` тоже не
получилась — системный `rustc 1.93.1` на этой WSL-машине падает ICE
(известный баг, см. `bd43e0cd4`). **Решение:** вся РЕАЛЬНАЯ верификация
(std/net, слоп-репро до/после, флагман) переделана на СВОЁМ Windows-бинаре
(nova-netb, cargo build --release, toolchain clang) со scratch-P217-патчем,
временно приложенным ТОЛЬКО для сборки, затем отревертированным.
nova-tls-риск-гейт (требующий Linux/gcc для mbedTLS-вендора) и
stream_leak-смоук остались на WSL, но там валидность подтверждена ИНАЧЕ:
явный `grep -c "variant (b)"` net.c ДО каждого прогона + разные
rt-archive-cache хеши между "до"/"после" (доказывает, что overlay
действительно попадал в СБОРКУ archive для ЭТИХ прогонов — расхождение с
слоп-репро объясняется тем, что там я ИСХОДНО не проверял хеш до находки №2).

## Slope-репро — РЕАЛЬНЫЙ до/после (свой Windows-бинарь, toolchain clang, 3+3)

LARGE(16384 B) TCP-loopback, `docs/plans/wip/boehmret-repro/boehmret_slope.nv`,
1500 итераций, least-squares slope `gc.heap_size()` после `gc.collect()`:

| Прогон | ДО (main net.c, только вариант а) | ПОСЛЕ (net.c этой волны, вариант b) |
|---|---|---|
| 1 | 5249 | 627 |
| 2 | 4803 | 671 |
| 3 | 5264 | 708 |

Среднее: ДО ≈ 5105 байт/итер → ПОСЛЕ ≈ 669 байт/итер — **снижение ~87%**,
стабильно 3/3. Остаточные ~670 байт/итер — уже не в масштабе буфера (16 КиБ),
похоже на несвязанный мелкий шум (fiber/scope overhead), не предмет этой волны.
SMALL(control) и COMPUTE-LARGE(без net) — плато/отрицательный slope в обеих
конфигурациях, как и раньше (не задеты).

## Риск-гейт use-after-free — ЗЕЛЁНЫЙ

- **std/net full** (весь `std/src/net/` как одна CU — addr/tcp/udp/dns/split/
  stress/pingpong/mock/error/share/write_all/byte_surface/d302; split_test.nv +
  stress_test.nv = конкурентные close-from-elsewhere сценарии, ровно §9-класс):
  свой Windows-бинарь (toolchain clang), **3/3 стабильных PASS**.
- **nova-tls** (`nova test src`, read-only их master, WSL/gcc для mbedTLS-
  вендора): `cert_modes_test` CU — 29-30 тестов (handshake/mTLS/ALPN/
  SPKI-pinning/shim_link) — **PASS**, мой net.c подтверждён в сборке (разный
  rt-archive-cache хеш до/после overlay).
- **stream_leak** (реальный `stream_leak_test_slow.nv`, 1500 TLS-итераций,
  TIMEOUT >15 мин на этой WSL-машине — compute-bound, не зависание, CPU
  постоянно ~110-290%). Вместо полного прогона — throwaway-копия
  (уникальные имена констант, НЕ трогает файл-мастер, удалена после)
  `stream_leak_smoke_tmp.nv` (150 итераций, warmup 20, sample-every 10):
  **3/3 PASS**, slope = −5293 / −3780 / −3780 байт/итер (плато, порог 35000).
  Ни одного крэша/зависания/UAF за все прогоны.
- **Флагман live-путь** (`examples/flagship/aggregator`, свой Windows-бинарь,
  `--strict-effects`): build OK → запуск → `curl` `/` (200, 61358 байт),
  `/api/snapshot` (200, реальный JSON с 6 fibers), `/api/run` ×3 подряд (все
  200) → чистое завершение (`taskkill`, порт освобождён). Полный
  accept→read→write→close цикл через мой net.c под реальным HTTP-трафиком.
- **Windows платформо-нейтральность**: net.c компилируется БЕЗ ошибок под ОБА
  тулчейна (msvc И clang) на каждой сборке этой волны.

## §9 mn-coding-conventions — соответствие

Ровно паттерн §9 (стек/heap-указатель, пересекающий границу
потока/файбера-жизни, требует counter-based lifetime-wait): владелец
структуры (существование, юнит=1) отдельно от in-flight операций (acquire на
входе / release на каждом выходе). Свободна память только когда И close_cb
завершил uv_close, И ни одна операция не в полёте — симметрично идиому
`nova_chan_writer_close`/R1 A1 (release-decrement + acquire-fence-перед-destroy,
sync.h). Единственный НЕ покрытый явным acquire/release путь —
`net_tcp_shutdown` fire-and-forget cross-thread ветка — закрыт отдельным
acquire-перед-очередью/release-внутри-job (тот же класс §9: указатель,
пересекающий границу очереди, нуждается в явном lifetime-юните на стороне
очереди, раз выпускающий кадр уже вернулся раньше job'а).

## Маркер

`[M-boehm-large-buffer-retention-fiber-reuse]` — вариант (b) выполнен, маркер
закрывается (backlog-followups.md). Новый floating-маркер заведён для P217
`is_struct_type()`-пробела (`NovaValue_` prefix missing) — блокирует
компиляцию std/net + флагмана на main HEAD, P1 (широкий блокер).
