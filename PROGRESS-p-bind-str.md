# p-bind-str — прогресс

Задача: str-перегрузки для сетевых конструкторов (D84) — `TcpListener.bind`,
`TcpStream.connect`, `UdpSocket.bind` принимают `str` наравне с `SocketAddr`
(решение владельца 2026-08-02). std-only, компилятор не трогать.

(NB: PROGRESS.md в корне worktree — чужой, уже закоммиченный трекер
завершённой задачи "перевод ///-комментариев std на английский"; не трогаем,
свой прогресс — в этом файле.)

## Сделано

1. `std/src/net/tcp.nv`
   - `export fn TcpListener.bind(addr str) Net -> Result[TcpListener, NetError]`
     (после существующей `bind(addr SocketAddr)`, ~строка 96) — делегат через
     `TcpListener.bind(addr.to_socket_addr()?)`.
   - `export fn TcpStream.connect(addr str) Net -> Result[TcpStream, NetError]`
     (после `connect(addr SocketAddr)`, ~строка 141) — тот же паттерн.
2. `std/src/net/udp.nv`
   - `export fn UdpSocket.bind(addr str) Net -> Result[UdpSocket, NetError]`
     (после `bind(addr SocketAddr)`, ~строка 46) — тот же паттерн.
3. `///`-доки на все три — английские, Rust-parity `ToSocketAddrs`
   упомянут, ошибка парса = тот же `NetError`, что и `str @to_socket_addr()`.
4. Тесты — `std/src/net/mock_test.nv` (mock_net(), детерминированно, без
   spawn/supervised): 6 новых тестов — bind/connect по str (успех) +
   invalid-str → Err для всех трёх (`TcpListener.bind`, `TcpStream.connect`,
   `UdpSocket.bind`), по образцу существующего `Ok(consume x) => {...
   assert(false)} / Err(_) => assert(true)` из tcp_test.nv:133-136.

## Гейты

- `nova check std/src` — **PASS: 147 FAIL: 26 WARN: 60** — байт-в-байт как
  baseline. `std.net`-модуль (весь folder — одна CU) сообщает `ok` с тем же
  1 pre-existing warning (d302_neterror_iokind_test.nv unused import), новых
  warning/error нет.
- `nova lint std/src/net/tcp.nv std/src/net/udp.nv std/src/net/mock_test.nv`
  — **0 findings**.
- `nova test std/src/net` — CC-FAIL на `std/src/net/addr.c` (codegen баг,
  `NovaRes_..._IoError` / `nova_unit` mismatch) — **подтверждено pre-existing**:
  идентичная ошибка (та же сигнатура, отличается только номер строки) на
  чистом main (`d:/Sources/nv-lang/nova/std/src/net`) БЕЗ наших правок. Не
  наша регрессия, компилятор не трогаем по заданию. targeted PASS-счёт для
  net2 тестов через C-codegen получить не удалось из-за этого блокера (нужен
  отдельный компилятор-фикс, вне мандата этого окна).

## examples/ + пакетные репы — простановка `bind("...")!!`

Разведка (read-only, без правок вне worktree — другие репы не в мандате
этого окна):

- `nova/examples/**`: 0 прямых сайтов-кандидатов (единственный
  `to_socket_addr` — `flagship/aggregator/src/main.nv:67`, строится через
  `flat_map`/`??` из интерполированной строки, не литерал-цепочка в
  `bind`/`connect`).
- `www`, `nova-tls`, `nova-http`, `nova-bigint`: 0 хитов `to_socket_addr`.
- `nova-polaris`: **11 прямых кандидатов** формы
  `TcpListener.bind("HOST:PORT".to_socket_addr()!!)!!` →
  `TcpListener.bind("HOST:PORT")!!`:
  - `src/doc_samples_test.nv:92`
  - `examples/01-hello/src/main.nv:27`
  - `examples/02-routing/src/main.nv:67`
  - `examples/03-json-api/src/main.nv:240`
  - `examples/04-middleware/src/main.nv:61`
  - `examples/05-auth/src/main.nv:105`
  - `examples/06-static-site/src/main.nv:51`
  - `examples/07-sse-stream/src/main.nv:54`
  - `examples/08-websocket-echo/src/main.nv:63`
  - `examples/09-graceful/src/main.nv:118`
  - `examples/10-mini-service/src/main.nv:143`
  - Плюс 4 хита в доках (README.md/README.ru.md/docs/overview.{md,ru.md}) —
    та же цепочка, вне .nv-скоупа.
  - НЕ кандидат: `src/serve/serve.nv:60-61` — `addr` параметр функции (уже
    `str`), парсится в промежуточную `sock`-переменную двумя строками; другой
    род упрощения (`TcpListener.bind(addr)?` напрямую), не однострочная
    цепочка.
  - Правки НЕ внесены — `nova-polaris` собирается против отдельного
    релизного nova-toolchain/std, ещё не содержащего это слияние; упрощение
    и `--strict-effects`-верификация — после мержа `p-bind-str` в `nova` main
    (отдельная задача интегратора/полярис-окна).
