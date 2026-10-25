# Отчёт задачи 120 «fix reg 1889 multi-module Carina build»

Форма — docs/dev/prompts/window-brief-template.md §6 плюс шаги приёмки проекта.

РЕПРО: подтверждён. Минимальная двухфайловая программа-пакет
(`target/repro120/twopkg/`, скрэтч): `nova.toml` (`[package] name = "twopkg"`,
`[lib] src = "."`), `main.nv` (`module twopkg.main`, `import ./helper.{greet}`,
`println(greet("Nova"))`), `helper/helper.nv` (`module twopkg.helper`,
`export fn greet(name str) -> str => "Hello, ${name}!"`). Штатный `nova build`
драйвером ДО фикса (Windows, pinned Carina B1
`nova-carina-0.2.1-windows-x86_64`, asset sha256
`7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb`, clang):
`Carina B1 emit failed (exit 1)`. Прямой прогон того же сида с историческим
окружением драйвера дал дословно (JSON, сохранён в
`target/repro120/seed-out.txt`):
`{"id":"d0","code":"E_NOVAC_SUBSET",...,"primary":{"file":"...twopkg\\main.nv",...},"message":"outside the subset: `greet` is not a callable novac knows in an expression ..."}`.
Контроль: тот же вход с `NOVAC_SELF_PATH=<корень программы>` — emit зелёный:
сид исправен, неверен контракт его вызова.

ТОЧКА: `nova-driver/src/carina_build.rs` — `configure_seed_command` выставлял
`NOVAC_SELF_PATH=<repo>/novac/src` (источники самой Карины) для ЛЮБОЙ
пользовательской сборки; вход вне этого корня сид компилирует одиночной
единицей, граф импортов программы до него не доходит, имена из второго модуля
неизвестны (E_NOVAC_SUBSET). НЕ подтвердились гипотезы «порядок emit» и
«линковка нескольких CU»: сид собирает мультимодульный вход целиком, когда
корень ему дан верно.

ФИКС (класса, driver-side — сид B1 запинен и неизменяем): новый модуль
`nova-driver/src/program.rs` (мелкий текстовый обход графа программы:
import-строки и module-строки, стейджинг достижимых файлов в
`target/carina-build/<stem>.<pid>.src/` с сохранением package-relative layout);
`carina_build.rs:445,515` — `NOVAC_SELF_PATH` = стейдж (self_root пронесён в
`configure_seed_command` и capture-metadata); `carina_build.rs:81,642` —
`normalize_verbatim_path` для `package_dir` (иначе `strip_prefix` по
verbatim-пути молча клал файлы плоско); сообщение об ошибке emit теперь несёт
stdout сида. Программа без собственных модулей идёт историческим путём, golden
D89 не тронут. Где ещё искался тот же корень: вариант «NOVAC_SELF_PATH = корень
пакета» отвергнут ЗАМЕРОМ, не вкусом — `spec_tests` как корень не завершается за
10 минут, а `standalone/` стал бы одной единицей из 404 main (golden сломался
бы). Попутно в этом же мердже: publish временного EXE через `std::fs::rename`
падал cross-drive (Windows os error 17) — добавлен fallback copy+remove; это
носитель открытой записи №1895, её статус и класс — на усмотрении интегратора.

ФИКСТУРЫ (зовут проверяемое, маркер EXPECT_STDOUT утверждает наблюдаемый вывод):
`spec_tests/conformance/crossmod_build_min/` (минимум: main + helper, ожидание
`Hello, Nova!`), `spec_tests/conformance/crossmod_build_program/` (порт реальной
трёхмодульной программы `novac/fixtures/multi_module/pos_1`, 14 строк вывода,
хвост `prog ok`). Реальные мультимодульные прогоны «после» (всё exit 0):
двухфайловый twopkg — печатает `Hello, Nova!`;
`spec_tests/conformance/const_peer_group/cpg_driver` — stdout `14`;
`crossmod_build_program` — 14 строк, `prog ok`. Oracle-прогон новых фикстур
(главная копия, `nova-cli/target/release/nova.exe`): PASS 3 FAIL 0 — расхождений
с Oracle не зафиксировано. Свежее дословное «после» (повторено при сдаче):
`nova build program: 2 source file(s) staged under ...\target/carina-build\main.11168.src`,
`nova build: target/repro120/twopkg/twopkg.exe binary_sha256=fd600b89...`, rc=0,
запуск: `Hello, Nova!`, rc=0.

САБОТАЖ (в обе стороны, условие = «стейдж программы отдаётся сиду как
NOVAC_SELF_PATH»): сломано — RED: `Carina B1 emit failed (exit 1)` с
`E_NOVAC_SUBSET ... doubled_limit` (имя из второго модуля снова неизвестно);
восстановлено — GREEN (build rc=0, ожидаемый stdout). Юнит-тесты драйвера:
`cargo test --release` в `nova-driver/` — 13/13 ok на финальном дереве.

std/src: не применимо: правка только nova-driver (Rust), компилятор
(`compiler-codegen/`) и `std/` не тронуты — `nova check std/src` не судит этот
шов (Carina-build путь в `nova check` не участвует).

