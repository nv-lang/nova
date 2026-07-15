# TLS-диамант [M-187-weather-live-tls-diamond-blocked] — прогресс

Ветка: `fix-tls-diamond` (worktree `nova-tlsdiamond`). Модель: sonnet.
База: main = be03db5bc.

## Диагноз

`examples/nova.toml` зависел от `tls` (path) И `http` (path); `nova-http/nova.toml`
зависит от `tls` git+version → ДРУГАЯ физическая копия. Резолвер трактовал
path-`tls` и git-`tls` (одно имя) как РАЗНЫЕ узлы → `nova.lock` содержал `tls`
дважды → два физических `tls` в одном CU → `[E_METHOD_REDEFINITION]` на
`TlsStream.connect`. `[replace]` дофикса №2 (D420) был узкий Go-scope: применялся
только к ребру, чей владелец-манифест = корень; транзитивный `tls` внутри `http`
не покрывался.

## Направление (выбрано): №1 — канонический `[replace]` = Cargo `[patch]`

Корневой `[replace]` перекрывает ЛЮБОЕ вхождение same-named пакета graph-wide
(прямое И транзитивное). + `examples/nova.toml`'s `tls` переведён на ту же
git+version форму, что у `http` (unify по git-URL) → lock фиксирует `tls` 1×.

## Что правил

- `compiler-codegen/src/imports.rs::lookup_dependency` — консультирует
  `[replace]` КОРНЕВОГО манифеста для любого `dep_name`, независимо от владельца
  ребра; `Path`-override относительно директории корневого манифеста. Инвариант
  «`[replace]` зависимости инертен» (`W_REPLACE_IN_DEPENDENCY`) СОХРАНЁН.
  + 2 unit-теста: `nested_dependency_replace_is_ignored_root_scope_only` (не
  регрессировал), `root_replace_overrides_transitive_same_named_dep` (новый).
- `examples/nova.toml` — `tls` → `{ git = "...", version = "0.1" }`.
- `examples/nova.local.toml` (gitignored) — `[replace] tls = { path = "../../nova-tls" }`.
- `.gitignore` — `nova.local.toml`.
- `spec/decisions/09-tooling.md` — D420 амендмент (дофикс №3).
- `nova-cli/src/main.rs` — `nova build` теперь мёржит `[ffi]` зависимостей
  (dependency native shims: nova-tls's `tls_c_shim.c`), симметрично
  `test_runner.rs`. Пре-существующий gap (на unmodified main `nova build
  echo_server` тоже link-FAIL'ил undefined `tls_*`).
- `examples/flagship/aggregator/src/app/live.nv` — маркер `tls diamond
  dependency` СНЯТ; `live_fetch_weather` делает реальный DNS+TCP + строит
  cross-package `ClientConfig` (проверка что диамант снят на уровне примера),
  handshake отложен (см. ниже).

## Lock ДО/ПОСЛЕ

- ДО: `grep -c 'name = "tls"' examples/nova.lock` = **2** (path + git).
- ПОСЛЕ: = **1** (git, version 0.1.0, commit 510acc25).

## Smoke weather-live (дословно)

```
curl "http://127.0.0.1:8187/api/run?legend=weather&mode=live"
{"fanout":4,"done":0,"failed":4,"cancelled":0,"wall_ms":101,...,"handlers":{"net":"real","time":"real"},...
 "status":{"state":"failed","error":"transport error: weather live TLS handshake deferred:
 [M-187-tls-cross-pkg-consume-cleanup] cross-package consume-cleanup codegen gap
 (dependency diamond RESOLVED)"},...}
```

Критерий выполнен: НЕТ строки `tls diamond dependency`; `handlers.net="real"`;
запрос завершается. Weather-лейны честно падают на ОТДЕЛЬНОМ codegen-баге (не
диаманте). Процесс убит, порт 8187 свободен.

## ОСТАВШИЙСЯ БЛОКЕР (НЕ диамант, НЕ моя территория — emit_c.rs, параллельная работа)

`[M-187-tls-cross-pkg-consume-cleanup]`: `TlsStream.connect(tcp, cfg)` из
downstream-пакета не линкуется — `Nova_TcpStream_consume_cleanup` (std.net)
referenced-but-undefined. Изолировано минимальным repro:
- pure std.net `consume st = s` at scope-exit → **линкуется** ✓
- nova-tls `TlsStream.connect(tcp,cfg)` (external-пакет консьюмит std.net
  TcpStream) → **undefined symbol** ✗
Причина: определение `Nova_<T>_consume_cleanup` эмитится только когда consume-сайт
в коде КОРНЕВОГО пакета; consume ВНУТРИ внешнего пакета не регистрирует тип для
эмиссии определения. Это emit_c.rs (consume-cleanup / cross-package method
emission). Затрагивает и `examples/tls/echo_server.nv`.

## Non-regression

- `nova check` echo_server.nv / echo_client.nv / aggregator — все PASS (0 FAIL).
- echo_client.nv имеет ОТДЕЛЬНЫЙ пре-существующий codegen-баг (`Nova_TlsVersion_p`
  unknown type), воспроизводится идентично на unmodified main — НЕ регрессия
  этой волны.

## Коммиты

(см. git log ветки)
