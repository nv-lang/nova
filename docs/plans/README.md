# Планы Nova

В этой директории — только **планы** (что и когда делаем). Справочные
материалы (таблицы сравнений, research-заметки, бенчмарки) живут в
[docs/research/](../research/).

> **Открытые followup'ы** (`[M-…]`-маркеры): живой project-wide OPEN-view —
> [backlog-followups.md](backlog-followups.md) (только актуальное). Plan-bound детали — в Followups
> своего плана; полная история — в [../simplifications.md](../simplifications.md). Конвенция: [AGENTS.md](../../AGENTS.md).

## ⚡ Актуальное состояние (снапшот 2026-07-13)

Быстрый вход для нового агента — [docs/promts/read-project.md](../promts/read-project.md). Сводка:

- **196 (одно окно) — высший приоритет; статус 2026-07-13 (вечер): стадия-1 ~70%, стабильность ДОСТИГНУТА.**
  Канал node_substs + композиция POST-mono влиты (промахи B1/B2 → единицы); реестр `infer_call_ret_c`
  **114 → 48** (волны 1-3 + census [196.5-stage-d-census]); 196.6: три первопричины нестабильности убиты
  (плавающий AV/b11x-флейк = мисскомпиляция auto-derive + рейс worker-sweep + утечка override-карт) —
  **гейт 468/0 строго зелёный**. Остаток до closeout: 45 живых веток (6 replay-доказанных → волна-4 идёт),
  фасеты B/C/D матрицы (callnorm/argbind, единый FnDecl-резолв), структурный финал-гейт + `->data`-греп.
  Точечный dispatch-фикс: [196.7 — method-dispatch через resolved_callees](196.7-method-dispatch-resolved-callees.md)
  ✅ ЗАКРЫТ 2026-07-15 (фасад `[]u8 @to_str` мис-диспатч по имени → канал+receiver-тип; снят обход `decode_utf8`,
  маркер `[M-174.1-to-str-name-collision-codegen-bug]` закрыт).
  Точечный dispatch-фикс: [196.8 — primitive receiver bounded blanket](196.8-primitive-receiver-bounded-blanket.md)
  ✅ ЗАКРЫТ 2026-07-16 (BOUNDED-бланкет `[T Ints]` на примитивном ресивере — `i64.checked_add` — мис-диспатч в
  concrete-коллизию чужого типа; новый регистр `type_set_members` для D310 type-set bound в guard'е Plan 164 Ф.3;
  маркер `[M-primitive-receiver-bounded-blanket-dispatch]` закрыт; попутно найден+залогирован
  `[M-i64-clamp-primitive-collision-dispatch]`, отдельное окно).
  Точечный dispatch-фикс: [196.9 — concrete-vs-concrete на разных примитивах](196.9-primitive-concrete-overload.md)
  🔧 В РАБОТЕ 2026-07-16 (два CONCRETE `@clamp` на `int`/`f64`, `i64`-ресивер без своего оверлоада тихо мис-диспатчился
  в `f64` через pattern-bound `match`-биндинг; корень — `f1_expr_inner`'s `ExprKind::Match` не расширял `scope`
  биндингами арма, из-за чего `check_instance_overload` вообще не видел ресивер → ни диагностики, ни
  `resolved_callees`; фикс переиспользует `match_arm_bindings` (172.1 АТОМ 2a) — ОДНО окно чинит и диспетч-по-типу
  (196.7 канал), и честный `[E_UNKNOWN_METHOD]` (177 Ф.3) разом; маркер `[M-primitive-concrete-overload-receiver-dispatch]`).
