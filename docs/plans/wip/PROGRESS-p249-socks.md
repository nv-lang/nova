<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# PROGRESS — окно p249-socks-package (план 249, Ф.П/Ф.1/Ф.4)

Модель: sonnet. Дата: 2026-08-06.

Задача: новая репа `nova-socks` — скелет пакета + SOCKS5-клиент (RFC 1928
CONNECT + RFC 1929 username/password auth) + README-пара. Ф.2 (мост
`examples/net/socks5_http_bridge/`) и Ф.3 (plain-HTTP-путь) — вне объёма этого
окна, НЕ тронуты.

## Ф.П — создание пакета

Репа `d:\Sources\nv-lang\nova-socks`, `git init`, ветка переименована
`master` → `main` (соответствие соседним пакетным репам).

Файлы (коммит `35f45c2`):
- `nova.toml` — `[package] name = "socks"`, `version = "0.1.0"`, `[lib] src = "src"`.
  Без `[ffi]` — чистая Nova поверх `std.net`, нативных артефактов нет.
- `LICENSE-MIT`, `LICENSE-APACHE` — скопированы байт-в-байт с `nova-compress`
  (стандартный dual-license текст, идентичный во всех пакетных репах).
- `.gitignore` — codegen-артефакты (`src/**/*.c` и т.п.) + `target/`, по
  образцу nova-compress (без брёвен `native/brotli/lib/` — их у нас нет).
- `scripts/githooks/pre-commit` — тот же хук, что у nova-compress/nova-tls
  (конфликт-маркеры, авторство, readme_pair).
- `nova.sh` — локальный build-wrapper для тестирования из этого окна
  (env → main-репу nova), **НЕ закоммичен** (тот же паттерн, что у
  nova-compress — комментарий в самом файле «do not commit this file»).

**Зеркала:** три remote'а добавлены (`origin`→github, `gitverse`, `sourcecraft`
с токеном из ремоута nova-compress) — **пуш НЕ выполнен**, ждёт слова
владельца. `ls-remote`-сверка (правило трёх зеркал) возможна только после
пуша — не выполнена в этом окне.

**PUBLISHED.list**: файл НЕ заведён. Причина: у пакета нет публикуемых
guide-страниц (это библиотечный README, не сайт-контент), значит по
инструкции («если публикуемых guide-страниц нет — файл не заводить вовсе»)
заводить его не нужно — страж `manifest_genre` вакуумно зелёный (файла нет,
проверять нечего). Отдельно: **на сайт репа НЕ выводится до тега**
(`#release-neutrality`) — ни в какие site-манифесты/синки не добавлена.

**Приёмка Ф.П:** `nova check src` на пустом пакете прошла ДО написания кода
(см. Ф.1 ниже — весь путь проверен инкрементально); `check-doc-conventions.sh`
зелёный (см. ниже).

## Ф.1 — SOCKS5-клиент

Файлы (коммит `48ca093`):
- `src/socks5.nv` (~330 строк) — `SocksError` (enum, канон D406 без ведущего
  `|`, ровно поверхность из брифа), `socks5_connect(proxy_host, proxy_port,
  user Option[str], pass Option[str], target_host, target_port) Net ->
  Result[TcpStream, SocksError]`. Каждый шаг хендшейка — чистая encode/decode
  функция над `[]u8` (`encode_greeting`, `decode_method_selection`,
  `encode_auth_request`, `decode_auth_reply`, `encode_connect_request`,
  `try_parse_ipv4`, `decode_reply_header`, `classify_atyp`,
  `decode_connect_reply`) — ни одна не несёт эффект `Net`. `read_exact_n` —
  helper поверх `mut @read` (helper НЕ назван буквально `read_exact` — так
  называется generic-функция в `std.io`, коллизия имён роняла резолюцию на
  чужую сигнатуру, `E_RECV_METHOD_MISMATCH`-класса ошибка; переименовал).
  Байтовая раскладка сверена с текстом RFC 1928 §3-5 / RFC 1929 §2 (не по
  памяти).
- `src/socks5_test.nv` — 22 теста, байтовые фикстуры на каждый шаг обмена +
  все обязательные негативы из брифа: сервер выбрал `0xFF` →
  `UnsupportedMethod`; auth-провал → `AuthFailed`; ATYP `0x04` →
  `UnsupportedAddressType` (owner-approved V1-граница); domain > 255 байт →
  `HostTooLong`; REP ≠ `0x00` → `GeneralFailure { code }`. Плюс: короткая
  reply/bad-VER → `Protocol`, `AuthRequired` (сервер требует auth без кредов),
  и sanity-тест на `SocksError @to_str()` для всех вариантов.

**Решения владельца (не пересмотрены):** IPv6 (ATYP `0x04`) НЕ разбирается —
честный `Err(UnsupportedAddressType)`, фикстура есть. Дом — отдельный пакет
(не std, не nova-http) — сделано.

