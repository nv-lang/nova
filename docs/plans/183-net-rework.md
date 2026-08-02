<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# План 183 — Переработка std/net: один слой extern "C", байты, M:N-безопасность

> **Маркер:** `[M-183-net-rework]` (поглощает `[M-net-redesign-owner-directive]`,
> `[M-178-servernet-live-net-substrate-segfault]`, `[M-net-socketaddr-value-record]`).
> **Статус:** ✅ **ЯДРО ВЫПОЛНЕНО (Ф.0-Ф.4 ЗАКРЫТЫ, 2026-07-06)** — см. журнал §Заход 1-4
> ниже и закрытие §Ф.5. Директива 2026-07-06 выполнена: новый однослойный `net2.c`/
> `std/net2` заменяет три корневых дефекта старой реализации (двойная обёртка, M:N-небезопасные
> `__thread`-слоты, `str`-носитель байтов). **Единственный открытый остаток:**
> `[M-183-old-net-removal-after-182]` — физическое удаление старого `net.c`/`std/net` +
> namespace-ренейм `net2`→`net`, гейтовано на санацию `nova_tests` (Plan 182: тесты
> `plan83_12`/`plan91_12`/`plan91_15`/`plan91_16`/`plan178/net_byte_surface_mock` ещё
> держат старый слой живым). Плюс backlog-хвосты компиляторных дефектов, вскрытых по ходу
> (таблица в §Ф.5) — ни один не блокирует закрытие ядра плана 183.

## 1. Диагноз владельца + подтверждение по коду (2026-07-06)

Три корневых дефекта текущей реализации (`compiler-codegen/nova_rt/net.c` ~2000 строк,
`std/net/*.nv` ~1100 строк):

**(Д1) Двойная обёртка.** Сеть исторически делалась как «расширение языка»: рукописный C
имитирует манглинг методов Nova — 174 функции вида `NovaRt_SocketAddr_method_port`,
`NovaRt_UdpSocket_method_send_to` (net.c:157+). Позже решили: подключение стандартных
модулей — через `extern "C"` (как fs/os), и старый слой обернули вторым слоем
(`std/net/ffi.nv`). Итог: два слоя обёрток, C-код зависит от деталей манглинга компилятора,
которых он знать не должен.

**(Д2) M:N-небезопасность.** Результаты операций передаются через статические
`__thread`-слоты (6 штук: `_net_tcp_read_data` :67, `_net_recv_data` :1379,
`_net_recv_sender` :1380, `_net_dns_addrs` :1453, `_net_parse_result` :1244,
`_net_tls_last_error` :1212) + аксессоры вида `tcp_stream_read_data()`. Волокна планировщика
**мигрируют между потоками ОС** (work-stealing, fibers.h:1360): волокно пишет результат в слот
потока А, просыпается на потоке Б, читает слот потока Б → чужие/пустые данные. Гонка по
построению; тот же класс, что задокументированная STALE-slot гонка M:N. Сюда же почти
наверняка уходит детерминированный сегфолт live-socket-теста
(`[M-178-servernet-live-net-substrate-segfault]`: чистый двух-волоконный
loopback-тест падает 5/5).

**(Д3) str как носитель байтов.** Операции эффекта несут `str`
(`effect.nv:70-72`: `write(stream, data str)`, `read(...) -> Result[str, NetError]`), а
байтовые методы (178 Ф.0.5 `read_bytes`/`write_bytes`) — обёртки НАД строковыми. `str` — это
UTF-8-текст, сеть возвращает произвольные байты. Носитель обязан быть `[]u8`; текст — только
явной конверсией с валидацией у пользователя.

## 2. Целевая форма

- **Один слой FFI**: чистые C-функции `nova_net_*` c C-ABI-сигнатурами по D282
  (скаляры, указатель+длина, out-параметры; НИКАКИХ `nova_str`/`NovaRt_*_method_*` в
  транспорте). Nova-типы (`TcpStream`, `SocketAddr`, …) и вся логика — в `.nv` поверх
  `extern "C"` (образец: std/fs поверх `uv_fs_*`, std/os).
- **Байты как носитель**: транспортные операции C-слоя — `(const uint8_t* buf, int64_t len)`
  вход / `(uint8_t* buf, int64_t cap, int64_t* out_n)` выход; эффект `Net` — `[]u8`-сигнатуры.
  `str`-удобства (`read_text()` и т.п.) — пользовательские хелперы в `.nv` c
  UTF-8-валидацией (`str.from_bytes -> Result[str, Utf8Error]`), НЕ операции эффекта.
- **Без статических слотов**: результат возвращается значением — вызывающий владеет буфером
  (out-параметры); составные результаты (адрес отправителя, список DNS-адресов) — заполнение
  переданной вызывающим структуры/массива либо GC-выделение с явной передачей владения.
  Ошибка — код возврата (int, -errno/UV-код); текст ошибки строится на Nova-стороне из кода.
  Инвариант: `grep -E "static.*__thread|__thread.*static" net.c` по результатным слотам = 0.
- **SocketAddr = value-запись** (снимает `[M-net-socketaddr-value-record]`): адрес — данные
  (16 байт + порт + вид), а не handle; это убирает и `_nova_alloc_addr`-кучу, и
  `_net_recv_sender`-слот.
- **Парковка/пробуждение** (park/wake поверх libuv) — сохраняется как есть: она
  отменобезопасна и M:N-корректна (проблема не в ней, а в передаче результатов).

### 2а. Нулевое копирование между слоями (требование владельца, 2026-07-06)

Данные НЕ переезжают между слоями (lib ↔ C ↔ nv) — «не хуже Rust/Go»:

- **Чтение (модель Go/Rust `read(buf) -> n`):** Nova-вызывающий владеет буфером (`mut buf []u8`);
  C-слой получает `(uint8_t* p, int64_t cap)` и отдаёт ЭТОТ ЖЕ срез libuv через `alloc_cb`
  (libuv сам спрашивает буфер — интеграция без промежуточного буфера); `read_cb` сообщает
  длину → возврат `n`. Данные записаны сетью прямо в память Nova-буфера. НОЛЬ копий,
  НОЛЬ аллокаций в C-слое. Текущая цепочка (libuv-буфер → копия в nova_str → TLS-слот →
  чтение аксессором) — удаляется целиком.
- **Запись:** `uv_write`/`uv_udp_send` получают указатель прямо на данные `[]u8`;
  на время операции ссылка на буфер держится в структуре запроса (консервативный GC видит
  её → объект жив; это уже так для парковки). НОЛЬ копий.
