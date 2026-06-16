# Plan 91.16 — TcpStream split: TcpReadHalf + TcpWriteHalf

**Status:** ✅ CLOSED 2026-06-17
**D-блоки:** D301
**Зависит от:** Plan 91.12 ✅ (std/net V2 algebraic effects, TcpStream), Plan 166 ✅ (UDP split D298 — образец)

---

## Мотивация

После Plan 91.12 `TcpStream` работал только в однофайберном режиме — `connect`,
`read` и `write` делили единственную пару `op_scope`/`op_slot` в
`NovaRt_TcpStream`. Это исключало полнодуплексный паттерн: один файбер читает
входящий поток, другой одновременно пишет ответы на том же соединении.

При конкурентном read+write на одной паре slot'ов park-bookkeeping одной
операции затирает другую — тот же класс бага, что TOCTOU в UDP `send_to`
(Plan 166 / D298).

Это TCP-аналог UDP split: `TcpStream` делится на read- и write-половины с
независимыми C-side park-слотами.

---

## Ф.1 — C runtime (net.h / net.c)

**`NovaRt_TcpStream`** расширен:
- `read_scope`/`read_slot` + `read_op_error` — park-слот read-половины.
- `write_scope`/`write_slot` + `write_op_error` — park-слот write-половины.
- `volatile int32_t split_refcount` — счётчик живых половин (0=un-split / 2 / 1).

Новые типы `NovaRt_TcpReadHalf` / `NovaRt_TcpWriteHalf` — обёртки над одним
`NovaRt_TcpStream*`.

Новые literal-name entry-points:
- `tcp_stream_split` — ставит refcount=2, возвращает тот же handle.
- `tcp_read_half_read` / `tcp_write_half_write` / `tcp_write_half_write_all` —
  read/write через независимые слоты (отдельные callbacks `_tcp_split_read_cb` /
  `_tcp_split_write_cb`, паркуются на read_scope/write_scope).
- `tcp_read_half_close` / `tcp_write_half_close` — `__atomic_sub_fetch` по
  refcount; `uv_close` только когда последняя половина закрылась.
- `tcp_*_half_{local,peer}_{port,addr}` — интроспекция (делегируют существующим
  `NovaRt_TcpStream_method_*`).
- `tcp_stream_write_all` — write_all на самом потоке.

`_tcp_stream_close_cb` будит все три park-слота (op/read/write).

## Ф.2 — Nova FFI (std/net/ffi.nv)

`extern "C" fn` для всех новых C-функций (handle = `CTcpStream`, общий указатель).

## Ф.3 — Effect (std/net/effect.nv)

В `TcpNet` effect добавлены `write_all`, `split_stream`, `read_half_*`,
`write_half_*` операции.

## Ф.4 — Типы + методы (std/net/tcp.nv)

```nova
export type TcpReadHalf  consume value { priv handle CTcpStream }
export type TcpWriteHalf consume value { priv handle CTcpStream }
export fn TcpStream consume @split() TcpNet -> (TcpReadHalf, TcpWriteHalf)
export fn TcpStream mut @write_all(data str) TcpNet Blocking -> Result[(), NetError]
// + read/write/close/local_port/peer_port/local_addr/peer_addr на каждой половине
```

`real_tcp_net()` handler получил арки для всех новых операций.

## Ф.5 — Mock (std/net/mock.nv)

`mock_tcp_net()` получил stub-арки для split + half-операций.

## Ф.6 — Spec (D301)

`spec/decisions/04-effects.md` — блок D301 (API, контракт конкурентности,
семантика owning/close через refcount, write_all семантика, негативные случаи).

## Ф.7 — Тесты (nova_tests/plan91_16/)

- `tcp_split_mock.nv` — позитив (mock): split + read/write/write_all/close.
- `tcp_split_echo_slow.nv` — позитив (real loopback): сервер+клиент обмениваются
  через read/write половины (cross-fiber полнодуплексный трафик).
- `tcp_split_stream_after_split_neg.nv` — негатив: использование `TcpStream`
  после `split()` (с `consume`-binding) → D131.

Результат: 3/3 PASS (`--include-slow`). Регрессия plan91_12: 27/28
(`net_v2_tcp_multi_client_slow` флакнул под параллельной нагрузкой,
3/3 PASS в изоляции — не регрессия).

---

## Ограничения V1

- **Tuple consume-binding:** `consume (rd, wr) = s.split()` → parse error;
  `mut (rd, wr)` не отслеживается на double-consume. → double-close одной из
  половин не ловится компилятором (refcount защищает на runtime).
  Маркер `[M-91.16-tuple-consume-binding]`.

## Закрытые маркеры

- `[M-91.16-tcp-split]` / `[M-91.12-split-halves]` (TCP) ✅
- `[M-91.15-write-all]` ✅ (write_all на TcpStream + TcpWriteHalf)