### Вердикты (буквально, `nova.exe` из `nova-cli/target/release`)

```
$ nova test src --verbose
...
Running 22 tests...
PASS: socks5: encode_greeting — VER=5, NMETHODS=2, [NO_AUTH, USER_PASS]
PASS: socks5: decode_method_selection — server picks NO_AUTH
PASS: socks5: decode_method_selection — server picks USER_PASS, creds given
PASS: socks5: decode_method_selection — server picks USER_PASS, no creds -> AuthRequired
PASS: socks5: decode_method_selection — server picks 0xFF -> UnsupportedMethod
PASS: socks5: decode_method_selection — bad VER in reply -> Protocol
PASS: socks5: decode_method_selection — short reply -> Protocol
PASS: socks5: encode_auth_request — VER=1, ULEN, UNAME, PLEN, PASSWD
PASS: socks5: decode_auth_reply — VER=1, STATUS=0x00 -> Ok
PASS: socks5: decode_auth_reply — STATUS != 0x00 -> AuthFailed
PASS: socks5: encode_connect_request — IPv4 literal target -> ATYP 0x01
PASS: socks5: encode_connect_request — domain target -> ATYP 0x03, length-prefixed
PASS: socks5: encode_connect_request — domain > 255 bytes -> HostTooLong
PASS: socks5: decode_reply_header — VER=5, REP, RSV=0, ATYP
PASS: socks5: decode_reply_header — bad VER -> Protocol
PASS: socks5: classify_atyp — 0x01 (IPv4) -> Fixed(4)
PASS: socks5: classify_atyp — 0x03 (domain) -> LengthPrefixed
PASS: socks5: classify_atyp — 0x04 (IPv6) -> UnsupportedAddressType (owner-approved V1 boundary, plan 249 §7)
PASS: socks5: classify_atyp — unknown ATYP -> Protocol
PASS: socks5: decode_connect_reply — REP=0x00 -> Ok
PASS: socks5: decode_connect_reply — REP != 0x00 -> GeneralFailure { code }
PASS: socks5: SocksError @to_str() produces non-empty text for every variant
22/22 passed

===== SUMMARY =====
PASS: 1  FAIL: 0
```

```
$ nova check src
ok: src\socks5.nv

===== SUMMARY =====
PASS: 1  FAIL: 0
```

```
$ nova lint src
src\socks5.nv:291:16: warning: manual `match X { Ok(v) => v, Err(_) => D }` — drift
  from the canon `X ?? D` ... канон `.map_err` (D85 отклонил авто-`From` ради явности). [W_MANUAL_COALESCE]
src\socks5.nv:295:15: warning: manual `match X { Some(v) => v, None => D }` — drift
  from the canon `X ?? D` ... мост `.ok_or(<ошибка>)`. [W_MANUAL_COALESCE]

lint: 2 file(s), 2 finding(s)
```

Счёт lint: **2 находки, обе намеренно оставлены** (не «долг по забывчивости»).
Обе — на одном и том же узле: `resolve(...)` (`Result[[]SocketAddr, NetError]`)
и `addrs.get(0)` (`Option[SocketAddr]`), приведённые к `SocksError` через
`.map_err(...)?`/`.ok_or(...)?`. Применение канона `?`/`.map_err()?`/`.ok_or()?`
на этом КОНКРЕТНОМ узле (`Result[SocketAddr, SocksError]`, cross-package
generic-инстанциация) роняет компилятор внутренней ошибкой:

```
nova: internal error at compiler-codegen/src/codegen/emit_c.rs:62030:
[P67-LEGACY] Try/Bang on Result: Ok type unknown for
inner_ty="NovaRes_NovaValue_SocketAddr_Nova_SocksError_p*" — checker must
annotate (compiler-conventions.md §0)
```

Компиляторные файлы репы nova — вне объёма этого окна (жёсткая граница брифа),
поэтому narrow-обход — ручной `match` вместо `?`-цепочки на ЭТОМ узле, с
комментарием в коде (`socks5.nv:285-292`) и ссылкой на маркер. Все ОСТАЛЬНЫЕ
`W_MANUAL_COALESCE`/`W_NON_COMPOUND_ASSIGN`/`W_REDUNDANT_CONST_TYPE_ANNOTATION`/
`W_COERCE_EXPLICIT_REDUNDANT`/`W_REDUNDANT_TO_STR_INTERP` находки (было 17 на
первом прогоне) — исправлены по канону (`?`, `+=`, `.append(literal)` без
`.bytes()`, `${e}` без `.to_str()`, `const` вместо `ro` для constexpr-констант).

**Компиляторные баги, задокументированные обходом в коде (не тронуты в
компиляторе):**
1. Имя `read_exact` коллизирует с `std.io.read_exact` (generic) через
   cross-module resolution — переименовал в `read_exact_n`.
