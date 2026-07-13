# План 205 — компрессия из nova_rt в пакет nv-lang/nova-compress

**Статус:** 📋 СОГЛАСОВАН 2026-07-13 (владелец: «ОК»). **После:** гейты 203 (не гнать две
миграции http-стека одновременно).

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