- **`read_to_end`/растущие данные** — растущий `Vec[u8]` на Nova-стороне, C дописывает в
  хвост по переданной ёмкости (амортизированный рост — на стороне владельца буфера,
  как `Vec::read_to_end` в Rust).
- **Неизбежная единственная копия** — только там, где её делают и Rust/Go: перенос из
  структур ОС (`addrinfo` DNS → массив value-`SocketAddr`; `sockaddr_storage` →
  value-`SocketAddr`, 16 байт). Фиксируется в карте Ф.0 поимённо.
- **Инвариант-приёмка:** в горячем пути read/write C-слоя `malloc`/`nova_alloc`/`memcpy`
  данных = **0** (grep + ревью каждой функции в Ф.0-карте: колонка «аллокации/копии»);
  единственные аллокации — структура запроса парковки (уже есть) и то, что пользователь
  сам выделил под буфер.
- **Ориентир-замер (Ф.4):** локальный эхо-тест пропускной способности (N мегабайт через
  loopback) — фиксируем число в план как базовую точку; цель — класс «ноль копий», а не
  микрогонка с Go, но деградация против старого слоя недопустима.

## 3. Фазы

**Ф.0 — карта и байт-план (½ дня агента).** Полный инвентарь: все 174 `NovaRt_*_method_*`,
все статические слоты, все str-опы, все потребители (std/http транспорт, tcp/udp/dns-тесты
plan91_12/15/16, examples). Решить формы C-сигнатур каждой операции (verb-first
`nova_net_tcp_read(h, buf, cap, &n)`). Зафиксировать амендмент D-блока (D357-семейство/D173)
СПЕКОЙ-СНАЧАЛА. Приёмка: таблица «старая функция → новая сигнатура» на каждую операцию,
с колонкой «аллокации/копии данных» (целевое значение 0 всюду, кроме поимённо перечисленных
ОС-переносов, §2а).

**Ф.1 — новый C-слой (1 заход).** Написать `nova_rt/net2.c` (рабочее имя; по завершении
замещает net.c): один слой `nova_net_*`, C-ABI, out-параметры, ноль статики, `[]u8`-транспорт;
парковка переиспользуется. Первым — TCP (connect/listen/accept/read/write/close/половинки),
затем UDP, затем DNS/адреса. Приёмка фазы: юнит-смоук на C-уровне не нужен — гейт через Ф.2.

**Ф.2 — новый .nv-слой (тот же заход или следующий).** `std/net`: `extern "C"`-деклы →
value-`SocketAddr` → `TcpStream`/`TcpListener`/`UdpSocket` (consume, D133) → эффект `Net` с
`[]u8`-сигнатурами → `real_net()`/`mock_net()`. Текстовые хелперы поверх байтов. Старый слой
ещё жив (потребители не мигрированы) — новый живёт рядом (namespaced), КРАСНОГО нет.

**Ф.3 — миграция потребителей + удаление старого (один заход, атомарно).**
std/http-транспорт, все net-тесты, examples → новый API; удалить net.c-старый слой,
`std/net/ffi.nv`-двойную обёртку, str-опы эффекта, все `NovaRt_*_method_*`. Приёмка:
grep-инварианты из §2 = 0; эталонные зелёные; http-семейство зелёное.

**Ф.4 — доказательство M:N + сегфолт.** Live-socket smoke (loopback, два волокна) — снова в
корпусе, детерминированно зелёный ≥20 прогонов; стресс: N волокон × параллельные read/write
под work-stealing (родич plan83-тестов). Проверить, ушёл ли P67-LEGACY ICE на `bind`-путях
(plan83_12) — если нет, точечный фикс/маркер. Закрыть/обновить маркеры из шапки.

**Ф.5 — журнал/спека/закрытие.** Амендменты D-блоков по факту, план-статусы, задачник,
simplifications; хвост: `Accept-Encoding`-интеграция http не трогается (уже поверх эффекта).

## 4. Критерии приёмки (все обязательны)

1. **Один слой:** `grep -c "NovaRt_.*_method_" nova_rt/net*.c` = **0**; C-слой не знает о
   манглинге Nova; все публичные C-функции — `nova_net_*` с C-ABI-типами (D282).
2. **Ноль статики результатов:** ни одного `__thread`/static-слота, переносящего результат
   операции; аксессоров вида `*_read_data()` нет.
3. **Байты:** операции эффекта `Net` не содержат `str` в транспортных сигнатурах;
   str-хелперы — только `.nv`, только через `Result[str, Utf8Error]`.
4. **M:N:** live-socket smoke + стресс под work-stealing — детерминированно зелёные;
   сегфолт-репро из `[M-178-...]` больше не воспроизводится.
4а. **Нулевое копирование (§2а):** read пишет сетевые данные прямо в Nova-буфер
   (alloc_cb = срез буфера вызывающего); write шлёт прямо из `[]u8`; в горячем пути C-слоя
   аллокаций/копий данных = 0 (кроме поимённых ОС-переносов из Ф.0-карты); эхо-замер Ф.4
   не хуже старого слоя.
5. Эталонные тесты зелёные; http/net/dns-семейства зелёные; отсутствие регрессий вне сети
   (базис-двоичник).
6. Спека-сначала на каждую фазу; конвенции §0/§3/§6/§7 соблюдены (§3: вся логика в .nv,
   C — только непортируемый транспорт).

## Ф.0-карта, заход 1 (2026-07-06)

Инвентарь снят по `compiler-codegen/nova_rt/net.c` (1548 строк) + `net.h` (354) +
`std/net/*.nv`. **Поправка к §1:** «174 функции» — историческая цифра; на факт
сейчас **68** `NovaRt_*_(method|static)_*` (grep) + **53** literal-name entry point
(`^extern "C" fn` в `ffi.nv`). Двойная обёртка (Д1) = именно эти два слоя.

### Ф.0-1. Статические результатные слоты (Д2) — все 6

| Слот (net.c) | Тип | Что переносит | Куда в новом слое |
|---|---|---|---|
| `_net_tcp_read_data` :67 | `nova_str` | данные последнего TCP read | **удалён** — read пишет в буфер вызывающего (§2а) |
| `_net_recv_data` :1379 | `nova_str` | данные последней UDP-датаграммы | **удалён** — recv пишет в буфер вызывающего |
| `_net_recv_sender` :1380 | `SocketAddr*` | адрес отправителя UDP | **удалён** — out-param `NovaNetAddr* sender` |
| `_net_dns_addrs` :1453 | `SocketAddr**` | массив DNS-результата | **удалён** — nova_alloc-массив, возврат `count` + `**out_arr` |
| `_net_parse_result` :1244 | `SocketAddr*` | результат parse | **удалён** — out-param `NovaNetAddr* out` |
| `_net_tls_last_error` :1212 | `char[4096]` | текст последней ошибки | **удалён** — код возврата (UV int); текст через `nova_net_strerror(code, buf, cap)` в буфер вызывающего |

