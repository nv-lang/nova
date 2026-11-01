# Улики задачи 129 — передача внешних исходников зависимостей сиду Carina B1

Снято 2026-10-30 на одной машине (Windows, clang). Все файлы — дословные
выводы прогонов, без правок.

## Источники и бинарь

- Дерево: worktree `nova-opencode-129-...`, ветка
  `task/129-ispravit-peredachu-vneshnih-ishodnikov-p` (коммит — в report.md).
- Бинарь драйвера: `nova-driver/target/release/nova.exe`, собран из этого
  дерева (`cargo build --release` в `nova-driver/`).
- Сид pinned B1 (проверен `carina-gate.py verify-seed` в каждом запуске):
  ZIP SHA-256 `7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb`,
  novac.exe SHA-256 `824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4`,
  source `3729abcce42ec2d94efc898ec6eafe91ff6210d5` — как в задаче 128.
- Oracle (compiler-codegen / nova-cli) не собирался и не запускался ни на
  одном шаге.

## Контур

Минимальный install-контур `<scratch>/install` (junctions:
`docs/dev/carina-seeds`, `novac/src`, `std/src`, `nova_rt`), env
`NOVA_RT_DIR=<install>/nova_rt`, `NOVA_CG_INCLUDE=<worktree>/compiler-codegen`.
Рабочие копии потребителей: nova-duckdb `a59a073`, nova-polaris
`24f9d1757868`, nova-postgres `8a46520269cf` (те же, что у замера 128).
Scratch сессионный — сначала `D:\Temp\opencode\129`, после замечания
интегратора перенесён в объявленный средой сессионный scratch
`%LOCALAPPDATA%\Temp\opencode\ses_mvv57le7eqn89n9963\129` (см. report.md,
«СРЕДА/SCRATCH»).

## Команды

- Каждый из 18 входов: `nova.exe build <вход> -o <scratch>.exe --toolchain clang`.
- Регрессия: 61 PASS-пример замера 128 (таблица `table-118.tsv` коммита
  `118e4ade5`), build + запуск бинаря (таймаут 30 с), сверка rc/run_rc.
- Страж класса: `python scripts/tools/carina-build-dep.py` с
  `NOVA_CARINA_DRIVER=<драйвер>`.

## Файлы

- `outcomes-18.tsv` — 18 строк: вход, явный `result` (все FAIL), сколько
  файлов стейджнуто, число диагностик сида, первая диагностика дословно
  (следующий блокер), распределение по файлам.
- `logs-18/` (18) — дословные stdout+stderr каждого входа ПОСЛЕ фикса.
- `regression-61.tsv` — 61 строка первого прогона: файл, rc, секунды, run_rc,
  run_rc замера 128, вердикт SAME/DIFF (все SAME).
- `regression-62-arithmetic.txt` + `regression-62-arithmetic-build.txt` —
  догон 62-го PASS-входа (`examples/basics/arithmetic.nv`, выпал из первого
  прогона из-за сдвига `[1:]` под несуществующую шапку в run61.py): SAME,
  итог регрессии 62/62.
- `probe-ab-types.txt` — живая проба одноимённых модулей: депы `a` и `b` с
  модулем `types` у каждого, разные значения; build rc=0, запуск печатает
  дословно `a=111 b=222 only_a=1 only_b=2` — смешения деклараций нет.
- `probe-ab-types-decoy.txt` — та же проба с приманкой (неимпортируемый
  `give_a() => 999` в `b.types`): громкий отказ D84-неоднозначности — граница
  сида (глобальный резолв по имени), отдельная строка №1931 в реестре.
- `guard-red.txt` / `guard-green.txt` — страж `carina-build-dep.py` на
  саботированном (`collect_dep_roots` возвращает пустые корни) и на
  восстановленном драйвере.
- `sabotage-class-red.txt` / `sabotage-class-green.txt` — то же условие на
  реальном входе duckdb `01-hello`: красный с дословной исходной сигнатурой
  класса (`undeclared type name QueryResult`, `no such method in the
  declarations handed to novac`, стейджинга нет), зелёный после возврата.
- `verify-stage-polaris.txt` — побайтовая сверка стейдж-дерева polaris
  `01-hello` (53 файла): 27 байт-идентичны источникам, 24 отличаются ТОЛЬКО
  токенами import, 2 «unmatched» — tls/config.nv и tls/ffi.nv, взятые из
  git-cache checkout'а pinned-ревизии nova-tls `ac7de2ab…` (проверено
  побайтово отдельно) — локальная копия nova-tls и не должна была
  использоваться (в `[replace]` polaris-примера её нет).
- `polaris-01-manual-seed-ice.txt` — ручной прогон pinned-сида на
  стейдж-дереве (без драйвера): тот же ICE `type_of.nv:632` — ICE привязан к
  сиду и входу, не к обвязке драйвера.
