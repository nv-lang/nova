# Task 116 — итоговый отчёт (STOP / NOT ACCEPTED)

## Решение

Задача доказала bounded Carina-only путь от pinned опубликованного seed к
зафиксированному `novac/src/main.nv`, same-volume publication и три ограниченные
CLI smoke-команды. Действующее уточнение scope закрепил интегратор 2026-10-23:
«Уточнить scope 116» было делегированным выбором оркестратора, а не прямой
цитатой или release-решением владельца. Текущий deliverable — сдать доказанный
путь, не выдумывать novac class-fix ради формы. Исходные критерии сохранены в
`doc/tasks/116-carina-seed-compatibility/task/message.md`
как история. Измеренные gaps остаются gaps: исходный 092 refusal не
воспроизведён, extern-call probe отсутствует, class controls не доказаны.
Общий статус — **STOP / NOT ACCEPTED** до DCO, независимой приёмки и
exact-candidate CI. Исходный STOP 092 ограничивал worker 116; по отдельному
owner GO в 13:40 интегратор получил разрешение на один bounded круг после нового
pinned preflight. Этот круг не запускался worker 116.

По решению, переданному владельцем через оркестратора по его делегации, полные generated C (13,250,302 байта), опубликованный AMD64 `novac.exe`
(4,389,888 байт), seed ZIP/EXE и cache binaries исключены из Git по решению
владельца. Их размеры и SHA-256 записаны в receipts, и их байты были
независимо проверены во время задачи. Если session scratch будет удалён,
повторная побайтная проверка этих исключённых артефактов станет невозможна.
Offline harness проверяет только включённые receipts и доступные inputs; он не
утверждает, что повторно проверил отсутствующий C/PE/seed.

## Матрица критериев 1–7

