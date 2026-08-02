<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 179 — std/encoding/compress (DEFLATE / zlib / gzip / brotli)

> **⚠ Код вынесен во внешний репозиторий `nv-lang/nova-compress`** ([Plan 205](205-compress-out-of-nova-rt.md),
> 🔨 Ф.2-хвост ОТКРЫТ — nova-compress опубликован, но nova-http всё ещё импортирует
> `std.encoding.compress`, дубликаты кода живут в main; см. backlog
> `[M-205-f2-consumer-switch]`). План ниже остаётся как исторический дизайн-документ.
>
> **Top-level** под-план. Создан 2026-06-26. **Статус:** 🟢 Ф.1 + Ф.3 + Ф.2(decode) LANDED — inflate/gzip/zlib **decode** (pure-Nova) + deflate/gzip/zlib **encode** (pure-Nova, levels + streaming) + **brotli decode (C-FFI, 2026-07-06)** готовы. **Ф.2 итог (2026-07-06):** vendored google/brotli v1.2.0 decoder (headers+lib, стиль libuv, БЕЗ исходников); `brotli_decode(data, max_output)` через `libbrotlidec` C-FFI (RFC 7932-вектора pass, bomb-cap инкрементально поверх FFI, типизированные ошибки); **условная линковка** «модуль→библиотека» (brotli-lib в команду компоновки ТОЛЬКО когда CU вызывает декодер — libuv-mandatory НЕ тронут); http `Content-Encoding: br` → `brotli_decode` + `Accept-Encoding: …, br` (`[M-178-autodecompress-br]` закрыт); conformance **54/0** (d337 внутри positive-CU); zero-regression (baseline-delta 0). **Остаток Ф.2:** streaming `BrotliReader` (`consume`) — deferred `[M-179-brotli-reader-streaming]` (C-примитивы шима готовы; owner-deliverable = one-shot `brotli_decode`; http использует one-shot). (Ред. 2 — 2026-07-03: аудит ground-truth/7-языков/siblings; stale-номера сняты, D-карта актуализирована, flush/level/FFI-фиксы.) Маркер **[M-179-std-compress]**.
>
> **✅ Spec-debt закрыт (2026-07-04):** формальные D-блоки **D333–D337 внесены в `spec/decisions/04-effects.md`** (класс-соседи D322/D323 io/fs + D325 fallible; аспирационный `05-stdlib.md` НЕ создан — коллизия с `05-memory.md`, block приземлён к реальным соседям; код/тест/plan-ссылки `05-stdlib.md`→`04-effects.md` синхронизированы). README-индекс decisions обновлён; коллизий D333–D337 нет (grep=0). Pre-existing долг Ф.1 (см. simplifications) снят.
>
> **Ф.3 итог (2026-07-04):** `std/encoding/compress/deflate.nv` (NEW, pure-Nova) + encode в `zlib.nv`/`gzip.nv`. Реализовано: `CompressLevel` value-record (fastest=1/default=6/best=sentinel→9/none=0/`new(0..11)`; deflate-range 0..9, `new(10/11)`→`InvalidData`); RFC 1951 **deflate-encoder** — LZ77 hash-chain matcher (greedy 1..6 / lazy 7..9, 32 KiB окно, min-match 3, len 3..258), stored-блоки (level 0), fixed-Huffman (1..6), **dynamic-Huffman** (7..9: частотный length-limited Huffman через miniz-порт `calc_min_redundancy`+`enforce_max_code_size`, RLE code-length-header 16/17/18, fixed-fallback на вырожденном CL-коде); **≥3 различимых режима** (Q7: stored/fixed/dynamic — `best < fastest < stored` на скошенном корпусе). `zlib_encode` (CMF/FLG+FCHECK+Adler-32-BE) + `gzip_encode` (10-байт header+CRC-32-LE+ISIZE-LE). Streaming **`Deflater`/`GzipWriter`/`ZlibWriter`** (value, НЕ consume): `feed` (auto-flush ≥64 KiB, persistent bit-buffer) / **`@flush` SYNC-FLUSH** (`00 00 FF FF` → decodable-префикс, D335) / `finish` (final-блок+padding+trailer). **Encode БЕЗ bomb-cap** (§3.5: выход<входа, decompression-bomb невозможен на компрессии — сигнатура `fn(data, level)` строго по плану, БЕЗ `max_output`). **ГЛАВНЫЙ acceptance — round-trip `inflate(deflate(x,lvl))==x` — PASS** на всех уровнях (пусто/1 байт/RLE/random/скошенный). **🔴 EXTERNAL ORACLE (anti-circular §8.3):** Nova-encode декодирован **НЕЗАВИСИМЫМ** python `gzip.decompress`/`zlib.decompress`/`zlib.decompress(-15)` И системным **`gzip -d`** — ВСЕ совпали с оригиналом (RFC 1950/1951/1952 framing подтверждён). Тесты: in-module deflate.nv (round-trip all levels, level-distinguish, stored, streaming feed/finish, flush-prefix-decodable, level-range-neg) + `nova_tests/compress/encode_roundtrip.nv` + conformance `d333` (encode round-trip + per-codec level-validation). **conformance 38/0**, zero-regression (0 .rs-правок → binary byte-identical; compress consumed только новыми тестами).
> **⚠ Cross-module фикс (Ф.3, зафиксировать):** `CompressLevel value { priv n u8 }` — чтение `priv`-поля свободной функцией (`resolve_deflate_level`/`Deflater.new`) ловит `E_FIELD_MODULE_PRIVATE` при cross-module re-check (Plan 178-потребитель!): Ф.1 не имела ни одного `priv`-поля, поэтому не всплывало. Фикс: чтение через own-type-method `CompressLevel @raw()` (own-method доступ к `priv` разрешён в любом module-контексте, D220/D281). Это НЕ упрощение — корректный идиом (совет самого компилятора). Consumer-usability проверена реальной cross-module программой (external oracle).
>
> **Ф.1 итог (2026-07-04):** `std/encoding/compress/{error,checksum,inflate,zlib,gzip}.nv` (pure-Nova, `module encoding.compress`). Реализовано: `CompressError`+OPEN `ErrorKind` (D325); `crc32*` (промоут) + `adler32*` (NEW, D336); резюмируемый `Inflater` (stored/fixed/dynamic Huffman, LZ77 32 KiB окно с overlap, single-distance-code Q13, bomb-cap на выход+вход-прогресс D334, streaming feed/read/finish D335) + one-shot `inflate`; `zlib_decode`+`ZlibReader` (Adler-32, FDICT-reject); `gzip_decode`+`GzipReader` (CRC-32+ISIZE-mod-2^32, multi-member, header-flag/field-cap). Проверено на РЕАЛЬНЫХ python-zlib/gzip векторах (round-trip fixed/stored/dynamic/RLE/multi-member; bomb→Bomb; checksum-mismatch→Checksum; truncated→UnexpectedEof; trailing→TrailingData; streaming-по-1-байту==one-shot). Тесты: in-module + `nova_tests/compress/` (pos+neg EXPECT_COMPILE_ERROR) + conformance `d333–d336` — **conformance 38/38 зелёный**, zero-regression (purely additive). **UNBLOCKS Plan 178 Q12 (gzip/deflate).**
> **⚠ Codegen-обходы (Ф.1, зафиксировать в followup/спеке):** (1) `Inflater` — plain record (не `value`): `value`-тип, встроенный по значению в reader'ы, ловит struct-emission-ordering-баг (embedded def раньше embedded-type). (2) `Inflater.read` → `Result[[]u8,_]` + `is_done()` вместо `Result[Option[[]u8],_]`: codegen генерит `Option[Option[Vec[u8]]]`-eq раньше inner-eq (C conflicting-types). Семантика D335 сохранена (EOF vs need-more через `is_done()`). Оба — компиляторные ограничения, не упрощения логики.
> **Запуск:** «выполни план 179».
> **Очередность (граф 173-181 — [README планов §Очередность](README.md), 2026-07-03):** **Волна 1 трек E —
> стартует сразу**: Ф.1 (inflate/gzip/zlib pure-Nova) — алгоритмика на str/Vec, ни одного входящего гейта.
> **Исходящие: Ф.1 = 🔴 гейт-опенер для 178 Ф.2** (auto-decompress). brotli (Ф.2, C-FFI) — координация
> 174.6 (FFI-тип-лист, §3.6 — extern без `[]u8`). Ред. 2 пройдена 2026-07-03; при запуске re-verify D333–D337 (grep).
> **Эталон (peers):** Go `compress/{flate,gzip,zlib}` (чистый Go) + 3rd-party brotli · Rust `flate2` (miniz_oxide pure / zlib) + `brotli` crate · Node `zlib` (built-in C, incl. brotli/zstd) · Java `Deflater`/`Inflater`+`GZIPStream` · Python `zlib`/`gzip`/`brotli` · **Zig `std.compress`** (чистый Zig flate/gzip/zlib/zstd, **нет brotli**) · Swift `Compression` (zlib/lzfse/lz4/lzma). Nova берёт **pure-implementation-дух Go/Zig** (nv-sourcing) и **чинит универсальную дыру**: bomb-cap by-construction + streaming-cap-on-the-fly + typed `CompressError`.
> **D-блоки (NEW):** **D333** (codec-контракт: PURE-codec без effect + byte-first + `CompressError`) · **D334** (bomb-cap DoS-инвариант — обязательный `max_output`) · **D335** (streaming incremental coder `feed`/`read`/`finish` + Plan 178 `BodyReader`-мост) · **D336** (checksum-контракт CRC-32/Adler-32/ISIZE-verify) · **D337** (brotli C-FFI-контракт + bomb-over-FFI). ⚠ **D-карта (Ред.2 2026-07-03):** committed high-water 3xx-серии = **D355** (в спеке уже D326/D327/D328/D354/D355); D316–D324 — резервы 175/175.1/176; у Plan 178 коллизия по D327/D328 → **178 renumber в D357–D362** (НЕ заходит в D333–D339); **D333–D337 свободны** (grep=0) + D338–D339 буфер → берём **D333+**, reservation-нота в индексе (Ф.0).
> **HARD-GATES/DEPS:** **этот план Ф.1 = 🔴 HARD-GATE-OPENER для Plan 178 Q12** (auto-decompress gzip/deflate **сейчас**; br — после Ф.2). DEP: Plan 177 (D325 Result-everywhere), Plan 176 (byte-first / must-consume / `io.Write` для followup copy_to), Plan 178 (consumer; schedule-coord). Brotli Ф.2 — gated на vendor `libbrotlidec` (build-infra verify Ф.0) + координация **174.6 M1** (C-ABI тип-лист: extern-сигнатуры без `[]u8` — §3.6).
> **Координация:** Plan 178 (consumer — reconcile Q12/§3.5/§9/§11, **+ amend «C-zlib FFI»→pure-Nova**, см. §6), Plan 177 (D325), Plan 176 (byte-first), `_experimental/checksums/crc32.nv` (промоут/реюз owner-sign-off).
> **Сквозной критерий (§8.0):** **без упрощений, как для прода** — bomb-cap НЕ опционально (§8.0-security-critical, критпуть Plan 178); checksum-verify by-default; streaming-память-bounded доказывается; brotli/encode честно **gated/scoped** с rationale.

---

## 1. Зачем

В Nova **нет `std/encoding/compress`** — ни inflate, ни deflate, ни gzip/zlib/brotli. Это прямо **держит Plan 178 Ф.2** (HTTP-клиент): авто-decompress ответа по `Content-Encoding: gzip|deflate|br` — **🔴 HARD-GATE на этот под-план** (178 Q12: auto-decompress gated на **179 Ф.1** — gzip/deflate сейчас, br после Ф.2; bomb→`BodyTooLarge` — маппинг §3.1). Без compress HTTP-клиент Nova получает на проводе сжатое тело и **не может его прочитать** — а `gzip`/`br` шлёт по умолчанию практически каждый современный сервер (CDN/API). То есть **без этого под-плана REST-клиент Nova нефункционален на реальном вебе**, не «менее эргономичен», а буквально получает байты, которые не декодирует.

