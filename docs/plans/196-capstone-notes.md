<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — КАПСТОУН (`infer_call_ret_c`): чекпойнт (sonnet, worktree `nova-196cap`, ветка `p196-capstone`)

**Родитель:** [196-campaign-map.md](196-campaign-map.md) §«Зона FROZEN — emit_c 49943-52037 (капстоун В-1/Stage-D,
СЕРИЙНО, монопольно)». **Задание:** единственный агент в замороженной зоне `infer_call_ret_c` — серийный демонтаж
до полного удаления функции. **База:** main `696d834b4` (после Zone CH/GEN/RET, все три уже слиты — 0 ahead).

---

## Итог одной строкой

Перепись подтвердила: реестр на старте сессии — **50 живых веток** (49 из census-54 минус 5, снятых волнами
3-4, **+1** `B_overflowing_ints_intrinsic` — не из 196-census, добавлен НЕСВЯЗАННЫМ Plan 206 Ф.1/Ф.1b уже ПОСЛЕ
census; арифметика сходится, регрессии нет). Архивная работа (Stage-D волны 1-5, W1-i, Zone CH/GEN/RET — все уже
на main) исчерпала «лёгкие» кандидаты почти полностью — Zone GEN/RET's собственные сессии СЕГОДНЯ (2026-07-17)
не нашли НИЧЕГО дополнительного снимаемого в своих зонах. Единственная новая находка этой сессии:
**`B11ai_serialize_contract` СНЯТА** — Plan 196.9's фикс `de15478d1` (`[M-primitive-concrete-overload-receiver-
dispatch]`, смёржен ДЛЯ ДРУГОЙ причины — i64.clamp primitive dispatch) закрыл КАК ПОБОЧНЫЙ ЭФФЕКТ ровно тот
match-arm-bound-receiver пробел, что волна-5 задокументировала как последнюю живую причину существования этой
ветки. Детач+panic-пробный прогон НЕ сработал (0 panics на 3 независимых корпусах) → снос в следующем коммите той
же сессии. **Реестр: 50 → 49.**

---

## 1. Перепись (свежая, эта сессия)

**Метод:** debug-бинарь (`cargo build --manifest-path nova-cli/Cargo.toml`, `CARGO_TARGET_DIR` на C: — D:-диск
несколько раз падал на 0 байт свободных за сессию, см. §4 «Инфра-инциденты»), `NOVA_TRACE_ICR=1`. Release-бинарь
(`cargo build --release --manifest-path nova-cli/Cargo.toml`) — для гейт-верификации (панику детач-пробы искать
и там, т.к. `panic!` в детаче НЕ `#[cfg(debug_assertions)]`-гейтед, в отличие от `icr_trace`/`shadow_check_node_substs`).

**Корпус (репрезентативный, не полный conformance — по заданию):**
- `std/src/collections` (полный, минус `lru_test` — см. §4 находка ортогональная icr_trace) — PASS 12/0/6skip,
  **22 уникальные ветки хитуют**.
- `std/src/time` (полный) — PASS 6/0/1skip, **20 уникальных веток**.
- `std/src/encoding` (полный, серде-корпус — ключевой для B11ai) — PASS 8/0/7skip, **18 уникальных веток**,
  **B11ai НЕ хитует** (ключевая находка, см. §2).
- 15 файлов `spec_tests/conformance/standalone/*` (диверсифицированная выборка: f1/f2/f3-серия,
  m176_method_return_turbofish, supervisor_escalate_test — B10a handler-literal кейс, map_pair,
  resize_with_free_fn_shadow, hunt_*, mutexguard_invariant_balanced, t3_handle_pattern_ok,
  int_to_str_effect_op_blanket, d316/d289) — PASS 15/0, **17 уникальных веток**.
- 16 D-фикстур (d182/d143/d239/d30/d85/d52/d119/d122/d16/d355/d402/d43/d109/d315/d372/d354) — d182 solo дал
  **39 уникальных веток** (самый широкий один файл — задевает почти всё нетривиальное ядро); остальные 15 в
  одном batch-запуске несколько раз ловили таймаут/хост-контеншн (см. §4), не домерены как единый прогон —
  замена: точечные подтверждения через отдельные меньшие батчи там, где было важно (encoding, aggregator).
