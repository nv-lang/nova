<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 206 — свип «длинного хвоста» intentional-overflow кода (2026-07-16)

**Ветка:** `p206-overflow-tail-sweep` (worktree `d:/Sources/nv-lang/nova-p206sweep`, база `main`
@ `2977e6580`).

**Триггер:** owner поймал runtime-crash под chaos-режимом — `splitmix64_step` в
`examples/flagship/aggregator/src/app/scenarios.nv` трапал `integer overflow: *` (уже
починено owner'ом отдельно, коммит `1581bc407`, ДО начала этого свипа). Значит round-2
grep-аудит Plan 206 (см. `docs/plans/206-progress.md` строки 87-95) мог пропустить что-то
ещё — этот свип ищет превентивно.

## Итог: миграций НЕ потребовалось (0 файлов, 0 коммитов кода)

Полный обход `std/**` + `examples/**` (+ read-only проверка `nova-tls/src`, `nova-http/src`
как отдельных репо) по паттернам `splitmix|xoshiro|xorshift|pcg|lcg|mulberry|wyhash|murmur|
fnv|siphash|checksum|crc|adler|hash|digest|mix|seed|rand|prng` + отдельный grep на голые
`* 0xHEXCONST8+` / `+ 0xHEXCONST8+` (магические PRNG/hash-константы без `.wrapping_*`) не
нашёл ни одного немигрированного intentional-overflow сайта.

**Все найденные PRNG/hash/checksum места уже мигрированы предыдущими волнами:**
- `std/src/checksums/fnv.nv` — `.wrapping_mul` (FNV-1a), помечено `Plan 206 Ф.2 (D423)`.
- `std/src/collections/bloom_filter.nv` — `hash1`/`hash2` + double-hashing в `@insert`/
  `@contains` — `.wrapping_add`/`.wrapping_mul`.
- `std/src/collections/vec/protocols.nv` — `Vec[T Hash] @hash()` (FNV-1a) — `.wrapping_mul`.
- `std/src/crypto/md5.nv` / `sha1.nv` / `sha256.nv` — компрессия-функция и `total_len` —
  `.wrapping_add`/`.wrapping_mul` (mod-2^32/2^64 по RFC/FIPS).
- `std/src/testing/handlers.nv::seeded` (xoshiro256++/splitmix64 PRNG) — `.wrapping_add`/
  `.wrapping_mul`.
- `spec_tests/conformance/inline_xoshiro_determinism.nv` — inline-репродьюсер, тоже wrapping.
- `examples/flagship/aggregator/src/app/scenarios.nv::splitmix64_step` — мигрировано
  owner'ом (коммит `1581bc407`) ДО этого свипа.

**Проверено и корректно НЕ мигрировано (голый `+`/`-`/`*` там либо не переполняется на
практике, либо переполнение — genuine misuse, которое ДОЛЖНО трапать):**
- `std/src/checksums/adler32.nv::adler32_update` — `a = (a + byte) % m`, `m = 65521`;
  аккумулятор всегда `< 65521 + 255`, никогда не подходит к `u32::MAX`. Не intentional
  overflow — просто маленькие числа.
- `std/src/checksums/crc32.nv` — только XOR/shift (`^`, `>>`), `+`/`-`/`*` вообще не
  использует — тем более `wrapping_*` не нужен.
- `std/src/identifiers/uuid.nv:322` (`acc = acc * 16 + d`, hex-parse) — bounded (макс.
  8 hex-цифр на вызов `parse_hex_range` = 32 бита), никогда не переполняет `u64`. Тот же
  вывод уже был у Plan 206 round-2-аудита (`206-progress.md:89-91`).
- `std/src/identifiers/uuid_namespace.nv` — только bitwise (`&`, `|`, `<<`, `>>`), без
  overflow-арифметики.
- `std/src/encoding/compress/{deflate,gzip,zlib,checksum}.nv` — `crc`/`isize`/`adler`
  трейлеры собираются через `|`/`<<`/`&` (bitwise), не через `+`/`-`/`*`; единственный
  голый `*`/`+` — `prng_bytes` test-helper в `deflate.nv` (классический LCG,
  `s * 1103515245 + 12345`), но `int` = `intptr_t` = 64-бит на текущей платформе, а
  `s` после `& 0x7FFFFFFF` всегда ≤31 бит → произведение ≤~62 бит, НЕ переполняет 64-битный
  `int` — уже безопасно без wrapping (арифметически проверено, не полагается на удачу).
- `std/src/encoding/hex.nv`, `std/src/encoding/json.nv:505`, `std/src/encoding/url.nv:374`
  — hex-digit парсинг (`code = code*16 + digit`, `hi*16+lo`), всегда bounded малым числом
  цифр (2-4) — никогда не переполняет.
- `std/src/crypto/hmac.nv`, `jwt.nv`, `_experimental/crypto/insecure_demo_kdf.nv` —
  только str-конкатенация / bitwise base64-упаковка / `1 << cost` (shift, не арифметика) —
  ничего overflow-склонного.
- `std/src/collections/hashmap.nv` — `used*4 < buckets.len()*3` (load-factor), `x+1`/`n-1`
  (pow2 rounding) — размерные вычисления над реальными count'ами, НЕ модульная арифметика
  по спецификации; overflow здесь означал бы genuine bug (переполнение реальной ёмкости),
  поэтому корректно оставлено trap-able.
- `std/src/collections/lru.nv` — только индексация, арифметики нет.
- `std/src/math/overflow_policy_test.nv` — тесты САМИХ `checked_*`/`wrapping_*`/
  `saturating_*` бланкетов (D423 Ф.2), не подлежит миграции (это и есть источник
  wrapping-API).
- Unicode (`collate.nv`/`normalize.nv`/`cp_utils.nv`), `bench.nv`, `encoding/{ini,toml,
  serde}.nv` — только `HashMap`-контейнер по имени, никакой ручной hash-mixing арифметики.
- `nova-tls/src/*.nv` — нет hash/PRNG-кода вообще (TLS-конфиг/FFI-обёртка).
- `nova-http/src/error.nv` — слово «checksum» только в doc-комментарии, не в коде.
- `examples/real_world/orm_demo.nv`, `examples/_wip/effect_density/*.nv`,
  `examples/flagship/aggregator/src/app/aggregate_test.nv` — `seed`/`hash` только как
  доменные идентификаторы (DB seed-данные, HashMap-хранилище), не PRNG-арифметика.

## Верификация

Точечная компиляция/тесты НЕ потребовались — ни один файл не был изменён (0 diff).
Компилятор (`nova.exe`, main-бинарь) не пересобирался и не запускался.

## Оценка полноты

Grep-паттерн (алгоритмические имена + голые hex-магические константы ≥4 hex-цифр без
`.wrapping_*`) покрывает ВСЕ известные классы intentional-overflow кода в проекте (PRNG-
степперы, hash-миксеры, checksum-аккумуляторы). Дополнительно перепроверено отдельным
grep на completely bare `* 0xHEXCONST8+` / `+ 0xHEXCONST8+` по всему `std/**` и
`examples/**` — 0 совпадений, т.е. ни одной немигрированной магической hash/PRNG-константы
не осталось. Результат сходится с независимым round-2-аудитом самого Plan 206
(`206-progress.md` строки 87-95) — те же файлы (uuid.nv, parse.nv, civil/*) признаны
безопасными по тем же причинам.

**Остаточный риск (честно):** grep не ловит intentional-overflow код, который НЕ содержит
ни одного из ключевых слов И не использует магическую hex-константу (например, гипотетический
чистый byte-accumulator без «говорящего» имени функции/константы). Ручной построчный обзор
всех файлов с явной арифметикой (`+`/`-`/`*` на `[iu](8|16|32|64)`/`int`/`uint` receiver'ах)
по всему `std/**`+`examples/**` НЕ проводился (только по keyword-совпадениям + hex-константам) —
это единственный класс пропуска, который теоретически возможен. Учитывая, что и хостовый
round-2-аудит, и этот свип независимо сошлись к одному и тому же «чисто» состоянию, оценка
уверенности — высокая, но не 100%.

## Следующий шаг

Нет открытых миграций. Если owner поймает ещё один runtime `integer overflow` crash —
следующий шаг: тот конкретный файл + расширить keyword-список этого свипа тем именем/
паттерном, который был пропущен (обновить эту таблицу).