| № | Вердикт | Evidence и граница |
|---|---|---|
| 1. Baseline, seed и исходный отказ | **Частично — gap** | Зафиксированы main `86af6bb82963148784868edb6e02e20aee6330de`, источник `novac/src/main.nv` SHA-256 `3104bbe6b6deea8995d49dc78a7f36fe9096bb6437eeb937b41426d247e19da8`, 565-file source/driver manifest, неизменённые seed ZIP/EXE hashes и SOURCE-SHA `3729abcce42ec2d94efc898ec6eafe91ff6210d5`. Direct seed diagnostic реально сохранил и локально проверил 13,250,302-byte C raw stdout SHA-256 `25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0`, exit 0; этот вывод не переиспользован в driver build. Исходный 092 refusal в 116 не воспроизведён; сохранённое prior evidence не подменяет повторяемое воспроизведение. По делегированному ответу владельца через оркестратора C bytes не приложены: receipt хранит только размер/hash, после удаления scratch byte-level revalidation недоступна. См. `receipts/baseline/` и `receipts/manifests/`. |
| 2. Carina-only compiler chain; no Oracle/fallback | **PASS в пределах выполненного stage** | Фактический child — неизменённый pinned Carina seed 0.2.1, `emit novac/src/main.nv`, exit 0; правильный Rust orchestration driver имеет проверенный compiler-free graph и stamp. Независимые build receipts подтверждают отсутствие Oracle execution, compatibility fallback и reuse direct-probe C. Историческое происхождение опубликованного seed не стирается: gate называет его `A built by published Oracle 0.1.0`; это baseline artifact, а не Oracle compiler, запущенный для текущего source stage. См. `receipts/build/`, `receipts/audits/`. |
| 3. C/native compiler и native smoke | **Доказано в пределах названного source snapshot** | Same-volume build сформировал C с SHA-256 `25c5404f…` и опубликовал AMD64 PE с SHA-256 `8287b9dd…`; независимый post-build audit подтвердил publication. Один разрешённый CLI smoke прошёл все три случая: usage (exit 2), `check stdout_hello.nv` (exit 0, оба потока пусты), absent absolute `.nv` (exit 2, точная cannot-read line); `native_exe_executed=true`, 3 подтверждённых старта, retry 0. Результат ограничен этим snapshot и smoke; C/PE bytes исключены и не перепроверяемы из Git. См. `receipts/smoke/` и build receipts. |
| 4. Controls измеренного класса | **Частично — gap** | Сохранены runner RED/GREEN probes для capture-control removal, launch GC override, same-volume predicate и raw EOL matcher; harness проверяет named refusals на in-memory synthetic evidence. Эти probes не доказывают class controls для измеренного language/compiler compatibility gap и не запускают compiler/Oracle. Критерий не требует именно language-level fixture, но положительные/отрицательные class controls остаются недоказанными. См. `receipts/probes/`, `receipts/smoke/pure-test-*`. |
| 5. Host/target и FFI contracts | **Частично — gap** | Проверенный target — `windows-x86_64` / `x86_64-pc-windows-msvc`; `NOVA_TARGET_OS` unset. GC/`atomic_ops` overrides выбирают только явные native inputs, не подменяют host/target. В этой задаче не добавлено `E_*`/`W_*`; отдельный targeted extern-call probe не выполнялся, поэтому поддержка действующего extern call не доказана. См. build/native manifests и launch receipts. |
| 6. Независимая pre/post stage provenance и caches | **Частично** | Независимый audit подтвердил actual argv/cwd/env/status, published AMD64 hash, 565 source/driver inputs, 58 runtime files, 469 libuv files, 49 cache outputs, backup 4 prior `carina-build` files и post-smoke rehash всех 1551 pinned task-092 inputs (`missing=[]`, `changed=[]`). Оговорки: сравнение task-092 ограничено pinned inputs, не всем dirty worktree; seed launch metadata — opt-in allowlist, не полный inherited environment (#1897); failure-before-`Command::output`-return не создаёт child-specific receipt (#1896); исключённые C/PE bytes после утраты scratch повторно не валидируются. См. independent audit receipts. |
| 7. DCO, независимая приёмка, exact-candidate CI/check-push-proven | **Не выполнено** | Первый DCO handoff commit `0e7b8e8678509d4fe22b0fb20657d606b3f5506f` был сдан; текущая packaging rework включена отдельным DCO commit на `task/116-carina-seed-compatibility`. Независимая приёмка и exact-candidate GitHub CI/check-push-proven остаются впереди; CI не запускался исполнителем. По проверке интегратора, main `86af6bb82` имеет success у `nova-gate` run `39887889269` и обязательных workflows; старый watch был dropped/code null — исторический incident, не текущий блокер main. Это не заменяет CI для exact candidate. Номера 1895–1898 назначены интегратором; baseline `rows=1806`, `max=1898` обновлён только как учёт строк. Никакие violation/R2 baselines не повышались. |

## Открытые строки и релизная классификация

- **№1895 (K2/P2, OPEN):** cross-volume output publication отказала с Windows OS error 17. Успешная same-volume попытка — другой carrier, не class fix. Oracle tag: не блокирует. Влияние на Carina 0.3 в рамках 116 не оценено; отдельного решения о допустимости для выпуска нет.
- **№1896 (K2/P2, OPEN):** `Command::output()?` возвращает до opt-in capture при spawn failure/раннем завершении; child-specific launch/status receipt на этой ветке отсутствует. Успешный child capture не закрывает failure-path класс. Oracle tag: не блокирует. Влияние на Carina 0.3 в рамках 116 не оценено; отдельного решения о допустимости для выпуска нет.
- **№1897 (K2/P2, OPEN):** capture metadata записывает известный NOVA/NOVAC allowlist, но child наследует иные env-переменные, которые в receipt не перечисляются. Не установлено, что они изменили фактический результат. Это provenance gap, отдельно от №1896. Влияние на Carina 0.3 в рамках 116 не оценено; отдельного решения о допустимости для выпуска нет.
- **№1898 (K2/P2, OPEN):** captured Python probe под `.py` suffix был классифицирован source hygiene guard как runnable source и вызвал check-mixed-eol RED, хотя это byte-pinned receipt. В этом пакете путь переименован в `.py.bin` без изменения исходных bytes; это carrier change, не общее закрытие класса. Влияние на Carina 0.3 в рамках 116 не оценено; отдельного решения о допустимости для выпуска нет.

## STOP и дальнейшие границы

Новых product class-fixes в `novac/` или `std/` в этом rework не делалось; ранее согласованный opt-in capture в driver не закрывает №1896/1897. Не запускать Oracle, C/PE из bundle, новые source stages, `emit`, build, B2/fixed-point или CI без отдельного решения. Published seed не менялся; prior temporary EXE не исполнялся. Отдельное owner GO 13:40 разрешило интегратору один bounded круг 092 после нового pinned preflight; worker 116 его не запускал. Main CI RED относится к историческому incident: по сообщению интегратора, `nova-gate` run `39887889269` и обязательные workflows для main `86af6bb82` завершились success. Release impact строк №1895–1898 не оценён в задаче 116; отдельного решения о допустимости этих находок для Carina 0.3 нет.

## Portable harness checks

`python harness.py self-test` прошёл пять named refusal cases in-memory: wrong
seed pin, wrong source pin, receipt hash mismatch, missing required output, и
Oracle impostor; Oracle execution count равен нулю. `python harness.py verify`
исторически прошёл на LF checkout по 94 доступным receipts, 565 source inputs и
трём smoke cases. На Windows `core.autocrlf` может дать checkout-sensitive
`REFUSE_SOURCE_HASH`: raw working-tree CRLF bytes сравниваются с сохранённой LF
source manifest; mismatch не нормализуется и не выдаётся за универсальный
portable PASS. Независимая проверка Git blob для
`nova-driver/src/carina_build.rs` подтвердила 29,054 bytes / SHA-256
`f6970454f74cdd4b507bf998777646cf287966b7ebb1119d26100763024695`, совпадающие
с source manifest; Windows checkout measured 29,859 bytes / SHA-256
`7f8c4e05cbb03378c4bcc34cc12916fa84207971600a4e400bfc1a035987c083` after CRLF
materialization. Отсутствующие C/PE/seed bytes не проверялись; acceptance
verdict не выносился. Exact current Windows verify output:
`REFUSED REFUSE_SOURCE_HASH: nova-driver/src/carina_build.rs` (exit 1); это
checkout/EOL limitation, не RED compiler/native stage.

Эти проверки подтверждают только проверяемые receipts и отказные контроли
самого verifier; они не повышают gaps исходного 092 refusal, extern probe или
class controls до PASS.

## Формат отчёта по `window-brief-template.md` §6

**РЕПРО:** подтверждён локально до rename и независимым CI. На exact SHA
`0e7b8e8678509d4fe22b0fb20657d606b3f5506f`, `nova-gate` run `39946416356`, tier
`push [rest]` и `loop` сообщил RED в `check-mixed-eol`; локальная полная-tree
проба дала один offending `.py` raw receipt. Разрешённая ограниченная scratch
модель воспроизводит тот же scanner RED на byte-identical receipt, не запуская
compiler/native stages. Its exact RED line is
`receipts/smoke/pure-test-native-cli-smoke-executor-pure.py CRLF=1 bare-LF=29
first-anomaly-line=1`; GREEN on the actual worktree is recorded below.

**ТОЧКА:** `scripts/guards/mixed-eol-scan.py:24–25` включает `.py` в `EXTS`;
captured raw probe ошибочно считался активным Python source.

**ФИКС:** переименование `receipts/smoke/pure-test-native-cli-smoke-executor-pure.py`
в `receipts/smoke/pure-test-native-cli-smoke-executor-pure.py.bin`; manifest
path/role обновлён. Исходные 2,376 bytes и SHA-256
`c1d488a579e1501803d78404d1d22ff8009fe3a8d939e09fdb51451c620db8ee` сохранены.
Добавлена открытая запись №1898 и обновлён только registry row-count baseline.
Guard и violation/R2 baselines не ослаблялись и не повышались.

**ФИКСТУРЫ:** `.py.bin` — raw evidence artifact, не исполняемая fixture;
существующие harness self-tests — только in-memory refusals. Product/class
fixtures не добавлялись; compatibility class controls остаются NOT PASS.

**САБОТАЖ:** RED до rename, точный вывод:
`check-mixed-eol: FAIL — файлов со смешанными окончаниями строк: 1`;
`pure-test-native-cli-smoke-executor-pure.py CRLF=1 bare-LF=29
first-anomaly-line=1`. Независимый CI RED — `nova-gate` run `39946416356`, SHA
`0e7b8e8678509d4fe22b0fb20657d606b3f5506f`, tiers `push [rest]` и `loop`.
GREEN после `.py.bin` rename:
`check-mixed-eol ok: смешанных окончаний строк в рабочем дереве нет`.
Raw bytes/hash не переписаны; точные RED/GREEN outputs уже приведены здесь и
независимо наблюдались приёмщиком; дополнительных receipt-файлов нет.

**std/src ДО/ПОСЛЕ:** не применимо / не применимо: rework касается raw receipt
filename, packaging metadata, отчёта и registry bookkeeping; `std/src` не
затронут.

**ВЕТКА/КОММИТ:** `task/116-carina-seed-compatibility`; evidence commit
`318f59885d5c6732f06960f678b1609d655b76b3`. SHA текущего cleanup commit
указан во внешних task submission metadata, без self-reference в файле.

**ЧТО НЕ СДЕЛАНО:** raw receipt не нормализован; новые source/native stages,
emit/build, Oracle, 092, seed, CI/push и full gate не запускались. Checkout-
sensitive `REFUSE_SOURCE_HASH` на Windows `core.autocrlf` остаётся limitation;
полные C/PE/seed bytes после утраты scratch не перепроверяемы. Не закрыты
исходный 092 refusal, class controls, extern-call probe, env/failure capture
gaps и OPEN №1895–1898; release decision отсутствует.

Отдельный non-required CI факт: Windows job `p1418` имел timeout; его не
перезапускали и timeout не меняли. Он не подменяет и не расширяет описанный
check-mixed-eol candidate blocker.