Инвариант приёмки: `grep -E "__thread|__declspec\(thread\)" net2.c` = **0**.
(в net.c их 6 результатных + это ровно источник M:N-гонки Д2.)

### Ф.0-2. Как устроены парковка и alloc_cb-путь СЕЙЧАС (что копируется)

**Парковка (корректна — переиспользуется как есть).** Каждая блокирующая опера
(connect/accept/read/write/recv/send/dns) по образцу `_nova_sleep_via_libuv`:
`_nova_active_scope`/`_nova_active_slot` → `nova_sched_register_pending(scope, slot,
handle, stop_cb)` → `nova_sched_park` → (libuv-cb на loop-потоке пишет в
`handle->…` и зовёт `nova_sched_wake(scope, slot)`) → resume →
`nova_sched_unregister_pending` → проверка `cancel_requested`. Отмена: `stop_cb`
делает CAS stage IDLE→CLOSING и `nova_loop_defer_close` (cross-thread-safe,
Plan 83.10.2). Cancel-scope берётся через `_nova_net_cancel_scope` (родительский
supervised). **Это M:N-корректно** — park-слот в scope, а не в потоке. Гонка НЕ
здесь. Механизм копируется в net2.c дословно (общий helper `_nova_net_cancel_scope`,
stage-enum, defer_close).

**alloc_cb-путь чтения СЕЙЧАС (Д2+нарушение §2а — три лишних переноса).**
`read_bytes` → `uv_read_start(alloc_cb, read_cb)`; `_tcp_alloc_cb` (:538)
**`malloc(cap)`** промежуточный буфер → `read_cb` пишет `read_len` → после park
код **`nova_alloc(len+1)` + `memcpy`** из malloc-буфера в GC-строку → `free` →
упаковка `nova_str*` в `nova_int` → в TLS `_net_tcp_read_data` → аксессор
`tcp_stream_read_data()`. **Итог: malloc + memcpy + nova_alloc на КАЖДЫЙ read.**
UDP recv — тот же паттерн (`_udp_alloc_cb` :1036). Запись: `write` (:661)
**`malloc(len)`+`memcpy`** копию пользовательских данных, держит до `write_cb`,
`free`. **Итог: malloc+memcpy на КАЖДЫЙ write.**

### Ф.0-3. Новый слой = zero-copy (модель Go/Rust, образец std/fs)

`std/fs/ffi.nv` уже доказал форму: `fs_read(fd, buf *mut u8, len) -> int`
(≥0=байты / <0=−errno), `fs_write(fd, buf *u8, len) -> int` — сеть-в-буфер
вызывающего, ноль промежуточных. Копирую её на сеть:

- **read**: `nova_net_tcp_read(void* s, uint8_t* buf, int64_t cap) -> int64_t`.
  `alloc_cb` отдаёт `uv_buf_init(s->read_ptr, s->read_cap)` — тот самый буфер
  Nova (указатель+ёмкость сохранены в handle перед `uv_read_start`); `read_cb`
  ставит `read_n`, `uv_read_stop`. Возврат: `n>0` байт, `0`=EOF, `<0`=−UV-код.
  **malloc=0, memcpy=0, nova_alloc=0.**
- **write**: `nova_net_tcp_write(void* s, const uint8_t* buf, int64_t len) -> int64_t`.
  `uv_write` получает `uv_buf_init((char*)buf, len)` — прямо память `[]u8`; на
  время операции указатель живёт в `write_req`, а сам буфер держит Nova-вызывающий
  на стеке волокна (консервативный GC его видит). Возврат `n`/`<0`. **0 копий.**
- Буфер `[]u8` вызывающего: Nova-сторона (Ф.2) владеет `mut buf []u8`, передаёт
  `buf.as_mut_ptr()`+ёмкость (read) / `buf.as_ptr()`+len (write) — как fs.

### Ф.0-4. Таблица «старая → новая сигнатура» (аллокации/копии)

**Адреса** (`NovaNetAddr` = будущий `SocketAddr value`; в C-слое передаётся
указателем на nova_alloc/стек-структуру, поля — данные, НЕ handle):

| Старое (net.c) | Новое (net2.c) `nova_net_*` | копии данных |
|---|---|---|
| `socket_addr_loopback(port)->CSocketAddr` | `nova_net_addr_loopback(u16)->NovaNetAddr*` | 0 (конструкция) |
| `socket_addr_loopback_v6` | `nova_net_addr_loopback_v6(u16)->NovaNetAddr*` | 0 |
| `socket_addr_v4(a,b,c,d,port)` | `nova_net_addr_v4(...)->NovaNetAddr*` | 0 |
| `socket_addr_parse(str)->int`+`_parse_result()` TLS | `nova_net_addr_parse(u8*,len, NovaNetAddr* out)->int` | 0 (TLS убран) |
| `socket_addr_port` | `nova_net_addr_port(NovaNetAddr*)->u16` | 0 (в Ф.2 — чистый Nova) |
| `socket_addr_ip->str` | `nova_net_addr_ip(NovaNetAddr*, u8* buf, cap)->int` | 0 (в буфер вызывающего) |
| `socket_addr_is_v4/_is_v6` | `nova_net_addr_is_v4/_is_v6->bool` | 0 |
| `socket_addr_to_str->str` | `nova_net_addr_to_str(NovaNetAddr*, u8* buf, cap)->int` | 0 |

**TCP** (handle = `void*`; ошибка read/write = `<0` UV-код; connect/listen/accept =
NULL + out-код через `int* out_err` — cold path):

