# Отчёт задачи 129 — передача внешних исходников зависимостей сиду Carina B1

РЕПРО: подтверждён — до фикса `nova build` duckdb `examples/01-hello/hello_duckdb.nv`
через install-контур падал дословно как в замере 128: `E_NOVAC_SUBSET`
«undeclared type name `QueryResult`/`Connection` … declarations handed to
novac», ноль диагностик внутри файлов депа (сид их не видел). Контур и
команды — `evidence/README.md`.

ТОЧКА: `nova-driver/src/program.rs` (обход №1889) — граф программы
пересекался только внутри собственного пакета; `resolve_import` для чужого
первого сегмента возвращал пусто, и до сида не доходил ни один файл депа.
Вторая точка: `nova-driver/src/carina_build.rs` — сбор списка файлов без
учёта депов.

ФИКС (только `nova-driver`, сид запинен и неизменён, без Oracle):
- `program.rs`: `DepRoots`/`collect_dep_roots` — транзитивный обход
  `[dependencies]`, `[replace]`/nova.override.toml КОРНЯ на каждое ребро
  (Go-scope, D420 §2 дофикс №2), git-депы через `git_cache::resolve_git_dep`,
  ключ == `[package].name` депа; `resolve_dep_import` — root peers для
  голого `import dep` (D78 rev-4 «Root peers», spec/decisions/07-modules.md);
  стейджинг модулей депов по каталогу на модуль под `_deps/<module path>/`;
  `rewrite_imports` — qualified-импорты на депы переписываются в относительные
  пути ТОЛЬКО в стейдж-копиях (сид связывает единицы только по относительным
  импортам — измеренный корень №1924); путь всегда кончается сегментом
  модуля (274.11 E.10); peer-фильтр `_test`/`_slow`/OS-суффикс по D376 и
  Plan 42.12 Ф.1 (`NOVA_TARGET_OS`, иначе host); BOM и CRLF в стейдж-копиях
  сохраняются.
- `carina_build.rs`: две точки вызова (сбор корней + discover/stage с
  переписыванием), удаление стейджа — только если он создавался.

ФИКСТУРЫ: `spec_tests/conformance/dep_build_min/` (пакет + path-деп внутри
подмножества сида; без EXPECT_*-маркера намеренно — импорт депа не резолвится
в контексте пакета spec_tests под Oracle-раннером, судья — страж) +
`scripts/tools/carina-build-dep.py` (драйвер + pinned B1, build → запуск →
построчная сверка stdout: `triple(7) = 21` из кода депа — деп передан,
слинкован и исполнен). Юнит-тесты `program.rs`: 23/23 (dep staging/rewrite,
ancestor-target, CRLF, BOM, peer-фильтр, матрица OS-суффиксов).

САБОТАЖ (дословно в evidence/):
- красный: `collect_dep_roots` с пустыми корнями → страж:
  `carina-dep FAIL: driver build failed (exit 1)` с
  `E_NOVAC_SUBSET «triple is not a callable novac knows»`; реальный вход
  duckdb 01-hello → дословная исходная сигнатура класса
  (`undeclared type name QueryResult`, `no such method … handed to novac`,
  стейджинга нет);
- зелёный после возврата: `carina-dep PASS: dep_build_min exit=0 stdout=1
  lines`; duckdb 01-hello → сигнатура класса исчезла (0 «undeclared»), сид
  выдаёт 269 диагностик ВНУТРИ стейдженных исходников депа.

std/src: не применимо — правка только в `nova-driver` (отдельный crate,
`nova check` его не потребляет); компилятор (compiler-codegen/nova-cli) и std
не тронуты ни одной строкой, nova-cli в этом дереве даже не собирался.

ВЕТКА/КОММИТ: `task/129-ispravit-peredachu-vneshnih-ishodnikov-p`, коммит —
в сдаче (ниже).

## Критерии приёмки задания

- **RED/GREEN на потере исходников** — см. САБОТАЖ; GREEN доказан не только
  сменой диагностики, но сквозной позитивной фикстурой (деп исполняется).
- **Позитивная и отрицательная регрессии разрешения внешних импортов** —
  позитивная: `dep_build_min` + транзитивный граф polaris (http/tls/compress
  из pinned git-cache, 45-55 файлов на вход, побайтовая сверка:
  `verify-stage-polaris.txt`); отрицательная: неизвестный импорт (не деп/не
  свой/не std) ничего не стейджит, отказ сида по имени как сегодня
  (юнит-тест + саботаж-форма); одноимённые последние сегменты у разных депов
  (`a.types`/`b.types`) — разные каталоги `_deps/`, сид различает (ручная
  проба, emit rc=0, обе записи в C).
- **Отсутствие регрессии локальных импортов** — 61/61 PASS-примеров замера
  128 пересобраны этим драйвером, rc и run_rc совпали (`regression-61.tsv`,
  все SAME); однофайльный исторический путь не тронут (без reachable-файлов
  стейдж не создаётся).
- **Фактические исходы 18 входов после фикса, сохранённая диагностика** —
  `outcomes-18.tsv` + `logs-18/`: все 18 остаются красными, но по честным
  причинам подмножества B1, а не по классу передачи: duckdb×3 — E_NOVAC_SUBSET
  внутри `duckdb/ffi/ffi.nv` и др. (`extern` вне подмножества; 269-369
  диагностик); postgres×3 — то же (66-233, включая tls из git-cache);
  polaris×12 — `E_NOVAC_ICE novac/src/check/type_of.nv:632 «anonymous
  constructor without a spread»`, воспроизведён ручным прогоном сида на
  стейдж-дереве без драйвера (`polaris-01-manual-seed-ice.txt`) — привязан к
  сиду и входу. Обещания «починил всё» нет и не было.
- **spec**: язык не меняется — «не применимо».
- **registry**: строка №TBD в docs/plans/221.1-bug-sweep.md (этим же
  слиянием); тесты не ослаблены; новых E_*/W_* нет.
- **invariant**: новых стражей-инвариантов нет; страж класса —
  `carina-build-dep.py` (самотест — его саботаж-RED выше).
- **gate/ci**: локально гейты не гонял (по брифу); кандидат и CI — за
  приёмщиком.

ЧТО НЕ СДЕЛАНО: qualified-импорты СОБСТВЕННОГО пакета не тронуты (поведение
как сегодня; корень — №1924); библиотеки без `main` и непубликованный SDL —
вне класса; последующие отказы сида на 18 входах (extern FFI, record-ctor
позиции, ICE type_of.nv:632) — отдельные классы, не чинились; мелкий
текстовый обход по-прежнему не видит import, разнесённый на несколько строк,
и не стейджит embed-ассеты (оговорка №1889 в силе).
