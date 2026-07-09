# Continuation — точка возобновления работ (обновлено 2026-07-09)

> Живой файл: перезаписывается в конце каждой сессии. Промпт внизу — вставить
> в начало новой сессии, когда появится лимит. Source of truth статусов —
> этот файл + [README.md](README.md) таблицы + [backlog-followups.md](backlog-followups.md) маркеры.

## HEAD на момент паузы
`main = 06d6c8bbe` (эталон conformance **70/0**, `nova test --positive --compile-error spec_tests/conformance`).
Всё закрытое — запушено в github/main.

## Директива владельца (базовая, неизменная)
Завершить планы 172-186 целиком, без упрощений, по конвенциям, автономно, вперёд.
**Первая очередь = 172-184; вторая = 185-186.** Семья 173 — **крайне важна** (приоритет).
Отвечать по-русски. Экономия лимитов: haiku=механика-по-списку (детальный рецепт,
запрет main, «суб-агентов не спавнить»), sonnet=исполнение-по-карте, opus=только
разведка корней. Модель писать в отчётах. Мониторить фоновых (server-обрывы/ребуты —
резюм «оцени git log/status, продолжай с места»; checkpoint-коммиты обязательны).
Гейты гонит главная сессия через Bash, не агенты.

## Закрыто в сессии 2026-07-07..09 (всё в main)
- **172.12** целиком (A1-A8 Vec-канон, снос NovaArray/array.h, коллапс триплификации −2476 строк).
- **172.13** долг (4 батча, ~30 компиляторных корней канально; эталон 66→70/0).
- **173.0** субстрат supervised (per-slot child_error[], serialized decision-loop + hook, пиннинг SpawnCtx).
- **173 Ф.2** (подтверждена готовой), **Ф.0R + Ф.3** (D414 §1-3: precedence PANIC>USER>CANCEL — фикс нарушения D13; enforce Detach-эффекта E_DETACH_REQUIRES_EFFECT; select None-арм).
- **174.1** §1а-система конверсий замкнута (to_int/to_str-семьи, char.to_str, доменные str@to_complex/to_version/to_url/to_cron; 17 деклараций/10 ретракций/~135 миграций).
- **174.5** маркер-часть (read/write голый deref, *_unaligned, from_bits/to_bits чистым .nv, &-as парсер).
- **176** целиком (Ф.4/Ф.5: NetError→IoError-проекция, io.Read/Write на TcpStream, D302).
- **D316-дрейф** nova_tests/concurrency 21/21 (Time.now→Monotonic.elapsed_since; разблокировал валидацию 173-тестов).
- Конвенции записаны: §1а (4 направления конверсий), §18а (срезы-виды), §21б (Type.new+дефолты), §4а module-conventions (типизированные C-хендлы CFooHandle), D99 host_style под #cfg, «платформенные константы бинаря — не эффект» (module-conventions §0).
- Промоушен std/_experimental 32/35.

## В работе на момент паузы (фоновый агент — проверить первым делом)
- **173.1** [sonnet, worktree /d/Sources/nv-lang/nova-p173, ветка parallel-collect-173-1] —
  parallel for → []T + supervised-как-значение. Получил готовое репро parallel_for CC-FAIL
  (результат Vec[int] в nova_unit-переменную; nova_tests/concurrency блокирована этим).
  ПРИ ВОЗОБНОВЛЕНИИ: `git -C /d/Sources/nv-lang/nova-p173 log --oneline -5` + status —
  если завершил, принять (merge→rebuild→гейты→push); если оборван — резюм с места.

## Очередь (по приоритету владельца)
1. **Семья 173 до конца** (приоритет): принять 173.1 → **173.2** supervision-as-effect
   (hook уже ждёт в nova_supervised_decide) → **173.3** data-race-freedom → Ф.5/Ф.6-хвосты.
2. В окна компилятор-зоны между шагами 173: **174.5-схема** (docs/plans/174.5-pointer-ops-methods.md:
   read_at/write_at/offset/dist/volatile методы + ретракция `p[i]`/`*p`/`p+i` ошибками; PROPOSED,
   решения владельца 2026-07-06 в таблице §3); **[M-lazy-const-init-race]** P1 (eager
   nova_consts_init() до спавна воркеров); ревизия **172.1.1** (вероятно почти закрыт A7/A8).
3. **185-MVP** (реестр правил под `nova check --lint` + 3-5 образцовых: W_NONVARIADIC_OF/
   W_RETIRED_PREFIX/W_FFI_BARE_HANDLE; архитектура финальная, nova lint-команда потом) → **186**
   (D412 x"…"/embed + свип литералов-простыней [haiku]).

## Заморожено (по директивам/лимитам — размораживать по слову владельца)
175.1 civil-time (масштаб java.time) · 172.14 value-ABI perf (P3, решение владельца 04.07) ·
constraint-ядро 172.13 (Ф.0-Ф.3 архитектурный атом) · [N]T-на-стек · _experimental-хвосты
(toml/semver_range; linkedlist — зона агента владельца Option[Self]) · TLS-116 (нужна
Ф.0-актуализация под 183-net) · журнал ч.66 · широкий GC/TLS-аудит хендлеров · полный nova lint.

## P1-маркеры в backlog (открытые, с репродукциями)
[M-lazy-const-init-race], [M-consume-rebind-nested-block-shadow], [M-result-direct-recursive-enum],
[M-option-self-recursive-record-mono] (зона агента владельца), [M-sched-park-concurrent-fs] закрыт.

---

## ПРОМПТ ДЛЯ НОВОЙ СЕССИИ (вставить целиком)

Продолжаем планы 172-186 (директива: целиком, без упрощений, по конвенциям, автономно,
по-русски, экономия лимитов — haiku механика/sonnet карта/opus разведка, модель в отчётах,
мониторь фоновых с резюмом-с-места, суб-агентов не спавнить, haiku не в main, гейты гоню сам).
Прочитай docs/plans/CONTINUATION.md — там полный статус на main=06d6c8bbe (эталон 70/0).
ПЕРВЫМ ДЕЛОМ: проверь фоновый агент 173.1 (worktree nova-p173, ветка parallel-collect-173-1) —
git log/status; если завершил — прими (merge→rebuild оба крейта→conformance 70/0 + err173_0 +
std/http/io/fs→push), если оборван — резюм с места. Дальше по очереди CONTINUATION.md:
семья 173 (173.2 supervision-as-effect → 173.3) — ПРИОРИТЕТ; в окна 174.5-схема, P1 lazy-const;
потом 185-MVP → 186. Заморозки не трогать без слова.