**Сверх HTTP** — DEFLATE/gzip/zlib/brotli нужны повсеместно и самостоятельно: распаковка `.gz`/`.zip`-членов (DEFLATE — компрессия #1 в `zip`, PNG `IDAT` = zlib, HTTP, `tar.gz`, gRPC, npm/cargo-tarball'ы registry-клиента Plan 03.x), сжатие request-body (`Content-Encoding` на upload) и server-side response-encode (middleware `mw_compress`). Любой язык несёт DEFLATE-семейство в std/первой-партии: **Go** `compress/{flate,gzip,zlib}` (чистый Go), **Rust** `flate2` (miniz_oxide pure-Rust / zlib), **Node** `zlib` (built-in C, incl. brotli), **Java** `Deflater`/`Inflater`+`GZIPStream`, **Python** `zlib`/`gzip`, **Zig** `std.compress` (чистый Zig flate/gzip/zlib), **Swift** `Compression`. Nova без него — на уровне «сырые байты, разбирайся сам».

**Что разблокирует:** (1) **Plan 178 Ф.2 auto-decompress** — `Inflater`/`GzipReader`/`BrotliReader` стримят чанки `BodyReader` chunk-by-chunk (НЕ one-shot — интегрируется с `BodyReader.@next_chunk` [178 §3.5 `BodyReader.@next_chunk`]), с **bomb-cap** (`max_decompressed`, дефолт 100 MiB [178 §3.client `@max_decompressed`]) → `CompressError{Bomb}` на превышении; (2) **server-side encode** — `mw_compress` (gzip/br по `Accept-Encoding`) и request-body compression на upload; (3) **архивы/форматы** — `.gz`/zlib (PNG/zip-член/`tar.gz`), registry-tarball'ы Plan 03.x, gRPC-message-compression. **Приоритет — DECOMPRESSION (inflate):** Ф.1 (raw-DEFLATE + zlib + gzip decode, streaming + bomb-cap) приземляется **первой** и закрывает HTTP-gate; brotli-decode (Ф.2) и encode (Ф.3) — следом; **zstd → followup** (§11).

> **⚠ Reconcile-нота (статус Ред.2 2026-07-03):** противоречие «Nova-logic над C-zlib FFI» в Plan 178 **УЖЕ СНЯТО** (grep «C-zlib» по 178 = 0; 178 говорит «inflate/gzip/zlib = pure-Nova; C-FFI только brotli» и называет decompress-gate = Plan 179 Ф.1/Ф.2 по имени). Plan 179 — source-of-truth по pure-vs-C (это его домен); Ф.0 — только **re-verify** (grep) + добить стейл-указатели при дрейфе (§6).

## 1a. Где Nova ЛУЧШЕ peers (differentiators — в доку)

- **🏆 bomb-cap-by-DEFAULT (decompression-DoS by construction):** **каждый** decode (`inflate`/`gzip_decode`/`Inflater.feed`) принимает `max_output` и при превышении возвращает **`CompressError{kind: Bomb(limit)}`** — НЕ растёт безгранично. Decompression-bomb (10 KB → 10 GB, классический DoS — `42.zip`) — **известнейшая дыра HTTP-клиентов/архиваторов**; Go `flate.NewReader`/Rust `flate2`/Java `Inflater` по умолчанию распаковывают **без лимита** (нужно вручную городить `io.LimitReader`/счётчик — все забывают). Node добавил `maxOutputLength` лишь после CVE-волны, и он **opt-in**. У Nova лимит — **обязательный параметр API** (`max_output` — see-it-in-the-signature), §8.0-security-критический, прямо служит Plan 178. **Никакой peer не делает cap дефолтом сигнатуры**, а единственный capped-one-shot peer (Swift `compression_decode_buffer`) при переполнении **молча обрезает** (возврат dst_size неотличим от exact-fit; высокоуровневый `NSData.decompressed(using:)` — вообще без cap) — антипример: cap обязан быть **ошибкой `Bomb`, не truncate**. *(Escape-hatch `max_output=0`=без-лимита существует для caller-trust low-level, но Plan 178 ВСЕГДА передаёт реальный cap — §3.3.)*
- **🏆 cap покрывает И ВХОД, не только выход (anti-flood):** счётчик ограничивает не только output-байты, но и **прогресс по входу** — флуд из миллионов пустых gzip-членов или гигантское `FNAME`-поле (output≈0, но CPU/память растут) ловятся как `Bomb`/`InvalidData`, а не зависают (§3.3, критик-gap). Go `Multistream(true)` тут уязвим. Nova капит обе оси by-construction.
- **🏆 streaming-decoder интегрируется с `BodyReader` (chunk-by-chunk, не one-shot) + cap-on-the-fly + bounded-per-call:** `Inflater`/`GzipReader` — инкрементальный value-coder (`feed(chunk)` + `read(max_emit) -> Result[Option[[]u8], CompressError]` + `finish()`), который скармливается чанками прямо из HTTP `BodyReader` ([178 §3.5 `BodyReader.@next_chunk`]). Bomb-cap **работает по ходу стрима** (счётчик растёт инкрементально, throw на пороге — не «сначала распаковали 10 GB, потом проверили»), **и каждый `read` отдаёт ≤ `max_emit` байт** — один highly-compressible входной чанк (32 KB → ~1 GB при ratio 1032:1) НЕ форсирует гигантскую single-аллокацию (§3.4, критик-gap). Go/Rust дают стрим **без встроенного cap**; Node стримит, но cap по итогу.
- **🏆 pure-Nova inflate transparency (nv-sourcing, прецедент Zig/Go):** raw-DEFLATE (RFC 1951) + zlib (1950) + gzip (1952) framing + CRC-32/Adler-32 — **детерминированные bit-stream-алгоритмы в ЧИСТОЙ Nova** (как `std.compress` в Zig и `compress/flate` в Go — оба pure, без C). Никакого скрытого C-blob для самого частого кодека: читаемо, аудитируемо, portable, GC-safe, **bomb-cap встроен в Nova-логику** (не зависит от поведения C-либы). Бьёт Rust-`flate2`-over-system-`zlib` (C-зависимость, разные версии в дистрибутивах) и Node (всё C). Brotli (RFC 7932 — 120 KB static dictionary + context modeling, **на порядок сложнее**) — прагматично C-FFI к `libbrotlidec` для V1, pure-Nova-brotli = followup; решение явно зафиксировано в §3.0.
- **🏆 Result-typed `CompressError` с expected/got у Checksum (D325):** единый структурный `CompressError{kind, ...}` с OPEN `ErrorKind` — `InvalidData(str)` · `UnexpectedEof` · `Bomb(limit)` · `UnsupportedMethod(str)` · `BadHeader(str)` · `Checksum{kind, expected u32, got u32}` (**несёт фактические значения** — не просто «mismatch») · `TrailingData` · `Other(str)` — wildcard-arm forced. Бьёт Go (`flate.CorruptInputError`/`gzip.ErrChecksum`-sentinel — без значений), Node (`Error.code`-string), Java (`DataFormatException`-checked-шум), Zig (error-union без data). **CRC/Adler — обязательная верификация** (не игнорируется на скорость); mismatch → `Checksum{expected, got}`.
- **byte-first done RIGHT:** вход/выход кодеков — **`[]u8`** (компрессия — байтовая операция, текста на этом уровне нет); никакого `str` в API кодека. Совпадает с zlib-ABI и byte-first-курсом std ([176:62]). **PURE codec — БЕЗ effect-триады** (нет I/O: чистая CPU-трансформация байт→байт): просто fallible-функции + value-type coder — сознательно проще net/fs (которым нужен `Http`/`Fs`-seam); конвенция различает effect-subsystem (триада) и pure-codec (нет seam), precedent `json`/`base64`.
- **format-transparent decode:** `gzip_decode`/`zlib_decode`/`inflate`/`brotli_decode` — единая форма `fn(data, max_output) -> Result[[]u8, CompressError]`; streaming — единый паттерн `feed`/`read`/`finish` на всех кодеках. HTTP-слой выбирает декодер по `Content-Encoding` единообразно (один dispatch, не четыре спец-API).

## 2. Эталон (cross-lang compress)

**Колонки:** Nova-target | Go `compress/{flate,gzip,zlib}` (+3rd-party brotli) | Rust `flate2`+`brotli` | Node `zlib` (built-in C) | Java `Deflater`/`Inflater`+GZIP | Zig `std.compress` | Swift `Compression` | Python `zlib`/`gzip`/`brotli`. **🏆** = Nova **строго лучше** лучшего peer'а; **=** = на уровне лучшего.

| Фича | **Nova-target** | Go compress (std) | Rust flate2+brotli | Node zlib (C) | Java Deflater/GZIP | Zig std.compress | Swift Compression | Python zlib/gzip/brotli |
|---|---|---|---|---|---|---|---|---|
| **raw-DEFLATE decode** (RFC 1951) | **= pure-Nova `inflate()`** (Zig/Go-precedent) | `flate.NewReader` (pure) = | `flate2` (miniz_oxide pure/zlib) = | `inflateRaw` (C) = | `Inflater(nowrap)` (zlib) = | `flate.decompress` (pure) = | `COMPRESSION_ZLIB` = **raw-DEFLATE** (документированный misnomer) | `decompress(wbits=-15)` = |
| **zlib decode** (RFC 1950, Adler-32) | **= pure-Nova `zlib_decode()` + Adler-32-verify** | `zlib.NewReader` (pure) = | `flate2` ZlibDecoder = | `inflate` (C) = | `Inflater` (zlib) = | `zlib.decompress` (pure) = | **n/a** (нет zlib-framing/Adler — misnomer) | `zlib.decompress` = |
| **gzip decode** (RFC 1952, CRC-32+ISIZE) | **🏆 pure-Nova `gzip_decode()` + CRC-32+ISIZE-verify + multi-member** | `gzip.NewReader` (CRC, multistream) = | `flate2` GzDecoder = | `gunzip` (C) = | `GZIPInputStream` (CRC) = | `gzip.decompress` (pure) = | n/a (нет gzip-framing) | `gzip.decompress` = |
| **brotli decode** (RFC 7932) | **= C-FFI `brotli_decode()` (libbrotlidec)** — pure-Nova=followup | **нет в std** (3rd-party) | `brotli` crate (pure) = | `brotliDecompress` (C) = | **нет** (3rd-party) | **нет** | n/a | 3rd-party `brotli` |
| **zstd decode** | followup (§11) — **scope-out V1** | 3rd-party | 3rd-party | `zstd` (Node 22+, C) | 3rd-party | `zstd` pure ✓ (`window_size_max` 8 MiB + `verify_checksum`) | `LZFSE` (не zstd) | 3rd-party |
| **encode** (deflate/gzip + levels) | **= `deflate`/`gzip_encode` + `Deflater`(level 0..9)** (Ф.3) | `flate.NewWriter`/`gzip.Writer` = | `flate2` encoder = | `gzip`/`deflate` = | `Deflater(level)` = | `flate.compress` (pure; **levels 4..9** + store/huffman) = | zlib фикс. ≈5, **НЕТ level-knob** | `compressobj(level)` = |
| **streaming (chunked decode)** | **🏆 `feed`/`read(max_emit)`/`finish` + cap-on-the-fly + bounded-per-call + `BodyReader`** | `io.Reader` (no cap) | `flate2::read` (no cap) | `createGunzip()` (cap by-итог) | `InflaterInputStream` (no cap) | reader (no cap) | stream (no cap) | `decompressobj().decompress(s, maxlen)` (есть maxlen!) |
| **bomb-cap (DoS)** | **🏆 ОБЯЗАТЕЛЬНЫЙ `max_output` в сигнатуре → `Bomb`; вход ТОЖЕ капится (anti-flood)** | **нет** (ручной `LimitReader`) | **нет** (ручной wrapper) | `maxOutputLength` **opt-in** | **нет** | нет (flate); zstd `window_size_max` | **silent-truncate** (хуже «нет») | per-call `max_length` (ручной loop) |
| **checksum-verify (CRC/Adler)** | **🏆 verify by-default → typed `Checksum{expected,got}`** | verify (sentinel, без значений) | verify | verify (silent/error) | verify (`ZipException`) | verify | **n/a** (нет framing) | verify (`error`) |
| **pure-Nova vs C** | **🏆 inflate/zlib/gzip = PURE Nova; brotli = C-FFI (V1)** | **pure Go** ✓ | C-zlib/pure miniz_oxide | **всё C** | **C** (zlib JNI) | **pure Zig** ✓ (incl. zstd) | **C** (system) | **C** |
| **error model** | **🏆 typed `CompressError`+OPEN kind+`InvalidData`/`Bomb`/`Checksum{exp,got}`/`UnexpectedEof`** | sentinel'ы + string | typed = | `Error.code` string | checked-шум | error-union (без data) = | `errno` | `error` string |
| **API uniformity** | **🏆 `fn(data, max) -> Result` + единый `feed/read/finish`** | per-pkg типы | per-decoder | per-fn | per-class | per-namespace | per-algo-enum = | per-module |

**Взять:** Go **pure-implementation-дух** (flate/gzip/zlib без C) + gzip **multi-member/multistream** + `Reset`-reuse; Python `decompressobj(...).decompress(data, max_length)` — **единственный peer с per-call output-cap** (взять как baseline, но сделать cap **обязательным**, не optional); Rust `flate2` **level-API** (0..9); Zig **allocation/streaming-transparency** + zstd-pure (followup-ориентир); Node **brotli-built-in-удобство** (но C — у Nova явный C-FFI gate); Zig **encode-levels 4..9** (0-3 сознательно выброшены — прецедент «3 различимых режима достаточно», Q7) + Zig pure **xz/lzma-decode** (§11-ориентир). **Избегать:** **универсального отсутствия bomb-cap** (Go/Rust/Java/Zig/Swift); Go **россыпь sentinel-ошибок** + per-package-типы; Node/Python **stringly error.code**; Java **checked-шум**; brotli-**отсутствие в std** (Nova несёт via C-FFI); Swift **silent-truncate-cap** (антипример Bomb-vs-truncate), zlib-misnomer (`COMPRESSION_ZLIB`=raw-DEFLATE), gzip-дыру (3rd-party GzipSwift) и streaming-без-sync-flush (только FINALIZE). **Доказательство ≥ best-peer построчно:** **gzip-decode / brotli / streaming-cap / bomb-cap / checksum-verify / pure-vs-C / error-model / API-uniformity** = 🏆; **raw-DEFLATE / zlib / encode** = **=** (паритет pure-Go/Zig/flate2); **zstd** — честно **scope-out V1 → followup** (§11; Zig единственный несёт pure-zstd в std).

---

## 3. Архитектура

**Принцип (codec-precedent: base64/json).** `std/encoding/compress` — **чистый CPU-кодек над байтами**, как `base64`/`json`: **НЕТ I/O-эффекта** (нет триады effect+real+mock — нечего мокать, операция детерминирована от входных байт). Это plain fallible-функции (`Result[T, CompressError]`, R1/D325) над `[]u8`, плюс **value-type инкрементальный кодер** (`Inflater`/`Deflater`) для стриминга. byte-first везде: вход/выход — `[]u8`; `str` не фигурирует (компрессия бинарна).

**nv-sourcing-граница (закрыто §3.0).** DEFLATE-`inflate` (RFC 1951), zlib-framing (RFC 1950 + **Adler-32**), gzip-framing (RFC 1952 + **CRC-32**) — **чистая Nova-логика** (битстрим-декодер Хаффмана + LZ77-back-reference-копирование). Прецедент: **Zig `std.compress.flate`** и **Go `compress/flate`** — обе чистые. Brotli (RFC 7932) — **C-FFI к `libbrotlidec`/`libbrotlienc`** (google/brotli): 120 KB статического словаря + контекстное моделирование делают pure-Nova-порт несоразмерным V1 (precedent: net поверх libuv). Brotli — **отдельная фаза Ф.2 за C-FFI**; pure-Nova-brotli — followup §11.

**Целочисленность (Plan 176-конвенция [176:74]).** Все размеры/счётчики/позиции/длины/дистанции — **`int` (i64)**: `max_output`, `bytes_written`, LZ77 length(3..258)/distance(1..32768), смещения bit-reader. **Исключение — `u32`** только там, где величина **по природе 32-битна**: CRC-32/Adler-32-значения и gzip-`ISIZE` (поле потока). **ISIZE-сравнение — `(uncompressed_len mod 2^32) == ISIZE`** (НЕ raw-equality): поток > 4 GiB имеет `ISIZE != actual_len`, но `mod 2^32` совпадает → НЕ `Checksum` (§3.3/D336, slow-тест на wrap). Length/distance-арифметика bit-reader — bounds-checked (distance > текущего размера окна → `InvalidData`, не OOB-read).

### Layering diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│ App / Plan 178  inflate(buf, cap)  ·  gz.feed(chunk); gz.read(max_emit)    │  ← one-shot + streaming value-API
├──────────────────────────────────────────────────────────────────────────┤
│ one-shot fns (inflate/gzip_decode/zlib_decode/deflate/gzip_encode)         │  ← convenience поверх кодеров
├───────────────────────────────────┬────────────────────────────────────────┤
│ Inflater / Deflater value-кодер   │ GzipReader / GzipWriter (framing+csum)  │  ← инкрементальный, bomb-cap
│ (DEFLATE bitstream, .nv)          │ ZlibReader / ZlibWriter                 │  ← plain value (НЕ consume)
├───────────────────────────────────┴────────────────────────────────────────┤
│ Huffman-decode · LZ77 window · CRC-32 · Adler-32   (ВСЁ .nv, нет C)         │  ← pure-Nova
├──────────────────────────────────────────────────────────────────────────┤
│ BrotliReader / brotli_encode ──FFI──► libbrotlidec / libbrotlienc (Ф.2)    │  ← brotli = C-FFI (ffi.nv), CONSUME
└──────────────────────────────────────────────────────────────────────────┘
```

**Plan 178-интеграция (HARD-GATE 178 Q12).** Стримовые кодеры — value-типы, **оборачивающие любой источник `[]u8`-чанков** (не только `BodyReader`): `feed(chunk)` накапливает вход; `read(max_emit) -> Result[Option[[]u8], _]` тянет ≤ `max_emit` раскодированных байт (`None` = stream-end); `finish()` валидирует завершённость+checksum. Plan 178 `BodyReader.@next_chunk` ([178 §3.5 `BodyReader.@next_chunk`]) → `gz.feed(chunk)` → `gz.read(budget)` → отдаёт раскодированный чанк наружу (park/wake-backpressure на стороне 178-транспорта; сам кодек без effect). **bomb-cap** — first-class: накопленный выход > `max_output` → `CompressError{Bomb}` **немедленно** (не дочитав вход). Plan 178 пробрасывает `@max_decompressed(n)` (дефолт 100 MiB [178 §3.client `@max_decompressed`]) → `max_output`. Это **§8.0-критичный security-инвариант**.

### 3.1. `CompressError` — единый структурный + OPEN ErrorKind (D325)

```nova
module std.encoding.compress

/// Единственная структурная ошибка домена compress (R1/R5/D325). OPEN `ErrorKind`
/// (wildcard-арм обязателен у потребителя). `offset` = байт-позиция во входе (диагностика).
#stable(since = "0.1")
export type CompressError value { ro kind ErrorKind, ro offset Option[int] }

export type ErrorKind
    | InvalidData(str)                                  // битый битстрим / неверный Хаффман / bad block-type (RFC 1951 §3.2.3)
    | UnexpectedEof                                     // вход кончился посреди блока/фрейма/trailer'а
    | Checksum { ro kind ChecksumKind, ro expected u32, ro got u32 }   // CRC-32/Adler-32/ISIZE не сошёлся — НЕСЁТ значения (🏆 §1a)
    | BadHeader(str)                                    // gzip magic 0x1f8b / zlib CMF·FLG %31≠0
    | Bomb(int)                                         // выход (или прогресс-вход) превысил max_output; int = сработавший лимит
    | UnsupportedMethod(str)                            // CM≠8 в zlib/gzip; FDICT=1 (preset-dict V1); brotli без C-фичи
    | TrailingData                                      // мусор после валидного потока (strict-режим — см. §3.3)
    | Other(str)                                        // OPEN → wildcard обязателен

export type ChecksumKind | Crc32 | Adler32 | Isize

export fn CompressError @to_str(self) -> str
```

`offset` — позиция во входном буфере (логи; `None` для framing/checksum-ошибок). **Никакого `Fail[E]` наружу** (R5): все функции → `Result`. Plan 178 source-chainит: `CompressError{Bomb}` → `HttpError{kind: BodyTooLarge}` (унификация с bomb-cap тела), прочие → `HttpError{kind: Protocol(...)}` (битый Content-Encoding). **Закрытие критик-gap «единый kind на FDICT/CM≠8»:** `CM≠8` и `FDICT=1` → **`UnsupportedMethod`** (это supported-method-gap, preset-dict = followup §11); `%31≠0`/bad-magic → `BadHeader`; **ровно один kind на кейс** (test-conventions: один маркер/assert).

### 3.2. Контрольные суммы (pure-Nova, переиспользуемы)

Решение Ф.0/§6: переиспользуем **уже существующий** `std/_experimental/checksums/crc32.nv` (RFC 1952-совместим, test-vector `0xCBF43926` PASS) в его **free-function-форме** (`crc32`/`crc32_init`/`crc32_update`/`crc32_finalize`, state-passing `u32`) — **меньше churn, тесты уже зелёные**. Промоут модуля в `std/encoding/compress/checksum.nv` (`module std.encoding.compress`). Adler-32 — **NEW** в той же форме. (Value-type-обёртка `Crc32`/`Adler32` с `mut @update` — **НЕ** делаем в V1: это была бы новая код-форма, не «реюз»; free-functions достаточны для framing-кода; обёртка — опц. followup.)

```nova
// CRC-32 (IEEE 802.3, poly 0xEDB88320) — gzip trailer. Промоут из _experimental.
export fn crc32(data []u8) -> u32                       // one-shot; "123456789" → 0xCBF43926
export fn crc32_init() -> u32                           // 0xFFFFFFFF
export fn crc32_update(state u32, data []u8) -> u32     // incremental, без финализации
export fn crc32_finalize(state u32) -> u32              // ^ 0xFFFFFFFF

// Adler-32 (RFC 1950 §9, mod 65521) — zlib trailer. NEW.
export fn adler32(data []u8) -> u32                     // one-shot
export fn adler32_init() -> u32                         // a=1, b=0  (упаковано: (b<<16)|a)
export fn adler32_update(state u32, data []u8) -> u32   // incremental
export fn adler32_finalize(state u32) -> u32            // identity (Adler уже в финальной форме)
```

Чистые функции (no-effect, no-resource). Экспортируются — полезны самостоятельно (integrity, PNG, ETag). Codec-таблицы CRC — runtime-lazy (как сейчас `table_value` в crc32.nv); comptime-const-array = followup §11 (perf, не корректность).

### 3.3. One-shot decode (PRIORITY — анблокит Plan 178 Ф.2)

```nova
/// Raw DEFLATE (RFC 1951): голый битстрим без фрейма/чексуммы. `max_output` — bomb-cap
/// (выход > лимита → CompressError{Bomb}). 0 = без лимита (caller-trust low-level; Plan 178
/// ВСЕГДА передаёт реальный cap). Trailing-data после BFINAL → TrailingData (strict, raw/zlib).
#stable(since = "0.1")
export fn inflate(data []u8, max_output int) -> Result[[]u8, CompressError]

/// zlib (RFC 1950): 2-байт CMF/FLG-заголовок + DEFLATE + Adler-32-trailer.
/// Проверяет CMF (CM=8, CINFO≤7), (CMF·256+FLG)%31==0, FDICT=1 → UnsupportedMethod (preset-dict V1).
#stable(since = "0.1")
export fn zlib_decode(data []u8, max_output int) -> Result[[]u8, CompressError]

/// gzip (RFC 1952): magic 0x1f8b + DEFLATE + CRC-32 + ISIZE-trailer.
/// Пропускает FNAME/FCOMMENT/FEXTRA/FHCRC; CRC-32 И ISIZE (mod 2^32) проверяются.
/// MULTI-MEMBER: конкатенированные члены склеиваются (RFC 1952 §2.2). Trailing-data
/// ПОСЛЕ валидного члена, не образующее новый член, → лениво игнорируется (multistream-совместимость).
#stable(since = "0.1")
export fn gzip_decode(data []u8, max_output int) -> Result[[]u8, CompressError]

/// brotli (RFC 7932). C-FFI libbrotlidec (Ф.2). WBITS из потока; bomb-cap через max_output.
#stable(since = "0.1")
export fn brotli_decode(data []u8, max_output int) -> Result[[]u8, CompressError]
```

**Контракт checksum (§8.0, D336):** `gzip_decode`/`zlib_decode` **обязаны** валидировать trailer — несовпадение → `Checksum{kind, expected, got}`. **ISIZE — `(len mod 2^32)==ISIZE`** (НЕ raw; §3-целочисленность). Тихий пропуск чексуммы — недопустимое упрощение (silent data-corruption). pos: round-trip с правильной суммой; neg: подмена 1 байта тела → `Checksum`.

**Контракт anti-flood (§8.0, D334, критик-gap).** Output-cap **сам по себе** не ловит вход, дающий ~0 выхода: (a) флуд из миллионов пустых gzip-членов (каждый ~20 байт, 0 выхода), (b) гигантское `FNAME`/`FEXTRA`/`FCOMMENT`-поле в gzip-header. Поэтому `max_output` ограничивает **И прогресс по входу**: если consumed-input-байт стало `> max_output` (когда `max_output>0`) при выходе ≤ малого, → `Bomb` или (для giant-header) `InvalidData`. neg-тесты: 100k пустых членов → `Bomb`/`InvalidData` (не hang); 4 GB `FNAME` → `InvalidData` (не unbounded skip).

**Контракт incomplete-Huffman (§8.0, критик-gap, interop).** RFC 1951 / zlib допускают **один incomplete distance-code** (single-distance-code special case) — строгие canonical-builder'ы его отвергают, но реальные PNG/zlib-энкодеры его эмитят. Декодер **ПРИНИМАЕТ** этот единственный случай (RFC 1951 §3.2.7 / zlib `enough.c` rationale); прочие over-/under-subscribed наборы → `InvalidData`. pos-тест: поток с одним distance-code декодится; neg-тест: over-subscribed → `InvalidData`.

### 3.4. STREAMING decode — инкрементальные кодеры (Plan 178 BodyReader)

`Inflater` — value-кодер-машина: кормишь `[]u8`-чанками (`feed`), тянешь раскодированный выход **порциями** (`read(max_emit)`). **bomb-cap встроен** (накопл. выход/прогресс-вход > лимита → `Bomb`). **НЕ must-consume** (Q6): нет внешнего ресурса/release-обязательства — окно 32 KiB и checksum-state в GC-памяти (precedent base64-`Buffer`). `GzipReader`/`ZlibReader` — надстройки с framing + checksum-аккумуляцией. **Исключение — `BrotliReader`** (§3.6): держит C-instance → **consume** (release-долг).

```nova
/// Инкрементальный DEFLATE-декодер. value-машина (НЕ consume): LZ77-окно (32 KiB),
/// частичный Хаффман-стейт, накопл.-выход-счётчик (bomb-cap), unconsumed-output-буфер.
#stable(since = "0.1")
export type Inflater value { priv st *InflaterState }

export fn Inflater.new(max_output int) -> Inflater                              // 0 = без лимита
/// Подать сжатый вход (буферизуется внутри). Возвращает () — выход тянется через read().
export fn Inflater mut @feed(self, chunk []u8) -> Result[(), CompressError]
/// Вытянуть ДО `max_emit` раскодированных байт. None = поток завершён (clean EOF).
/// `max_emit` гарантирует bounded-per-call аллокацию (один compressible-чанк НЕ форсирует
/// >cap-но-большую single-аллокацию — §1a streaming-bounded). Внутри проверяет bomb-cap.
export fn Inflater mut @read(self, max_emit int) -> Result[Option[[]u8], CompressError]
/// Сигнал «вход кончился». Незавершённый битстрим (нет BFINAL) → UnexpectedEof.
export fn Inflater mut @finish(self) -> Result[(), CompressError]
export fn Inflater @is_done(self) -> bool                                       // встретил BFINAL
export fn Inflater @bytes_written(self) -> int                                  // диагностика bomb-cap

/// gzip-стрим-декодер: framing (header-parse + CRC-32 + ISIZE) поверх Inflater. Тот же
/// feed/read/finish-контракт; finish() верифицирует CRC-32 + ISIZE(mod 2^32). НЕ consume.
#stable(since = "0.1")
export type GzipReader value { priv inner GzipState }
export fn GzipReader.new(max_output int) -> GzipReader
export fn GzipReader mut @feed(self, chunk []u8) -> Result[(), CompressError]
export fn GzipReader mut @read(self, max_emit int) -> Result[Option[[]u8], CompressError]
export fn GzipReader mut @finish(self) -> Result[(), CompressError]
export fn GzipReader @is_done(self) -> bool

/// zlib-стрим-декодер: Adler-32-trailer-верификация поверх Inflater. НЕ consume.
#stable(since = "0.1")
export type ZlibReader value { priv inner ZlibState }
export fn ZlibReader.new(max_output int) -> ZlibReader
export fn ZlibReader mut @feed(self, chunk []u8) -> Result[(), CompressError]
export fn ZlibReader mut @read(self, max_emit int) -> Result[Option[[]u8], CompressError]
export fn ZlibReader mut @finish(self) -> Result[(), CompressError]
export fn ZlibReader @is_done(self) -> bool
```

**EOF-семантика (критик-gap, D335):** в стриме «нет ещё BFINAL» — **легитимное need-more-input** (НЕ ошибка); `read` отдаёт доступное и `Some([])`/`None`-по-готовности. `finish()` ДО BFINAL → `UnexpectedEof`. Граница-блока без BFINAL = valid-partial (ждём `feed`). Mid-symbol = need-more. **Trailing-data:** raw/zlib `finish` при мусоре после BFINAL → `TrailingData` (strict); gzip — лениво (multistream). neg-тесты на `UnexpectedEof`(truncate) и `TrailingData`(raw) — оба в §7.

**Plan 178 BodyReader-композиция (контракт-доказательство; glue в `real_http` Plan 178, НЕ в compress — направление зависимости: compress НЕ знает про http, §6):**

```nova
// псевдо-glue в real_http (Plan 178): decompress-обёртка над BodyReader
loop {
    match raw_reader.@next_chunk()? {            // Http-park над транспортом ([178 §3.5 `BodyReader.@next_chunk`])
        Some(enc) => {
            gz.@feed(enc)?                        // буферизуем вход
            loop {
                match gz.@read(64 * 1024)? {      // тянем bounded-чанками; Bomb внутри → 178 BodyTooLarge
                    Some(dec) => if dec.len() > 0 { yield dec } else { break }
                    None => { gz.@finish()?; return }   // финализация + CRC/ISIZE-проверка
                }
            }
        }
        None => { gz.@finish()?; return }
    }
}
```

**Единый feed/read/finish-контракт у всех ридеров** — Plan 178 выбирает кодер по `Content-Encoding` и работает полиморфно. Опц. протокол `ChunkDecoder` (`mut @feed; mut @read; mut @finish; @is_done`) — **деталь Plan 178** (там dispatch по encoding); **compress его НЕ экспортирует** (избегаем навязывания trait; duck-typing/match на стороне http).

### 3.5. One-shot encode + инкрементальные энкодеры (Ф.3 — полнота)

Сжатие нужно для **HTTP request-body** и **server response-encode** (`mw_compress`). Encode НЕ имеет bomb-cap (выход < входа). `level` — `CompressLevel`. Энкодеры — **value (НЕ consume)**: нет внешнего ресурса.

```nova
export type CompressLevel value { priv n u8 }          // raw 0..11; интерпретация PER-CODEC (deflate 0..9, brotli 0..11)
export fn CompressLevel.fastest() -> CompressLevel     // 1
export fn CompressLevel.default() -> CompressLevel     // 6
export fn CompressLevel.best()    -> CompressLevel     // СЕНТИНЕЛ: резолвится per-codec (deflate→9, brotli→11)
export fn CompressLevel.none()    -> CompressLevel     // 0 = stored-only
export fn CompressLevel.new(n u8) -> Result[CompressLevel, CompressError]   // >11 → InvalidData; per-codec range-валидация НА ВЫЗОВЕ кодека: deflate(level=10..11) → InvalidData

// ── one-shot ──
export fn deflate(data []u8, level CompressLevel)       -> Result[[]u8, CompressError]   // raw DEFLATE
export fn zlib_encode(data []u8, level CompressLevel)   -> Result[[]u8, CompressError]   // + CMF/FLG + Adler-32
export fn gzip_encode(data []u8, level CompressLevel)   -> Result[[]u8, CompressError]   // + gzip header + CRC-32 + ISIZE
export fn brotli_encode(data []u8, level CompressLevel) -> Result[[]u8, CompressError]   // C-FFI (Ф.2)

// ── инкрементальный (стриминг-encode для chunked-TE / server-stream) ──
#stable(since = "0.1")
export type Deflater value { priv st *DeflaterState }
export fn Deflater.new(level CompressLevel) -> Deflater
export fn Deflater mut @feed(self, chunk []u8) -> Result[[]u8, CompressError]   // отдаёт готовый сжатый выход
/// SYNC-FLUSH (Z_SYNC_FLUSH): byte-align + пустой stored-блок (00 00 FF FF) → весь скормленный
/// вход становится decodable-префиксом (SSE/chunked поверх gzip). Прецеденты: Go Writer.Flush,
/// Node flush(Z_SYNC_FLUSH), Java SYNC_FLUSH, Zig Compressor.flush; Swift НЕ имеет (его боль).
export fn Deflater mut @flush(self) -> Result[[]u8, CompressError]
export fn Deflater mut @finish(self) -> Result[[]u8, CompressError]             // flush + BFINAL

#stable(since = "0.1")
export type GzipWriter value { priv inner GzipEncState }    // framing + CRC поверх Deflater
export fn GzipWriter.new(level CompressLevel) -> GzipWriter
export fn GzipWriter mut @feed(self, chunk []u8) -> Result[[]u8, CompressError]
export fn GzipWriter mut @flush(self) -> Result[[]u8, CompressError]         // sync-flush (см. Deflater)
export fn GzipWriter mut @finish(self) -> Result[[]u8, CompressError]           // дописывает CRC-32 + ISIZE
// (ZlibWriter — симметрично, Adler-32-trailer; тоже несёт @flush)
```

**Encode-объём и level-честность (закрыто §3.0, simplification-audit).** V1-энкодер **корректен по фрейму/чексумме на всех level**, но ratio-оптимум (dynamic-Huffman level-9) может уступать zlib. Чтобы **level-knob не был ложью**: V1 даёт **минимум 3 различимых режима** — `none()`=stored (без Huffman), `fastest..default`=fixed-Huffman+greedy-LZ77, `best()`=dynamic-Huffman+lazy-matching (хуже zlib-9 по ratio, но **реально лучше** fixed). Полный optimal-parse — followup §11. Это **scoped-out с rationale** (на encode-пути Ф.3 «позже», НЕ критпуть 178-decode), НЕ подвешенный пробел.

### 3.6. FFI-слой brotli (`ffi.nv`, Ф.2)

```nova
// std/encoding/compress/ffi.nv — module-private (нет export). C: libbrotlidec/libbrotlienc.
module std.encoding.compress
type CBrotliDec(*())                                          // BrotliDecoderState*
type CBrotliEnc(*())                                          // BrotliEncoderState*
extern "C" fn brotli_dec_new() -> CBrotliDec
// ⚠ C-ABI (174.6 M1): []u8/Vec = GC-типы, в extern-сигнатурах ЗАПРЕЩЕНЫ (E_FFI_NON_C_ABI_TYPE) →
// raw-ptr+len / out-буфер+cap; конвертация []u8 ↔ ptr — в Nova-обёртке brotli.nv (C-shim как в net).
extern "C" fn brotli_dec_stream(h CBrotliDec, input *u8, input_len int, out *mut u8, out_cap int) -> (int, int)   // (code, written): 0=more,1=done,2=err
extern "C" fn brotli_dec_error(h CBrotliDec) -> int                                       // BrotliDecoderGetErrorCode
extern "C" fn brotli_dec_free(h CBrotliDec) -> ()
extern "C" fn brotli_enc_oneshot(data *u8, data_len int, quality int, lgwin int, out *mut u8, out_cap int) -> (int, int)
```

```nova
/// brotli-стрим-декодер: BrotliDecoderDecompressStream-обёртка. ДЕРЖИТ C-instance →
/// MUST-CONSUME (D133): release-долг (free C-state). Единственный consume-кодер в плане.
#stable(since = "0.1")
export type BrotliReader consume value { priv h CBrotliDec, priv max_output int, priv written int }
export fn BrotliReader.new(max_output int) -> BrotliReader
export fn BrotliReader mut @feed(self, chunk []u8) -> Result[(), CompressError]
export fn BrotliReader mut @read(self, max_emit int) -> Result[Option[[]u8], CompressError]   // bomb-cap on-the-fly
export fn BrotliReader consume @finish(self) -> Result[(), CompressError]                       // free C-instance + clean-EOF
```

**Bomb-cap-over-FFI (критик-gap, D337/D334):** `max_output` капит **output-байты** инкрементально (через `out_cap`-буфер `brotli_dec_stream`). C-декодер **сам** держит внутренний ring-buffer/окно (≤ 16 MiB при `lgwin≤24`) — это **фиксированно-ограничено самим форматом** (lgwin кодируется ≤24), задокументировано как acceptable (output-cap ≠ window-cap; Node `maxOutputLength` так же капит только output). neg-тест: brotli-поток с max-lgwin + bomb-тело → `Bomb` на output-cap, НЕ OOM.

C-handle освобождается через `consume @finish` (или drop-path RAII-финализатор). **Feature-gate (Q11):** если brotli-libs недоступны в сборке → `brotli_decode`/`encode`/`BrotliReader.new` возвращают `CompressError{UnsupportedMethod("brotli not built")}` (НЕ паника). Ф.1 (inflate/gzip/zlib pure-Nova) **самодостаточна** без brotli и анблокит Plan 178 gzip/deflate.

### 3.0. Закрытые решения (closed-decisions)

| # | Вопрос | РЕШЕНИЕ | Обоснование (peer) |
|---|--------|---------|--------------------|
| Q1 | inflate/gzip/zlib: pure-Nova vs C-FFI | **pure-Nova** (битстрим Хаффмана + LZ77 + CRC-32/Adler-32 в `.nv`) — 178 уже reconciled на pure-Nova (Ф.0 re-verify) | nv-sourcing; **Zig `std.compress.flate`** + **Go `compress/flate`** обе чистые — алгоритм well-defined |
| Q2 | brotli: pure-Nova vs C-FFI | **C-FFI `libbrotlidec`/`libbrotlienc`** (Ф.2 за gate); pure-Nova-brotli = followup | 120 KB словарь + контекст-моделирование → несоразмерно V1; precedent net-over-libuv; даже Zig std **не несёт** brotli |
| Q3 | effect-or-not | **НЕТ effect** (нет триады) — plain fallible-fns + value-кодеры над `[]u8` | чистый CPU-кодек, детерминирован; precedent `base64`/`json` — триада только для I/O |
| Q4 | streaming-модель | **value-кодер `feed`/`read(max_emit)`/`finish`**; единый контракт raw/gzip/zlib/brotli; `read` отдаёт ≤ max_emit (bounded-per-call) | компонуется над любым `[]u8`-источником (Plan 178 `BodyReader`); Python `decompressobj(...,max_length)` — единственный peer с per-call-cap |
| Q5 | bomb-cap (DoS) | **first-class `max_output`** (one-shot + streaming + brotli-FFI); накопл. выход/прогресс-вход > лимита → `Bomb` НЕМЕДЛЕННО; cap капит **И вход** (anti-flood); 0=trust (low-level); граница: `output==max_output` ok, первый байт сверх → `Bomb` | §8.0-security; Plan 178 пробрасывает `@max_decompressed` (100 MiB); Go исторически **без cap** (CVE-класс) |
| Q6 | must-consume кодеры? | **НЕТ для pure-Nova** (`Inflater`/`Deflater`/`GzipReader`/`ZlibReader`/`GzipWriter` = plain value); **ДА только `BrotliReader`** (держит C-instance → consume, D133) | нет release-долга у GC-памяти-окна; D133 — только для release-обязательства; precedent base64-`Buffer` (value). Brotli C-state = реальный долг |
| Q7 | encode-в-scope-or-later | **encode В SCOPE** (Ф.3): deflate/zlib/gzip_encode + `Deflater`/`GzipWriter`; brotli_encode за brotli-gate. Level-honesty: ≥3 различимых режима (stored/fixed/dynamic); optimal-parse = followup | HTTP нужен encode; V1 «good-enough» — корректность фрейма > ratio, но knob не пустой; прецедент Zig: std несёт ТОЛЬКО levels 4..9 + store/huffman (0-3 выброшены) = «3 различимых режима достаточно» |
| Q8 | byte-first | **`[]u8` вход/выход везде**, `str` не фигурирует | byte-first-конвенция; `str` = lossy на бинарных данных |
| Q9 | checksum-строгость | **gzip/zlib decode ОБЯЗАНЫ верифицировать** CRC-32/Adler-32 + **ISIZE(mod 2^32)**; несовпадение → `Checksum{kind,expected,got}` | тихий пропуск = silent corruption (§8.0); neg-тест на подмену байта |
| Q10 | gzip multi-member | **склеивать конкатенированные члены** (RFC 1952 §2.2); **счётчик-членов/прогресс-капится** (anti-flood) | `gzip -c a b` / лог-ротация; Go/zlib/Python/Node склеивают; Rust flate2 РАСКАЛЫВАЕТ API (`GzDecoder`=1 член vs `MultiGzDecoder` — источник silent-truncate-багов) — аргумент ЗА единый multi-дефолт; Go `Multistream(false)` opt-out → followup §11; empty-member-flood ловим (критик-gap) |
| Q11 | brotli-libs недоступны | **feature-gate**: `brotli_*`/`BrotliReader.new` → `UnsupportedMethod` (не паника); Ф.1 самодостаточна | сборка без C-deps не падает; Plan 178 gzip/deflate работает независимо |
| Q12 | целочисленность/overflow | **`int`(i64) для размеров/length/distance/offset; `u32` только CRC/Adler/ISIZE**; ISIZE-cmp = `mod 2^32`; bit-reader bounds-checked | Plan 176-конвенция [176:74]; >4 GiB-ISIZE-wrap НЕ Checksum; crafted distance/length → `InvalidData`, не OOB |
| Q13 | incomplete-Huffman | **принять один incomplete distance-code** (RFC 1951 §3.2.7 / zlib); прочие incomplete → `InvalidData` | реальные PNG/zlib-энкодеры эмитят; строгий reject → interop-fail (Go/zlib принимают) |
| Q14 | trailing-data strict? | **raw/zlib: strict → `TrailingData`** на мусоре после BFINAL; **gzip: lenient** (multistream-совместимость) | Go `flate` strict / gzip multistream — паритет |
| Q15 | crc32 реюз-форма | **free-function-форма** (промоут as-is, `crc32_init/update/finalize`); Adler-32 NEW в той же форме; value-type-обёртка = followup | существующие тесты (0xCBF43926) зелёные, меньше churn — «реюз», не «rewrite» |
| Q16 | zstd | **followup §11** (не V1) | RFC 8878; HTTP `zstd` редок (2024+); Plan 178 не требует; Zig — единственный pure-в-std |
**Условно-закрытые (решения приняты; re-verify при реализации — Ред.2 сняла JSON-артефакт авторинга):**
- **Модульная раскладка** (folder = один модуль `std.encoding.compress`): `error.nv` (CompressError) · `checksum.nv` (CRC-32 промоут + Adler-32 NEW) · `inflate.nv` (bit-reader + Huffman + Inflater + one-shot decode) · `deflate.nv` (Deflater + encode, Ф.3) · `gzip.nv`/`zlib.nv` (framing) · `brotli.nv` + `ffi.nv` (extern C, Ф.2) · `mod.nv` (re-export) — подтвердить раскладку при реализации (исполнитель Ф.1, §6).
- **Crc32-value-type:** решено НЕ делать в V1 (free-functions реюз, Q15); если framing покажет удобство инкрементального value-объекта — followup §11, не ломая free-fn API.
- **ChunkDecoder-протокол:** решено НЕ экспортировать из compress (деталь Plan 178 dispatch); если 178-реализация покажет нужду в общем protocol-bound — пересмотреть в координации с 178.

---

## 4. Фазы

**Dep-chain:** Ф.0 → **Ф.1 (inflate: raw-DEFLATE + zlib + gzip decode, streaming + bomb-cap)** → Ф.3 (encode) → Ф.6 (tests+docs); **Ф.2 (brotli decode, C-FFI)** параллельна Ф.3 после Ф.1. **«сейчас» (UNBLOCKS Plan 178 Ф.2):** Ф.0, Ф.1, Ф.6(decode-часть). **«позже»:** Ф.2 (brotli), Ф.3 (encode), Ф.6(encode/brotli-часть). Коммит после каждой фазы (§10).

> **🎯 Gate-релевантность:** **Plan 178 Q12 (auto-decompress) — 🔴 HARD-GATE именно на Ф.1.** Ф.1 **самодостаточна** и приземляет gate для gzip/deflate **сейчас**; `br`-ветка 178 gated на Ф.2. Ф.3 (encode) и Ф.2 (brotli) — для полноты, НЕ на критпути 178-decode.

- **Ф.0 — GATE (без кода). «сейчас».** (1) Закрыть §3.0 (готово). (2) **D-номера (Ред.2-карта):** committed high-water 3xx = **D355** (D326/D327/D328/D354/D355 уже в спеке — прежний claim «committed до D326» устарел); **D333–D337 свободны** (verify grep=0) + D338–D339 буфер; Plan 178 renumber'ится в **D357–D362** и НЕ заходит в D333–D339; reservation-нота в `docs/plans/README.md`/spec-index: «D333–D339 reserved Plan 179» (чтобы конкурентная 178-работа не схватила D333+). (3) **RECONCILE Plan 178 — ✅ выполнен (Ред.2 2026-07-03):** «C-zlib FFI» в 178 отсутствует (grep=0); Q12/§9 называют Plan 179 Ф.1/Ф.2 по имени → Ф.0 только re-verify grep + синхронизация line-refs при дрейфе. (4) **verify на main:** (a) `_experimental/checksums/crc32.nv` — RFC-1952-совместим (`0xCBF43926` PASS) → промоут-план (§6); (b) Adler-32 — **НЕ существует** → NEW Ф.1; (c) bit-ops/`[]u8`-slice/`Vec[u8].push` достаточны для LSB-first bit-reader + canonical-Huffman (verify против `vec_seq.nv`); (d) `consume value` для `BrotliReader` (verify против `Body` 178 / `File` 176); (e) **vendor-link-инфра brotli** — подтвердить, что проект собирает vendored static C (как `libz3.lib`/libuv) ДО коммита Ф.2 (simplification-audit: механизм link не верифицирован — Ф.0 проверяет реальный путь). (5) **D333–D337 spec-first** (§5). (6) **Координация:** подтвердить с owner Plan 178, что Ф.1 закрывает Q12-gate для gzip/deflate сейчас; brotli ждёт Ф.2. **GATE.** DEP: Plan 177 (D325), 176 (must-consume/byte-first), 178-schedule.

- **Ф.1 ✅ LANDED (2026-07-04) — inflate: raw-DEFLATE + zlib + gzip decode (streaming + bomb-cap). 🔴 UNBLOCKS Plan 178 Q12 (gzip/deflate).** **Чистая Nova (.nv), БЕЗ C** (precedent Zig/Go; §3.0 Q1). Реализовано полностью (см. «Ф.1 итог» вверху). Контент:
  - **RFC 1951 DEFLATE inflate:** LSB-first bit-reader над `[]u8`; три блок-типа — `00` stored, `01` fixed-Huffman, `10` dynamic-Huffman (`11`→`InvalidData`); canonical-Huffman-декодер (code-length → code); **incomplete-Huffman: принять один distance-code** (Q13); LZ77 back-reference (length 3..258, distance 1..32768) с copy-from-window incl. overlap `dist<len`; 32 KiB sliding-window; **distance > window → `InvalidData`** (bounds-checked, Q12). One-shot `inflate(data, max_output)` + streaming `Inflater` (Q6 — **plain value, НЕ consume**).
  - **RFC 1950 zlib:** header (CM=8, CINFO≤7, `(CMF·256+FLG)%31==0`, **FDICT=1→`UnsupportedMethod`**), DEFLATE-payload, **Adler-32**-trailer (NEW). `zlib_decode` + `ZlibReader`. Trailing-data strict → `TrailingData` (Q14).
  - **RFC 1952 gzip:** header (magic `1f 8b`, CM=8, FLG/MTIME/XFL/OS, опц. FEXTRA/FNAME/FCOMMENT/FHCRC — **header-field-длина капится**, Q10-flood), DEFLATE, **CRC-32 + ISIZE(mod 2^32)**-trailer (реюз crc32.nv). `gzip_decode` + `GzipReader`. **Multi-member** (Q10; иначе truncate-на-первом = silent-bug); **member-flood капится**.
  - **🔴 BOMB-CAP (§8.0, D334):** every decode-path принимает `max_output`; превышение output **ИЛИ прогресс-вход** → `Bomb(limit)` ДО аллокации сверх лимита (инкрементально, НЕ post-factum). Plan 178 → `max_decompressed`.
  - **🔴 STREAMING (§8.0, D335, Plan 178 BodyReader):** `Inflater`/`GzipReader`/`ZlibReader` — `feed`/`read(max_emit)`/`finish`; **bounded-per-call** (`read` ≤ max_emit — критик streaming-bound); 32 KiB window + bit-leftover между вызовами; `finish` валидирует checksum/ISIZE + clean-EOF; **EOF-семантика** (Q14: no-BFINAL=need-more; finish-before-BFINAL→`UnexpectedEof`).
  - **`CompressError`** (§3.1, D325/D333): wildcard-arm обязателен; `Checksum{kind,expected,got}`.
  - **PURE codec, NO effect** (§3.0 Q3): plain fallible fns — НЕ триада (§9). spec: D333/D334/D335/D336.
  - **pos:** RFC-vector decode (gzip/zlib/deflate известные вектора + `"123456789"`-corpus); round-trip (ref-сжатое → decode==original); **streaming feed-по-1-байту == one-shot** (байт-в-байт, произвольные chunk-границы incl. mid-symbol/mid-back-ref); **sync-flush-маркеры прозрачно глотаются** (DEFLATE-поток с interleaved пустыми stored-блоками `00 00 FF FF` == one-shot — реальные Go/Node-стриминг-серверы их шлют; interop-критично для 178 auto-decompress); multi-member gzip; stored/fixed/dynamic блоки; back-ref overlap (`dist<len` RLE); **single-distance-code** (Q13); CRC/Adler/ISIZE-verify pass; empty/single-byte edge; `max_output=0` escape-hatch (Q5); cap-граница (`output==cap` ok / `cap+1` → `Bomb`); **streaming bounded-per-call** (read(8) на распухающем входе). **neg:** **`inflate(data)` без `max_output` → compile-error** (neg/, D334 see-it-in-the-signature); **bomb→`Bomb`** (раздутие>cap, EXPECT-явно); **member-flood→`Bomb`/`InvalidData`**; **giant-FNAME→`InvalidData`**; truncated→`UnexpectedEof`; malformed Huffman (over-subscribed)→`InvalidData`; block-type `11`→`InvalidData`; bad-magic/`%31≠0`→`BadHeader`; distance>window→`InvalidData`; **checksum-mismatch→`Checksum`**; FDICT=1→`UnsupportedMethod`; **raw trailing-data→`TrailingData`**; **>4 GiB ISIZE-wrap→НЕ-Checksum** (slow pos). DEP: Ф.0.

- **Ф.2 ✅ LANDED (2026-07-06) — brotli decode (C-FFI). `[M-179-brotli-vendor-lib]` СНЯТ.** **Vendor:** google/brotli v1.2.0 декодер (commit `27cc9fe9`, BSD-2/MIT) собран из исходников однократно — `common/{constants,context,dictionary,platform,shared_dictionary,transform}.c` + `dec/{bit_reader,decode,huffman,prefix,state,static_init}.c`, MSVC x64 `/MT /O2` (как libuv) → `libbrotlidec.lib`. Вендорены **только заголовки + lib** (исходники в репо НЕ кладутся, стиль libuv): `compiler-codegen/nova_rt/brotli/include/brotli/{decode,types,port,shared_dictionary}.h` + `.../brotli/lib/libbrotlidec.lib` (tracked, `!`-негейт в `compiler-codegen/.gitignore`) + build-cache `target/brotli-cache/`. **Реализация:** `std/encoding/compress/{ffi.nv,brotli.nv}` — `brotli_decode(data, max_output)` через `extern "C"` (C-ABI без `[]u8`: `*u8`/`*mut u8`+`int`, модель fs) поверх шима `nova_rt/brotli_shim.{h,c}` (обёртка `BrotliDecoderDecompressStream`). **BOMB-CAP** инкрементально поверх FFI (per-pull budget `min(64 KiB, max_output−total+1)` → перебор ≤1 байта, память bounded; window ≤16 MiB lgwin-bounded — output-cap≠window-cap). **УСЛОВНАЯ ЛИНКОВКА** (owner-требование «линковаться только при использовании»): «модуль→библиотека» — brotli-lib в команду компоновки ТОЛЬКО когда генерённый `.c` содержит call-site к `brotli_decode` (`c_file_uses_brotli` в `test_runner.rs`, фильтрует decl/def-header — std-fn’ы эмитятся даже мёртвыми); шим-прототипы всегда в `nova_rt.h`; без vendor-lib → feature-gate-заглушки (`UnsupportedMethod`, Q11, НЕ link-error). **libuv (mandatory, всегда линкуется) НЕ тронут.** spec: D337 (аменд по факту). **Тесты (pass):** `std/encoding/compress/brotli_test.nv` + conformance `d337_brotli_ffi.nv` (RFC 7932 official-vectors 10x10y/64x/empty; bomb→`Bomb` по границе; truncated/corrupt→Err) + http `std/http/client/decompress_br_test.nv` (`Content-Encoding: br` → прозрачно распакован через мок). **conformance 54/0; условность доказана** (gzip-only CU → `uses_brotli=false` → lib НЕ линкуется; brotli/http → линкуется); zero-regression (baseline-delta 0). **Остаток:** streaming `BrotliReader` (**consume**, Q6) — deferred `[M-179-brotli-reader-streaming]` (C-примитивы шима `feed`/`pull` готовы; http использует one-shot; consume-neg-тест — вместе с ней). DEP: **Ф.1**, координация 174.6 M1 (E_FFI_NON_C_ABI_TYPE — grep=0). **Координация Plan 178:** `br`-ветка 178-decompress закрыта (`[M-178-autodecompress-br]`).

- **Ф.3 ✅ LANDED (2026-07-04) — encode: deflate + gzip + zlib (levels + streaming). Pure Nova.** См. «Ф.3 итог» вверху. **Реализовано полностью:** `deflate`/`gzip_encode`/`zlib_encode`(data, level) + streaming `Deflater`/`GzipWriter`/`ZlibWriter` (value, `@feed`/`@flush` SYNC-FLUSH/`@finish`); ≥3 различимых level-режима (stored/fixed/dynamic-Huffman); round-trip `inflate(deflate(x))==x` ВСЕ уровни; **external-oracle** (python zlib/gzip + `gzip -d`) подтвердил RFC-framing; per-codec level-range neg (`deflate(10/11)`→InvalidData). Encode БЕЗ bomb-cap (§3.5 — сигнатура строго `fn(data, level)`). conformance d333-encode зелёный; zero-regression. **Историческое описание (реализовано):** **Pure Nova** (precedent Go-writer/Zig). DEFLATE-compressor: LZ77-matcher (hash-chain/lazy) + Huffman (fixed для low / dynamic+code-length для high); **≥3 различимых level-режима** (Q7 honesty: stored/fixed/dynamic). `deflate`/`gzip_encode`/`zlib_encode`(data, level) + streaming `Deflater`/`GzipWriter`/`ZlibWriter` (value, НЕ consume) + **`@flush` SYNC-FLUSH** (byte-align + пустой stored-блок; SSE/chunked поверх gzip — D335; прецеденты Go `Writer.Flush`/Node `Z_SYNC_FLUSH`/Java/Zig). brotli-encode — followup §11 (asymmetric). spec: D333 §encode. **pos:** round-trip (Nova-encode → **Ф.1 inflate** == original, ВСЕ levels); level-0 stored; streaming-encode-chunked; level-различимость (best ratio < fastest size); **prefix-decodable после `@flush`** (накопленный выход декодится в ровно скормленный вход); brotli level 11 валиден. **neg:** invalid-level (`new(12)`)→`InvalidData`; **per-codec range: `deflate(level=10)` → `InvalidData`**. DEP: **Ф.1** (round-trip-gate), Ф.0.

- **Ф.6 (номер исторический — Ф.4/Ф.5 не существуют, артефакт схемы 178) — тесты+docs+polish. «сейчас» (decode-часть; encode/brotli после Ф.3/Ф.2).** §7 pos+neg полный; **🔴 external-oracle round-trip** (Nova-encode декодится эталоном gzip/zlib-CLI-фикстурой И ref-encode декодится Nova — критик-gap анти-circular); D333–D337 финал; `docs/encoding-compress.md` (RFC 1950/1951/1952/7932-модель + cross-lang §2 + bomb-cap/streaming-контракт + §1a + **bomb-cap-security-раздел**); **`*_slow.nv`** — large-file decode/encode (multi-MiB, real RFC-corpus, **streaming-память-bounded** — feed 32 KB→900 MB под 1 GB cap, резидентная память доказанно ограничена). DEP: all (encode/brotli-блоки gated Ф.3/Ф.2).

---

## 5. Spec / D / Q / docs

**D-номера (Ред.2 2026-07-03):** D325=Result-everywhere (**177**) ✅ committed, D326=ref-param-mode (172.5) ✅, D327=Codepoint (172.2) ✅, D328=value-record-== (172.4, inline) ✅ → у Plan 178 коллизия D327/D328 → **178 renumber в D357–D362** (его Ф.0); compress старт **D333** (свободны, grep=0). D316–D324 — резервы 175/175.1/176; **reservation-нота D333–D339** в индексе (как Plan 177, Ф.0).

- **NEW D333** — **codec-контракт `std/encoding/compress`** (внесён в `spec/decisions/04-effects.md` — класс-соседи D322/D323/D325; `05-stdlib.md` был аспирацией, не создан): PURE-codec (НЕ effect — plain fallible fns над `[]u8` + coder-value; §9 — явное conventions-исключение «PURE codec/serde need NO effect»); byte-first (`[]u8` вход/выход, `str` НЕ участвует); one-shot `inflate`/`zlib_decode`/`gzip_decode`/`brotli_decode` + encode-аналоги; `CompressError` структурный + OPEN `ErrorKind` (R5/D325, wildcard обязателен; `Checksum{kind,expected,got}`); D325-нейминг (R1 fallible→`Result[T,CompressError]`; R2 bare `inflate`/`deflate`/`gzip_decode`; R3 `try_`-нет; R4 `Option`=genuine absence — streaming-EOF `Option[[]u8]`); целочисленность (`int` размеры, `u32` checksums/ISIZE, ISIZE mod 2^32, bounds-checked bit-reader); incomplete-Huffman-exception (Q13); trailing-data-policy (Q14).
- **NEW D334** — **bomb-cap (DoS-инвариант)**: каждый decode-путь (one-shot + streaming + brotli-FFI) несёт `max_output`; превышение output **ИЛИ прогресс-вход** (anti-flood: member-flood/giant-header) → `Bomb(limit)`, проверка **инкрементальная** (ДО over-аллокации); **граница: `output == max_output` → ok, первый байт сверх → `Bomb`**; Plan 178 пробрасывает `max_decompressed` (100 MiB). **§8.0-critical.**
- **NEW D335** — **streaming incremental coder**: `feed(chunk)` + `read(max_emit) -> Result[Option[[]u8],CompressError]` (`None`=end, **bounded-per-call** — anti-single-huge-alloc) + `finish`; window/bit-leftover/checksum-state между вызовами; **EOF-семантика** (no-BFINAL=need-more; finish-before-BFINAL→`UnexpectedEof`); **must-consume только `BrotliReader`** (C-instance, D133); pure-Nova-кодеры = plain value (Q6). **Encode-`@flush` = SYNC-FLUSH** (byte-align + пустой stored-блок `00 00 FF FF`; после flush накопленный выход — decodable-префикс: SSE/chunked). **Multi-member-семантика:** `is_done`/`read→None` = конец ВСЕХ членов; граница члена не наблюдаема в V1 (single-member opt-out — followup §11). **Контракт-мост с Plan 178 `BodyReader`** (`@next_chunk`→`feed`→`read`) фиксируется здесь.
- **NEW D336** — **checksum-контракт** (CRC-32/Adler-32/ISIZE): промоут `crc32.nv` (free-function-форма) `_experimental/checksums/` → `std/encoding/compress/checksum.nv`; Adler-32 NEW (mod 65521); trailer-verify обязателен, mismatch→`Checksum{kind,expected,got}`; **ISIZE = `(uncompressed_len mod 2^32) == ISIZE`** (НЕ raw).
- **NEW D337** — **brotli C-FFI-контракт** (Ф.2): C-FFI-необходимость (pure-Nova=followup; rationale §3.0 — 120 KB словарь, Zig-std-нет); `libbrotlidec` в `ffi.nv`; streaming-decode поверх C-instance; `BrotliReader` consume (release C-instance); **bomb-cap-over-FFI** (output-cap инкрементально; window≤16 MiB lgwin-bounded, output-cap≠window-cap); error-code-маппинг → `CompressError`.
- **docs/* (новые):** `docs/encoding-compress.md` (RFC-модель + cross-lang + bomb-cap/streaming + §1a + security-раздел). Опц. `docs/dev/idioms/compress.md` (one-shot vs streaming, выбор level).

---

## 6. Миграция

Аддитивно (`std/encoding/compress/*` — folder=один модуль `module std.encoding.compress`, рядом с `json`/`base64`/`utf16`). Раскладка: `inflate.nv` (bit-reader + Huffman + `Inflater` + one-shot decode), `deflate.nv` (encode, Ф.3), `gzip.nv` (framing), `zlib.nv` (framing), `checksum.nv` (CRC-32 + Adler-32), `error.nv` (`CompressError`), `brotli.nv` + `ffi.nv` (Ф.2), `mod.nv` (re-export).

**CRC-32 промоут (Ф.0/D336):** `std/_experimental/checksums/crc32.nv` (module `checksums.crc32`, free-functions, `0xCBF43926` PASS) → промоут в `std/encoding/compress/checksum.nv` (`module std.encoding.compress`, **free-function-форма as-is** — Q15, меньше churn, тесты переносятся). После промоута — Grep импортёров `checksums.crc32`, обновить отдельным коммитом, удалить/`re-export-shim` `_experimental`-версию. **Adler-32 — NEW** в `checksum.nv` (в дереве нет).

**Brotli C-lib vendor (Ф.2, §6):** vendoring `google/brotli` (BSD-2/MIT) — `libbrotlidec` + headers под `std/encoding/compress/vendor/brotli/` (как libuv в net): минимальный decode-набор (`dec/` + `common/` + 120 KB `common/dictionary.c`); НЕ тащить `enc/` в V1 (encode=followup). Статическая сборка (как `libz3.lib`/libuv); build-glue в `build.rs`/CMake-shim. **Ф.0 verify:** реальный vendor-link-путь существует (simplification-audit — механизм не подтверждён). Build-gate: Ф.2 не стартует пока vendor не интегрирован; vendor-коммит отдельный, ПЕРЕД Ф.2-кодом.

**Plan 178-reconcile — ✅ ВЫПОЛНЕН (Ред.2 2026-07-03):** «C-zlib FFI» в 178 отсутствует (grep=0); 178 говорит «inflate/gzip/zlib = pure-Nova; C-FFI только brotli» и указывает decompress-gate = **Plan 179 Ф.1 (gzip/deflate) + Ф.2 (br)**. Остаток Ф.0 — re-verify grep «C-zlib»=0 + актуальность указателей при дрейфе.

**Зависимость-направление (§3.4):** **compress НЕ импортирует `std.http`** (избежать цикла); Plan 178 импортирует compress. Glue (`BodyReader`+decoder) живёт в `real_http` Plan 178.

**`_experimental` cleanup:** после промоута crc32 — пересобрать `nova-cli` (`include_str!` после правок `.nv`); верификация против чистого бинаря.

---

## 7. Тесты (pos + neg; `nova_tests/compress/` — тема, НЕ per-plan папка)

Раскладка по test-conventions Ред.2 (тема compress, как 175/176 §7): pos = folder-module (`module nova_tests.compress`), обычные test-блоки; **Err-Result-проверки (Bomb/Checksum/UnexpectedEof/…) = ПОЗИТИВНЫЕ test-блоки с assert на Err-kind** (НЕ neg/); `rt/` = standalone `fn main` только для `EXPECT_STDERR`/`EXPECT_RUNTIME_PANIC`-фикстур; `neg/` = **ТОЛЬКО** `EXPECT_COMPILE_ERROR` (`module neg.<name>`, маркер `// EXPECT_COMPILE_ERROR <substr>` **без двоеточия**, один маркер/файл). **PURE-codec → НЕ mock-тест** (нет effect/I/O; §9 — mock-mandatory НЕ применяется). slow/large-file = `*_slow.nv` (D277/D298: opt-in `--include-slow`, коммитятся).

**spec_tests/conformance — ОБЯЗАТЕЛЬНОЕ D-покрытие (Ред.2):** `spec_tests/conformance/d333_compress_codec.nv` · `d334_bomb_cap.nv` · `d335_streaming_coder.nv` · `d336_checksum.nv` · `d337_brotli_ffi.nv` (gated Ф.2) + compile-error-сторона в `spec_tests/conformance/neg/` (`BrotliReader`-не-consume, `CompressError`-match без wildcard, `inflate` без `max_output`); `module spec_tests.conformance`, локалы с префиксом `d333_…`; амендов нет (D333–D337 все NEW — при future-amend обновлять СУЩЕСТВУЮЩИЙ d-файл); прогон `nova test spec_tests` отдельной командой.

**pos / контрактные:**
- **RFC test-vectors (decode):** известные gzip/zlib/raw-DEFLATE вектора → ожидаемый plaintext (RFC 1951/1952 + `"123456789"`); brotli RFC 7932-вектора (Ф.2).
- **round-trip:** ref-сжатые фикстуры (от эталонного gzip/zlib) → decode==original; (Ф.3) Nova-encode → Nova-inflate==original ВСЕ levels.
- **🔴 external-oracle (Ф.6, анти-circular):** Nova-encoded декодится **внешним** gzip/zlib-CLI-фикстурой; ref-encoded декодится Nova-inflate. (§8.3-gate — self-round-trip недостаточен.)
- **block-types:** stored(`00`)/fixed(`01`)/dynamic(`10`); back-ref overlap (`dist<len` RLE `aaaa`); max-distance (32 KiB) + max-length (258); **single-distance-code** (Q13, interop).
- **streaming feed-по-1-байту == one-shot:** произвольные chunk-границы (1-byte, mid-symbol, mid-back-ref) → output **байт-в-байт** == one-shot; **bounded-per-call** (`read(8)` на распухающем входе → ≤8 байт/вызов).
- **gzip multi-member:** concatenated `.gz` → конкатенация (НЕ truncate); gzip header-flags FEXTRA/FNAME/FCOMMENT/FHCRC корректно пропускаются.
- **checksum-verify (pos):** CRC-32/Adler-32/ISIZE pass на валидных; `crc32("123456789")==0xCBF43926`; adler32 RFC-vector.
- **edge:** empty-input; single-byte; incompressible (stored-fallback на encode).
- **sync-flush-прозрачность (decode, Ф.1):** DEFLATE-поток с interleaved пустыми stored-блоками (`00 00 FF FF`, Z_SYNC_FLUSH от Go/Node-серверов) == one-shot байт-в-байт.
- **cap-граница + escape-hatch:** `output == max_output` → ok; `max_output+1` → `Bomb`; `max_output=0` = без лимита (Q5).
- **prefix-decodable после `@flush` (Ф.3):** накопленный выход после sync-flush декодится в ровно скормленный вход (SSE/chunked-инвариант).

**neg (`EXPECT_COMPILE_ERROR`):**
- **`BrotliReader` не consume** (Ф.2, D133/D335 — **единственный** consume-кодер); double-consume; use-after-consume; `CompressError`-match без wildcard (OPEN-kind); **`inflate(data)` без `max_output`** → compile-error (D334 see-it-in-the-signature). *(pure-Nova `Inflater`/`GzipReader` — plain value, НЕ consume → у них НЕТ «не-consume»-neg-теста, критик-gap resolved.)*

**Err-Result-проверки (ПОЗИТИВНЫЕ test-блоки с assert на Err-kind; `EXPECT_STDERR`-варианты → `rt/`):**
- **🔴 BOMB → `Bomb`:** малый-вход→раздутие>cap (`aaaa`-DEFLATE / brotli-bomb) → `Bomb(limit)` (**§8.0-critical, EXPECT-явно**, НЕ OOM/hang).
- **🔴 anti-flood → `Bomb`/`InvalidData`:** 100k пустых gzip-членов → bounded (не hang); 4 GB `FNAME`-поле → `InvalidData` (не unbounded-skip).
- **truncated → `UnexpectedEof`** (обрезанный DEFLATE / trailer); **finish-before-BFINAL → `UnexpectedEof`** (streaming).
- **malformed → `InvalidData`:** block-type `11`; over-subscribed Huffman; distance>window; back-ref за границу.
- **header → `BadHeader`:** bad gzip-magic; zlib `%31≠0`.
- **checksum-mismatch → `Checksum`:** флипнутый CRC-32/Adler-32/ISIZE-байт → `Checksum{kind,expected,got}`.
- **unsupported → `UnsupportedMethod`:** zlib FDICT=1; CM≠8.
- **level-range per-codec (Ф.3):** `deflate(data, CompressLevel.new(10)!!)` → `InvalidData` (brotli 10..11 — pos).
- **trailing → `TrailingData`:** мусор после BFINAL (raw/zlib strict; gzip lenient — pos-вариант).
- **brotli (Ф.2):** truncated→`UnexpectedEof`; malformed→`InvalidData`; bomb→`Bomb` (cap поверх C); max-lgwin+bomb→`Bomb` не OOM.

**slow/large (`*_slow.nv`, opt-in):** multi-MiB real-file decode (RFC-corpus / реальный `.gz`); large round-trip ВСЕ levels; **🔴 streaming-память-bounded** (feed 32 KB inflating→900 MB под 1 GB cap → резидентная память доказанно ограничена — критик streaming-bound); **>4 GiB ISIZE-wrap** (НЕ-Checksum, Q12); brotli large-file (Ф.2).

---

## 8. Критерии приёмки

0. **🔴 ОБЯЗАТЕЛЬНО: «без упрощений, как для прода».** Ни одного «решим потом» на критпути (DEFLATE-инфлейт-корректность incl. dynamic-Huffman + back-ref-overlap + 32 KiB-window + incomplete-distance-code; **bomb-cap — НЕ опционально, капит И вход**; **streaming incremental + bounded-per-call — required**, НЕ только one-shot; checksum-verify обязателен incl. ISIZE-mod-2^32; multi-member gzip + flood-cap) — на КАЖДОЙ приземлённой фазе. Каждая behavior-change — **pos+neg + аргумент звучности**; RFC-корпус НЕ заменяет edge-тесты (block-types/overlap/chunk-границы/truncation/flood). **Гейт корректности (Ред.2):** `spec_tests/conformance` зелёный (d333–d337) + pos/neg-фикстуры фазы + **nova_tests baseline-delta = 0** (baseline = parent-коммит через temp-worktree/commit+reset, ТОТ ЖЕ бинарь; nova_tests сам по себе НЕ гейт корректности; флака ≠ регрессия; батчами <10мин). **Явно gated, НЕ «решим потом»:** brotli-decode (Ф.2, libbrotli vendor — Ф.0 verify link), encode (Ф.3) — исключены из landed-acceptance Ф.1; inflate/gzip/zlib-decode самодостаточен и **закрывает Plan 178 Q12 для gzip/deflate сейчас**. **Явно scoped-out с rationale (НЕ violation):** zstd/lz4/lzma, pure-Nova-brotli, brotli-encode, zlib-preset-dictionary, optimal-parse-level-9, comptime-codec-tables (§11).
1. **inflate decode (Ф.1):** raw-DEFLATE (stored/fixed/dynamic + back-ref-overlap + 32 KiB-window + single-distance-code) + zlib (RFC 1950 + Adler-32 + FDICT-reject) + gzip (RFC 1952 + CRC-32 + ISIZE-mod-2^32 + multi-member + header-flags + flood-cap); RFC-vector pass; round-trip-decode pass. **🔴 bomb-cap:** раздутие>cap И flood-вход → `Bomb` (инкрементально, НЕ OOM/hang). **🔴 streaming:** `feed`/`read(max_emit)`/`finish` incremental, chunked==one-shot байт-в-байт, **bounded-per-call**, EOF-семантика; pure-Nova-кодеры **plain value** (НЕ consume); **интегрируется с Plan 178 BodyReader.** **checksum:** mismatch→`Checksum{kind,exp,got}`. `CompressError` OPEN-kind. **sync-flush-маркеры в decode** (interleaved пустые stored-блоки == one-shot). **UNBLOCKS Plan 178 Q12 (gzip/deflate).**
2. **brotli decode (Ф.2) — gated libbrotli vendor:** RFC 7932-vector; streaming; bomb-cap-over-FFI (window-bound задокументирован); `BrotliReader` **consume** (release C-instance, единственный consume-кодер); error-маппинг; honest-gate (vendor не интегрирован / Ф.0-link-verify провален → Ф.2 не стартует, явно). Закрывает `br` Plan 178.
3. **encode (Ф.3):** `deflate`/`gzip_encode`/`zlib_encode` + **≥3 различимых level-режима** (honesty) + streaming `Deflater`/`GzipWriter` (value); round-trip через собственный inflate (ВСЕ levels) + **🔴 external-oracle cross-check** (Nova-output декодится эталоном — НЕ self-circular); stored-fallback; **`@flush` SYNC-FLUSH** (Deflater/GzipWriter/ZlibWriter) + prefix-decodable pos; per-codec level-валидация (`deflate(level>9)` → `InvalidData` neg).
4. **checksum (Ф.1):** CRC-32 (промоут crc32.nv) + Adler-32 (NEW) — test-vector pass; trailer-verify enforced; ISIZE-mod-2^32.
5. **byte-first + PURE-codec:** `[]u8` вход/выход; `str` НЕ участвует; НЕ effect (нет триады — §9); D325-нейминг (bare `inflate`/`deflate`/`gzip_decode`); целочисленность (`int` размеры, `u32` checksums).
6. **spec:** D333–D337 в `spec/decisions/` (+ reservation-нота D333–D339); **`spec_tests/conformance` d333–d337 зелёные**; docs `encoding-compress.md`; §1a/§2-ячейки фактчекнуты (Swift COMPRESSION_ZLIB-misnomer/silent-truncate, Zig levels 4..9/`window_size_max`); D335 несёт flush- и multi-member-семантику; **Plan 178 reconcile re-verified** (grep «C-zlib»=0).
7. Большие/slow вне дефолт-сэмпла (`*_slow.nv`); **streaming доказанно память-bounded** (slow-тест).

---

## 9. Конвенции + координация

**Конвенции (refs):** module-conventions (folder=один модуль `std.encoding.compress`; **PURE codec без I/O → НЕ триада, НЕ effect** — plain fallible fns над `[]u8`/coder-value, как `json`/`base64`/`utf16` — **явное conventions-исключение для pure-codec**, зафиксировано D333; **НЕ требовать mock-тест**; scalars=value-record D215; **resources=must-consume D133 — ТОЛЬКО `BrotliReader`** (C-instance release-долг); pure-Nova-кодеры = plain value (нет долга у GC-окна); **byte-first** `[]u8`; FFI в `ffi.nv` extern "C" (только brotli Ф.2); **inflate/gzip/zlib/deflate в .nv не C — nv-sourcing-максимум**, brotli — C-FFI by-necessity). **D325** (R1→`Result[T,CompressError]`; R2 bare; R3 `try_`-нет; R4 `Option`=genuine absence — streaming-EOF; R5 `Fail[E]` запрещён → структурный `CompressError`). **consume** D131/D133/D180 (только `BrotliReader`). **test-conventions** (EXPECT_*; pos folder-module / neg `neg/`; **mock НЕ-mandatory — нет effect**). conventions-governance: изменения только по согласованию.

**🔴 GATE-релевантность (Plan 178):**
- **Plan 178 Q12 (auto-decompress) — 🔴 HARD-GATE на Plan 179 Ф.1.** Ф.1 (gzip/deflate/zlib decode + bomb-cap + streaming-`BodyReader`) **открывает gate для gzip/deflate сейчас**; `br` gated на Ф.2. Plan 178 → `max_decompressed`→`max_output` (D334). **Streaming-контракт-мост** (`BodyReader.@next_chunk` ↔ `feed`/`read`) — D335; verify против Plan 178 §3.2/§3.5.
- **Reconcile — ✅ выполнен** (Ред.2 2026-07-03; §6): 178 уже pure-Nova; Ф.0 re-verify grep «C-zlib»=0.

**Координировать:**
- **Plan 178 (std/http)** — потребитель Ф.1 (gzip/deflate) + Ф.2 (br); reconcile ✅ выполнен (Ред.2 2026-07-03; Ф.0 re-verify). Dispatch по `Content-Encoding` и `ErrSource.Compress(CompressError)` — задачи 178 Ф.2 (зафиксированы там).
- **Plan 177 (D325)** — Result-everywhere (conformant by-construction).
- **Plan 176 (io/fs)** — `copy_to(w mut impl io.Write)` (followup §11, decode-to-file/encode-from-file) гейтит на `io.Write`; large-file-slow-тесты читают fixture-файлы (fs).
- **`_experimental/checksums/crc32.nv`** — промоут (D336/Q15); owner-sign-off при stable-промоуте.
- **`std/encoding/json`** — Plan 178 `json()` отдельный serde-gate (НЕ этот план); layout-сосед.
- **Plan 174.6 (C-ABI FFI)** — Ф.2 `ffi.nv` обязан проходить `E_FFI_NON_C_ABI_TYPE` (M1): extern-сигнатуры без `[]u8`/GC-типов — raw-ptr+len/out-буфер (§3.6); в std нет ни одного extern с `[]u8` (grep=0).

**⚠ Конвенции/std, заметки (owner-sign-off, conventions-governance):**
- **PURE-codec-исключение из effect-триады** — конвенция сама говорит «PURE codec/serde need NO effect» → **D333 = явное исключение, НЕ violation**.
- **brotli C-FFI by-necessity** — nv-sourcing требует .nv где feasible; brotli=heavy native → C-FFI (как net/libuv). **Necessity-аргумент D337**; pure-Nova-brotli=followup §11.
- **crc32 промоут** `_experimental`→stable — ✅ owner-sign-off получен 2026-07-03 (реюз as-is free-function-формы, Q15).

После большой задачи — обновить `project-creation.txt` + `nova-private/discussion-log.md` + `simplifications.md`.

---

## 10. Фоновые агенты

- **НЕ `git stash`** (worktree делят `.git` → repo-global коллизия/потеря); baseline — **temp-worktree** (`git worktree add ../nova-179-base <parent>`) / commit+reset, **ТОТ ЖЕ бинарь**. Постоянный worktree `nova-p179` (naming `nova-pNN`) первой командой, самозарегистрироваться; cwd сбрасывается/дрейфует → **префикс абсолютным путём в каждой команде**; ссылки на файлы worktree — полный абсолютный путь. Env: `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main-repo; libuv-submodule скопировать без его `.git`.
- **Rate-limit-устойчивость (фазы resumable/идемпотентны):** коммит после каждой фазы, без amend; малые батчи; `agent()`-null-tolerant — фильтровать выживших, ре-ран упавших; `git add` только конкретные файлы (никогда `-A`/`.`); `git diff --cached --stat` перед commit; **DCO `git commit -s`** (CI-гейт); без `Co-Authored-By`; после фазы — **bidirectional sync с main**. Подтверждение перед background-`Agent`.
- **Тесты:** `nova test` требует **ЯВНЫЙ путь**; батч-канон: `nova test nova_tests/<dirs> --results-file rN.json` батчами <10мин + хвост `--rerun-failed`; **ОТДЕЛЬНО** `nova test spec_tests` и `nova test std`. Гейт = §8.0 (spec_tests d333–d337 + targeted pos+neg soundness: RFC-vector/round-trip/bomb/flood/streaming/checksum/external-oracle + baseline-delta=0; nova_tests сам по себе НЕ гейт; флака ≠ регрессия). **`*_slow.nv` (large-file/streaming-bound/brotli-stress) вне дефолт-сэмпла.** **Пересобрать `nova-cli` после правок `.nv`** (`include_str!`); **mtime-touch `.rs` перед `cargo build`** (stale-build; Ф.2 FFI-glue). **Ф.2 vendor:** libbrotli-сборка (статика, как libz3/libuv) — отдельный vendor-коммит ПЕРЕД Ф.2-кодом; **Ф.0 verify реальный build-link-путь**. Не выдумывать синтаксис — `spec/decisions/`+`examples/` (bit-ops/`[]u8`-slice/`consume value` — verify против `vec_seq.nv`/`crc32.nv`/`Body` 178).

---

## 11. Followup

`[M-179-std-compress]`. **Под-планы при росте:** **179.1** (encode-suite + optimal-parse), **179.2** (pure-Nova-brotli). Deferred (с rationale, НЕ на критпути inflate+gzip+zlib-decode = 178-gate):
- **zstd decompress + encode** (RFC 8878; FSE+Huffman+dictionaries — большой codec, отдельный план). Zig `std.compress.zstd` = pure-decode с `window_size_max` (дефолт 8 MiB) + `verify_checksum` — прямой nv-sourcing-порт-ориентир И прецедент cap-параметра в сигнатуре (валидирует D334-подход); Node 22+ капит `maxOutputLength`; `Content-Encoding: zstd` растёт (Chrome 2024+) → **приоритет ВЫШЕ lz4/lzma**, но НЕ гейт 178.
- **brotli encode** (asymmetric: Ф.2 decode-only; encode = `libbrotlienc` vendor / pure-Nova — server-side `Content-Encoding`).
- **pure-Nova-brotli** (убрать C-FFI: порт 120 KB словаря + context-modeling в .nv — nv-sourcing-чистота, замена Ф.2).
- **lz4 / lzma(xz):** lz4 = простой block-codec, pure-Nova-кандидат — таргетим **стандартный lz4-frame** (⚠ Swift `COMPRESSION_LZ4` = Apple-собственный block-framing magic `bv4x`, НЕ стандарт — interop-ловушка); lzma/xz: Zig std несёт **pure xz/lzma-decode** → pure-Nova-decode feasible (encode — C-FFI/skip).
- **zlib preset-dictionary** (FDICT=1 — V1 reject как `UnsupportedMethod`; redis/protobuf-dict use-case; peers: Go `NewReaderDict`/Rust `set_dictionary`/Node `options.dictionary`/Java/Python `zdict=` — все имеют, Zig/Swift нет). Pure-Nova делает raw-dict **тривиальным** (preload 32 KiB окна); аддитивная форма зафиксирована: `Inflater.new_with_dict(dict []u8, max_output)` / `zlib_decode_dict(...)` — followup НЕ ломает API; gzip словаря не имеет by-format.
- **gzip single-member режим** (`GzipReader`-option; Go `Multistream(false)`-прецедент) — record-ориентированные форматы (BGZF, `.warc.gz`) с доступом по членам.
- **optimal-parse encode level-9** (V1 = dynamic-Huffman+lazy «good-enough» < zlib-9 ratio; optimal-parse когда бенч покажет нужду).
- **comptime codec-tables** (DEFLATE Huffman/length/dist + CRC/Adler сейчас runtime-lazy как crc32.nv `table_value`; comptime-const-array когда язык даст — perf).
- **value-type `Crc32`/`Adler32`-обёртки** (V1 = free-functions реюз; объектная инкрементальная форма — sugar followup).
- **`copy_to`/streaming-to-file** (decode-to-`io.Write` / encode-from-`io.Read` — fs-gate Plan 176; download-decompress без буферизации-всего).
- **gzip encode header-config** (FNAME/MTIME/OS при encode — сейчас минимальный header 1f8b0800…00ff, OS=unknown).
- **cross-block streaming-encode matches** (Ф.3 V1: `Deflater.@flush`/auto-flush компилирует каждый сегмент как self-contained блок — back-ref только внутри своего сегмента, НЕ через границу flush'а; корректно (декодер-окно непрерывно), но ratio на границах хуже непрерывного. Followup:持続 LZ77-окно через flush-границы). One-shot `deflate` — один непрерывный блок (без этого ограничения).
- **`[M-179-brotli-vendor-lib]` — ✅ ЗАКРЫТ (2026-07-06):** `libbrotlidec` вендорен (google/brotli v1.2.0 decoder, headers+lib без исходников, MSVC `/MT /O2`) — Ф.2 приземлена. Linux/macOS `.a` не вендорен (Windows-хост) → на них Q11-заглушки (`UnsupportedMethod`); `.a`-vendor при появлении такого хоста.
- **`[M-179-brotli-reader-streaming]` — streaming `BrotliReader`** (`consume value`, D133/D337 R2 — единственный consume-кодер). C-примитивы шима (`nova_brotli_dec_feed`/`pull` инкрементальные) уже готовы; остаётся тонкая Nova-обёртка + consume-neg-тесты (`EXPECT_COMPILE_ERROR`). Отложено сознательно: Ф.2-deliverable = one-shot `brotli_decode`; единственный потребитель `br` (http auto-decompress) использует one-shot, как gzip/deflate-ветки.
- **SIMD/hardware-CRC** (CRC-32 via PCLMULQDQ/CRC32-instr — perf).

Имена/детали — финал при реализации (после Ф.0).