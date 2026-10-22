# Задание: Совместимость Carina seed для выпуска 0.3

Заказчик: интегратор Новы; принимает независимый приёмщик проекта Nova.
Репозиторий: nova-opencode, ветка task/116-carina-seed-compatibility. Дата задания: 2026-10-22.
Источник: измеренный отказ pinned seed в 092, решение владельца о раздельной совместимости и прямое поручение оркестратора 18:11 запустить согласованную работу без нового вопроса владельцу.

Быстрый путь. Основа — origin/main (`12ac789aa712b9682126ba56e5de846d507efca8`). Номер 115 остаётся за отложенным разделением корпуса; 116 — отдельная работа, не продолжение или снятие STOP 092.

## Чего хочет владелец

- «совместимость отдельно» (владелец, 2026-10-22, чат оркестратора; подтверждённый relay 15:13).
- «основная задача - выпустить 0.3 как можно скорее, не нужно тратить время на правки, которые нас не приближают к 0.3 оптимальным образом» (владелец, 2026-10-22, чат оркестратора; relay 16:42).
- «Карина 0.3 - это цель по интегратору Новы, др проекту идут по своим путям, например, Алула и CrewHarness» (владелец, 2026-10-22, чат оркестратора; relay 16:44).

## Что нужно

1. Сначала зафиксировать точный baseline: source main, опубликованный seed и фактические входы сборки. Различить main, committed baseline 092 и dirty snapshot 092; не называть неизменённый относительно ветки носитель доказанным baseline seed refusal. В первую очередь проверить минимальный Carina-only путь, а не реализовывать все неподдержанные формы языка заранее.
2. Прочитать существующие решения о сборке/FFI/host и target OS, исходники принятого compiler-free драйвера 111, затем сохранённые receipts 092. Назвать класс совместимости и минимальный путь до реализации. Если нужен новый язык или иной bootstrap policy — STOP с точным противоречием и scope, не обход.
3. В своей ветке получить минимальный проверяемый Carina-only путь от неизменённого опубликованного Windows seed 0.2.1 к актуальному source Карины. Если нужен промежуточный compiler, его Nova source, std/runtime и происхождение должны быть явными, а сам он собран только уже доказанным Carina compiler. Исправлять только измеренный класс совместимости, сохраняя существующие host/target и FFI-контракты.
4. Проверить штатный путь через правильный `nova-driver/Cargo.toml` из принятого 111, не compatibility `nova-cli`. До source execution независимый приёмщик сверяет source/graph/binary/stamp, actual seed child, cwd/env, manifest и новый output. Точная команда и raw stdout/stderr/status каждого child сохраняются, включая отказы.
5. Дать минимальный native smoke и негативные контроли для доказанного пути, затем передать интегратору проверенный bridge/compiler и его receipt. Если следующая ступень невозможна без запрещённого расширения — измеренный STOP; не выдавать blocker или Rust unit tests за B1/B2 GREEN.
6. Сдать класс-фикс и необходимые fixtures/evidence с DCO и обычной независимой приёмкой. Найденные дефекты внести в 221.1 как №TBD тем же слиянием; номер назначает интегратор. Для продолжения 092 передать входы и доказанную границу, а не запускать её работу самостоятельно.

## Критерии приёмки

1. Источник и исходный refusal воспроизводимы: exact source/diff/untracked hashes, full input manifest без missing/extra, pinned ZIP SHA256 `7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb`, seed exe SHA256 `824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4`, seed SOURCE-SHA `3729abcce42ec2d94efc898ec6eafe91ff6210d5`. Обе raw streams и exit сохранены. Нулевая работа не PASS.
2. Каждый source compiler в принятой цепочке — проверенный Carina executable, восходящий к pinned seed; нет Oracle parser/checker/emit/judge, подменённого compatibility CLI и скрытого fallback. Сборка Rust orchestration driver отдельно от исполнения Nova source compiler; dependency graph правильного driver не содержит nova_codegen.
3. Получены C и native compiler актуального названного source snapshot либо строго назван промежуточный достигнутый этап. Принятая сдача требует доказанного совместимого пути, а не только промежуточного компилятора; неполный результат остаётся STOP/NOT ACCEPTED. Native smoke имеет D-linked stdout/stderr/exit и запускает именно полученный бинарь.
4. Для измеренного класса есть положительный сосед, отрицательная fixture и RED при сломанном условии с восстановленным GREEN; не ослаблены исходные tests/goldens. Неверные seed/source/hash, отсутствующий output и подставленный Oracle дают именованный отказ, а не успех на старом файле.
5. Host OS не заменена выбранным target/env/константой. Поддержка существующего extern call проверена по действующей спецификации. Не появились независимые формулы applicability или дополнительные sem exports ради 092. Новые E_*/W_* имеют line-pinned nova:expect.
6. Независимый приёмщик подтверждает source-to-binary/seed-child соответствие ДО нового stage, actual launch metadata после него, cache provenance отдельно, полные manifest comparisons и отсутствие повторного использования старых Oracle C/exe. Для затронутого compiler/std сохранить целевые before/after проверки; недоступный результат назвать недоступным.
7. Для приёмки и landing — exact candidate от свежего main, обязательный GitHub CI и check-push-proven без ложного GREEN и обходов. Известный main commit-refs RED не скрывается и не разрешает менять baseline в этой задаче.

