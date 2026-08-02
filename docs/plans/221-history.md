<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# История плана 221 — архив

> Архив дословных текстов секций, свёрнутых в [221-release-v0-1.md](221-release-v0-1.md) в
> компактные таблицы при ревизии 2026-08-02 (владелец: «есть ощущение, что 221 устарел»).
> Ничего не потеряно — полные исходные формулировки атомов и порядков исполнения здесь,
> дословно, как они были в плане на момент закрытия/свёртки. Текущее состояние — только в
> самом 221; этот файл не обновляется задним числом.

## Ф.0 — Ноль багов очереди (гейт входа в релизную фазу) — полный текст {#ф0}

*Свёрнуто в компактную таблицу в 221 при ревизии 2026-08-02. Все атомы ниже — ✅ ЗАКРЫТЫ
2026-07-21/22, критерий Ф.0 достигнут 2026-07-21.*

- [x] **A-B1 ✅ 2026-07-21** Текущие волны влиты: 196-closeout (честный реестр, 196 дальше в
      фоне) · 217 (D432 + фикс TcpStream-регрессии хойста) · Linux-гонка (known_red снят) ·
      named-default-arg-shift. Всё в пушах 8e843e2ac/181b32e41.
- [x] **A-B2 ✅ 2026-07-21** Ш4-снос conv.h+kill-switch+D422-амендмент+примеры §4-§6 — влит
      (один путь = `*_display_spec`; остатки честные: pad user-типов + ptr-debug).
