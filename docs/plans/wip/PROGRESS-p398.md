<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# PROGRESS — окно p398-cancel-direct-body

**Дефект:** реестр 221.1 №398 (переоткрывший №224,
`[M-supervised-cancel-no-interrupt-parked-accept]`) — `supervised(cancel:
tok)` не прерывает прямую (без `spawn`) блокирующую операцию тела; D442
заявлял покрытие «ВСЕГО блока», для `cancel:` это было неправдой.

**Итог:** ЗАКРЫТ. Корень найден, пофикшен в двух местах, покрыт матрицей
позитив/негатив, сабботаж-пробой подтверждён, спек-амендмент внесён
(D442 «Границы»), реестр/backlog обновлены. Попутно найден и
задокументирован (не пофикшен — отдельный класс, вне бюджета окна) НОВЫЙ
баг: `Channel.recv()` прямо в теле под `cancel:` вешает процесс навсегда.

---

## 1. Затык со сборкой проб — решён

Брифовые env-пути были НЕВЕРНЫ (взяты из главной репы `nova`, но указывали
на несуществующий `vcpkg_installed` в её корне): `gc.h`/`gc.lib`
фактически лежат под `compiler-codegen/vcpkg_installed/...`, не под
корневым `vcpkg_installed/...`. Как только пути поправлены (см. ниже),
трассы `[p398] ...` печатались с первой попытки — гипотезы про
rt-archive-cache/пересборку nova-cli/env-путь на разных стадиях сборки
из брифа НЕ подтвердились, задача была тривиальнее.

Верные env для этого worktree:
```
NOVA_RT_DIR=d:/Sources/nv-lang/nova-p398/compiler-codegen/nova_rt
NOVA_GC_LIB_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib
NOVA_GC_INCLUDE_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
```

## 2. Трассой найден корень

Трассы (все временные, полностью откачены — `git diff` по
`fibers.h`/`nova_sched.h`/`driver.c`/`driver.h` сейчас содержит ТОЛЬКО
рабочий фикс, ноль строк с `p398`/`NOVA_P398_TRACE`, проверено grep'ом
перед каждым коммитом) показали:

```
[p398] tok.cancel t=... bound_scope=... (не NULL)
[p398] deliver q=... owner_scope=... owner_slot=0
[p398] cancel-via-driver scope=q armed_sleeps_head=NULL   <- q's список ПУСТ
[p398] slot-cancel scope=owner_scope slot=0
[p398]   found cb=0 hdl=0                                 <- pending_stop_cb ПУСТ
[p398] handle-arm-sleep(driver-thread) ms=3000 cancel_scope=owner_scope <- !!! sleep НЕ под q
```