spec: не применимо — язык не меняется, D-блок не нужен.

ВЕТКА/КОММИТ: task/120-fix-reg-1889-multi-module-carina-build
(коммиты: 356aebef3 папка задачи; c5d176944 фикстуры-пробы; e4242dfbc фикс
драйвера — его пометка «unverified» относится к моменту чекпоинта перед
перезапуском сервера, все замеры «после» сделаны на этом коде и перечислены
выше; dd5764ba0 реестр №1889 + база + отчёт; 0936cdac1 контрольная точка log;
8006c43cc фикс `{}`→`{:#}` по красному CI; финальный коммит — итоги CI в этом
отчёте). Кандидат приёмки: integrate/t120.

ЧТО НЕ СДЕЛАНО (прямым текстом):
- Обход графа в program.rs — мелкий текстовый скан: import, разнесённый на
  несколько строк, не отслеживается; embed-ассеты не стейджатся. Оговорка
  записана в №1889.
- B1 ICE `pipeline/program.nv:157` на package-qualified импортах собственного
  пакета (`import twopkg.helper.{greet}`) — ребро между единицами не
  создаётся; НЕ чинился (сид запинен), кандидат на отдельную запись №TBD.
- №1895 (cross-drive publish): починен только носитель (fallback copy+remove);
  закрытие класса — за интегратором.
- MSVC (№1890) — вне объёма 0.3 по слову владельца 18:05.

## Шаги приёмки проекта

- criteria: пункты задания выполнены — 1) репро «до» красное, лог выше;
  2) класс назван и зафиксирован: №1889 в docs/plans/221.1-bug-sweep.md
  (номер из резерва интегратора 1889-1891); 3) двухфайловая программа и два
  реальных мультимодульных примера собираются и исполняются (выводы дословно
  выше); 4) саботаж в обе стороны — выше; 5) golden D89 зелёный:
  `scripts/tools/carina-build-golden.py` PASS, generated C побайтово равен
  записанному SHA-256 `0b066e3bccd3c8d0a93afe09be182dd314913038ef8d17ec582640d9c3cae4a1` —
  emit-контракт не сломан; 6) реестр и стражи по конвенциям — ниже.
- fixture: обе новые фикстуры с EXPECT_STDOUT утверждают наблюдаемый вывод и
  зовут проверяемое (main зовёт функцию второго модуля).
- sabotage: выше, оба направления с дословным выводом.
- class: фикс класса (контракт вызова сида: self-path — корень графа
  ПРОГРАММЫ, а не исходники Карины); альтернативный корень «пакет целиком»
  отвергнут замером; границы фикса названы (№TBD на package-qualified imports).
- registry: №1889 заведён тем же слиянием; тесты не ослаблены и не удалены;
  новых E_*/W_* нет. Стражи (Git Bash/python, все rc=0):
  check-registry-rows-intact (1820/1820, max 1911, дублей 0, протухших дыр 0),
  check-registry-entry-shape (343/343, разобрано 1820),
  check-registry-routes (блокеров 111/111, без маршрута 0, без оговорки 0),
  check-registry-single-verdict ok, check-registry-status-field ok,
  check-registry-past-tense-status ok, check-registry-closure-kept ok,
  check-registry-row-closed ok. База scripts/guards/registry-rows.baseline:
  rows 1819→1820, max без изменений, 1889 убран из gaps (1890-1891 — резерв
  интегратора), летопись дописана сверху.
- spec: не применимо (язык не меняется).
- std-check: не применимо (правка без компилятора и std — только nova-driver).
- invariant: новых инвариантов и стражей не введено; регресс держат две
  conformance-фикстуры с EXPECT_STDOUT.
- gate / ci: локальные гейты не гонялись (слово владельца 2026-10-07: гейты
  отданы GitHub CI). Первый кандидат 0936cdac1 показал ОДИН настоящий отказ
  по этой работе — check-error-rewrap-chain на carina_build.rs:331 (`{}`
  вместо `{:#}` у значения ошибки; коммит 8006c43cc, страж локально ok) — и
  четыре обрыва novac-стражей по стенам 300/600с, которых у main в тот же
  день не было (те же стражи 17-19с); повторный прогон подтвердил средовость:
  на кандидате **8006c43cccd4202dbc8c55f3e1798722217749ae** все 9 джобов
  success (nova-gate 22m0s, run 40218724750; windows-process-acceptance
  40218724786; crate-tests 40218724825; nova-test-regression 40218724783;
  contracts-z3 40218724815; contracts-crosscheck 40218724823; nova-lint
  40218724741; nova-doc 40218724851; nova-driver-graph 40218724764), и
  `python scripts/guards/check-push-proven-by-ci.py
  8006c43cccd4202dbc8c55f3e1798722217749ae` → `ok: 7/7 required workflows
  green on 8006c43cc`. Строка LANDED — после вливания приёмщиком
  (замок/merge/accept — не исполнителя, crew_help «ПРИЁМКА»).
- commits: сообщения на английском, Signed-off-by, без Co-Authored-By;
  `git add` по именам, коммит с `--only -- <файлы>`, проверка маркеров
  конфликта в той же команде.