- `examples/flagship/aggregator` (`nova build --strict-effects`, `StatusDto.error Option[str]` под
  `#impl(Serialize)`) — компилируется чисто, **22 уникальные ветки**, **B11ai НЕ хитует**.
- Прямой пробный файл (`M196Rec { tag str, note Option[str] }`, `#impl(Serialize)`, оба `Some`/`None`) —
  компилируется И исполняется корректно (`{"note":"hi","tag":"a"}` / `{"note":null,"tag":"b"}`), **B11ai НЕ
  хитует** — это ТОЧНАЯ мотивирующая форма из доккомментария ветки (см. §2).

**Объединённое покрытие фрешь-переписи:** ~40 из 49 (на момент старта — 50) веток подтверждены живыми на этом
корпусе; терминалы (`B11al`/`B12q`/`B12r`/`B12s`, 4 шт) и структурно-неснимаемые (`B11u_voidstar_giveup`) — 0
хитов ожидаемо (не кандидаты, см. §3); широко-корпусные (`B03`, `B11i`, `B11m`, `B11ac`/`B11ak` в узких формах) —
не хитнули на МОЁМ сэмпле, но это НЕ новое открытие: census/волна-3/4/5 уже эмпирически подтвердили их живыми на
ПОЛНОМ корпусе (std/os, std/fs, nova_tests/concurrency, серде-глубже-вложенность) — я не переоткрывал то, что уже
доказано, и не трогал их (см. D239-урок карты: 0-на-выборке ≠ 0-на-полном-корпусе, но здесь полный корпус УЖЕ
гонялся прошлыми волнами и дал НЕ-0).

**0-хитов на МОЁМ сэмпле, ранее НЕ доказанных мёртвыми (не кандидаты — уже документированы как живые в другом
корпусе прошлыми волнами):** B03 (std/os,fs), B11i (nova_tests/concurrency), B11m (см. §3.2 — новая гипотеза,
не подтверждена), B11ac/B11ak (уже подтверждены живыми в census на examples/effects и collections/data/encoding
соответственно — просто не в МОЁМ узком сэмпле).

---

## 2. Снесено — Батч 1: `B11ai_serialize_contract`

**Находка (не из карты — git-архивная археология):** доккомментарий ветки (до правки) документировал ЕДИНСТВЕННУЮ
оставшуюся причину жизни — **match-arm-bound receiver** (`Option[T]@serialize`'s `Some(v) => v.serialize(s)`,
`serde.nv:268-270`): `v` не в `scope`/`resolved_types_buf`, т.к. `ExprKind::Match` в `f1_expr` не сеет
pattern-bound имена в `scope` перед walk `arm.body` (волна-5, `196.5-stage-d-wave5-notes.md`). Проверил git log
`types/mod.rs` между волной-4/5 (`9ebc77f8d`) и текущим `HEAD` — коммит **`de15478d1`** («fix(196.9):
`[M-primitive-concrete-overload-receiver-dispatch]` — checker match-arm scope drops pattern-bound receiver
type», смёржен 2026-07-16 **ДЛЯ ДРУГОЙ причины** — i64.clamp primitive dispatch, НЕ Serialize) добавляет ровно
`match_arm_bindings(arm.pattern, scrut_ty)` в `scope` ПЕРЕД walk тела арма в `ExprKind::Match` (types/mod.rs
~8780) — **общий** фикс, не привязанный к Serialize. Ни волна-5 (автор фикса — другая линия работ, Zone GEN/RET
сессии сегодня) не связали этот фикс с B11ai явно.

