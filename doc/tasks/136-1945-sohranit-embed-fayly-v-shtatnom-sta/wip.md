# 136 — контрольная точка перед переходом OpenCode

Историческая контрольная точка перед переходом; это НЕ текущее состояние и НЕ сдача. Пауза впоследствии снята интегратором в 17:27. Текущие результаты — в [report.md](report.md). Ниже сохранено состояние на момент передачи, без ретроспективной подмены фактов.

## ТОЧКА

HEAD `878952579d395d1afa62fb27b44f7ce369a76ac8`, ветка `task/136-1945-sohranit-embed-fayly-v-shtatnom-sta`.
Коммитов исполнителя пока нет. Изменённый log.md принадлежит плагину.
Scratchpad: `%LOCALAPPDATA%/Temp/opencode/ses_mvxsg6da38widh1var`.
В нём `cargo-target`, `unit-red.txt`, `unit-green.txt`.

## РЕПРО / ФАКТ

До фикса добавлена unit-фикстура `staging_preserves_explicit_embed_assets_without_sweeping_directories`, зовущая настоящий `stage_program`.
Получено дословно:

```text
test program::tests::staging_preserves_explicit_embed_assets_without_sweeping_directories ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 26 filtered out; finished in 0.02s
```

Первый отсутствующий файл — `helper/shell.tpl.c`.
После первого исправления:

```text
test program::tests::staging_preserves_explicit_embed_assets_without_sweeping_directories ... ok
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.17s
```

Существующие unit-фикстуры импортов 129/132 вошли в эти 14.
После этого добавлены ещё 4 unit-теста геометрии/пакетной границы/отсутствия ресурса/коллизии — ЕЩЁ НЕ ЗАПУЩЕНЫ.

## НОРМА / РЕШЕНИЕ

read-spec выполнен через spec-reader; цитата лично сверена в `spec/decisions/03-syntax.md:12649–12667`:
путь относительно `.nv`, граница — package-root вызова, наружу нельзя.
Source root `[lib] src` не равен package-root (D78).
Scratch-патч 135 прочитан, НЕ переносился: он рекурсивно копирует не-.nv и подавляет ошибки.

Решение владельца через дежурного/интегратора 2026-11-01: разделить дефекты, продолжить driver-only доставку;
сид и D412 не менять; расхождение `project_root_of` учесть отдельно.
**Находка №1949 (номер впоследствии назначен интегратором):** `novac/src/main.nv:312–332` выбирает workspace/верхний manifest;
`03-syntax.md:12652–12654` требует package-root вызова. Ещё НЕ внесена в реестр, номер просить у интегратора.

## ФИКС WIP

- `nova-driver/src/embed_stage.rs`: неглубокое распознавание plain literal `embed("path")`, без compiler dependency;
  комментарии, строки и member-call не считаются вызовами. Только явно названные файлы, без рекурсивного обхода.
- Проверяются лексическая и каноническая границы ближайшего пакета каждого вызывающего файла ДО чтения ресурса.
- Относительная геометрия обычных sibling/подкаталог/родитель внутри stage сохраняется.
  Абсолютный путь, путь вне stage и `.nv`-данные получают приватное `.bin`-имя и правку литерала ТОЛЬКО staged-копии.
- Коллизии назначения не перетираются; directory-аргумент сохраняется пустой папкой для отказа сида;
  отсутствующий файл получает гарантированно отсутствующий staged-путь для `E_EMBED_NOT_FOUND`.
- `program.rs` вызывает доставку после import-rewrite. BOM/байты без rewrite сохраняются через UTF-8 roundtrip.
- `embed_dir`, `embed_str`, сложные литералы НЕ покрыты. Это частичный driver-only ремонт, не соответствие всему D412.
- Сид/пины не изменены. Контракт отправлен исполнителю 137; его выбранный PASS-носитель embed не использует.

## СЛЕДУЮЩИЙ ШАГ ПОСЛЕ РАЗРЕШЕНИЯ ПРОДОЛЖИТЬ ПРОГОНЫ

1. Проверить WIP и форматирование; запустить все целевые unit драйвера в машинной очереди с `NOVA_TEST_TMPDIR`/`CARGO_TARGET_DIR` в scratchpad.
2. Собрать свежий штатный driver в scratchpad. Выполнить `scripts/tools/carina-build-embed.py --driver <binary> --scratch <fresh-session-subdirectory>`.
   Скрипт ЕЩЁ НЕ ЗАПУСКАЛСЯ. Он создаёт изолированный install-root с двумя точечно скопированными distribution assets,
   junction на std и runtime для чтения; все кэши в этом install-root. Нужны инициализированный runtime
   (`CARINA_EMBED_RUNTIME`) и существующие GC includes/lib через `NOVA_GC_INCLUDE_DIR`/`NOVA_GC_LIB_DIR` при необходимости.
   Fixture в `spec_tests/conformance/fixtures/embed_stage_probe` проверяет external dep + own qualified import,
   sibling `shell.tpl.c` и вложенный `certs/localhost_cert.pem`, только синтетические публичные байты.
   Ожидаемый stdout: `shell-136`, `cert-136`, `own` — ровно три строки. Затем негатив missing и package escape.
3. Обязательны сквозные RED/GREEN и саботаж (отключить свой вызов доставки, свежий driver, красный тест, вернуть и зелёный).
   Сейчас имеется ТОЛЬКО unit RED/GREEN, нельзя выдавать его за build/run.
4. Проверить symlink escape, `.nv`-ресурс и разрешённый ресурс за source-root, но внутри package-root; проверить коллизии/сканер.
5. Внести находку №1949 в `docs/plans/221.1-bug-sweep.md`, уточнить статус №1945 как частичный.
6. Отчёт по шаблону, scoped DCO-коммит, затем `crew_task submit`. CI/слияние/приёмка — приёмщику, не исполнялись.

## ЧТО НЕ СДЕЛАНО

Сквозной native build/run, саботаж после фикса, финальный прогон расширенных unit, запись находки в реестр,
коммит, submit, CI/кандидат/вливание. Полные гейты и Oracle не запускались.
`nova check std/src` — не применимо: меняется только compiler-free драйвер, не компилятор/stdlib; Oracle-судья запрещён задачей.
