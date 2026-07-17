<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 209 Ф.4 — замеры part-size порога (`MULTI_TU_PART_THRESHOLD_BYTES`)

Рабочий чекпоинт-файл (не финальный отчёт — тот идёт в план 209 Ф.4-статус). Обновляется
по одной точке за раз, коммитится после каждой точки (владелец: сеть рвалась, CPU уже не
пустой — фиксировать промежуточные результаты).

## Константа

`compiler-codegen/src/codegen/emit_c.rs:2022` — `MULTI_TU_PART_THRESHOLD_BYTES: usize`.
**Это Rust-константа, НЕ env-override** (env есть только для `multi_tu_enabled` —
`NOVA_MULTI_TU=1`). Значит каждая точка порога требует **пересборки компилятора**
(`cargo build --release --manifest-path nova-cli/Cargo.toml`, ~3 мин/точка). Матрица по
инструкции сокращена до **{500КБ (baseline), 1.5МБ, 4МБ}** (не 5 точек).

Соседняя константа `MULTI_TU_SIZE_THRESHOLD_BYTES = 2МБ` (emit_c.rs:2019, гейт "включать ли
multi-TU вообще" — по размеру ИЛИ >200 top-level fn) — НЕ трогается, это Ф.4 не касается.

## Окружение замера

- Worktree `d:/Sources/nv-lang/nova-209f4` (branch `p209-f4-measure`), бинарь
  `nova-cli/target/release/nova.exe`.
- `NOVA_GC_LIB_DIR`/`NOVA_INCLUDE_DIR`/`NOVA_GC_INCLUDE_DIR` → main-репа vcpkg_installed
  (worktree без vcpkg).
- `NOVA_MULTI_TU=1` (opt-in флаг, дефолт off — не трогаем).
- `NOVA_CACHE=0` (иначе build-cache hit пропускает codegen ЦЕЛИКОМ — `EmitOutput::Single`
  даже выше порога; см. `nova-cli/src/main.rs:4858` коммент — сравнение было бы бессмысленным).
- `TEMP`/`TMP` → изолированная scratch-папка (папка `$TEMP` шэрится с другими параллельными
  агентами на машине — иначе артефакты/PID-папки перемешиваются).
- ⚠ **Шум**: владелец сообщил, что CPU перестал быть тихим (флот агентов работает
  параллельно) в процессе замера. Точки, где это могло повлиять, помечены «шумная» —
  для них медиана по ×3 прогонам вместо ×2.

## Цели (таргеты)

1. **aggregator** — `nova build examples/flagship/aggregator/src/main.nv --strict-effects`
   (dev mode, без `--keep-artifacts` кроме первого прогона на точку — для part-count/size).
   CU переходит порог multi-TU НЕ по байтам (общий вывод ~2МБ, чуть ПОД
   `MULTI_TU_SIZE_THRESHOLD_BYTES`), а по **>200 top-level fn** — годный, хоть и не крупный,
   тест-кейс (реальный multi-part split при 500КБ: 4 части).
2. **conformance mega-CU** — `nova test --filter app_effect_basic_t8_1 --jobs 1
   spec_tests/conformance`. **Важность находки**: `nova check spec_tests/conformance`
   (как было в задании) НЕ годится — `nova check` вообще не проходит через codegen/C-компиляцию
   (чистый параллельный per-file type-check, `cmd_check`/`check_one_file`), порог part-size
   там ни на что не влияет. Реальный gate одного мега-CU (13 МБ / 963 файла, folder-module
   `spec_tests.conformance` — все top-level `.nv` в каталоге суть co-equal peer-файлы ОДНОГО
   модуля) достигается запуском `nova test` с `--filter` на ЛЮБОЙ файл этого общего модуля —
   `app_effect_basic_t8_1` (857 байт исходника) выбран как representative-имя именно потому,
   что упомянут в тексте Ф.4; сам файл тривиален, тяжесть — в общем CU его co-equal peers.
   `--jobs 1` (не 4) — чтобы не мешать с другими файлами/каталогами (у них своя, отдельная,
   компиляция — `--filter` всё равно требует пройтись по дереву, но выполнится только 1 тест).
   Один прогон на порог (дорого — ~130+с), как проговорено в задании.
   Известный PRE-EXISTING RUN-FAIL (`[M-208-vec-chained-debug-display-red]`, Vec chained
   .debug/.display assert) — НЕ связан с 209/этим замером, время сборки не искажает.
3. **std/src/collections** — `nova test std/src/collections` (13 файлов с test-блоками, 6
   library-skip). Каждый файл — СВОЙ отдельный CU (folder ≠ единый модуль здесь — отдельные
   test-файлы). При 500КБ каждый CU уже чуть выше 2МБ-гейта (общий prelude-объём) → **ровно 1
   part на файл** (13 `_common.h` / 13 `_part0.c`, ПРОВЕРЕНО) — т.е. per-file CU здесь МЕЛКИЕ,
   part-size порог 500КБ→1.5МБ→4МБ **не должен менять part-count** (все и так ниже 500КБ на
   part), полезен как negative-контроль (ожидаем плоскую линию).

## Таблица замеров

| Порог | Цель | N прогонов | Время (медиана) | Прогоны (с) | TU-частей | Общий .c (байт) | Заметки |
|---|---|---|---|---|---|---|---|
| 500КБ (baseline, как в main) | aggregator | 3 | **33.25s** | 44.11 / 30.81 / 33.25 | 4 | 1 992 529 | rep1 включал `--keep-artifacts` (лишний I/O) |
| 500КБ | conformance (app_effect_basic_t8_1, jobs=1) | 1 | **134.14s** | 134.14 | 24 | 13 259 539 (11 975 018 parts + 1 284 521 common.h) | известный RUN-FAIL (пред-существующий, не наш) |
| 500КБ | std/src/collections | 3 | **78.04s** | 76.92 / 78.04 / 79.50 | 1/файл (13 CU × 1 part) | — (per-CU мелкие) | 13 PASS / 0 FAIL / 6 SKIP; part-count не варьируется порогом (ожидаемо) |

_(точки 1.5МБ и 4МБ — в процессе, дописываются по мере готовности; см. план 209 Ф.4 за
финальным вердиктом)_