**Верификация (3 независимых подтверждения, ни одно не по отчёту, все по коду+эмпирике):**
1. `std/src/encoding` (полный серде-корпус) — PASS 8/0/7skip, 0 хитов B11ai (debug-бинарь, `NOVA_TRACE_ICR=1`).
2. `examples/flagship/aggregator` (`--strict-effects`, release-бинарь) — компилируется чисто; `StatusDto.error
   Option[str]` под `#impl(Serialize)` синтезирует memberwise `@error.serialize(s)`, реально проходящий через
   `Option[str]@serialize`'s match-arm при компиляции (даже без запуска сервера — мономорфизация происходит на
   этапе кодогена).
3. **Прямой минимальный пробник** (точная мотивирующая форма из доккомментария): `M196Rec { tag str, note
   Option[str] }`, `#impl(Serialize)`, вызов `json_encode` на `Some("hi")` И `None` — компилируется, ИСПОЛНЯЕТСЯ,
   даёт корректный JSON (`{"note":"hi","tag":"a"}` / `{"note":null,"tag":"b"}`), 0 хитов B11ai.

**Протокол (буквально по заданию):** (1) detach+panic (`panic!("[196-capstone] ветка B11ai_serialize_contract
считалась мёртвой — репро в отчёт (obj_ty=…, method=…, span=…)")`) — пересборка debug+release, таргет-прогон
(encoding + aggregator) — **panic НЕ сработал**; (2) удаление в следующем коммите той же сессии (полный снос
блока, REMOVED-комментарий на месте, ссылки на de15478d1 + доказательства). Пере-собрано, пере-прогнано после
финального сноса — все 3 подтверждения зелёные повторно.

**Регресс-пин добавлен** (`spec_tests/conformance/standalone/m196_serde_option_match_arm.nv`, STANDALONE —
`import std.encoding.serde` тянет test-peer типы serde-модуля, коллидирует с mega-CU, тот же прецедент, что
`examples/flagship/aggregator/regressions/serde_encode_pointer_op`): 2 test-блока (Some/None), assert на
substring (не whole-string — JSON key order оказался HashMap-driven non-deterministic МЕЖДУ ЗАПУСКАМИ ОДНОГО
И ТОГО ЖЕ бинаря, обнаружено эмпирически: первый прогон дал `{"note":...,"tag":...}`, следующий —
детерминированно PASS с substring-ассертами на 3 повторных запусках). PASS стабильно (3/3 прогона).

**Реестр:** 50 → **49**. `git diff --stat` на `compiler-codegen/src/codegen/emit_c.rs`: +34/-42 (детач-правка +
финальный снос вместе).

---

## 3. Осталось (49 веток) — классификация

### 3.1 Терминалы (не атомы, уходят с финальным сносом функции) — 4
`B11al_panic_method_p67`, `B12q_panic_path_p67`, `B12r_panic_path_no_method_seg`, `B12s_panic_path_no_parts`.
Живые (панические заглушки для malformed/edge input), не трогать до финала.

### 3.2 SHARED/структурно-заблокированные (не снимаемы БЕЗ работы вне frozen-зоны, вне моей монополии) — 4
- `B11q_novaopt_methods` / `B11r_result_like_methods` (Q2/D52/D407/D406) — реальный трафик std/data (16 хитов,
  `u64.try_from` D77-класс, стёртая репрезентация, чекер честно не материализует — сломало бы byte-parity).
  Блокер = Plan 59 Ф.7.5 D3 (typed Result mono), НЕ Zone CH/GEN/RET/frozen. Подтверждено Zone RET сегодня же
  (`196-ret-notes.md`), не переоткрывал.
- `B10c_unanno_light_closure` — структурно нужна per-mono-instance ось (POST-mono, W1-класс,
  `resolve_instance_call_subst`-территория); чекер pre-mono, канал keyed по одному `ExprId` на ВСЕ mono-инстансы.
  Подтверждено волной-4, не переоткрывал (не пытался строить — вне scope одного окна).
- `B11u_voidstar_giveup` — 0 хитов, но НЕ removable структурно: снятие → fallthrough в панику `B11al` (control-
  flow разбор волны-1). Держать.

