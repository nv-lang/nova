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
не нашли НИЧЕГО дополнительного снимаемого в своих зонах. **Найден и подтверждён НОВЫЙ класс: несвязанный merge
закрывает документированный gap как побочный эффект** — 2 находки этого класса, ОБЕ сняты по протоколу
детач+panic (пробный снос → верификация → полный снос тем же заходом):
1. **`B11ai_serialize_contract`** — Plan 196.9's фикс `de15478d1` (`[M-primitive-concrete-overload-receiver-
   dispatch]`, смёржен ДЛЯ ДРУГОЙ причины — i64.clamp primitive dispatch) закрыл match-arm-bound-receiver пробел
   (волна-5's задокументированная последняя причина жизни).
2. **`B11m_stringbuilder_instance`** — Plan 208 Ф.2 (D374 AMEND, Unified Formatter) + Plan 198/196.6 (`[race-198]`
   name-only-fallback AV фикс) вместе закрыли ОБЕ причины (derive-тело больше не зовёт StringBuilder; `write_str`
   заменён на `write`) — волна-3's задокументированный блокер.

Детач+panic-пробные прогоны НЕ сработали НИ РАЗУ (0 panics на всех целевых корпусах, обе ветки) → полный снос
обеих в тех же заходах. **Реестр: 50 → 49 → 48.**

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
корпусе прошлыми волнами):** B03 (std/os,fs), B11i (nova_tests/concurrency), B11ac/B11ak (уже подтверждены
живыми в census на examples/effects и collections/data/encoding соответственно — просто не в МОЁМ узком сэмпле).
(`B11m` был в этом списке до §2b — снят этой сессией, см. ниже.)

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

## 2b. Снесено — Батч 2: `B11m_stringbuilder_instance`

**Находка:** волна-3 (`196.5-stage-d-wave3-notes.md` §3, микро-композиция) держала ветку живой на **kill-switch
A/B .c-diff** — синтезированное Debug-тело (D229) звало `write_str` на `Nova_StringBuilder*`-ресивере, fallthrough
менял unit-вердикт statement-эмиссии (`(void)`-обёртка). Прочитал текущий `auto_derive.rs`
(`synth_debug_record_body`/`synth_display_record_body`, строки 1082-1170ish) — доккомментарии там ФИКСИРУЮТ ДВЕ
несвязанные правки с тех пор: (1) Plan 208 Ф.2 (D374 AMEND) перетипизировал синтезируемый параметр `sb
StringBuilder` → `w Fmt`; (2) `[race-198 / 196.6]` заменил `write_str` на `write` (несвязанная причина —
name-only-fallback мисдиспатч, AV-баг Plan 198). Комбинация: derive-тела сегодня НЕ вызывают НИЧЕГО на
`Nova_StringBuilder*`-ресивере вообще — мотивирующий кейс волны-3 структурно исчез.

**Верификация (детач+panic, НЕ сработал):**
1. `std/src/runtime/string_builder_test.nv` (единственный ДРУГОЙ класс трафика этой ветки — прямое использование
   StringBuilder вне derive: `.len()`/`.append()`/`.into()`/…) — PASS, 0 hits, panic не сработал.
2. `std/src/collections` + `std/src/time` + `std/src/encoding` (--skip lru_test) — PASS 26/0/14skip, 0 hits.
3. `d229_debug_format_spec.nv` + `d422_generic_container_derive.nv` (оба зовут auto-derive `@debug`/`@display` —
   ТОЧНЫЙ мотивирующий класс волны-3) — компилируются и запускаются (2 ПРЕ-СУЩЕСТВУЮЩИХ несвязанных RUN-FAIL на
   `Vec.from().into_str()`, задокументированных Zone RET сегодня же, `196-ret-notes.md` §0 — не регрессия, тот
   же паттерн ошибки byte-в-byte).

**Реестр:** 49 → **48**. `git diff --stat`: +27/-19.

---

## 3. Осталось (48 веток) — классификация

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
assert; `f1_expr`'s `ExprKind::With { body, .. }` всё ещё игнорирует `bindings[i].handler` — проверил по коду,
де15478d1 НЕ трогал `With`-арм, только `Match` — НЕ переоткрывал, НЕ фиксил, вне scope: `types/mod.rs` правка).

### 3.4 Core (не снимаемо без Zone CH channel-расширения) — 32
> **Реклассификация 2026-07-17 (Q2-волна, wip/196-q2-notes.md):** `B12h` перенесён в класс
> B11q/B11r §3.2 — блокер Plan 59 Ф.7.5 D3 (typed-Result mono интринсика), НЕ Zone CH.
`B01`, `B02`, `B05`, `B06`, `B06a`, `B06b`, `B06c`, `B06d`, `B07`, `B07r`, `B08`, `B08r`, `B10e`, `B10f`, `B10h`,
`B10j`×2, `B10l`, `B10m`, `B11a`, `B11ae`, `B11af`, `B11d`, `B11e`, `B11f`, `B11j`, `B11k`, `B12b`,
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

## 5. Коммиты сессии (ветка `p196-capstone`, worktree `nova-196cap`)

1. `df0e0ea53` — `fix(codegen): [196-capstone] B11ai_serialize_contract снята — de15478d1 закрыл match-arm-bound
   gap побочно` (детач+panic пробный снос → panic не сработал → полный снос, ОДИН коммит) + регресс-пин
   `spec_tests/conformance/standalone/m196_serde_option_match_arm.nv` + этот notes-файл (первая версия).
2. `18be31441` — `fix(codegen): [196-capstone] B11m_stringbuilder_instance снята — Plan 208/196.6 закрыли обе
   причины жизни` (детач+panic пробный снос → panic не сработал → полный снос, ОДИН коммит).
3. (этот коммит) — `docs(196): capstone — Батч 2 (B11m) в notes, финальная классификация 48 веток`.

**В main НЕ мёржено.** Push запрещён по заданию — интегратор вливает батчами и гоняет CI/полный conformance сам.

---

## 6. Рекомендация следующей волне (если сессия продолжится)

1. **SHADOW mismatch ICE** (§4.4) — сообщить Zone CH/владельцу; debug-only, но подрывает доверие к
   `node_substs`-каналу для LinkedList-generic-K класса; НЕ в scope frozen-зоны, но нашёл честно.
2. **Ядро (§3.4, 33 ветки)** — не снимаемо без Zone CH channel-расширения на erased/mono-клон call-site'ы
   (тот же владелец-гейтед развилка «ExprId-coverage», что документирована с самого начала кампании).
3. **Продолжать грепать git log между волнами на предмет «несвязанный merge закрыл undocumented gap»** — ЭТОТ
   паттерн (не «снос дублирующего легаси», а «сторонний фикс попутно насытил канал») дал **100% находок этой
   сессии** (обе — B11ai и B11m, подтверждены, сняты) и НЕ покрывается стандартным icr_trace-счётчиком на узком
   сэмпле — нужен именно git-archaeology заход (читать доккомментарии живых веток → искать соответствующий
   `[M-...]`-маркер/план в `git log -S<term> -- types/mod.rs` и `auto_derive.rs` между базой прошлой волны и
   текущим HEAD), не только измерение. Кандидаты для следующего захода этим методом: §3.2/3.3 ветки, чьи
   доккомментарии называют конкретный внешний блокер (`B10a` — `With`-арм handler-walk; `B03`/`B11ac`/`B11ai`-типа
   формулировки, если появятся новые в будущих волнах).
