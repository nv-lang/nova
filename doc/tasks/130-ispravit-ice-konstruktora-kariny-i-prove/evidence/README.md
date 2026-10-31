# Улики задачи 130 — ICE анонимного конструктора Карины (класс №1484/№1705, строка №1933)

Снято 2026-10-31 на одной машине (Windows, clang). Всё — дословные выводы
прогонов, без правок. ДОПУСТИМЫЕ доказательства — Carina-only (норма —
спецификация, D478/D489/D491/D55/D52); Oracle-прогоны судьями не являются
(см. report.md, «СПРАВКА СОВМЕСТИМОСТИ»).

## Точные пути и команды (для независимого повторения)

- Pinned B1 (источник RED):
  `%LOCALAPPDATA%\Nova\carina-seeds\windows-x86_64\unpacked\nova-carina-0.2.1-windows-x86_64\novac.exe`
  (sha256 `824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4`,
  source `3729abcce42ec2d94efc898ec6eafe91ff6210d5`; ZIP из
  `…\carina-seeds\windows-x86_64\nova-carina-0.2.1-windows-x86_64.zip`,
  verify-seed PASS в каждом прогоне драйвера).
- Канонический исправленный бинарь (GREEN) собирается repro-скриптом (ниже);
  после прогона лежит в `<scratch>\130\repro\novac-fixed-from-commit.exe`
  (в сборке задачи — `…\130\out\novac-fixed.exe`); C —
  `<scratch>\130\repro\fixed-from-commit.c`.
- ВОСПРОИЗВЕДЕНИЕ БИНАРЯ (полный сценарий, все шаги дословно):
  1. `git -C <репозиторий> archive --format=tar -o base.tar 3729abcc… novac std`
     и распаковать: база сида.
  2. Наложить семь файлов из `evidence/seed-backport/check/` в
     `novac/src/check/` (SHA-256 каждого — `seed-backport/MANIFEST.md`).
  3. Emit запиненным B1 (самосборка; stdout = C):
     `NOVA_STD_PATH=<base>/std/src NOVAC_SELF_PATH=<base>/novac/src <B1>/novac.exe emit <base>/novac/src/main.nv > fixed.c`
     → 13 260 516 байт, sha256 `c6340905…` (стабильная связь исходник↔C;
     перенос №1934→№1933 в комментариях C не меняет).
  4. Компиляция C — окно `nova build`: nova-driver-support
     `native_build::compile_c_to_exe` (те же дефолты, что драйвер:
     detect_toolchain(auto) → clang, rt_dir=`<runtime>/compiler-codegen/nova_rt`,
     cg_include=`<runtime>/compiler-codegen`, Boehm, libuv detect_or_build,
     mode Release; таймаут 900 с). Для одиночной команды использован
     40-строчный помощник cc130 (scratch), вызывающий ровно эту функцию;
     сам скрипт: `tools/repro_from_commit.sh` сессионного scratch.
     Итог: exe (sha256 путь-зависим через PE-отладочную информацию — опора
     связи C sha256 + MANIFEST).
  5. Пробы: `cd probes; NOVA_STD_PATH=<base>/std/src <exe> emit <p>.nv`
     (носители rc=0 с C на stdout; границы rc=1 с одной именованной
     диагностикой). Дословно — `repro-from-commit.txt`.
- СВОДКА ПО rc В ЛОГАХ: в ранних логах последняя строка `rc=1` — это выход
  пробы-ГРАНИЦЫ (po2: ожидаемый именованный отказ) сквозь `set -e` старой
  редакции скрипта, а не сбой воспроизведения; в финальной редакции скрипт
  ловит rc каждой пробы и завершается 0 (`== repro complete`).
- 12 входов polaris: install-контур (`docs/dev/carina-seeds`,
  `scripts/tools/carina-gate.py`, `novac/src`, `std`, `nova_rt`),
  `NOVA_RT_DIR=<install>/nova_rt`, скретч-копия драйвера с константами
  ЛОКАЛЬНОГО сида (пост-тег состояние), `nova.exe build <вход> -o …exe
  --toolchain clang --timeout 300`.
- std/src до/после: `nova check std/src` на базе 0b8438a34 и на ветке —
  дословно `PASS: 150 FAIL: 39 WARN: 58` (одинаково; 39 — предсуществующее).

## Файлы (.log → .txt: *.log игнорируется git, сняты дословно)

- `seed-backport/` — ТОЧНЫЕ семь файлов бэкпорта + MANIFEST.md (SHA-256;
  6/7 совпадают с файлами ветки по модулю концов строк; calls.nv —
  база+хунки задачи, без host_os-моста задачи 92).
- `repro-from-commit.txt` — воспроизведение бинаря из сданного состояния
  (C/EXE sha256, пробы).
- `red-probes-pinned-b1.txt` — 7 форм на pinned B1: exit=2, ICE 632 дословно.
- `green-probes-fixed-binary.txt` — 13 проб на каноническом бинаре.
- `cycle-red.txt` / `cycle-green.txt` / `red2.txt` / `cycle-final.txt` /
  `cycle-v4.txt` — саботаж RED/GREEN, два прохода, обе стороны.
- `build-fixed.txt`, `compile-fixed.txt`, `emit-fixed.txt` — циклы сборки.
- `base-vs-main-compare.txt` — побайтовое сравнение файлов база/main.
- `polaris-outcomes-12.tsv` + `polaris-logs/*.txt` — финал: 0 ICE на всех
  12, именованные отказы подмножества (843–981 диагностик на вход).
- `polaris-01-stage-diags-named.txt` — ручной прогон стейджа 01-hello.
- `seed-descriptor-local.json` — дескриптор ЛОКАЛЬНОГО ассета (пины
  репозитория не тронуты; публикация — отдельное решение владельца).
