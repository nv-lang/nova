# PROGRESS — p579-net-boundary (реестр 221.1 №579, D456, план 268)

Окно: p579-net-boundary. Модель: sonnet. Worktree:
`d:/Sources/nv-lang/nova-p262` (ветка `p579-net-boundary`).

## Резюме

Проба ФАКТ 3 (`spec_tests/conformance/standalone/effect_vec_payload_survives.nv`)
повторена дословно: `3000000|8|0|21|8|7|14|8|21|`, `PASS: 1 FAIL: 0`.
Гипотеза подтвердилась: `Net.lookup` переведён на
`Result[[]SocketAddr, NetError]`, обход через `(array_base, count)` убран
вместе с оправдывавшим его комментарием.

## Фикс

- `std/src/net/effect.nv` — сигнатура `lookup`, комментарий переписан на
  честный (ссылка на пробу №579, а не на несуществующее ограничение).
- `std/src/net/tcp.nv` (`real_net`) — `lookup` строит `[]SocketAddr` через
  `build_addrs(base, count)` и возвращает его напрямую через vtable.
- `std/src/net/mock.nv` (`mock_net`) — `mock_lookup` возвращает
  `Result[[]SocketAddr, NetError]` тем же путём (`build_addrs`).
- `std/src/net/dns.nv` — `resolve` больше не собирает `[]SocketAddr` сама
  (это делает обработчик до пересечения границы), просто пробрасывает
  `Net.lookup(...)`; `lookup_ok`/`err_lookup` — новая сигнатура.

## Фикстуры

- `spec_tests/conformance/standalone/net_lookup_prints_socketaddr_values.nv`
  (новая) — `resolve("localhost", 80)` через `real_net()`, печатает
  `len>=1` и порт первого адреса. `EXPECT_STDOUT true|80|`. PASS.
- `std/src/net/dns_test.nv` / `mock_test.nv` (существующие) — уже
  проверяли `resolve` через `real_net`/`mock_net`; проходят с новой формой
  без изменений в самих тестах.

## Саботаж

Временно `build_addrs(base, count)` → `build_addrs(base, 0)` в
`tcp.nv:lookup` → `NEG-WRONG-STDOUT ... expected stdout pattern 'true|80|'
not found in: false|` (красное). Возврат → зелёное
(`PASS: 2 FAIL: 0 SKIP: 3`).

## nova test std/src/net

ДО: `PASS: 1  FAIL: 0  SKIP: 3 (skipped)` (изредка мигает pre-existing
#165, timeout в `addr.nv:143`, к `lookup` не относится — не мой код).
ПОСЛЕ: `PASS: 1  FAIL: 0  SKIP: 3 (skipped)`.

## nova check std/src

ДО и ПОСЛЕ идентичны: `PASS: 154  FAIL: 26  WARN: 62` (26 FAIL — три
`neg`-фикстуры в `std/src/net/neg/` + прочие по всему `std/src`,
намеренно проваливающиеся compile-error лейны, не регрессия).

## Остальная граница Net по D456 — честный список (не исправлено в этом окне)

Нарушают п.4 («Out-параметры»), тот же класс, что цитата D456
(`stream_peer_addr(stream, out mut []u8)`):

- `listener_local_addr(listener TcpListener, out mut []u8) -> ()`
- `stream_local_addr(stream TcpStream, out mut []u8) -> ()`
- `stream_peer_addr(stream TcpStream, out mut []u8) -> ()`
- `socket_local_addr(sock UdpSocket, out mut []u8) -> ()`
- `recv_from(sock, buf mut []u8, sender mut []u8) -> Result[int, NetError]`
  — ЧАСТИЧНО: `buf` легитимен (тот же канон, что `read`), `sender` —
  тот же адресный out-параметр, что и выше.

Должная форма (не реализовано): геттеры адресов — вернуть `SocketAddr`
напрямую (`listener_local_addr(listener) -> SocketAddr` и т.д.), собирая
образ внутри обработчика — доказано этим же окном, что `SocketAddr`
переживает vtable. `recv_from` — `recv_from(sock, buf mut []u8) ->
Result[(int, SocketAddr), NetError]` (счётчик + адрес отправителя одним
возвратом, `buf` не трогать).

НЕ нарушают: `read`/`write`/`send_to` (канон zero-copy, буфером владеет
вызывающий), все `*_port(...) -> int` (законный скаляр-счётчик, не
курсор), типизированные хендлы (`TcpListener`/`TcpStream`/`UdpSocket`).

Побочная находка (не почищено — вне явного объёма задачи 1): шапка
`effect.nv` (строки 7-13, не тронуты этим окном) обосновывает
byte-surface-форму `read`/`write` той же самой посылкой про
«erasure to nova_int», которую проба №579 поставила под сомнение для
Vec-полезной нагрузки в целом. Сама форма `read`/`write` верна по
независимой причине (канон Go/Rust zero-copy), но формулировка
обоснования не проверена этой пробой конкретно для `[]u8`/`Result[[]u8,_]`
и заслуживает отдельной проверки, а не переноса вывода по аналогии.

## Не сделано

- Остальная граница `Net` (5 операций выше) — исправление НЕ обязательно
  по брифу, только список.
- `buf mut []u8` vs `mut buf []u8` (реестр №611) — корпус не переписан,
  как и просил бриф.
- Мега-CU / полный `nova test` — не гонялся, это работа интегратора.
