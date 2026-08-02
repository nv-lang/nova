# План 205 — компрессия из nova_rt в пакет nv-lang/nova-compress

**Статус:** ✅ ЗАКРЫТ 2026-07-17. Ф.0-Ф.1 (репа `nv-lang/nova-compress`, тег `v0.1.0`,
паритет с монорепо сверен файл-к-файлу) и Ф.3 (публикация) были сделаны раньше;
Ф.2 (потребители + вычистка монорепо) закрыт этим заходом (ветка `p205-dedup`,
коммит `899347db4`): грепом по `std/`+`examples/`+`spec_tests/`+`nova_tests/`
подтверждено НОЛЬ прочих потребителей `std.encoding.compress` (только два
spec_tests-фикстуры — `d337_brotli_ffi.nv`, удалён вместе с эквивалентом,
он же живой в nova-compress; `xmodule_struct_variant_ctor_*` — общий
регресс-кейс cross-module type-collision, не привязан к compress, оставлен);
удалены `std/src/encoding/compress/**` (модуль+тесты+neg) и
`nova_rt/brotli_shim.{c,h}` + `nova_rt/brotli/**` (7 МБ vendored); в
`test_runner.rs` retired `BrotliConfig`/`detect_brotli`/`c_file_uses_brotli`/
`source_uses_brotli` + все conditional-link сайты (build_command clang/msvc/gcc
+ multi-TU) — generic `[ffi] vendor_src_dirs`/`build_missing_vendor_ffi_libs`
(прецедент nova-tls/mbedTLS) полностью покрывает use-case, nova-compress уже
на него переехал. `nova-test-regression.yml` — `libbrotli-dev` снят из
apt-install (два джоба); `docs/guide/linux-build.md` — brotli-строка снята из
таблицы пакетов. Гейты: `cargo build --release` чист; `nova check std` —
дельта ровно исчезнувший compress (18 pre-existing FAIL, все neg-фикстуры/
известный single-file `prelude/protocols.nv` артефакт, ноль новых); `nova test
std/src/encoding` 8/0/7skip; `nova test src` в nova-compress (та же
модифицированная тулчейн-сборка) — весь функциональный набор (checksum/
deflate/gzip/zlib/brotli d333-d337, реальный RFC 7932 decode через
generic-FFI-собранный libbrotlidec) зелёный; греп `brotli` по
`compiler-codegen/**` = 0 кроме двух historical "retired"-комментариев.
`std/tls`/`libmbedtls-dev` НЕ тронуты (отдельный precedent/scope).

## Мотив

`compiler-codegen/nova_rt/brotli` (7 МБ vendored: заголовки + lib) живёт внутри рантайма
компилятора по историческим причинам: когда его вносили (авто-распаковка
`Content-Encoding: br` для http), механизма «vendored C внутри пакета» не существовало —
nova_rt был единственным местом, откуда toolchain берёт C-заголовки и линкует библиотеки.
План 193 создал прецедент (nova-tls: `native/`-шим в репе пакета).

Целевой принцип: **nova_rt = только рантайм языка** (GC, libuv/событийный цикл, fibers,
effects). Кодеки — пакеты.

## Целевая картина

- Публичная репа `nv-lang/nova-compress` (пакет `compress`): `.nv`-фасад
  (нынешний `std/src/encoding/compress/**`: brotli.nv, deflate.nv, ffi.nv, error.nv) +
  `native/` с vendored brotli (и deflate-бэкендом, если он тоже в nova_rt — см. Ф.0).
- Слои: `nova-http → nova-compress` (авто-распаковка) и `nova-http → nova-tls`;
  compress НЕ в nova-http — gzip/deflate нужны fs/архивам/другим протоколам.
- Зависимости — path-dep как dev-форма до Plan 204, затем git+semver.

## Фазы

### Ф.0 — инвентарь vendored-C в nova_rt
Перечислить всё стороннее в `compiler-codegen/nova_rt/` (brotli — точно; проверить
zlib/miniz/deflate-бэкенд и прочее), для каждого: потребитель, механизм линковки
(test_runner.rs/toolchain), план выселения или обоснование «остаётся» (GC/libuv/fibers —
остаются, это язык). Таблица в этот файл ДО правок.

### Ф.1 — репа nova-compress
По эталону nova-tls: `nova.toml` (`[lib] src="src"`), `src/*.nv` (module `compress`,
переезд из `std/src/encoding/compress/**` с тестами), `native/` (vendored brotli + шим;
конвенция C-символов: публичные `compress_*`, static `_compress_*` — spec 07-modules).

### Ф.2 — потребители + вычистка
- nova-http: `[dependencies] compress` (авто-распаковка).
- Прочие потребители compress в std/examples — инвентарь грепом, переключить.
- Удалить `std/src/encoding/compress/**` и `nova_rt/brotli/**`; вычистить упоминания
  из toolchain (test_runner.rs include/link-пути).

### Ф.3 — публикация
`gh repo create nv-lang/nova-compress --public` + push (после зелёных гейтов Ф.2);
лицензии: MIT+Apache для фасада, LICENSE брotli сохранить в native/ (MIT — проверить).

## Гейты
conformance полный; `nova test src` в nova-compress (тесты переехали живыми) и nova-http
(авто-распаковка жива); `nova check std` — дельта = ровно исчезнувший compress; сборка
флагмана; греп `brotli` по compiler-codegen/** = 0 (кроме changelog/доков).

## Границы
Язык не меняется (D-амендмент не нужен; конвенция шим-символов уже в спеке).
GC/libuv/fibers/effects остаются в nova_rt — это сам язык.
