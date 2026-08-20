---
name: reference-msys2-grep-lc-all-c
description: "msys2 grep на этом хосте (ru_RU.UTF-8): байт-диапазоны падают EXIT=2, астральные эмодзи молча не матчатся — стражи с мультибайт-паттернами ОБЯЗАНЫ export LC_ALL=C"
metadata: 
  node_type: memory
  type: reference
  originSessionId: c80784bc-9a61-41c1-a8f4-b52cba9515c4
  modified: 2026-08-01T11:16:05.740Z
---

msys2/GitBash grep на хосте владельца (локаль ru_RU.UTF-8) — два тихих режима отказа
на не-ASCII паттернах (оба пойманы селфтестами gate.sh 2026-08-01):

1. **Байтовые bracket-диапазоны** (`[\220-\277]`) → «Invalid collation character»,
   EXIT=2, ноль строк — конвейер с `|| true`/храповиком «только вниз» маскирует в
   ложное зелёное (check-doc-hygiene: счётчик кириллицы тихо стал 0 при baseline 598).
2. **Астральные (4-байтовые UTF-8) литералы** — 🟡🔴📋 дают 0 хитов при корректных
   байтах в файле И паттерне (проверено `grep -f` матрицей); BMP-символы (✅,
   3-байтовые) матчатся → пороги «N строк» тихо не добираются
   (check-no-manual-status-table).

**Правило:** любой страж/скрипт с не-ASCII grep-паттерном — `export LC_ALL=C`
(байтовый матч) сразу после `set -e`-строки, с комментарием класса. Фиксы-образцы:
scripts/guards/check-doc-hygiene.sh (644a6ee8a), check-no-manual-status-table.sh +
check-guard-wiring.sh (320ee0bb5).

**Мета-урок:** страж без селфтеста не работает — оба дефекта жили под зелёным гейтом
и всплыли только когда селфтесты стражей встали в gate.sh отдельным шагом.

Связано: [[feedback-gate-filter-must-assert-pass-line]] (класс «пустой grep ≠ зелёно»).