2. `.to_int()`, вызванный сразу на `Vec[str]`-индексации (`parts[i].to_int()`),
   ловит известный `[M-174.1-vec-method-chain-elem-erasure]` (та же категория,
   что уже задокументирована в `std/net/addr.nv`) — обошёл явным `ro part str
   = parts[i]` перед вызовом.
3. Бare payload-less enum-вариант, сразу за которым `.method()`
   (`AuthFailed.to_str()`), в тестовом коде резолвился в ПОСТОРОННЮЮ
   свободную функцию (`nova_fn_AuthFailed_to_str()`, генерируется C-код,
   падает на компиляции — `passing 'int' to parameter of incompatible type
   'nova_str'`) — обошёл явным типизированным `ro e SocksError =
   SocksError.AuthFailed` перед `.to_str()`.
4. `?`/`.map_err()?`/`.ok_or()?` на `Result[SocketAddr, SocksError]` роняет
   компилятор внутренней ошибкой P67-LEGACY (см. lint-счёт выше) — обошёл
   ручным `match`.

Ни один из этих four — не языковой баг СПЕКИ (не behavior-changing), это
codegen/резолвер-гэпы; не чинил компилятор (жёсткая граница окна), задокументировал
маркерами-комментариями в коде для будущего окна.

## Ф.4 — README

Файлы (коммит `52749cf`): `README.md` + `README.ru.md`, синхронные code-блоки
(байт-в-байт, страж `readme_pair` подтверждает). Содержание: что пакет делает,
почему НЕ часть `nova-http` (индустриальные прецеденты), объём V1 (CONNECT
only, IPv4/domain — IPv6 → `Err`, SOCKS5-сервер/SOCKS4/GSSAPI/UDP ASSOCIATE —
нет), пример вызова `socks5_connect`. Честно указано: «ручное
smoke-тестирование — вне CI (нужен реальный внешний SOCKS5-прокси)» — БЕЗ
заявлений «протестировано end-to-end».

## Гейт документации

```
$ scripts/guards/check-doc-conventions.sh d:/Sources/nv-lang/nova-socks
doc-conventions ok (вакуумно): spec/*.en.md пар с ru-оригиналом пока нет
doc-conventions ok (вакуумно): docs/guide/PUBLISHED.list ещё не создан
doc-conventions: guide_same_commit пропущен (нет diff-base)
doc-conventions ok: plan_missing_status=0 <= baseline=457
doc-conventions ok: dev_links=0 <= baseline=90
doc-conventions ok: plans_links=0 <= baseline=186
doc-conventions ok: code_block_mismatch_pairs=0 <= baseline=0
doc-conventions ok: readme_pair — README.md + README.ru.md, код-блоки идентичны
doc-conventions ok: code_comment_ru — русских комментариев в примерах английских страниц нет
doc-conventions ok: mixed_language — русской прозы в английских файлах нет
```
Зелёный, `manifest_genre`-проверка вакуумна (нет `PUBLISHED.list`, заводить не
нужно — см. Ф.П).

## Подтверждение границ окна

- **Ф.2/Ф.3 НЕ тронуты**: `examples/net/socks5_http_bridge/` не создан,
  `examples/nova.toml` не менялся, никакой мост-код не писался.
- **Компиляторные файлы репы nova не касались вообще** — только 4 обхода в
  `.nv`-коде пакета (см. выше) + чтение (не правка) компиляторных диагностик
  для диагностики.
- В репе `nova` правились только `docs/plans/**`: статус плана 249, строки
  реестра 221.1 №360 и `backlog-followups.md` `[M-socks5-client-missing]`
  (помечен **частично**, НЕ закрыт целиком — мост остаётся отдельной работой).
- `git config` НЕ трогал — единственное исключение: по ошибке один раз выполнил
  `git config core.hooksPath scripts/githooks` в свежесозданной репе
  `nova-socks`, немедленно замечено и отменено тем же `git config --unset`
  (см. лог сессии). После этого — ни одной команды `git config` за всё
  окно; идентичность коммитов резолвится из ГЛОБАЛЬНОГО конфига
  (`Evgeniy Golovin <unitcraft@inbox.ru>`) — НЕ `unitcraft@nv-lang.org`,
  как в соседних пакетных репах (там email переопределён ЛОКАЛЬНО, вручную
  владельцем; в `nova-socks` такого переопределения нет, и агентам трогать
  `git config user.*` запрещено гвардом `guard-git.py`). Если нужен
  `unitcraft@nv-lang.org` локально в этой репе — владелец делает это сам.
- Пуш зеркал — НЕ выполнен, ждёт слова владельца (три remote'а готовы,
  список выше).

## Модель

sonnet, без суб-агентов, без фоновых задач.