## Результат и куда он попадает

Нет: результат — код минимального класса совместимости в `novac/` и его разрешённых fixtures; воспроизводимый harness и постоянные receipts — в `docs/plans/repro/116-carina-seed-compatibility/`; дефекты — в `docs/plans/221.1-bug-sweep.md`. Действующие build/provenance указания обновляются только в относящихся к этому пути существующих документах. Папка задачи хранит постановку и отчёт, не становится источником норм языка.

## Связи

- Зависит от: принятый compiler-free драйвер 111, source main `12ac789aa712b9682126ba56e5de846d507efca8`; неизменённый опубликованный Carina seed 0.2.1.
- Блокирует: разрешённое только после доказанного Carina-only пути возобновление bounded круга 092 и актуальную цепочку компилятора для 0.3.
- Пересекается (файлы): compiler-пути `novac/` с остановленной 092; её worktree и dirty bytes не изменять. Конкретный edit set назвать интегратору до правок.
- Начинается после: штатного backlog 109/110/114, освободившего один слот; inflight остаётся 5. Новой папки предыдущих legacy-задач нет — их номера указаны как исторические связи, не выдуманные пути.

## Границы

- Не менять Rust Oracle, не исполнять его для Nova source compilation/comparison/acceptance, не чинить его долги. Compiler-free driver не доказывается одним именем `nova.exe` или stamp.
- Не менять опубликованные 0.2.1 ZIP/exe/tag/assets, не публиковать новый seed, bridge или релиз. Промежуточный бинарь остаётся evidence до отдельного решения владельца.
- Не подменять host default target OS, окружением или константой; не редактировать generated C вручную, не подсовывать stub host bridge и не ослаблять fixtures/guards/ratchets.
- Не возобновлять и не редактировать 092, не реализовывать её три source-переписи или cursor fix как незаявленное расширение 116. Номер 115 и массовый перенос корпуса не занимать.
- Не менять административные записи 112/113, project limits или настройки ради обхода. Не чинить посторонний commit-refs debt и naming-polish.
- Не запускать worker full nova test/mega-CU/локальный полный gate. Только необходимые targeted проверки; тяжёлые команды через machine queue. Scratch исключительно в session scratchpad; старые receipts и Oracle incident 092 сохраняются.

## Открытые вопросы

- Есть ли минимальный путь через уже поддерживаемые текущим compiler формы без расширения языка? Сначала измерить и выбрать самый узкий путь по spec; при конфликте — STOP, не новый seed или Oracle fallback.
- Три dirty subset carriers 092 и неизменённый относительно её HEAD host bridge не доказывают четыре независимых дефекта main. Их применимость к исходному baseline должна быть установлена до реализации.
- Compatibility proof не закрывает Linux raw fixed point, multi-module/MSVC, пакетную матрицу или весь релиз 0.3; эти критерии и STOP остаются самостоятельными.

## Признаки маршрута

Быстрый путь выбран владельцем для отдельной задачи совместимости; независимая приёмка, stage provenance, sabotage и точный CI не отменяются. Full route отдельно не разрешён. Формальное слово запуска и его исходный relay фиксируются плагином без выдуманных цитат.

Перечень источников: `AGENTS.md`; `docs/dev/novac-architecture.md`; действующие spec/decisions об FFI и D99/D495 при их применимости; `docs/plans/274.16-carina-all-examples-and-packages.md`; `nova-driver/`; `nova-driver-support/`; `docs/dev/carina-seeds/windows-x86_64.json`; `scripts/tools/carina-gate.py`; существующие receipts 092.

## Сохранённые входные свидетельства

- 092 source HEAD: `0e2a3e742eb6f2294535521f629d53084a90d194`; dirty diff SHA256 `525989d317437436130010bba95e29f1255fa4f65ea006835deaaf2a1d6cd243`; fixture SHA256 `a95f448997f2c40aa933d505ca2565c4a1ead3be2337a59886b9973459958e26`.
- Correct preflight: `C:/Users/Евгений/AppData/Local/Temp/opencode/ses_mvaj1spd3ef13fyfyk/task92/preflight111-corrected/verdict.json`.
- Correct source-stage watch: `1792639411998-6r6zhq`, exit 1 до C. Единственная direct probe: `D:/Temp/opencode/ses_mvaegvbn6jn817f0qh/task92/round11/direct-emit-diagnostic-06-28/receipt.json`, stdout.bin содержит четыре E_NOVAC_SUBSET, stderr пуст; receipt SHA256 `bcb1323774ab6c720fc558ccee6910d7e86ecb4f293eeef1854a1493ad1e60aa`.
- Предыдущие `1792636149377-ott1bu` и `1792636714161-qc6sv8` исполняли stale Oracle CLI и НЕ являются Carina stage proof; их C/exe не запускать и не переиспользовать.