- **187** — Ред.5-v2 готова к запуску Ф.MVP-2: ВСЕ внешние гейты сняты (TLS=nova-tls, 173 закрыт, SSE в main); демо = живой Nova-бек, канон показа Docker; предложена Ред.6-пятёрка витринных улучшений.
- **173** ✅ семейство закрыто 2026-07-13 (MultiError D414 + propagation-trace per-fiber + suppressed явным параметром); остаток: п.4 semaphore-cap (P3, опция) + [M-173-trace-not-in-child-error] (P3).
- **193** ✅ закрыт (std/tls → внешний dep `../nova-tls`, ноль Rust в TLS-пути); хвост — vendored mbedTLS.
- **198** ✅ Ф.2 REDO ЗАКРЫТ 2026-07-13: корпус мигрирован в spec_tests по D307 (merged-CU 2585 блоков PASS; гейт 468/0+12skip); вечно-красные в fixtures/known_red; остаток Ф.4c = 9 классов компиляторных находок (198-redo-notes).
- **201** ✅ ЗАКРЫТ 2026-07-13 (consume-блок D188 v1/v2/multi-var/v3/v3.1 + @share/refcount в nv + M-178 прямой move в consume-поле D133; спек-амендменты в тех же слияниях).
- **202** ✅ ЗАКРЫТ 2026-07-13 (path-keyed реестр модулей + root peers D78 rev-4 + миграция nova-tls; [M-d78-duplicate-decl-module-swallow] снят).
- **200** — живой реестр: П1/П2/П4/П5/**П7** (scalar→str `@to_str`, str.from убран, влито) ✅; П6 (Vec.data→ptr) ⏸️ на паузе (ABI-правка); **П8 → [Plan 208](208-unified-formatter.md)** (unified Formatter — дизайн); П3 (As*) — в Q.
- **200.1** — [скорость `nova test std`](200.1-std-test-speed.md) 📋 согласован 2026-07-13: папочные CU для std-тестов + кеш + профиль медленных; после 196/198.
- **203** ✅ ЗАКРЫТ 2026-07-13: http = публичная nv-lang/nova-http (root peers, module-path прежний), std самодостаточен; +2 фикса резолвера.
- **204** ✅ ЗАКРЫТ 2026-07-13 (D420): 03.x уже дал git+semver+lock+резолвер; дельта = [replace]-секция + W_DEP_PATH_NO_RELEASE + lock-семантика (replace не течёт в lock); nova-http на git-форме v0.1.0 с lock в репе.
- **194** — [модель исполнения контрактов: `#debug` + `--contracts`](194-contract-execution-model.md) ✅ СОГЛАСОВАН 2026-07-14 (сверка против D81/D24/Plan-140): `#unchecked` РЕТРАКТ, `debug_assert`→`#debug assert`, три режима checked|optimized|verified, bounds/overflow=always-on-safety; готов к очереди на реализацию.
- **206** — [арифметическая политика: 5 исходов из 1 overflow-примитива](206-arithmetic-overflow-policy.md) ✅ ЗАКРЫТ 2026-07-15 (D423): trap-дефолт sized-int + `@overflowing_*` интринсик (Ф.1/Ф.1b) + `.nv`-бланкеты checked/saturating/wrapping на `Ints` (Ф.2) + Duration/Timestamp D317-миграция (Ф.3); conformance 470/0; в main. Остаток 206.1 (`unchecked_*`) — отдельный план.
- **207** ✅ ЗАКРЫТ 2026-07-15 — [`compare_exchange` возвращает свидетеля](207-atomic-cas-witnessed-value.md) (bool → `Result[(), T]`, D425 amends D168 §1); все 13 CAS-методов (`AtomicI8..I64`/`U8..U64`/`Isize`/`Usize`/`Ptr`/`Bool`/legacy `AtomicInt`); private `@cmpxchg` intrinsic + plain-`.nv` wrapper (без hand-written Result C); codegen-фикс `RUNTIME_DEFINED_TYPES` NamedTuple-схема (emit_c.rs); закрывает `[M-cas-return-witnessed-value]`; conformance 150/0.
- **205** — [компрессия из nova_rt → nv-lang/nova-compress](205-compress-out-of-nova-rt.md) 📋 согласован 2026-07-13 (nova_rt = только рантайм; brotli 7МБ уезжает пакетом по школе nova-tls; после гейтов 203).
- **152.7.2** — [формат-контекст в Display (D419) + интерполяция прямо-в-sink](152.7.2-format-context.md) 🔨 в работе 2026-07-13 (Fmt-протокол, `#`=pretty, str.from уходит из движка интерполяции).
- Гейт: conformance (мега-CU 2585 блоков + корпус) **468/0 + 12 SKIP** (2026-07-13); язык-меняющее — только со спек-амендментом в том же слиянии.

## Схема нумерации

- `01-…`, `02-…` — главные планы по порядку создания.

## std-library (навигация по модулям)

Концептуальная группировка std-планов (физически — в общей таблице ниже). Сквозная **конвенция** над всеми — [Plan 177](177-fallible-result-everywhere.md) (Result-everywhere, D325): любая публичная std-операция возвращает `Result[T, E]`. 177 — НЕ модуль, а политика, governing все модули ниже.

Статус модуля — в его плане (`**Статус:**`) / сводно в [STATUS.md](STATUS.md); здесь — только группировка модуль→план.

| Модуль | План |
|---|---|
| parse (str→примитив) | [174.1](174.1-primitive-parse-api.md) |
| time | [175](175-time-system-rework.md) + [175.1](175.1-civil-time.md) (civil) |
| io / fs / os | [176](176-io-fs-os.md) (umbrella) |
| **nova lint** (полная: сабкоманда+реестр) | [185](185-nova-lint.md) |
| http (client+server, HTTPS, h2) | [178](178-std-http.md) (umbrella) |
| encoding/compress (gzip/deflate/brotli) | [179](179-std-encoding-compress.md) — гейт 178 decompress |
| serde / typed-json | [180](180-serde-derive.md) — гейт 178 `.json[T]` |
| encoding/json · base64 · url | существующие / `_experimental` (url → промоут в 178) |

**Сквозная конвенция:** [177](177-fallible-result-everywhere.md) (Result-everywhere) — применяется ко ВСЕМ модулям выше.

## Очередность исполнения 173-181 (граф зависимостей; зафиксировано 2026-07-03)

> **Все девять (173-181) прошли полную сверку (Ред. 2, 2026-07-03)**: 173-176 и 178-180 READY; 177 — ✅ CLOSED 2026-07-04
> (D325 полностью: спека+guard+conformance; stable-std мигрирована; остаток маркирован); 181 — proposed (Ф.0 = owner sign-off). Бывшие внешние коллизии СНЯТЫ:
> 178 renumber D327-D332 → **D357-D362 ✅ выполнен**; from_bytes = prereq 176 Ф.0.5; Plan-80 → D133 ✅ shipped.
> Очередность выводится из гейтов; стрелки продублированы в шапках планов. Слабосвязанные треки — параллелятся.

**Волна 0 — реконсиляции и gate-верификации (без кода; параллельно):**
`173 Ф.0R` ∥ `174 Ф.0R` ∥ `175 Ф.0` (D316-D318 в спеку) ∥ `176 Ф.0`+`Ф.0.5` ∥ `178 Ф.0`
(обычный GATE: spec-first D357-D362 + verify-list; Ред.2-сверка ✅ выполнена 2026-07-03 — renumber сделан, from_bytes = prereq 176 Ф.0.5, D133 ✅ shipped)
∥ `180 Ф.0-verify` (🔴 компилятор-гейты: `[M-126-sum-*-rich]` OPEN; `[M-161]` ✅ CLOSED — re-verify D355 typevar-receiver; #serde-attr
AST — честная оценка объёма до старта) ∥ `181 Ф.0-остаток` (verify+пины; sign-off R1-R7 ✅ 2026-07-03).
177: D325 ✅ уже в спеке — Ф.0-часть фактически выполнена.
Координация: 174 Ф.0R и 176 Ф.0(d) оба правят 174.6 (§2 CWStr) — согласованно, не параллельно по файлу.
Renumber D216/D282 ✅ уже выполнен (2026-07-03) — гейты 174.5/174.6-M0 сняты.

**Волна 1 — четыре параллельных трека:**

| Трек | Последовательность | Почему не ждёт других |
|---|---|---|
| **A: lang/FFI** | **174.3 (🔴 P1 — критический путь!)** ∥ 174.4 ∥ 174.6-M0 | 174.3 сидит на готовой type_id-инфре Plan 61, НЕ ждёт 172.1; 174.4 carrier-независим |
| **B: errors** | 173 Ф.1 (soundness, risk-0) → 173 Ф.2 (defer-kernel + renames Cleanup[E]/@cleanup) | багфиксы + унификация не зависят от 174/175/176 |
| **C: time** | 175 Ф.1 → Ф.1b → {Ф.1c ∥ Ф.2} → Ф.3 → Ф.4 | самодостаточен (коорд. 172.1-канал только на Ф.1) |
| **D: io-core** | 176 Ф.0.5 (from_bytes) → 176 Ф.1 (io.Read/Write/Seek, IoError, BufWriter) | io-core не трогает fs/время; Ф.0.5 = переоформление интринзика |
| **E: compress** | **179 Ф.1 (inflate/gzip/zlib — pure-Nova)** | алгоритмика на str/Vec — ни одного гейта; 🔴 гейт-опенер для 178 Ф.2 |
| **F: net byte-surface** | **178 Ф.0.5** (additive read_bytes/write_bytes + SocketAddr→value + AddrNet-retract + **TcpNet/UdpNet/DnsNet→единый Net**, owner 2026-07-03) | после 178 Ф.0-сверки; разблокирует 176 Ф.4(b); НЕ ждёт остального 178 |
| **G: Result-sweep** | 177-миграция (read_buffer 22 bare-twins, emit_c builtins) | конвенция D325 в спеке; sweep независим (Ф.2b parse-rename — координация 174.1, Волна 3) |

**Волна 2 — стыковки (каждая ждёт конкретный вход):**

| Что | Ждёт | Источник гейта |
|---|---|---|
| **174.2-остаток** (spec-closure + cross-carrier `?`-диагностики) | 173 Ф.1 (ядро кода `?` там) | 174 §3.2 / 173 Ф.1 п.2 |
| **173 Ф.3-семейство**: 173.0 → 173.3 → 173.1 → 173.2 + Ф.3-остаток | 173 Ф.2; `deadline:`/`timeout:`-параметры и удаление `with_timeout` — **после 175** (Monotonic/Duration + мокабельность) | 173 §3a п.3-4, 173.1 §2 п.5 |
| **173 Ф.4** (MultiError e2e, typed ScopeOutcome) | **174.3 done** («реализуй первым») | 173 Ф.4-гейт |
| **176 Ф.2 (fs)** ∥ **176 Ф.3 (os)** | Ф.2 ← **175** (Timestamp в Metadata) + координация 173 Ф.2 (`File impl Cleanup[IoError]`) + CWStr в 174.6 §2 | 176 Ф.2-DEP |
| **174.6 M1→M3** (checker/тег/тесты) | 174.6-M0 | 174 §2 п.5 |
| **176 Ф.4** (NetError→IoError + TcpStream io-conformance) | 176 Ф.2+Ф.3; (b)-часть — 178 byte-surface (при отсутствии → `[M-176-tcp-io-conformance]`, не блокер) | 176 Ф.4-DEP |
| **173 Ф.6** (panics-клаузула, −78 CU) | 173 Ф.1 + Ф.5-`nova_runtime_reset` | 173 Ф.6-гейт |
| **178 Ф.1** (message-model: Method/HeaderMap/Body/Url) | 178 Ф.0.5 + **176 Ф.0.5** (`from_bytes` для `Body.text()`) + D133 (✅) | 178 §6/§9 |
| **180 Ф.1-Ф.3** (data-model, record-derive, атрибуты) | 180 Ф.0-verify: закрытие `[M-126-sum-*-rich]` + re-verify `[M-161]` (✅ CLOSED, D355 typevar-receiver) + attr-AST (компиляторная работа — возможно слот 172.1-владельца) | 180 §4 Ф.0 |
| **181-реализация** (alpha-rename pass, Ф.1-Ф.5) | 181 Ф.0 sign-off; координация 172.1 (parser/checker-зона) | 181 §Ф.0; вне критического пути — любой свободный слот |

**Волна 3 — за внешними/поздними гейтами:**
- 174.1 + 174.5 ← D-трек 172.1 (+ координация владельца); **177 Ф.2b** (rename SHIPPED parse-триады) ↔ 174.1 —
  тройная координация 172.1×174.1×177;
- 175.1 civil-time ← 175; 176.1 process ← 176 Ф.1-Ф.3;
- **178 Ф.2 (client)** ← **179 Ф.1** (decompress) + 175/173 (deadline-by-default) → **178 Ф.3 (server)** ←
  173-семейство (per-conn-fiber, graceful-shutdown) → **178 Ф.4 (HTTPS) / Ф.5 (h2)** ← **Plan 116 TLS — внешний
  🔴 HARD-GATE**;
- **180 Ф.4 (JSON-backend)** → разблокирует 178 typed `.json[T]` (Q20);
- 173 Ф.5 — сквозная. Хвосты: mock-clock ↔ scope-deadline единый реестр (175 Ф.5c ↔ 173 §3a).

**Критические пути (в порядке важности):**
1. `174.3 → 173 Ф.4 → MultiError-агрегация` (ядро ошибок).
2. `175 → 173 §3a/173.1 (deadline) → 178 Ф.2 (deadline-by-default client)`.
3. **178 — точка схождения пяти стрелок:** ← 179 Ф.1 (decompress), ← 180 Ф.4 (.json[T]), ← 175/173 (deadline),
   ← 176 Ф.0.5 (from_bytes), ← Plan 116 (TLS — единственный внешний hard-gate волны). Всё, что можно сделать
   ДО 178 — треки E/F Волны 1.
4. `180 Ф.0-гейты (компилятор) → 180 Ф.1-Ф.4` — самый рискованный по объёму скрытой компиляторной работы.

**Вне критических путей:** 181 (rebinding), 174.4 (registry), 177-sweep — заполняют свободные слоты агентов.

**Кратчайший старт «прямо сейчас» (после Волны 0):** до **шести** параллельных агентов в своих worktree
(nova-pNNN, непересекающиеся файлы): 174.3 + 173 Ф.1 + 175 Ф.1 + 176 Ф.0.5/Ф.1 + **179 Ф.1** + **177-sweep**;
седьмым слотом — 178 Ф.0-GATE (spec-first D357-D362; Ред.2-сверка ✅ уже выполнена).

## Статусы планов

Источник правды статуса — строка `**Статус:**` в самом файле плана `NNN-*.md`.
Сводный обзор — сгенерированный [STATUS.md](STATUS.md) (регенерация: `bash scripts/gen-plan-status.sh`).

> Plan 19 — see `19-closure-and-error-ops.md` (closure-rev + D85 error-ops).
> Plan 20 и 21 — последовательные (Plan 21 зависит от Plan 20).
> Plan 22 — самостоятельный, не блокирует Plan 20/21.
> Plan 25 — gap analysis vs Go/Rust state-of-the-art; не план-исполнения, а honest assessment.

## Связанные директории

- [docs/research/](../research/) — справочные материалы и сравнения
