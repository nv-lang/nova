# nova_tls_shim — C-ABI шим поверх rustls (Plan 116, std/tls)

Отдельный Rust staticlib-крейт: единственное место, где Nova зависит от
rustls. **НЕ входит в dep-tree nova-codegen** (тот остаётся clap+anyhow) —
артефакт линкуется в user-бинарь условно, по факту использования `tls_*`
символов (механизм brotli/D337; интеграция в test_runner.rs — Ф.2 плана 116).

## Сборка

```sh
cd compiler-codegen/tls_shim
cargo build --release
# артефакт: target/release/nova_tls_shim.lib (Windows) / libnova_tls_shim.a (Unix)
```

Кэш для линковки Nova-программ: `<repo>/target/tls-cache/` (Ф.2, зеркало
`target/brotli-cache`). Прекомпилят в репо не трекается (размер; в отличие от
libbrotlidec.lib — см. план 116 Ф.2.1).

Rust staticlib тянет системные либы; их точный список — `RUSTFLAGS="--print
native-static-libs" cargo build --release` (фиксируется в Ф.2.3).

## Провенанс / зависимости

| Крейт | Версия | Зачем |
|---|---|---|
| rustls | 0.23.x (пин — Cargo.lock) | TLS 1.2/1.3 state machine (sans-I/O) |
| rustls (feature `ring`) | — | крипто-провайдер: НЕ aws-lc-rs (тому нужны cmake+nasm); ring собирается имеющимся cc/clang/MSVC |
| webpki-roots | 0.26.x | Mozilla CA bundle (SystemRoots), вкомпилирован |
| rustls-pemfile | 2.x | parse PEM (certs, keys) |
| rustls-pki-types | 1.x | типы DER/ServerName |

Cargo.lock закоммичен = точный пин всего дерева (риск R-9 supply-chain).
Обновление rustls — ТОЛЬКО отдельным followup `[M-116-rustls-upgrade]`
с прогоном T-серий плана 116.

## Контракт границы

Полная таблица символов и кодов ошибок — план 116
(docs/plans/116-std-tls-effect.md, §«FFI-граница»); Nova-сторона —
`std/tls/ffi.nv` (Ф.1). Инварианты:

- символы `tls_*` (compiler-conventions §5а), хендлы — непрозрачные указатели
  (`CTlsHandle(*())`/`CTlsCfgHandle(*())`, module-conventions §4а);
- `nova_int` = `isize`; буферы = (ptr, len), владение через границу не ходит;
- шим — чистый компьют (sans-I/O): сокеты паркуются на Nova-стороне через
  эффект `Net`, GC-буферы живут на стеке фибры (шим их не удерживает);
- panic = abort (паника через FFI = UB);
- строковые выходы (`tls_alpn`/`tls_cipher_suite`/`tls_last_error`/
  `tls_peer_cert_der`): возврат = ПОЛНАЯ длина, копируется min(cap, len).

## Тесты

`cargo test` — санити C-ABI поверхности (ClientHello строится, PEM/SNI/
конфиг-ошибки классифицируются). Полные handshake-тесты — на Nova-стороне
(std/tls/*_test.nv, self-signed fixtures) начиная с Ф.3.