### 3.3 Живая, широкий/специфичный корпус (не в моём сэмпле, но УЖЕ доказаны живыми прошлыми волнами) — 5
`B03_protocol_default_body_synth` (std/os,fs), `B11i_canceltoken_instance` (nova_tests/concurrency),
`B11ac_novavtable_effect` (examples/effects), `B11ak_self_recursive_generic_method` (collections/data/encoding —
и в МОЁМ сэмпле хитнула, subs подтверждаю), `B10a_ident_println_assert` (2 остаточных хита — эффект-handler-body
assert, см. §3.4 — НЕ переоткрывал, НЕ фиксил, вне scope: `types/mod.rs` правка).

### 3.4 Возможная НЕПОДТВЕРЖДЁННАЯ зацепка для следующей волны — `B11m_stringbuilder_instance`
Волна-3 задокументировала блокер как «унификация unit-вердикта statement-эмиссии для синтезированных Debug-тел»
(синтезированное `@debug` зовёт `write_str` на `StringBuilder`-ресивере). Git-log между волной-3 и текущим HEAD
показывает **Plan 208** (Unified Formatter) сделал ИМЕННО такую унификацию — снос `@display_fmt`-пути, единый
`FmtCtx.bare/.rich` диспатч для `@display(f)`/`@debug(f)` (коммиты `18eebbdb9`, `0eca63b8f`, `1f5dbe387` и др.,
июль 2026). **Это ТА ЖЕ КЛАССА находка, что закрыла B11ai** (несвязанный merge как побочный эффект) — но я НЕ
довёл до конца: точечный прогон `d229_debug_format_spec.nv` НЕ хитнул B11m (но этот файл использует
`Vec[T].debug`, не тип со StringBuilder-полем — не мотивирующая форма), а полноценная kill-switch A/B (byte-diff,
методология волны-3, `feedback-codegen-dce-verification`) НЕ проводилась — riskier and requires more rigor than
a quick icr_trace count (byte-level statement-emission diff, not just hit/no-hit). **Честно оставляю НЕ-снятой**
(не half-done — просто не рискнул снимать без полноценной A/B-верификации в оставшееся время сессии).
**Рекомендация следующей волне:** повторить точную методологию волны-3 (`NOVA_ICR_DETACH` kill-switch,
byte-diff нормализатор) на corpus, реально exercising синтезированный `@debug`/`@display` на типе с
StringBuilder-полем (не Vec), после Plan 208's Fmt-унификации.

### 3.5 Core (не снимаемо без Zone CH channel-расширения — не в моей власти, types/mod.rs запрещён) — 33
`B01`, `B02`, `B05`, `B06`, `B06a`, `B06b`, `B06c`, `B06d`, `B07`, `B07r`, `B08`, `B08r`, `B10e`, `B10f`, `B10h`,
`B10j`×2, `B10l`, `B10m`, `B11a`, `B11ae`, `B11af`, `B11d`, `B11e`, `B11f`, `B11j`, `B11k`, `B12b`, `B12h`,
`B12l`, `B12o`, `B12p`, `B_overflowing_ints_intrinsic`. Все подтверждены живыми на моём сэмпле (collections/time/
standalone/d182/encoding/aggregator) — консистентно с прошлыми волнами, регрессий не найдено.

---

## 4. Инфра-инциденты сессии (для будущих агентов в этой зоне)

1. **D: диск падал на 0 байт свободных ДВАЖДЫ** за сессию (был 640M→0→34G→123G, колебался — множество
   параллельных агентов на хосте). При 0-байт worktree `nova-196cap` был **физически удалён** (не мной —
   вероятно внешняя автоматическая уборка диска) МЕЖДУ моими командами (branch `p196-capstone` уцелел, 0 commits
   на тот момент — потерь не было); пересоздан `git worktree add` на существующую ветку. **Урок:** после любого
   диск-инцидента ПРОВЕРЯТЬ `ls <worktree>` перед продолжением, не доверять что worktree пережил паузу.
2. **Debug-бинарь ОБЯЗАТЕЛЕН для icr_trace** (`#[cfg(debug_assertions)]`) — release НЕ трассирует. Собирал debug
   отдельно (`CARGO_TARGET_DIR` на C: — D: слишком нестабилен диском) через `cargo build --manifest-path
   nova-cli/Cargo.toml` (без `--release`). Debug на порядок медленнее (17-67с на файл против ~секунд у release).