| Старое | Новое | копии |
|---|---|---|
| `tcp_listener_bind`+TLS-err | `nova_net_tcp_listen(NovaNetAddr*, int backlog, int* out_err)->void*` | 0 |
| `tcp_listener_accept` | `nova_net_tcp_accept(void* lst, int* out_err)->void*` (parks) | 0; peer=1 named OS-перенос при запросе |
| `tcp_stream_connect` | `nova_net_tcp_connect(NovaNetAddr*, int* out_err)->void*` (parks) | 0 |
| `tcp_stream_read_bytes`+`_read_data()` TLS | `nova_net_tcp_read(void*, u8* buf, cap)->int64` (parks) | **0** (§2а) |
| `tcp_stream_write` / `_write_all` | `nova_net_tcp_write(void*, u8* buf, len)->int64` (parks) | **0** (§2а) |
| `tcp_stream_close` | `nova_net_tcp_close(void*)` (refcount-aware) | 0 |
| — (новое) | `nova_net_tcp_shutdown(void*, int* out_err)` (uv_shutdown, half-close) | 0 |
| `tcp_stream_local/peer_port` | `nova_net_tcp_local/peer_port(void*)->u16` | 0 |
| `tcp_stream_local/peer_addr` | `nova_net_tcp_local/peer_addr(void*, NovaNetAddr* out)` | 1 named (sockaddr→addr) |
| `tcp_stream_set_nodelay/_keepalive` | `nova_net_tcp_set_nodelay/_keepalive(void*, bool)` | 0 |
| `tcp_listener_local_port/_addr/_close` | `nova_net_listener_local_port/_addr/_close` | 0/1-named/0 |
| `tcp_listener_set_reuse_address` | `nova_net_listener_set_reuse_address(void*, bool)` | 0 |

**TCP split (Д1-упрощение).** 12 функций `tcp_read_half_*`/`tcp_write_half_*` +
`tcp_stream_split` в net.c — **исчезают**. В новом слое у handle с рождения
раздельные `read_scope`/`write_scope`, поэтому read и write независимы БЕЗ split-API;
«split» на Nova-стороне = раздать один `void*` двум half-значениям.
C добавляет лишь `nova_net_tcp_mark_split(void*)` (refcount=2) — close по refcount.

**UDP**:

| Старое | Новое | копии |
|---|---|---|
| `udp_socket_bind`+TLS-err | `nova_net_udp_bind(NovaNetAddr*, int* out_err)->void*` | 0 |
| `udp_socket_send_to(str)` | `nova_net_udp_send_to(void*, u8* buf, len, NovaNetAddr*)->int64` (parks) | **0** (§2а; было malloc+memcpy) |
| `udp_socket_recv_from`+`_recv_data()`+`_recv_sender()` TLS | `nova_net_udp_recv_from(void*, u8* buf, cap, NovaNetAddr* sender)->int64` (parks) | **0** в буфер; sender=1 named OS-перенос |
| `udp_socket_local_port/_addr/_close` | `nova_net_udp_local_port/_addr/_close` | 0/1-named/0 |

**DNS**:

| Старое | Новое | копии |
|---|---|---|
| `dns_lookup`+`dns_addr_at()` TLS | `nova_net_dns_lookup(u8* host, len, u16 port, NovaNetAddr** out_arr, int* out_err)->int64 count` (parks) | 1 named OS-перенос: `addrinfo`→GC-массив `NovaNetAddr`; TLS убран, массив передан явно |

**Ошибки/текст**: `net_last_error()` TLS **удалён**. Опы возвращают UV-код
(<0 / через `int* out_err`). Текст строит Nova-сторона: `nova_net_strerror(int code,
u8* buf, int64_t cap) -> int` (обёртка `uv_strerror`, канон-строки для
EACCES/ECONNRESET как раньше) пишет в буфер вызывающего. 0 статики.

### Ф.0-5. Неизбежные копии (поимённо, как у Rust/Go) — итог

1. `accept`/`*_peer_addr`/`*_local_addr`: `sockaddr_storage` → `NovaNetAddr`
   (≤16 байт), **только по запросу адреса**, не в hot-path read/write.
2. `udp recv_from`: sender `sockaddr` → `NovaNetAddr` (out-param, 1 шт/датаграмму).
3. `dns_lookup`: `addrinfo`-список → GC-массив `NovaNetAddr` (N адресов, 1 раз).

В hot-path **read/write/send/recv-payload**: malloc=0, memcpy=0, nova_alloc=0.

### Ф.0-6. Инвентарь потребителей (кого мигрировать в Ф.3)

- `std/net/ffi.nv` (53 extern), `addr.nv`, `error.nv`, `tcp.nv` (real_net-handler,
  ~37 ops), `udp.nv`, `dns.nv`, `mock.nv` — переписываются на новый слой/байты.
- `std/http/transport/real.nv`, `std/http/servernet/servernet.nv` — единственные
  внешние потребители эффекта `Net` (grep). Мигрируют на байтовый surface.
- Тесты: `nova_tests/plan83_12/*` (tcp_*), `nova_tests/plan91_12/net_v2_*`
  (tcp/udp/dns smoke+slow), `plan91_15/*`, `plan91_16/*` (split).
- `examples/net/echo_{client,server}.nv`.

### Ф.0-7. Точка входа Ф.1 (этот заход)

Новый файл `nova_rt/net2.c`+`net2.h` рядом со старым (net.c НЕ трогаю — на нём все
потребители). Порядок: TCP (addr-хелперы → listen/accept/connect/read/write/close/
shutdown/mark_split/ports/addrs) → UDP → DNS. Линковка net2.c во все 3 toolchain-блока
test_runner.rs по образцу net.c. Smoke — Nova-тест на НОВОМ слое (extern "C" nova_net_*
напрямую, только proven-типы `*()`/`*mut u8`/`*u8`/int): loopback echo двумя волокнами
(тот самый, что на старом слое сегфолтит). .nv-обвязка эффекта (Ф.2) — namespaced-новая,
если влезет; иначе следующий заход.

### Заход 1 (2026-07-06) — итог и точка возобновления

**Сделано:** Ф.0 целиком (карта выше + амендмент D407) · Ф.1 целиком —
`nova_rt/net2.{c,h}` (TCP listen/accept/connect/read/write/shutdown/close/
mark_split/ports/addrs + UDP bind/send_to/recv_from/ports + DNS lookup + адреса/
strerror), линковка в 3 toolchain-блока test_runner.rs, прототипы в nova_rt.h ·
Начало Ф.2 — модуль `std/net2` (ffi.nv: весь extern-слой; тесты `*_test.nv`
рядом с модулем по конвенции владельца 2026-07-06).

**Найдено и исправлено два субстратных дефекта (сверх списка Д1-Д3):**
1. *Implicit-decl truncation:* extern "C"-вызов без прототипа в TU → C-компилятор
   обрезает возвращаемый указатель до 32 бит (SEGV на первом же прогоне).
   Фикс: net2.h включён в nova_rt.h (как net.h). Симптом ловится NOVA_DIAG_SEGV.
