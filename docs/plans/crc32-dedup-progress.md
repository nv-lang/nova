<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# crc32-dedup — checkpoint (ветка `fix-crc32-dedup`, sonnet, 2026-07-15)

Задача: `[M-compress-checksum-cleanup]` (docs/plans/backlog-followups.md P1) —
crc32 троился (std/checksums, std/encoding/compress, nova-compress). Решение
владельца: дедуп через std, `adler32` промоутнуть в std/checksums.

## Сделано (std-часть, эта ветка)

1. `std/src/checksums/adler32.nv` — NEW, зеркало формы `crc32.nv`
   (`adler32`/`adler32_init/update/finalize`, RFC 1950 §9, mod 65521).
2. `std/src/checksums/adler32_test.nv` — NEW: пустой ввод, `"Wikipedia"`→
   `0x11E60398`, `"123456789"`→`0x091E01DE`, incremental==монолитный,
   bit-sensitivity, 1KB consistency. Зеркало `crc32_test.nv`.
3. `std/src/encoding/compress/checksum.nv` — своя реализация CRC-32/Adler-32
   снята; теперь `export import std.checksums.crc32.{...}` +
   `export import std.checksums.adler32.{...}`. Инлайн-тесты (8 шт.) НЕ
   тронуты (per заданию — их удаление ждёт 205-endgame удаления std/compress
   целиком).
4. **Найденный нюанс (важно для будущих peer-module рефакторингов):**
   folder-module `share namespace` (D29) распространяется на декларации,
   но НЕ на имена, которые peer-файл сам импортировал/ре-экспортировал —
   `export import` в одном peer-файле не делает имя видимым в ДРУГОМ
   peer-файле без его собственного `import`. Подтверждено эмпирически
   (CODEGEN-FAIL → добавлен точечный import → PASS). Поэтому:
   - `gzip.nv` получил `import std.checksums.crc32.{crc32, crc32_init, crc32_update, crc32_finalize}`
   - `zlib.nv` получил `import std.checksums.adler32.{adler32, adler32_init, adler32_update, adler32_finalize}`
   - `d336_checksum_test.nv` получил оба точечных импорта (только нужные имена)
5. Маркер `[M-compress-checksum-cleanup]` обновлён в backlog-followups.md —
   std-часть отмечена DONE, nova-compress-синк остаётся OPEN (в) отдельным
   шагом, вне этой ветки/репы.

## Гейты (foreground, worktree `nova-crc32`)

- `nova test std/src/checksums` → `PASS: 3 FAIL: 0 SKIP: 3` (crc32/adler32/fnv
  compiled-ok-no-test SKIP; `*_test.nv` PASS×3 включая новый `adler32_test`).
- `nova test std/src/encoding/compress` → `PASS: 1 FAIL: 0` (весь merged-CU
  — gzip/zlib/checksum/brotli/d333-336 включая d336-checksum-контракт —
  зелёный одним прогоном под именем `brotli`).
- Собран release: `compiler-codegen` (cargo build --release) +
  `nova-cli` (cargo build --release) в этом worktree, оба Finished release.

## НЕ сделано (осознанно, вне объёма этой ветки)

- **(в) nova-compress синк** — `nova-compress/src/checksum.nv` (внешняя
  репа) должен получить тот же `import std.checksums.{crc32,adler32}`.
  НЕ тронуто — задание явно исключало трогать внешнюю репу. Push —
  оркестратором ПОСЛЕ вливания std-части (пакет тянет std как зависимость).
- Полный `spec_tests/conformance` — не прогонялся в этой ветке (точечная
  верификация по заданию; полный гейт — оркестратор перед merge).
- Не язык-меняющее слияние (чистая перекладка кода между модулями std +
  facade re-export) → **D-амендмент спеки не требуется**.

## Файлы

- `std/src/checksums/adler32.nv` (new)
- `std/src/checksums/adler32_test.nv` (new)
- `std/src/encoding/compress/checksum.nv` (реализация → re-export)
- `std/src/encoding/compress/gzip.nv` (+ import crc32-семейства)
- `std/src/encoding/compress/zlib.nv` (+ import adler32-семейства)
- `std/src/encoding/compress/d336_checksum_test.nv` (+ оба точечных импорта)
- `docs/plans/backlog-followups.md` (маркер обновлён)