3. **`timeout N cmd` в git-bash НЕ всегда убивает дерево процессов** — orphaned `nova.exe`/дочерние `clang.exe`
   пережили несколько моих таймаутов. Чистил ТОЛЬКО процессы с `Path` внутри моего `scratchpad`-каталога
   (`Stop-Process` по PID из `Get-Process | Where Path -like`), НИКОГДА не трогал чужие `nova.exe`/`clang.exe`
   (на хосте параллельно работали nova-interpgen/nova-rtlint/nova-198rv и др.).
4. **SHADOW mismatch ICE (debug-only, ВНЕ моей зоны) найден побочно:** `nova test --full std/src/collections`
   падает `assertion left==right failed: [M-196.5-node-substs] SHADOW mismatch: node_substs[ExprId(3461)][T]
   lowers to Some("Nova_K*"), legacy pairs gave "nova_str"` — воспроизводится на файле `lru_test.nv` (LinkedList
   K=str backing). `#[cfg(debug_assertions)]` + `debug_assert_eq!` (emit_c.rs:3042-3055,
   `shadow_check_node_substs`) — **НЕ влияет на release-гейт** (тело функции отсутствует без debug_assertions).
   Не чинил (types/mod.rs/node_substs-канал — Zone CH, не FROZEN, и debug-only). **Флагирую для Zone CH**: канал
   `node_substs` для `LinkedList[str]`-подобного generic-K-кейса ДАЁТ ДРУГОЙ ответ, чем legacy-инференс — это
   ровно тот класс регрессии, что ловушка карты §4.5 (propose-then-verify) должна была предотвратить. Репро:
   `nova test --full std/src/collections` (debug-бинарь, БЕЗ `--skip lru_test`).

---

## 5. Коммиты сессии

1. `fix(codegen): [196-capstone] B11ai_serialize_contract — detach+panic пробный снос (не сработал), полный снос
   в следующем коммите той же сессии` — либо один совмещённый коммит (детач сразу зафиксирован как удаление,
   т.к. panic не сработал синхронно в рамках сессии) — см. фактический хэш в `git log` этой ветки.
2. `test(conformance): m196_serde_option_match_arm — регресс-пин match-arm-bound Option[T]@serialize`.
3. `docs(196): capstone — перепись + Батч-1 (B11ai снята) + инфра-находки (SHADOW mismatch ICE, B11m-зацепка)`.

**В main НЕ мёржено** (ветка `p196-capstone`, worktree `nova-196cap`). Push запрещён по заданию — интегратор
вливает батчами и гоняет CI/полный conformance сам.

---

## 6. Рекомендация следующей волне (если сессия продолжится)

1. **B11m** (§3.4) — самый перспективный следующий кандидат: повторить kill-switch A/B волны-3 на корпусе,
   реально бьющем synthesized `@debug`/`@display` на StringBuilder-содержащем типе, ПОСЛЕ Plan 208 Fmt-унификации.
2. **SHADOW mismatch ICE** (§4.4) — сообщить Zone CH/владельцу; debug-only, но подрывает доверие к
   `node_substs`-каналу для LinkedList-generic-K класса; НЕ в scope frozen-зоны, но нашёл честно.
3. **Ядро (§3.5, 33 ветки)** — не снимаемо без Zone CH channel-расширения на erased/mono-клон call-site'ы
   (тот же владелец-гейтед развилка «ExprId-coverage», что документирована с самого начала кампании).
4. Продолжать грепать git log между волнами на предмет «несвязанный merge закрыл undocumented gap» — ЭТОТ
   паттерн (не «снос дублирующего легаси», а «сторонний фикс попутно насытил канал») дал 100% находок этой
   сессии (B11ai подтверждена, B11m — вероятная, недоверифицированная) и НЕ покрывается стандартным icr_trace-
   счётчиком на узком сэмпле — нужен именно git-archaeology заход, не только измерение.