- [x] **A-B3 ✅ 2026-07-21** Ш2: перенос примитив-тел — ВЛИТ (план 208 «один путь» закрыт
      целиком; V1-упрощение #3 снято). A-B3a (write-collision, file-anchored types) — тоже ✅ влит.
- [x] **A-B4 ✅ 2026-07-21** box-vtable P2 `[emit_c/vtable]` (sonnet, worktree `nova-boxvt`,
      ветка `p-fix-box-vtable`, коммит `847cdbc84`) — `[M-protocol-box-callarg-vtable-incomplete]`
      РЕШЁН (тот же корень закрыл и «latent protocol-box» из A-B6 ниже — одна запись backlog,
      два пункта плана). Root: (1) `fn_protocol_params` регистрировался только внутри `emit_fn`
      без пре-пасса — caller, эмитящийся раньше callee, пропускал бокс даже для generic-протокола;
      (2) `protocol_type_args` безусловно возвращал `None` для NAMED non-generic протокола, хотя
      `type_ref_to_c` уже лоуэрил такой параметр/возврат в `NovaBox_<Proto>` — CC-FAIL на call-arg
      И return. Фикс: пре-пасс + `protocol_type_args` теперь `Some((proto, vec![]))` для
      non-generic + 2 box-имя-форматера поправлены (без хвостового `_`). Фикстура
      `spec_tests/conformance/m221_protocolbox_callarg_ok.nv` (3 позитивных теста). Гейты:
      conformance мега-CU PASS 130/FAIL 0/SKIP 18; флагман `--strict-effects` build чист, test
      9 PASS/1 SKIP (1 RUN-FAIL `aggregate.nv:45` — подтверждён pre-existing timing-флаки,
      standalone PASS ×2, не связан с protocol-кодом). Отдельно найден (НЕ чинился, вне зоны
      атома) `[M-protocol-embed-vtable-missing-method]` — use-embed протокол теряет
      embedded-метод в vtable-struct (другой корень: codegen не зеркалит checker-side
      `flatten_protocol_methods`).
- [x] **A-B5 ✅ 2026-07-21** net-утечка-b free-on-close — ВЛИТ (refcount §9, утечка −87%,
      риск-гейты UAF зелёные).
- [x] **A-B6 ✅ 2026-07-21 ЦЕЛИКОМ** Мелочь P3/P4: d55-const ✅ (гибрид: module-level const
      []u8 — прямая эмиссия bytes-вызова в lazy-init; scope-local — честная диагностика
      E_CONST_BYTES_NOT_CONSTEXPR) · oot-дефисы ✅ · generic-match-scope-gap ✅ (carrier
      current_fn_generics + резолв 0-arg метода по протокол-баунду только для match-bindings) ·
      latent protocol-box ✅ (вместе с A-B4). Гейт после вливания: 517/19/0 + флагман.
- [x] **A-B7 ✅ 2026-07-21** 216-defer-хвосты — ВЛИТЫ (Err-пейлоады + tuple-пейлоады
      consume-enforce, спек-амендмент 05-memory; record-пейлоад = узкий followup
      `[M-216-record-payload-consume]`).
- [x] **A-B8 ✅ 2026-07-21** d216/write_at паника — ВЛИТ (корень: Block-арм infer_expr_c_type
      без пре-регистрации let-локалов + stale плоский var_types; folder-CU 517/0 после фикса).
- [x] **A-B9 ✅ 2026-07-22 (влит целиком: handler-тела→общий путь, #default_handler D431, Time→std.time, typed-опы)** — исходная формулировка: 🔨 2026-07-21 (поздний А-класс, решение владельца: «API эффектов не менять
      после v0.1»)** [175.2](175.2-typed-effects.md) typed effects ОДНИМ окном (полный план — в файле подплана): handler-тела → обычные fn
      через протокол-машинерию (снос особого эмиттера emit_handler_lit) + Time-эффект →
      std.time + типизация опов (sleep(Duration)/now()->Timestamp/now_monotonic()->Monotonic).
      Корень: `[M-effect-handler-body-record-literal]`; детали — 221.1 Ф.2б. + Расширения владельца 2026-07-21: механизм #default_handler (дефолты эффектов из C-хардкода в .nv, лениво до main) И ретракция ambient-статуса Time (D62): Time в сигнатурах как все эффекты — «без магии». Волна в полёте
      (sonnet, worktree nova-typedfx). БЛОКЕР ТЕГОВ.
      - **A-B9 частично ✅ 2026-07-22 влито:** handler-тела→общий-путь + #default_handler (D431).
- [x] **A-B10 ✅ 2026-07-22 (v3 влит 65ba4fb90; остаток Mem/TimerMetrics-vtable → после тегов, 196)** — исходно: ОБЯЗАТЕЛЬНЫЙ ДО ТЕГОВ (решение владельца 2026-07-22: «v0.1 ждёт typed-эффекты»)**
      Эффект-рефактор 175 Ф.2-v3 (продолжение A-B9): (1) снос рукописных C-vtable Time/Mem/
      TimerMetrics из nova_rt (Fail остаётся хардкодом — сильно встроен); (2) typedef value-record
      перед effect-vtable (снимает scalar-bridge — идея владельца); (3) типизация опов, sleep-канон
      = ТОЛЬКО метод `d.sleep()` (D9 одна дверь, свободная sleep() убрана); (4) РЕТРАКЦИЯ ambient
      Time (D62) — Time обязателен в сигнатурах как Fs/Net, миграция std+examples+conformance до
      нуля. Волна `p175-effect-refactor` (sonnet). **v3 ✅ ВЛИТ (65ba4fb90): typed Time,
      2-pass emission снял 4× барьер, ambient честный, sleep=метод.** Mem/TimerMetrics-vtable
      остаток → после тегов (196).
- [x] **A-B11 ✅ 2026-07-22 (Ф.2-v4 влита: П1-П10 — D434 полная декларация handler-опов, priv nanos, Time в folder-модуль time.duration, #default_handler без аргумента, real_time, time_*-extern, sleep(Duration), тесты в пир-файлах; П11 typed local_offset ОТКАЧЕН — CU-wide inference баг [M-offset-result-mono-bleed-if-let], после тегов)** — исходно: 🔨 эффект-API-полировка (Ф.2-v4, находки владельца вычиткой 2026-07-22) ДО ТЕГОВ**
      [175.2 Ф.2-v4](175.2-typed-effects.md): (1-3 ✅ ветка p-fx-polish: extern `time_*`-нейминг,
      sleep(d Duration)-тип, вынос тестов в пир-файлы). Остаток (новая волна): (4) handler-опы
      полная декларация обязательна (`now() -> Timestamp =>`, E-ошибка, ~78 сайтов); (5) priv nanos
      Monotonic/Timestamp/Duration; (6) перенос Time-эффекта+default-handler в folder-модуль
      time.duration (priv-доступ handler'у, БЕЗ публичного from_ns; возможно т.к. ambient снят);
      (7) #default_handler без аргумента (вывод из -> Effect[X]); (8) нейминг time_default→real_time
      (симметрия real_fs). Список ОТКРЫТ (владелец продолжает вычитку) — финал ОДНОЙ волной.
      БЛОКЕР ТЕГОВ.
- [x] **Критерий Ф.0: ✅ 2026-07-21 ДОСТИГНУТ** — исходная очередь пуста; CI без known_red.
      Пост-Ф.0 добавка из A-S4 (не в исходном критерии, но релиз-скоуп): for-in cross-package
      (nova-http, 5 CU) + git-кэш гонка раннера — волна в полёте, финал-гейт A-R1 после неё.

## Ф.1 — Стабилизация: A-S1..A-S5 — полный текст {#ф1}

*Свёрнуто в компактную таблицу в 221 при ревизии 2026-08-02. Все атомы — ✅ 2026-07-21.*

- [x] **A-S1 ✅ 2026-07-21** Полный `nova test` Windows — батчи 1-5 ПРОГНАНЫ 2026-07-21
      (std целиком: collections/checksums/encoding/math/text/unicode 32/0 ·
      io/fs/path/time/os/crypto/data/identifiers 24/0 · runtime/concurrency/testing/prelude
      11/1 · net/ffi/sort/text 2/0 · examples+flagship-regressions). Находки: (1)
      `std/src/testing/handlers/core` mut_clock auto-idle-advance — ДЕТЕРМИНИРОВАННЫЙ
      RUN-FAIL 3/3 → фикс-волна В ПОЛЁТЕ (бисекция: пачка-2026-07-21 или давний);
      (2) `spawn_capture_value_struct` — RUN-FAIL под конкурентной нагрузкой батча,
      изолированно 3/3 PASS → флака-подозрение (гонка под нагрузкой?), перепроверка
      ×10 на тихой машине в A-R1. Обе находки ЗАКРЫТЫ: mut_clock — корень в тест-раннере
      (folder-модуль читал ENV-директивы только из алфавитно-первого peer; влит+запушен);
      флака — на перепроверку A-R1. nova_tests-baseline: 313 каталогов 4 батчами — 169 PASS,
      12 компайл-фейлов = документированное легаси (STATUS.md: retired-API str.from и пр.,
      non-blocking с 2026-07-11, миграция Plan 198) — НЕ релиз-блокеры.
- [x] **A-S2 ✅ 2026-07-21** WSL conformance 517/0/19 (полный tally, актуальное дерево).
- [x] **A-S3 ✅ 2026-07-21** loadtest полный: PASS 68/0, все 7 блоков (включая concurrency-80 c честным shedding и детерминизм seed=42).
- [x] **A-S4 ✅ 2026-07-21** tls зелёный (slow-лейн требует --timeout 300 — конфиг, не дефект) · compress зелёный · http 5/0 после for-in+ErrorKind фиксов.
- [x] **A-S5 ✅ 2026-07-21** WSL net-slope --full 4/0 (stream_leak с refcount-фиксом в допуске).

## Ф.4 — A-R1 финал-прогон — полный текст {#a-r1}

*Свёрнуто в компактную таблицу в 221 при ревизии 2026-08-02.*

- [x] **A-R1 ✅ 2026-07-21** Финал-прогон на релизном состоянии (2ac6e708c): Windows
      мега-CU 527/0/55 · WSL Linux конформанс 527/0/55 · CI github success (вкл.
      examples anti-rot шаг) · spawn_capture ×10 = 10/10 (флака-подозрение A-S1 снято:
      была деградация среды под нагрузкой) · 5 CI-целей built. Release notes
      отревизированы (vsix прилагается).

## Сводный порядок исполнения (задачи + баги, зафиксирован 2026-07-21 вечером) {#порядок-2026-07-21}

*Устарело — заменено разделом «★ ЕДИНЫЙ ПЛАН К РЕЛИЗУ (2026-08-02)» в 221. Оставлено здесь
как исторический снимок планирования того дня.*

Приоритеты багов — триаж А/Б/В в [221.1](221.1-bug-sweep.md). Сводная очередь:

**Такт 1 — СЕЙЧАС (блокеры, класс А):** literal-fit D433 (🔨 доработка) → финальная
пачка мёржей (spawn-cleanup ✅ + ErrorKind ✅ + literal-fit) → пересборка → большой гейт
(мега-CU + ВСЕ 5 CI-целей + nova-http сьют) → пуш → CI зелёный.
∥ ПАРАЛЛЕЛЬНО (не ждут): Б1 diag-104.10 🔨 · Б2 d78-dup 🔨 · Б5 проверки-репро 🔨.

**Такт 2 — стабилизация (после зелёного CI):** A-S3 loadtest полный [флагман] ∥
A-S5 GC-slope [WSL] ∥ Б3 freefn-arity [types] (затем Б4 vec-spelling-cap — та же зона).
+ A-E2: examples-гейт шаг в nova-gate.yml (лист готов).

**Такт 3 — релиз:** A-R1 финал-CI на релизном коммите (оба ОС; + spawn_capture ×10 на
тихой машине; + все examples strict-effects) → ревизия release notes (vsix «tracked
separately» снять, limitations актуализировать) → **A-V6: теги v0.1.0 на 4 репы
согласованно (nova → tls/http/compress), артефакты на GitHub Releases (zip + vsix),
мёрж www-ветки → страница live** → анонс (текст — владелец).

**Такт 4 — после релиза (класс В + фон):** 221.1-Ф.2/Ф.2а/Ф.2б/Ф.3 программой
«1-2 волны в день» (как 196): d424 → compound-assign → d376 → d289 → 198-семья →
старые P2 → 196-остатки (B11q «имя→баунды») → архитектурные (loc_for_span, m221-отбор)
→ 217.1 раскатка → 214-реализация → роадмап-ревизия PARTIAL ≤170.

### Матрица параллельности (2026-07-21)

СЕЙЧАС ЗАПУЩЕНО: A-B1 (4 волны) + A-V1/A-V2 (version+zip) + A-W1 (www-страница).
СЛЕДУЮЩИЕ СЛОТЫ: A-S1 (полный тест, фоновые Bash) → A-Q1/Q2 → A-D1 (docker) → A-V3/V4 → A-Q3.
Зонная развязка: доки/скрипты/www не пересекаются с баг-волнами вообще; A-B2..B6 —
последовательность по зонам emit_c/types.

### Оценка (после декомпозиции)

Ф.0 ~2 дня · Ф.1 хвост ~1 день (A-S1 гонится уже сейчас) · Ф.2+Ф.3 — параллельно Ф.0
(готовы к моменту Ф.0-выхода) · Ф.4 — полдня. **Реалистично: ~4-6 рабочих дней до v0.1.0**
(было ~1.5 недели — параллелизация доков/дистрибуции срезает хвост).
