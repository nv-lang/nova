<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 249 — `examples/net/socks5_http_bridge`: SOCKS5-клиент + HTTP↔SOCKS5-мост

**Статус:** 📋 НАБРОСОК (написан интегратором по слову владельца 2026-08-04; реализация НЕ
начата). **Приоритет:** P3 (новый пример; не блокер релиза v0.1).
**Источник:** реестр [221.1](221.1-bug-sweep.md) №360, `[M-socks5-client-missing]`
(`docs/plans/backlog-followups.md`) — вопрос владельца о feasibility.

## 0. Мотив

Внешний, реальный сценарий (личный инструмент владельца, детали не публикуются):
провайдер выдаёт прокси по протоколу **SOCKS5 с логином/паролем**; системные настройки
Windows принимают только **HTTP-прокси** и не умеют передавать креды для SOCKS5.
Нужен локальный мост: браузер говорит с ним как с обычным HTTP-прокси, мост сам
переупаковывает трафик в SOCKS5 и подставляет авторизацию.

```
браузер ──HTTP──> 127.0.0.1:PORT ──SOCKS5+пароль──> прокси провайдера ──> интернет
```

**Языковая ценность примера** (независимо от личного сценария): демонстрирует реализацию
чужого бинарного протокола (SOCKS5, RFC 1928/1929) поверх сырого `std.net`, и
двусторонний байтовый relay через `spawn` — класс задач, которого среди нынешних 12
примеров `nova-polaris` нет вовсе (все они — HTTP-уровень поверх готового фреймворка).

## 1. Разведка (сделана интегратором 2026-08-04, до этого плана)

- **SOCKS5-клиента нет нигде** в экосистеме — грепом подтверждено: `std`, `nova-http`,
  `nova-polaris`, `nova-tls` — ноль носителей.
- **Уже есть, переиспользуется без изменений:**
  - `std.net` (`TcpListener`/`TcpStream`/`Net`-эффект) — сырой TCP;
  - `Method.Connect` — готовый вариант `Method`-enum (`nova-http/src/method.nv:16`);
  - `ServerResponse.upgrade(fn(TcpStream) Net -> ())` (`nova-polaris/src/ws_upgrade.nv:
    110-115`) — ОБЩИЙ (не WS-специфичный) хук «захвата сокета после ответа»; структурно
    то, что нужно CONNECT-туннелю, НО...
- **Загвоздка, определяющая архитектуру:** `polaris.Router`/серверный парсер НЕ понимает
  CONNECT-цель (authority-form `host:port`, RFC 7230 §5.3.3 — без схемы и пути);
  `ServerRequest.@url()` парсит только обычный `Url`. Значит `upgrade()`-хук через Router
  подключить нельзя без доработки самого фреймворка (вне объёма примера) — **пример
  ОБЯЗАН идти поверх сырого `std.net`**, минуя `polaris` целиком.
- **Дом:** `examples/net/` (репа `nova`, категория уже существует —
  `echo_client.nv`/`echo_server.nv`), **НЕ** `nova-polaris/examples/`.

## 2. Объём V1

SOCKS5-клиент (CONNECT-команда, username/password auth, IPv4/domain-адреса — IPv6
опционально) + HTTP-мост (CONNECT-туннель — основной путь; обычный HTTP через прокси —
вторичный, как в мотивирующем сценарии). **НЕ входит:** SOCKS5-сервер (только клиент);
SOCKS4/4a; GSSAPI-аутентификация (RFC 1961, редкая в проде); UDP ASSOCIATE.

## 3. Дизайн

### 3.1 `std.socks` — SOCKS5-клиент (новый модуль)

```nova
export type SocksError enum
    | ConnectFailed { reason str }
    | AuthFailed
    | AuthRequired            // сервер требует auth, креды не даны
    | UnsupportedMethod
    | HostTooLong             // domain-адрес > 255 байт (протокольный лимит)
    | GeneralFailure { code u8 }   // s709 REP-код сервера, не наш (D30-conventions: код в поле)
    | Protocol { detail str }

export fn socks5_connect(
    proxy_host str, proxy_port int,
    user Option[str], pass Option[str],
    target_host str, target_port int
) Net -> Result[TcpStream, SocksError]
```

Реализация — прямой перевод RFC 1928 (version/methods negotiation → auth
sub-negotiation RFC 1929, если сервер выбрал `0x02` → CONNECT-команда → разбор
bound-адреса ответа) в последовательные `Net.write`/`Net.read` на уже открытом
`TcpStream`. Без крипто — чистое byte-level message framing.

### 3.2 Мост — `examples/net/socks5_http_bridge.nv`

