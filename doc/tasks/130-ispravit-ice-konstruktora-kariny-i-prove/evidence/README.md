# Улики задачи 130 — ICE анонимного конструктора Карины (класс №1484/№1705)

Снято 2026-10-31 на одной машине (Windows, clang). Всё — дословные выводы
прогонов, без правок.

## Источники и бинари

- Pinned B1 (источник RED): novac.exe sha256
  `824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4`,
  source `3729abcce42ec2d94efc898ec6eafe91ff6210d5`, ZIP `7e8264fd…` — как в
  задачах 128/129, verify-seed PASS в каждом прогоне драйвера.
- Исправленный бинарь (источник GREEN): `novac-fixed.exe` sha256
  `be28805ef6f915278dd6aaecc8bfb45106c04ea1c6428d49805063dccfdbc81a`,
  собран Carina-only: B1 emit дерева `3729abcc+фикс` (rc=0, C 13.26 МБ) →
  clang через `nova-driver-support::native_build::compile_c_to_exe` (окно
  `nova build`). Oracle в стадии B не участвовал.
- Оракул (только норма `nova check`/`nova build` на пробах, не судья сида):
  `nova-opencode/nova-cli/target/release/nova.exe`.

## Команды

- RED/GREEN пробы: `cd probes; NOVA_STD_PATH=<std> <novac.exe> emit <p.nv>`
  (stdout = C или JSONL-диагностика).
- Сборка исправленного бинаря: `tools/emit_fixed.sh`-эквивалент + `cc130`
  (помощник в scratch, зовёт compile_c_to_exe с дефолтами драйвера).
- 12 входов polaris: install-контур (`docs/dev/carina-seeds`,
  `scripts/tools/carina-gate.py`, `novac/src`, `std`, `nova_rt`),
  `NOVA_RT_DIR=<install>/nova_rt`, скретч-драйвер с константами локального
  сида, `nova.exe build <polaris>/examples/<NN>/src/main.nv -o …exe
  --toolchain clang --timeout 300`.

## Файлы

- `red-probes-pinned-b1.txt` — 7 форм на pinned B1: все exit=2, ICE
  `type_of.nv:632` дословно.
- `green-probes-fixed-binary.txt` — те же формы + контроли на финальном
  бинаре: носители emit rc=0, границы — именованные отказы.
- `cycle_red.log` / `cycle_green.log` — саботаж (1): дверь опусточена →
  ICE возвращается; восстановлена → GREEN.
- `red2.log` — саботаж (2) против константного носителя: ICE возвращается.
- `cycle-final.log` — финальная сборка + пробы (12/12) + стейдж.
- `emit_fixed.log`, `compile_fixed.log`, `build_fixed.log` — первые циклы
  сборки (B1 emit rc=0 — самосборка исправленного дерева цела).
- `base-vs-main-compare.txt` — побайтовое сравнение изменённых файлов
  база/main (4 идентичны; calls.nv — только host_os-мост задачи 92 + сдвиг
  литерала ice_at).
- `polaris-outcomes-12.tsv` + `polaris-logs/` — финальные исходы 12 входов:
  0 ICE, все отказы именованные (843–981 диагностик на вход).
- `polaris-01-stage-diags-named.txt` — ручной прогон стейджа 01-hello
  финальным бинарем: 953 именованные диагностики, ни одного ICE.
- `seed-descriptor-local.json` — дескриптор ЛОКАЛЬНОГО ассета (пины
  репозитория не тронуты; публикация — владелец тегом).

## Зонды

`probes/` сессионного scratch: p1484/m2/p1705/e1/e3/e4/ar1/m1/m5/e2/pn1/pn2/
po1/po2/c1..c9/mc1/mc2/neg4 — каждая форма минимальна; содержимое видно в
фикстурах `novac/fixtures/anon_ctor/` и в красном/зелёном снятиях.
