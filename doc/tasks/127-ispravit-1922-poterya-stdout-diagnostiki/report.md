# Отчёт задачи 127 «Исправить №1922: потеря stdout-диагностики сида»

Основа: свежий origin/main `aaf59cb76` (fetch перед началом; локальная main не
использовалась). Коммит задачи: `66d9f542a`.

## РЕПРО

1. Запись №1922 в `docs/plans/221.1-bug-sweep.md` прочитана на свежем main.
   Воспроизведение задачи 119 (папки 119 в дереве нет — замер жил в scratch
   сессии 119; реестр хранит итог: 52 носителя с пустым хвостом) повторено на
   носителе `examples/getting_started.nv`.
2. Прямой запуск pinned-сида (env как у драйвера): `rc=1`, stdout 1052 байта
   (JSONL `E_NOVAC_SUBSET`), stderr 0 байт — улики
   `docs/plans/repro/1922-carina-emit-stdout-diagnostics/seed-stdout-getting_started.txt`,
   `seed-stderr-getting_started.bin`.
3. `nova-driver\target\release\nova.exe build examples\getting_started.nv` на
   main: `nova: Carina B1 emit failed (exit 1): {"id":"d0","code":"E_NOVAC_SUBSET",...}`
   (`driver-before-main.txt`).

## ТОЧКА

`nova-driver/src/carina_build.rs`, ветка `if !emitted.status.success()`.
Актуальное состояние: задача 120 уже закрыла половину класса (stderr пуст →
берётся stdout), поэтому 52 носителя с пустым stderr на main уже именуют
причину. Живая дыра класса: при НЕПУСТОМ stderr stdout-диагностики сида
отбрасывались. Вторая копия обвязки в `nova-cli/src/carina_build.rs` — мёртвый
код (`#[allow(dead_code)]`, вызывающих нет, вытеснена `cmd_build_oracle`,
задача 092) — носителем не является, не тронута.

## ФИКС

Функция `emit_failure_detail(stdout, stderr)` в `nova-driver/src/carina_build.rs`:
stderr-only → stderr; stdout-only → stdout; оба потока → оба текста ровно по
одному разу (`{stderr}\nseed stdout: {stdout}`); совпадающие потоки не
дублируются; оба пустые → пусто. Ветка «success но нет C» уже несла оба потока —
без изменений. Семантика языка не меняется — D-блок не применим.

## ФИКСТУРЫ

Пять юнит-тестов в `nova-driver/src/carina_build.rs` (`emit_failure_detail_*`),
утверждают наблюдаемое поведение (точный текст детали, счётчик вхождений
каждого потока = 1) и зовут проверяемую функцию. `cargo test` в `nova-driver`:
`test result: ok. 18 passed; 0 failed`
(`docs/plans/repro/1922-carina-emit-stdout-diagnostics/unit-tests-green.txt`).
Фикстуры .nv не добавлялись: проверяемое поведение — текст ошибки драйвера на
потоках дочернего процесса, живёт на уровне Rust-юнитов; интеграционный полюс
закрыт прогоном штатного драйвера на реальном носителе.

## САБОТАЖ

Хелпер временно сведён к stderr-only (исходный дефект):

* юнит, дословно: `test result: FAILED. 16 passed; 2 failed` — красные
  `emit_failure_detail_shows_stdout_jsonl_when_stderr_is_blank` и
  `emit_failure_detail_keeps_both_streams_without_duplication`;
* интеграция, дословно (`sabotage-red.txt`):
  `nova: Carina B1 emit failed (exit 1): ` — пустой хвост, подпись №1922.

Возврат фикса: `test result: ok. 18 passed; 0 failed`; драйвер
(`driver-after-fix.txt`):
`nova: Carina B1 emit failed (exit 1): {"id":"d0","code":"E_NOVAC_SUBSET",...}`.

## std/src

`nova check std/src` бинарём основного дерева
(`D:\Sources\nv-lang\nova-opencode\nova-cli\target\release\nova.exe`,
компилятор и std правкой не затронуты), ДО и ПОСЛЕ — одна строка дословно:
`PASS: 162  FAIL: 26  WARN: 67` (26 FAIL — негативные фикстуры
`std/src/encoding/serde_neg/`, ожидаемо).

## ВЕТКА/КОММИТ

Ветка `task/127-ispravit-1922-poterya-stdout-diagnostiki`, коммит `66d9f542a`,
запушена в origin. Реестр: строка №1922 обновлена фактурой и закрыта
(«ЗАКРЫТА в ветке задачи 127 2026-10-30, ожидает приёмки интегратором»);
`registry-routes.baseline`: blockers 117 → 116 со строкой хроники. Стражи
зелёные: `check-registry-routes` (116==116), `check-registry-closure-kept`,
`check-registry-past-tense-status`, `check-registry-rows-intact` (1837==1837),
`check-registry-entry-shape` (343<=343), `check-registry-single-verdict`,
`check-error-rewrap-chain`, `check-carina-no-oracle-build`, гигиена коммита.

## ЧТО НЕ СДЕЛАНО

* Полный прогон всех 118 примеров не повторялся (мини-свип нашёл 3 носителя
  за первые ~40 файлов; класс доказан на одном носителе + юнитах).
* Смешанный вывод на живом сиде не воспроизводился: pinned B1 печатает
  диагностики только в stdout; смешанный случай покрыт юнит-тестами.
* `docs/plans/274.16` (строка 70) и `docs/dev/carina-release-notes-v0.3.0.md`
  (строка 42) всё ещё называют №1922 блокирующей тег — снятие блокера с тега
  остаётся за интегратором после приёмки; готовность релиза этим исправлением
  не заявляется.
* Гейт/CI на кандидате `integrate/t<N>` — шаг приёмки интегратора.