```nova
fn main() Net Time -> () {
    consume listener = TcpListener.bind("127.0.0.1:8899")!!
    loop {
        consume client = listener.accept()!!
        spawn { handle_client(client) }
    }
}

fn handle_client(consume client TcpStream) Net -> () {
    ro head = read_headers(client)              // до \r\n\r\n
    match parse_request_line(head) {
        Connect(host, port) => match socks5_connect(PROXY_HOST, PROXY_PORT, user, pass, host, port) {
            Ok(consume upstream) => {
                client.write_all("HTTP/1.1 200 Connection Established\r\n\r\n")!!
                pipe_bidirectional(client, upstream)
            }
            Err(_) => { ro _ = client.write_all("HTTP/1.1 502 Bad Gateway\r\n\r\n") }
        }
        Plain(host, port, rewritten) => { /* аналогично: connect → forward → pipe */ }
    }
}
```

**Отличия от типового скрипт-прототипа этого класса задач (осознанные архитектурные
решения, не копирование один-в-один):**
- upstream-коннект **до** ответа клиенту, не после — 502 при провале естественный, без
  гонки «уже отправили 200, а сервер недоступен»;
- `pipe_bidirectional` — два `spawn` с `Net.read`/`Net.write` в цикле, не
  callback-ориентированный event-loop;
- ретраи на временную недоступность upstream (провайдеры нередко ротируют IP) —
  через `Time.sleep`, тем же паттерном, что остальные примеры используют для
  detrministic-тестируемости (`with Time = th.fixed_ms(...)` в тестах).

**Сознательно НЕ переносится:** список «шумных» доменов-трекеров, которые
скрипт-прототип не пишет в лог — это специфика конкретного internet-сценария, не
языковая демонстрация.

## 4. Фазы

- **Ф.0** — пины: подтвердить пробой форму error-enum'а (`SocksError`, D30-конвенция) и
  что `Net.read`/`Net.write` дают удобный примитив для фиксированной длины
  (handshake-сообщения SOCKS5 имеют точные байтовые размеры на большинстве шагов).
- **Ф.1** — `std.socks.socks5_client` + юнит-тесты (мок upstream — байтовые
  фикстуры известных handshake-обменов из RFC 1928/1929, реальный SOCKS5-сервер
  для тестов НЕ нужен).
- **Ф.2** — мост (`examples/net/socks5_http_bridge.nv`): accept-loop, парсинг
  первой строки, CONNECT-путь.
- **Ф.3** — plain-HTTP-путь (переписывание absolute-URI → origin-form, срез
  `Proxy-*`-заголовков) — вторичный, можно отложить за Ф.2 отдельным коммитом.
- **Ф.4** — README (EN+RU, по конвенции examples) — что показывает, как
  запустить, честная пометка «нужен реальный SOCKS5-прокси для ручной проверки,
  не входит в CI-гейт».

## 5. Гейты

`std.socks` — таргетные юнит-тесты (мок-байты, без сети) обязательны и входят в
общий гейт `nova test std`. Сам мост (`examples/net/`) — компилируется
`--strict-effects` (конвенция examples); **ручной smoke — вне CI** (нужен реальный
внешний SOCKS5-прокси, недоступен в автоматическом прогоне) — задокументировать
это ограничение явно в README примера, не заявлять «протестировано end-to-end».

## 6. Риски

| Риск | Митигация |
|---|---|
| SOCKS5 handshake — точная байтовая раскладка, легко ошибиться в порядке полей | юнит-тесты на КАЖДЫЙ шаг протокола отдельно, байтовые фикстуры сверены с RFC 1928/1929 текстом, не с памятью |
| `Router`/`polaris` эволюционирует и научится CONNECT — пример устареет архитектурно | не блокирует V1; ревизия при появлении authority-form в polaris — отдельный follow-up |
| Ручной smoke невозможен в CI | честно задокументировано (§5), не выдаётся за протестированное |

## 7. Открытые вопросы владельцу (до запуска волны)

1. **Имя модуля:** `std.socks` (в стандартной библиотеке) или изолированный
   package-пример без претензии на std-API (`examples/net/_socks5.nv`,
   module-private, не `export`)? Рекомендация: **package-пример** — SOCKS5-клиент
   без реальных других потребителей в экосистеме пока не тянет на std-поверхность
   (§3 maximize-nv не про «добавить в std всё, что написано», а про «где уже есть
   потребитель»).
2. **IPv6-адреса в SOCKS5** (address type `0x04`) — в V1 или явный `Err(Unsupported)`
   до первого реального носителя? Рекомендация: явный `Err` — YAGNI, домен/IPv4
   покрывают мотивирующий сценарий.

## Связи

Реестр [221.1](221.1-bug-sweep.md) №360 / `[M-socks5-client-missing]`
(`backlog-followups.md`) · `nova-http/src/method.nv` (`Method.Connect`) ·
`nova-polaris/src/ws_upgrade.nv` (`ServerResponse.upgrade` — прецедент хука, не
переиспользуется напрямую из-за загвоздки §1) · `examples/net/` (категория-дом,
`echo_client.nv`/`echo_server.nv` — прецедент оформления) · RFC 1928 (SOCKS5) ·
RFC 1929 (username/password auth) · RFC 7230 §5.3.3 (authority-form request-target).
