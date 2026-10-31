# Улики задачи 130 — ICE анонимного конструктора Карины (класс №1484/№1705)

Снято 2026-10-31 на одной машине (Windows, clang). Всё — дословные выводы
прогонов, без правок. ДОПУСТИМЫЕ доказательства — Carina-only (норма —
спецификация, D478/D489/D491/D55/D215); Oracle-прогоны в подготовке не
участвовали как судьи (перекрёстные наблюдения — см. report.md).

## Источники и бинари

- Pinned B1 (источник RED): novac.exe sha256
  `824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4`,
  source `3729abcce42ec2d94efc898ec6eafe91ff6210d5`, ZIP `7e8264fd…` — как в
  задачах 128/129, verify-seed PASS в каждом прогоне драйвера.
- Канонический исправленный бинарь (GREEN): exe sha256
  `de88e571ba60e7f569e35a615a37c772f03f79d6e13a0c3b6a83ea2f84f141e1`,
  C sha256 `c6340905dfb014ad4e0823536e5898837a5380ebebfec686154991422ed1fe2a`,
  собран ВОСПРОИЗВОДИМО из сданного состояния (см. `seed-backport/` и
  `repro-from-commit.log`): `git archive 3729abcc novac std` + накладка
  семи файлов бэкпорта + pinned B1 emit (rc=0, самосборка цела) + clang
  через `nova-driver-support::native_build::compile_c_to_exe` (окно
  `nova build`). Oracle в стадии B не участвовал.
- Локальный ассет для повтора polaris: zip sha256
  `339d7096db40ab2a062ece78858e756632ca1516b29e9d8fd8151c0c0c8ee74b`,
  собран из канонического прогона; дескриптор —
  `seed-descriptor-local.json` (пины репозитория НЕ тронуты; публикация —
  владелец тегом, см. БЛОКЕР в report.md).

## Команды

- RED/GREEN пробы: `cd probes; NOVA_STD_PATH=<std> <novac.exe> emit <p.nv>`.
- Воспроизведение бинаря: `repro-from-commit.log` (дерево из git archive,
  семь файлов из `seed-backport/`, B1 emit, compile_c_to_exe).
- 12 входов polaris: install-контур (`docs/dev/carina-seeds`,
  `scripts/tools/carina-gate.py`, `novac/src`, `std`, `nova_rt`),
  `NOVA_RT_DIR=<install>/nova_rt`, скретч-копия драйвера с константами
  ЛОКАЛЬНОГО сида (пост-тег состояние), `nova.exe build <input> -o …exe
  --toolchain clang --timeout 300`.
- std/src до/после: `nova check std/src` на базе 0b8438a34 и на ветке —
  дословно `PASS: 150 FAIL: 39 WARN: 58` (одинаково; 39 — предсуществующее).

## Файлы

- `seed-backport/` — ТОЧНЫЕ семь файлов бэкпорта + MANIFEST.md (SHA-256;
  6/7 байт-идентичны коммиту efb2af8c4; calls.nv — база+хунки задачи).
- `repro-from-commit.log` — воспроизведение: C/EXE sha256 + пробы.
- `red-probes-pinned-b1.txt` — 7 форм на pinned B1: exit=2, ICE 632 дословно.
- `green-probes-fixed-binary.txt` — 13 проб на каноническом бинаре
  (носители rc=0, границы — именованные отказы).
- `cycle_red.log`/`cycle_green.log`/`red2.log`/`cycle-final.log` — саботаж
  RED/GREEN, два прохода, обе стороны.
- `emit_fixed.log`, `compile_fixed.log`, `build_fixed.log` — циклы сборки.
- `base-vs-main-compare.txt` — побайтовое сравнение файлов база/main.
- `polaris-outcomes-12.tsv` + `polaris-logs/` — финал: 0 ICE на всех 12,
  именованные отказы подмножества (843–981 диагностик на вход).
- `polaris-01-stage-diags-named.txt` — ручной прогон стейджа 01-hello
  каноническим бинарем: именованные диагностики, ни одного ICE.