2. *Lost-wake парковки (вероятный второй корень [M-178-...]-класса зависаний):*
   `nova_sched_wake` находит волокно через `parked_co[slot]`, выставляемый только
   ВНУТРИ park; libuv-колбэк на потоке цикла, выстреливший между запуском uv-опа
   и park, находит NULL → wake молча теряется → вечная парковка (поймано:
   UDP-смоук TIMEOUT ~3/10). Наивный порядок net.c (publish-scope ПОСЛЕ issue,
   одноразовый `nova_sched_park`) имеет эту дыру в КАЖДОЙ операции. Фикс в net2:
   publish scope/slot + сброс атомарного done-латча ДО запуска опа; колбэк
   пишет результаты → латч → wake; парковка через `nova_sched_park_until`
   (predicate: done || stage>=CLOSING). После фикса 15/15 харнесс-прогонов.

**Гейты захода:** conformance 54/0 (=базис «до») · smoke TCP echo двумя волокнами
на новом слое 20/20 подряд (бинарь) + 15/15 (харнесс, все 3 теста: 2×TCP+1×UDP,
двух-волоконные) · старый корпус plan91_12/15/16 в изоляции 0 FAIL · net.c не
тронут · cargo release чистый. plan83_12 не компилируется из-за до-существующего
P67-LEGACY ICE на bind-путях (закрывается в Ф.4, см. план). Старый echo-тест
(net_v2_tcp_echo_slow) в этом окружении на одиночном прогоне прошёл — истор.
сегфолт [M-178-...] был на plan-178 servernet-сценарии; контраст-клейм строится
на 20/20 нового слоя, не на пере-репро старого.

**Возобновление (заход 2 = Ф.2):** публичная .nv-обвязка в `std/net2` поверх
ffi.nv — value-`SocketAddr` (20-байтная запись; парсинг/форматирование через
буферные externs), `TcpStream`/`TcpListener`/`UdpSocket` (consume, D133), эффект
`Net` с `[]u8`-сигнатурами, `real_net()`/`mock_net()`, текст-хелперы через
`Result[str, Utf8Error]`. Все суммы — `type X enum A | B | C` (D406, директива
владельца 2026-07-06; в ffi.nv/тестах сумм нет — проверено). out_err-указатели:
дать честный канал кода ошибки (в Ф.1-тестах передаётся null). Затем Ф.3
миграция потребителей + удаление старого слоя; Ф.4 стресс M:N + эхо-замер.

### Заход 2 (2026-07-06) — итог и точка возобновления (Ф.2 обвязка)

**Сделано (Ф.2 целиком):** публичный `.nv`-слой `std/net2` поверх ffi.nv —
- **value-`SocketAddr`** = `value { priv raw []u8 }` (20-байтный NovaNetAddr-образ,
  данные не handle; конструкция/parse/format через буферные externs) — закрывает
  `[M-net-socketaddr-value-record]` для нового слоя;
- **consume-типы** `TcpListener`/`TcpStream`(+`TcpReadHalf`/`TcpWriteHalf`)/`UdpSocket`,
  must-consume `@close()`; split = один handle двум half-значениям + refcount (D407 §6);
- **эффект `Net`** с `[]u8`-сигнатурами (`read(stream, buf mut []u8)->Result[int]`,
  `write(stream, data []u8)->Result[int]`; никакого `str` в транспорте; addr-геттеры
  и UDP-sender заполняют 20-байтный буфер, не пересекают vtable как multi-word return);
- **честный out_err-канал** (заход-1 null-заглушка убрана): UV-код → `NetError` через
  `NetError.from_code` = `nova_net_strerror` + классификация по канон-строке; суммы `enum` (D406);
- **триада** `real_net()`/`mock_net()`; **DNS** `resolve()` — один `getaddrinfo`, C выделяет
  GC-массив точного размера + count (без повторного запроса, замечание владельца), `.nv`
  строит `[]SocketAddr` в `resolve()` (вне vtable); **текст-хелперы** через `str.from_bytes`.
- **out_err / DNS-base** — через `&local` (`*mut int`, escape-промоут D216 §4), без вектор-ячеек
  (замечание владельца). C `net2.{c,h}`: additive `_into`-конструкторы, `addr_copy_at`, DNS назад
  на GC-массив+count. **net.c/std/net не тронуты; test_runner не тронут.**

**Гейты:** conformance `--positive --compile-error` = **54/0** (= базис fe631543) ·
TCP echo двумя волокнами через эффект детерминированно зелёный (10/10) · DNS/mock/addr/neg
детерминированно зелёные · neg double-close → **D131** (use-after-consume; форма: `consume`-binding
через `_open`-экстрактор в neg-модуле).

**НАЙДЕННЫЕ ДЕФЕКТЫ СУБСТРАТА/КОМПИЛЯТОРА (для Ф.3/Ф.4 и владельца):**
1. **UDP-флейк (net2.c Ф.1, Windows-libuv):** конкурентный send/recv+close даёт `reqs_pending>0`
   assertion (митигировано: close ПОСЛЕ supervised, не в spawn) + остаточный **TIMEOUT ~2/10**
   (lost-wake/потеря датаграммы в recv-пути под concurrency). TCP-путь стабилен (0/8). Это Ф.1-substrate
   дефект / Ф.4 M:N-территория — **UDP-тесты закоммичены, но флейк открыт**.
2. **GC-liveness `[]SocketAddr`:** образы-`[]u8` внутри `Vec[SocketAddr]` освобождаются GC, если
   (a) Vec пересекает vtable как Ok-payload, ИЛИ (b) извлекается generic `_must[T]` (erasure).
   Митигации в коде: DNS строит Vec в `resolve()` (не в handler), через vtable идёт скаляр-пара
   `(base,count)`; тесты — прямой `match` (не generic `_must`) + чтение значений up-front. Корень —
   компиляторная GC-трассировка сквозь `Vec[value-struct-with-heap]` / generic-erasure. Инлайн
   `[20]u8`-rep убрал бы проблему, но литерал `[0; N]` — известный gap компилятора (crypto FAIL).
3. **`unwrap()` на typed-error:** pre-existing gap — `Result[_, XError].unwrap()` эмитит
   `Nova_Fail_fail(str)` и не компилится (даже `parse_int().unwrap()`). Тесты — `match`/`_must`-хелпер.
4. **resize-инференс:** `mut b = []u8.new()` (inferred) НЕ персистит `resize`; обязательна
   аннотация `mut b []u8 = []u8.new()` (иначе len=0 → null_buf → порча). Все буферы аннотированы.