**Диагноз:** direct-body `Time.sleep` под M:N (driver уже стартован —
т.е. в программе БЫЛ хотя бы один `spawn` раньше по времени) арминг идёт
через `_nova_sleep_via_driver`, чей `cancel_scope` резолвится через
`NovaSpawnCtxBase::_nova_parent_scope` ТЕКУЩЕЙ выполняющейся coroutine —
для прямой (без нового `spawn`) операции это ВСЕГДА тот же fiber, что
исполнялся ДО входа в `supervised(cancel:)`, чей `_nova_parent_scope`
зафиксирован в момент СВОЕГО СОБСТВЕННОГО spawn'а и НЕ обновляется при
входе во вложенный `supervised{}` (согласовано с D439's design — тот же
факт, на котором стоит `owner_scope`/`owner_slot`). Итог: сон реально
армится под `owner_scope`'s `armed_sleeps_head`, а `nova_scope_deliver_
cancel`'s `_nova_cancel_via_driver(q)` смотрит `q`'s (ЧУЖОЙ, пустой)
список — ровно диагноз, который D442's «Границы» уже сформулировал
дословно, но никто не реализовал.

**Второй, изначально неучтённый факт (найден этим окном):** ТОТ ЖЕ
разрыв бьёт `timeout:`/`deadline:`, не только `cancel:` — но ТОЛЬКО под
M:N (driver уже запущен). `_nova_early_dl_timer_cb` (early-armed timer,
единственный механизм, способный прервать direct-body операцию ДО того,
как join-loop вообще стартует) звал ТОЛЬКО `nova_sched_cancel_pending_
slot` — тот же недостаточный вызов. Проба интегратора «timeout: 200 →
прерван за 489мс» не была ошибочна: её стенд не имел ни одного `spawn`
до замера → driver не стартовал → `Time.sleep` шёл ЛЕГАСИ
`_nova_sleep_via_libuv`-путём, который `nova_sched_cancel_pending_slot`
(fix №165) УЖЕ покрывал. С driver'ом (обычный случай реальной M:N
программы) `timeout:`/`deadline:` были сломаны ТАК ЖЕ, как `cancel:` —
подтверждено пробой B4 (ниже): до фикса 3024мс, после 220мс.

## 3. Фикс

Новый driver-job `NOVA_DRV_JOB_CANCEL_SLOT` (`compiler-codegen/nova_rt/
driver.h`+`driver.c`, `_nova_driver_handle_cancel_slot`) — таргетированный
по `(scope, slot)` аналог `CANCEL_SCOPE`: walk `armed_sleeps_head` того
scope'а, но CAS/close ТОЛЬКО запись, чей `slot` совпадает — соседние
операции ТОГО ЖЕ (возможно внешнего, долгоживущего) owner-scope не
трогает. Submit-обёртка `_nova_cancel_via_driver_slot` (`fibers.h`) —
зеркалит `_nova_cancel_via_driver`'s lifetime-контракт
(`pending_driver_jobs` инкремент/декремент, безопасно т.к. `owner_scope`
— предок текущего стека). Вызывается из ДВУХ мест:
- `nova_scope_deliver_cancel` (закрывает `cancel:`), сразу после
  существующего `nova_sched_cancel_pending_slot(q->owner_scope,
  q->owner_slot)`;
- `_nova_early_dl_timer_cb` (закрывает `timeout:`/`deadline:` под
  driver-режимом), сразу после его собственного такого же вызова.

Код по `docs/dev/mn-coding-conventions.md`: identity-not-index (действует
на ЧТО БЫ ни было валидно зарегистрировано в слоте на момент вызова, не
кэширует «ожидаемый» указатель); single-mutator (driver-thread-only walk
списка, как у существующего `CANCEL_SCOPE`-обработчика); тот же
lifetime-паттерн (`pending_driver_jobs`), что уже проверен и задокументи
рован для `_nova_cancel_via_driver`.

## 4. Матрица приёмки (план 221 п.11)

Легенда: ✅ покрыто и зелено · ⚠️ покрыто, но НЕ через №398 (пред-
существующий механизм) · 🆕 найден НОВЫЙ баг, задокументирован отдельно
· — физически не покрыть в этом окне (причина в тексте).

| Операция | Размещение | `cancel:` | `timeout:` | `deadline:` | `cancel:`+`timeout:` |
|---|---|---|---|---|---|
| `Time.sleep` | прямо в теле | ✅ фикс №398 | ✅ фикс №398 | ✅ фикс №398 | ✅ фикс №398 (обе гонки: cancel-раньше, timeout-раньше) |
| `Time.sleep` | через `spawn` | ✅ регресс, УЖЕ работал | ✅ регресс (`supervised_deadline_test.nv` #2/#3) | ✅ регресс (`supervised_deadline_test.nv` #4) | ✅ регресс (`supervised_deadline_test.nv` #6a/#6b) |
| сетевой `accept()` | прямо в теле | ⚠️ УЖЕ работал (fix №165, `nova_sched_cancel_pending_slot`, НЕ через armed_sleeps_head) | ⚠️ УЖЕ работал (тот же механизм) | не отдельно протестировано в этом окне (тот же механизм, что timeout:) | не отдельно протестировано (тот же механизм) |
| сетевой `accept()` | через `spawn` | ⚠️ УЖЕ работал (до №165 тоже) | ⚠️ УЖЕ работал | — | — |
| сетевой `read()` | любое | — | — | — | — | физически не покрыть в этом окне — блокер №390 (ОС не постит завершение), тот же класс что и в брифе |
| `Channel.recv` | прямо в теле | 🆕 НОВЫЙ баг: infinite hang (НЕ №398 — подтверждено сабботажем СВОЕГО фикса, hang воспроизводится и БЕЗ него). Маркер `[M-supervised-cancel-direct-body-recv-hang]` заведён, не чинится этим окном | не проверено (та же парковка, тот же ожидаемый hang) | не проверено | не проверено |
| `Channel.recv` | через `spawn` | ✅ регресс, работает (elapsed ~105мс) | не проверено в этом окне (вне scope №398 — recv не парковано на armed_sleeps_head) | не проверено | не проверено |
| `join` (ожидание `spawn`-детей) | — (нет прямой формы: join — implicit) | ✅ покрыто КАЖДЫМ spawn-тестом выше (нет отдельной прямой формы — join = implicit-ожидание конца блока, «разместить прямо в теле» бессмысленно) | ✅ то же | ✅ то же | ✅ то же |

**Негативы (стоячее требование владельца 2026-08-06), все реализованы
как отдельные фикстуры (см. §5):**
- нет отмены → sleep досиживает полностью (`no cancel — completes fully`,
  x2: direct-body и spawn-placed);
- тело укладывается в timeout-бюджет → `TimeoutError` НЕ летит
  (`within timeout budget`);
- `cancel()` ПОСЛЕ выхода из области → no-op, не паника (`after scope
  already exited`);
- чужой/несвязанный токен не трогает область (`unrelated token cancel`);
- двойная отмена идемпотентна (`double cancel() is idempotent`, ПОЗИТИВ
  — не падает, elapsed корректен);
- `Channel.recv`: значение уже в канале → отмена никогда не проверяется,
  не throw'ит (`value sent before cancel`).

## 5. Фикстуры

`std/src/concurrency/supervised_cancel_direct_body_test.nv` (НОВЫЙ файл,
14 test-блоков) — `nova test` (тот же worktree, тот же toolchain):
```
PASS: 1  FAIL: 0
```
Повторено 5 раз подряд — 5/5 зелёных (после структурной правки порядка
тестов, см. §6 «Гонка между тестами» — settle-паузами это НЕ чинится
надёжно, чинится порядком).

Регресс существующего `std/src/concurrency/supervised_deadline_test.nv`
(весь — spawn-based, 10 сценариев + №169 nested-deadline) — 3 прогона:
```
PASS: 1  FAIL: 0   (x3)
```

`nova check std/src`: **PASS: 149  FAIL: 26  WARN: 61** — канон
148/26/61 из памяти + 1 (новый файл, PASS) = ровно ожидаемо, ноль
регрессий по счёту.

`accept()`-фикстура (`std/src/net/supervised_cancel_accept_test.nv`,
существующая, из №165) — **`nova test std/src/net` физически не
запустить в этом worktree**: блокирована НЕСВЯЗАННЫМ pre-existing
CC-FAIL (`addr.c`/`std/src/net` целиком — `initializing 'nova_unit' with
an expression of incompatible type 'NovaRes_...'`), воспроизводится и
БЕЗ моих правок (только nova_rt-заголовки трогал, C-codegen для std/net
не менял). Проверено НАПРЯМУЮ standalone-пробой (`nova build`, не через
`nova test`) — см. §7, b2.nv — сетевой `accept()` под `cancel:` работает,
elapsed ~217мс (было 227мс в брифе интегратора — совпадает по порядку).

`cargo build --release` (nova-cli) — **Finished, exit 0**, 2m49s,
только пред-существующие warnings (unused var/dead code в main.rs/
smt-модуле, не связаны с этим окном — я не трогал ни строки Rust).

## 6. Гонка между тестами (найдена и закрыта СТРУКТУРНО, не settle-паузами)

`_nova_cancel_via_driver_slot`/`_nova_cancel_via_driver` доставляют отмену
АСИНХРОННО (отдельный driver-thread). `nova test` компилирует ВЕСЬ .nv-
файл в ОДИН бинарь — тесты идут последовательно в ОДНОМ процессе, на
одной и той же стековой глубине (тот же адрес `owner_scope`, тот же
индекс `owner_slot`). Запоздавшая доставка job'а от теста N может
попасть в операцию теста N+1, если N+1 успел заармить СВОЮ операцию на
том же (owner_scope, owner_slot) идентити раньше, чем доставилась job N.

Воспроизведено эмпирически (settle-пауза 300мс НЕ помогла надёжно —
3/3 повторных прогона с паузами всё равно ловили ложное срабатывание
в `within timeout budget`). Настоящий фикс — СТРУКТУРНЫЙ: все тесты,
которые ТОЧНО проверяют «отмены не было» (негативы), поставлены ПЕРВЫМИ
в файле, ДО того как хоть один тест успел отправить асинхронную job;
все тесты, которые ОТПРАВЛЯЮТ такую job (позитивы), поставлены
ПОСЛЕДНИМИ — после них в файле ничего чувствительного к запоздалой
доставке уже нет. 5/5 прогонов после реструктуризации — зелено.

Это НЕ новый класс гонки — тот же residual race, что D442's
«Гонко-анализ» уже документирует для сиблинг-механизма
(`nova_sched_cancel_pending_slot`/early-deadline timer): «the remaining,
narrower residual is the early-deadline timer firing LATE... after
owner_slot was reused by an unrelated LATER fiber». Memory-safe по
построению (тот же CAS-guard паттерн, что `_handle_cancel_scope`) —
худший исход это спурious ранняя отмена НЕСВЯЗАННОЙ ПОЗЖЕ работы, не
корраптится память. Не отдельный маркер — то же явление, что уже
принято/задокументировано.

## 7. Проба «подсунь заведомо негодное» (обязательный шаг приёмки)

Обе строки `_nova_cancel_via_driver_slot(...)` (в `nova_scope_deliver_
cancel` и в `_nova_early_dl_timer_cb`) закомментированы (`// SABOTAGE-
TEST (p398): ...`), пересобрано, прогнан `supervised_cancel_direct_
body_test.nv`.

**Вердикт дословно:**
```
RUN-FAIL       std/src/concurrency/supervised_cancel_direct_body_test  #
  FAIL: direct-body sleep: deadline: (absolute) wakes it early under M:N driver (#398) — supervised_cancel_direct_body_test.nv:237: assert failed: elapsed < 2000 |
  FAIL: direct-body sleep: cancel:+timeout: — cancel fires first (#398) — supervised_cancel_direct_body_test.nv:268: assert failed: elapsed < 2000 |
  FAIL: direct-body sleep: cancel:+timeout: — timeout fires first (#398) — supervised_cancel_direct_body_test.nv:285: assert failed: elapsed < 2000 |
  FAIL: double cancel() is idempotent (#398 regression guard) — supervised_cancel_direct_body_test.nv:302: assert failed: elapsed < 2000

PASS: 0  FAIL: 1
```
Новые позитивные фикстуры КРАСНЕЮТ без фикса (ровно ожидаемая ассерция
`elapsed < 2000` не держится — операция без фикса не прерывается рано,
досиживает полную длительность). Сабботаж отменён, обе строки
восстановлены дословно, пересобрано, весь файл снова зелёный
(`PASS: 1 FAIL: 0`, см. §5).

Отдельно подтверждено сабботажем: НОВЫЙ найденный баг (§8, Channel.recv
hang) НЕ вызван этим фиксом — тот же infinite hang воспроизводится и с
ОТКЛЮЧЁННЫМ `_nova_cancel_via_driver_slot` (пробовано ДО того, как решил
не включать эту фикстуру в тестовый файл вовсе — hang сломал бы `nova
test` CI).

## 8. Побочная находка — НОВЫЙ баг, НЕ входит в этот фикс

`supervised(cancel: tok) { spawn { sleep(50); tok.cancel() }; rx.recv()
}` (recv строго прямо в теле, БЕЗ обёртки в СВОЙ `spawn`) — **вешает
процесс НАВСЕГДА** (не 3000+мс, infinite hang, killed only by external
timeout). Подтверждено НЕЗАВИСИМЫМ от фикса №398 (см. §7). ТА ЖЕ форма,
только `rx.recv()` обёрнутый в СВОЙ `spawn { rx.recv() }` — работает
штатно (elapsed ~105мс).

Гипотеза корня (НЕ подтверждена глубже — вне scope №398, который про
`Time.sleep`): wake-before-park race между `nova_sched_register_pending`/
`nova_sched_park_with_unlock` (recv держит channel-мьютекс до самого
park) и синхронным `_nova_channel_waiter_stop_cb`, вызванным с ДРУГОГО
(canceller) потока через `nova_sched_cancel_pending_slot` — тот же
класс, что driver-sleep's «Race 2a/2b»/futex-park фиксы чинили для
`Time.sleep`, но `recv()` использует ОБЩИЙ `nova_sched_park_with_unlock`,
не свой специализированный futex-park.

Маркер `[M-supervised-cancel-direct-body-recv-hang]` заведён в
`docs/plans/backlog-followups.md` — НЕ чинится этим окном (другой
класс/файл, требует собственного расследования с state-dump по образцу
`docs/dev/mn-coding-conventions.md`/`reference-mn-race-case-study.md`).
Фикстура-позитив НЕ добавлена в тестовый файл (сломала бы `nova test`
CI зависанием) — задокументирована комментарием в файле фикстур с
точным репро.

## 9. Изменённые файлы

- `compiler-codegen/nova_rt/driver.h` — `NOVA_DRV_JOB_CANCEL_SLOT` +
  `cancel_slot` union-член.
- `compiler-codegen/nova_rt/driver.c` — `_nova_driver_handle_cancel_slot`
  + dispatch-case.
- `compiler-codegen/nova_rt/fibers.h` — `_nova_cancel_via_driver_slot` +
  forward-decl + два call site'а (`nova_scope_deliver_cancel`,
  `_nova_early_dl_timer_cb`).
- `std/src/concurrency/supervised_cancel_direct_body_test.nv` — НОВЫЙ,
  14 test-блоков, матрица §4.
- `spec/decisions/06-concurrency.md` — D442 «Границы» дополнена
  амендментом 2026-08-07 (фикс + сужение остатка).
- `docs/plans/221.1-bug-sweep.md` — №398 ЗАКРЫТ, №224 ЗАКРЫТ (был ложно
  переоткрыт этим же окном по факту №398 — теперь настоящее закрытие).
- `docs/plans/backlog-followups.md` — `[M-supervised-cancel-no-
  interrupt-parked-accept]` ЗАКРЫТ; НОВЫЕ строки заведены:
  `[M-supervised-direct-sleep-timeout-silent]` (сужена, остаётся
  открытой — «marker to add at close» из D442, наконец добавлен) и
  `[M-supervised-cancel-direct-body-recv-hang]` (НОВЫЙ баг, §8).

**НЕ вошло в слияние (проверено grep'ом перед каждым коммитом):** ни
одной строки с `p398`/`NOVA_P398_TRACE`/`SABOTAGE-TEST` — все временные
трассы и весь сабботажный код полностью откачены до финальных коммитов.

## 10. Что НЕ входит в этот фикс (осознанно, отдельные маркеры)

1. `[M-supervised-direct-sleep-timeout-silent]` — прерванный direct-body
   `Time.sleep` теперь корректно ПРОСЫПАЕТСЯ рано (этот фикс), но не
   throw'ит сам (пост-wake проверка смотрит `cancel_scope->cancel_
   requested`, где `cancel_scope` = owner_scope, не `q`) — тело
   статически ПОСЛЕ прерванной операции ещё исполняется. Внешний
   `supervised` тем не менее ловит cancel/timeout корректно и в срок —
   прикладной наблюдатель СНАРУЖИ блока разницы не видит, но код МЕЖДУ
   прерванной операцией и концом блока успевает выполниться. Не входило
   в измеримый критерий №398 (elapsed время внешнего блока).
2. `[M-supervised-cancel-direct-body-recv-hang]` — §8, НОВЫЙ, отдельный
   класс (Channel, не Time.sleep).
3. сетевой `read()` под `cancel:`/`timeout:` — блокер №390 (ОС не
   постит завершение), не пересекается с №398's механизмом,
   физически не покрыть в этом окне.