**Возобновление (заход 3 = Ф.3):** миграция потребителей `std/http/transport/real.nv` +
`std/http/servernet/servernet.nv` + net-тесты (plan83_12/91_12/15/16) + `examples/net/*` на `std/net2`;
удалить net.c/std/net/ffi.nv-двойную-обёртку/str-опы/`NovaRt_*_method_*`; grep-инварианты §2 = 0.
**Перед Ф.3 или в Ф.4:** починить UDP-substrate (#1) и рассмотреть инлайн-SocketAddr после закрытия
`[0;N]`-gap (#2). unwrap-gap (#3) и resize-инференс (#4) — отдельные компиляторные задачи.

### Заход 3 (2026-07-06) — итог Ф.3 (миграция потребителей, БЕЗ удаления старого слоя)

**Мигрировано на `std.net2`:**
- `std/http/transport/real.nv` — `import std.net2.{TcpStream, resolve}` (был
  `SocketAddr.lookup` → теперь свободная fn `resolve()`); `write_all(str)` →
  `write_all(request.as_bytes())`; `read(65536)->str` → `read_to_vec(65536)->[]u8`
  (цикл переписан на `Ok(0)`-EOF вместо старого `Err(Eof)`, т.к. байт-поверхность
  D407 возвращает `Ok(0)` на чистый EOF, а не `Err`).
- `std/http/servernet/servernet.nv` — `import std.net2.{TcpListener, TcpStream,
  NetError}`; `write_all_bytes`→`write_all`, `read_bytes`→`read_to_vec`; `.len`
  (голое поле, старый стиль) → `.len()` везде.
- Тесты-потребители (не в исходной Ф.0-карте, но ловятся сигнатурой
  `handle_connection`/`real_http` — без миграции не компилировались бы):
  `nova_tests/http_transport/transport_test.nv` (`std.net.mock_net` →
  `std.net2.mock_net`), `nova_tests/http_servernet/servernet_smoke_test.nv`
  (`std.net.{TcpListener,TcpStream,SocketAddr}` → `std.net2.*`, `write`→`write_str`,
  `read_bytes`→`read_to_vec`).
- **Побочный фикс (не net2-специфичный):** `servernet_smoke_test.nv` не имел
  `with Net = real_net() { … }` вокруг теста (баг существовал и на старом `std.net`
  — воспроизведён идентично) → `TcpListener.bind` диспетчерил на null-vtable-слот
  → SEGV (`Nova_Net_tcp_bind`, подтверждено `NOVA_DIAG_SEGV`). Ранее это
  списывалось на «M:N net-substrate segfault» ([M-178-servernet-live-net-substrate-segfault]);
  реальный корень — отсутствующий handler-install, НЕ M:N-гонка. Фикс: обернуть
  тело в `with Net = real_net()`. 5/5 детерминированных прогонов после фикса.
- `examples/net/echo_client.nv` / `echo_server.nv` — были УЖЕ СЛОМАНЫ до этого
  захода (0 импортов вообще — `SocketAddr`/`TcpListener` не резолвились,
  `nova build` падал ICE на unresolved-symbol legacy-fallback; `unwrap()` на
  `Result[_, NetError]` тоже был бы недостижим, gap #3). Переписаны: явные
  `import std.net2.{…}`, `with Net = real_net() { … }`, `match` вместо `unwrap()`,
  байт-поверхность (`write_str`/`read_to_vec`). `nova check` → PASS на обоих.
  `nova build` **не удалось верифицировать бинарём** — упирается в отдельный,
  пре-существующий (НЕ Ф.3) ICE, см. `[M-183-nova-build-consume-effect-close-ice]`
  в `docs/plans/backlog-followups.md` (репродуцирован идентично и на старом `std.net`,
  вне зависимости от net2 — общий разрыв `nova build` vs `nova test` в тайпчеке
  consume-результатов effect-операций). Логика подтверждена эквивалентным
  `test{}`-паттерном (accept/read/write/close) зелёным в `http_servernet` +
  `plan91_12/net_v2_tcp_echo_slow`.

**НЕ мигрировано (по карте, намеренно):** `nova_tests/plan83_12/*`,
`nova_tests/plan91_12/*`, `plan91_15/*`, `plan91_16/*`,
`nova_tests/plan178/net_byte_surface_mock.nv` — остаются на `std.net` до
санации Plan 182.

**Старый слой НЕ удалён** (потребители из списка выше ещё живы): `std/net/*.nv`
получили баннер `// DEPRECATED (план 183 Ф.3): модуль заменён std/net2;
удаление после санации nova_tests (план 182).`; остаток («физическое удаление
net.c + std/net + `NovaRt_*_method_*`» + grep-инварианты + возможный
namespace-ренейм `net2`→`net`) зафиксирован как
`[M-183-old-net-removal-after-182]` в `docs/plans/backlog-followups.md`.

**Гейты захода:** conformance `--positive --compile-error` = **54/0** (базис
не изменился — Rust-компилятор не трогался, только `.nv`/`.md`) · http-семейство
(`nova_tests/{http,http_transport,http_server,http_typed,http_decompress,
http_servernet/servernet_smoke_test}` + `std/net2/tcp_test` + `std/http/client`)
= **8/8 PASS** (было 5/5 до захода — 3 новых зелёных: `http_servernet` (был
скрыто мёртв), `tcp_test`, `client`, все без регрессий) · дельта против
до-Ф.3-состояния: **0 новых FAIL** (Rust-бинарь не менялся, разница чисто в
`.nv`-миграции).

**Новый найденный дефект (не Ф.3-специфика, задокументирован):**
`[M-183-nova-build-consume-effect-close-ice]` — `nova build` (в отличие от
`nova test`) падает ICE на `mut x = match Effect.op(...) {…}.close()` для
ЛЮБОГО эффекта с consume-результатом (репродуцировано на старом `std.net`
тоже) — блокирует бинарную верификацию `examples/net/*`, но НЕ является
регрессией этого захода.

**Возобновление (Ф.4):** M:N-стресс + эхо-замер пропускной способности;
UDP-substrate флейк (заход 2, #1); `[M-183-old-net-removal-after-182]`
гейтуется на Plan 182 (санация nova_tests) — тогда удалить старый слой +
namespace-ренейм `net2`→`net`.

### Заход 4 (2026-07-06) — итог Ф.4 (корень UDP-флейка + M:N-стресс + замер)

**Корень UDP-флейка НАЙДЕН (факт, трейсом): loop-affinity, НЕ lost-wake и НЕ
потеря датаграммы.** Изолированный однотестовый бинарь воспроизводил
TIMEOUT ~1/40 (seq) / ~1/96 (16-way parallel). Временные printf-трейсы в
net2.c дали смокинг-ган: датаграмма ДОСТАВЛЕНА (`recv_cb nread=9`), а
`send_cb` (или, в другом прогоне, `recv_cb`) вообще не выстреливал; при этом
оба сокета `bound-on-loop = _evloop` (main-thread), а `send_to`/`recv_from`
выдавались из spawn-волокон с worker-loop'ами. Watchdog-дамп: зависшее
волокно `parked=1 pstate=WAIT hdl=0 stop_cb=0` (= send-путь, парковка без
register_pending). Механика: uv-handle пришпилен к loop'у создания
(`nova_current_loop()` на bind), libuv-loop'ы не thread-safe (единственный
безопасный cross-thread вход — `uv_async_send`); uv-оп, выданный worker-волокном
на main-loop-handle, конкурирует с `uv_run(UV_RUN_ONCE)` main-thread'а в
supervised-drain → req теряется, completion не приходит, `park_until`-предикат
никогда не истинится. Заход-1 lost-wake-латч в UDP-пути ПРИСУТСТВУЕТ и
корректен — класс другой. TCP не флейкал, т.к. `connect`/`accept` создают
stream на loop'е текущего worker'а.

**Фикс (паттерн M:N, тест-сторона + контракт в субстрате):** сокет создаётся
ВНУТРИ волокна, которое им оперирует (`handle.loop` == loop оперирующего
worker'а); порты передаются буферизованными каналами; паркующий канальный оп
только ДО bind'а или ПОСЛЕ uv-опа (не между bind и uv-вызовом). `udp_test.nv`
переписан (оба теста). Контракт задокументирован в заголовке `net2.c`
(«LOOP-AFFINITY CONTRACT»). Изолированный репро после фикса: **60/60 seq +
128/128 16-way-parallel** (было 1/40, 1/96). Остаточный узкий класс
(steal-миграция волокна между park'ами → следующий оп с чужого worker'а) и
полный субстратный фикс (defer-op-маршалинг на owning-loop-thread, обобщение
`nova_loop_defer_close`) — `[M-183-net2-loop-affinity-cross-thread-op]`
(backlog, P2). Свойство UDP из плана («датаграмма может теряться») НЕ
подтвердилось — loopback-датаграмма доставлялась во всех пойманных прогонах.

**M:N-стресс (`std/net2/stress_test.nv`, новый):**
1. 8 клиент-волокон × конкурентные echo-обмены через ОДИН listener под
   work-stealing; каждое волокно шлёт свой 8-КиБ узор `(fid*31+k*7)%251` и
   побайтно верифицирует эхо (класс «чужие байты» старого TLS-слоя поймался бы
   на assert'ах). Сервер-волокно: bind-in-fiber, 8 последовательных accept+echo.
2. Замер-ориентир §2а/§4а: **~600 MiB/s** (589/616/603 в трёх прогонах;
   8 МиБ ping-pong чанками 64 КиБ через одно loopback-соединение, ~13 мс;
   Dev-режим C, один поток данных). Старый слой БЕЗ правок такой замер не
   прогоняет (нет эквивалентного теста; plan91-echo — smoke, не замер) —
   по плану зафиксирован только новый как базовая точка.

**Стресс вскрыл НОВЫЙ компиляторный дефект (SEGV, не сеть) — ✅ CLOSED 2026-07-06:**
`mibps.to_str()` на **int** внутри модуля `std.net2` (где определён
`NetError @to_str`) разрешался в `Nova_NetError_method_to_str(mibps)` —
int уходил как указатель на enum, FaultAddress = значению int'а (mibps=12 →
0xC), SEGV в `println`-конкатенации. **Корень:** чекер (`infer_expr_type`) не
выводил return-тип вызова эффект-операции (`Time.now_monotonic_ns()`) → `mibps`
оставался без типа в scope → примитивный gate `[E_UNKNOWN_METHOD]` пропускался →
codegen coarse-by-name (`method_receivers` last-wins) диспатчил на чужой `to_str`.
**Фикс (§0):** effect-op arm в `infer_expr_type` возвращает объявленный return-тип
операции → `mibps: int` известен чекеру → чистый `[E_UNKNOWN_METHOD]` (int не
владеет `to_str`; конверсия — `str.from`/`${...}`). `${...}`-обход в тесте остаётся
(правильный итог — clean checker-error, а не рабочий вызов). Тесты:
`plan183_f4/effect_op_int_result.nv` (pos) + `plan183_f4/neg/
int_to_str_effect_collision_neg.nv` (neg, `EXPECT_COMPILE_ERROR E_UNKNOWN_METHOD`).
`[M-183-int-to-str-module-method-collision]` — CLOSED (детали:
`docs/dev/simplifications.md`).

**P67-LEGACY ICE (plan83_12) на net2 — класса НЕТ.** Старый слой: `nova test
nova_tests/plan83_12/tcp_bind_used_port_test.nv` → ICE `[P67-LEGACY] Path call
return type unknown for method=bind` (репро 2026-07-06). net2-эквивалент
(новый `nova_tests/plan183_f4/net2_bind_used_port_test.nv`: тот же
bind-to-used-port shape, TCP+UDP) — компилируется и PASS. Класс уходит вместе
с удалением старого слоя (`[M-183-old-net-removal-after-182]`).

**Гейты захода:** conformance `--positive --compile-error` = **54/0** ·
бинарь всего модуля std.net2 (19 тестов: 2×udp, 6×tcp, 2×stress, addr/dns/mock)
**20/20 подряд** · харнесс: udp_test **5/5**, tcp_test **5/5**, stress_test
**3/3** · plan183_f4 PASS · http-семейство (http, http_transport, http_server,
http_typed, http_decompress, http_servernet, std/http/client) **7/7 PASS** ·
дельта против базиса 68151871: **0 новых FAIL** (Rust-бинарь не менялся;
net2.c — только комментарий-контракт; временные трейсы убраны) ·
Ф.4-приёмка §4/§4а закрыта (M:N smoke+стресс детерминированно зелёные,
эхо-замер зафиксирован).

**Остаток плана:** Ф.5 (журнал/спека/закрытие) + `[M-183-old-net-removal-after-182]`
(гейт Plan 182) + backlog-хвост Ф.4 (`loop-affinity` P2). `int-to-str-collision` P1
— ✅ CLOSED 2026-07-06 (см. выше).

## 5. Риски / связи

- **Объём**: net.c ~2000 C-строк + 1100 .nv + потребители; 3-4 агент-захода. Самая тонкая
  часть — сохранить парковку/отмену нетронутыми (они корректны).
- **Параллельная работа**: `supervised(deadline:/timeout:)` (идёт сейчас) — независим
  (областной механизм); TLS (план 116) — строго ПОСЛЕ этого плана, на новом слое.
- **Мок-слой**: `mock_net()` переписывается на `[]u8` — тесты http это уже умеют
  (mock_http данные-ориентирован).
- Известный ICE `[M-codegen-*]` на комбинированных CU нейтрализован D381 — новой сети не
  мешает.

## Ф.5 — закрытие (журнал/спека/задачник, 2026-07-06)

**Статус фаз:** Ф.0 ✅ · Ф.1 ✅ · Ф.2 ✅ · Ф.3 ✅ · Ф.4 ✅ — все закрыты, гейты см. в
журнале «Заход 1-4» выше (каждый заход: conformance 54/0 + подсистемные гейты, 0 регрессий
против предыдущего базиса). **Остаток плана — ровно один пункт:**
`[M-183-old-net-removal-after-182]` (физическое удаление `net.c`/`std/net/*.nv`/
`NovaRt_*_method_*` + namespace-ренейм `net2`→`net`), гейтованный на санацию
`nova_tests` в Plan 182 (тесты `plan83_12`/`plan91_12`/`plan91_15`/`plan91_16`/
`plan178/net_byte_surface_mock` держат старый слой живым намеренно, по директиве Ф.3).

**Спека:** D407 (`spec/decisions/04-effects.md`) сверен построчно с фактическим
`net2.c`/`std/net2/*.nv` и амендирован (2026-07-06): явный **20-байтный** образ
`NovaNetAddr` (было «≤16 байт», округлено без общего числа); DNS `resolve()` — явно
**один** `getaddrinfo`-вызов (без повторного/угадывающего запроса); refcount-механика
split-close (`split_refcount`, `nova_net_tcp_mark_split`) детализирована; добавлен
**loop-affinity контракт**, найденный Ф.4-стресс-тестом (см. amend к пункту 7)
— он отсутствовал в исходном D407 целиком, хотя обнаружен уже во время реализации плана.

### Свод найденных дефектов (маркер → статус → приоритет)

| Дефект | Маркер | Статус | Приоритет |
|---|---|---|---|
| Implicit-decl truncation (extern "C" без прототипа → указатель обрезан до 32 бит) | — (фикс на месте, заход 1: `net2.h` в `nova_rt.h`) | ✅ ЗАКРЫТ | — |
| Lost-wake парковки (libuv-колбэк между issue и `park` видит `parked_co[slot]==NULL`) | — (фикс на месте, заход 1: publish scope/slot + done-латч ДО issue, `nova_sched_park_until`) | ✅ ЗАКРЫТ | — |
| UDP-флейк — loop-affinity (uv-handle пришпилен к loop создания; cross-thread-оп теряет completion) | `[M-183-net2-loop-affinity-cross-thread-op]` | контракт задокументирован + тесты приведены к нему (60/60+128/128); полный субстратный фикс (defer-op-маршалинг) открыт | P2 |
| `int.to_str()` внутри `std.net2` резолвится в одноимённый `NetError@to_str` (same-module method collision) → SEGV | `[M-183-int-to-str-module-method-collision]` | OPEN — по имеющимся сведениям в работе (вне периметра плана 183/этого захода) | P1 |
| GC-трассировка `Vec[value-record c heap-полем]` не переживает vtable-Ok-payload / generic `_must[T]`-erasure | `[M-183-gc-vec-value-heap-tracing]` (новый, добавлен Ф.5) | OPEN — митигировано в коде (DNS строит `Vec` вне vtable, тесты избегают generic `_must`), фикс компилятора не начат | P2 |
| `nova build` ICE на consume-результате effect-операции (`mut x = match Effect.op(){...}; x.close()`) | `[M-183-nova-build-consume-effect-close-ice]` | OPEN — идентично воспроизводится и на старом `std.net`, общий разрыв build/test-путей тайпчека | P1 |
| `Result[_, XError].unwrap()` не компилируется (эмитит `Nova_Fail_fail(str)`) | `[M-183-unwrap-typed-error]` (новый, добавлен Ф.5) | OPEN — pre-existing gap, обход `match`/`_must`-хелпером во всех net2-тестах | P2 |
| `mut b = []u8.new()` (inferred) теряет эффект `resize` (len=0 → null_buf) | `[M-183-resize-inference-inferred-vec]` (новый, добавлен Ф.5) | OPEN — обход: явная аннотация `mut b []u8 = []u8.new()` (применена везде в net2) | P3 |
| `nova test nova_tests/plan83_12/*` ICE `[P67-LEGACY] Path call return type unknown for method=bind` | — (класс уходит вместе со старым слоем, не отдельный маркер) | известно, уйдёт при `[M-183-old-net-removal-after-182]` | — |

**Три новых маркера** (`gc-vec-value-heap-tracing`, `unwrap-typed-error`,
`resize-inference-inferred-vec`) существовали только как журнальная проза в
«Заход 2» этого документа — Ф.5 промотировала их в `docs/plans/backlog-followups.md` для
видимости владельцу (не были потеряны, но и не были трекаемы отдельно от плана).

**Нулевое копирование (§2а) — как достигнуто:** `alloc_cb` в hot-path read/recv отдаёт
libuv тот же срез буфера, что передал Nova-вызывающий (указатель+ёмкость сохранены в
handle перед `uv_read_start`/`uv_udp_recv_start`); write/send получают указатель прямо на
`[]u8` вызывающего (буфер жив на стеке волокна — консервативный GC его видит). Итог:
`malloc`/`memcpy`/`nova_alloc` данных в hot-path read/write/send/recv = **0** (инвариант
верифицирован по коду `net2.c`, не только декларативно). Единственные копии — поимённые
ОС-переносы (sockaddr→addr, DNS addrinfo→массив), вне hot-path. Ориентир-замер (Ф.4,
`std/net2/stress_test.nv`): **~600 MiB/s** (589/616/603 в трёх прогонах; 8 МиБ ping-pong
чанками по 64 КиБ через одно loopback-соединение, Dev-сборка, один поток данных) —
базовая точка для нового слоя (у старого `net.c` эквивалентного throughput-теста не было).

**Гейт закрытия Ф.5:** `cargo run -q --bin nova -- test --positive --compile-error
../spec_tests/conformance` из `nova-cli` = **54/0** (без изменений; правки Ф.5 —
только `.md`/докблоки, Rust/`.nv`/`.c` не тронуты).
