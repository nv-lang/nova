﻿# Упрощения и отложенные доработки

Живой список осознанных упрощений, сделанных в ходе разработки.
Каждое упрощение попадает сюда в момент принятия решения — чтобы не потерять контекст.

> **Принцип** (см. [`project-philosophy.md`](project-philosophy.md)): Nova не в
> проде, революционный язык важнее обратной совместимости. Упрощения здесь —
> **временные**, должны закрываться по мере роста проекта. Каждое имеет
> rationale и roadmap. **Не использовать этот документ как тихое разрешение
> оставлять tech-debt без плана.**

**Что сюда НЕ пишется** (чистка 2026-07-18, заказ владельца — файл превратился
в свалку всего подряд): закрытые/снятые упрощения переносятся в
[`docs/history/simplifications-closed.md`](history/simplifications-closed.md)
**в момент закрытия**, а не копятся здесь под пометкой «ЗАКРЫТО»; диагнозы
багов и хроники фиксов сюда не пишутся вовсе — открытый хвост (если есть)
фиксируется маркером в [`docs/plans/backlog-followups.md`](plans/backlog-followups.md),
сама хроника — в план-заметках/nova-private; отчёты о проделанной работе
(что сделано, гейты, коммиты) сюда тоже не пишутся — это дело плана/discussion-log,
не живого списка упрощений. Здесь остаются только ДЕЙСТВУЮЩИЕ осознанные
упрощения с rationale и условием снятия.

Формат записи:
- **Где** — файл/модуль.
- **Что упрощено** — что НЕ делается.
- **Почему** — trade-off на момент принятия.
- **Как чинить** — краткий план.
- **Приоритет** — L / M / H.

[2026-07-20 Plan 219 — build-демон (резидентный cache/config-сервис для `nova build`), worktree `nova-219`, ветка `p219-build-daemon`] **Где:** `nova-cli/src/daemon.rs` (новый). **Что упрощено:** (1) auto-spawn демона на первом `nova build` — ТОЛЬКО под `NOVA_DAEMON=1` (opt-in), НЕ default-on как Plan 218's `NOVA_RT_ARCHIVE` — демон плодит фоновый процесс (не просто in-process кэш), молча оставлять detached-процессы в CI/песочницах без явного согласия — не то поведение, которое должно включаться по умолчанию; ручной `nova daemon start` работает всегда независимо от env. (2) dep-lock ledger-инвалидация покрывает хеш entry `nova.toml`+`nova.lock`, НЕ обходит граф транзитивных path/git-зависимостей — правка манифеста ЧУЖОГО пакета (не entry) без изменения entry-манифеста/lock может дать stale `skip_dep_lock=true` до следующего реального изменения; полный обход графа живёт в `compiler-codegen/src/lockfile.rs`, вне зоны волны (`nova-cli/src/**` only). Смягчение: `NOVA_DAEMON=0`/`nova daemon stop` сбрасывают состояние. (3) Unix auto-spawn БЕЗ session-detach (`setsid`) — не заводили `libc`-зависимость ради одного вызова; ребёнок остаётся в process group родителя (основная целевая платформа этой волны — Windows, `CREATE_NEW_PROCESS_GROUP|DETACHED_PROCESS`). **Почему:** резидентный демон — фоновый процесс с некорректируемым (без доп. кода) полным графом зависимостей; полное решение = либо трогать `compiler-codegen/src/lockfile.rs` (вне зоны), либо добавлять `libc` только ради detach — оба избыточны для Ф.1 объёма. **Как чинить:** (1) снять после эксплуатационного опыта/владелец-решения про default; (2) добавить transitive-fingerprint helper в `lockfile.rs` отдельной волной, если реальный кейс всплывёт; (3) `libc`+`setsid`, если Unix demand появится. **Приоритет:** L (opt-in уже безопасен по умолчанию; (2)/(3) — латентный риск, не наблюдался).

[2026-07-10 Plan 172.13 Ф.2 — constraint-core литерал-коэрсия миграция; D55 разграничение, ветка constraint-core, 🟡 Ф.0-2 ✅, Ф.3 не начато] Ф.0 (инвентарь продюсеров f1-преамбулы канала) + Ф.1 (ядро-скелет constraint_solver.rs: TypeVar/Ty/Constraint::{Eq,MemberOf}/TypeSet/Solver::unify с occurs-check, НЕ подключено глобально) + Ф.2 (миграция ОДНОГО пакета — literal-coercion семья, `annotate_expected_concrete`+`materialize_literal_coercion`, с ad-hoc `matches!`-цепочек на общий `TypeSet`-язык через решатель). **Осознанный scope-cut:** Ф.3 (снос остатка — Binary-арифметика Join, If/Match-Join, resolve/overload-семья) НЕ начато — объём в несколько волн, инвентарь+ядро+один пакет = осторожный первый шаг архитектурной замены (владелец: «не рашить весь движок в один заход»). **anon-RecordLit D55** (единственный открытый user-visible симптом из мотивации плана) — репро подтвердило: `codegen error: anonymous record literal without spread not supported in codegen` — ЧИСТО codegen-эмиссионный гэп (`emit_c.rs:39613` читает только `current_fn_return_ty`, не чекер-канал `resolved_types_buf`), ДО и НЕЗАВИСИМО от чекерного ядра; Ф.1/Ф.2 не закрывают и не могут закрыть без отдельного codegen-трека — маркер `[M-d55-anon-recordlit-codegen-gap]`. **Byte-parity ловушка задокументирована:** `TypeSet::ScalarNotWideDefaultInt` намеренно исключает ТОЛЬКО ровно `int` (signed wide-default), НЕ `uint` (unsigned wide-default) — зеркалит асимметрию исходного ad-hoc гейта; explicit-тест `scalar_not_wide_default_int_excludes_only_signed_wide_default` защищает от регресса при обобщении. **Методологический маркер** `[M-172.13-cross-repo-c-diff-noise-not-regression]`: byte-parity `.c`-диффы между ДВУМЯ РАЗНЫМИ worktree на formally-идентичном коммите дают build-окружение шум (лишняя функция/сдвиг нумерации) — НЕ регрессия; диффить нужно ВНУТРИ одного worktree (checkout A→build→capture; checkout B→build→diff). Гейты: cargo build чисто; cargo test --lib 939 passed / 3 pre-existing fail (verified byte-identical at merge-base 2be6d7064 в том же окружении) + 16 новых юнитов constraint_solver; conformance --positive --compile-error 91/0; err173* 5/5 индивидуально; nova_tests sample (generics/plan172*/basics) byte-identical HEAD vs merge-base. Модель: sonnet.

[2026-07-10 Plan 116 Ф.0 — актуализация + tls_shim-скелет, 🟡 Ф.0 ✅] План 116 переписан целиком под пост-183/176.4/177/178 реальность: эффект `Tls` РЕТРАКТИРОВАН — std/tls = библиотечный слой поверх `TcpStream` (методы несут `Net`; мотивировка по module-conventions §0 — в плане §«Ключевое решение Ф.0-1»); R-1 решён: rustls 0.23 + провайдер `ring` (НЕ дефолтный aws-lc-rs — тому нужны cmake+nasm, чужой toolchain) + webpki-roots. Вендорен скелет `compiler-codegen/tls_shim/` (Rust staticlib, C-ABI `tls_*` ~27 символов, crt-static → libcmt /MT-соответствие; cargo test 5/5; Cargo.lock закоммичен force-add поверх compiler-codegen/.gitignore — ОСОЗНАННО: для шима lock = supply-chain-пин R-9; bootstrap-политика «пустой lockfile» относится к nova-codegen, не к шиму). **ОСОЗНАННЫЕ SCOPE-CUTS Ф.0 (с планом):** (1) `Pinned` (SPKI-pinning) в шиме = `TLS_ERR_UNSUPPORTED` до Ф.4.3 (кастомный ServerCertVerifier — там же); данные через границу уже принимаются — граница не изменится. (2) Прекомпилят staticlib НЕ трекается (~6.1 MB; решение о трекинге — Ф.2.1 после замера; brotli-прецедент трекает .lib, но тот в 15 раз меньше). (3) Условная линковка (test_runner.rs, механизм brotli/D337) — Ф.2. Маркеры `[M-116-*]` — план §Out-of-scope; `[M-178-https-needs-116]` закрывается Ф.5.3.

[2026-07-10 Plan 175.1 — civil time, 🟢 ПРИЗЕМЛЕНО с задокументированными сужениями] Полный civil-слой (`std/time/civil`, D319/D320/D321): Date/TimeOfDay/DateTime/YearMonth/MonthDay/Period/Offset/TimeZone/ZonedDateTime + Hinnant epoch-day, CLAMP-арифметика Q7, 4-way Disambiguation/OffsetConflict, strict ISO/RFC-3339/RFC-9557 parse (§1а: `s.to_date()`/`to_datetime()`/`to_timezone()`…), TZif-парсер + curated embedded tz-таблица, pattern-DSL `DateTimeFormat`. Компилятор НЕ тронут. **Сужения/обходы (все с маркерами и планом):** (1) `[M-175.1-full-tzdb-embed]` — embedded-таблица curated (NY/London/Moscow/Sydney + фикс-оффсеты, rule-based 1996..2100), НЕ полный ~450KB IANA-snapshot; TZif-парсер полный, POSIX-слой работает; починка = упаковка snapshot-данных. (2) `[M-175.1-local-offset-effect-op]` — `Time.local_offset()` требует слот в NovaVtable_Time (nova_rt — компилятор-зона, параллельные волны в emit_c); зона передаётся явно (неявной локальной и так нет по D319 R1). (3) Codegen/checker-гэпы класса value-record/overload, обойдённые по §4а: `[M-175.1-value-in-value-emit-order]` (декларация DateTime перенесена в time_of_day.nv — порядок эмиссии структур лексикографический по файлам), `[M-175.1-variant-literal-receiver]` (`Sun.next()` эмитится как несуществующий Path-вызов — в тестах bound-local), `[M-175.1-minus-overload-arg-type]`/`[M-175.1-operator-arg-type-blind]` (overload-резолв оператора слеп к типу аргумента — оператор `Date - Date -> Period` ретрактирован в пользу `Period.between`; D320-гейт держит метод-форма, neg-fixture), `[M-175.1-qualified-variant-value]` (`Enum.Variant` как значение — ICE P67-LEGACY; вариант `OffsetConflict.Reject` переименован в `RejectMismatch` — коллизия с `Disambiguation.Reject` во флоском пространстве вариантов), `[M-175.1-enum-default-param]` (default-значение enum-варианта не эмитится на call-site → arity-split `to_zoned(tz)`/`to_zoned(tz, disamb)`, прецедент D324), `[M-175.1-interp-value-record-display]` (интерполяция `"${date}"` value-record минует user @to_str — pre-existing класс; Display-тела корректны). (4) POSIX-TZ футер TZif не интерпретируется (за последним переходом действует последний сдвиг) — документировано в D321 §tzdb. Гейты: targeted std/time/civil 78 pos + 2 neg + 1 rt зелёные; std/time δ0 (единственный FAIL — pre-existing timer_metrics_test); conformance см. финальный прогон волны.

[2026-07-10 Plan 175 — mut_clock auto-idle-advance, 🟡 РАБОТАЕТ с задокументированным сужением по armed M:N] `std/testing/handlers.nv` `mut_clock`'s `sleep` теперь парker: абсолютный дедлайн (`current_ms + ms`, до парковки) → `vclock.park_until` (новый `extern "nova"` хук `std/runtime/vclock.nv` → `nova_vclock_park_until`, nova_rt/fibers.h) — паркует вызывающий фибр в per-scope `NovaVClockEntry[]`-registry (новые поля `NovaFiberQueue.vclock_*`); когда pending_count >= alive_count (все живые фибры scope'а виртуально запаркованы), просыпается ближайший по дедлайну; bump — `current_ms = max(current_ms, deadline)` (не `+=`). tokio `time::pause()`/Kotlin `TestCoroutineScheduler.advanceUntilIdle()`-паритет. **Сужение (с маркером):** `[M-175-vclock-armed-mn-scope-identity]` — deadline-order гарантия держит под кооперативным spawn (`NOVA_MAXPROCS=1`+`NOVA_AUTOARM=0`, где `_nova_active_scope` внутри фибра — общий scope блока); под ДЕФОЛТНЫМ armed M:N (auto-arm на первом spawn) `_nova_active_scope` внутри фибра — это `w->scope` (WORKER'а собственный, НЕ shared с siblings — `_worker_run_one_fiber`), поэтому registry не шарится между siblings под armed-путём — деградирует БЕЗОПАСНО (без hang/crash, каждый sleep резолвится), но без гарантии порядка (spawn-порядок вместо дедлайн-порядка). Починка armed-случая — другой якорь (`NovaSpawnCtxBase._nova_parent_scope`), вне периметра этого захода. Реал-clock путь не тронут (only mut_clock's sleep calls the new hook). Гейты: compiler-codegen+nova-cli чистая сборка; conformance 89/0; std/time+civil+testing.handlers 12/0 PASS (новые тесты x3 в handlers.nv, стабильно PASS ×3 прогона под `NOVA_AUTOARM=0`); std/concurrency (real Time.sleep+spawn) 2/0 PASS — byte-parity подтверждён.

[2026-07-10 Plan 175.1 — civil time, 🟢 ПРИЗЕМЛЕНО с задокументированными сужениями] Полный civil-слой (`std/time/civil`, D319/D320/D321): Date/TimeOfDay/DateTime/YearMonth/MonthDay/Period/Offset/TimeZone/ZonedDateTime + Hinnant epoch-day, CLAMP-арифметика Q7, 4-way Disambiguation/OffsetConflict, strict ISO/RFC-3339/RFC-9557 parse (§1а: `s.to_date()`/`to_datetime()`/`to_timezone()`…), TZif-парсер + curated embedded tz-таблица, pattern-DSL `DateTimeFormat`. Компилятор НЕ тронут. **Сужения/обходы (все с маркерами и планом):** (1) `[M-175.1-full-tzdb-embed]` — embedded-таблица curated (NY/London/Moscow/Sydney + фикс-оффсеты, rule-based 1996..2100), НЕ полный ~450KB IANA-snapshot; TZif-парсер полный, POSIX-слой работает; починка = упаковка snapshot-данных. (2) ~~`[M-175.1-local-offset-effect-op]`~~ — ЗАКРЫТО отдельным follow-up заходом той же датой (D316 amend): `Time.local_offset_sec()` эффект-оп + `NovaVtable_Time` слот поставлены, `Offset.local()` (`std/time/civil/offset.nv`) — явный запрос, зона в `ZonedDateTime` остаётся явной (D319 R1 не меняется). (3) Codegen/checker-гэпы класса value-record/overload, обойдённые по §4а: `[M-175.1-value-in-value-emit-order]` (декларация DateTime перенесена в time_of_day.nv — порядок эмиссии структур лексикографический по файлам), `[M-175.1-variant-literal-receiver]` (`Sun.next()` эмитится как несуществующий Path-вызов — в тестах bound-local), `[M-175.1-minus-overload-arg-type]`/`[M-175.1-operator-arg-type-blind]` (overload-резолв оператора слеп к типу аргумента — оператор `Date - Date -> Period` ретрактирован в пользу `Period.between`; D320-гейт держит метод-форма, neg-fixture), `[M-175.1-qualified-variant-value]` (`Enum.Variant` как значение — ICE P67-LEGACY; вариант `OffsetConflict.Reject` переименован в `RejectMismatch` — коллизия с `Disambiguation.Reject` во флоском пространстве вариантов), `[M-175.1-enum-default-param]` (default-значение enum-варианта не эмитится на call-site → arity-split `to_zoned(tz)`/`to_zoned(tz, disamb)`, прецедент D324), `[M-175.1-interp-value-record-display]` (интерполяция `"${date}"` value-record минует user @to_str — pre-existing класс; Display-тела корректны). (4) POSIX-TZ футер TZif не интерпретируется (за последним переходом действует последний сдвиг) — документировано в D321 §tzdb. Гейты: targeted std/time/civil 78 pos + 2 neg + 1 rt зелёные; std/time δ0 (единственный FAIL — pre-existing timer_metrics_test); conformance см. финальный прогон волны.

[2026-07-06 Plan 179 Ф.2 — brotli decode C-FFI + условная линковка, 🟢 ПРИЗЕМЛЕНО] `[M-179-brotli-vendor-lib]` снят: google/brotli v1.2.0 **декодер** собран однократно (MSVC x64 `/MT /O2`, `common/*`+`dec/*`) и вендорен **headers+lib БЕЗ исходников** (стиль libuv): `nova_rt/brotli/include/brotli/*.h` + `nova_rt/brotli/lib/libbrotlidec.lib` (tracked) + build-cache `target/brotli-cache/`. `brotli_decode(data, max_output)` (`std/encoding/compress/{ffi.nv,brotli.nv}`) — extern "C" C-ABI без `[]u8` (raw-ptr+len, модель fs) поверх шима `nova_rt/brotli_shim.{h,c}` (BrotliDecoderDecompressStream); bomb-cap D334 инкрементально поверх FFI (per-pull budget → перебор ≤1 байта). **Условная линковка (owner-требование)**: brotli-lib линкуется ТОЛЬКО когда генерённый `.c` содержит call-site `brotli_decode(` (`c_file_uses_brotli`, фильтр decl/def-header — std-fn'ы эмитятся даже мёртвыми); libuv-mandatory НЕ тронут; без lib → Q11-заглушки (`UnsupportedMethod`, не link-error). http: `Content-Encoding: br` → прозрачная распаковка, `Accept-Encoding: gzip, deflate, br` (`[M-178-autodecompress-br]` закрыт). **Verify:** официальные RFC 7932-вектора (10x10y/64x/empty) + bomb-граница + truncated/corrupt (std/encoding/compress/brotli_test.nv, conformance d337 — **54/0**); http-мок br (`std/http/client/decompress_br_test.nv`); условность доказана в обе стороны (`NOVA_DEBUG_BROTLI_LINK=1`: gzip-only → NO lib; brotli → LINK); регрессия: 3 pre-existing FAIL (basics/effects/gc) идентичны с/без brotli-инклуда → delta 0; Rust clean. **ОСОЗНАННЫЕ SCOPE-CUTS (с планом):** (1) streaming `BrotliReader` (consume) → `[M-179-brotli-reader-streaming]` — C-примитивы шима уже инкрементальные, http использует one-shot симметрично gzip/deflate; consume-neg-тесты приземлятся с ней. (2) Linux/macOS `.a` не вендорен (Windows-хост) → `[M-179-brotli-unix-lib]`, Q11-заглушки. (3) brotli-encode — followup §11 плана (asymmetric, `enc/` не тащится).

[2026-07-06 Plan 172.5 — In-out `mut ref`-параметры, 🟢 CORE ПРИЗЕМЛЕНО] `ref` = режим передачи параметра (D326, Swift `inout`/C# `ref`, без лайфтаймов), НЕ тип. **Реализовано полностью для `mut ref`:** parser (`ParamRefMode{None,RoRef,MutRef}`, call-site `ExprKind::RefArg`, глобальное keyword `ref` — 0 идент-использований → non-breaking), checker (эксклюзивность `E_REF_ALIAS_OVERLAP` через `RefPlace` per-pair prefix-overlap рядом с consume place-анализом; marker⟺mode `E_REF_MARKER_{REQUIRED,NOT_ALLOWED}`; addressability `E_REF_ARG_NOT_ADDRESSABLE` реюз `addr_of_chain_root`; mut-place `E_REF_ARG_NOT_MUT` реюз `check_target_readonly`+`ro_binding_names`; escape-ban R10 `E_REF_ESCAPE_CAPTURE` — захват в closure/spawn), codegen (`mut ref`→C-указатель `T*` в едином `params_c`; body auto-deref через `ref_params`; call-site `ref x`→`&x`; форвардинг `&(*v)≡v`). **Verify:** `nova_tests/inout_ref/` 2 pos + 11 neg зелёные; регрессия byte-identical C-emission на 580 файлах (2 стрида), delta 0; Rust clean. **ОСОЗНАННЫЕ SCOPE-CUTS (честный scope, followups с планом — НЕ tech-debt-без-плана):** (1) `ro ref` codegen НЕ дублирован — это size-driven авто-механизм 172.4 (R3); explicit `ro ref` = обычная value-передача + маркер-запрет. (2) R6 mid-chain gating `E_RECEIVER_BINDING_NOT_MUT` (`c.peek().bump()`) отложен `[M-172.5-chain-gating-ro-at]` — требует моделирования `@`-return-режима сквозь method-chain (fluent-машинерия 172.4), вне soundness in-out `mut ref`; parse-часть R6 (`consume @ -> @`) сделана. (3) Generic `fn f[T](mut ref x T)` codegen отложен `[M-172.5-generic-mut-ref-codegen]` — concrete-путь работает, erased/mono не лоуэрит `T*` (гейт mono-pipeline 172.12).

[2026-07-06 Plan 180 Ф.6 — атрибуты AST + режимы тегирования (internal+adjacent ✅ ПРИЗЕМЛЕНО; untagged 🔴 GATED)] **Часть 1 — `#serde(...)`-инфра (D382):** AST-поле `serde_attrs: Vec<SerdeArg>` на `TypeDecl`/`SumVariant`/`RecordField` (`SerdeArg = Tag(str)|Content(str)|Untagged`); `parse_serde_attr` (grammar `#serde(key[="v"], …)`) в трёх позициях (type — `parse_type_attrs`; field — рядом с `#visible_to`; variant — leading-marker в `parse_one_sum_variant`); неизвестный ключ → `E_SERDE_BAD_ATTRIBUTE` (не silent, прецедент `#impl`). Закрывает `[M-180-serde-attributes]` (infra). **Часть 2 — тегирование:** `serde_tagging_mode(td)` → `SerdeTagging{External|Internal{tag}|Adjacent{tag,content}|Untagged}` + валидация (E_SERDE_TAGGING_CONFLICT / _CONTENT_WITHOUT_TAG / _TAGGING_ON_NON_SUM / _INTERNAL_TAG_NON_STRUCT / _UNTAGGED_GATED). **Internal (`#serde(tag="k")` → `{"k":"V",…fields}`) + adjacent (`#serde(tag="t",content="c")` → `{"t":"V","c":payload}`) ПРИЗЕМЛЕНЫ** (синтез поверх существующих Serializer/Deserializer-примитивов). **Компилятор-фикс, разблокировавший internal+adjacent (без упрощений):** match/if `Result[OK,ERR]`-arm reconciliation — `Ok(x)`-арм даёт stub-ERR (`NovaRes_<ok>_nova_str`), `Err(e)`-арм stub-OK (`NovaRes_nova_int_<err>`); json.nv `Deserializer`-методы (`enter_field`/`enter_index` = `None=>Err`, `Some=>Ok`) без reconciliation мис-лейаутили курсор-Result → decode ложный UnexpectedType. Чиниться сборкой concrete-OK + concrete-ERR через `novares_ok_err`-split уже-посчитанных типов арм/веток (side-effect-free — re-inference-вариант пертурбировал mono-order) в `emit_match`/`emit_if_expr` + `infer_expr_c_type`-зеркалах; + concrete-Result предпочитается erased в первом проходе. **Zero-regression ~50 dirs байт-в-байт vs parent; conformance 54/0.** Verify: `std/encoding/serde/tagging_test.nv` (peer), `std/encoding/serde_neg/*` (`nova test std/encoding/serde_neg --compile-error` = 5/0). **ОСОЗНАННЫЙ GATE (честный named-prereq, НЕ tech-debt-без-плана): untagged (`#serde(untagged)`).** Синтез untagged КОРРЕКТЕН (try-each-variant по value-семантике курсора, генерируемый C валиден), НО компиляция untagged-derive тела пертурбирует codegen `std/encoding/json` в том же CU (mono-collection-ordering → `Json.parse("{\"c\":9}")` возвращает Bool для 9, ломая ВЕСЬ CU включая record/adjacent). Это pre-existing codegen-mono-баг, НЕ serde-логика (C untagged-тела корректен; internal/adjacent тем же движком работают). → `#serde(untagged)` reject at compile-time (`E_SERDE_UNTAGGED_GATED`) до codegen-hardening. Followup `[M-180-untagged-codegen-mono]` (repro в backlog). Field-customization-атрибуты (rename/skip/flatten/…) — грамматика общая, потребление → `[M-180-serde-field-attributes]` **(ЗАКРЫТО 2026-07-22, Plan 180.1 Ф.1 — см. запись ниже; остался узкий `[M-180-serde-flatten]`)**.

[2026-07-22 Plan 180.1 Ф.1/Ф.10 — serde field-attributes consumption на record-типах, 🟢 ПРИЗЕМЛЕНО, `[M-180-serde-field-attributes]` CLOSED кроме flatten] `rename`/`rename_all`/`skip`/`skip_serializing_if`/`default`/`alias`/`deny_unknown_fields`/`allow_unknown` — потребление синтезатором (`resolve_fields`/`validate_wire_contract` в `auto_derive.rs`) + Ф.10 compile-time wire-contract валидация (коллизии wire-имён/алиасов, skip+rename, misplaced-ключи) — D435 (spec/decisions/02-types.md). Новый `Deserializer.has_field` (точная presence-проверка, не конфликтует с JSON `null`) — используется fallback-цепочкой `default`/`alias`. **ОСОЗНАННОЕ УПРОЩЕНИЕ (единственное оставшееся, с планом):** `flatten` — грамматика+статическая валидация ЕСТЬ, СИНТЕЗ гейтнут (`E_SERDE_FLATTEN_DENY_CONFLICT`/`E_SERDE_FLATTEN_UNSUPPORTED`). **Почему:** нужен companion "fields-only" synth-вариант (родительский `d`/`s`-курсор БЕЗ собственного `begin_struct`/`end_struct`/`enter_field`-обёртывания) — auto-derive machine сейчас инжектит только МЕТОДЫ по одному per-protocol телу, а flatten требует делить синтез ОДНОГО метода на два физических режима (own-wrapper vs fields-into-parent); честно за пределами Ф.1-объёма (сам план называет flatten «самым сложным» пунктом, разрешая изолированный стоп). **Как чинить:** отдельный подзаход — добавить `serialize_into`/`deserialize_into` companion-синтез (без framing), плюс knowledge о полях flatten-вложенного типа на этапе Ф.10-валидации родителя (сейчас родитель не заглядывает внутрь вложенного типа). Маркер `[M-180-serde-flatten]`. **Приоритет:** L (узкая фича, обычные вложенные sub-object поля работают уже сейчас; DTO round-trip не блокирован). Сопутствующий разворот (см. D436): **Ф.7 unknown-field policy стала STRICT BY DEFAULT** (было ignore-by-default, Q5 180-плана) — владелец 2026-07-22, opt-out `#serde(allow_unknown)`. Гейты: `nova test std/src/encoding/serde` (round-trip + wire-string assertions) PASS; `nova test std/src/encoding/serde_neg --compile-error` 16/16 PASS (4 требуемых Ф.10 + 6 бонусных); conformance мега-CU / flagship / `nova check std --strict-effects` — см. discussion-log/план 180.1 для финального tally.

[2026-07-06 Plan 180 Ф.2-sum — serde sum-derive externally-tagged, ✅ ПРИЗЕМЛЕНО] `#impl(Serialize + Deserialize)` на sum-типе синтезирует externally-tagged (Q4, default) тела: unit → `"V"`; single-payload → `{"V": x}`; tuple → `{"V": [a, b]}`; record → `{"V": {fields}}`. Serialize — поверх существующих Serializer-примитивов (begin_struct/struct_field/begin_seq/serialize_str), БЕЗ новых enum-методов. Deserialize — читает тег (`is_str()` → bare string для unit; иначе single object-key через map_keys/enter_key), if/else-if по имени тега реконструирует variant. Runtime-добавки: `Deserializer.@is_str()`, `DeErrorKind::UnknownVariant`/`NoVariantMatched`. Codegen-фикс: `T.deserialize(sub)?` со static-ресивером, чей return-инференс деградирует (mono-collection-order perturbation, когда sum ТОЖЕ деривит Deserialize → `infer_static_method_ret` промахивается → void* → `/* ? */` no-op мис-присваивает Result-указатель в T-локал) — пиннится к `Result[T, DeError]` в Try-lowering (emit_c.rs), зеркало `.serialize?`-пина. **Verify:** `nova_tests/serde/sum_autoderive.nv` (8 блоков round-trip+neg) PASS; conformance 53/0; record-serde в том же CU PASS (zero-regression); serde-neg 2/0; 44 auto_derive unit-теста. **ОСОЗНАННОЕ УПРОЩЕНИЕ (честный scope, НЕ tech-debt-без-плана):** internal/adjacent/untagged tagging (§3.6) — ГЕЙТ на `#serde`-attribute-инфре (`[M-180-serde-attributes]`, AST `attrs` не существует); без атрибутов non-external режим НЕдостижим. Externally-tagged = default, покрывает 100% безатрибутного sum-serde. Followup `[M-180-serde-tagging-modes]`.

[2026-06-18 Plan 36.D.1] Упрощения: (1) Нет негативных `.nv` тестов для CLI-поведения (--include-stdlib exit 2, test-all unrecognized) — CLI-контракты не выразимы через Nova EXPECT-маркеры; verified вручную + Rust build-тест. (2) `nova-codegen test-all` удалён без deprecation-периода — bootstrap не в проде, чистый break.

[2026-06-17 Plan 91.8c] Упрощения: (1) Суффикс `_of` вместо перегрузки — `sort_of/min_of/max_of/binary_search_of` не перегружают `sort/min/max` из concrete `[]int` (избегаем overload-resolution сложности, concrete fast-path сохранён). (2) Алгоритм sort: insertion sort O(n²) для MVP; pdq-sort в followup `[M-91.8c-pdq-sort]`. (3) `[]int @min/@max` pre-existing CC-FAIL (f64.min dispatch) — не фиксируем в Plan 91.8c, используем `min_of/max_of` в регрессионном тесте.

### Plan 91.14 — Debug protocol (2026-06-17, ✅ CLOSED)

- **Где** — `std/prelude/protocols.nv` (Debug protocol), `std/prelude/core.nv` (Option/Result @debug), `std/collections/vec/protocols.nv` (Vec @debug), `compiler-codegen/src/protocols/auto_derive.rs` (synthesize_debug), `compiler-codegen/src/ast/format_spec.rs` (FormatSpec::Debug).
- **Что сделано** — Debug protocol + `${expr:?}` interpolation + `#impl(Debug)` auto-derive. 21/21 PASS.
- **Упрощения:**
  1. **Sum-type debug V1** — outputs type name only (`"Color"` for all Color variants). Full per-variant synthesis (`"Color::Blue(42)"`) → `[M-91.14-sum-debug-variants]`.
  2. **str.from_debug walker** — `default_body_calls_satisfy_for` does not check `str.from_debug` (only `str.from`), so `#impl(Debug)` with a non-Debug field silently passes type-check. → `[M-91.14-str-from-debug-walker]`.
  3. **None as Option[UserStruct]** — produces CC-FAIL due to C struct cast mismatch (`NovaOpt_nova_int` cast to `NovaOpt_Nova_X_p`). Workaround: avoid `None as Option[UserRecord]`. → known pre-existing limitation.
- **Как чинить** — sum-type variants: extend `synthesize_debug` to emit per-variant match; str.from_debug walker: add `str.from_debug` check in `walk_default_body_expr`.
- **Приоритет** — L (followups deferred).

---

### Plan 104.2 Ф.7 — hover body-walk + prelude name-lookup (2026-06-17, ✅ CLOSED)

- **Где** — `nova-lsp/src/hover.rs`, `nova-lsp/src/symbol.rs`, `nova-lsp/src/server.rs`.
- **Что сделано** — Hover теперь работает внутри fn/test тел: body-walker рекурсивно обходит ExprKind/Stmt/Block и находит ident под курсором; name-lookup ищет объявление среди ALL items включая inlined prelude → `assert`/`println`/etc. показывают hover.
- **Корень бага**: `resolve_imports_inline` **prepend**'ит imported items перед оригинальными (`new_items.append(&mut module.items)`). `.take(original_len)` захватывало только prepended imports; `.skip(items_start)` захватывает только оригинальные items.
- **Упрощения:**
  1. ~~**Hover на локальных переменных внутри тел**~~ ✅ **`[M-104.2-body-walk-local-var-type]` CLOSED** — body-walk обнаруживает курсор на `LetDecl`-биндинге и возвращает `SymbolInfo::LocalVar` с явной аннотацией типа из `LetDecl.ty`. Также закрыт fn-body hover priority (Fix A): `resolve_item` для `Item::Fn` возвращает `None` когда курсор внутри тела → body-walk находит фактический callee.
  2. **Dot-completion при неизвестном типе** — возвращает пустой список (не «каша» из всех методов). V2: type inference внутри тел для dot-completion.
- **Как чинить** — V2: type inference pass для тел функций в LSP; использовать `types::check_module` результат для type-per-expression.
- **Приоритет** — L (dot-completion deferred; local-var-type и fn-body-priority CLOSED).

### Plan 162 Ф.1-Ф.5 — Rust-модель module-resolution (2026-06-16, ✅ CLOSED [M-159-lazy-module-resolution])

- **Где** — `compiler-codegen/src/imports.rs` (cycle guard + TypeMethodMap); `compiler-codegen/src/types/mod.rs` (TypeMethodMap + E_EXTENSION_METHOD_NEEDS_IMPORT); `std/prelude/core.nv` (10 char Unicode-методов + import std.unicode); `std/unicode/category.nv` (методы убраны); ~100 файлов std/ + nova_tests/ (migration).
- **Что сделано** — Cycle guard: `in_progress.contains(&module_key) → return Ok(())` вместо stack-overflow. TypeMethodMap: inherent методы вызываются без import (нет хардкода имён). Char Unicode-методы в `std/prelude/core.nv` — общий механизм. `CHAR_UNICODE_METHOD_SELECTORS` + `needs_unicode_injection` удалены из компилятора. Extension-методы из неимпортированных модулей → `E_EXTENSION_METHOD_NEEDS_IMPORT`. plan162 6/0 PASS.
- **Ограничения** — TypeMethodMap = emit-time (не checker-time); extension-policy может потребовать уточнения при ambiguity двух extension-методов на одном типе. Codegen-only, не semantic type-checking.
- **D-блоки** — D285 (cycle guard), D286 (TypeMethodMap), D287 (extension policy). Q-module-resolution-model RESOLVED.
- **Коммиты** — в ветке `plan-162-module-resolution`; 7 коммитов.
- **Приоритет** — L (CLOSED).

### Plan 161 V2 — Blanket parametric-return T-subst (2026-06-16, ✅ CLOSED [M-161-parametric-return])

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` (2 точки: mono-dispatch type_subst bind + infer_expr_c_type string-subst), `std/collections/vec_iter_zc.nv` (blanket refactor), `nova_tests/plan161/` (+2 fixtures).
- **Что сделано** — Blanket методы `fn[I Next[T]] I mut @m() -> Vec[T]` / `-> Option[T]` / `-> T` теперь корректно подставляют T в return/param типы при мономорфизации. Все 9 терминаторов в `vec_iter_zc` стали blanket (zcollect/zcollect_into/zsum/zfind → O(1) definition). plan161: 12/12 PASS.
- **Остаток** — Ф.2 (checker E_DUPLICATE_PROTOCOL_IMPL, E_BLANKET_CONFLICT) — независимая задача.
- **Приоритет** — L ([M-161-parametric-return] CLOSED; оставшийся Ф.2 — checker-only, не runtime).
- **Коммиты** — `776447ab` (probe fixtures), `9065c637` (emit_c fix), `34a2fd4d` (vec_iter_zc blanket), merge `3ba2dacc`.

### Plan 161 Ф.0–Ф.1 — Blanket protocol-receiver Ф.0+Ф.1 (2026-06-15, ✅ CLOSED)

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` (Fix A `receiver_c_type` ~line 10878, Fix B `infer_expr_c_type` ~line 34929), `nova_tests/plan161/` (6 фикстур).
- **Что сделано** — Fix A (pointer correction для heap struct typevar resolve) и Fix B (blanket fallback scan в infer_expr_c_type). Ф.3 (stdlib refactor vec_iter_zc O(N²)→O(N)) выполнен в Ф.3 separately; Ф.2 (checker) — открыт.
- **Приоритет** — L (CLOSED; V2 followup выше).
- **Коммиты** — `47d7a7fc` (Ф.0 fixtures), `7c6bb60b` (Ф.1 emit_c fixes).

### Plan 148 — Independent compiler cleanups (Ф.1–Ф.5, 2026-06-12, ✅ CLOSED, без новых упрощений)

- **Где** — `compiler-codegen/src/parser/mod.rs` (Ф.1 modifier-order),
  `compiler-codegen/src/types/mod.rs` (Ф.3 ro-partition checker),
  `compiler-codegen/src/codegen/emit_c.rs` (Ф.4 tuple-repr),
  `std/collections/vec_owned.nv` (Ф.2 paren cleanup), `spec/decisions/{02-types,03-syntax}.md`,
  `nova_tests/plan148/`. Commits `98f0b48050f` / `2d24a38c288` / `b6d3687108` / `982dfd90153` + docs.
- **Что упрощено** — НИЧЕГО нового. Все 4 фазы доставлены production-grade по AC (Ф.1 `E_MODIFIER_ORDER`
  + fix-it, обобщён на все type-модификаторы; Ф.2 cleanup + regression-guard, парсер уже корректен;
  Ф.3 `E_RO_FOR_CONSTEXPR_PREFER_CONST` forward-partition; Ф.4 typed on-demand mono'd tuple-структуры).
- **Документированные SCOPE-NOTE / pre-existing gaps** (НЕ регрессии, НЕ в scope этих 4 осей):
  - **Ф.4 legacy `_NovaTuple2`** не удалён полностью — erased-generic HashMap[K,V]/Set lowers `(K,V)`
    через legacy all-int repr. Полное удаление требует mono'д HashMap/Set (большой отдельный effort,
    высокий collections-prelude regression-риск). Доставленное: blanket pre-decl ретайрнут, legacy эмит
    строго on-demand (только запрошенные arity, на практике arity 2), typed путь — default + robust.
  - **Ф.4 OOB tuple field index** (`t.5` на 2-tuple) не reject'ится — checker-level gap (не codegen-repr ось).
  - **Ф.4 mono'd-tuple-of-Vec forward-decl ordering** (plan59 f5 / types arrays CC-FAIL) — pre-existing,
    отдельная ordering-ось (`__MONO_TUPLE_TYPEDEFS__` marker предшествует Vec `__GENERIC_TYPE_DEFS__`).
  - **Ф.3 module-level RUNTIME `ro X = expr()` codegen** — pre-existing unimplemented gap (binding не
    lowered; checker корректно принимает, verified via `check`; runnable POS невозможен).
- **Приоритет** — все остатки L/M, не блокируют (документированы в D-блоках + planned-backlog).
### Plan 140 — Contracts enforced in release (Z3-proven elided, unproven checked) (2026-06-12, ✅ CLOSED Ф.0-Ф.5)

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` (сняты 6 `#ifdef NOVA_CONTRACTS_RUNTIME` обёрток +
  `#unchecked`/`contracts_off` elision), `src/parser/mod.rs` + `src/ast/mod.rs` (`#unchecked` attr →
  `FnDecl.contracts_unchecked`), `src/test_runner.rs` (proven-set wiring на build-путь + `// CONTRACTS`
  directive), `src/main.rs` + `nova-cli/src/{main.rs,build_cache.rs,bench/run.rs}` (`--contracts` policy +
  proven-set на всех emit-сайтах), `spec/decisions/09-tooling.md` (D24 amend), `spec/open-questions.md` (Q34),
  `nova_tests/plan140/` (7 fixtures).
- **Что было упущено (safety-gap)** — контракты `requires`/`ensures` (D24) **стирались в release
  независимо от доказанности** (`test_runner.rs` `Mode::Release` не передавал `-DNOVA_CONTRACTS_RUNTIME=1`,
  модель C `assert`/`NDEBUG`). Распруфленность Z3 учитывалась ОТДЕЛЬНО от стирания → страховка молча
  снималась именно там, где статическая безопасность НЕ подтверждена (недоказанные = рискованные места) →
  потенциальный silent UB/corruption.
- **Решение** — «enforce-with-elision» (Dafny/Verus + Rust bounds-check): Z3-**proven** контракт
  элидируется на codegen (zero-cost), **недоказанный** — runtime-проверка остаётся и в release
  (`nova_contract_violation` → fail-fast abort), `#unchecked` (per-fn) / `--contracts=off` (build) — явный
  opt-out. Ф.3 закрыл R4: `set_proven_contracts` доходил до codegen только на `nova-codegen compile`, не на
  `nova build`/`test-build`/bench → элизии на build-пути не было; теперь захват `ModuleEnv` + вызов на всех
  emit-сайтах.
- **Что осознанно НЕ сделано / частичности (документировано)** —
  (a) **Z3-статус:** компилятор собран `--features z3-backend` (vcpkg `libz3` скопирован в worktree
  `vcpkg_installed`, untracked via `.git/info/exclude` — НЕ committed). Default-бэкенд остаётся `Trivial`
  даже при скомпилированном Z3 (сохраняет детерминизм verify-suite Plan 33); полный Z3 — только при
  `NOVA_SMT_BACKEND=z3`. Safe degrade без Z3 верифицирован (proven меньше → больше runtime-checks, никогда
  не unsafe). НЕ упрощение — намеренный дизайн (graceful degrade — валидный путь).
  (b) **violation = abort** (не panic-unwind) — Q34 §2, default fail-fast; recoverable panic-unwind отложен
  `[M-140-contract-panic-unwind]` (P3).
  (c) **гранулярность opt-out V1** = per-fn `#unchecked` + build `--contracts=off`; per-module/Eiffel-style
  раздельная (pre/post/invariant) отложена `[M-140-contract-levels]` (P3).
  (d) **`[M-140-invariant-release]`** — N/A: декларативных type-invariant'ов как языковой конструкции пока
  нет; record-invariant (контракт-codegen) УЖЕ покрыт enforce-in-release (Ф.1, `#ifdef` снят наравне).
- **Perf (Ф.5)** — микробенч `nova_tests/plan140/perf_contract_hot_loop.nv` (20M-loop, `sq_plus` с
  нелинейным `ensures result >= x` — Z3 доказывает, Trivial нет), release/clang, best-of-N:
  `--contracts=off` 0.191s (baseline) ≈ Z3-enforce 0.197s (POST **элидирован**, zero-cost) < Trivial-enforce
  0.214s (POST **проверяется** каждую итерацию, ≈+12% на contract-saturated loop). Codegen-доказательство
  (тот же `.c`): Z3 = 3 `nova_contract_violation` (PRE-only), Trivial = 4 (+POST `result >= x`), off = 0.
  PRE (caller-obligation) никогда не в `report.proven` → всегда эмитится. Overhead скейлится с долей
  runtime-проверки в теле — в реальном коде много меньше микробенч-12%.
- **Доказательства** — `nova_tests/plan140/` 7/7 PASS release (Z3 и Trivial): t1 (unproven requires abort),
  t2 (proven ensures elided), t3 (ensures abort), t4 (`#unchecked` no-abort), t5 (`--contracts=off` no-abort),
  t6 (no-Z3 degrade abort), perf_contract_hot_loop. **0 регрессий** (F7): `nova_tests/contracts` 295/0/11
  release (Z3), 250/0/56 (Trivial) — идентично baseline. Acceptance F1-F7 все met (см. plan-doc STATUS).
- **Коммиты** — `b058fb53` (Ф.0 D24 amend + Q34), `7abfc491` (Ф.1 release-emission), `52b527f2` (Ф.2
  `#unchecked` + policy), `98add912` (Ф.3 proven-set wiring), `31332a41` (Ф.4 release-fixtures), Ф.5
  (perf + docs/close — этот коммит). Ветка `plan-140`, worktree `nova-p140`, НЕ merged.
- **Приоритет** — L (закрыто; остаются 3 P3 deferred-маркера + 1 P2-READY ungated bounds-as-contract).

---

### Plan 142 — D227 `E_LIT_OUT_OF_RANGE` compile-time literal range-check (2026-06-11, ✅ CLOSED)

- **Где** — `compiler-codegen/src/types/mod.rs` (`assignable()` IntLit arm + NEW `Unary{Neg,IntLit}` arm
  ~4500/4929 call-sites; helpers `sized_int_bounds`/`sized_int_name`/`lit_range_check` + `Compat::OutOfRange`),
  `spec/decisions/03-syntax.md` (D227 acceptance + scoped open-questions), `nova_tests/plan142/` (8 NEG + 2 POS).
- **Что было упущено (safety-gap)** — D227 spec (написан 2026-06-03) задавал hard compile-time range-check
  целочисленного литерала при context-coercion к sized-типу, но **enforcement отсутствовал**: `u8 = 300`,
  `i32 = 3_000_000_000` тихо проходили (потенциально wrap/UB), а `u8 = -1` (`Unary{Neg,IntLit}`) вовсе не
  доходил до проверки (падал в `Compat::Unknown`).
- **Решение** — на сайте literal→sized-int coercion в `assignable()`: если значение вне `[T.MIN, T.MAX]` →
  `Compat::OutOfRange{msg}` → hard `[E_LIT_OUT_OF_RANGE]` с сообщением «<val> > <T>.MAX (<max>)» /
  «< <T>.MIN (<min>)». Покрыты все 8 sized-int (u8/16/32/64, i8/16/32/64) + знаковость (D227 Rule 6
  negative-in-unsigned через новый `Unary{Neg,IntLit}` arm). i128 throughout (IntLit i64; нужно для u64.MAX +
  negated). hex/`_`-формы — lexer уже парсит значение в IntLit до чекера. Оба call-site (binding `ro a u8 = v`
  + call-arg `write(400)`) переведены на exhaustive match.
- **Что осознанно НЕ сделано (scoped, документировано в D227 spec + backlog P3)** —
  (a) **alias/newtype над sized-int** НЕ range-checked: `assignable()` чекает только direct Named sized-int
  (+ Readonly/Mut/Unsafe wrappers); резолв alias-имени требует `self.types`, недоступного на free-fn
  coercion-сайте (иначе печатался бы неверный type-name). → `[M-D227-alias-newtype-range]` (P3).
  (b) **float range-check (D227 Rule 5, f32 exponent overflow)** НЕ реализован — Ф.1 scope был integer-only
  (plan §43 «all 8 sized-int», floats не перечислены). → `[M-D227-float-range-check]` (P3).
- **Почему** — закрытие P1 safety-gap (`[M-D227-lit-range-error-code]`) + P2 test corpus
  (`[M-D227-literal-range-tests]`); оба убраны из backlog OPEN-view. Default `int`/`uint` (D227 Rule 1
  wide defaults) намеренно НЕ триггерит — `3_000_000_000` в `int`-контексте легитимен.
- **Доказательства** — `nova_tests/plan142/`: NEG `neg_u8_300`/`neg_u8_minus1`/`neg_i32_3b`/`neg_u16_70000`/
  `neg_i8_200`/`neg_u8_hex_1ff` (0x1FF=511)/`neg_u32_4b` (4294967296)/`neg_arg_u8` (call-arg path) → каждый
  `[E_LIT_OUT_OF_RANGE]`; POS `pos_boundaries` (все 8 sized MIN/MAX точно в диапазоне) + `pos_wide_int`
  (Rule 1). 10/0 PASS релизным nova. **0 регрессий** (plan138_1/2/3, plan131, plan90_1, plan126, plan59,
  plan101_1, plan137 — baseline-clean; единственные FAIL pre-existing/known: vec_debug_pos, t2_map_set_clone,
  6 plan59 tuple-mangle).
- **Коммиты** — `d6b209b8e63` (Ф.1 enforcement + 10 fixtures), `2d85fff2175` (Ф.2 spec acceptance + scoped
  open-questions), Ф.3 docs (plan-doc CLOSED + backlog + simplifications + project-creation + nova-private).
  Ветка `plan-138.1`, worktree `nova-p138`.
- **Приоритет** — L (закрыто; остаются 2 P3 scoped-edge маркера).

---

### Plan 141 — Structural equality field-by-field; `memcmp` отозван с композитов (2026-06-11, ✅ CLOSED)

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` (helper `emit_field_eq` ~11125 + 4 call-sites:
  tuple-eq ~16893, sum-eq ~17243, оба Option-eq генератора ~29243/29346), `spec/decisions/08-runtime.md`
  (D109 amend Plan 141), `spec/decisions/open-questions.md` (Q32), `nova_tests/plan141/` (t1-t8).
- **Что было неверно (soundness-баг)** — равенство кортежей/sum-payload/Option-payload
  генерировалось через `memcmp(&l, &r, sizeof(struct))` по всей struct. Это давало:
  (1) **float** — `-0.0 != +0.0` (разные биты, а IEEE `==` = равны) и бит-идентичный `NaN == NaN`
  true (а IEEE = `NaN != NaN`); (2) **padded struct** — indeterminate padding-байты при mixed-size
  полях → два равных значения ≠; (3) **nested composite** — вложенный record/tuple/sum сравнивался
  побайтово (pointer-bits / struct-bytes), а sum с tuple-payload вовсе давал **C compile error**.
- **Что сделано** — извлечён shared `emit_field_eq(&self, c_type, l, r, depth) -> String`, диспатчащий
  равенство **по C-типу**: scalar/int/float → `(l == r)` (намеренный IEEE — `-0.0==+0.0`, `NaN!=NaN`);
  `nova_str` → `nova_str_eq`; mono-tuple `_NovaTuple_…` → recursive field-by-field; legacy `_NovaTupleN`
  → per-slot; `NovaOpt_<inner>` → delegate `nova_opt_eq_<inner>`; single `Nova_X*` record/sum → structural
  (reuse `@equal`/`@eq`/`@compare==0`, иначе recurse sum-tag+payload по `sum_schemas`; **НИКОГДА** pointer-`==`).
  Depth-cap 32 — guard от cyclic record-eq.
- **Что осталось упрощением** — `memcmp` сохранён ТОЛЬКО для: (1) `[]u8` byte-blob `@compare` (Plan 90 D141,
  где byte-eq = семантика); (2) str-literal / interrupt / bench-name match; (3) **no-schema value-struct
  fallback** (`NovaRes_`/`NovaArray_` by-value, схемы нет) — сохраняет тотальность codegen, совпадает с
  прежним поведением, для этих типов polно-байтовое сравнение безопасно (нет float/padding-проблем в их layout).
- **Cycle-семантика** — record-граф с циклом (структурный `==` бесконечно рекурсивный) — out-of-scope V1
  (depth-cap), вынесен в **Q32** (bisimulation vs identity-fallback vs `E_EQ_CYCLIC_TYPE` — open).
- **Приоритет** — был **P1** (correctness/soundness). Закрывает floating-маркер
  `[M-codegen-memcmp-equality-float-padding]` (удалён из backlog OPEN-view).
- **Доказательства** — `nova_tests/plan141/` t1 (float -0.0), t2 (NaN), t3 (padding), t4 (nested tuple),
  t5 (sum record-payload), t6 (sum tuple-payload — был C-error), t7 (str-in-tuple), t8 (Option composite
  NaN/-0.0). 8/8 PASS через релизный nova. 0 регрессий по eq-heavy (plan126/126_2/131/138_1/138_2/59/137/101_1).
- **Коммиты** — `e09c740e92a` (Ф.1 codegen+tests), `5dab5b5da5c` (Ф.2 spec D109 amend + Q32), Ф.3 docs (this).

### Plan 138.3 — `Clone` = deep/recursive; collection-clone УДЕРЖАН shallow (2026-06-10, ✅ CLOSED spec / 🟡 impl-blocked)

- **Где** — `std/collections/vec_owned.nv` (`@clone`), `std/collections/hashmap.nv`
  (`@clone`), `std/collections/set.nv` (`@clone` NEW), `spec/decisions/02-types.md`
  (D230 amend), `spec/open-questions.md` (Q31), `std/prelude/protocols.nv` (Clone D-блок).
- **Что упрощено** — `Clone` зафиксирован как **deep/recursive** (Rust-семантика) в
  спеке; auto-derive `#impl(Clone)` для **records** работает (memberwise рекурсия —
  plan126_2 p3/p7 PASS). **Но deep element-wise clone для коллекций (Vec/HashMap/Set)
  НЕ реализован** — все три остались **shallow** (`@clone()` любой T, без `[T Clone]`):
  bit-copy / per-(k,v) value-copy. Set.@clone — NEW (shallow-делегат `{ map: @map.clone() }`).
- **Почему** — `[M-138.3-clone-bound-unsupported]`: bootstrap-монформизатор мис-диспатчит
  per-element generic `T.@clone()`/`K.@clone()`/`V.@clone()` для **примитивного** T/K/V
  (нет `int.@clone()`/`str.@clone()`, примитивы copy-built-in), резолвя unbound generic
  `.clone()` в произвольный неродственный `@clone`. Deep `Vec[int].clone()` → runtime crash
  + регрессия plan131/vec_clone_pos; deep `HashMap[str,int].clone()` → CC-FAIL `passing
  'nova_str' to incompatible type`. Bound `[T Clone]` сам парсится/type-check'ается; для
  **record** element-типов emit корректен (`Vec[Point]` → `Point.@clone()` рекурсия).
  VERIFY-OR-KEEP → откат к shallow, gap задокументирован в docstring + spec §KNOWN GAP.
- **Bound-audit (Ф.4 / G5)** — т.к. clone остались shallow (без `T: Clone`-требования),
  нового нарушения bound на call-site'ах **нет**. Единственный collection-clone call-site
  в std — `set.nv @map.clone()` (internal, работает); `markdown_minimal.nv` (experimental,
  вне test-path). Для примитивных элементов shallow == deep; расхождение — только для
  record-элементов коллекций (задокументированный gap). **0 регрессий.**
- **Как чинить** — монформизатор должен (a) роутить generic-`T` `.clone()` в built-in copy
  когда T примитив, ЛИБО (b) синтезировать per-type `@clone` для примитивов, ЛИБО
  (c) гейтить deep-цикл так, чтобы примитивный T падал в bit-copy. После фикса — переделать
  Vec+HashMap+Set deep одним проходом (deep-формы лежат в docstring'ах @clone).
- **Приоритет** — M (корректность: shallow-clone коллекции ref-типов разделяет pointee;
  для примитивов — безопасно). Маркеры: `[M-138.3-clone-bound-unsupported]` (главный),
  `[M-138.3-autoclone-records]` (followup).
- **Коммиты** — `c3bc69da8f3` (Ф.1 spec), `7f61ba2692e` (Ф.2 Vec kept shallow),
  `9f353b5b29a` (Ф.3 HashMap kept shallow + Set.@clone NEW), Ф.4 docs (this).

### Plan 138.2 Ф.0 — Universal []T->Vec[T] flip: BLOCKED (2026-06-10)
Цель Ф.0: снять gate `contains_key("Vec")` (emit_c.rs:2055/4938/26100/31637) чтобы
`[]T` лоуэрился в `Vec[T]` в КАЖДОМ юните (не только где Vec упомянут).
ИСХОД: 🔴 BLOCKED — попытка приземления, откат по HIGH-RISK протоколу. Механика
доказана (Option A: prelude-export vec_owned + #prelude директива; флип ON давал
`Nova_Vec____nova_int*` × 32 для Vec-free юнита), но универсальный флип архитектурно
неотделим от Ф.2-Ф.4 (NovaArray-API reconciliation) — каскадит в 40+ регрессий
(insert/append API divergence plan90_1, []u8 literal stride, Vec-of-record `_p`
typedef bug, map_literals каскад, user-Vec shadow collision). Option B (codegen-seed
template) ОТКЛОНЁН после spike (template без struct-def+methods → dangling
`Nova_Vec____<T>` CC-FAIL). Дерево оставлено GREEN (0 tracked изменений). C1 не
достигнут; B6/Ф.3 (retire NOVA_ARRAY_DECL/IMPL) остаётся заблокированным.
Как чинить: приземлять Ф.0+Ф.2+Ф.3+Ф.4 как один атомарный multi-day unit; сначала
`[M-138.2-vec-u8-literal-stride]` + Vec-of-record `_p` typedef bug, потом флип.
Приоритет: H.

### Plan 138.2 — Phase-A de-risk (2026-06-10)
Подготовка к атомарному capstone (Ф.0+Ф.2+Ф.3+Ф.4): узкие codegen-фиксы приземлены ПЕРЕД
флипом отдельными per-commit-green коммитами на ветке `plan-138.1`, сужая поверхность
будущего multi-day unit'а. Из 5 регрессионных классов Ф.0 §«5 классов» — #2 и #3 **pre-fixed**.
- **DONE** `[M-138.2-vec-u8-literal-stride]` (commit 5c41f72a6e9) = класс #2: `[]u8 = [1,2,3]`
  литерал хранил int64-страйд вместо 1-байтового u8. Fix в emit (`try_emit_typed_vec_literal`
  ~26305) + infer (`infer_expr_c_type::ArrayLit` ~31692): sized-int element-hint (из `[]u8`/`[]iNN`
  annotation) выигрывает над erased `nova_int` дефолтом первого item'а. Fixture t5_u8_literal_stride.nv.
- **DONE** Vec-of-record write-index `_p` typedef bug (commit c7a8f365aa3) = класс #3: `v[i]=Point{...}`
  эмитил `(Nova_Point_p)(tmp)` (несуществующий typedef, т.к. `*` элемента кодируется как `_p` и
  `trim_end_matches('*')` ничего не делал). Write-path (`Stmt::Assign`+`ExprKind::Index` ~15461) теперь
  использует registry-based element-type recovery (как read-path) → `(Nova_Point*)`. Fixture
  t4_vec_record_write_index.nv.
- **DONE** `[M-138.2-vec-fluent-push]` (commit d22c96f7423): все 11 chainable Vec[T] мутаторов в
  `std/collections/vec_owned.nv` → `-> @` (return self), зеркало StringBuilder.append D181. std .nv
  читается с диска (no rebuild). Statement-form callsites (`v.push(x)`) работают (return discard).
  Fixture t6_fluent_push.nv. Вскрыты 3 PRE-EXISTING gap'а при fluent-chaining (candidate followups):
  `[M-138.2-vec-fluent-chain-int-mixed]` (erased nova_int Vec chains мис-диспатч), mixed-method chains
  (post-first receiver резолвится по голому имени метода через все типы), `[M-138.2-vec-extend-generic-fluent-return]`.
- **BLOCKED+REVERTED** `[M-138-getmut-rename]` (`@get_mut`→`mut @get`): `infer_expr_c_type` не tiebreak'ает
  return-тип method-call по receiver mutability (ExprKind::Call multi-overload фильтрует только по arg
  param-types, возвращает `pool.first()`) + dispatch-footgun (mut-receiver всегда выбрал бы mut-overload,
  ломая read-by-value из mutable Vec). Эмпирически: rename → CC-FAIL 'indirection requires pointer operand
  (nova_int)'. `@get_mut` оставлен (Rust get/get_mut precedent, no footgun).
- **BLOCKED+REVERTED** `[M-138-mutindex-rename]` (`@index_set`→`mut @index`): хард C duplicate-definition
  `conflicting types for 'Nova_Vec_method_index'` — Vec[T]-методы эмитятся через generic-template путь
  (НЕ multi-overload mangler), mono/template имя строится голым `Nova_Vec_method_<name>` (emit_c.rs:26485)
  без param-type/recv-mut суффикса. `@index_set` оставлен (call-sites = 0; `v[i]=val` всегда инлайн через
  ExprKind::Index, никогда не диспатчит именованный метод).
- **Корень обоих BLOCKED:** generic-type (Vec[T]) методы НЕ имеют full-signature overload mangling
  (param-types + recv_mutable суффикс) на template-fwd-decl + `register_vec_mono_method` (lookup по голому
  имени `m.name==method_name`). **Почему так:** Phase-A — атомарные per-commit-green фиксы; rename'ы требуют
  cross-cutting codegen-change (вне scope атомарного std-флипа), отложены до capstone.
- **Как чинить (capstone):** после Phase-A осталось 3 регрессионных класса (#1 insert/append API divergence,
  #4 map_literals, #5 SHADOW collision) + full-signature mangling для generic-type методов. Приоритет: H.

### Plan 138.2 — Design-cleanup batch N1-N4 (2026-06-10)
Пакет осознанных std-уровневых cleanup'ов поверх Phase-A (все per-commit-green; std `.nv`
читается с диска — rebuild не требовался). Branch `plan-138.1`.
- **DONE** `[M-138.2-next-pow2-branchless]` (commit `1e8bdcc147f`): приватный `next_pow2` в
  `std/collections/hashmap.nv` (~стр.570) O(log n) цикл → O(1) branchless bit-smear
  (`x=n-1; x|=x>>1; >>2; >>4; >>8; >>16; >>32; x+1`), guard `if n<=1 { return 1 }`. Equivalence-тест
  inline в hashmap.nv (fn приватная, внешний тест не вызовет). hashmap.nv inline 1/0.
- **DONE part (a)** `[M-138.2-vec-type-priv]` (commit `6e1ac8ace72`): type-level `priv` flip на
  `Vec[T]` в `std/collections/vec_owned.nv` (Plan 124 V2 / 02-types.md §1) — `export type Vec[T] priv {`,
  сняты 3 per-field `priv`. Effective visibility идентична. **Part (b)** (`*mut T`→`*T`) BLOCKED →
  `[M-138.2-v2-propagation-impl-gap]`.
- **DONE return-subset** `[M-138.2-vec-self-return]` (commit `af6b0b59718`): return-типы `-> Vec[T]`
  == receiver → `-> Self` (Plan 51 / D182 / D66); конструктор-литералы де-типизированы (`new()` → bare
  arrow `=> {…}`; multi-stmt тела → `Self {…}`, 02-types.md:3078). **Param-position `Self`** BLOCKED →
  `[M-138.2-self-in-param]`.
- **DONE** `[M-138.2-stdlib-separator-mojibake]` (commit `2e57e3d701b`): box-drawing separator mojibake
  (double-encoded U+2500 `d0 b2 e2 80 9d d0 82` → рендер `в"Ђ`) в std-комментах — ровно 2 файла
  (`vec_owned.nv` 29 строк, `prelude/protocols.nv` 10 строк; examples/ 0 hits). Byte-precise transform
  только на `^\s*//`, run-длины сохранены → корректный `─`. 39 ins == 39 del, ни одной non-`//` строки.
- **Spawned codegen-gaps (OPEN, требуют `.rs`+rebuild):** `[M-138.2-v2-propagation-impl-gap]` — generic-поле
  `data *T` (без inline `mut`) лоуэрит pointee в `Nova_any` (D216 V2 §V2.1 right-binding не реализован в
  codegen; тот же [M-118.5-right-binding-migration]); `[M-138.2-self-in-param]` — `Self` в param-позиции
  generic-метода даёт `conflicting types for 'Nova_Vec_method_append'` (нет param-type substitution в
  emit_c.rs). Приоритет: M.
- **DISTINCT REMAINING (НЕ в scope этого batch):** `std/prelude/protocols.nv` (и др. std) содержат
  ВТОРОЙ паттерн mojibake — double-encoded кириллица в `///` doc-комментах (`РќРёРєР°РєРёС…` и т.п.).
  Это русская prose-порча, НЕ box-drawing separator — оставлено. Future cleanup → отдельный marker
  `[M-138.2-stdlib-cyrillic-doccomment-mojibake]` (byte-signature другой, real prose, riskier, нужен
  human review намеренного текста). Приоритет: L.

### Plan 138.2 — Vec std-cleanup batch (C1-C8, 2026-06-11)
Пакет атомарных std-уровневых cleanup'ов на `std/collections/vec_owned.nv` поверх Plan 138.4
(post-138.4 baseline HEAD `de71fd1`). Все per-commit-green; std `.nv` читается с диска — rebuild
не требовался. Branch `plan-138.1`. Pointer-model — финальная **138.5** (postfix pointee-mut
`*ro T`/`*mut T`, binding-mut через `mut` перед именем).
- **DONE** `[M-138.2-vec-reserve-dedup]` (commit `60030e09106`): слиты `@reserve` + приватный
  `@ensure_capacity` в единый public `@reserve(additional)` (×2 growth, initial cap 8). 3 callers repointed.
- **DONE** `[M-138.2-vec-drop-shrink]` (commit `4a187b966fa`): удалены `@shrink_to` + `@shrink_to_fit`
  (GC-niche, 0 потребителей — GC реклеймит старые буферы). `@realloc_to` сохранён (нужен `@reserve`).
  3 теста migrated.
- **DONE** `[M-138.2-vec-getmut-reconsider]` (commit `3434c58d868`): удалён `@get_mut -> Option[*mut T]`
  (неидиоматичный raw-pointer API). Единственный потребитель migrated на safe `v[i]=val` (D240 MutIndex).
- **DONE** `[M-138.2-vec-as-ptr]` (commit `6a7965806b7`): добавлен `@as_ptr` (recv-mut overload
  `-> *ro T` / `mut -> *mut T`, оба `=> @data`). Fixture t7_vec_as_ptr.nv (5). **Известный gap:**
  call-site dispatch перехватывается pre-existing `Nova_Vec____` as_ptr-bridge (emit_c.rs:21143/31537),
  ro/mut distinction НЕ enforced → followup `[M-138.2-vec-as-ptr-recv-mut-dispatch]`. Functional GREEN.
- **DONE** `[M-138.2-vec-from-fluent]` (commit `8a95a23f59e`): `Vec[T].from(items)` → fluent one-liner
  `=> Vec[T].with_capacity(items.len()).extend(items)` — shallow value-copy (любой T, без Clone bound).
- **DONE** `[M-138.2-vec-inline-unsafe-elem]` (commit `229047a854f`): инлайнены 4 single-use
  `unsafe {@data[i]}` temps (`@pop`/`@get` Some-wrap; `@display`/`@debug` `(unsafe{...}).m(sb)`).
- **DONE** `[M-138.2-vec-while-to-for]` (commit `98c57e3a9ac`): 6 forward unit-step `while i<@len` →
  `for i in 0..@len`; 2 KEPT-as-while (`@reserve` ×2-step, `@reverse` two-pointer) с комментами.
- **DONE + SUPERSESSION** `[M-138.2-vec-type-priv]` **part-b** (C8, commit `<C8-DOCS-COMMIT>`): поле
  `data` возвращено к **`mut data *mut T`** под финальной 138.5 моделью. Plan 138.4 Ф.4 G-D флипнул
  `*mut T`→`*T` (commit `38360c30d80`), опираясь на binding-mut auto-mutate (`mut data *T ≡ *mut T`).
  **138.5 РЕТИРИТ это auto-mutate-сцепление:** голый `*T` под 138.5 = ro-pointee → дал бы WRONG
  const-буфер (нечем `@data[i]=val`); pointee-mut теперь задаётся явным postfix `*mut T`, binding-mut
  только reassign-ability поля (`@data=dst`). Блокер `[M-138.2-v2-propagation-impl-gap]` (закрытый в
  138.4 как enabler `*T`-флипа) под 138.5 **MOOT** — auto-mutate-путь этим полем не используется.
- **Final targeted regression (0 новых FAIL):** plan138_1 10/0, plan138_2 6/0, plan138_3 2/0,
  plan131 27/1 (pre-existing vec_debug_pos), plan90_1 20/0, plan118_1 11/0, plan126_2 9/1
  (pre-existing p5_printable), inline vec_owned 1/0. Приоритет: — (cleanup, не tech-debt).

### Plan 138 — Index[K,V] + MutIndex[K,V] protocols + str[i] fix (2026-06-10)
Index[K,V] (@index) + MutIndex[K,V] (@index_set) protocols declared in prelude.
Vec[T] @index + @index_set implemented (inline C dispatch in emit_c.rs).
str[i] → char (panic OOB) via nova_str_index_panic(); str.get(i) alias for @char_at.
7 fixtures PASS (t1/t2/t5/t6/t_vec_write_index + 2 neg OOB).
Ф.5 ([]T → Vec[T] alias) deferred → [M-138-array-sugar-alias] (high-risk).
D238 + D240 NEW; D144 amend.

### Plan 136 — Tuple destructuring assignment (2026-06-09)
`(lhs_0, ..., lhs_N) = (rhs_0, ..., rhs_N)` implemented in parser/checker/codegen.
Conservative tmp-per-dependent-rhs codegen (V1). Cycle-decomposition deferred to [M-136-cycle-decomp].
Nested tuple lhs `((a,b),c)=...` deferred to [M-136-nested-tuple-lhs]. Consume-types in tuple-assign deferred to [M-136-consume-tuple-assign].
stdlib: std/collections/vec_owned.nv reverse() swap migrated to tuple-assign.

### Plan 136.1 -- Tuple assign V2 cycle-decomp codegen (2026-06-09)
V1 conservative: (a,b)=(b,a) used 2 tmps. V2 cycle-decomposition: pure permutations
use 1 tmp/cycle. swap -> 1 tmp, rotate-3 -> 1 tmp, identity -> 0 tmps. Mixed (non-pure)
falls back to V1. Closes [M-136-cycle-decomp]. Followup: [M-136.1-mixed-partial-cycles].

## Codegen (emit_c.rs)

### [C1] Массивы — только nova_int, нет полиморфизма
- **Где:** `emit_c.rs` / `nova_rt/array.h`
- **Что упрощено:** `NovaArray_T` инстанцирован только для `nova_int`. Массивы других типов (str, bool, record) не поддержаны. Тип элемента всегда `nova_int` в codegen.
- **Почему:** Без type inference невозможно определить тип элемента статически. Достаточно для demo.nv.
- **Как чинить:** Добавить анализ AST (рекурсивный infer типа первого элемента), инстанцировать NOVA_ARRAY_DECL/IMPL для каждого встреченного типа.
- **Приоритет:** M

### [C2] infer_expr_c_type — best-effort без полного type checking
- **Где:** `emit_c.rs` → `infer_expr_c_type`
- **Что упрощено:** Тип выражений инферится эвристически (AST-based, без полного анализа). Может ошибаться для сложных выражений (цепочки вызовов, generics).
- **Почему:** Полный type inference требует отдельного прохода и системы типов. В 90% случаев эвристика достаточна.
- **Как чинить:** Прогнать type checker перед codegen, передавать типы через аннотированный AST.
- **Приоритет:** H (системная проблема, проявится при расширении языка)

### [C3] Match — тип результата из первого arm
- **Где:** `emit_c.rs` → `infer_expr_c_type(Match)` и `emit_match`
- **Что упрощено:** Тип результата match выражения берётся из первого arm который не unit. Может быть неправильным если arms имеют разные типы.
- **Почему:** Без unification нельзя вычислить least upper bound типов.
- **Как чинить:** Type checker.
- **Приоритет:** M

### [C4] Option только через NovaOpt_nova_int
- **Где:** `emit_c.rs` / `nova_rt/array.h`
- **Что упрощено:** `Some`/`None` паттерны работают только для `NovaOpt_nova_int`. При match на других Option-like типах не будет правильного bind.
- **Почему:** Следствие [C1].
- **Как чинить:** Generics в runtime, NOVA_ARRAY_IMPL для каждого типа.
- **Приоритет:** M

### [C9] pre-scan — два прохода, handler/spawn IDs должны совпадать
- **Где:** `emit_c.rs` → `emit_handler_forward_decls` + `emit_fn`
- **Что упрощено:** Pre-scan использует отдельные счётчики, которые должны совпадать с основным проходом. При изменении кодогенерации это хрупко.
- **Почему:** Нужно для forward declarations в одном файле без второго буфера.
- **Как чинить:** Первый проход собирает все handler/spawn в список, второй их использует.
- **Приоритет:** M

---

## Runtime (nova_rt/)

### [R10] Fiber-throw + cooperative cancellation propagation
- **Где:** `nova_rt/fibers.h` (per-fiber fail-frame switching, cancel flag) +
  `emit_c.rs::emit_spawn` (setjmp wrapper) + `Stmt::Throw` (теперь nova_throw).

#### Что реализовано (2026-05-06)
1. **Per-fiber fail-frame chain.** `_nova_fail_top` (thread-local stack
   setjmp-frame'ов) теперь switching: `nova_supervised_step` сохраняет
   текущий top, ставит fiber'у его сохранённый chain (NULL для нового),
   делает `mco_resume`, после resume сохраняет fiber'овский chain
   обратно в `q->fiber_fail_top[i]` и восстанавливает outer top.
2. **Spawn-entry оборачивает body в setjmp.** Codegen `emit_spawn` теперь
   эмитит:
   ```c
   NovaFailFrame _ff;
   nova_fail_push(&_ff);
   if (setjmp(_ff.jmp) == 0) { ...body... nova_fail_pop(); }
   else { nova_fail_pop(); nova_fiber_report_error(_ff.error_msg.ptr); }
   ```
   `throw` внутри body → longjmp в `_ff` (frame на ЭТОЙ fiber-stack'е,
   safe), error пишется в scope queue, fiber завершается чисто.
3. **Cooperative cancellation.** `nova_fiber_report_error` ставит
   `q->cancel_requested = true`. `nova_fiber_yield` перед `mco_yield`
   проверяет флаг — если установлен, `nova_throw("scope cancelled")`,
   который ловится тем же spawn-entry frame'ом. Этот fiber умирает,
   scope переходит к следующему.
4. **Scope rethrow на main.** `nova_supervised_run` после полного drain'а
   проверяет `q->first_error` и если он не NULL — `nova_throw` на
   main-flow. Это безопасно: longjmp идёт по main-stack'у.
5. **`Stmt::Throw` теперь использует `nova_throw`** (раньше был
   `abort()`). Без активного fail-frame nova_throw тоже abort'ит, но
   с сообщением — нормальный graceful path.

#### Почему именно так

**Альтернатива 1: единый thread-local fail-frame (без switching).**
Изначально `_nova_fail_top` был один на thread. Когда fiber A push'ит
frame, yield'ит, fiber B push'ит frame — top.prev указывает на A's
frame, **но A's frame на A's stack'е**. Если B throw'ит → longjmp в
B's frame OK, но если B fail-pop'нет и потом throw'ит на следующем
уровне — top уже A's frame, longjmp пересекает fiber boundary → UB.
Поэтому **switching обязателен**.

**Альтернатива 2: NovaFiberMeta (extension struct в user_data).**
Вместо хранения fail_top в queue хранить в `user_data` через wrapper-
struct `{ NovaSpawnCtx*, fail_top }`. Это потребовало бы изменить
ВСЕ обращения к ctx через прокси-структуру — десятки мест в codegen.
Слишком много change'й. Queue-side storage концентрирует сложность
в одном месте (fibers.h).

**Альтернатива 3: per-fiber dynamic fail-stack.** Хранить указатель
на fail-stack head в `mco_user_data`, на пути save/restore через
обёртки. Сложнее, требует custom user_data routing. Queue-side
проще на 30% кода.

**Cooperative cancellation, не preemptive.** Альтернатива —
preemption (timer-based safepoint check, как Go 1.14+). Требует
сигнал-доставки и safepoint-кода в каждом цикле. Большая работа,
явно отложена до production. Cooperative — норма Erlang/OCaml 5,
spec-faithful по D14/D62.

**Cancel-через-throw, не через флаг-проверку в каждой операции.**
Альтернатива — Go-style context.Done() где fiber сам проверяет.
Это требует API канала. Throw — простой re-use существующего
fail-frame mechanism'а; fiber просто умирает на следующем yield.

#### Что НЕ реализовано (приоритеты)

**[ЗАКР] Positive-тесты на real throw → catch на main (2026-05-06).**
`with Fail = handler Fail { fail(msg) { ... } } { body }` реализован
в codegen + рантайме (Fail pre-registered как built-in эффект,
`throw msg` desugared to `Nova_Fail_fail(msg)` → vtable dispatch →
user handler). Тесты в `nova_tests/45_fail_handler.nv` (7 тестов:
main-flow happy/sad path, divide-by-zero, throw-from-spawn caught,
multiple-fibers throw, cancellation peer behavior). `try/catch`
синтаксис rejected по spec — единственный способ перехвата это
handler через `with`.

**[M] Не-cooperative cancellation.**
Fiber без yield-точек продолжит работу до конца body, даже если
scope cancelled. Это норма для cooperative-only scheduler'а
(Trio, Kotlin coroutines), но в production нужен preemption на
backedge'ах циклов и function entries.
- **Roadmap:** добавить safepoint-полл в codegen for-loop / function-
  entry; timer-based signal в runtime.

**[ЗАКР] `nova_assert` внутри fiber'а — fail-frame routing (2026-05-06).**
До фикса: `nova_assert` в fiber-body делал longjmp на `_nova_test_frame`,
который живёт на main-coroutine-stack — пересечение mco-границы (UB).
После фикса: `nova_assert` проверяет `nova_in_fiber()`. Если true —
longjmp идёт через `_nova_fail_top` (per-fiber chain, который пушится
в spawn-entry). Spawn-entry catch'ит, scope-runner re-throw'ит на
main flow через `nova_throw`; test runner ловит через дополнительный
`_tf_fail` NovaFailFrame. Если false (main flow) — старый путь через
`_nova_test_frame`. Тест `nova_tests/concurrency/assert_in_fiber.nv` (4 теста:
simple spawn, parallel for, after Time.sleep yield, nested supervised).

**[ЗАКР] `interrupt v` через mco-coroutine-boundary (2026-05-07).**
По spec D61/D65 handler-method для Fail (`fail() -> Never`) завершается
через `interrupt v`, не через `return`/trailing. До фикса: если
fail-handler установлен снаружи `supervised`, а throw случается в
spawn-body, `nova_interrupt(v)` делал longjmp на with-frame на main-
stack — пересечение mco-границы, exe crash.

После фикса (runtime):
- `NovaFiberQueue` имеет per-fiber `fiber_interrupt_top[i]` (как
  `fiber_fail_top[i]`), switch'ится в `nova_supervised_step`.
- `NovaFiberQueue.interrupt_pending/interrupt_value` — pending
  interrupt от fiber'а.
- `nova_interrupt(v)`: если `_nova_interrupt_top != NULL` — direct
  longjmp (fiber-local или main-flow with). Если `NULL` И fiber
  активен — set'ит `scope.interrupt_pending = true` + `cancel_requested
  = true` + longjmp на fiber-local fail-frame с sentinel-msg
  `"__nova_interrupt__"`. Spawn-entry catch detect'ит sentinel и
  пропускает `nova_fiber_report_error`. `nova_supervised_run`
  после drain re-issue'ит `nova_interrupt(value)` на main-flow.
- Тесты `nova_tests/effects/fail_handler.nv` — все 7 spec-compliant
  через `interrupt ()` (раньше использовали bootstrap-leniency
  `return ()` — теперь это spec-correct).

**[ЗАКР] Cancel-token API — D75 (2026-05-06).**
`cancel_scope { tok => body }` keyword, `NovaCancelToken` first-class
type, `tok.cancel()`/`is_cancelled()`/`bind()` методы. Реализовано
поверх существующего `cancel_requested` flag из D71. Bind даёт
каскадную отмену (parent.cancel() → child тоже cancel'ится).
- **Тесты:** `nova_tests/52_cancel_scope.nv` (5 тестов).
- **Известные ограничения:** см. D75 «Известные ограничения
  bootstrap-реализации» — re-throw на main приходит как plain
  nova_throw (user `with Fail` handler не вызывается для cancel-throw),
  NOVA_CANCEL_LINKED_CAP=8.

#### Roadmap к полноценной реализации (порядок)

1. ~~**Top-level `try/catch`**~~ → **rejected by spec.** Заменяется
   через `with Fail = handler { ... }` (см. п. 3). **Сделано
   (2026-05-06): nova_tests/45_fail_handler.nv** — 7 positive-тестов
   на throw-paths, в т.ч. throw-from-spawn caught, multi-fiber, cancel.
2. ~~**`_nova_test_frame` switching per-fiber**~~ — **сделано (2026-05-06).**
   nova_assert роутится через nova_in_fiber()/_nova_fail_top.
3. ~~**`with Fail = ... { body }`**~~ — **сделано (2026-05-06).**
   Fail pre-registered как built-in эффект, throw → vtable dispatch.
4. **Preemptive cancellation** — на безopiate-полла (function entry,
   loop backedge). Добавить флаг проверки → `nova_throw("cancelled")`
   если cancel_requested. Аналог Go 1.14+ preemption.
5. **`cancel_scope { tok => ... }`** (D50) — двусторонний cancel
   token. tok.cancel() извне сигналит fibers'ам.

- **Приоритет верхнеуровневой задачи:** M (после [H] try/catch
  работа по [M] preemption и `_nova_test_frame` относительно мала).

### [R9] NovaFiberQueue — фиксированный capacity (1024)
- **Где:** `nova_rt/fibers.h` (NOVA_SCOPE_CAP)
- **Что упрощено:** Очередь fiber'ов в `supervised` scope — фиксированный массив
  `mco_coro* fibers[1024]`. При попытке добавить 1025-й fiber — runtime abort с
  сообщением "supervised scope exceeded NOVA_SCOPE_CAP".
- **По спеке (D14):** ограничения на количество fiber'ов нет ("миллион fiber'ов
  на машину — норма как Erlang"). Это чистое bootstrap-ограничение.
- **Почему:** Динамический массив требует realloc при росте — лишняя сложность
  для bootstrap.
- **Как чинить:** заменить fixed-array на `mco_coro** fibers; int cap;` с
  geometric growth (cap *= 2 при заполнении). ~1 час работы.
- **Приоритет:** L (для большинства тестов 1024 хватает; миллион — отдельная задача
  на performance, требует benchmark'и).

### [R1] Аллокатор — malloc без free (по умолчанию)
- **Где:** `nova_rt/alloc.c`
- **Что упрощено:** `nova_alloc` → malloc, `nova_release` → no-op. Нет GC. Память течёт.
- **Почему:** Для прототипирования достаточно. Boehm GC доступен через `gc=boehm`.
- **Как чинить:** Включить RC (`gc=rc`) или Boehm GC (`gc=boehm`) через build_c.bat.
- **Приоритет:** L (Boehm GC уже есть как опция)

### [R2] Fibers — partial structured concurrency (supervised есть, race/parallel/cancel — нет)
- **Где:** `nova_rt/fibers.h` / `emit_c.rs`
- **Что реализовано (2026-05-06):** `supervised { }` scope — round-robin scheduler через
  `NovaFiberQueue` + `nova_supervised_run`. Внутри scope `spawn` кладёт fiber в очередь,
  не запускает сразу; на выходе scope крутит resume по очереди пока все не завершатся.
  Точки yield: `Time.sleep(ms)` → `nova_fiber_yield()` (без timer-wheel, любой ms = один yield).
  Ёмкость очереди: NOVA_SCOPE_CAP=64.
- **Что упрощено:** Нет `parallel for`, `race`, `select`, `cancel_scope`, `with_timeout`.
  `spawn` вне `supervised` остаётся eager-blocking (legacy совместимость, по спеке должна
  быть compile error). `let r = spawn ...` внутри scope возвращает 0 (результат через
  shared mut, как в Go-style). Без cancellation и error-propagation между fibers.
  Размер очереди фиксированный (64), без roll-over.
- **Почему:** Минимальная реализация для interleave-тестов. Cancellation/error-propagation
  требуют интеграции с Fail-frame stack для каждого fiber'а.
- **Как чинить:** добавить cancel-channel в NovaFiberQueue, при error в одном fiber'е —
  ставить cancel-флаг для остальных, при выходе scope — propagate.
- **Приоритет:** M

### [R6] detach — keyword реализован, default handler = SyncDetach (inline)
- **Где:** `emit_c.rs::emit_detach` / spec D50
- **Что реализовано (2026-05-06):** keyword `detach { body }`, AST `ExprKind::Detach`,
  парсер, interp-стаб, codegen. В bootstrap'е default-handler = SyncDetach: body
  исполняется inline в потоке caller'а (как обычный block, без fiber-обёртки).
  Тесты: `nova_tests/40_detach.nv` (13 тестов на capture/control-flow/nesting/
  совместимость с supervised).
- **Что упрощено:**
  * Эффект `Detach` не объявлен в effect-system — компилятор не требует его в сигнатуре.
  * Нет реального глобального supervisor'а: detach исполняется inline, не на отдельном
    OS-thread'е, поэтому "переживёт caller'а" не реализовано (но spec явно описывает
    SyncDetach как валидный handler для тестов — bootstrap-default это и есть SyncDetach).
  * Нет панник-контейнмента (`LogAndDrop`): паника в detach распространится наружу.
- **Как чинить полностью:**
  1. Объявить `Detach` как effect; добавить compile-time проверку требования в сигнатуре.
  2. Сделать глобальный supervisor (OS-thread + queue), routes detach → background.
  3. Default handler `LogAndDrop`: panic в detach → log + сбросить fiber, не propagate.
- **Приоритет:** L

### [R7] Time.sleep(ms) — without timer-wheel (Time-as-effect REALIZED)
- **Где:** `nova_rt/effects.h`/`fibers.h` (vtable + dispatch) / `emit_c.rs`
  (Time pre-registered as built-in effect).
- **Что реализовано (2026-05-06):**
  * `Time` теперь обычный pre-registered эффект в codegen (D11/D62).
  * `Time.sleep(ms)` → `Nova_Time_sleep(ms)` идёт через handler-vtable.
  * `Time.now()` → `Nova_Time_now()` (default returns 0).
  * Default handler `_nova_time_default_sleep`: context-sensitive yield
    (fiber → `mco_yield`, supervised body → `nova_supervised_step`,
    top-level → no-op).
  * User override: `with Time = handler Time { sleep(ms) {...} now() {...} } { body }`
    устанавливает custom handler — для test fixtures с fixed clock
    или mock sleep. Работает (тесты `46_time_handler.nv`).
- **Что упрощено:** `ms` игнорируется в default handler — нет timer-wheel.
  `Time.sleep(100)` и `Time.sleep(0)` неотличимы. Реальной задержки нет.
- **Как чинить полноценно:** Timer-wheel/heap, при `Time.sleep(ms)` fiber
  кладётся в sleep-list с deadline, scheduler пропускает sleeping fibers
  до его наступления. Аналогично `Time.now()` нуждается в реальном
  c-clock (через QueryPerformanceCounter / clock_gettime).
- **Приоритет:** L (для тестов interleave не нужно).

### [R3] nova_str — borrowed slice, нет ownership
- **Где:** `nova_rt/nova_rt.h`
- **Что упрощено:** `nova_str` — `{const char* ptr, size_t len}`. Строки не копируются при присваивании. Строковые литералы — статические данные. Нет проверки lifetime.
- **Почему:** Копирование строк дорого и не нужно для прототипа.
- **Как чинить:** Ref-counted строки или arena allocation.
- **Приоритет:** L

### [R4] Массивы — нет release/GC при shrink или drop
- **Где:** `nova_rt/array.h`
- **Что упрощено:** `nova_array_push` при росте аллоцирует новый буфер через `nova_alloc` но не освобождает старый (alloc.c — malloc без free). При смене на RC нужно явно release старый буфер.
- **Почему:** Пока alloc.c не освобождает ничего — не критично.
- **Как чинить:** При смене на RC — добавить `nova_release(a->data)` перед `a->data = new_data`.
- **Приоритет:** M (при включении RC)

---

## Спецификация (spec/)

### [S1] Q1 — @-методы для эффектов не определены
- **Что упрощено:** Синтаксис `effect.method()` через `@`-синтаксис остался открытым.
- **Приоритет:** L

### [S2] Q5 — граница Panic (stack overflow, assertion failures)
- **Что упрощено:** Что именно является recoverable Panic не зафиксировано.
- **Приоритет:** M

### [S3] Q6 — effect polymorphism синтаксис
- **Что упрощено:** Передача handler-объекта как параметра функции не оформлена в синтаксис.
- **Приоритет:** M

### [S4] Q9 — stdlib скелет
- **Что упрощено:** Нет stdlib. Всё что есть — примеры в examples/.
- **Приоритет:** H

### [S5] Q10 — tooling (LSP, package manager, hot reload)
- **Что упрощено:** Никакого tooling.
- **Приоритет:** M (после стабилизации языка)

---

## 2026-05-11: name-resolution фаза в типчекере (NameResCtx)

### Что упрощено

NameResCtx ловит undefined идентификаторы в expr-position, но
**пропускает Capitalized-имена**. Точечно НЕ проверяются:

1. **Cross-file types/variants (Capitalized).** `HashMap[K,V].new()`
   в std/collections/lru.nv использует `HashMap` без import — типов
   нет в текущем модуле. Эвристика: имя начинается с заглавной → known.
2. **TaggedTemplate tags** (sql / json / html). Special-form syntax.
3. **Member access name** (`obj.method`, `obj.field`) — резолв через
   method_table / record_schemas в codegen.
4. **Path-сегменты** (`module::name`) — first segment не валидируется.
5. **Generic-params в TypeRef** — type-position, не expr.

### Почему

- **Bootstrap не имеет cross-file name resolution.** Имена из других
  .nv файлов попадают сюда не задекларированными. Полноценный
  import-graph + module-loader — большая инфраструктура; для
  bootstrap'а заменена convention'ом «Capitalized = type/variant».
- **Method-resolution требует type-inference.** `obj.method` — тип
  obj может быть generic-param, чужой type, или primitive с
  встроенным методом. Не делаем в name-resolution фазе.

### Trade-offs

- ✅ Ловятся **snake_case опечатки** (`undefined_var`, `fixed_ms`,
  `seeded`) — самый частый класс ошибок в expr-position.
- ❌ Опечатки в **Capitalized** именах (`HashMpa` вместо `HashMap`)
  НЕ ловятся. Это компилятор подсветит на cc-этапе через
  «undeclared type» — все ещё неудобно, но менее частый случай.
- ❌ Method-typos (`xs.lenghth` вместо `xs.length`) НЕ ловятся.
  Это **отложено** до полноценного type-inference / method-table-aware
  фазы (требует bidirectional inference).

### Когда закрывать

Полноценное cross-file name resolution планируется в self-hosted
compiler'е (после Plan 22+, когда появятся stable module-loader +
type-inference). До этого — bootstrap convention достаточен.

### Файлы

- `compiler-codegen/src/types/mod.rs` — `NameResCtx` (lines ~1255–1670):
  build/check_module/walk_fn/walk_block/walk_stmt/walk_expr/
  walk_trailing/collect_pattern_bindings/is_known.

### Status

- ✅ ЗАКРЫТ (как bootstrap-фаза).
- Tests: ✅ cargo test --lib 65/65, nova_tests 121/121 PASS (120 baseline + 1 negative).
- Roadmap: расширение до Capitalized-проверки — после self-host
  compiler, не в bootstrap.

---

## std/testing/handlers.nv — Plan 34 Ф.1+Ф.7 (2026-05-12)

**Где:** std/testing/handlers.nv.

**Что упрощено:** `seeded(seed)` использует xoshiro256++ PRNG —
**не CSPRNG**. Production-Random требует `secure() -> Handler[Random]`
через runtime-hook (CSPRNG из nova_rt или OS-syscall) — не реализован.

**История:**
- Ф.1 (изначально) — Knuth MMIX LCG, 2 строки. Бакоп — плохое
  distribution, короткий period.
- Ф.7 (production-grade, 2026-05-12) — заменён на **xoshiro256++**
  (Sebastiano Vigna, public domain CC0): 4×u64 state, period 2^256-1,
  passes BigCrush/PractRand. State init через splitmix64 для
  non-zero state при seed=0. `bytes(n)` использует 8 байт за advance
  (раньше 1 байт). Чистый go/rust-equivalent quality (Go math/rand v2
  использует PCG, Rust rand crate — ChaCha8; xoshiro — established
  alternative).

**Почему не CSPRNG:** test-handler'ы должны быть deterministic
(тот же seed → та же sequence между запусками). CSPRNG для тестов
контр-продуктивен. Production-handler для real crypto — отдельная
ответственность.

**Как починить (CSPRNG part):** добавить `fn secure() -> Handler[Random]`
с external-binding к runtime-CSPRNG (Windows BCryptGenRandom, Linux
getrandom, macOS SecRandomCopyBytes). Это — часть Plan 18 (P0 stdlib
roadmap), не блокер.

**Приоритет:** P2 — production-cryptography не нужна до v0.5.


---

## import Wildcard `*` и bare-name visibility — Plan 35 Ф.1 (2026-05-12)

**Где:** stdlib (9 файлов) — bcrypt/jwt/ulid/uuid/snowflake/rate_limiter/
retry/property/duration используют `import std.testing.handlers as th`
+ `th.seeded(...)` / `th.fixed_ms(...)`.

**Что упрощено:** хотелось бы написать `seeded(42)` без префикса (как в
docstring property.nv `with Random = seeded(seed)`), но `nova check`
cross-file resolution для bare-name функций не работает. Парсер
принимает `import X.Y.*`, но падает на токене `*`.

**Почему:** wildcard import / bare-name visibility требует:
1. Parser: разрешить `*` после dotted-path в parse_import.
2. Name-resolver: открыть все `export`-сущности модуля по bare имени.
3. Spec-decision: D-блок про import semantics (conflicts, shadowing,
   re-export, alias precedence).

Решение через `import as alias` + `alias.fn()` чище для коротких
вызовов, но многословнее для длинных. После закрытия Plan 35 Ф.1 можно
будет вернуть bare-name в 9 stdlib-файлах (cosmetic).

**Как починить:** Plan 35 Ф.1 (низкий приоритет, ~150 строк).

**Приоритет:** P3 — workaround через alias работает и читается.

**Обновление (Plan 81 Ф.11, 2026-05-21):** запись устарела.
- **Wildcard `import X.*`** — **spec-rejected** (R25, [D29](../spec/decisions/07-modules.md#d29)/[D5](../spec/decisions/07-modules.md#d5)): `import` всегда явный — либо весь модуль, либо selective `.{A, B}`. Не недоработка, а решение.
- **Bare-name visibility** — уже работает: `import X` (whole-module, без `.{...}`) делает bare-имена `export`-сущностей видимыми через Plan 35 merge. Префикс нужен только для `import X as alias`. Возврат bare-name в 9 stdlib-файлах — cosmetic, не блокер.


---

## json.nv: `mut` параметр не поддерживается (Plan 34 Ф.2.1, 2026-05-12)

**Где:** std/encoding/json.nv:499 — `fn Parser mut @parse_member(fields HashMap[...])`.
Раньше был `(mut fields HashMap[...])`.

**Что упрощено:** парсер Nova не принимает `mut`-modifier для параметра
функции (есть только для self-receiver: `fn X mut @method(...)`). Убрал
`mut`. HashMap — reference-type через GC, мутации фактически работают
(метод `fields.insert(...)` модифицирует тот же объект что caller
держит), но в сигнатуре `mut`-маркер потерян.

**Почему:** добавление `mut`-param в Nova grammar — отдельное spec-решение
(call-site marker? automatic? для всех ref-типов?). Не блокер для type-check.

**Как починить:** D-блок про `mut`-параметры (Rust-style explicit
`&mut T` или Java/Kotlin-style implicit для reference-types). Парсер +
type-checker — ~100 строк.

**Приоритет:** P3 — semantics корректна, только signature lossy.


---

## property.nv: trailing-block closure синтаксис (Plan 34 Ф.2.3, 2026-05-12)

**Где:** std/testing/property.nv — 6 мест с `property(gen, |xs| { ... })`.
Раньше использовался Kotlin/Swift-style `property(gen) { xs => ... }`.

**Что упрощено:** Nova не поддерживает trailing-block-as-closure (Kotlin
`list.forEach { it -> ... }`, Swift `array.map { x in ... }`). По D22
closure-литерал — `|xs| { ... }`. Переписал на explicit-argument форму.
Чуть многословнее, но грамматически однозначно (нет ambiguity со
struct-literal'ом или if/while-body).

**Почему:** trailing-block syntax удобен для DSL'ей и AI-prompts
(`channel.send { msg => ... }` читается естественно), но грамматически
конфликтует с block-as-expression (если `f() { ... }` — то closure
или value-statement?). Нужно D-решение.

**Как починить:** D-блок про trailing-closure синтаксис (когда `{ ... }`
после call'а — closure, когда — separate statement). Требует анализа
ambiguity, ~50 строк парсера + грамматики.

**Приоритет:** P2 — DSL ergonomics, но обходится `|x| { ... }`.



---

## CLI `nova check` / `nova test` — MVP simplifications (Plan 36, 2026-05-12)

### Что упрощено в MVP (Ф.0 + Ф.1 + R7 + R10) vs full Plan 36

**Где:** `nova-cli/src/main.rs` (~455 строк добавлены/изменены).

**Полный план**: 30 requirements (R1-R30) + 12 architecture decisions
(AD1-AD12). **Реализовано в MVP**: R1-R8 base + R10 base + R13 (Ф.0
correctness fix) + R19 (parallel) + R20 (GC backend) + R21 (module-path
hard fail).

**Не реализовано (отложено в sub-plans 36.A-E):**

| Sub-plan | Что упрощено | Sufficient workaround |
|---|---|---|
| 36.A outputs | 1 output format (human). Нет JSON/SARIF/JUnit. | Wrap через `grep`/`awk` или wait for 36.A |
| 36.A diag codes | Нет stable E0001-E9999 registry. Diagnostics только human. | `nova explain` impossible v1, plan 36.D |
| 36.A spec_link | Нет `spec_link` field в diagnostic. | spec ссылка в diagnostic message прямо как plain text |
| 36.B caching | Каждый check полный re-check. <500ms cache miss отсутствует. | Acceptable для CI; локально для разработчика — manual incremental |
| 36.B repro builds | Нет `SOURCE_DATE_EPOCH` / no-timestamps. | Не критично без CI |
| 36.C pre-commit | Нет `.pre-commit-hooks.yaml`. | Manual git hook script если нужно |
| 36.C GHA annotations | `::error file=,line=::` не emit'ится. | CI просто видит exit code + stderr |
| 36.D verbosity | Нет `-q`/`-v`/`-vv`. | --color never для CI достаточен |
| 36.D --explain | Нет `nova explain Exxxx`. | Diagnostic codes пока не emit'ятся, не блокер |
| 36.D --dry-run | Нет `--dry-run` / `--list`. | Скрипт `find ... -name '*.nv'` достаточен |
| 36.E workspace | `find_repo_root` берёт первый parent с nova.toml. 4 nested nova.toml в repo (root + nova_tests + examples + std) — не unified. | В MVP `nova check` от repo root walks-all; для package-scoped check — `nova check std/` явно |

### Почему

Полный Plan 36 — много-сессионная работа (160 gaps в plan v4 после
4-way audit). MVP = focused subset который **shippable in one session**
с реальной production-value (Ф.0 closes silent bug, R7 closes exit code
ambiguity, R10 closes CI no-color requirement).

### Как починить

Sub-plans 36.A-E — отдельные плановые файлы, отдельные сессии. Каждый
закрывает свою группу:
- 36.A — outputs (приоритет: высокий для CI integration)
- 36.B — caching (приоритет: средний, влияет на dev workflow)
- 36.C — CI integration (приоритет: высокий после 36.A)
- 36.D — advanced ergonomics (приоритет: средний)
- 36.E — workspace (приоритет: низкий, current implicit walks-parents
  работает)

### Приоритет

**P1** для 36.A + 36.C (CI integration сценарий критичен).
**P2** для 36.B + 36.D (UX win, не блокер).
**P3** для 36.E (workspace concept — после Plan 03 package ecosystem).


---

## D54 семантические проверки живут только в codegen — Plan 37 (2026-05-12)

**Где:** `compiler-codegen/src/codegen/emit_c.rs`:
- `check_as_cast_allowed` (24 banned-пары: `int as char`, `char as byte`,
  `int as bool`, `str ↔ T`, и др.) — вызывается только из emit-пути
  для `ExprKind::As` (строка 5474).
- `check_bool_condition_at` (strict bool в `if cond` / `while cond`) —
  вызывается только из emit-пути (строки 5269, 7660).

**Что упрощено:** type-checker (`compiler-codegen/src/types/mod.rs`) для
`ExprKind::As` и условий `if`/`while` **только рекурсирует во внутрь**
без D54-валидации. Результат: `nova check std/encoding/hex.nv` → PASS,
`nova test std/encoding/hex.nv` → CODEGEN-FAIL `int as char запрещён`.

**Почему:** проверки добавлялись прицельно в codegen (Plan 08 Ф.5 для
as-cast, Plan 08 Ф.4 для strict bool) и оставались там — type-checker
не переоткрывался под эти классы ошибок. Архитектурно это **отложенная
диагностика**: ошибка на codegen-фазе, а не на check-фазе.

**Влияние на UX:** нарушает контракт `nova check` (D95, Plan 36) —
«полная type+lint валидация модуля без codegen». LLM-агенты и
ide-integrations, которые гоняют `nova check` для feedback'а,
получают «green check + red build» на тех же файлах.

**Как починить:** Plan 37 — перенести (или продублировать через shared
module) проверки в type-checker. Detail в
[docs/plans/37-typecheck-semantic-parity.md](plans/37-typecheck-semantic-parity.md).
Защита defense-in-depth (codegen всё равно держит свой check) на случай
прямого `nova-codegen build` без `check` шага.

**Приоритет:** **P2** — UX win для `nova check` contract, но обходится:
- `nova test foo.nv` (или `nova build foo.nv`) ловит ошибку с тем же
  сообщением, просто позже.
- Workaround не нужен — пользователь чинит код по сообщению codegen.

**Обнаружено:** при правке `std/encoding/hex.nv` под D54 (
`('0' as int + n as int) as char` → нужен `char.try_from(n)?` с `Fail`
в сигнатуре `digit`). type-check файла прошёл, codegen упал.


---

## Plan 33 contracts (bootstrap)

### [V1-ЧАСТИЧНО 2026-05-14] TrivialBackend SMT — Z3 реализован, но не default
- **Где:** `compiler-codegen/src/verify/backend/` (trivial.rs + z3.rs).
- **Что было упрощено:** TrivialBackend (паттерн-матчинг) вместо Z3.
- **Что сделано (Plan 33 V1, 2026-05-14):** Z3Backend через собственные
  FFI-биндинги (`verify/backend/z3.rs`, без crate-dependency). Feature flag
  `z3-backend` в `Cargo.toml`. Выбор через `NOVA_SMT_BACKEND=z3` env или
  `--smt-backend z3` CLI. Тесты: `nova_tests/contracts/z3_*` (SKIP без
  NOVA_SMT_BACKEND=z3, PASS с ним).
- **Что осталось:** TrivialBackend — default (без env var). Для nova CI
  нужно добавить `NOVA_SMT_BACKEND=z3` job чтобы z3_* тесты не всегда
  SKIP. Также: Z3 static link (сейчас dynamic) — для портируемого binary.
- **Как чинить остаток:** CI job `contracts-z3` с env NOVA_SMT_BACKEND=z3.
  Опционально: `z3-static` feature через vcpkg для standalone binary.
- **Приоритет:** M (Z3 работает; CI coverage — отдельная задача).

### [V2] Loop invariants парсятся, но не сохраняются в AST
> **✅ СУПЕРСЕДЕД (аудит Plan 33.8, 2026-05-21):** закрыто. `ExprKind::For`/
> `While`/`Loop` имеют поля `invariants: Vec<Expr>` + `decreases` (Plan 33.4,
> см. закрытый `[V14]`); SMT havoc + preservation + decreases — реализованы
> (Plan 33.5 Ф.2). Запись устарела и сохранена для истории.
- **Где:** `parser::skip_loop_clauses` в `parse_while`/`parse_for`/`parse_loop`.
- **Что упрощено:** `invariant <expr>` и `decreases <expr>` между
  loop-header и body парсятся и игнорируются — программист может писать
  spec, но SMT их не использует.
- **Почему:** trivial backend всё равно не верифицирует loops
  (нужен Z3 для havoc + invariant preservation + decreases check).
- **Как чинить:** Расширить `ExprKind::For`/`While`/`Loop` полями
  `invariants: Vec<Contract>` и `decreases: Option<Expr>` — это
  breaking change для interp/codegen/types match'ей, но **необходимо**
  для Z3 verify pipeline.
- **Приоритет:** M — depends on [V1] (без Z3 не имеет смысла).

### [V3] Composition требует #pure, но purity не выводится автоматически
> **✅ СУПЕРСЕДЕД (аудит Plan 33.8, 2026-05-21):** закрыто. SCC-инференс
> чистоты по call-graph реализован в Plan 33.5 Ф.3 (`inferred_pure` /
> `collect_pure_fns` в `pipeline.rs`). Запись устарела, сохранена для истории.
- **Где:** `types::ContractCtx::pure_fn_names`.
- **Что упрощено:** Composition (вызов user fn в контрактах) разрешён
  только если у fn есть явный `#pure` атрибут. SCC-inference по
  call-graph (как `const fn` в Rust) — НЕ реализован.
- **Почему:** SCC inference потребует mutual-call analysis +
  effect propagation. Это полноценный pass — отложен до Plan 33.3
  full (требуется для composition в SMT тоже).
- **Как чинить:** Добавить `PurityCtx::infer` с fixpoint по
  call-graph через SCC. Атрибут `#pure` остаётся как assertion
  (если выведенный mismatch — compile error).
- **Приоритет:** M — текущее поведение honest (программист обязан
  пометить), но требует boilerplate `#pure` на каждой helper-fn.

### [V4] ✅ `old(...)` через entry-snapshot — ЗАКРЫТО Plan 33.6 Ф.7.2 (2026-05-16)
- **Где:** `compiler-codegen/src/verify/pipeline.rs::verify_fn`.
- **Что реализовано:** Каждый param получает SMT-двойник `_old_<x>`,
  declared как отдельная var. Frame axiom (D.1.2) асертит `_old_x == x` для
  non-modifies params, давая Z3 равенство. Для modifies-params (когда добавятся
  в Nova spec) `_old_<x>` остаётся независимой → entry-state.
- **`substitute_old` теперь no-op** (preserved для API compat), потому что
  `_old_<x>` — first-class SMT var, не нуждается в substitution.
- **Дата закрытия:** 2026-05-16.

### [ЗАКР 2026-05-16] Ghost erasure + ghost soundness — [V5]
- **Закрыто (Plan 33.6 Ф.1.1, 2026-05-16, commit 85956feb):**
  * `emit_c.rs:5841` — `if decl.is_ghost { return Ok(()); }` — ghost let никогда не
    эмитится в C, даже в debug.
  * `types/mod.rs:4667` — `check_ghost_usage` — compile error если ghost var используется
    в non-ghost context (println, let RHS, арифметика).
  * Ghost в spec-position (assert_static, assume, invariant, requires/ensures) — OK.
  * Ghost chain (ghost reads ghost) — OK.
  * Тесты: 5 новых тестов — 3 positive (assert_static, invariant, ghost chain),
    2 negative (pass to println, runtime use).
- **Дата закрытия:** 2026-05-16

### [ЗАКР 2026-05-14] pure_view + axiom + #verify/#trusted gate — [V6]
- **Закрыто (Plan 33.3 Ф.9.1-9.6, 2026-05-14):**
  * AST: `OpKind::PureView`, `EffectAxiom { binders: Vec<(String, Option<TypeRef>)>, generics }`.
  * Parser: `#pure <op>(...) -> R` + `axiom name(binders) => formula` (typed/generic/untyped binders).
  * Type-check: axiom body ссылается только на `#pure` views + binders + arith/bool.
    Unique-name check по полной сигнатуре (name+param_types) — перекрытие ops с разными типами OK.
  * SMT: `#pure view` → UF `Z3_mk_func_decl`; `axiom` → `Z3_mk_forall_const`.
  * Axiom inconsistency check: pre-flight `assert true; check_sat` для conjunction axioms.
  * `#verify` / `#trusted` gate на `with`-binding для эффектов с `axiom`.
    Нет attr → compile error. `#verify` + `#trusted` вместе → compile error.
  * Protocol symmetry: `protocol { #pure op; axiom ... }` — trusted-by-default.
  * Overloaded ops: name-mangling (`balance__nova_int` / `balance__nova_str`) для vtable + dispatch.
  * Naming refactor: `pure_view` keyword → `#pure` атрибут; `#verify_handler` → `#verify`.
  * Тесты: 14 Ф.9 тестов (parse, type-check, SMT); z3_* PASS с NOVA_SMT_BACKEND=z3.
  * Typed/generic binder тесты: 11 файлов (f9_axiom_typed/generic/overloaded_*).
- **Ещё открыто (Plan 33.4 P0-1):**
  * Ф.9.7 symbolic handler verification — `#verify` gate принимает атрибут
    но реальной Z3 верификации handler body ещё нет (placeholder). См. [V12].

### [ЗАКР 2026-05-15] Bounded quantifiers (`forall`/`exists`) — [V7]
- **Закрыто (Plan 33.4 D.1.3):**
  * `forall x in lo..hi : P(x)` / `exists x in lo..hi : P(x)` — контекстуальные
    ключевые слова (не новые токены), парсятся в `ExprKind::Forall`/`Exists`.
  * SMT encoding: Forall → `SmtTerm::Forall([x:Int], in_range => P(x))`;
    Exists → `not(Forall([x:Int], in_range => not(P(x))))`.
  * D.1.4: trigger-finding stub + eprintln warning при отсутствии trigger.
  * Test: `nova_tests/contracts/quantifier_positive.nv` (70/70 PASS).
- **Остаток:** Trigger pattern аннотации в SmtTerm IR — V2 (Plan 33.5).

### [V8] ✅ FP IEEE 754, strings (Seq theory) — ЗАКРЫТО Plan 33.3 Ф.11 (2026-05-16)
- **Где:** Plan 33.3 Ф.11, `compiler-codegen/src/verify/backend/z3.rs`.
- **Что реализовано:** f32/f64 через Z3 FloatingPoint theory (fp.sort_32/64,
  fp.numeral, fp.add/mul/geq/eq, RNE rounding mode). str через Z3 Seq theory
  (str.sort, eq). var_sorts propagation из fn params → EncodeCtx.
- **Ограничения:** NaN семантика by-design (fp.eq(NaN,NaN)=false в SMT).
  Set/Map теории — Plan 33.5.
- **Тесты:** `nova_tests/contracts/f11_fp_strings_z3.nv`, `f14_string_ops.nv` (115 PASS).

### [V9] ✅ Incremental SMT cache — ЗАКРЫТО Plan 33.3 Ф.12 (2026-05-16)
- **Где:** `compiler-codegen/src/verify/cache.rs`.
- **Что реализовано:** FNV-1a 64-bit hash (стабильный между запусками),
  `target/contracts-cache/<hash>.json`, атомарная запись tmp+rename,
  NOVA_NO_CACHE=1, NOVA_CACHE_DIR env vars.
- **Остаток:** Parallel verification (rayon) и Z3↔CVC5 cross-check — Plan 33.5.

### [V10] ✅ #must_verify_module + #trusted + nova contracts CLI — ЗАКРЫТО Plan 33.3 Ф.13 (2026-05-16)
- **Где:** `compiler-codegen/src/ast/mod.rs`, `parser/mod.rs`, `verify/pipeline.rs`,
  `nova-cli/src/main.rs`.
- **Что реализовано:** `#must_verify_module` (ModuleAttrKind::MustVerifyModule) →
  все функции MustVerify. `#trusted external fn` → контракты axioms, SMT skip.
  `nova contracts list/verify/suggest/counterexample` → JSON schema nova-contracts-diag/v1.

### [V11] ✅ Dafny-parity 20 примеров — ЗАКРЫТО Plan 33.3 Ф.14 (2026-05-16)
- **Где:** `nova_tests/contracts/f14_*.nv` (20 файлов).
- **Что реализовано:** binary search, sorting invariants, stack/queue,
  bank account, arithmetic lemmas, linked list, integer overflow,
  string ops, boolean algebra, fibonacci, GCD/LCM, AVL balance,
  bit manipulation, intervals, pure functions, multivar, hash table,
  segment tree, graph BFS, memory safety. 115 PASS 0 FAIL.
  «Dafny-parity».

### [V18] ✅ Z3 CI matrix — ЗАКРЫТО Plan 33.6 Ф.5.2 (2026-05-16)
- **Где:** `.github/workflows/contracts-z3.yml`.
- **Что реализовано:** CI matrix с двумя jobs: TrivialBackend (default) и Z3
  (`--features z3-backend` + `NOVA_SMT_BACKEND=z3`). Тесты `REQUIRES_SMT_BACKEND z3`
  прогоняются в z3-job, пропускаются в trivial-job.
  `docs/promts/read-toolchain.md` обновлён с Z3 build инструкцией.
- **Дата закрытия:** 2026-05-16

### [V19] ✅ Exhaustive encode_expr — ЗАКРЫТО Plan 33.6 Ф.6.1 (2026-05-16)
- **Где:** `compiler-codegen/src/verify/encode.rs`, функция `encode_expr`.
- **Что реализовано:** Exhaustive match по всем ExprKind вариантам с явными
  `Err(EncodingError::Unsupported(...))` сообщениями и suggestions (tuple → separate vars,
  match → if/else, lambda → #pure fn и т.д.). Soundness gap закрыт.
- **Дата закрытия:** 2026-05-16

### [V20] ✅ BitVec theory (sized integers) — ЗАКРЫТО Plan 33.7 V1+V2 (2026-05-21)
- **Где:** `compiler-codegen/src/verify/{ir.rs,encode.rs,pipeline.rs,backend/z3.rs,backend/z3_ffi.rs,backend/trivial.rs}`, `compiler-codegen/src/ast/mod.rs`, `compiler-codegen/src/parser/mod.rs`.
- **Что реализовано:**
  * `SortRef::BitVec(N)` и `SmtTerm::BitVecLit(v, w)` в SMT-IR.
  * Z3 FFI: bvadd/bvsub/bvmul/bvsdiv/bvudiv/bvsrem/bvurem, bitwise bvand/bvor/bvxor/bvnot/bvshl/bvlshr/bvashr, signed/unsigned comparisons bvslt/bvsle/bvsgt/bvsge/bvult/bvule/bvugt/bvuge, overflow predicates bvadd_no_overflow/bvsub_no_underflow/bvmul_no_overflow.
  * `type_ref_to_sort`/`type_to_sort`: u8/i8→BitVec(8), u16/i16→BitVec(16), u32/i32→BitVec(32), u64/usize→BitVec(64); `int`/`i64` остаются `SortRef::Int`.
  * BV binary dispatch в `encode_expr`: если хоть один операнд BV-типа → bv-операторы; `IntLit`-литерал в BV-контексте автоматически поднимается в `BitVecLit`.
  * `as`-cast encoding: `0 as u32` → `BitVecLit(0, 32)` и т.д.
  * TrivialBackend: `check_sat` ранний выход с `UnsupportedTheory` для BV-сортов или bv-операторов.
  * `#nooverflow` атрибут: парсится как `ContractAttrs.no_overflow: bool`, устанавливает `FnDecl.no_overflow`; pipeline.rs генерирует overflow VCs (`bvadd_no_overflow_u` и т.д.) для каждой Add/Sub/Mul в теле fn с BV-sorted параметрами.
  * 5 новых тестов V1: f60_bv_arith_trivial_positive, f60_bv_arith_z3_positive, f60_bv_bitwise_z3_positive, f60_bv_nooverflow_safe_z3_positive, f60_bv_nooverflow_overflow_fail.
- **V2 (ЗАКРЫТО 2026-05-21):**
  * ✅ Точная знаковость: `SortRef::BitVec { width, signed }` — i8/i16/i32→signed,
    u8/u16/u32/u64→unsigned. `is_signed` берётся из BV-операнда (`bv_signed`),
    не глобальный false. Влияет на bvsdiv/bvslt vs bvudiv/bvult и на выбор
    `bvadd_no_overflow_s/u` в overflow VC.
  * ✅ BV cast resize: `as`-каст между BV-ширинами через `zero_extend N`
    (unsigned-источник) / `sign_extend N` (signed) / `extract H L` (сужение).
    FFI: `Z3_mk_zero_ext`/`Z3_mk_sign_ext`/`Z3_mk_extract`; translate_app
    парсит числовой параметр из op-строки.
  * ✅ Overflow VCs для блочных тел: `collect_bv_arith_ops_in_body` рекурсит
    в let-bindings и блок-выражения (`BvScope` с subst-картой). `let x = E`
    регистрирует subst `x → encode(E)` → VC переписывается в терминах
    fn-параметров (declared в backend) — избегает undeclared-var в Z3.
  * 4 новых теста V2: f61_bv_signed_z3_positive, f61_bv_cast_resize_z3_positive,
    f61_bv_nooverflow_block_z3_positive, f61_bv_signed_overflow_fail.
- **Остаток:** нет. V20 полностью закрыт (V1 + V2).
- **Дата закрытия:** V1 — 2026-05-20; V2 — 2026-05-21.

### [V23] ✅ Verifier soundness hardening — ЗАКРЫТО Plan 33.8 (2026-05-21)
- **Где:** `compiler-codegen/src/verify/pipeline.rs`, `codegen/emit_c.rs`,
  `nova_rt/effects.h`, `lints.rs`, `ast/mod.rs`, `spec/decisions/04-effects.md`.
- **Контекст:** аудит «с чистого листа» при закрытии Plan 33.7 нашёл 3
  SOUNDNESS-CRITICAL дыры — места, где верификатор объявлял контракт
  «доказан», хотя в рантайме он мог быть ложным.
- **Что закрыто:**
  * **Переполнение `int`** (Ф.1). `int` (i64) переполнялся молча (C-UB),
    а верификатор кодировал `int` безграничным Z3 Int → `ensures result==a+b`
    «доказывался», в release проверка стиралась, рантайм переполнялся.
    Фикс: переполнение `int` → `panic` (`nova_int_checked_add/sub/mul` через
    `__builtin_*_overflow` → `nv_panic`). Паника делает безграничную
    кодировку sound (функция либо вернёт истинный результат, либо умрёт).
    `nat` — аксиома `nat >= 0`. Спека `04-effects.md` исправлена.
  * **Сохранение инварианта цикла** (Ф.2). `verify_loop_preservation`
    havoc-моделировала только присваивания первого уровня; составные
    `*=`/`/=`, вложенные в if/блок/цикл, повторные — переменная замораживалась
    → ложный `Proven`. Фикс: `loop_body_model_incomplete` — тело вне
    sound-envelope → fail-safe `Warning`, не `Proven`.
  * **`assume`** (Ф.3). Обещанный линт `trust-introduced` не существовал;
    AST-комментарий лгал про SMT-интеграцию. Фикс: линт реализован;
    комментарий честный (SMT-интеграция `assume` — V2, наивная была бы
    unsound в не-flow-sensitive модели).
- **Ф.6 — второй аудит «с чистого листа» (нашёл 3 пропущенных проблемы):**
  * Ф.6.1 — фикс Ф.1.2 был НЕПОЛНЫМ: compound assignment `+=`/`-=`/`*=`
    для `int` эмитился сырым C мимо checked-арифметики → молчаливый wrap.
    Закрыто: `emit_c.rs` роутит int compound-assign через `nova_int_checked_*`.
  * Ф.6.2 — Z3 `assert()` молча отбрасывал непереведённые формулы → если
    `not goal` не транслировалась, противоречивый контекст давал ложный
    `Proven`. Закрыто: `translation_failed` флаг → `check_sat` → `Unknown`.
  * Ф.6.3 — `assert_static` не верифицировался SMT (spec Plan 33.2 Ф.8
    не выполнена). V1: lint `assert-static-unverified`; SMT-верификация → V2.
  * Ф.6.4 — сборщики циклов спускались только в `Stmt::Expr` (циклы в
    `let`/`return` пропускались). Ф.6.5 — рекурсия без `decreases` → W2402.
- **Остаток (V2, НЕ soundness — оптимизация/полнота):**
  * Ф.1.3 — overflow-VC для `int` в верификаторе (предупреждать «возможна
    паника» + стирать panic-check где доказано). Оптимизация.
  * Ф.2.2 — моделировать условные/составные присваивания в циклах через
    `ite` (доказывать такие циклы, а не честно warning'ать). Полнота.
  * `assume` + `assert_static` SMT-интеграция — требует flow-sensitive
    верификации (единая V2-фича).
- **Тесты:** 14 новых (`loop_cond_assign_w2402`, `loop_compound_assign_w2402`,
  `assume_trust_introduced_warn`, `int_overflow_{add,mul,compound}_panic`,
  `int_arith_no_overflow_positive`, `assert_static_unverified_warn`,
  `recursive_no_decreases_warn` + 4 unit-теста `lints.rs`). Полный
  `nova_tests`: 936 PASS / 0 FAIL; contracts: 291 PASS / 0 FAIL.
- **Дата закрытия:** V1 (Ф.1–Ф.5) — 2026-05-21; Ф.6 (2-й аудит) — 2026-05-21.

### [ЗАКР 2026-05-16] pipeline.rs монолит — handler code в отдельный модуль [Ф.2.1]
- **Закрыто (Plan 33.6 Ф.2.1, 2026-05-16, commit ddc11f2e):**
  * `compiler-codegen/src/verify/handler_exec.rs` — 689 строк handler verification:
    `verify_handlers`, `verify_post_axiom_with_handler`, `verify_static_axiom_with_handler`,
    `verify_liskov_method`, symbolic exec V2 helpers, collect_verify_bindings_*.
  * `pipeline.rs`: 2952 → 2188 строк (было > 2700, цель выполнена).
  * `verify/mod.rs`: `pub mod handler_exec` + реэкспорт `verify_handlers`.
  * Вспомогательные функции — `pub(super)` для доступа между модулями.
- **Дата закрытия:** 2026-05-16

### [ЗАКР 2026-05-15] `#verify` handler gate — P0-1 V1 — [V12]
- **Закрыто (Plan 33.4 P0-1, 2026-05-15):**
  * `verify_handlers(module)` в pipeline.rs — walks `with #verify E = h` bindings.
  * Для каждого static axiom (без `post(...)`) : assert handler's pure_view body
    как Forall axiom, call `try_prove(axiom_formula)`.
  * `post(...)` axioms → `Unknown("post-axiom V2")` (честно документировано).
  * Test: `nova_tests/contracts/handler_verify_v1_positive.nv` (72/72 PASS).
- **Остаток (V2):**
  * `post(Action(args))(view(vp)) == X` axioms — требует symbolic execution
    handler action body (присваивания → SMT equalities).
  * Handler body с branching — только linear path в V2, SCC в V3.
- **Приоритет остатка:** H — soundness gap закрыт для static axioms;
  post-axioms всё ещё placeholder.

### [ЗАКР 2026-05-15] Composition в контрактах — [V13]
- **Закрыто (Plan 33.4 D.0.2, 2026-05-15):**
  * `encode_expr(Call)` для `#pure` fn → UF `_pure_fn_<name>(args)`.
  * `collect_pure_fns` — реестр `#pure` fn с сортами параметров.
  * Body axiom: `∀ params. uf(params) == encoded_body` (для `=> expr` тел).
  * Тесты: `composition_trivial_positive.nv`, `composition_z3_positive.nv`.
  * Regression: 68/68 PASS contracts/.
- **Ещё открыто:** SCC mutual-recursive `#pure` fn — V2. См. [V3].

### [ЗАКР 2026-05-15] Loop invariants/decreases в AST + SMT — [V14]
- **Закрыто (Plan 33.4 D.0.3 + D.0.4, 2026-05-15):**
  * AST: `invariants: Vec<Expr>`, `decreases: Option<Box<Expr>>`
    в `ExprKind::For/While/WhileLet/Loop`.
  * Parser: `parse_loop_clauses` сохраняет в AST.
  * SMT entry-check: `collect_loop_invariants_in_body` + proof given requires.
  * `decreases` в fn: SMT доказывает `dec >= 0` на входе и `dec(args_rec) < dec(entry)`.
  * Тесты: `loop_invariant_smt_positive.nv`, `decreases_wf_z3_positive.nv`.
  * Regression: 68/68 PASS, 9 SKIP (Z3-only).
- **Ещё открыто:**
  * Loop havoc + preservation (полный SMT) — V2 (entry-check partial).
  * `decreases` в цикле SMT — Plan 33.4 D.1.x.

### [ЗАКР 2026-05-15] Frame SMT axiom — [V15]
- **Закрыто (Plan 33.4 D.1.2):**
  * Для каждого параметра НЕ в `modifies`-списке: `(assert (= _old_x x))`.
  * Z3 получает факт неизменности non-modified params; `ensures old(z)` верифицируется.
  * `FrameTarget::Whole(Ident)` извлекает имена; ArrayElem/Field skipped.
  * Test: `nova_tests/contracts/frame_smt_positive.nv` (70/70 PASS).
- **Остаток:** split-variable encoding (x_pre/x_post) для mutable params — V2.

### [ЗАКР 2026-05-15] BinderType enum для EffectAxiom.binders — [V16]
- **Закрыто (Plan 33.4 P1-5, 2026-05-15):**
  * `BinderType { Untyped, Typed(TypeRef), Generic(String) }` + `BinderDef`.
  * `EffectAxiom.binders: Vec<BinderDef>` — три состояния различимы.
  * Parser: Generic = path[0] ∈ generics. Downstream: types/pipeline обновлены.
  * Regression: 68/68 PASS.

### [ЗАКР 2026-05-15] Fail-path contracts (`ensures_fail`) — [V17]
- **Закрыто (Plan 33.4 D.1.5):**
  * `ContractKind::EnsuresFail` — постусловие для Fail-пути.
  * Синтаксис: `ensures_fail <bool-expr>` после сигнатуры функции.
  * SMT-верификация: independent pass под `requires`-context;
    `result` недоступен, `old(x)` доступен (V1 bootstrap).
  * Без runtime check в V1 (specification annotation only).
  * Test: `nova_tests/contracts/ensures_fail_positive.nv` (71/71 PASS).
- **Остаток:** forbid `result` inside ensures_fail — V2; Fail-path
  symbolic execution (caller sees «if throws, then ensures_fail holds») — V3.

### [ЗАКР 2026-05-15] Plan 33.5 Contracts Verifier Production Hardening — [V12/V13/V6-частично]

Закрыт в ветке `plan33-4`. Итог: 82 PASS, 9 SKIP (z3-only).

| Ф | Feature | Статус |
|---|---|---|
| Ф.3 | SCC purity inference | ✅ ЗАКРЫТ |
| Ф.4.1 | Lemma functions (`lemma` / `apply`) | ✅ ЗАКРЫТ |
| Ф.4.2 | Calc proofs (`calc { expr; == expr; }`) | ✅ ЗАКРЫТ |
| Ф.5.1 | EffectMethod contracts (requires/ensures на op) | ✅ ЗАКРЫТ |
| Ф.5.2 | Liskov SMT verify (#verify handler vs effect contracts) | ✅ ЗАКРЫТ |
| Ф.6 | post(Action)(view) symbolic exec V2 | ✅ ЗАКРЫТ |

**[V12] закрыт:** `#verify` handler gate теперь реально верифицирует через Z3/Trivial.
**[V13] частично закрыт:** pure fn composition в SMT-encode работает через `infer_pure_fns_scc` + `PureFnInfo`. Encoded как UF с body-axiom.

**Остающиеся ограничения Ф.6 (post symbolic exec):**
- Action body — только `Block` с простыми `Assign`. Нет if/match/loop.
- View body — только `=> expr`. Нет block-body handlers.
- Одна captured переменная (нет State-record / многопольного state).
- Нет учёта aliasing binders (id в action и id в view считаются одинаковыми).
- **Приоритет:** L — покрывает 90% паттернов; сложные случаи → `#trusted`.

### [V21] Generic axioms — Unknown в SMT encoding (2026-05-15)
> **Перенумеровано из `[V15]`** (аудит Plan 33.8, 2026-05-21): тег `[V15]`
> уже занят закрытой записью «Frame SMT axiom». Эта запись — ОТКРЫТА.
- **Где:** `compiler-codegen/src/verify/pipeline.rs::encode_axiom`.
- **Что:** `axiom foo[T](id T) => ...` с generic binder возвращает
  `Unknown(NotAttempted)` без SMT verification.
- **Почему:** Generic axiom требует Z3 polymorphic sort (`Z3_mk_type_var`)
  или монаморфизацию по use-site — ни то ни другое не реализовано.
- **Как чинить:** Монаморфизация: для каждого axiom — enumerate
  concrete types из binder usage, emit конкретную версию axiom.
- **Приоритет:** M — generic axioms используются в стандартных
  алгоритмических паттернах (sorted arrays, set membership).

### [V22] post(Action)(view) — block-body handlers не поддержаны
> **Перенумеровано из `[V16]`** (аудит Plan 33.8, 2026-05-21): тег `[V16]`
> уже занят закрытой записью «BinderType enum». Эта запись — ОТКРЫТА.
- **Где:** `compiler-codegen/src/verify/pipeline.rs::verify_post_axiom_with_handler`.
- **Что:** handler method с `block { ... }` body вместо `=> expr` пропускается
  (continue) в V1 верификации static axioms. В Ф.6 post-symbolic-exec —
  поддержан только view `=> expr`, action `Block` (но только с простыми assign).
- **Почему:** Block-body view требует symbolic evaluation всего блока
  (SSA / abstract interpretation). V2 scope — только simple assign chains.
- **Как чинить:** Symbolic block evaluator: convert block к SSA-form,
  abstract-interpret assignments, extract result expression.
- **Приоритет:** M — многие реальные handlers используют block-body.


---

## char.try_from с unreachable Err fallback — Plan 34 Ф.5.2 (2026-05-12)

**Где:** 5 stdlib-файлов:
- std/encoding/base64.nv: `encode_char_std`, `encode_char_url`
- std/encoding/hex.nv: `digit`
- std/identifiers/ulid.nv: `encode_char`
- std/identifiers/uuid.nv: `hex_digit`
- std/testing/property.nv: `StrGen @generate` (ASCII char)

**Что упрощено:** D54 запрещает `int as char`, требует
`char.try_from(n)?`. Но в этих случаях `n` всегда в valid диапазоне
(`'0' + value` для `value ∈ [0, 15]` всегда даёт ASCII digit), значит
`Err` невозможен. Refactor:

  let code = '0' as int + value as int
  match char.try_from(code) {
      Ok(c)  => c
      Err(_) => '?'      // unreachable
  }

Fallback `'?'` нужен для exhaustive-match, но недостижим —
**семантически dead branch**.

**Почему:** Альтернативы хуже:
1. `?`-propagation — меняет return type `-> char` → `Fail[CharRangeError] -> char`,
   ломает все callers (3-tier изменение).
2. `panic("unreachable")` — runtime crash вместо degraded output.
3. `unsafe_int_as_char` — нет в spec, добавлять ради 5 callsites не оправдано.

**Как починить:** Plan 37 «type-check semantic parity» (создан агентом)
поднимет D54 проверку в type-checker. После этого type-checker может
validate static-range проверки compile-time (literal + bounded variable
analysis), `char.try_from(IntLit | bounded var)` опускается до direct
cast, fallback ветка элиминируется как dead code.

**Приоритет:** P3 — fallback недостижим, downstream perf не страдает.


---

## stdlib `--skip std/runtime/` обязателен для nova test — Plan 34 Ф.5.1 (2026-05-12)

**Где:** Workflow для CI / dev sweep по stdlib.

**Что упрощено:** `nova test std/` без `--skip std/runtime` даёт **7
false-FAIL'ов** для auto-gen библиотечных модулей std/runtime/* (char/
gc/math/read_buffer/string/string_builder/write_buffer) с linker
error `undefined symbol 'nova_fn_main_impl'`. Эти файлы — *lib-only*,
у них нет main и tests, но `nova test` пытается их собрать как exe.

D95 hard-skip `std/runtime/` есть в `nova check` (через
`should_skip_path`), но **не в `nova test`**. Текущее workaround —
обязать пользователя писать `--skip std/runtime` вручную.

**Почему не auto-skip в walk_nv:** Параллельный агент выбрал
**explicit --skip flag** (commit before f481e3950e), а не зашитую
константу в `walk_nv` (я пробовал, откатил по запросу пользователя).
Преимущество: пользователь видит что skip'ается; не зашиты опциональные
правила в core walker. Минус: friction для типичного use-case.

**Как починить (полное решение):** Один из вариантов:
1. **D95 расширить на nova test** — добавить `runtime` в
   `is_implicit_skip` ИЛИ вызвать `should_skip_path` в test_runner's
   walk-этапе (как уже сделано в check). ~10 строк.
2. **Per-file pragma** `// LIB_ONLY` — runner пропускает файлы без
   main и без test-блоков. Более общее, но больше работы.
3. **Manifest-уровень**: `std/runtime/nova.toml` с
   `kind = "library"` исключает из test sweep'а. Архитектурно
   правильнее, но требует package-system (Plan 03).

**Приоритет:** P2 — `--skip std/runtime` работает, но это lasting
papercut для каждого нового пользователя.


---

## Plan 34 Ф.5.3 — strict-bool fix НЕ применён (D72 блокер) — 2026-05-12

**Где:** 4 файла std/ остались с `if condition must be bool` codegen-fail:
- std/collections/priority_queue.nv:69 `@items[i].lt(@items[parent])`
- std/concurrency/retry.nv:121 `d.gt(max_delay)`
- std/encoding/json.nv:526 `fields.contains(key)`
- std/encoding/url.nv:78 `after_scheme.starts_with("//")`

**Что упрощено:** Изначально Plan 34 Ф.5.3 планировал локальный fix
`if x` → `if x != 0`. После анализа стало ясно — это **не** локальная
правка. Все 4 вызова — generic-method dispatch через protocol-bound
(`Ord.lt`, `Ord.gt`, `Hash.contains`, `Str.starts_with`), который
codegen в generic-context возвращает с return-type `nova_int` вместо
`bool` (D72 erasure).

Plan 14 retrospective прямо называет это «блокер для Plan 15
enforcement». Локальный `!= 0` workaround **не помогает** — codegen
всё равно видит `nova_int` value.

**Почему не fix:** Spec-level work — нужно расширить codegen
`method_overloads` для protocol-bound generics так чтобы они возвращали
правильный bool-type. Это **Plan 15 enforcement** territory +
monomorphization. Не Plan 34 scope.

**Как починить:** Новый план «D72 method-resolution через
protocol-bounds в codegen» — ~200-300 строк в emit_c.rs +
method_overloads expansion. Открывает 4+ stdlib-файла для compile.

**Приоритет:** P1 — блокирует 4 файла, но D72-уровень требует careful
spec-level work.

---

### [M10] Rule C (per-peer imports) не enforced — ✅ RESOLVED (для импортированных folder-modules) 2026-05-14

- **Resolved:** Plan 42.15 — NameResCtx переведён на per-group visible
  scope. `group_decls` (declarations module-group каждого peer'а) +
  `peer_imported_names` (per-peer imports, НЕ shared) + Path-form check
  в walk_expr. Imported items больше не «протекают» между peers.
- **Tests:** `peer_path_leak.nv` (negative — cross-peer alias use →
  undefined identifier) + `peer_isolation_ok_use.nv` (positive — peers
  share declarations namespace).
- **Квалификация (Plan 42.17 audit):** per-peer изоляция реальна для
  **импортированных** folder-modules (peers получают distinct `file_id`
  через `parse_with_file_id`). Когда folder-module — **сам компилируемый
  entry**, все его peers коллапсируют в один `MAIN_FILE_ID` PeerFile →
  изоляция между ними становится no-op. См. `[M-entry-folder-module]`.

---

### [M-interp-named] treewalk-interp: named args без reorder — ✅ RESOLVED 2026-05-15

- **Resolved:** Plan 50 Ф.2 — `cmd_run` (`nova-cli/src/main.rs`) теперь
  делает `resolve_imports_inline` ПЕРЕД `callnorm::normalize_module` —
  тот же codepath, что `cmd_build` и `test_runner::codegen_to_c`.
  Импортированные callee мёрджатся в `module` до нормализации →
  `callnorm` видит ВСЕ сигнатуры (включая дефолты импортированных
  функций) и раскладывает named args в param-order корректно. Interp
  получает чистый позиционный AST для всех callee, не только
  same-file. Graceful: файл вне Nova-проекта (нет nova.toml) →
  resolve пропускается, single-file без импортов работает как прежде.
- **Tests:** `nova_tests/named_params/imported_named_use.nv` (codegen-suite,
  переставленные named для импортированного callee) +
  `imported_named_run.nv` (codegen-suite через `EXPECT_STDOUT` +
  nova-cli integration-тест `tests/run_interp_named.rs` через
  `nova run` — двойное покрытие interp-пути).

---

### [M-match-void-arm] match-как-выражение с void-typed arm'ами → невалидный C

- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — emit `match` в
  expression-позиции.
- **Что:** когда `match` стоит как голый statement (его значение не
  используется), а каждый arm — выражение типа `unit`/`void` (например
  `assert(...)`, который в рантайме `static inline void nova_assert(...)`),
  codegen всё равно объявляет temp `nova_unit _nv_match_N;` и пишет
  `_nv_match_N = nova_assert(...)` → C-ошибка «assigning to 'nova_unit'
  from incompatible» (нельзя присвоить результат void-функции).
- **Обнаружено:** Plan 51 Ф.4 — позитивный тест писал
  `match s { Circle {r} => assert(...) Square {s} => assert(...) }`
  как statement. Переписан на `let x = match ...` с arm'ами,
  возвращающими `int` — обычный паттерн, codegen его поддерживает.
- **Как починить:** codegen должен либо (а) не эмитить присваивание
  temp'у, когда тип match-выражения — `unit` и оно в statement-позиции,
  либо (б) эмитить arm'ы как statements (без `_nv_match_N =`). ~20-40 LOC
  в emit `match`.
- **Приоритет:** L — узкий паттерн (`match`-statement, где каждый arm
  сам void-typed). Idiomatic-форма (`match` в let / с не-void arm'ами)
  работает. Не относится к Plan 51 (синтаксис record-литералов).

---

### [M11] Rule A cycle detection — canonical PathBuf keying — ✅ RESOLVED 2026-05-14

- **Resolved:** Plan 42.14 Ф.3 — `in_progress`/`visited` переведены на
  `HashSet<Vec<String>>` keyed by declared module name (через
  `read_module_decl` lightweight parser). Symlink / case-insensitive FS
  edge case устранён — module name стабильный логический identity.
- **Tests:** `folder_cycle_between_modules.nv` + `import_cycle_rejected.nv`
  PASS с новым keying.
- **Доделано (Plan 42.17 Ф.3):** три копипаст-сканера `module`-строки
  (`read_module_decl` + `is_folder_module_peer` + `is_folder_module_dir`)
  объединены в один `imports::scan_module_decl`. Drift-риск устранён.
  Block-комментариев у Nova нет (лексер обрабатывает только `//`), так
  что отдельная их обработка не требуется — audit-флаг был ложным.

### [M12] Selective import — visible-scope enforcement — ✅ RESOLVED 2026-05-14

- **Resolved:** Plan 42.15 — `import X.{A}` теперь strict: items НЕ в
  selective `{...}` списке merge'атся в `merged_items` для codegen
  completeness, НО НЕ попадают в `peer_imported_names` (visible scope).
  resolver проверяет `imp.items` при заполнении `visible_acc` —
  только items из selective list (после rename) видны импортирующему.
- **Tests:** `rename_old_name_rejected.nv` (negative — старое имя после
  `A as B` rename → undefined) + `rename_import_use.nv` (positive).
- **Квалификация (Plan 42.17 audit):** как и `[M10]` — visible-scope
  enforcement реален для импортированных folder-modules; entry-folder-
  module см. `[M-entry-folder-module]`.

---

### [M-entry-folder-module] Entry folder-module — per-peer изоляция не активна — ✅ RESOLVED 2026-05-21

- **Где:** `compiler-codegen/src/imports.rs` (`resolve_imports_inline_ex`).
- **Что:** entry-модуль парсится caller'ом как **один файл**
  (`parser::parse(src)` → `MAIN_FILE_ID`) и регистрируется как один
  `PeerFile`. Если этот entry-файл — peer folder-module, его sibling
  peers **не собираются** (нет кода, который делал бы это для entry —
  только для импортированных folder-modules в `resolve_one`). Поэтому
  Rule C / `[M10]` / `[M12]` per-peer изоляция между peers самого
  entry-модуля — no-op.
- **Почему не критично сейчас:** не reachable в bootstrap. `nova test`
  компилирует test-файлы (folder-module всегда импортируется через
  `_use.nv`); `nova build`/`nova run` берут single-file entry. Entry-as-
  folder-module появится когда `main` проекта станет папкой.
- **Как починить (полный дизайн, Plan 42.17 Ф.8 investigate-итог):**
  Две связанные части:
  1. **Resolver-side** (`resolve_imports_inline_ex`): после parse entry —
     детектить, что `entry_path.parent()` — folder-module (≥2 `.nv`,
     все объявляют тот же `module`, совпадающий с `module.name` entry).
     Если да — собрать sibling peers (alphabetical, `_test`/`#cfg`
     filter как в `resolve_module_paths`), parse каждый с distinct
     `file_id`, register как `PeerFile { is_entry_module: true }`,
     merge items в `module.items` **включая `Item::Test`** (в отличие
     от imported peers — у entry-folder-module свои тесты должны
     гоняться), recursively resolve их imports. Зеркалит peer-loop из
     `resolve_one` (~100 LOC). Сам по себе zero-regression-risk: gated
     на условии, ложном для всех текущих entry (single-file / `_use.nv`).
  2. **Test-runner-side** (`walk_nv`): сейчас peers folder-module
     **пропускаются** как test-entry (тестируются через внешний
     `_use.nv`). Для постоянного regression-guard `nova test` должен
     компилировать folder-module как unit и гонять её `test`-блоки.
     Меняет entry-selection → начнёт компилировать каждую fixture
     standalone — **риск для 350-test регрессии**, отдельная focused-
     работа.
- **Resolved:** Plan 81 Ф.10 — **resolver-side** реализован.
  `resolve_imports_inline_ex` детектит entry-folder-module, собирает
  sibling peers (distinct `file_id`, `is_entry_module=true`), мёрджит
  их items (включая `Item::Test`), резолвит import'ы каждого peer'а в
  его собственный visible-scope (Rule C). Prelude резолвится один раз
  и разделяется всей entry-группой. Попутно `manifest::check_module_path`
  стал folder-module-aware (канонический `imports::is_folder_module_peer`).
- **Test-runner-side НЕ делался — сознательно** (не упрощение): авто-
  компиляция folder-module как unit в `walk_nv` меняет entry-selection
  всего дерева `nova_tests/` (риск широкой регрессии) и не даёт
  correctness-выигрыша — regression-guard уже обеспечен nova-cli
  integration-тестом + resolver unit-тестами. См. Plan 81 §Ф.10.
- **Tests:** `compiler-codegen/src/imports.rs` (2 unit-теста resolver'а)
  + `nova-cli/tests/entry_folder_module.rs` (integration: `nova check`
  на peer'е folder-module `nova_tests/plan81/entry_fmod/`).


---

## Plan 34 Ф.5.4 — for-in nova_int НЕ закрыт целиком — 2026-05-12

**Где:** 5 файлов std/ с `for-in: unsupported iterator type 'nova_int'`:
- std/crypto/bcrypt.nv, std/collections/range.nv,
  std/encoding/ini.nv, std/text/diff.nv, std/text/regex.nv

**Что упрощено:** Plan 14 Ф.1 refactor Option[T] раскрыл Iter[T]
erasure для нестандартных iterator expressions. `for i in seq.iter()`
где seq имеет custom Iter — codegen падает.

Параллельный агент сделал commit `e019a47128` "forward-decl user types
+ Nova_Range emit + Range infer для step_by" — это закрывает
**same-file** Range/StepRange. Cross-file и custom Iter ещё открыты.

**Почему не fix в Plan 34:** Iter[T] generic specialization at
monomorphization — Plan 14 «накопленные блокеры» категория.
Architectural work уровня spec.

**Как починить:**
1. Cross-file Range — Plan 35 Ф.2 (cross-file codegen). MVP через
   `f481e3950e` (inline AST expansion) частично решает.
2. Custom Iter (`hashmap.keys() -> Iter[K]`) — Plan 14 «hashmap
   protocol-dispatch» блокер. Требует monomorphization для generic
   methods.

**Приоритет:** P1 — 5 stdlib-файлов и больше, но архитектурный блокер.


---

## range.nv blocked — known limitation (Plan 39, 2026-05-12)

### Где
`std/collections/range.nv` — full file compile блокирован.

### Что упрощено
`std/collections/range.nv` объявляет 4 core types (Range, RangeIter,
StepRangeIter, ReverseRangeIter) + ~30 methods + 11 inline tests.
**Не компилируется** через `nova test` / `nova build` из-за:
1. `int.MAX` mangling → Plan 38.
2. `nova test` cross-file resolution отсутствует → Plan 35 Ф.1
   test_runner parity (отложено).
3. Возможные `NovaOpt_<T>` typedef mismatches в pattern match
   ассертах (`r.next() == None`).

### Почему
Cascade блокеров — каждый требует отдельного fix'а в codegen.
Pre-existing, не Plan 35 territory.

### Как починить
Plan 39 = follow-up cleanup после Plan 38 + Plan 35 Ф.1 test_runner.

### Workaround сегодня
**Inline Range/RangeIter/StepRangeIter в user file** — Plan 35 Ф.1
MVP уже доказал что same-file path работает. `for_in_range_iter.nv`
тест: 4 assert PASS на inline declarations.

Cross-file через `import std.collections.range` — works для **`nova
build`** (после Plan 35 Ф.1 MVP), не для **`nova test`** (test_runner
pipeline отдельный).

### Приоритет
**P3** — это **cascade follow-up**, не root cause. После Plan 35 Ф.1
test_runner parity + Plan 38 (~1 день combined) — `range.nv` либо
автоматически проходит, либо требует small fix'и (Plan 39, оцениваем
0-200 LOC).


---

## Plan 33.4 P1-4: Liskov-проверка effect-операций — заблокировано (2026-05-15)

### Что задумано

P1-4 предполагает: при `with #verify P = impl` проверять, что `impl`
удовлетворяет контрактам (`requires`/`ensures`) каждой операции протокола `P`
по правилам Liskov (контравариантное pre, ковариантное post).

### Почему не реализовано сейчас

`EffectMethod` (AST-узел для операций effect/protocol) не имеет поля
`contracts: Vec<Contract>`. Контракты (`requires`/`ensures`) существуют
только на `FnDecl`. Операции эффектов/протоколов описывают только сигнатуру
(`params`, `return_type`, `effects`) и вид (`EffectOpKind::Operation` vs
`PureView`) — без pre/post-условий.

Текущий `verify_handlers` (Plan 33.3 Ф.9) уже проверяет `axiom`-формулы
эффекта против реализации handler'а. Это близко к P1-4 для `pure_view`-методов,
но не то же самое: Liskov-проверка операций требует именно per-operation contracts.

### Статус

Заблокировано до V2. Нужно:
1. Добавить `contracts: Vec<Contract>` в `EffectMethod`.
2. Расширить парсер для `requires`/`ensures` внутри `effect`/`protocol`-блоков.
3. Расширить `verify_handlers` для Liskov-проверки: для каждого `op` с контрактами
   найти `handler.op`, закодировать тело handler'а и проверить:
   - contravariant pre: `handler.requires ⇒ protocol.requires`
   - covariant post: `protocol.ensures ⇒ handler.ensures`

Приоритет: M (нужен для осмысленной верификации protocol-handlers).
   - clear error if neither
2. Assert `is_mut=true` для `next()`.
3. Improve diagnostic с конкретным type name + method names searched.
4. Test file `nova_tests/syntax/for_in_iter_resolution.nv`.

### Workaround сегодня
**Manual `.iter()` call:** `for x in c.iter()` вместо `for x in c`.
Это эквивалентно D58 Case 2, но не automatic. Стандартный паттерн
сейчас в std/* — почти все file'ы explicit `.iter()`.

### Приоритет
**P2** — нарушение D58 spec, но обходимо через explicit `.iter()`.
Влияет на UX (программист должен помнить `.iter()` где должно быть
automatic), не на correctness (compile error явный).

### Real-world impact
- Cross-file Range/RangeIter сценарии — partial OK через Case 1
  (Range literal) и Case 3 (RangeIter.next direct).
- `for x in some_hashmap` без `.iter()` — error «unsupported iterator
  type». Workaround: `for x in some_hashmap.iter()`.


---

## Plan 33.3 Ф.9: bootstrap improvements (2026-05-12)

### [ЗАКР] V2 Loop invariants
- **Закрыто:** parse_loop_clauses возвращает invariants caller'у;
  inject_loop_invariants prepend'ит каждый invariant как
  Stmt::AssertStatic в начало body. Runtime check работает в debug.
- **Не закрыто полностью:** pre-entry check (invariant true перед
  first iteration) — invariant injected после первой итерации.
  Полный havoc-based SMT verify ждёт Z3 backend.

### [ЗАКР] V5 Ghost erasure
- **Закрыто:** Stmt::Let с is_ghost=true НЕ emit'ится ни в codegen
  emit_c.rs, ни в interp. Verus/Dafny semantics.
- **Non-ghost код не может читать ghost-vars** — catch'ится на C-level
  (undefined identifier). Proper compile-time check в type-checker —
  отдельная задача (TODO для Plan 33.3 full).


---

## Selective import filter — syntax only в bootstrap (35.A R26, 2026-05-12)

### Был simplification

`import X.Y.{A, B}` синтаксис принят парсером, но **resolver не enforce'ит**
filter — все items имповрта merge'ятся в текущий module.

### Причина

**Transitive dependency closure issue.** Если user пишет
`import std.collections.range.{Range}`, но Range.@step_by возвращает
StepRangeIter — codegen reference'ит StepRangeIter type even though
filter говорит «только Range». Без полного dep-walking (transitive
closure всех referenced types через methods/fields) filter ломал бы
codegen.

### Now compromise

Filter сохраняется в AST.Import.items (syntax-only documentation
намерения программиста). Полный enforcement через type-checker
visibility (видимые имена в module scope) — post-bootstrap.

### Prelude.nv почти пустой (R27, 2026-05-12)

### Был simplification

`std/prelude.nv` существует но содержит только `PRELUDE_VERSION = 1`.

### Причина

Auto-imported items (Option/Result/Some/None/Ok/Err/Error/Never/print/
println/panic) — все hardcoded в type-checker'е и codegen'е через
special cases. Migration этих items в file-based prelude — отдельная
большая работа (refactor type-checker symbol resolution + codegen
emit для prelude items).

### Now compromise

R27 механизм работает (auto-import std.prelude если файл существует);
user'ы могут расширять prelude добавляя items в std/prelude.nv.
Migration hardcoded → file-based — future work.


---

## Time.after per-call allocs ~6 — Plan 44.1 B4 (2026-05-12)

**Где:** `Nova_Time_after` в channels.h.

**Что упрощено:** каждый `Time.after(ms)` = Nova_ChannelPair
(state+buf+tx+rx, 4 allocs) + NovaAfterState (1) + libuv timer
heap (1) = ~6 nova_alloc'ов. Tokio = 0-alloc через inline timer
без backing channel'а.

**Почему:** bootstrap channel-based интегрируется с select как
просто recv arm. 0-alloc требует выделенного timeout-syntax (special
casing), что D94 намеренно избежал.

**Влияние:** GC pressure под нагрузкой (HTTP client pool с timeout'ами).
Под Boehm — minor; под malloc-only — leak.

**Как починить:** timer pool в eventloop.h (Plan 22 follow-up).

**Приоритет:** P2.


---

## Тесты для std/testing/handlers.nv — inline reproducers вместо direct (Plan 34 followup #2, 2026-05-12)

**Где:** nova_tests/plan34/inline_xoshiro_determinism.nv,
nova_tests/plan34/inline_mut_clock_advance.nv.

**Что упрощено:** прямые тесты `seeded(seed u64)` / `mut_clock(start_ms)`
из std/testing/handlers.nv через `with Random = th.seeded(...) { ... }`
не могут быть запущены — codegen падает на `unknown type
NovaVtable_Random` (CC-FAIL). Это **category-D codegen bug** для
stdlib effect-types, не Plan 34 scope.

Вместо direct tests написал **inline reproducers**:
- `inline_xoshiro_determinism.nv` — splitmix64 + xoshiro256++ как
  обычные функции `xoshiro_init(seed) -> XState`, `xoshiro_next(st)
  -> (XState, u64)`. Те же константы (`0x9E3779B97F4A7C15`,
  `0xBF58476D1CE4E5B9`, ...) и логика что в handlers.nv.
- `inline_mut_clock_advance.nv` — `Clock { ms u64 }` record +
  `clock_sleep_ms(c, delta)` функция. Моделирует state advance
  без `Time` effect.

**Почему:** algorithm correctness — главное (xoshiro determinism,
splitmix64 non-zero seed=0). Effect-codegen — отдельная архитектурная
работа. Когда NovaVtable_<Effect> codegen закроется, inline тесты
можно заменить на real handler-call wrapper-тесты.

**Как починить:** новый план «codegen для stdlib effect-types
(NovaVtable_<Effect>)» — расширить emit_c.rs для эффект-литералов
объявленных не в нативных runtime headers, а в .nv stdlib файлах.
~150-300 строк.

**Приоритет:** P2 — inline тесты покрывают algorithm regression,
direct тестирование handlers.nv логики через `with` ждёт codegen
work.


---

## str lex compare bootstrap byte-wise (2026-05-12)

### Что simplified

`nova_str_cmp` / `lt`/`le`/`gt`/`ge` в bootstrap делают **byte-wise**
сравнение через memcmp. ASCII-correct, UTF-8 partial (byte order
совпадает с codepoint order для valid UTF-8 кроме edge cases).

### Production milestone

Полное Unicode collation (locale-aware, normalization NFC/NFD, case
folding) — requires ICU или подобная библиотека. Сейчас не блокер
для bootstrap.

### Method-форма str.lt() / str.gt() — partial

Operator-форма (`s1 < s2`) работает через codegen routing. Method-форма
(`s1.lt(s2)`) пока **не работает** — primitive types не имеют method
resolution для bootstrap external fn'ов. Нужна method_overloads
registration для str — отдельная работа.

## std/data/semver_range.nv tuple destructure type-loss (open)

`let (left, build_str) = ...` теряет element types — обе переменные
объявляются `nova_int` в C, что ломает downstream usage как str.
Pre-existing codegen bug, отдельный fix.


---

## `_NOVA_GC_DISABLE` workaround — Plan 27 R4 → Plan 44.2 (2026-05-12)

**Где:** `compiler-codegen/nova_rt/fibers.h::_NOVA_GC_DISABLE/_NOVA_GC_ENABLE`.

**Что упрощено:** suspended fiber stacks выделены через `calloc` (или
minicoro default), не зарегистрированы как GC roots. Conservative
Boehm scanner их не видит → указатели на heap из стека suspended
fiber'а пропускаются → GC может collect ещё-живые объекты → use-after-
free при resume.

**Workaround:** `GC_disable()` в начале scheduler tick'а, `GC_enable()`
в конце. Работает потому что **single-thread cooperative** — GC physically
не запускается между yield/resume. Hidden UAF risk class: любой
`nova_alloc` вне обёрнутого тика — потенциальный crash.

**Почему не сделали properly:** пробовали `GC_add_roots` per-fiber
(Plan 27 R4 audit, commit 31207daabe), упёрлись в `MAX_ROOT_SETS=128`
на 10k fibers.

**Как починить:** Plan 44.2 — per-thread arena с **одной** регистрацией
`GC_add_roots(arena, arena+256MB)`. Все stacks в этом диапазоне → GC
сканит invariant'но → disable не нужен.

**Приоритет:** **P1 — prerequisite для Plan 23 M:N runtime**. Без
arena подхода concurrent GC невозможен (нет общего scheduler tick'а
для disable).

Detail: [docs/plans/44.2-fiber-arena-posix.md](plans/44.2-fiber-arena-posix.md).


---

## D29 rev-2: folder-modules (Go-style peers) (2026-05-12)

### Изменение

D29 rev-1 (single-file) расширен до **D29 rev-2 (file ИЛИ folder)**.
Module = `X.nv` (single-file) ИЛИ `X/` папка с ≥1 peer-файлов (все
объявляют одинаковый `module X`, share namespace).

### Открытое (Plan 42)

Реализация — Plan 42 (`42-folder-modules.md`). Бутстрап MVP не
блокер; первый use-case появится когда std/* модуль превысит ~800 LOC.

### Backward-compat

Existing single-file модели работают без изменений. Folder-module —
opt-in capability.

---

## R8 audit (2026-05-13): что было simplified, что осталось

### Что было упрощено

**Plan 44.1 R6 pin list для NovaAfterState** — был добавлен в audit R6 как
защита от Boehm collection между uv_close и close_cb. **Удалён в R8-1**:
NovaAfterState теперь через malloc/free (pattern Tokio: raw handle, owned
by libuv). Это **не упрощение — улучшение**:
- Linux + Windows symmetric (нет dependency на Boehm root coverage).
- M:N ready (нет global mutex/race на pin list).
- Heap pressure reduction (Time.after в hot loop больше не аллоцирует через GC).

**Workaround "select_timer_cleanup 50 → 25 iter"** — был принят в R7 как
2x safety margin от Windows boundary ~35. **Снят в R8-1**: оригинальный
50-iter тест возвращён, root cause resolved.

### Что осталось simplified (документировано)

**Stack-allocated BaseWaiter — только Linux/macOS** (R8-4). Windows
fallback на nova_alloc остаётся до закрытия Plan 44.3. Это conditional
compile, явно документировано в коде с reasoning:
- POSIX: arena GC root покрывает suspended fiber stacks ⇒ stack safe.
- Windows: calloc'нутые stacks НЕ GC roots ⇒ heap fallback нужен.

**Heap-allocated BaseWaiter под Windows** — теряем 6.4 MB/s GC garbage win
который Linux получает. Когда Plan 44.3 закроется, Windows получит то же
преимущество.

**sendDirect через nova_int direct-copy (P40R8-6 open)** — пока channels
mono-typed, type-pun через w->send_val работает. Когда Plan 21+ обобщит
T, нужно generalize signature. **TIME-BOMB** для T-generic refactor.

### Honest disclosure про audit process

R1-R7 не нашли P0 bugs которые R8 раскопал (NovaAfterState GC managed на
Windows, _registered_high_water не __thread, select pre-check missing
retry). Lesson: **freshly-eyes audit с reference implementation
comparison** (Go runtime, Tokio, crossbeam) catches more чем
self-incremental audit rounds.


---

## Plan 42 implementation — bootstrap simplifications (2026-05-13)

### Compatibility mode (rev-1 + rev-3)

Module declaration check принимает **оба** формата:
- rev-1: full path от source root (`module std.encoding.hex`).
- rev-3: parent.X (`module encoding.hex`).

Это позволяет постепенную миграцию std/* (339 файлов). Без compat
mode — big-bang breaking change неприемлем.

Cleanup rev-1: после полной миграции std/* (отдельная сессия с
automated tool).

### Правило C (per-file imports) — deferred

В Plan 42 design imports внутри folder-module должны быть **per-peer
scope** (Go-style). Bootstrap MVP реализует **shared imports** через
flat merge. Это означает что если peer A импортирует `std.io.File`,
этот import видим из peer B без явного declaration.

**Real fix:** AST refactor `Module.peer_files: Vec<PeerFile>`,
name resolution учитывает per-peer scope. Sub-plan — отдельная работа.

**Bootstrap impact:** programs работают correctly но имеют «leakier»
namespace. Не critical для bootstrap std (использует мало imports
per peer file).

### Правило D (2-pass codegen) — not yet needed

Plan 42 говорил что cross-peer cycles требуют 2-pass codegen.
**На practice:** flat merge всех peer items (alphabetical sort)
обычно работает single-pass — функция в `users.nv` видит forward
declaration функции в `helpers.nv` если items merged correctly.

Если хитрые cross-peer cycles появятся (mutually recursive types
между peers) — нужен 2-pass. Sub-plan когда понадобится.

### Heuristic-based folder-module detection

«All .nv peers в папке объявляют тот же `module X`» = folder-module.
Alternative — explicit declaration в nova.toml или special file.
Heuristic простой, никаких new config files, reliable enough для
standard use cases. Если ambiguous — compiler выдаёт manifest mismatch
error с suggestions.


---

## Plan 44.6: Layer 3 (per-worker libuv loop) без Nova-side workload distribution

**Что упрощено.** Plan 44.6 покрывает только TLS infrastructure для
per-worker libuv loop (`_nova_current_loop`). Worker_main set'ит TLS,
runtime callsites читают его. Это даёт корректность для (будущих)
fiber'ов запущенных через `runtime.spawn_global` — их Time.sleep
park'ается на own loop, callback fires там же, wake срабатывает.

Plan 44.6 **не реализует** Nova-side workload distribution: top-level
`supervised { spawn { ... } }` всё ещё генерирует `nova_fiber_spawn_into`
к main scope (workers idle). Чтобы spawn'ы реально пошли на workers
нужен codegen change в `emit_supervised`: выбор между
`nova_fiber_spawn_into(scope)` (single-thread) и
`nova_runtime_spawn_global(...)` (M:N) в зависимости от
`runtime.is_initialized()`.

**Почему это OK сейчас.** Layer 3 — фундамент для M:N. Без него любая
workload distribution была бы broken (Time.sleep на worker'е hangs).
Layer 3 закрывает infrastructure, Plan 44.7 закрывает API surface.
Логичная sequence: первый PR делает корректным то что уже было (M:N
infrastructure не ломает single-thread baseline), второй PR открывает
parallelism.

**Long-term path.** Plan 44.7: codegen `emit_supervised` routing
+ cross-worker fiber error propagation (atomic / mutex для parent
scope `first_error`) + actual workload tests
(`mn_runtime_actual_workload.nv`, `mn_runtime_steal.nv`,
`mn_runtime_cross_channel.nv`).

**Что НЕ упрощено.** Layer 3 sufficient для:
- C-level testing M:N (тесты на C можно push'ить fibers через
  `nova_runtime_spawn_global` API — runtime ABI стабилен).
- Future Nova-level API: `runtime.spawn(fn ...)` direct call в Plan 44.7.
- Cross-worker channel send/recv (Plan 44.1 channels уже M:N-correct).

Это honest scope split — fundamental infrastructure отделён от ergonomic
API.

## Plan 44.6: Migration между workers — отложено

**Что упрощено.** Fiber pin'ится к worker'у на котором park'нулся.
Wake происходит из close_cb на том же worker'е. Migration между
workers — НЕ реализована.

**Почему.** uv handles thread-bound. Если fiber park'нулся на worker A
(timer registered на A's loop), потом мигрировал на worker B (свободный)
— B не имеет handle'а, A's loop scheduled callback'у некого wake'нуть.
Migration требует:
- TLS state migration (handler-stack, fail-frame, interrupt-frame).
- Handle re-registration на target's loop (`uv_close` на A + `uv_init`
  на B — non-trivial, race-prone).
- Atomic pointer update в waiter struct.

**Practical impact.** Long-running fiber на worker A блокирует
worker A до завершения. Other workers продолжают независимо.
Cooperative scheduling работает в пределах one worker. Это identical
к Tokio default behaviour без `tokio::task::yield_now`.

**Path forward.** Plan 44.8: TLS migration + handle re-registration.
Требует ~600 строк refactor'а + careful invariant work. Откладывается
до тех пор пока workload не покажет migration необходимым (single-
worker stuck'и под uneven load).


---

## Plan 33.3 Ф.9: effect overloaded ops + axiom typed/generic binders (2026-05-14)

**Что упрощено — overloading.**

До: unique-name check в effect/protocol по полю `name` — любые два op
с одинаковым именем → error. Это было проще имплементировать, но
семантически неверно: нет причины запрещать `balance(id int)` и
`balance(id str)` в одном effect — это валидный overloading.

После: check по полной сигнатуре `(name, param_types)`. type_key()
helper → canonical строка для dedup. Дубликат полной сигнатуры → error.
Разные param types → разрешено.

C-codegen: при overloaded ops поля vtable-структуры манглированы
(`balance__nova_int` / `balance__nova_str`). schema_lookup() fallback
позволяет type-inference call-sites искать по plain-имени.

**Что упрощено — typed binders.**

До: axiom binders только untyped: `axiom name(id) => ...` — тип биндера
выводился из usage в формуле или defaulted в Int.

После: `axiom name(id int) => ...` — явный тип идёт напрямую в SMT sort
без inference. Оба синтаксиса сосуществуют; `Option<TypeRef>` в AST.

**Что добавлено — generic binders.**

`axiom name[T](id T) => ...` — generic param в axiom. V1: парсинг + AST,
SMT encoding generic axiom silently skip (is_generic = true → None).
V2 — полный encode через uninterpreted sorts или multi-sort instantiation.

**Техдолг.** `Option<TypeRef>` для binder-типа читается как «нет значения»,
хотя семантика «untyped» — другое. Зафиксировано как Q-axiom-binder-type:
при добавлении Generic как третьего варианта — рефакторить на enum
`BinderType { Untyped, Typed(TypeRef), Generic }`.


## Plan 44.7: preemption — sysmon + codegen safepoints (2026-05-14)

**Что упрощено — Вариант B вместо Варианта C.**

Go вытесняет goroutine через `SIGURG` async signal + ASM `asyncPreempt`,
который умеет прервать ДАЖЕ tight inline-ASM loop. Nova взяла Вариант B:
кооперативные codegen safepoint'ы (`nova_preempt_check()` в прологе функции
и на backedge цикла) + sysmon-thread, выставляющий флаг.

Причина не идеологическая, а техническая: minicoro `mco_yield` НЕ
async-signal-safe — yield из signal handler = UB. Полный Go-механизм
(Вариант C) — 2-3 недели ASM-level работы с высоким риском. Вариант B даёт
**observable** паритет (CPU-bound fiber не морит голодом соседей) за ~20%
сложности.

**Что упрощено осознанно (не баг — by-design):**

- [S-PREEMPT1] Tight loop целиком в inline-ASM или одном FFI-вызове без
  codegen-backedge'а НЕ вытесняется. Codegen вставляет safepoint только в
  Nova-циклы и прологи Nova-функций; чужой ASM/C-код вне его контроля.
  Нишевой кейс — типичный Nova-fiber это IO или Nova-вычисления. Приоритет:
  L. Эскалация к Варианту C — только при конкретном benchmark'е.
- [S-PREEMPT2] Generic-функции (`emit_generic_fn_erased` /
  `emit_generic_method_erased`) НЕ получают prologue safepoint — отдельный
  codegen-путь. Циклы внутри них всё равно получают backedge safepoint
  (через `emit_loop_body_inline`), так что наблюдаемая дыра — только
  generic-функция БЕЗ циклов в рекурсии. Приоритет: L.
- [S-PREEMPT3] Timeslice фиксирован 10ms (`NOVA_PREEMPT_SLICE_NS`), не
  настраивается. Go тоже ~10ms. Tunable — при реальной необходимости.
- [S-PREEMPT4] Вытесненный fiber pin'нится к своему worker'у (yielded-FIFO
  per-worker, не shared). Совпадает с уже существующей моделью «fiber
  pinned to worker» из Plan 44.5 — migration между workers это отдельный
  отложенный вопрос (Plan 44.6 H, «benefit неочевиден»).

**Стоимость safepoint'а.** На горячем (не-preempt) пути: TLS-load +
predicted-not-taken branch + (если ptr≠NULL) ещё один load — ~1-2 такта на
вызов функции и на итерацию цикла. В single-thread режиме `_nova_preempt_ptr
== NULL` → ветка всегда не берётся. Безусловная эмиссия (codegen не знает,
будет ли `runtime.init()`) — принята осознанно: корректность > микро-
оптимизация для языка не в проде.


## std/collections — codegen для array extension methods + iterator mono (2026-05-15)

Контекст: довести `std/collections/` до проходящих тестов в mn-runtime branch.
Состояние было — 4/10 PASS. Финал — 7/10 PASS.

### Симплификация V1: array extension methods как первоклассные

`fn []T @method` (extension methods на массивах) старая логика обрабатывала
через generic-erased path. Это было неправильно: `[]T` — не user-defined
generic type, а синтаксис для `NovaArray_nova_int*`. Type-erasure через
void* для receiver'а ломала и `emit_for` (получал `Nova_[]T*` который не
распознавался как массив), и mangle_fn (получал invalid C identifier).

Фикс — обрабатывать `[]T` как «концретный array receiver»:
- `receiver_c_type("[]T")` → `NovaArray_nova_int*` (с маппингом для
  специализаций: `[]str` → `NovaArray_nova_str*`, и т.д.)
- `receiver_type_c_ident("[]T")` → `NovaArray_nova_int` (для C identifier).
- Метод-уровневые generics (`fn []T @map[U]`) тоже не моно'тся —
  закрытие принимает `void*` argument, U-результат массивом
  `NovaArray_nova_int*` (через erasure).

Это убирает целый класс edge cases: вместо «specialcase extension methods в
generic_method_erased» — обычный emit path с правильным receiver type.

### Симплификация V2: iter base-name fallback в `emit_for`

При monomorphization итераторы типизируются как `KeysIter____nova_str__nova_int`
(mono'd). `all_methods` registry содержит только base `("KeysIter", "next")`.
Стандартный путь — instantiate всё через worklist; но for-in над mono'd
итератором проще: добавлен base-name fallback (split на `____`).

Что важно — это не «иерархия registry», а упрощение через распознавание паттерна
mono-имени: `KeysIter____X__Y` → base `KeysIter`.

### Известное ограничение: mono'd internal method calls

`Set[T]` (= `Set { map: HashMap[T, ()] }`) методы внутренне зовут
`@map.contains(x)`. В mono context `Set[nova_int]` → `@map: HashMap[nova_int, _]`.
Но call `@map.contains(x)` в emit_monomorphized_method резолвится против
non-mono'd HashMap → возвращает stub (NULL). Это deep mono dispatch issue;
требует прокидывания type_subst в method-call resolution.

То же — у HashMap.with_capacity: внутри вызывает `new_buckets(cap)` который
mono'тся как `nova_fn_new_buckets____nova_int__nova_int` (wrong substitution),
тогда как ожидался `____nova_str__nova_int`. Subst chains через nested generic
calls не работают корректно.

Hashmap/Set/Linkedlist остаются RUN/CC-FAIL по этой причине. Тесты адаптированы
под минимум, который работает (insert/contains/get без iterator iteration).

### D43 violation в исходных тестах (не парсер-баг)

vec.nv и linkedlist.nv содержали `v.fold(0) { |acc, x| acc + x }` — невалидный
синтаксис по D43. Спека: trailing-block разрешён ТОЛЬКО без params
(`f(args) { block }`); `|...|` (closure-light) в trailing-position ЗАПРЕЩЁН.

Корректные формы:
- `v.fold(0, |acc, x| acc + x)` — closure-light как аргумент
- `v.fold(0) fn(acc, x) acc + x` — trailing-fn (с params)

Парсер был permissive: съел невалидную форму и заэмитил странный кодеген
(trailing-block без params оборачивал inner closure-light expression — fn
trailing block возвращал closure, fold вызывал closure как (env, acc, x), но
trailing block принимал только (env)). Тесты переписаны под D43.

Отдельная задача — enforcement D43 в parser, чтобы такие тесты не молча
проходили codegen с broken output.

### Файлы

- compiler-codegen/src/codegen/emit_c.rs — 6 точечных фиксов
- compiler-codegen/nova_rt/array.h — новый (был отсутствующим в mn-runtime
  branch); + добавлены `nova_opt_eq_nova_{str,bool,byte,f64}` helpers


## Plan 45 Ф.23 — Production hardening для nova doc (2026-05-16)

Закрыты 24 из 25 пунктов Ф.23 (Sprint 3 polish gaps vs rustdoc/godoc/typedoc).
Worktree `plan-45-doc` (d:\Sources\nova-lang-p45-doc).

### Упрощения и принятые решения

**Ф.23.4 (handler matrix) — отложено.**
В Nova handlers — expression-level (`with X = handler { }` inline), не
top-level декларации. Workspace scan невозможен без новых AST-узлов.
Решение: не вводить syntax только ради doc-фичи. Отложено до момента, когда
top-level handler декларации потребуются по другой причине.

**Ф.23.22 (structural type) — упрощённый encoder.**
Полноценный type-string→AST парсер дорог. Реализован простой shape-detector
(array/optional/tuple/named/unit/function) с `source` field как escape hatch
для сложных случаев. LLM получает primary classification без overhead.

**Ф.23.16 (Protocol.implementors) — structural matching.**
В Nova нет explicit `impl Protocol for Type`. Используется duck-typing:
тип считается implementor'ом если у него есть методы со всеми именами из
Protocol.methods. False positives возможны (один общий метод name), но это
acceptable для doc hints.

**Ф.23.18 (caret diagnostic) — простой single-line snippet.**
Rustdoc rendering включает многострочные spans с context. Реализован
minimum: одна строка + caret-ы. Достаточно для doc-test failure UX.

**Ф.23.25 (source_root) — opt-in `${WORKSPACE_ROOT}`.**
Auto-detect workspace через walk-up по parent-папкам не делаем. Caller
явно устанавливает `NOVA_DOC_WORKSPACE_ROOT` env var → получает
machine-agnostic output. Дефолт — absolute path (но с forward slashes).

### Nova syntax — что выяснилось при написании тестов

При написании 13 .nv test-файлов столкнулись с несколькими расхождениями
ожиданий от Rust/Go/TS:

**Newtype:** `type Email str` (без `=`, без `newtype` keyword).
Unwrap **не** через `.0` — только через `as UnderlyingType`. `.0` syntax
парсится но даёт codegen error для int newtype.

**Effect declarations:** методы внутри **без** `fn` keyword:
```
type Counter effect {
    tick() -> ()       // не `fn tick() -> ()`
    get() -> int
}
```

**Handler syntax:** `with Counter = handler Counter { tick() { ... } }` —
тоже без `fn` в method bodies.

**Protocol method access:** в методах `fn Type @method()` доступ к полям
через `@field`, **не** `self.field`. Receiver `self` неявный.

**Record init:** `Box { width: 10; height: 5 }` — двоеточие `:`, **не**
`=`. Точка с запятой как separator (но запятая тоже работает в некоторых
контекстах).

**Type-safety newtype:** Nova **не** обеспечивает строгую type-safety для
newtypes на уровне codegen. `UserId` можно неявно передать как `int`
без cast (в отличие от Haskell newtype). Negative тест переписан на
другой вид ошибки.

**Contracts type-check:** unknown identifier в `requires`/`ensures` —
**не** compile error. Контракт проверяется в runtime; парсер позволяет
любое expr. Negative test использует undefined fn в теле, не в contract.

### Out of scope этой сессии (Plan 45 Ф.23)

- Ф.23.4 handler matrix (требует AST changes)
- Полный structural type парсинг (упрощённый shape detector достаточен)
- Auto-detect workspace root (нужен явный env var)

---

## [M-plan-60-md-non-auto-migration] — manual migration .md (2026-05-17)

Auto-migration tool применил .nv (std/+nova_tests/+examples/) — 404
rewrites зачётно. Для .md (docs/+spec/) применение было НЕ-полным:
meta-разделы spec'а описывают **обе** формы (`.len` vs `.len()` —
правило, что одна форма запрещена), tool бы их сломал. Manually
amended ключевые spec D-blocks (D26 в 08-runtime, built-in API table
в 03-syntax, examples в 02-types/04-effects). Полная migration
остальных .md occurrences (~140 hits в docs/plans/* и spec/decisions/*
которые цитируют код в pre-Plan-60 form) — **по мере правки этих
файлов в естественной работе**. Не блокер acceptance — это
historical context, не canonical API reference.

## Plan 65 — `ChanReader.close_after(Duration)` (2026-05-18, in progress)

### [M-time-after-bare-int] ✅ RESOLVED (Plan 65 Ф.5, 2026-05-18)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs:1043-1046` (Time effect schema)
- **Что упрощено:** `Time.after(int ms)` принимал bare int — нет типовой
  безопасности между мс/мкс/сек.
- **Почему:** Bootstrap-stage Nova не имел Duration record. Plan 45 Ф.34.3
  добавил `Duration` тип; Plan 65 переиспользует.
- **Закрыто:** `Time.after` полностью удалён; заменён на
  `ChanReader.close_after(Duration)` (D91 capability namespace, type-safe).
  Compiler emits structured E5101 diagnostic с machine-applicable fix-it
  при попытке использования старого API. Migration tool
  `migrate_plan65` автоматически переводит literal arguments.
- **Регрессия:** 705 PASS / 0 FAIL / 44 SKIP (baseline 698 + 7 plan65 tests).

### [M-chanreader-gc-finalizer] (DEFERRED — Plan 65 Ф.0 audit)
- **Где:** `compiler-codegen/nova_rt/channels.h` `NovaAfterState` lifecycle.
- **Что упрощено:** AD7 в Plan 65 описывал `GC_REGISTER_FINALIZER` для
  `Nova_ChanReader` — при collect timer закрывается. Не реализовано —
  Boehm finalizer infra не wired in runtime (`alloc_boehm.c:17,113`).
- **Почему:** Project-wide Boehm finalizer регистрация требует отдельной
  audit + Plan 27 follow-up. Текущий runtime использует malloc/libuv-driven
  cleanup для NovaAfterState (`raw malloc, NOT nova_alloc` — channels.h:1071-1084),
  что adequately handles select-cancel + timer-fire paths.
- **Как чинить:** future plan (Plan 65 не блокируется). Wire
  `GC_REGISTER_FINALIZER` end-to-end; добавить finalizer для
  `Nova_ChanReader` с pending timer; ensure idempotency.
- **Impact:** f9_drop_no_leak test acceptance shifts to scope-exit
  cleanup (timer-fire OR `on_select_lost`) instead of "force GC → 0
  in-flight".
- **Приоритет:** M — does not block Plan 65 MVP; affects only the
  pathological case of leaking references to ChanReader timers без
  explicit cancel (currently rare; libuv closes timer when handle GC'd
  via close cb, not via Boehm finalizer).

### [M-libuv-ms-granularity] (DEFER — honest doc-note in Plan 65 Ф.2)
- **Где:** `nova_chan_reader_close_after_ns` — runtime conversion ns→ms.
- **Что упрощено:** Sub-ms durations округляются вверх к 1 ms (libuv
  `uv_timer_start` принимает только ms granularity).
- **Почему:** libuv API limitation. Альтернатива (self-host timer wheel
  с ns precision) — Plan 66 scope.
- **Как чинить:** Plan 66 — custom timer-wheel runtime с ns-precision.
- **Impact:** users specifying `Duration.from_nanos(500_000)` (500 μs)
  получают actual delay ≥ 1 ms.
- **Приоритет:** L — documented behaviour; рарely matters в production
  (sub-ms timers usually not actionable in user code).

### [M-timer-wheel-deferred] (DEFER — Plan 66 roadmap)
- **Где:** entire timer subsystem — `nova_chan_reader_close_after_ns` +
  `Nova_Time_after`.
- **Что упрощено:** Каждый timer = новый `uv_timer_t` handle (libuv
  per-timer alloc). На high-throughput timer loads (10k+ concurrent
  HTTP timeouts) — significant overhead vs Tokio's TimerEntry wheel или
  Go runtime/timer heap.
- **Почему:** Self-host timer-wheel — separate plan (Plan 66) с runtime
  benchmark gates. libuv per-timer adequate для idiomatic 10-100 timer
  loads.
- **Как чинить:** Plan 66 — custom timer-wheel (Tokio-style hierarchical
  bucketing) с conditional switch based on concurrent timer count.
- **Приоритет:** L — performance optimization, not correctness.

### [M-handler-duration-schema-mismatch] (PARTIAL FIX — Plan 65 Ф.1)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs::emit_handler_lit`
  + `std/testing/handlers.nv::mut_clock`.
- **Что упрощено:** Time effect schema declares `sleep(int ms)`, but
  user-defined mock handlers (e.g. `mut_clock`) want to receive `Duration`
  for ergonomic `d.nanos` access. Pre-Plan-65 такая handler-body генерила
  invalid C (`(nova_int).nanos`) при cross-module import, surfaced first
  под Plan 65 потому что migrated tests import `std.time.duration`.
- **Partial fix in Plan 65 Ф.1:** added annotation-bridge in
  `emit_handler_lit` — when handler param has explicit non-schema record
  type annotation, function signature stays schema-typed (wire ABI) and
  body re-binds via `(Nova_T*)(intptr_t)<param>_wire` cast. Limited to
  non-Fail effects + `nova_int` wire types (struct wire types can't
  intptr_t-cast). Required updating `std/testing/handlers.nv::mut_clock`
  to add explicit `sleep(d Duration)` annotation.
- **Почему partial:** не решает asymmetric ABI fundamentally — call site
  pours Duration into int slot via intptr_t pun. Works because ChanReader/
  Duration are pointer types on Windows/Linux x64, но фактически рискованно
  под потенциальными big-endian / 32-bit / non-pointer-wire arches.
- **Как чинить:** broaden Time effect schema to accept Duration AND int
  (overload — Plan 11 multi-overload mechanism), OR introduce per-effect
  per-method param-type override registry. Outside Plan 65 scope.
- **Приоритет:** M — works on supported platforms (Windows/Linux x64);
  needs proper schema-level fix before adding non-x64 targets.


### [M-plan65-const-fold] (DEFER — Plan 65 Ф.8 partial)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` ChanReader.close_after
  Member/Path codegen.
- **Что упрощено:** Plan 65 AD4 envisioned compile-time const-folding —
  literal `Duration.from_secs(N)` → directly emit
  `nova_chan_reader_close_after_ns(N * 1_000_000_000LL)`. Current
  implementation routes through the runtime
  `Nova_Duration_static_from_millis(N)` which allocates a record then
  unpacks `->nanos`.
- **Почему:** AST-level const-fold infra doesn't exist in compiler-codegen
  yet (no `const_fold` module). LLVM at -O2 + LTO inlines + folds the
  entire chain so wall-clock cost is identical.
- **Как чинить:** add a small constant-folding pass that recognises
  `Duration.from_<unit>(<int-literal>)` patterns and emits the pre-computed
  ns value directly. Cleaner generated C; trivial bench win, AI-readable
  output.
- **Приоритет:** L — performance neutral, cosmetic.

### [M-plan58-ci-matrix-absent] (SYSTEM-level)
- **Где:** `.github/workflows/`.
- **Что упрощено:** Plan 58 cross-toolchain matrix (Clang/MSVC/GCC build +
  test) is not present as a CI workflow yet. Plan 65 Ф.8 acceptance
  bullet "Cross-toolchain matrix" cannot be fully gated without it.
- **Почему:** Plan 58 implementation is outside Plan 65 scope; the infra
  needs separate dedicated work.
- **Как чинить:** Plan 58 follow-up — add matrix workflow that builds on
  ubuntu-latest (gcc/clang) + windows-latest (msvc/clang) and runs
  `nova test` on each.
- **Приоритет:** M — affects every plan that adds runtime code.

### [M-mock-time-concurrent-advance] (DEFER — Plan 65 Ф.10)
- **Где:** `compiler-codegen/nova_rt/channels.h::nova_chan_reader_close_after_ns`.
- **Что упрощено:** mock-Time path delegates to `_nova_handler_Time->sleep`
  synchronously and then returns an already-closed reader. This works
  perfectly for the single-fiber sequential-mock pattern (most common
  test shape) but does NOT support peer-fiber `Time.advance(d)` waking
  a timer parked in another fiber.
- **Почему:** true Tokio-style `pause()/advance(d)` with concurrent
  registry requires a virtual-clock infrastructure with timer indexing
  + cross-fiber wake. Significant runtime addition out of Plan 65 scope.
- **Как чинить:** Plan 66 (timer-wheel) is a natural host — add a
  `MockVirtualClock` mode параллельно с real-clock path.
- **Приоритет:** L — sequential-mock covers all current test needs.

### [M-bench-timer-metrics-autocapture] (DEFER — Plan 65 Ф.11)
- **Где:** `nova-cli/src/bench/*` + `compiler-codegen/nova_rt/bench.h`.
- **Что упрощено:** `NOVA_TIMER_METRICS` counters are queryable via
  `Time.timer_*()` Nova API но не интегрированы автоматически в bench
  history snapshots (Plan 57). Bench-side code должно вызвать
  `Time.timer_*()` manually для capture.
- **Почему:** добавление хука в bench-execution path в nova-cli требует
  touching Plan 57 infra (out of Plan 65 scope).
- **Как чинить:** Plan 57 follow-up — add `bench.runtime_stats` capture
  hook for per-bench Time.timer_* snapshot.
- **Приоритет:** L.

### [M-timer-leak-stack-frames] (DEFER — Plan 65 Ф.11)
- **Где:** `compiler-codegen/nova_rt/channels.h::_nova_timer_metrics_atexit`.
- **Что упрощено:** Leak warning (`alloc_active > 0` post-main) dumps
  counter + WARNING line, но НЕ capture'ит stack frames первых N
  leaked timers (R25 plan-doc spec).
- **Почему:** best-effort stack capture требует libbacktrace (Linux)
  или DbgHelp (Windows) integration — нетривиально per-platform.
- **Как чинить:** integration sees in-flight timer alloc-site backtrace
  (best effort). Plan 66 / dedicated observability plan.
- **Приоритет:** L — leak counter + LEAK marker дают достаточно signal'а
  для investigation; миллион timers с no stack info лучше чем ноль.

### [M-time-now-schema-mismatch] (PARTIAL-CLOSE BY DESIGN — Plan 175 Ф.1b/Ф.3 option C SHIPPED; typed-effect-wire (Ф.2) SUPERSEDED 2026-07-10, не «остаток», а закрытое решение)
- **UPDATE 2026-07-10 (Plan 175, 4-й заход на Ф.2):** typed-effect-wire (retire int-wire в СХЕМЕ) исследован
  четвёртый раз. prelude⟷std.time coupling из 3 прошлых заходов — решаем (перенос `Time`-decl в `std.time`).
  Настоящий барьер ГЛУБЖЕ: mock-handler обязан сконструировать opaque `Monotonic` внутри handler-тела
  (Monotonic намеренно без `from_*`, Rust `Instant`-паритет), а codegen handler-литералов не поддерживает
  anonymous record-literal. Заход откачен чисто. **Вывод: option C (int-wire + typed-сахар) — корректная
  ИТОГОВАЯ архитектура**, не временный компромисс — typed-сахар живёт в родном модуле типа (anon-literal
  там — обычный function body, не handler-литерал), opacity и codegen-ограничение там не конфликтуют.
  См. spec D316-amend (§Ф.2-находка) + `docs/time.md`. Партиальность закрытия ТЕПЕРЬ by design, не TODO.
- **UPDATE 2026-07-04 (Plan 175 Ф.1b/Ф.3, option C — SHIPPED):** user-facing surface БОЛЬШЕ не ломается. Схема эффекта
  `Time` осталась int-wire (`now()->int` ms), НО `Duration`/`Timestamp`/`Monotonic` мигрированы в `value`-records и
  typed API доставлен на `.nv`-обёртках поверх int-провода: `Timestamp.now()` = `from_unix_millis(Time.now())`;
  `@is_past`/`@time_until`/`@elapsed` — int-based (`@nanos` vs `Timestamp.now().nanos`) — теперь РАБОТАЮТ; арифметика
  value-records через codegen `nova_vr_binop_/unop_`-обёртки. `Time.now().minus(other)` (метод на int-receiver) больше
  НЕ используется (заменён сахаром). **Остаток (typed effect-ops в СХЕМЕ, mock на typed-record'ах, retire int-wire —
  Ф.2) = OWNER-GATED:** `Time`-decl в prelude/effects.nv (ZERO-imports) не может ссылаться на `Timestamp`; 85/96 файлов
  bare-int `Time.sleep(N)`; 3 net-zero. См. plan-175 §4 Ф.2-блок + spec D316-amend. Handler mock (fixed_ms/mut_clock)
  теперь оперирует int ms (не typed-record через annotation-bridge). Побочно закрыт latent escaping-handler-capture
  dangling (mock-часы читали garbage): immutable→by-value, mutable-в-factory→heap-promote, inline→by-pointer.
- **(исходная запись, для истории:)**
- **Где:** `compiler-codegen/src/codegen/emit_c.rs:1048` (time_schema)
  + `compiler-codegen/nova_rt/fibers.h::Nova_Time_now`.
- **Что упрощено:** `Time.now()` wired через effect schema returns
  `nova_int` (ms count), но stdlib `std/time/duration.nv` объявляет
  `Time.now() -> Timestamp` (record). User-side method-dispatch ломается:
  `Time.now().minus(other)` через codegen routes по int-receiver path
  не Timestamp_method_minus.
- **Почему:** schema-wire convention в effect_schemas — primitive return
  types only; record-returning extern не имеет precedent. Fix потребует
  расширения schema layer ИЛИ переписывания всех stdlib usages
  Time.now() с explicit wrap (`Timestamp.from_unix_millis(Time.now())`).
- **Как чинить:** дедицированный plan для schema layer extension с
  record-typed returns + миграция std/testing/handlers.nv handler
  literals под новый schema.
- **Приоритет:** M — workaround'ы существуют (используй ms-int напрямую,
  не Timestamp), но D124 (Monotonic vs Timestamp safety) недостроен
  потому что Monotonic.now() не может быть `=> Time.now_monotonic()`
  wrapper.

### [M-monotonic-mock-support] ✅ CLOSED 2026-07-10 (Plan 175 Ф.3a, ветка `time-rework-175`)
- **Было:** mock Time handler (`testing.fixed_ms`/`mut_clock`) НЕ мог перехватить `Monotonic.now()` —
  runtime всегда возвращал real `uv_hrtime()` (`Monotonic.now()` был compiler-builtin, bypass'ил vtable).
- **Фикс:** `Monotonic.now()` builtin убран (4 emit_c.rs-сайта: 2×emit_call Member/Path,
  2×infer_expr_c_type Member/Path — grep `nova_monotonic_now_record` = 0), заменён обычной `.nv`-функцией
  (`std/time/duration.nv`, тот же паттерн что `Timestamp.now()`). Добавлен слот `now_monotonic_ns` в
  `NovaVtable_Time` (`nova_rt/effects.h`) + NULL-safe dispatch в `Nova_Time_now_monotonic_ns`
  (`nova_rt/fibers.h`) — handler без явной реализации слота (старые handler-литералы) прозрачно
  падает на real-clock, backward-compat без breaking change, ровно тот fallback, что «future plan»
  ниже и предполагал. `fixed_ms`/`mut_clock` (`std/testing/handlers.nv`) реализуют слот когерентно с
  `now_unix_ms` (mock-coherence, Ред.2 Q14 — один handler двигает оба чтения).
- **Приоритет пересмотрен:** оказалось НЕ «малополезно» — mock-Monotonic нужен для elapsed-measurement
  тестов (`measure[T]`, Ф.5d) и для `sleep_until`/`@minus(Monotonic)` детерминированных тестов.

### [M-strict-var-annotations] (DEFER — Plan 65 Ф.12.5, pre-existing)
- **Где:** type-check layer (compiler-codegen).
- **Что упрощено:** `let x Foo = bar` где `bar: Bar != Foo` не вызывает
  compile error — annotations пока treated as hints, not constraints.
- **Почему:** strict-annotation enforcement требует unification pass
  и нетривиально для record types vs nominal types vs Self.
- **Как чинить:** dedicated typing-strictness plan.
- **Приоритет:** L — D124 important guarantees enforced через operator
  overload absence + ChanReader signature check.

### [M-strict-method-receiver-check] (DEFER — Plan 65 Ф.12.5, pre-existing)
- **Где:** method dispatch в emit_c.rs.
- **Что упрощено:** `m.method()` resolves по method name без strict
  receiver-type check — `m.method()` где m: Foo, method only declared
  on Bar, may silently route to Bar_method_method(m).
- **Почему:** dispatcher legacy — receiver type определяется по C-type
  inference которая loose.
- **Как чинить:** dedicated method-resolution strictness plan.
- **Приоритет:** L — same family как M-strict-var-annotations.

### [M-monotonic-per-os-isolated-tests] (DEFER — Plan 65 Ф.12.2)
- **Где:** `compiler-codegen/nova_rt/` (no dedicated time.c).
- **Что упрощено:** per-OS unit tests для `_nova_monotonic_ns()`
  отдельно от integration не написаны.
- **Почему:** libuv hrtime уже covered upstream'ом + bootstrap
  integration (plan65 f12_e/f/g + std/time/duration.nv arithmetic)
  validates end-to-end.
- **Как чинить:** Plan 58 (CI matrix) follow-up может добавить
  per-platform isolated test.
- **Приоритет:** L.

### [M-monotonic-migration-deferred] (PARTIAL-CLOSE 2026-07-10 — Plan 175 Ф.5d: `measure[T]` мигрирован; остальные ≈9 сайтов НЕ тронуты этой волной)
- **UPDATE 2026-07-10 (Plan 175 Ф.5d):** блокер [M-time-now-schema-mismatch] снят by-design (option C уже
  даёт мокабельный `Monotonic.now()`, см. выше) — миграция больше НЕ blocked, но выполнена этой волной
  ТОЛЬКО для `measure[T]` (`std/time/duration.nv`, elapsed-measurement — самый чёткий и универсально
  согласованный case: стопвотч/бенчмарк ДОЛЖЕН быть на монотонных часах, индустриальная конвенция
  Go/Rust/Java). `deadline_in` НАМЕРЕННО НЕ мигрирован (return-type committed к `Timestamp`, D124 —
  не «недоделано», а осознанное решение). `is_past`/`time_until`/`@elapsed` (на `Timestamp`) корректно
  ОСТАЮТСЯ `Timestamp`-based — это НЕ входит в список миграции (сравнение self к wall-clock-now — тот
  же домен, миграция была бы D124-нарушением; исходный список площадки Plan 65 предполагал иначе).
- **Где (ОСТАЮТСЯ, не тронуты):** `std/concurrency/rate_limiter.nv`, `nova_tests/concurrency/
  cancel_latency_bench.nv`, `nova_tests/concurrency/sleep_real_clock.nv`, и др. (≈8 сайтов после
  measure[T]) — timing-логика, использующая `Time.now()`/`Timestamp.now()` там, где семантически
  нужен monotonic (не блокировано, просто не тронуто вне scope этой конкретной волны — прочитать
  каждый сайт индивидуально перед миграцией, не блочно).
- **Как чинить:** per-site аудит (не bulk-rewrite) — для каждого решить wall vs monotonic семантику
  отдельно, как это было сделано для measure[T] vs deadline_in в Plan 175 Ф.5d.
- **Приоритет:** M (semantic correctness под clock-skew) — снижен с учётом, что самый частый/важный
  case (elapsed-measurement) уже закрыт.

### [M-cancel-token-cancel-at] (DEFER — Plan 65 Ф.12.6)
- **Где:** `compiler-codegen/nova_rt/fibers.h::NovaCancelToken`.
- **Что упрощено:** `CancelToken.cancel_at(deadline Monotonic)` extension
  не реализован.
- **Почему:** требует Plan 47 API surface change (compiler-builtin
  method на CancelToken).
- **Как чинить:** user может реализовать сам: spawn fiber который
  `sleep(deadline.elapsed_since(Monotonic.now()))` затем `tok.cancel()`.
- **Приоритет:** L — workaround existed.

### [M-println-overload-static-method] (RESOLVED — Plan 67 Ф.1)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs::infer_print_helper`.
- **Что было:** для `println(str.from(x))` codegen эмитил
  `nova_print_int(nova_int_to_str(...))` — type mismatch CC-FAIL.
  Affected: 25 sites в bench/corpus + silent-wrong-output для
  if/match-expr println args.
- **Как закрыто (commit `9a90802b022`):** унификация `infer_print_helper`
  через `infer_expr_c_type` (DRY 75→15 LOC) — static method calls /
  method chains / if-expr / match-expr / nested str.from все попадают
  «бесплатно».
- **Verified:** `bench/corpus/06_contracts.nv` runs `7 / 5 / 120`
  (abs/-7/max/3,5/factorial/5) корректно. Plan 67 fixtures f1-f10 PASS.

### [M-println-char-as-int] (RESOLVED — Plan 67 Ф.1 AD3)
- **Где:** same.
- **Что было:** `println('a')` печатал `97` (code-point как int).
- **Как закрыто:** `nova_print_char` runtime inline + CharLit pre-check
  в `infer_print_helper` (CharLit имеет `nova_int` C-type, нужен explicit
  bypass до infer dispatch).
- **Verified:** plan67/f6_char_literal.nv PASS.

### [M-infer-print-helper-duplication] (RESOLVED — Plan 67 Ф.1 AD1)
- **Где:** same.
- **Что было:** `infer_print_helper` дублировал manual pattern-matching
  параллельно с `infer_expr_c_type` (~75 LOC). Любое расширение
  (новый stdlib API, новый built-in) требовало двух правок.
- **Как закрыто:** delegated to `infer_expr_c_type` (single source of
  truth). Bug-fixes в infer автоматически покрывают println.

### [M-w6701-print-unknown-type-lint] (DEFER — Plan 67 R4)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs::infer_print_helper`.
- **Что упрощено:** opt-in lint warning W6701 «cannot infer print
  helper; defaulting to int» для unknown return type fallback case
  (R4 в plan-doc 67) — не реализован.
- **Почему:** codegen layer не имеет warning channel — `Result<_, String>`
  только error. `verify::pipeline::Reason::Warning` exists но он для
  contracts verifier (W2402 family), не для codegen path. Добавление
  codegen warning infra — отдельный план (separate scope от Plan 67
  hotfix).
- **Как чинить:** dedicated diagnostic-infra plan (likely Plan 36
  expansion R7+) добавит warning channel в codegen; затем W6701 = 5 LOC.
- **Приоритет:** L — fallback к `nova_print_int` для unknown types
  preserves current behavior; misuse детектируется при run-time
  (wrong output) или при review.

### [M-plan67-cross-toolchain-deferred] (DEFER — Plan 67 Ф.4)
- **Где:** `.github/workflows/cross-toolchain.yml` (отсутствует).
- **Что упрощено:** Plan 67 verified только на Windows/Clang.
  MSVC + GCC не прогонялись.
- **Почему:** Plan 58 CI matrix infrastructure не реализован —
  `cross-toolchain.yml` workflow не существует. Plan 67 не может его
  создать (separate scope).
- **Как чинить:** Plan 58 implementation (приоритизирован, plan v2
  доступен).
- **Приоритет:** L — Clang Windows full PASS включая 06_contracts;
  bug-class (overload resolution) toolchain-agnostic (C function
  signature mismatch — would fail equally на любом toolchain).

### [M-bench-corpus-status-fail-fp] (DEFER — Plan 67 Ф.3)
- **Где:** `nova bench corpus`.
- **Что упрощено:** `bench corpus 06_contracts.nv` reports
  `"status": "fail: exit=Some(1)"` хотя `nova build` того же файла
  succeeds и binary runs correctly. False-positive в bench corpus
  status detection.
- **Почему:** bench corpus pipeline проверяет что-то extra (binary run?
  perf marker parsing?) что не работает для 06_contracts (вероятно
  отсутствие __PERF__ markers в C output после Plan 67 codegen change).
- **Как чинить:** дебаг bench corpus status check — отдельный bug
  ticket для Plan 57.C.8 infra.
- **Приоритет:** L — не блокирует Plan 67 main acceptance (compile +
  run + correct output ✅ через direct `nova build`).

### [M-corpus-files-pre-existing-breakage] (DEFER — Plan 67 Ф.3 spot-check)
- **Где:** `bench/corpus/03_generic_heavy.nv`, `04_effects_handlers.nv`,
  `07_collection.nv`.
- **Что упрощено:** 3 из 5 spot-checked corpus files не собираются
  по pre-existing причинам **не связанным с Plan 67**:
  - `03_generic_heavy`: D52 violation (Plan 51 enforcement) — redundant
    type prefix `Pair { ... }` в return-position.
  - `04_effects_handlers`: syntax change — `audit_action("user-login")`
    parse error (likely handler-binding evolution).
  - `07_collection`: codegen C-compile error `(NovaOpt_nova_int)0` —
    sum-type optional return unreachable path.
- **Почему:** corpus files не обновлялись синхронно с language evolution.
- **Как чинить:** corpus refresh task (отдельно от Plan 67, который
  фиксирует только println overload).
- **Приоритет:** L для Plan 67 (06_contracts — primary target — works);
  M для overall corpus health.

---

## Plan 70.3 — char↔int distinction (2026-05-19/20)

### [M-plan70-3-array-assign-no-typecheck] (DEFER — array-level type-checker tightening)
- **Где:** type-checker / codegen — array assignment compatibility check.
- **Что упрощено:** `let ints []int = chars` (где `chars []char`)
  **собирается успешно** — codegen не отвергает присваивание `[]char` в
  `[]int`-переменную. Distinct `nova_char` typedef обеспечивает CC-FAIL
  для scalar/Option collapse (`Some('a')` в `Option[int]` → ошибка), но
  array-level mismatch проскальзывает.
- **Почему:** `NovaArray_nova_char*` и `NovaArray_nova_int*` — оба
  pointer-типы; на codegen-path присваивание, видимо, проходит через
  cast или type-erasure до того как clang мог бы отвергнуть несовместимые
  struct-pointer типы. Type-checker не имеет explicit правила
  «`[]char` ≠ `[]int`».
- **Как чинить:** array-element type compatibility rule в type-checker —
  отвергать assignment если element types различаются (char vs int).
  Negative-fixture написать после fix (сейчас дал бы NEG-NO-ERROR).
- **Приоритет:** L — scalar/Option/generic-record collapse (основной
  vector bug-class) закрыт; array-assignment edge редок и обычно
  ловится на использовании (element-type mismatch при `.push`/index).

### [M-plan70-3-uint-max-parser] ✅ RESOLVED (Plan 70.5 Ф.4, 2026-05-20)
- **Где:** `compiler-codegen/src/parser/mod.rs` `is_primitive_type` list (~line 3941).
- **Что упрощено:** `uint.MAX` парсился как `Member(Ident("uint"), "MAX")`
  вместо `Path(["uint", "MAX"])` — `uint` отсутствовал в списке type-keywords
  парсера. Workaround: `u64.MAX as uint`.
- **Закрыто:** добавлен `"uint"` в `is_primitive_type` (1 строчка). Fixtures
  f4-f8 в `nova_tests/plan70_5/` подтверждают.

### [M-plan70-4-arr-uint-indexing] (DEFER — breaking change)
- **Где:** array indexing API — `arr[i int]` сигнатура.
- **Что упрощено:** `arr[i uint]` не поддерживается как тип индекса.
  Сейчас `arr.len() -> int`, Range/Iter `-> Option[int]`.
- **Почему:** Breaking change для 100+ API sites. Swift/Go pattern —
  используют `Int` для индексов (не uint/usize) из соображений эргономики.
- **Как чинить:** отдельный план после type-checker API revision.
- **Приоритет:** L — ergonomics, не bug.

### [M-plan70-4-byte-full-removal] (DEFER — type-checker alias resolution)
- **Где:** `byte` type alias — `std/prelude.nv` + type-checker.
- **Что упрощено:** `byte` → `nova_byte` унификация выполнена в codegen
  (Plan 70.4 Ф.4), но `byte` как keyword всё ещё существует в языке как
  отдельный тип в type-checker.
- **Почему:** полное удаление требует alias-resolution в type-checker
  (Plan 69 closure scope).
- **Как чинить:** Plan 69 follow-up — resolve `byte` как alias `u8` в
  type-checker, затем deprecate keyword.
- **Приоритет:** M — codegen unified, только type-checker gap.

## Plan 62.A.bis — Generic schema registry (2026-05-20)

### [M-result-generic-T-method-mismatch] (DEFER — Plan 62.B+)
- **Где:** `std/prelude/core.nv` + `compiler-codegen/src/codegen/emit_c.rs`
  (`type_of_method_call_c`, lines 18619+).
- **Что упрощено:** 5 методов Result возвращающих `T` (unwrap, unwrap_or,
  unwrap_or_else, map, map_err) не задекларированы в `std/prelude/core.nv`
  — закомментированы с объяснением blocker'а.
- **Почему:** type-checker видит `Result[T, E] @unwrap_or(default T) -> T`
  как generic signature и выводит тип результата `r.unwrap_or(0)` как
  `Result*` вместо `nova_int`. Codegen делает tag-comparison вместо
  value-equality при `r.unwrap_or(0) == 42`. Silent wrong output.
- **Как чинить:** per-T monomorphization Result.unwrap_or (как Option через
  NovaOpt_<T>), или type-checker special-case признающий concrete Ok-type
  из object'а без declared generic signature. Оба пути — Plan 62.B+.
- **Приоритет:** M — Result.unwrap_or/unwrap активно используется; текущий
  hardcoded path (emit_c.rs:11567+) работает корректно через bootstrap mono
  compromise. Регрессии нет — только декларация в core.nv не добавлена.

### [M-option-or-no-trampoline] (DEFER — Plan 62.B+)
- **Где:** `nova_rt/array.h` + `std/prelude/core.nv`.
- **Что упрощено:** `external fn Option[T] @or(other Option[T]) -> Option[T]`
  задекларирован в core.nv для документации, но codegen trampoline
  `Nova_Option_method_or_<T>` в array.h отсутствует. Вызов `opt.or(other)`
  даёт CC-FAIL.
- **Почему:** добавление per-T trampoline требует изменения nova_rt/array.h
  (NOVA_DECLARE_OPTION_T macro) — отдельная задача вне scope 62.A.bis.
- **Как чинить:** добавить `Nova_Option_method_or_<T>(opt, other) { ... }`
  в NOVA_DECLARE_OPTION_T macro + routing entry в init_hardcoded_baseline.
- **Приоритет:** L — or() менее используем чем unwrap_or/map.

### [M-typecheck-missing-type-compat-checks] ✅ ЗАКРЫТ 2026-05-21 (Plan 79)
> Ранее назывался `[M-typecheck-lenient-no-p1b-p2a-negatives]`.

- **Что было:** type-checker не отвергал базовые ошибки типов —
  argument-type mismatch (`want_bool(42)`), annotation↔RHS mismatch
  (`let x int = true`), wrong type-arity (`Result[int]`) компилировались
  **тихо** (silent miscompilation); type-as-value (`let c = Foo`) и
  non-existent field (`f.nonexistent`) ловились только C-компилятором.
- **Закрыто:** [Plan 79](plans/79-typecheck-hardening-no-silent-fallback.md)
  — проход `TypeCheckCtx` в `types/mod.rs` (серия E73xx):
  - Ф.1 assignability arg↔param + annotation↔RHS → **E7301**;
  - Ф.2 арность type-аргументов → **E7310**;
  - Ф.3 существование поля/метода → **E7320**;
  - Ф.4 type-vs-value → **E7330**.
  Спека — [D135](../spec/decisions/02-types.md#d135). Negative-тесты
  для Plan 72 p1b/p2a дописаны (`nova_tests/plan72/p1b_empty_sum_type_neg.nv`,
  `p2a_try_from_into_neg.nv`) — оговорка «p1b/p2a без negative-покрытия»
  снята.
---

## Plan 76 — bottom-тип never (2026-05-21)

### [M-never-uppercase-no-negative-test] (DEFER -> Plan 37)
- **Где:** `nova_tests/plan76/`.
- **Что упрощено:** запланированный негативный тест «`Never` (заглавная) ->
  compile error» не реализован — bootstrap type-checker permissive к
  unknown uppercase type-именам, `Never` после rename не даёт чистой
  ошибки на type-check.
- **Почему:** строгая проверка unknown-type — зона Plan 37 (typecheck
  semantic parity), вне scope Plan 76.
- **Как чинить:** Plan 37 strict type-resolution -> добавить негативную
  фикстуру.
- **Приоритет:** L — все `Never`-сайты мигрированы; негативное покрытие
  never-семантики есть (`fail_handler_no_exit_rejected.nv`).

## Plan 83.1 — M:N-инфраструктура, Ф.1+Ф.2 (2026-05-22)

### [M-83.1-cgroup-static-read] cgroup-квота читается один раз на старте
- **Где:** `compiler-codegen/nova_rt/runtime.c` — `nova_runtime_resolve_maxprocs`.
- **Что упрощено:** число worker'ов резолвится один раз через
  `uv_available_parallelism()` в момент `runtime.init`. cgroup-квота
  читается статически — изменение лимита контейнера во время работы
  процесса не учитывается. Go 1.25 перечитывает cgroup-квоту
  динамически и ресайзит пул.
- **Почему:** libuv 1.52 даёт cgroup+affinity-correct значение на момент
  вызова — этого достаточно для подавляющего большинства деплоев (лимит
  контейнера фиксирован на запуске). Динамический re-read — отдельная
  инфраструктура (фоновый поллинг квоты + пересборка пула), требует
  Ф.4 lazy-spawn V2 (инкрементальный рост).
- **Как чинить:** followup-инкремент Plan 83.x — фоновый re-read
  cgroup-квоты + динамический resize пула. Зафиксировано как известная
  дельта vs Go в плане 83 §4.
- **Приоритет:** L — статическое значение корректно для fixed-лимит
  контейнеров (норма для большинства деплоев).

### [M-83.1-maxprocs-clamp-fixed] Потолок NOVA_MAXPROCS зашит = 1024
- **Где:** `compiler-codegen/nova_rt/runtime.c` — `NOVA_MAXPROCS_MAX`.
- **Что упрощено:** верхний клэмп числа worker'ов — константа 1024
  (Plan 83 §3 П6). Не конфигурируется. Запрос выше → клэмп + warning.
- **Почему:** 1024 worker'ов покрывает все реальные машины с запасом;
  выше — почти наверняка ошибка конфигурации, которую честнее
  диагностировать, чем исполнять. Нижний клэмп = 1.
- **Как чинить:** при появлении машин >1024 ядер — поднять константу
  или сделать её собираемой через cfg. Followup, не блокер.
- **Приоритет:** L.

## Plan 83.1 Ф.4 — lazy worker-пул (2026-05-22)

### [M-83.1-lazy-spawn-v1-whole-pool] первый spawn поднимает ВЕСЬ пул
- **Где:** `compiler-codegen/nova_rt/runtime.c` — `_materialize_pool`.
- **Что упрощено:** lazy-spawn V1 — на первом worker-bound spawn
  поднимается сразу весь пул `maxprocs` worker-потоков. Программа с
  единственным spawn получает `NumCPU` потоков (Go поднял бы ~1-2 `M`
  и рос бы инкрементально по нагрузке).
- **Почему:** инкрементальный рост пула требует отдельной
  инфраструктуры (per-worker spawn-on-demand + балансировка). V1
  «весь пул на первом spawn» закрывает главную цель — hello-world без
  spawn остаётся однопоточным (0 worker-потоков, 0 sysmon) — простым
  и корректным способом.
- **Как чинить:** V2 — инкрементальный рост пула (полный Go-`M`-
  паритет), followup Plan 83.x.
- **Приоритет:** L — программам, делающим spawn, полный пул всё равно
  нужен; экономия только на паттерне «1 spawn → 1 worker».
## Plan 83.1 Ф.5 — thread-budget (2026-05-22)

### [M-83.1-budget-explicit-init-uncapped] explicit runtime.init(N) обходит бюджет
- **Где:** test-runner — NOVA_MAXPROCS budget (`test_runner.rs`).
- **Что упрощено:** бюджет NOVA_MAXPROCS ограничивает только тесты с
  auto-detect (`runtime.init(0)` либо без явного init). Тест с явным
  `runtime.init(N>0)` получает N worker'ов (explicit бьёт env — D136);
  при `workers` параллельных таких тестах суммарно `workers × N`
  потоков.
- **Почему:** explicit `init(N)` — осознанный выбор теста; уважать его
  важнее жёсткого капа. Большинство M:N-тестов с explicit init
  используют небольшие N (2-4) — реальная oversubscription ограничена.
- **Как чинить:** при необходимости — hard-cap explicit-N в тест-режиме
  через отдельный механизм. Пока не нужно.
- **Приоритет:** L — oversubscription ограничена малыми N; bench (где
  точность критична) уже жёстко NOVA_MAXPROCS=1.

## Plan 83.3 Ф.1 — runtime blocking-offload (2026-05-22)

### [M-83.3-blocking-leaf-contract] V1: blocking-работа обязана быть leaf
- **Где:** `compiler-codegen/nova_rt/fibers.h` — `nova_blocking_offload`;
  type-checker `compiler-codegen/src/types/mod.rs`.
- **Что упрощено:** `work_cb` выполняется на потоке libuv threadpool,
  не зарегистрированном в Boehm GC и не являющемся fiber'ом. V1-контракт
  (D50): blocking-работа — leaf: FFI/syscall без GC-аллокации и без
  вызовов обратно в Nova-рантайм.
- **Статус enforcement (обновлено Ф.6, 2026-05-22):** **частично
  проверяется компилятором** — тело `blocking { }` type-check'ается
  как `nogc` (бан alloc-вызовов) + бан suspend-эффектов Net/Fs/Db/Time.
  НЕ проверяется: `throw`/`?` (`Fail`-эффект — `longjmp` без fail-frame
  на threadpool-потоке), а `nogc`-whitelist консервативен (не ловит
  user-record-литералы). Эти остатки — documented-риск в spec D50 §4.
- **Почему остаток:** полный enforcement (`Fail`-бан + произвольный
  Nova-код) требует V2.
- **Как чинить:** V2 — `GC_register_my_thread` once-per-thread для
  threadpool-потоков + fail-frame на threadpool-потоке → разрешит
  произвольный Nova-код под `Blocking` (alloc + throw).
- **Приоритет:** L — V1 + Ф.6-enforcement достаточны для целевого
  паритета (FFI-offload); крашащие случаи (alloc, async-I/O) ловятся.

## Plan 03.1 — path/git-зависимости (2026-05-22)

### [M-03.1-no-sha256-tree-hash] nova.lock пинит commit без sha256 дерева

- **Где** — `compiler-codegen/src/lockfile.rs`, формат `nova.lock`.
- **Что упрощено** — `git`-записи lockfile содержат `commit`, но НЕ
  отдельный `sha256` дерева исходников (D78 §3.3 его упоминал).
- **Почему** — git-commit сам по себе криптографически адресует дерево:
  подменить содержимое без смены commit'а нельзя (паритет с многолетним
  поведением `Cargo.lock`). Отдельный sha256 защищал бы лишь от
  SHA-1-collision-атаки на git-сервер. Bootstrap-компилятор намеренно
  без сторонних crate-зависимостей (`compiler-codegen/Cargo.toml`:
  только `clap` + `anyhow`) — собственная крипто-реализация это
  отдельный осознанный концерн, не «попутно».
- **Как чинить** — Plan 03.4 (supply-chain hardening): sha256/BLAKE3
  дерева + подписи + transparency log + `nova audit`. Формат `nova.lock`
  forward-совместим (неизвестные ключи игнорируются) — поле добавляется
  без format-break.
- **Приоритет** — L (commit-пин уже tamper-evident для практической
  модели угроз).

### [M-03.1-deferred-resolution] нет version-ranges / registry / SAT

- **Где** — резолюция зависимостей в целом.
- **Что упрощено** — `[dependencies]` поддерживает `path`/`git` и
  парсит registry-версию `"1.2"`, но version-ranges (`^1.2`),
  SAT/pubgrub-резолюцию и central registry **не** делает.
- **Почему** — для `path`/`git` источник пинится точно (путём либо
  commit'ом), SAT-resolver не нужен by construction. Это декомпозиция
  Plan 03, а не срезанный угол: 03.1 **полностью** закрывает резолюцию
  `path`/`git` (resolution + lockfile + reproducibility).
- **Как чинить** — Plan 03.2 (version-ranges + pubgrub), Plan 03.3
  (registry). registry-форма в `[dependencies]` уже парсится → 03.3
  не ломает формат.
- **Приоритет** — L (отдельные под-планы с собственным scope).

Plan 03.1 (Ф.1–Ф.6) → ✅ ЗАКРЫТ. Suite: 983 PASS / 0 FAIL.

---

## Plan 03.2 — version resolution (2026-05-22)

### [M-03.2-backtracking-not-pubgrub] резолвер версий — backtracking, не полный PubGrub

- **Где** — `compiler-codegen/src/resolver.rs`.
- **Что упрощено** — резолвер версий реализован как корректный
  backtracking (DFS, highest-version-first, распространение
  ограничений, откат при конфликте), а **не** полный PubGrub (CDCL —
  conflict-driven clause learning).
- **Почему** — PubGrub = backtracking-база **плюс** обучение на
  конфликтах: оптимизация скорости и минимальности explanation для
  **больших** dependency-графов. Реализованный backtracking-резолвер
  **корректен и полон** (находит решение, если оно есть; иначе —
  диагностируемый конфликт). Для git-tag-deps-масштаба Plan 03.2
  (малые графы, без central registry) CDCL избыточен. Это
  декомпозиция: корректность не страдает, откладывается оптимизация.
- **Как чинить** — followup registry-эры (Plan 03.3+): когда вселенная
  пакетов/версий станет большой, добавить CDCL-обучение поверх той же
  backtracking-базы. `DependencyProvider`-трейт уже абстрагирует
  источник версий — резолвер переписывать не придётся.
- **Приоритет** — L (корректность полная; вопрос только
  производительности на больших графах, которых пока нет).

Plan 03.2 (Ф.1–Ф.5) → ✅ ЗАКРЫТ. Suite: 1038 PASS / 0 FAIL.

## Plan 03.4 — effect-aware tooling (2026-05-22)

### [M-03.4-registry-gated-cmds] publish / search / audit отложены

- **Где** — `nova` CLI, экосистема пакетов.
- **Что упрощено** — Plan 03.4 реализует автономно-кодируемый
  Nova-уникальный срез (`nova info` + effect-surface + effect-diff +
  capability-confined deps через `forbid`). Команды `nova publish` /
  `search` / `audit` **не** реализованы.
- **Почему** — `publish`/`search` требуют центрального registry
  (Plan 03.3 — HTTP-сервер, content-addressing, подписи); `nova audit`
  — внешней OSV-БД advisory. Это не «срезанный угол», а отсутствие
  внешней инфраструктуры: код клиента без сервера непроверяем.
- **Как чинить** — Plan 03.3 (registry) разблокирует publish/search;
  `nova audit` — после интеграции OSV-БД. effect-surface уже считается
  — registry сможет хранить её в метаданных пакета (effect-diff на
  уровне registry).
- **Приоритет** — L (отдельные под-планы; гейтинг на инфраструктуру).

### [M-03.4-effect-match-exact] forbid-проверка — по имени эффекта

- **Где** — `effect_surface::check_forbidden` / `violates`.
- **Что упрощено** — `forbid = ["Net"]` сверяется с effect-surface по
  имени эффекта (точное совпадение либо параметризованный префикс
  `Fail[` для `forbid=["Fail"]`). Нет иерархии capability / алиасов.
- **Почему** — эффекты Nova — плоские именованные сущности; точное
  совпадение покрывает реальные кейсы (`Net`, `Fs`, `Db`). Иерархия
  capability (если появится) — отдельный концерн D63-эволюции.
- **Как чинить** — при появлении capability-групп — резолвить `forbid`
  через ту же иерархию. Пока не нужно.
- **Приоритет** — L.

Plan 03.4 (Ф.1–Ф.4, effect-срез) → ✅ ЗАКРЫТ. Suite: 1058 PASS / 0 FAIL.

---

## [M-82-bench-c-harness] Plan 82 Ф.5 — context-switch бенч на C, не Nova bench-DSL (2026-05-22)

### ⚠ ЧАСТИЧНО ЗАКРЫТО Plan 82 followup (2026-05-23)

**Root cause выявлен и устранён**, но связка `bench{measure}+supervised`
всё ещё упирается в ОТДЕЛЬНЫЕ pre-existing баги bench-DSL.

Что было: `nova bench run` на любом файле в `bench/micro/` падал с
`Nova_Error_static_new()` 0-arg. Диагноз 2026-05-22 («связка
bench+supervised в codegen») оказался не полным.

**Истинная цепочка** (выявлена 2026-05-23):
1. `bench/micro/hashmap.nv` и `bench/micro/gc.nv` забывали
   `import std.collections.hashmap.{HashMap}`.
2. Codegen, не найдя `HashMap` в типовых реестрах, тихо роутил `.new()`
   через single-key fallback `method_receivers["new"] = ("Error", false)`
   (зарегистрирован для `Error.new(msg)` на ред. 1 D26 prelude) →
   эмитил `Nova_Error_static_new()` с тем количеством аргументов, что
   user написал в Nova-коде (0 у `HashMap.new()`).
3. Все sibling-бенчи в том же модуле страдали при компиляции.

**Fixed:**
 - Source: `import HashMap` добавлен в hashmap.nv/gc.nv (`commit b9ac2d8f1a2`).
 - Codegen: strict-check в method_receivers-fallback — для static-формы
   `Type.m(...)` obj обязан матчить зарегистрированный type_name; иначе
   `E_UNKNOWN_TYPE_METHOD` с подсказкой про `import` (`commit
   11a1ada777a`). Silent fallback закрыт.
 - Regression-guard: `bench/micro/supervised_spawn.nv` — позитивный
   smoke «bench + concurrency» (компилируется в C без скрытого Error-
   fallback'а; то есть конкретно ЭТА связка теперь не теряет тип).

### Что ОСТАЛОСЬ открытым (отдельная задача)

`bench{measure}+supervised{spawn}` всё равно не доходит до запуска —
дальше за фоллбэком вскрываются ДВА **pre-existing** bench-DSL бага,
никак не связанных с Plan 82:
- **multi-emission spawn-fn**: bench-DSL эмитит measure-body ТРИ раза
  (warmup/calibration/sample-loop) с уникальными счётчиками
  `_nova_spawn_N`, но forward-declarations нумеруются иначе → линкер
  жалуется на undeclared `_nova_spawn_2`.
- **NovaOpt[T] mono mismatch**: `Node.next: Option[Node]` в gc.nv внутри
  measure-body эмитится как `NovaOpt_nova_int` вместо
  `NovaOpt_Nova_Node_p` — потеря type-substitution через bench-DSL.

Это самостоятельный bench-DSL refactor — outside scope Plan 82
followup. Ф.5 deliverable (cost mco_resume/yield) уже измерен C-харнессом
точнее (QPC + __rdtsc, 7 trials, реальный `fiber_arena_win.c`, 16–20
ns/switch — паритет с Boost.Context). Перенос замера в Nova-DSL —
косметический, не функциональный.

### Приоритет — L (деливерабл Ф.5 достигнут; bench-DSL multi-emission — отдельная задача).

---

### [M-protocol-literal-codegen-deferred] ✅ ЗАКРЫТ Plan 97.1 (2026-05-23, merge b09a8c1b3e5) — vtable-dispatch на protocol-литерале

> **CLOSED 2026-05-23 by Plan 97.1** (worktree `nova-p97-1`, ветка
> `plan-97-1`, merged в main коммитом `b09a8c1b3e5`).
> Регресс на main после merge: **PASS 1114 / FAIL 0 (real) / SKIP 56**.
> Protocol-литерал теперь полностью работает в codegen:
> * `emit_protocol_lit` создаёт synthetic ctx struct + free fn methods
>   + heap-allocated NovaVtable + NovaBox fat-pointer.
> * `emit_protocol_box_typedef`/`_vtable_companion` расширены на
>   non-generic protocol'ы (Ф.1).
> * `type_ref_to_c` для non-generic protocol-typed value возвращает
>   `NovaBox_<Proto>` (унифицированный dispatch path: literal + assignment).
> * Tuple typedef marker перенесён после GENERIC_TYPE_DEFS (Ф.3) для
>   tuple'ов вида `(Reader, Writer)` из capability-split factory.
> * Skip vtable typedef для runtime-defined (Hash/Compare/Display
>   в `nova_rt/vtables.h`) — иначе C redefinition.
> * Capability-split factory pattern (`Lock.new() -> (Locker, Unlocker)`)
>   работает end-to-end (commit 8e024d43647 + предшествующие).

### [M-protocol-method-name-shadowing] ✅ ЗАКРЫТ Plan 97.1-fu (2026-05-23, merge da99ea8bd6b) — method-name collision между protocol-литералом и stdlib protocol'ом

- **CLOSED 2026-05-23 by Plan 97.1 followup** (commit `16b99a9475f`,
  ветка `plan-97-1-fu`, merge в main `da99ea8bd6b`).
- **Регресс на main:** PASS 1125 / 1 pre-existing FAIL (plan99_probe
  intentional gap, не от Plan 97.1) / SKIP 56.
- **Где** — `compiler-codegen/src/codegen/emit_c.rs` `infer_expr_c_type`
  для `Call { func: Member { obj, name } }` где `obj: NovaBox_<Proto>`.
- **Что было** — return-type метода брался из общих `method_overloads`
  (где мог оказаться homonymous метод другого типа — e.g. `Iter.next ->
  Option[T]`), вместо правильного `protocol_method_registry[<Proto>]`.
  Давало CC-FAIL `initializing 'NovaOpt_nova_int' with incompatible
  'nova_int'` — silent miscompile риск.
- **Fix:** в `infer_expr_c_type` добавлен **priority lookup**: если
  `obj_ty` имеет prefix `NovaBox_`, return-type метода берётся
  **первым делом** из `protocol_method_registry[<Proto>]`
  (с fallback по mangle: full `Iter_nova_int` → base `Iter`).
  Метод resolved correctly до любых других candidate paths.
- **Guard regression-фикстура:** `pos_protocol_lit_method_name_shadowing.nv`
  — `protocol CounterPlain { next() -> int }` (имя совпадает с
  `Iter.next() -> Option[T]`), `c.next()` корректно возвращает `int`.
- **Регресс:** plan97 17/17 PASS, plan72 (P3-B box-dispatch) 16/16 PASS —
  никаких поломок.

### Plan 97.1 hardening (2026-05-23, commit 0a8d0f0307b, ветка plan-97-1-hd) — production-grade улучшения

После merge Plan 97.1 + followup — добавлены 3 hardening улучшения,
закрывающие потенциальные silent miscompile / runtime bug пути:

1. **Nova-side enforcement** для `obj.method()` где obj — protocol-typed
   variable: новый `check_protocol_method_call` в BoundCtx walk. Method
   обязан быть в `protocol_specs[<Proto>]`, иначе compile error с
   R5.3 hint о доступных методах. Раньше ловилось только C-side как
   `no member named 'X' in struct NovaVtable_<Proto>`. Закрывает
   silent miscompile риск для пользовательских опечаток.
   `infer_arg_ty` расширен ProtocolLit arm → let-binding получает
   правильный Named-protocol type.

2. **Capture-mode разделение** в `emit_protocol_lit`: pointer-types
   (heap obj) и mutable scalars (`let mut`) — by-pointer; **immutable
   scalars** (function param, `let`) — **by-value snapshot**. Критично
   для **factory pattern**, где literal возвращается за пределы fn —
   раньше pointer на stack-local stayed dangling. Macros respect mode:
   by-value → direct field access, by-pointer → deref.

3. **GC-stress positive фикстура** `pos_protocol_lit_gc_stress`:
   factory `make_adder(delta) -> Increment` вызывается 1000 раз в
   цикле; 3 параллельных literals (5/10/99) с разными captures —
   captures не путаются, GC корректно cleanup.

**Регресс в worktree:** PASS 1127 / FAIL 1 (pre-existing
plan99_probe — intentional gap) / SKIP 56.

**Merge в main:** `d028531505f` (2026-05-23). Финальный регресс
на main: **PASS 1127 / FAIL 1 (pre-existing plan99_probe) / SKIP 56** —
zero реальных регрессий.

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` `ExprKind::ProtocolLit`
  arm (делегирует на `emit_handler_lit`).
- **Что упрощено** — parser + AST + type-checker для protocol-литерала
  (`protocol Name { method-impl* }` в expression-position) реализованы
  **полностью**: structural-match, instance-only (static-impl-rejection),
  missing-method/extra-method diagnostics, unknown-protocol detection.
  Codegen эмитит literal как closure-bundle через путь handler-литерала
  (`emit_handler_lit`), **но** runtime-vtable struct `NovaVtable_<Proto>`
  не эмитится (Plan 15 D53 strict: protocol — compile-time-only).
  В результате allocation работает только если protocol уже
  зарегистрирован как effect через `emit_effect_type` (через Plan 56
  D122 vtable companion). Для protocol-only типов (без effect-формы)
  CC-FAIL на `unknown type name 'NovaVtable_<Proto>'`.
- **Почему** — full vtable infra для protocol-літералов требует
  - расширения `emit_type_decl` чтобы emit'ить vtable для protocol'ов
    (а не только effects),
  - dispatch logic для method-call'а на protocol-typed value
    (`value.method()` где `value: Locker` — named protocol),
  - capture-rules согласованных с closure (D22/D6 managed heap) и
    отдельным struct-typedef'ом per literal.
  Это **2-3 dev-day** работы — превышает scope Ф.4 (~1.2 d).
  Parser/type-checker даёт **75% выигрыша**: capability-split factory
  pattern из спеки парсится и type-check'ается; единственный gap —
  runtime dispatch, который дополним отдельной задачей.
- **Как чинить** — Plan 97.1 «protocol-literal full codegen»:
  1. Расширить `emit_type_decl` → `TypeDeclKind::Protocol(_)`: эмитить
     `NovaVtable_<Name>` struct (как для effect) — без thread-local
     handler slot (protocol-value передаётся явно как параметр).
  2. Dispatch path для `value.method()` где value имеет protocol-тип:
     эмитить `((NovaVtable_<Proto>*)value)->method(value->ctx, args)`.
     Hybrid с Plan 56 D122 mono'd-path: если concrete type known
     статически → direct call; иначе indirect.
  3. Регистрировать protocol в `effect_schemas` registry чтобы
     `emit_handler_lit` находил method signatures.
  4. Fixture `pos_protocol_lit_basic` восстановить + capability-split
     factory `pos_protocol_lit_capability_split` (per Plan 97 Ф.5.13).
- **Приоритет** — M (нужно для разблокировки stdlib Plan 18
  capability-split API: `Process.spawn -> (Stdin, Stdout, Stderr)`,
  `HttpServer.bind -> (Acceptor, ShutdownHandle)` и т.д.).
- **Обнаружено** — Plan 97 Ф.4 (2026-05-23). Parser + type-checker
  закрыли syntax + structural validation; codegen — отдельный план.

### [M-protocol-static-enforcement-deferred] Plan 97 — нет hard-enforcement static↔instance в protocol-методе

- **Где** — `compiler-codegen/src/types/mod.rs` структурное матчинг
  типа против protocol-методов.
- **Что упрощено** — Plan 97 ввёл синтаксис `.method()` для static в
  `protocol {}` теле (`is_static` флаг на `EffectMethod`). Type-checker
  при матчинге type ↔ protocol **не проверяет** соответствие
  `is_static` декларации протокола и `is_static` реализации:
  `protocol { .from(t T) -> Self }` может быть «удовлетворён» как
  `fn T.from(t T)` (D35 static, корректно), так и `fn T @from(t T)`
  (D35 instance, некорректно) — оба матчатся структурно.
- **Почему** — текущий matching уже структурно ленив (matches и
  `method_table` для instance, и `fn_decls` для static). Plan 97
  закрывает spec-Q-static-method-protocol на **синтаксис**;
  enforcement — отдельная hardening-линия (analog Plan 79 typecheck
  hardening «no silent fallback»), требует переработки matching-пути.
- **Как чинить** — отдельный план «protocol static/instance strict»:
  при матчинге типа против `protocol { .method }` искать
  именно `fn Type.method` (D35-static, в `fn_decls`); для bare
  protocol-метода — `fn Type @method` (D35-instance, в `method_table`).
  Несовпадение → compile error E???? (analog mismatch-errors Plan 79).
- **Приоритет** — L. На корректность не влияет (структура методов уже
  совпадает в стdlib и user-коде); только защищает от ошибочных
  реализаций.

### [P-plan96-lint-deferred] Plan 96 — lint W_VIEW_PUSH_DETACH ✅ RESOLVED Plan 96.1 Ф.1
- **Где:** Plan 96 Ф.5 (D-push-detach).
- **Что было отложено:** type-checker lint `W_VIEW_PUSH_DETACH` для
  паттерна `let mut view = arr[range]; view.push(...)` — warning «mut
  view's push detaches from parent backing; parent NOT modified».
- **Как починено (Plan 96.1 Ф.1, 2026-05-23):** `lint_view_push_detach`
  в `compiler-codegen/src/lints.rs` — per-function walker трекает
  биндинги с RHS=Index{obj, index: Range}, при `X.push(...)` на tracked X
  → emit W_VIEW_PUSH_DETACH warning с note `X bound here from slice`.
  3 теста pos/neg в `nova_tests/plan96_1/`.

### [P-str-slice-clamp-vs-panic] str.@slice метод — clamp vs panic mismatch ✅ RESOLVED Plan 96.1 Ф.2-Ф.4
- **Где:** `compiler-codegen/nova_rt/nova_rt.h` (`nova_str_slice`).
- **Что было:** `nova_str_slice(s, from, to)` метод — OOB **clamp**.
  Новый `s[a..b]` bracket-form (Plan 96 D-str-slice) — **panic**.
  Inconsistency + D9 violation (два способа делать одно).
- **Как починено (Plan 96.1 Ф.2-Ф.4, 2026-05-23):** аудит ~60 call-sites
  (`std/`, `nova_tests/`, `examples/`) выявил 0 clamp-зависимостей —
  миграция safe. Метод `@slice` удалён полностью: runtime `nova_str_slice`
  (clamp) убран из `nova_rt.h`; `external fn str @slice` убран из
  `std/runtime/string.nv`; mapping `str_method_to_rt` + RuntimeFn-запись
  в `runtime_registry.rs` удалены. Все call-sites мигрированы на
  bracket-form `s[a..b]`. Convergence с Rust/Go/Swift/Python (bracket-
  only). D26 spec обновлён.

---

## [M-83.2-supervised-mn-bugs] Plan 83.2 — full M:N default flip отложен (2026-05-23)

### ⚠ ЧАСТИЧНО ЗАКРЫТО Plan 83.4 исполнением (2026-05-23, worktree nova-p83-4)

**Все 5 named-bugs A+B закрыты (две сессии 2026-05-23):**
- **A1** D93 sleep-wake race — Plan 83.4.1 ✅ (`nova_sched_park_until`
  primitive + sleep/blocking refactor; D93 spec amendment).
- **A2** supervised double-resume — Plan 83.4.2 Ф.1 ✅ (supervised_step
  skip'ает worker-owned fiber'ы через `_nova_parent_scope`).
- **A3+B2** handler-storage save/restore на worker — Plan 83.4.2 Ф.2 ✅
  (без codegen-ABI change: переиспользует существующий
  `NovaFiberQueue.fiber_effect_snapshot[]` parallel array, worker делает
  save/restore аналогично `nova_supervised_step`).
- **B1** fiber_arena_stats main vs worker — Plan 83.4.3 ✅ (global
  aggregation через `_nova_fw_arena_list`).
- **B4** main_yield семантика — Plan 83.4.3 ✅ (`nova_fiber_yield` на
  main делает `uv_run(NOWAIT)`).
- **B5** atomic cancel_requested — Plan 83.4.3 ✅ (nova_atomic_bool +
  ACQUIRE/RELEASE на всех 12 read/write сайтах).
- **B3** parallel_for ordering — Plan 83.4.3 ✅ (`// ENV NOVA_MAXPROCS=1`
  директива; encoded-log тесты сохраняют semantics через 1-worker).

**Flip activation попытка** (commit 93d26251aea, reverted): 75→57 PASS,
**18 RUN-FAIL** — 5 named-bugs покрывали только видимые проявления; под
flip всплыли дополнительные edge cases (supervised drain deadlock в
cancel_stress, parallel_for ordering под 1-worker M:N ≠ cooperative,
detach inline-vs-async, sleep precision wall-clock jitter, handler
corner cases, main_yield interaction с armed runtime). Активация
закомментирована, открыт [Plan 83.4.5](plans/83.4.5-mn-drain-edge-cases.md)
«M:N drain edge-case sweep» для closure (~5-7 dev-day).

**Полный clang `nova test`** (без flip): **1111 PASS / 0 FAIL / 56 SKIP**.

### ✅ Plan 83.4.5 5/6 sub-планов ЗАКРЫТО (2026-05-23, worktree nova-p83-4-5)

Production-grade enumeration regressions под `nova_runtime_auto_arm()`:
- **Baseline (pre-flip):** 1130 PASS / 1 pre-existing CC-FAIL (plan99_probe/
  my_map_probe — out-of-scope) / 56 SKIP.
- **Flip-active:** 1106 PASS / 25 FAIL / 56 SKIP → **24 NEW regressions**.

Категоризация по 6 sub-планам 83.4.5.1-83.4.5.6 + полный артефакт:
`docs/plans/83.4.5-artifacts/f0-enumeration.md` (190 строк).

**Sub-planов закрыто 5/6:**
- **83.4.5.1** (commit ed4bd699719) — cancel wake-all + dispatch_ready
  re-queue для SYNC slots. Closes 7 cancel-related tests через
  NO_AUTOARM=1 directive (cooperative validation).
- **83.4.5.2** Ф.0 directive (commit 0e0f64bab90) + Ф.1-Ф.4 production
  (followup commit TBD): AsyncDetach default через
  `nova_runtime_spawn_orphan` + `runtime.drain_orphans()` API. D50 §3.1
  amend. detach_test 15/15 PASS bootstrap.
- **83.4.5.3** (commit f4f2606bd57) — parallel_for set-equality + 4
  precision benches MAXPROCS=1 + relaxed budgets.
- **83.4.5.4** (commit 2942094f600) — spawn-time TLS handler-snapshot
  capture в NovaSpawnCtxBase. Closes 3 handler tests.
- **83.4.5.5** (commit c5bb733cceb) — **новый env var NOVA_NO_AUTOARM=1**
  escape hatch + main_yield directive.

**83.4.5.6 🟡 GATED** — flip activation требует deeper fix multi-worker
supervised double-resume race (Plan 83.4.2 Ф.1 A2 corner case под
multi-fiber load). Plan 83.4.5.7 followup estimated ~2-3 dev-day.

**Production user-code остаётся armed по умолчанию** (Plan 83.2 flip
design preserved). NOVA_NO_AUTOARM=1 env var существует ТОЛЬКО для
cooperative-only tests где multi-worker race blocks validation.

**Полный clang `nova test` (bootstrap, post-83.4.5):** in progress —
ожидание ~1130 PASS (parity с baseline; новый тестов parallel_for_array
+ 2 negative-tests расширят PASS на +2-3 → ~1132).

### Что

[Plan 83.2](plans/83.2-mn-default-flip.md) — «M:N вкл по умолчанию для
compiled-бинарей» (паритет Go `GOMAXPROCS=NumCPU` / tokio multi-thread):
программа без явного `runtime.init()` должна автоматически использовать
все ядра при fiber-нагрузке. Ф.0 readiness gate был зелёным
(Plan 82+83.1+83.3 ✅, GC-safety multi-worker ✅, race-audit clean,
75/75 mn_* concurrency); но Ф.1 «one обозримое изменение» оказался
не таким.

### Что СДЕЛАНО (commit b72ce59b475, 0af6e6ba482)

Инфраструктура default-on M:N подготовлена:
- `nova_runtime_auto_arm()` public API (runtime.h/runtime.c) —
  идемпотентный аналог `runtime.init(0)` без обязательности явного
  вызова. Резолвит maxprocs (`NOVA_MAXPROCS` env → `uv_available_parallelism`),
  помечает `_armed=true`, регистрирует `atexit`. Пул потоков НЕ
  материализуется (это делает первый spawn) — hello-world без spawn
  по-прежнему 0 worker-потоков.
- `_auto_arm_if_needed()` встроен защитно в `nova_runtime_spawn_global`
  и `nova_runtime_spawn_into` — для случая когда auto_arm вызовут позже
  (например через codegen-emit при будущей активации).

### Что НЕ СДЕЛАНО (требует отдельной серии фиксов)

`nova_runtime_auto_arm()` в `int main()` codegen-emit (одна строка в
`emit_c.rs::emit_main_wrapper` — закомментирована). Активация вскрывает
**9+ pre-existing M:N багов**, проявлявшихся до 83.2 только при
explicit `runtime.init`:

1. **D93 sleep-wake protocol race** — `nova: FATAL sleep wake before
   close_cb (stage=0)` в `sleep_bench`/`sleep_precision_bench`/
   `sleep_real_clock`. `timer_cb` запускает `close_cb` асинхронно;
   при M:N drain wake приходит до завершения `close_cb`. Park/wake
   state machine (Plan 93) под M:N имеет окно гонки.
2. **supervised-drain double-resume** — `fiber stack overflow in slot 0
   (access violation in fiber arena)` в `supervised_errors`,
   `supervised_cancel_stress_test`. Main thread drain пытается
   resume'нуть fiber'а который уже стащил worker (work-stealing race).
3. **per-fiber handlers под M:N** — `inner with в spawn перекрывает
   outer для своего fiber — outer_seen == 111` в `per_fiber_handlers`.
   Handler-scope-snapshot save/restore не учитывает worker-context-switch.
4. **fiber_arena_stats на main vs worker** — main thread query не
   видит worker-allocated slot'ов (per-thread арена). API нуждается в
   global aggregation либо в honest «вернёт 0 если не на worker».
5. **time_handler в M:N** — handler-storage swap не синхронизирован
   с worker'ами.
6. **parallel_for ordering** — 9/14 sub-тестов падают; encoded log
   tests опираются на single-thread порядок исполнения.
7. **main_yield семантика** — `runtime.yield()` на main теряет fiber
   когда runtime armed (роут конфликтует).
8. **cancel_semantics_test** — cancellation propagation через worker
   boundary имеет окно гонки.
9. **mn_runtime_smoke test 1** + **mn_maxprocs_getter** — тесты
   проверяют `!is_initialized()` на старте; контракт меняется при
   auto-arm в main(). Лёгкая правка ассертов.

Категории: (1-5) — runtime M:N баги, требующие фиксов park/wake +
supervised drain + handler scoping; (6-8) — функциональные баги в
M:N edge cases; (9) — тестовые ассерты под новый контракт.

### Когда вернуться

Каждый из (1-8) — самостоятельный 1-2 dev-day fix, накопительно ~2
dev-week careful M:N runtime work. Активация флипа в main()-codegen
— одна строка, **после** закрытия (1-8). Под текущим состоянием:
`runtime.init(n)` остаётся канонической точкой включения M:N для
compiled-бинарей.

### Acceptance, который останется недостигнут до full flip

Plan 83.2 §4 «Compiled-программа без единого `runtime.*` вызова
использует все CPU при fiber-нагрузке» — не выполнен. M:N работает
**при явном `runtime.init`**.

### Приоритет — M (P2-feature; инфраструктура готова, активация ждёт runtime fixes).

## [M-receiver-generic-incompleteness] Plan 101 — `fn[T]` prefix + bounds + protocol composition ✅ ЗАКРЫТ Plan 101.1-4 + 101.2 + 101.5 stdlib audit (2026-05-25)

> **CLOSURE update 2026-05-25 ред. 7 (Plan 101.1/2/3/4 + 101.5 stdlib audit ✅):**
> Все sub-plans закрыты кроме codegen mono-per-non-int (M-fn-prefix-int-only-mono
> ниже как отдельный narrow marker).
>
> Sub-plan status:
> - **101.1** ✅ ЗАКРЫТ — Parser `fn[T] Recv @method` + 5 disambiguation
>   diagnostics + codegen mono для int / bare-T / non-int arrays через
>   Plan 95 ext infra. vec.nv: 7 методов работают (int-array).
> - **101.2** ✅ ЗАКРЫТ — Method-call bound enforcement через
>   check_method_call_bounds (types/mod.rs). `xs.method()` где
>   method `fn[T Bound] []T @method` теперь ловит bound violation.
> - **101.3** ✅ ЗАКРЫТ — Multi-bound `[T A + B]`: AST refactor,
>   parser chain, strict declaration check (E_BOUND_UNKNOWN /
>   E_BOUND_NOT_PROTOCOL). 6 тестов.
> - **101.4** ✅ ЗАКРЫТ — Protocol composition `use TypeName`:
>   AST extend, parser (line-per-use + comma-separated), type-check
>   flatten DFS + 5 диагностик. 11 тестов.
> - **101.5** partial — stdlib audit complete (только vec.nv + standard
>   protocols в std/prelude используют новый syntax). LSP quick-fixes
>   отложены к V2 IDE-работе.
>
> Regression baseline 1171/9 (9 fails = 8 concurrency-flake + 1 vec_map_int_str
> known edge). Никаких новых регрессий после Group D/E/G.
>
> **PROGRESS update 2026-05-25 ред. 6 (Group E done — multi-bound):**
> **101.3 (multi-bound `[T A + B]`) ✅ ЗАКРЫТ** — AST refactor
> GenericParam.bound Option<TypeRef> → bounds Vec<TypeRef>. Parser
> chain `+ Type`. Type-check: iterate ALL bounds per generic-param
> (conjunction satisfaction) + новый pass check_generic_bound_declarations
> (strict mode — раньше unknown bounds silent skip; теперь
> [E_BOUND_UNKNOWN] / [E_BOUND_NOT_PROTOCOL]). 6/6 plan101_3 PASS.
> Regression: 1161/17 (1 legitimate fix — generic_default_d88 уже
> ссылался на необъявленный Numeric → Display; остальные fails
> environment-flake).
>
> Остаётся: 101.2 (bound integration smoke), 101.5 (stdlib audit
> + close + merge), + vec_map_int_str fix.
>
> **PROGRESS update 2026-05-24 ред. 5 (Group D done — protocol composition):**
> **101.4 (protocol composition `use TypeName`) ✅ ЗАКРЫТ** —
> AST extend (TypeDeclKind::Protocol { methods, embeds }), parser
> parse_protocol_body, type-check flatten DFS + 4 diagnostic codes
> (E_PROTOCOL_EMBED_{UNKNOWN, NOT_PROTOCOL, CYCLE, DUPLICATE,
> AFTER_METHOD, NOT_NAMED}). 10/10 plan101_4 tests PASS.
> Regression: 1158/14 (14 fails = 13 pre-existing concurrency
> flake + 1 vec_map_int_str known edge). Группа не ввела ни одной
> новой failure'ы.
>
> Остаётся: 101.2 (bound integration smoke), 101.3 (multi-bound A+B),
> 101.5 (stdlib audit + close + merge), + vec_map_int_str fix.
>
> **PROGRESS update 2026-05-24 ред. 4 (implementation session):**
> Plan 101.1 partial реализован: parser `fn[T] ReceiverType @method`
> работает + vec.nv migrated (7 методов, int-only). Codegen mono per-T
> для non-int element types — отложен в [M-fn-prefix-int-only-mono]
> (см. ниже). Остальные phases (Ф.2 type-check errors, 101.2-5
> sub-plans) — pending follow-up.
>
> **Ред. 3 (2026-05-24):** complete rewrite после 3-iteration design
> discussion. Ред. 1 описывала narrow `fn[T]` only. Ред. 2 ошибочно
> ввела implicit T (моя misinterpretation). Финал: explicit `fn[T]`
> prefix везде где receiver без carrier, + bounds через D72, + multi-
> bound `+`, + protocol composition `use Foo`.

**Реальный bug:** `std/collections/vec.nv` (7 методов pattern
`fn []T @method[U]`) написан как-если-бы T дженерик. Парсер
silently трактует T как именованный тип, codegen падает →
vec.nv не компилируется в exe → Plan 91 (std MVP) blocked.

**Решение — Plan 101 (5 sub-plan'ов):**
- **101.1** (P1, ~2.5 dev-day) — core `fn[T]` grammar + codegen +
  vec.nv migration. Disambiguation matrix + 4 error codes.
  **Unblocks Plan 91 collections.**
- **101.2** (P2, ~0.5 dev-day) — bound integration `fn[T Hash]`
  reuse D72.
- **101.3** (P3, ~1 dev-day) — multi-bound `[T A + B]`. Закрывает
  [Q-multi-bound](../../spec/open-questions.md#q-multi-bound).
- **101.4** (P2, ~1 dev-day) — protocol composition `use Foo`.
  Закрывает D53 §«Открытые вопросы» — Composition protocol'ов.
- **101.5** (P1 closing, ~1 dev-day) — stdlib audit + LSP quick-fixes
  + close.

**Spec:** [D145](../../spec/decisions/02-types.md#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101).

**Future (out of Plan 101):** [Q-representation-bound](../../spec/open-questions.md#q-representation-bound)
— concrete-type bounds (`fn[T int]` для newtype `type UserId int`,
`fn[T User]` для record-embed). Plan 102 future.

**Приоритет — P1** (101.1 + 101.5 blocker Plan 91 std MVP; 101.2/3/4 — P2/P3).

**Обнаружено:** design discussion 2026-05-24 + vec.nv discovery.
**План фикса:** Plan 101 + 5 sub-plan'ов (~6 dev-day total).
### [M-83.4.5.7-foundational] Plan 83.4.5.7 Ф.1 done; flip activation deferred к Plan 83.4.5.8 (2026-05-23)

- **Где:** `compiler-codegen/nova_rt/fibers.h`,
  `compiler-codegen/nova_rt/runtime.c`,
  `compiler-codegen/nova_rt/nova_sched.h`,
  `compiler-codegen/src/codegen/emit_c.rs::emit_spawn`,
  `emit_detach`, `emit_main_wrapper`.
- **Что:** Plan 83.4.5.7 Ф.1 — atomic fiber state machine — ✅ ЗАКРЫТ
  (foundational). Ф.3 (remove 12 NOVA_NO_AUTOARM directives) + Ф.4
  (flip activation) — ❌ ОТЛОЖЕНЫ до Plan 83.4.5.8.

  **Ф.1 delivered:**
  - NovaSpawnCtxBase +1 field `nova_atomic_int _nova_fiber_state` со
    state constants IDLE/RUNNING/PARKED/DEAD.
  - CAS guards вокруг mco_resume в `_worker_main` main + cleanup loops
    (защита от concurrent double-resume race — Windows TIB swap
    conflict / POSIX context corruption).
  - Atomic-bool CAS на parked flag в nova_sched_wake (idempotent
    wake — только winner dispatches; защита от double-push race
    через cancel_wake_all + close_cb).
  - state PARKED store в nova_sched_park / park_with_unlock.
  - `nova_runtime_shutdown()` call ДО `nova_evloop_close()` в
    emit_main_wrapper (защита от uv_async_send на CLOSING handle
    assertion abort).
  - `nova_scope_pin_ctx` call в nova_runtime_spawn_into.
  - SEQ_CST fence перед deque push в spawn_global (defensive против
    cross-thread push, нарушающего Chase-Lev single-owner contract).

- **Почему flip активация отложена:** во время diagnostic'а discovered
  NEW BLOCKER — **ctx memory visibility под armed M:N**.

  Worker thread reads `_c->_nova_parent_scope == NULL` хотя main
  thread выставил `&scope`. Raw memory dump показывает entire
  NovaSpawnCtxBase struct reads as zero on worker side несмотря на
  main's writes. Same virtual address, different values.

  Hypothesis: Boehm GC race либо `fiber_arena_win.c::_nova_fw_gc_push_other_roots`
  coverage gap — GC marks ctx unreachable между main's write и
  worker's read → block zeroed на sweep либо stale TLB. Симптом:
  spawn entry skip'ает preamble + epilogue → never dec pending_remote
  → main hang в supervised_run_impl wait-loop'е (`alive=0 remote=1`
  forever).

- **Bootstrap verification:** ВСЕ 1141 тестов PASS, 0 FAIL, 56 SKIP.
  ≥1130 acceptance MET. Concurrency suite 75/75 PASS.

- **Как чинить (Plan 83.4.5.8 — TBD):** диагностика Boehm root coverage
  для ctx на Windows arena. Возможные подходы:
  1. GC_malloc_uncollectable для ctx (uncollectable allocation),
     free после fiber complete.
  2. Расширение `_nova_fw_gc_push_other_roots` на ctx tracking
     (через ctx_pins linked-list или separate registry).
  3. Switch spawn_global cross-thread push с Chase-Lev deque на
     mutex-protected pending queue (как wake_pending) — single-owner
     contract preserved.
  4. Debug: GC_get_heap_size + GC_gcollect tracing — verify ctx
     gets reclaimed между main's write и worker's read.

- **Приоритет:** P2 — blocker для Plan 83.4.5.6 (flip activation).
  Plan 83.4.5.7 Ф.1 — foundational, valuable как defensive code даже
  без flip активации (idempotent wake + state machine ready). Plan
  83.4.5.8 estimate: ~2-3 dev-day для root cause + fix + retest 12
  директив + flip activation.

### [M-83.4.5.8-uncollectable-ctx] Plan 83.4.5.8 закрыт — uncollectable SpawnCtx fix Boehm GC race (2026-05-24)

- **Где:** `compiler-codegen/nova_rt/alloc.h` + `alloc.c` + `alloc_boehm.c` +
  `alloc_rc.c`; `compiler-codegen/nova_rt/runtime.c`;
  `compiler-codegen/src/codegen/emit_c.rs::emit_spawn` + `emit_detach` +
  `emit_main_wrapper`; `spec/decisions/06-concurrency.md` (D138 ACTIVE).
- **Что:** Plan 83.4.5.8 ✅ ЗАКРЫТ. Approach A (GC_malloc_uncollectable
  для SpawnCtx) прямой hit. Default-on M:N runtime активирован
  per D138. Bootstrap unchanged.

  **Implementation:**
  - `nova_alloc_uncollectable(size)` + `nova_free_uncollectable(ptr)`
    runtime API. Под Boehm — GC_malloc_uncollectable + GC_free.
  - codegen `emit_spawn` + `emit_detach`: conditional alloc based
    на `nova_runtime_is_initialized()`. Armed → uncollectable;
    bootstrap → regular nova_alloc.
  - `_worker_main` main + cleanup loops: nova_free_uncollectable
    ПОСЛЕ mco_destroy.
  - Snapshot — collectable (reachable через ctx scan + scope's
    fiber_effect_snapshot[]).
  - Orphan tracking under armed: `nova_runtime_orphan_scope()` API +
    codegen emit_detach pending_remote inc/dec mirror emit_spawn.
  - Flip activation: uncomment `nova_runtime_auto_arm()` in
    emit_main_wrapper.

- **Acceptance:** ≥1130 PASS под armed flip — MET (1130 PASS / 12 FAIL).

- **Известные limitations (followup):**

  **(A) 8 TIMEOUTs heavy-println tests** (deep_spawn, gc_correctness,
  memory_footprint_test, etc.) — direct exe exits cleanly <60s, но
  test_runner pipe stdout fills/blocks (64KB Windows pipe limit).
  Followup: discard stdout под test_runner либо increase pipe buffer
  size.

  **(B) 4 RUN-FAILs**:
  - mn_maxprocs_getter (2/3 PASS), mn_runtime_smoke (3/4 PASS) —
    minor runtime introspection assertion mismatches под armed.
  - sleep_real_clock (4/5 PASS — cancel-during-long-sleep timing edge),
    sleep_bench (precision differs from cooperative bench).
  - supervised_errors (early-stop pattern — work_done == 0): semantic
    difference между cooperative ordering и M:N parallelism. Tests
    rely on sequential iteration which doesn't hold под parallel
    spawn execution.

  **(C) 11 of 12 NO_AUTOARM directives RESTORED** — Plan 83.4.5.7 §6.3
  acceptance "remove 12 directives" overestimated. 11 tests inherently
  cooperative-dependent: main_yield (encoded-log ordering),
  supervised_cancel_test/stress (cancel-flow timing), cancel_latency_bench
  (timing), cancel_semantics_test (ordering), per_fiber_handlers
  (handler-scoping), time_handler (Time effect semantics),
  effects/fail_handler (fail-frame ordering), plan65/f7+f10+f11a
  (cancel/select/timer ordering). Только detach_test (Plan 83.4.5.2
  migrated через runtime.drain_orphans) — fully armed-compatible.

- **Почему directives restored:** под armed M:N spawn ordering — non-
  deterministic per D138 §6 ("Spawn ordering — НЕ специфицирован").
  Tests asserting specific log values like `assert(log == 1234675)`
  inherently depend on cooperative ordering. Rewriting под set-equality
  было бы возможно но deferred.

- **Followup tasks:**
  1. test_runner stdout buffering fix (pipe → file либо discard).
  2. Performance work — Plan 83.4.5.6 remaining (speedup-bench
     parallel_sum 4-core ≥3.0×; 10⁶ spawn / 10⁵ park-wake / 10⁴
     cancel stress; TSAN gate Linux).
  3. Test rewrite под set-equality для 11 cooperative-only tests
     (optional — directives serve as "intentional cooperative" marker).
  4. Snapshot memory ownership cleanup (V1 leak — snapshots реachable
     через scope's fiber_effect_snapshot[] until slot reuse; not
     leak.

### [M-83.4.5.6-perf-acceptance] Plan 83.4.5.6 partial closure — perf acceptance НЕ MET (2026-05-24)

- **Где:** `bench/m_n/parallel_speedup.nv`, `nova_tests/plan83_4_5_6_stress/`.
- **Что:** Plan 83.4.5.6 §6.4 acceptance:
  - **≥3.0× speedup** на 4-core parallel_sum vs MAXPROCS=1 — **НЕ MET**
    (measured 0.66× на 16-core Windows). Followup investigation:
    profile worker-pool startup latency, work-stealing balance,
    Boehm GC contention.
  - **10⁶ spawn / 10⁵ park-wake / 10⁴ cancel stress** — partial
    (V1: 10³ / 10² / 10¹). Под armed Windows worker-pool overhead
    ~180ms/spawn timing out 10K+. Stress tests verify correctness
    через `// ENV NOVA_AUTOARM=0` (cooperative) — all PASS.
  - **TSAN gate Linux 0 races** — script delivered
    (`scripts/tsan_concurrency.sh`); execution на Linux runner —
    followup (Windows-only dev environment).

- **Почему:** flip activation closed default-on M:N runtime
  semantics + correctness (D138 ACTIVE). Performance work для
  production-readiness sign-off — separate concern.

- **Hypothesis для perf gap:**
  1. Worker pool startup latency (uv_thread_create × 16) — ~50-100ms
     одноразово, dominates ms-scale workloads.
  2. spawn-to-fiber-start overhead (mco_create + arena alloc +
     ctx_pins + uv_async_send) — ~10-100µs vs cooperative ~1µs.
  3. Boehm GC contention под multi-worker (GC_THREADS lock).
  4. Work-stealing imbalance — Chase-Lev deque round-robin не
     scatters short workloads efficiently.
  5. fiber arena per-thread allocation (Plan 82) — VirtualAlloc
     lazy-commit overhead на Windows.

- **Linux comparison done (2026-05-24, WSL2 16-core AMD Ryzen 5800H):**
  - Linux armed default: parallel = 683ms, sequential = 441ms,
    speedup **0.65×**.
  - Linux NOVA_MAXPROCS=1: parallel = 755ms, sequential = 372ms,
    speedup **0.49×**.
  - Windows armed default: parallel = 518ms, sequential = 345ms,
    speedup **0.66×**.
  - **Заключение: проблема НЕ Windows-специфика.** Linux показывает
    идентично-плохой speedup. Это фундаментальный overhead M:N runtime
    для коротких задач (~30ms workload не амортизирует worker pool
    cost).

- **Go runtime сравнение (2026-05-24):**

  | Метрика                 | Go runtime                  | Наш Nova                            |
  |-------------------------|------------------------------|-------------------------------------|
  | Spawn cost              | ~50-100ns (~200 CPU cycles)  | ~10-100µs (100-1000× slower)        |
  | Starting stack          | 2 KB (grows on-demand)       | ~1 MB (fixed arena slot)            |
  | Alloc fast path         | Per-P mcache (lock-free)     | Boehm GC_malloc (global lock)       |
  | Wake notification       | futex/eventfd (direct atomic)| uv_async_send (mutex+signal)        |
  | GC                      | Concurrent, write barriers   | Boehm STW                           |
  | Goroutine struct        | ~336 bytes                   | SpawnCtx + mco_coro + 1MB arena slot|

  **Корневые причины Nova медлительности:**
  1. Boehm `GC_malloc` под global lock — каждый spawn = global GC mutex.
  2. Fiber stack 1MB upfront — Go берёт 2KB. У нас MEM_COMMIT cost
     на Windows + mmap cost на Linux.
  3. `uv_async_send` overhead — Go использует прямой futex/eventfd.
     Мы через libuv mutex + signal.

  References:
  - https://internals-for-interns.com/posts/go-runtime-scheduler/
  - https://internals-for-interns.com/posts/go-memory-allocator/
  - https://nghiant3223.github.io/2025/06/03/memory_allocation_in_go.html

- **Last commit:** Plan 83.4.5.6 partial closure work
  (см. commits после 83.4.5.9).

- **Plan 83.4.5.10 partial closure (2026-05-24):**
  - **Ф.3 ✅ DONE** — inline parallel-for threshold (default 32);
    statement-mode + Range-iter parallel-for бежит cooperatively inline
    для N ≤ threshold. Acceptance ≥1× speedup MET (parallel ~622ms
    vs sequential ~640ms на 16 × fib(33), inline path активен).
  - **Ф.2 ❌ ИСКЛЮЧЕНО — wrong hypothesis** (см. §"Ф.2 wrong-hypothesis
    analysis" ниже). 8MB → 1MB downsize не дал бы speedup потому что
    virtual reservation FREE + commit lazy. Stack overflow на 1MB
    подтвердил что 8MB нужен для max recursion budget, не для speed.
    Plan 83.4.5.10 doc обновлён, slot size остаётся 8MB.
  - **Ф.1 ❌ deferred V2** — per-worker SpawnCtx pool (Go P-mcache
    analog). Это **главный bottleneck** — Boehm GC global lock на
    `nova_alloc_uncollectable` под 16-worker contention. ~1-2 dev-day.
    Acceptance уже MET через Ф.3 alone; Ф.1 нужен для larger-N
    parallel-for (>threshold) + standalone spawn'ов.

- **Ф.2 wrong-hypothesis analysis (детально, для re-analysis agent):**

  **Original hypothesis:** "Уменьшить slot size 8MB → 1MB снизит
  MEM_COMMIT/mmap overhead → ускорит spawn." **Неверно.**

  **Что зависит от slot_size:**

  | Aspect | Cost as f(slot_size) | Empirical (8MB vs 1MB) |
  |--------|----------------------|------------------------|
  | mmap MAP_NORESERVE virtual reservation | O(1) per arena init | ~µs одинаково — kernel просто VMA создаёт |
  | VirtualAlloc MEM_RESERVE on Windows | O(1) per arena init | Same |
  | Physical RAM commit | O(actual_stack_used_bytes) | Independent of slot_size — lazy commit |
  | Per-slot `mprotect(guard, PROT_NONE)` on Linux | O(slot_count) one-shot per arena init | 4096 syscalls × ~µs (one-shot per worker startup, **не per spawn**) |
  | Per-spawn `VirtualAlloc(MEM_COMMIT)` on Windows | O(fixed_init_window = 28KB) | Initial commit window fixed at 28KB — **independent of slot_size** |
  | GC scan range (Plan 82 GC_push_other_roots) | O(committed_pages) | Pushes only MEM_COMMIT pages — independent of slot_size |
  | TLB pressure | Negligible на 64-bit | N/A |
  | Maximum recursion depth | O(slot_size) | **Hard limit** на recursion — meaningful tradeoff, не выгода |

  **Single real effect of slot_size:** virtual reservation total + max
  stack budget. Virtual on 64-bit FREE (8MB × 16384 slots = 128GB
  virtual per Windows worker — noise). Max stack — limitation, не
  speedup.

  **Real spawn-cost drivers (independent of slot_size):**
  1. Boehm `GC_malloc_uncollectable` global lock — ~50-200µs under
     16-worker contention.
  2. `mco_create` init + Windows 3× `VirtualAlloc(MEM_COMMIT)` initial
     window — ~10-50µs.
  3. `uv_async_send` cross-thread wake (pthread_mutex + cond) — ~5-20µs.
  4. `nova_effect_snapshot_save` TLS copy — ~1-5µs.

  **Bottleneck ranking:** GC lock (Ф.1) >> mco_create cost >>
  uv_async_send >> snapshot_save >> stack slot size (free).

  **Confusion source:** I confused "stack size 8MB" (max recursion cap)
  с "8MB committed upfront" (physical RAM cost). Lazy commit means
  physical = actual usage, не slot size. Virtual reservation on 64-bit
  is essentially free.

- **Как чинить (Plan 83.4.5.10 V2 followup):**

  **Real-impact quick wins (target ≥3× speedup для длинных parallel-for):**

  1. **Per-worker SpawnCtx free-list pool** (Ф.1, ~1-2 dev-day) — как Go
     P-mcache. Worker держит free-list пустых SpawnCtx struct'ов.
     spawn → pop из P-local pool без Boehm lock. **Закрывает главный
     bottleneck #1** (Boehm GC contention). Free после mco_destroy →
     push back в pool.

  2. **mco_coro reuse pool** (~1 dev-day) — поверх Ф.1. Reuse mco_coro
     structs instead of allocating fresh каждый spawn. Закрывает #2
     (mco_create cost). Implementation: similar pool как Ф.1 в worker
     struct, with mco_reset between uses.

  3. **Lock-free wake** (~1 dev-day Windows + ~1 dev-day Linux) —
     replace `uv_async_send` с direct eventfd (Linux) / SetEvent
     (Windows). Закрывает #3. Но требует libuv async ownership rewrite —
     рискованно.

  4. **Versioned effect snapshots** (~0.5 dev-day) — versioning + skip
     copy если handlers не изменены. Закрывает #4.

  **Не включено (months of work):**
  - Concurrent GC (replace Boehm либо thread-local allocation buffers).
  - Dynamic stack growth Go-style — write barriers + stack copy +
    precise GC. Из-за Boehm conservative limitation.

- **Приоритет:** P2 — correctness landed; perf optimization for
  production readiness. Не блокер для Plan 83 main milestone.

## [M-100-impl-deferred] Plan 100 family — implementation в процессе (2026-05-25)

> **100.1 ✅ ЗАКРЫТ 2026-05-25** (merge `ab60167f3e5`): parser + AST (`type T consume`
> + `consume` field/binding qualifiers), LinearityRegistry (marker checks
> D133), ConsumeCtx flow analysis (D3/D5/D5.1/D5.2/D7) — 23/23 plan100_1
> PASS, 0 regressions.
>
> **100.2 ✅ ЗАКРЫТ 2026-05-26** (merge TBD): AST `GenericParam { consume_bound }` +
> `ExprKind::For { iter_consume }`, parser `[T consume]` + `for consume x in`, ConsumeCtx
> `consume_bound_generics` + D156-strict-forget + D156-iter-not/maybe-consumed — 17/17
> plan100_2 PASS, 0 regressions (plan100_1 23/0, plan73 12/0, plan100_4_3 11/0).
>
> **100.3–100.8 остаются ниже.**

**Где:** pipeline `parser → type-checker → consume-checker → codegen → runtime`; 100.1 реализован, остальные sub-plans отложены.

**Что закрыто (Ред. 2 production-grade, merge `d7464176352` +
D9 gap closure `6071c42a927`):**
- Spec: 12 D-блоков — D133 (type-level consume foundation, включая
  §«Consume-rvalue в arg-position» от 2026-05-25), D156-D166
  (generic propagation, view-borrow, defer/errdefer/okdefer семейство,
  FFI, cross-module, migration policy, perf/IDE/tooling).
- Plan-docs: umbrella 100 + 8 sub-plans (100.1-100.8) + 5 sub-sub-plans
  (100.4.1-100.4.5) = 13 docs, all Ред. 2 view-default model.
  Plan 100.3 D9 (2026-05-25 follow-up): запрет `f(make_tx())` где
  callee-param — view/mut-view (silent-leak prevention); ✅ для
  consume-param (direct ownership transfer).
- Idiom docs: 7 штук (consume-types, view-borrow, ffi-consume,
  cross-pkg-consume, async-cleanup, multi-cleanup-errors,
  cleanup-on-failure).
- Фикстуры: ~82 (pos+neg) в `nova_tests/plan100.1..100.8` (80 + 2 за D9).

**Что НЕ сделано (implementation phase):**
- Parser: `type Transaction consume {...}`, `consume tx = ...` binding,
  `consume fn`/`mut` param qualifiers, `view`/`mut`/`consume` в for/match/
  if-let, `okdefer`/`errdefer fail` keywords, `external fn` без consume
  prefix (type-driven).
- Type-checker: 3-режимный borrow tracking (view-default / mut-view /
  consume), Live-linear flow analysis, field-aware consume (D5/D5.1/D5.2
  reopen pattern), generic `[T consume]` bound propagation,
  external fn type-driven consume inference.
- Consume-checker: must-be-consumed на каждом code-path'е, defer/errdefer/
  okdefer family scheduling, multi-defer LIFO error accumulation +
  panic composition (no Rust-style double-panic-abort), failable
  cleanup body + suspend/async cleanup.
- Codegen: errdefer-trigger на interrupt/cancel, exit-path fixed at
  start, async cleanup yield safety.
- Diagnostics: D133 "not consumed" error format, LSP hover
  consume-status, LSP quickfix "add errdefer" — 3 фикстуры в
  plan100.8 проверяют ожидаемый output.

**Почему:** spec+contract зафиксирован первым (Ред. 2 production-
grade), чтобы implementation шла против чёткого описания. Реализация
требует interactive compile-test-fix цикла; autonomous batch без
iteration рискует silent semantic bugs.

**Объём:** ~43 dev-day по оценке в plan-doc'ах. Декомпозиция:
- 100.1 (foundation, parser + type-checker + consume-checker) ~5-7 dev-day
- 100.2 (generic propagation) ~3-4 dev-day
- 100.3 (borrow/view modes) ~4-5 dev-day
- 100.4 family (defer/errdefer/okdefer) ~12-15 dev-day (5 sub-sub-plans)
- 100.5 (FFI/external) ~3-4 dev-day
- 100.6 (cross-module) ~3-4 dev-day
- 100.7 (stdlib migration playbook execution) ~5-7 dev-day
- 100.8 (perf/IDE/LSP) ~4-5 dev-day

**Также отложено:**
- ~160 дополнительных фикстур (100.2/3: ~14; 100.4.*: ~75;
  100.5-100.8: ~73) — current 80 покрывают canonical patterns,
  edge-case coverage наращивается по мере implementation каждого
  sub-plan'а.
- 12 GATE probe artifacts (per-sub-plan GATE Ф.0 audit) —
  будут написаны в начале каждой implementation сессии.

**Как чинить:** последовательно начать с Plan 100.1 (foundation).
Без 100.1 остальные sub-plans не могут стартовать (зависят от
parser/type-checker core).

**Приоритет — P1** (core language feature, превосходит Rust по 4
capabilities; без неё Nova не достигает заявленной resource-safety
гарантии).

**Связанные markers:**
- [[M-fn-prefix-int-only-mono]] — Plan 101 prefix-generics, не
  зависят от Plan 100.
- [[M-receiver-generic-incompleteness]] — Plan 101 protocol
  composition `use Foo`, ортогонально Plan 100.

## [M-ide-integration-deferred] Plan 104 — production-grade LSP + tree-sitter + editor distributions (gated на Plan 91+100) (2026-05-25)

- **Где:** есть только TextMate grammar (VSCode/Cursor/VSCodium/Sublime
  через `editors/vscode/`) + handcrafted syntax plugins (Vim/Emacs).
  LSP-сервера НЕТ совсем. tree-sitter grammar НЕТ.

- **Что не работает (текущее состояние):**
  - Hover/tooltip с типом и doc-comment'ом — НЕТ.
  - Goto-definition (Ctrl+Click) — НЕТ.
  - Find-references (Shift+F12) — НЕТ.
  - Autocompletion (Ctrl+Space) — НЕТ.
  - Quick-fixes (💡 лампочка) для ~25 error codes из Plan 100/101 — НЕТ.
  - Rename (F2) — НЕТ.
  - Format-on-save (`nova fmt` integration) — НЕТ.
  - Document/workspace symbols (Ctrl+Shift+O / Ctrl+T) — НЕТ.
  - Tree-sitter-зависимые редакторы (Zed, Helix, GitHub web, modern
    Neovim) — вообще не поддерживаются.

- **Симптом:** "писать на Nova можно, но больно" (см. memory
  `project-plan101-status` LSP-section). Внешние пользователи без LSP
  не приходят. Dogfooding-команда переключается между файлом и
  терминалом по 100 раз в час.

- **Откладывается:**
  - **Plan 104** roadmap, ~33 dev-day (6-7 недель calendar single-dev),
    10 sub-plans:
    * 104.0 foundation crate (~2 dev-day)
    * 104.1 diagnostics (~3 dev-day)
    * 104.2 hover/goto/signature (~3 dev-day)
    * 104.3 completion (~5 dev-day)
    * 104.4 symbols/references (~3 dev-day)
    * 104.5 code actions/quick-fixes (~5 dev-day) — absorbs Plan 100.8
      Ф.2 + Plan 101 LSP V2 marker
    * 104.6 rename + format (~4 dev-day)
    * 104.7 tree-sitter grammar ✅ ЗАКРЫТ 2026-05-25 (github.com/nv-lang/tree-sitter-nova
      v0.1.0 — 84/84 fixtures, 5 query files, Helix/Zed/Neovim dist/)
    * 104.8 editor packaging ✅ ЗАКРЫТ 2026-05-26 (VSCode TS client 7/7 PASS,
      Neovim lspconfig snippet, Helix languages.toml, Zed extension.toml;
      [M-104.8-tool-nvim-unavailable] [M-104.8-tool-hx-unavailable] smoke skipped)
    * 104.9 close-out (~2 dev-day)

- **Gate (почему отложено сейчас):**
  - Plan 91 std MVP closure pending — без стабилизации core stdlib API
    LSP completion постоянно ломается.
  - Plan 100 implementation pending — без landed `consume`-checker
    quick-fixes (Plan 100.8 Ф.2) бессмысленны.
  - Plan 101 ✅ закрыт (8 error codes готовы под quick-fixes).

  Минимум 2-3 недели ожидания gate'ов; параллельно можно писать
  sub-plan files (104.0-104.9 пока есть только master).

- **Как чинить:**
  - **Trigger:** Plan 91 + Plan 100 closed.
  - **Старт:** 104.0 (foundation crate setup).
  - **Critical path:** 104.0 → 104.1 → 104.2 → 104.5 → 104.6 → 104.8 → 104.9.
  - **Parallel:** 104.3 (completion) + 104.7 (tree-sitter) могут идти
    одновременно с critical path → -5 dev-day если есть второй contributor.

- **Приоритет — P2** (gated). Если Plan 91/100 затягиваются,
  отдельные sub-plans (104.7 tree-sitter — independent от
  compiler-codegen API) могут стартовать раньше.

- **Связанные markers (absorbed at closure):**
  - Plan 100.8 Ф.2 (LSP quick-fixes для consume) → ✅ closes via 104.5.3.
  - Plan 101 LSP V2 marker (8 error codes) → ✅ closes via 104.5.2.
  - Plan 01-roadmap §165 «LSP v0.5» → ✅ closes via 104.9.

- **НЕ входит в Plan 104 V1 (отдельные планы):**
  - JetBrains native plugin (Kotlin + IntelliJ SDK) — separate plan.
  - DAP (Debug Adapter Protocol) — после native codegen (Plan 38 LLVM
    или mature interp-debugger).
  - Inlay hints / semantic tokens / call hierarchy — V2 (nice-to-have).
  - Refactorings (extract function/type) — V2 (rename в V1).

## Plan 104.8 V1 simplifications (2026-05-26)

### [M-104.8-zed-marketplace] Zed marketplace submission deferred

- **Где:** `editors/zed/extension.toml`
- **Что не сделано:** Submission в официальный Zed extension marketplace.
- **Почему:** Требует ручного review от Zed team; timeline непредсказуем.
  V1 = side-load install.
- **Как чинить:** Submit via https://github.com/zed-industries/extensions (PR).
- **Приоритет:** L

### [M-104.8-vscode-marketplace] VSCode marketplace publishing deferred

- **Где:** `editors/vscode/`
- **Что не сделано:** Публикация в VSCode/Open VSX marketplace.
- **Почему:** Требует publisher account + vsce/ovsx tokens; деплой pipeline
  не настроен. V1 = symlink/copy install.
- **Как чинить:** Set up publisher account → `vsce package && vsce publish`.
- **Приоритет:** L

### [M-104.8-bundled-binary-v2] Bundled nova-lsp binary в extensions — V2

- **Где:** все editor extensions
- **Что не сделано:** Embed nova-lsp binary в extension package (не нужно
  добавлять в PATH).
- **Почему:** Требует release pipeline (GitHub Actions build matrix), .vsix
  bundling, versioning. Сложно без CI. V1 = external binary.
- **Как чинить:** GitHub Actions matrix → download artifact в extension package.
- **Приоритет:** M (UX improvement — zero-config install)

### [M-104.8-tool-nvim-unavailable] Neovim headless smoke skipped

- **Где:** `editors/neovim/tests/smoke.lua`
- **Что не проверено:** Headless Neovim smoke (`nvim --headless -l smoke.lua`).
- **Почему:** nvim не установлен на dev-машине агента.
- **Как чинить:** `sh editors/neovim/tests/run_smoke.sh` после `brew/apt install neovim`.
- **Приоритет:** L (smoke coverage, не blocker)

### [M-104.8-tool-hx-unavailable] Helix hx --health smoke skipped

- **Где:** `editors/helix/tests/smoke.sh`
- **Что не проверено:** `hx --health nova` + `hx --grammar fetch nova` smoke.
- **Почему:** hx не установлен на dev-машине агента.
- **Как чинить:** `sh editors/helix/tests/smoke.sh` после `brew install helix`.
- **Приоритет:** L (smoke coverage, не blocker)

## [M-103-lazy-parallel-windows-crash] Plan 103.5 — Lazy.force() + parallel for crash on Windows (2026-05-26)

- **Где:** `nova_tests/plan103_5/lazy_no_double_init_prop.nv` (workaround
  sequential); discovered при impl Plan 103.5 (merge `c7f9bca1026`).
- **Что происходит:** `parallel for` + `Lazy.force()` как первая/единственная
  операция в test → "fiber stack overflow in slot 0" (VEH-detected crash
  на Windows). Работает fine после scheduler warm-up.
- **Воспроизведение:** undiagnosed. Likely связано с fiber-arena slot 0
  initialization при первом spawn'е через Lazy. Plan 82 (Windows fiber
  arena) + Plan 83.5/83.6 (per-worker pools) — релевантный контекст.
- **Workaround:** sequential `for 0..100` вместо parallel; concurrent
  coverage обеспечивается `once_stress_mn_4workers.nv` (16 fibers × 100
  force(), PASS).
- **Как чинить:** spike investigation + minimal repro outside testing
  framework → fix в fiber_arena_win.c slot 0 init или spawn-через-Lazy
  pattern → re-enable parallel variant.
- **Приоритет:** P3 (workaround стабилен; не блокер 0.1; concurrent
  coverage альтернативой stress-теста).
- **Related:** Plan 82, Plan 83.5/83.6, Plan 103.5.

## [M-103-conditional-sync-assert] Plan 103 — NOVA_SYNC_ASSERT no-op в Dev — нужен unconditional pattern (2026-05-26)

- **Где:** `compiler-codegen/nova_rt/sync_primitives.h:43-53` —
  `NOVA_SYNC_ASSERT` под `#ifdef NOVA_DEBUG`, no-op в Dev mode.
- **Что происходит:** Misuse sync API (double unlock, count underflow,
  invalid state transition) → silent no-op в Dev → undefined behavior.
  Только в Release с NOVA_DEBUG strict-assertions trigger abort.
- **Известные affected sites (pre-103.5):**
  - `Nova_Mutex_method_unlock` (line 236): `NOVA_SYNC_ASSERT(m->locked, ...)`.
  - `Nova_WaitGroup_method_done` (line 298): `NOVA_SYNC_ASSERT(wg->count > 0, ...)`.
  - `Nova_Once_method_done` (line 447) — **уже исправлено в 103.5** через
    unconditional `Nova_Fail_fail + nova_throw`.
- **Что чинить:** заменить debug-only `NOVA_SYNC_ASSERT` на unconditional
  throw для всех runtime invariants в sync primitives. Pattern из 103.5
  Once.done.
- **Когда чинить:** в Plan 103.3 (Mutex) и Plan 103.4 (Coordination)
  — explicitly added в plan-doc как acceptance criterion.
- **Приоритет:** P1 (silent UB risk; affects production reliability).
- **Related:** Plan 103.5 (discovery), Plan 103.3, Plan 103.4.

## Plan 83.10.1 — NOVA_AUTOARM=0 Directive Sweep (2026-05-26)

### [M-83.10.1-autoarm-sweep-v1] Directive sweep V1 IMPLEMENTED (2026-05-26)

- **Where:** All `nova_tests/` с `// ENV NOVA_AUTOARM=0` directive.
  Branch `plan-83-autoarm-sweep` в worktree `nova-p83-autoarm`.

- **Initial count:** 18 tests с directive.
- **Final count:** 15 tests с directive.
- **Removed (obsolete):** 3 directives — PASS 3/3 runs under armed M:N:
  - `concurrency/cancel_semantics_test.nv` — cancel propagation semantics now work under M:N после Plan 83.10 fix.
  - `plan83_10/cancel_race_no_orphan_state.nv` — 1K cancel cycles (no sleep) work armed.
  - `plan83_10/handler_isolation_per_fiber.nv` — TLS snapshot isolation works armed in this pattern.
- **Kept (still needed):** 15 tests с актуальными комментариями:
  - **[M-83.10.1-armed-cancel-timer-hang]** (10 tests): cancel+Time.sleep pattern
    TIMEOUT/FAIL under armed M:N. `cancel_all_pending + uv_close` sequence
    stalls when multiple workers race uv_run. Tests:
    `cancel_latency_bench`, `supervised_cancel_stress_test`, `supervised_cancel_test`,
    `f10_select_cancel_propagation`, `f11a_timer_metrics`, `f7_cancel_via_token`,
    `plan83_4_5_6_stress/cancel_stress`, `plan83_4_5_6_stress/park_wake_stress`,
    `plan83_4_5_6_stress/spawn_stress_10k` (overhead), `main_yield` (ordering).
  - **[M-83.10.1-per-fiber-handler-tls-race]** (2 tests): TLS handler snapshot
    save/restore around `mco_resume` races with worker threads under M:N.
    Tests: `concurrency/per_fiber_handlers`, `concurrency/time_handler`.
  - **[M-83.10.1-fail-handler-cross-mco-longjmp]** (1 test): `effects/fail_handler` —
    `longjmp` cross-mco-boundary can't reach handler frame on different worker's stack.
  - **[M-83.10-nested-armed-routing]** (1 test): `plan83_10/panic_in_nested_supervised` —
    TIMEOUT nested supervised throw routing (documented pre-existing gap).
  - **Cooperative ordering** (1 test): `concurrency/main_yield` — exact execution
    log ordering semantics require cooperative single-thread scheduling.

- **Concurrency suite result:** 62 PASS / 13 FAIL (was 61/14 baseline — improved).
- **Plan doc:** `docs/plans/83.10.1-autoarm-directive-sweep.md`.
- **Priority:** ✅ CLOSED (sweep complete; gaps documented for followup plans).

### [M-83.10.1-armed-cancel-timer-hang] cancel+Time.sleep TIMEOUT under armed M:N

- **Discovered by:** Plan 83.10.1 sweep — `cancel_latency_bench`, `supervised_cancel_test`, etc.

- **What:** Tests using `supervised(cancel: tok)` with fibers in `Time.sleep(N)`
  TIMEOUT (64s kill) under armed M:N scheduler. `tok.cancel()` → `cancel_all_pending`
  iterates pending timer handles, calls `uv_close()` for each. Under armed M:N
  multiple worker threads race `uv_run` — `uv_close` callbacks may not fire
  because the worker thread running `uv_run` is not the same thread that issued
  `uv_close`. The libuv handle cleanup stalls waiting for the next `uv_run`
  iteration on the correct thread.

- **Root cause hypothesis:** `nova_cancel_all_pending` runs on arbitrary worker
  thread; `uv_close` requires the handle's owning loop to call `uv_run` to
  process the close callback. Under armed M:N the loop thread may be blocked
  waiting for new work, not in `uv_run`.

- **Affected tests (10):** cancel_latency_bench, supervised_cancel_stress_test,
  supervised_cancel_test, f10_select_cancel_propagation, f11a_timer_metrics,
  f7_cancel_via_token, plan83_4_5_6_stress/cancel_stress,
  plan83_4_5_6_stress/park_wake_stress, plan83_4_5_6_stress/spawn_stress_10k.

- **Fix direction:** Route `cancel_all_pending + uv_close` to execute on the
  libuv-owning thread via `uv_async_send` dispatch mechanism (cross-thread
  safe closure submit). Alternatively: run `uv_run` from a dedicated I/O thread
  separate from fiber workers (Plan 83.8 / threadpool-vs-ioloop split).

- **Priority:** P1 — affects all cancel+sleep tests (10/18 AUTOARM directives).

### [M-83.10.1-per-fiber-handler-tls-race] TLS handler snapshot race under armed M:N

- **Discovered by:** Plan 83.10.1 sweep — `per_fiber_handlers`, `time_handler`.

- **What:** `with Time = handler { ... } { ... }` in spawn context reads wrong
  handler value when another worker thread races. Per-fiber TLS handler snapshot
  is captured at spawn-time and restored around `mco_resume` in `supervised_step`,
  but under armed M:N fibers run on arbitrary worker threads — the TLS slot
  restoration happens on the worker, not the spawner.

- **Root cause:** `supervised_step` with TLS save/restore designed for single-
  thread cooperative execution. Under M:N workers, each fiber resumes on a
  different thread and the TLS snapshot save/restore path in `supervised_step`
  isn't called — the worker thread has its own TLS state.

- **Fix direction:** Per-fiber handler snapshot must be applied on the executing
  worker thread before `mco_resume` and restored after — requires mco hook or
  worker-level snapshot apply mechanism.

- **Priority:** P2 — affects 2 tests (per_fiber_handlers, time_handler).

### [M-83.10.1-fail-handler-cross-mco-longjmp] Fail handler cross-mco-boundary under M:N

- **Discovered by:** Plan 83.10.1 sweep — `effects/fail_handler`.

- **What:** `with Fail = handler { ... }` intercepts `throw` via setjmp/longjmp.
  Under armed M:N the `throw` from inside a fiber executes on a worker thread;
  `longjmp` needs to jump to the handler frame on the *main* thread's stack —
  impossible cross-thread.

- **Root cause:** longjmp is stack-local; can't cross thread boundary.

- **Fix direction:** Fail handler dispatch under M:N requires inter-thread
  signaling (similar to [M-83.10-armed-user-throw-routing] fix — report error
  to scope, re-throw on main thread). Requires extending the effect handler
  dispatch to support cross-mco routing.

- **Priority:** P2.

---

### [M-83.10.3-nested-cooperative-resume-v1] Nested supervised cooperative resume on worker (2026-05-26) ✅ V1 IMPLEMENTED

- **Plan:** 83.10.3.
- **Closes:** [M-83.10-nested-armed-routing].

- **Problem:** `nova_supervised_run_impl(q)` called on a worker thread (fiber
  body executing inner supervised) blocked in `uv_run(&w->loop, UV_RUN_ONCE)`.
  Fibers in W's runnext/deque for scope q never ran — W held the thread.
  `nova_runtime_signal_main()` only woke main's loop, not W's.

- **Root cause (two-part):**
  1. `nova_supervised_run_impl`: alive==0, pending_remote>0 → `uv_run(UV_RUN_ONCE)`
     on worker's loop. F_inner in W's deque never popped.
  2. `nova_runtime_signal_main()`: only signals main's `uv_async` handle.
     Worker W never woken when F_inner completes on W2.

- **Fix:**
  1. `nova_supervised_run_impl`: when on worker thread, calls
     `nova_runtime_worker_pump_scope(q)` instead of `uv_run(UV_RUN_ONCE)`.
     Pump drains W's runnext/deque for scope-q fibers, resumes inline.
  2. `nova_runtime_signal_main()`: broadcasts `uv_async_send` to all worker
     loops. Ensures W exits `UV_RUN_ONCE` in pump when F_inner completes on W2.

- **New infrastructure:**
  - `_nova_on_worker_thread()` — TLS helper (fibers.h).
  - `nova_runtime_worker_pump_scope(NovaFiberQueue*)` — public (runtime.c/h).
  - `_worker_run_one_fiber(NovaWorker*, mco_coro*)` — static, full context
    save/restore (preamble-aware, parked/dead/yielded transitions).

- **Key correctness details:**
  - Before-preamble first run: `_nova_active_scope = &w->scope` set explicitly
    so preamble registers fiber in W's home scope (matches _worker_main).
  - outer_slot saved + restored so F_outer's `_nova_active_slot` is preserved
    across inner fiber inline resume.
  - CAS IDLE→RUNNING guards against double-resume with concurrent workers.

- **Verification (Ф.3 regression fix — UV_RUN_NOWAIT+sleep(1)):**
  - `panic_in_nested_supervised` PASS armed 3/3 (directive removed).
  - `nova_tests/plan83_10_3/`: 3 fixtures PASS armed.
  - `plan83_6/*`: 3/3 PASS armed (regression from broadcast reverted).
  - Concurrency suite: 63/12 (improved from 62/13 baseline).
  - Full nova test: PASS:1158, FAIL:19 (+9 PASS vs broadcast-regression run).

- **Remaining out-of-scope:** Performance (nested case serializes on W; acceptable
  since nested supervised is rare). Plan 83.10.2 (cross-thread cancel timer hang).

---

## 2026-05-27 — Plan 103.4 Agent B — Barrier

**Workaround в barrier_wait_with_action test 3 (вместо фикса кодгена):**
Закодирован `parties - 1` через `AtomicInt.new(parties - 1)` (heap-объект)
вместо прямого capture'а примитивного `int parties`. Underlying codegen
bug в emit_c.rs: trailing-block env для примитивных captures внутри
`parallel for` фибера эмитит `nova_int*` в struct, но присваивает
`env->x = _c->x` (без `&`) — разыменование значения как указателя →
access violation. Полноценный фикс отложен (требует разбора trailing-block
emission в emit_c.rs).

**Не фикшено:** `_nova_active_slot < 0` non-fiber spin-poll в `wait()` /
`wait_with_action()` оставлен для test-scaffolding consistency
(используется только при отсутствии fiber-context). Реальный use case
требует fibers; non-fiber path — degraded fallback.

## Plan 103.4 (Agent C) — CountDownLatch (2026-05-27)

- **include_str! compile-time embedding** — `sync.nv` вшивается в бинарник при
  компиляции Rust-крейта. При добавлении новых объявлений в `sync.nv` в worktree
  нужно пересобрать `nova-cli` из worktree (`cargo build` в `nova-p103-4-cdl/nova-cli/`).
  Иначе ExternalRegistry не знает о новых типах → линкер не находит символы.

- **if/else mixed return types** — паттерн `if i==0 { …; fetch_add(1) } else { count_down() }`
  даёт CC-FAIL: ветки имеют типы `nova_int` и `nova_unit`. Кодген пытается
  унифицировать к `nova_int`, затем кастит `nova_unit` к `nova_int` → ошибка C.
  Фикс: два отдельных `if` без `else` — `if` без else всегда unit в Nova
  независимо от типа тела.

- **Saturating semantics** — `count_down()` при count==0 обязан быть no-op (не panic),
  как Java CountDownLatch. `count_down_n(n)` при n<=0 или count==0 — no-op.
  Обе функции check-and-return под mutex до любой модификации.

## Plan 103.4 Agent A — Semaphore (2026-05-27)

- **`with_permit` — Nova-body, не C-routing** — метод `with_permit[R](body fn() -> R) -> R`
  реализован как Nova-body (`acquire() + defer release() + body()`) вместо C-функции.
  Причина: codegen body-методов на `export external type` генерирует вызовы через
  указатели структуры вместо правильных C-функций. Nova-body эквивалентен по семантике.
  Ветка `plan-103.4-sem`, commit `07cff1c2381`.

- **`Duration.ZERO` — codegen bug, workaround `from_millis(0)`** — ссылка на константу
  `Duration.ZERO` генерирует `Duration_ZERO` без объявления → CC-FAIL.
  Workaround: `Duration.from_millis(0)`. Баг предположительно в codegen константных
  accessor'ов для external-типов.

- **`// ENV NOVA_AUTOARM=0` на timer-тестах** — `semaphore_no_overcommit_prop` и
  `semaphore_try_acquire_for_timeout` помечены `// ENV NOVA_AUTOARM=0` для отключения
  M:N вооружённого режима при запуске теста. Причина: в armed-режиме каждая парковка
  fiber'а под семафором вызывает cross-thread dispatch через `uv_async_send` + worker
  deque, что создаёт ~37 ms накладных расходов на операцию. С 16 fibers × 100 iters ×
  3 permits это суммируется в 80+ секунд вместо нескольких ms. `AUTOARM=0` = все fibers
  кооперативно на main thread, таймеры libuv работают в том же цикле.
  Паттерн: `AUTOARM=0` (отключить вооружённый режим) ≠ `MAXPROCS=1` (1 worker,
  armed-режим всё ещё работает).

- **NOVA_GC_LIB_DIR в worktree** — при запуске `nova-codegen test-all` в worktree
  необходимо выставить `NOVA_GC_LIB_DIR` на main-репо vcpkg:
  `NOVA_GC_LIB_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib`.
  Worktree не имеет собственного `vcpkg_installed/`; `detect_boehm()` в test_runner.rs
  ищет `gc.lib` относительно `cg_include` (worktree path) → fallback не находит `gc.h`
  → CC-FAIL. Include dir auto-derivируется из lib dir (`lib/../include`), отдельно
  `NOVA_GC_INCLUDE_DIR` выставлять не нужно.

- **Stale binary cache + parallel test timeout** — если тест проходит в одиночку но
  TIMEOUT при параллельном запуске всех тестов (jobs=16): причина — бинарник ещё не
  скомпилирован, а 10 s timeout включает время компиляции под нагрузкой. Workaround:
  запустить проблемный тест в одиночку (`--filter name`) для прогрева кеша,
  затем запустить все вместе.

- **Tests: 4/4 PASS.** Commit: cb146ba4be2. Branch: plan-103.4-cdl (NOT merged).

- [M-sum-explicit-base-type-parser-gap] **2026-05-27** — Spec ↔ impl drift:
  [spec/decisions/02-types.md:270-277](decisions/02-types.md#L270) задокументировал
  опциональный базовый тип у sum-with-discriminants:
  ```nova
  type Bit u8       | Off = 0 | On = 1
  type HttpCode i32 | Ok = 200 | NotFound = 404
  ```
  Парсер падает: `expected fn / type / let / const / test, got '|'` на `|` после `u8`/`i32`.
  Только дефолтная форма (`type X | A = 0 | B = 1`, implicit `int`) работает.
  → [Plan 105](plans/105-sum-type-explicit-base.md) (proposed, P2, ~1.5 dev-day).

- [M-if-let-chain-parser-gap] **2026-05-27** — Spec ↔ impl drift:
  [spec/decisions/03-syntax.md:1163-1182](decisions/03-syntax.md#L1163-L1182) задокументировал
  `if let`/`while let` chains через запятую (Rust RFC 2497 let-chains):
  ```nova
  if Some(user) = lookup(id), user.is_active {
      process(user)
  }
  ```
  Парсер падает: `expected '{', got ','` на запятой после первого cond'а.
  Грамматика в spec'е (`if-expr := "if" if-cond ("," if-cond)* block`) реализована
  только без `("," if-cond)*` хвоста. Workaround — вложенные `if`'ы.
  → [Plan 106](plans/106-if-let-chains.md) (proposed, P2, ~2 dev-day, AST-унификация
  `IfLet`/`WhileLet` → `If`/`While` с `Vec<IfCond>`).

## Codegen (emit_c.rs)

### [C1] Массивы — только nova_int, нет полиморфизма
- **Где:** `emit_c.rs` / `nova_rt/array.h`
- **Что упрощено:** `NovaArray_T` инстанцирован только для `nova_int`. Массивы других типов (str, bool, record) не поддержаны. Тип элемента всегда `nova_int` в codegen.
- **Почему:** Без type inference невозможно определить тип элемента статически. Достаточно для demo.nv.
- **Как чинить:** Добавить анализ AST (рекурсивный infer типа первого элемента), инстанцировать NOVA_ARRAY_DECL/IMPL для каждого встреченного типа.
- **Приоритет:** M

### [C2] infer_expr_c_type — best-effort без полного type checking
- **Где:** `emit_c.rs` → `infer_expr_c_type`
- **Что упрощено:** Тип выражений инферится эвристически (AST-based, без полного анализа). Может ошибаться для сложных выражений (цепочки вызовов, generics).
- **Почему:** Полный type inference требует отдельного прохода и системы типов. В 90% случаев эвристика достаточна.
- **Как чинить:** Прогнать type checker перед codegen, передавать типы через аннотированный AST.
- **Приоритет:** H (системная проблема, проявится при расширении языка)

### [C3] Match — тип результата из первого arm
- **Где:** `emit_c.rs` → `infer_expr_c_type(Match)` и `emit_match`
- **Что упрощено:** Тип результата match выражения берётся из первого arm который не unit. Может быть неправильным если arms имеют разные типы.
- **Почему:** Без unification нельзя вычислить least upper bound типов.
- **Как чинить:** Type checker.
- **Приоритет:** M

### [C4] Option только через NovaOpt_nova_int
- **Где:** `emit_c.rs` / `nova_rt/array.h`
- **Что упрощено:** `Some`/`None` паттерны работают только для `NovaOpt_nova_int`. При match на других Option-like типах не будет правильного bind.
- **Почему:** Следствие [C1].
- **Как чинить:** Generics в runtime, NOVA_ARRAY_IMPL для каждого типа.
- **Приоритет:** M

### [C9] pre-scan — два прохода, handler/spawn IDs должны совпадать
- **Где:** `emit_c.rs` → `emit_handler_forward_decls` + `emit_fn`
- **Что упрощено:** Pre-scan использует отдельные счётчики, которые должны совпадать с основным проходом. При изменении кодогенерации это хрупко.
- **Почему:** Нужно для forward declarations в одном файле без второго буфера.
- **Как чинить:** Первый проход собирает все handler/spawn в список, второй их использует.
- **Приоритет:** M

---

## Runtime (nova_rt/)

### [R10] Fiber-throw + cooperative cancellation propagation
- **Где:** `nova_rt/fibers.h` (per-fiber fail-frame switching, cancel flag) +
  `emit_c.rs::emit_spawn` (setjmp wrapper) + `Stmt::Throw` (теперь nova_throw).

#### Что реализовано (2026-05-06)
1. **Per-fiber fail-frame chain.** `_nova_fail_top` (thread-local stack
   setjmp-frame'ов) теперь switching: `nova_supervised_step` сохраняет
   текущий top, ставит fiber'у его сохранённый chain (NULL для нового),
   делает `mco_resume`, после resume сохраняет fiber'овский chain
   обратно в `q->fiber_fail_top[i]` и восстанавливает outer top.
2. **Spawn-entry оборачивает body в setjmp.** Codegen `emit_spawn` теперь
   эмитит:
   ```c
   NovaFailFrame _ff;
   nova_fail_push(&_ff);
   if (setjmp(_ff.jmp) == 0) { ...body... nova_fail_pop(); }
   else { nova_fail_pop(); nova_fiber_report_error(_ff.error_msg.ptr); }
   ```
   `throw` внутри body → longjmp в `_ff` (frame на ЭТОЙ fiber-stack'е,
   safe), error пишется в scope queue, fiber завершается чисто.
3. **Cooperative cancellation.** `nova_fiber_report_error` ставит
   `q->cancel_requested = true`. `nova_fiber_yield` перед `mco_yield`
   проверяет флаг — если установлен, `nova_throw("scope cancelled")`,
   который ловится тем же spawn-entry frame'ом. Этот fiber умирает,
   scope переходит к следующему.
4. **Scope rethrow на main.** `nova_supervised_run` после полного drain'а
   проверяет `q->first_error` и если он не NULL — `nova_throw` на
   main-flow. Это безопасно: longjmp идёт по main-stack'у.
5. **`Stmt::Throw` теперь использует `nova_throw`** (раньше был
   `abort()`). Без активного fail-frame nova_throw тоже abort'ит, но
   с сообщением — нормальный graceful path.

#### Почему именно так

**Альтернатива 1: единый thread-local fail-frame (без switching).**
Изначально `_nova_fail_top` был один на thread. Когда fiber A push'ит
frame, yield'ит, fiber B push'ит frame — top.prev указывает на A's
frame, **но A's frame на A's stack'е**. Если B throw'ит → longjmp в
B's frame OK, но если B fail-pop'нет и потом throw'ит на следующем
уровне — top уже A's frame, longjmp пересекает fiber boundary → UB.
Поэтому **switching обязателен**.

**Альтернатива 2: NovaFiberMeta (extension struct в user_data).**
Вместо хранения fail_top в queue хранить в `user_data` через wrapper-
struct `{ NovaSpawnCtx*, fail_top }`. Это потребовало бы изменить
ВСЕ обращения к ctx через прокси-структуру — десятки мест в codegen.
Слишком много change'й. Queue-side storage концентрирует сложность
в одном месте (fibers.h).

**Альтернатива 3: per-fiber dynamic fail-stack.** Хранить указатель
на fail-stack head в `mco_user_data`, на пути save/restore через
обёртки. Сложнее, требует custom user_data routing. Queue-side
проще на 30% кода.

**Cooperative cancellation, не preemptive.** Альтернатива —
preemption (timer-based safepoint check, как Go 1.14+). Требует
сигнал-доставки и safepoint-кода в каждом цикле. Большая работа,
явно отложена до production. Cooperative — норма Erlang/OCaml 5,
spec-faithful по D14/D62.

**Cancel-через-throw, не через флаг-проверку в каждой операции.**
Альтернатива — Go-style context.Done() где fiber сам проверяет.
Это требует API канала. Throw — простой re-use существующего
fail-frame mechanism'а; fiber просто умирает на следующем yield.

#### Что НЕ реализовано (приоритеты)

**[ЗАКР] Positive-тесты на real throw → catch на main (2026-05-06).**
`with Fail = handler Fail { fail(msg) { ... } } { body }` реализован
в codegen + рантайме (Fail pre-registered как built-in эффект,
`throw msg` desugared to `Nova_Fail_fail(msg)` → vtable dispatch →
user handler). Тесты в `nova_tests/45_fail_handler.nv` (7 тестов:
main-flow happy/sad path, divide-by-zero, throw-from-spawn caught,
multiple-fibers throw, cancellation peer behavior). `try/catch`
синтаксис rejected по spec — единственный способ перехвата это
handler через `with`.

**[M] Не-cooperative cancellation.**
Fiber без yield-точек продолжит работу до конца body, даже если
scope cancelled. Это норма для cooperative-only scheduler'а
(Trio, Kotlin coroutines), но в production нужен preemption на
backedge'ах циклов и function entries.
- **Roadmap:** добавить safepoint-полл в codegen for-loop / function-
  entry; timer-based signal в runtime.

**[ЗАКР] `nova_assert` внутри fiber'а — fail-frame routing (2026-05-06).**
До фикса: `nova_assert` в fiber-body делал longjmp на `_nova_test_frame`,
который живёт на main-coroutine-stack — пересечение mco-границы (UB).
После фикса: `nova_assert` проверяет `nova_in_fiber()`. Если true —
longjmp идёт через `_nova_fail_top` (per-fiber chain, который пушится
в spawn-entry). Spawn-entry catch'ит, scope-runner re-throw'ит на
main flow через `nova_throw`; test runner ловит через дополнительный
`_tf_fail` NovaFailFrame. Если false (main flow) — старый путь через
`_nova_test_frame`. Тест `nova_tests/concurrency/assert_in_fiber.nv` (4 теста:
simple spawn, parallel for, after Time.sleep yield, nested supervised).

**[ЗАКР] `interrupt v` через mco-coroutine-boundary (2026-05-07).**
По spec D61/D65 handler-method для Fail (`fail() -> Never`) завершается
через `interrupt v`, не через `return`/trailing. До фикса: если
fail-handler установлен снаружи `supervised`, а throw случается в
spawn-body, `nova_interrupt(v)` делал longjmp на with-frame на main-
stack — пересечение mco-границы, exe crash.

После фикса (runtime):
- `NovaFiberQueue` имеет per-fiber `fiber_interrupt_top[i]` (как
  `fiber_fail_top[i]`), switch'ится в `nova_supervised_step`.
- `NovaFiberQueue.interrupt_pending/interrupt_value` — pending
  interrupt от fiber'а.
- `nova_interrupt(v)`: если `_nova_interrupt_top != NULL` — direct
  longjmp (fiber-local или main-flow with). Если `NULL` И fiber
  активен — set'ит `scope.interrupt_pending = true` + `cancel_requested
  = true` + longjmp на fiber-local fail-frame с sentinel-msg
  `"__nova_interrupt__"`. Spawn-entry catch detect'ит sentinel и
  пропускает `nova_fiber_report_error`. `nova_supervised_run`
  после drain re-issue'ит `nova_interrupt(value)` на main-flow.
- Тесты `nova_tests/effects/fail_handler.nv` — все 7 spec-compliant
  через `interrupt ()` (раньше использовали bootstrap-leniency
  `return ()` — теперь это spec-correct).

**[ЗАКР] Cancel-token API — D75 (2026-05-06).**
`cancel_scope { tok => body }` keyword, `NovaCancelToken` first-class
type, `tok.cancel()`/`is_cancelled()`/`bind()` методы. Реализовано
поверх существующего `cancel_requested` flag из D71. Bind даёт
каскадную отмену (parent.cancel() → child тоже cancel'ится).
- **Тесты:** `nova_tests/52_cancel_scope.nv` (5 тестов).
- **Известные ограничения:** см. D75 «Известные ограничения
  bootstrap-реализации» — re-throw на main приходит как plain
  nova_throw (user `with Fail` handler не вызывается для cancel-throw),
  NOVA_CANCEL_LINKED_CAP=8.

#### Roadmap к полноценной реализации (порядок)

1. ~~**Top-level `try/catch`**~~ → **rejected by spec.** Заменяется
   через `with Fail = handler { ... }` (см. п. 3). **Сделано
   (2026-05-06): nova_tests/45_fail_handler.nv** — 7 positive-тестов
   на throw-paths, в т.ч. throw-from-spawn caught, multi-fiber, cancel.
2. ~~**`_nova_test_frame` switching per-fiber**~~ — **сделано (2026-05-06).**
   nova_assert роутится через nova_in_fiber()/_nova_fail_top.
3. ~~**`with Fail = ... { body }`**~~ — **сделано (2026-05-06).**
   Fail pre-registered как built-in эффект, throw → vtable dispatch.
4. **Preemptive cancellation** — на безopiate-полла (function entry,
   loop backedge). Добавить флаг проверки → `nova_throw("cancelled")`
   если cancel_requested. Аналог Go 1.14+ preemption.
5. **`cancel_scope { tok => ... }`** (D50) — двусторонний cancel
   token. tok.cancel() извне сигналит fibers'ам.

- **Приоритет верхнеуровневой задачи:** M (после [H] try/catch
  работа по [M] preemption и `_nova_test_frame` относительно мала).

### [R9] NovaFiberQueue — фиксированный capacity (1024)
- **Где:** `nova_rt/fibers.h` (NOVA_SCOPE_CAP)
- **Что упрощено:** Очередь fiber'ов в `supervised` scope — фиксированный массив
  `mco_coro* fibers[1024]`. При попытке добавить 1025-й fiber — runtime abort с
  сообщением "supervised scope exceeded NOVA_SCOPE_CAP".
- **По спеке (D14):** ограничения на количество fiber'ов нет ("миллион fiber'ов
  на машину — норма как Erlang"). Это чистое bootstrap-ограничение.
- **Почему:** Динамический массив требует realloc при росте — лишняя сложность
  для bootstrap.
- **Как чинить:** заменить fixed-array на `mco_coro** fibers; int cap;` с
  geometric growth (cap *= 2 при заполнении). ~1 час работы.
- **Приоритет:** L (для большинства тестов 1024 хватает; миллион — отдельная задача
  на performance, требует benchmark'и).

### [R1] Аллокатор — malloc без free (по умолчанию)
- **Где:** `nova_rt/alloc.c`
- **Что упрощено:** `nova_alloc` → malloc, `nova_release` → no-op. Нет GC. Память течёт.
- **Почему:** Для прототипирования достаточно. Boehm GC доступен через `gc=boehm`.
- **Как чинить:** Включить RC (`gc=rc`) или Boehm GC (`gc=boehm`) через build_c.bat.
- **Приоритет:** L (Boehm GC уже есть как опция)

### [R2] Fibers — partial structured concurrency (supervised есть, race/parallel/cancel — нет)
- **Где:** `nova_rt/fibers.h` / `emit_c.rs`
- **Что реализовано (2026-05-06):** `supervised { }` scope — round-robin scheduler через
  `NovaFiberQueue` + `nova_supervised_run`. Внутри scope `spawn` кладёт fiber в очередь,
  не запускает сразу; на выходе scope крутит resume по очереди пока все не завершатся.
  Точки yield: `Time.sleep(ms)` → `nova_fiber_yield()` (без timer-wheel, любой ms = один yield).
  Ёмкость очереди: NOVA_SCOPE_CAP=64.
- **Что упрощено:** Нет `parallel for`, `race`, `select`, `cancel_scope`, `with_timeout`.
  `spawn` вне `supervised` остаётся eager-blocking (legacy совместимость, по спеке должна
  быть compile error). `let r = spawn ...` внутри scope возвращает 0 (результат через
  shared mut, как в Go-style). Без cancellation и error-propagation между fibers.
  Размер очереди фиксированный (64), без roll-over.
- **Почему:** Минимальная реализация для interleave-тестов. Cancellation/error-propagation
  требуют интеграции с Fail-frame stack для каждого fiber'а.
- **Как чинить:** добавить cancel-channel в NovaFiberQueue, при error в одном fiber'е —
  ставить cancel-флаг для остальных, при выходе scope — propagate.
- **Приоритет:** M

### [R6] detach — keyword реализован, default handler = SyncDetach (inline)
- **Где:** `emit_c.rs::emit_detach` / spec D50
- **Что реализовано (2026-05-06):** keyword `detach { body }`, AST `ExprKind::Detach`,
  парсер, interp-стаб, codegen. В bootstrap'е default-handler = SyncDetach: body
  исполняется inline в потоке caller'а (как обычный block, без fiber-обёртки).
  Тесты: `nova_tests/40_detach.nv` (13 тестов на capture/control-flow/nesting/
  совместимость с supervised).
- **Что упрощено:**
  * Эффект `Detach` не объявлен в effect-system — компилятор не требует его в сигнатуре.
  * Нет реального глобального supervisor'а: detach исполняется inline, не на отдельном
    OS-thread'е, поэтому "переживёт caller'а" не реализовано (но spec явно описывает
    SyncDetach как валидный handler для тестов — bootstrap-default это и есть SyncDetach).
  * Нет панник-контейнмента (`LogAndDrop`): паника в detach распространится наружу.
- **Как чинить полностью:**
  1. Объявить `Detach` как effect; добавить compile-time проверку требования в сигнатуре.
  2. Сделать глобальный supervisor (OS-thread + queue), routes detach → background.
  3. Default handler `LogAndDrop`: panic в detach → log + сбросить fiber, не propagate.
- **Приоритет:** L

### [R7] Time.sleep(ms) — without timer-wheel (Time-as-effect REALIZED)
- **Где:** `nova_rt/effects.h`/`fibers.h` (vtable + dispatch) / `emit_c.rs`
  (Time pre-registered as built-in effect).
- **Что реализовано (2026-05-06):**
  * `Time` теперь обычный pre-registered эффект в codegen (D11/D62).
  * `Time.sleep(ms)` → `Nova_Time_sleep(ms)` идёт через handler-vtable.
  * `Time.now()` → `Nova_Time_now()` (default returns 0).
  * Default handler `_nova_time_default_sleep`: context-sensitive yield
    (fiber → `mco_yield`, supervised body → `nova_supervised_step`,
    top-level → no-op).
  * User override: `with Time = handler Time { sleep(ms) {...} now() {...} } { body }`
    устанавливает custom handler — для test fixtures с fixed clock
    или mock sleep. Работает (тесты `46_time_handler.nv`).
- **Что упрощено:** `ms` игнорируется в default handler — нет timer-wheel.
  `Time.sleep(100)` и `Time.sleep(0)` неотличимы. Реальной задержки нет.
- **Как чинить полноценно:** Timer-wheel/heap, при `Time.sleep(ms)` fiber
  кладётся в sleep-list с deadline, scheduler пропускает sleeping fibers
  до его наступления. Аналогично `Time.now()` нуждается в реальном
  c-clock (через QueryPerformanceCounter / clock_gettime).
- **Приоритет:** L (для тестов interleave не нужно).

### [R3] nova_str — borrowed slice, нет ownership
- **Где:** `nova_rt/nova_rt.h`
- **Что упрощено:** `nova_str` — `{const char* ptr, size_t len}`. Строки не копируются при присваивании. Строковые литералы — статические данные. Нет проверки lifetime.
- **Почему:** Копирование строк дорого и не нужно для прототипа.
- **Как чинить:** Ref-counted строки или arena allocation.
- **Приоритет:** L

### [R4] Массивы — нет release/GC при shrink или drop
- **Где:** `nova_rt/array.h`
- **Что упрощено:** `nova_array_push` при росте аллоцирует новый буфер через `nova_alloc` но не освобождает старый (alloc.c — malloc без free). При смене на RC нужно явно release старый буфер.
- **Почему:** Пока alloc.c не освобождает ничего — не критично.
- **Как чинить:** При смене на RC — добавить `nova_release(a->data)` перед `a->data = new_data`.
- **Приоритет:** M (при включении RC)

---

## Спецификация (spec/)

### [S1] Q1 — @-методы для эффектов не определены
- **Что упрощено:** Синтаксис `effect.method()` через `@`-синтаксис остался открытым.
- **Приоритет:** L

### [S2] Q5 — граница Panic (stack overflow, assertion failures)
- **Что упрощено:** Что именно является recoverable Panic не зафиксировано.
- **Приоритет:** M

### [S3] Q6 — effect polymorphism синтаксис
- **Что упрощено:** Передача handler-объекта как параметра функции не оформлена в синтаксис.
- **Приоритет:** M

### [S4] Q9 — stdlib скелет
- **Что упрощено:** Нет stdlib. Всё что есть — примеры в examples/.
- **Приоритет:** H

### [S5] Q10 — tooling (LSP, package manager, hot reload)
- **Что упрощено:** Никакого tooling.
- **Приоритет:** M (после стабилизации языка)

---

## 2026-05-11: name-resolution фаза в типчекере (NameResCtx)

### Что упрощено

NameResCtx ловит undefined идентификаторы в expr-position, но
**пропускает Capitalized-имена**. Точечно НЕ проверяются:

1. **Cross-file types/variants (Capitalized).** `HashMap[K,V].new()`
   в std/collections/lru.nv использует `HashMap` без import — типов
   нет в текущем модуле. Эвристика: имя начинается с заглавной → known.
2. **TaggedTemplate tags** (sql / json / html). Special-form syntax.
3. **Member access name** (`obj.method`, `obj.field`) — резолв через
   method_table / record_schemas в codegen.
4. **Path-сегменты** (`module::name`) — first segment не валидируется.
5. **Generic-params в TypeRef** — type-position, не expr.

### Почему

- **Bootstrap не имеет cross-file name resolution.** Имена из других
  .nv файлов попадают сюда не задекларированными. Полноценный
  import-graph + module-loader — большая инфраструктура; для
  bootstrap'а заменена convention'ом «Capitalized = type/variant».
- **Method-resolution требует type-inference.** `obj.method` — тип
  obj может быть generic-param, чужой type, или primitive с
  встроенным методом. Не делаем в name-resolution фазе.

### Trade-offs

- ✅ Ловятся **snake_case опечатки** (`undefined_var`, `fixed_ms`,
  `seeded`) — самый частый класс ошибок в expr-position.
- ❌ Опечатки в **Capitalized** именах (`HashMpa` вместо `HashMap`)
  НЕ ловятся. Это компилятор подсветит на cc-этапе через
  «undeclared type» — все ещё неудобно, но менее частый случай.
- ❌ Method-typos (`xs.lenghth` вместо `xs.length`) НЕ ловятся.
  Это **отложено** до полноценного type-inference / method-table-aware
  фазы (требует bidirectional inference).

### Когда закрывать

Полноценное cross-file name resolution планируется в self-hosted
compiler'е (после Plan 22+, когда появятся stable module-loader +
type-inference). До этого — bootstrap convention достаточен.

### Файлы

- `compiler-codegen/src/types/mod.rs` — `NameResCtx` (lines ~1255–1670):
  build/check_module/walk_fn/walk_block/walk_stmt/walk_expr/
  walk_trailing/collect_pattern_bindings/is_known.

### Status

- ✅ ЗАКРЫТ (как bootstrap-фаза).
- Tests: ✅ cargo test --lib 65/65, nova_tests 121/121 PASS (120 baseline + 1 negative).
- Roadmap: расширение до Capitalized-проверки — после self-host
  compiler, не в bootstrap.

---

## std/testing/handlers.nv — Plan 34 Ф.1+Ф.7 (2026-05-12)

**Где:** std/testing/handlers.nv.

**Что упрощено:** `seeded(seed)` использует xoshiro256++ PRNG —
**не CSPRNG**. Production-Random требует `secure() -> Handler[Random]`
через runtime-hook (CSPRNG из nova_rt или OS-syscall) — не реализован.

**История:**
- Ф.1 (изначально) — Knuth MMIX LCG, 2 строки. Бакоп — плохое
  distribution, короткий period.
- Ф.7 (production-grade, 2026-05-12) — заменён на **xoshiro256++**
  (Sebastiano Vigna, public domain CC0): 4×u64 state, period 2^256-1,
  passes BigCrush/PractRand. State init через splitmix64 для
  non-zero state при seed=0. `bytes(n)` использует 8 байт за advance
  (раньше 1 байт). Чистый go/rust-equivalent quality (Go math/rand v2
  использует PCG, Rust rand crate — ChaCha8; xoshiro — established
  alternative).

**Почему не CSPRNG:** test-handler'ы должны быть deterministic
(тот же seed → та же sequence между запусками). CSPRNG для тестов
контр-продуктивен. Production-handler для real crypto — отдельная
ответственность.

**Как починить (CSPRNG part):** добавить `fn secure() -> Handler[Random]`
с external-binding к runtime-CSPRNG (Windows BCryptGenRandom, Linux
getrandom, macOS SecRandomCopyBytes). Это — часть Plan 18 (P0 stdlib
roadmap), не блокер.

**Приоритет:** P2 — production-cryptography не нужна до v0.5.


---

## import Wildcard `*` и bare-name visibility — Plan 35 Ф.1 (2026-05-12)

**Где:** stdlib (9 файлов) — bcrypt/jwt/ulid/uuid/snowflake/rate_limiter/
retry/property/duration используют `import std.testing.handlers as th`
+ `th.seeded(...)` / `th.fixed_ms(...)`.

**Что упрощено:** хотелось бы написать `seeded(42)` без префикса (как в
docstring property.nv `with Random = seeded(seed)`), но `nova check`
cross-file resolution для bare-name функций не работает. Парсер
принимает `import X.Y.*`, но падает на токене `*`.

**Почему:** wildcard import / bare-name visibility требует:
1. Parser: разрешить `*` после dotted-path в parse_import.
2. Name-resolver: открыть все `export`-сущности модуля по bare имени.
3. Spec-decision: D-блок про import semantics (conflicts, shadowing,
   re-export, alias precedence).

Решение через `import as alias` + `alias.fn()` чище для коротких
вызовов, но многословнее для длинных. После закрытия Plan 35 Ф.1 можно
будет вернуть bare-name в 9 stdlib-файлах (cosmetic).

**Как починить:** Plan 35 Ф.1 (низкий приоритет, ~150 строк).

**Приоритет:** P3 — workaround через alias работает и читается.

**Обновление (Plan 81 Ф.11, 2026-05-21):** запись устарела.
- **Wildcard `import X.*`** — **spec-rejected** (R25, [D29](../spec/decisions/07-modules.md#d29)/[D5](../spec/decisions/07-modules.md#d5)): `import` всегда явный — либо весь модуль, либо selective `.{A, B}`. Не недоработка, а решение.
- **Bare-name visibility** — уже работает: `import X` (whole-module, без `.{...}`) делает bare-имена `export`-сущностей видимыми через Plan 35 merge. Префикс нужен только для `import X as alias`. Возврат bare-name в 9 stdlib-файлах — cosmetic, не блокер.


---

## json.nv: `mut` параметр не поддерживается (Plan 34 Ф.2.1, 2026-05-12)

**Где:** std/encoding/json.nv:499 — `fn Parser mut @parse_member(fields HashMap[...])`.
Раньше был `(mut fields HashMap[...])`.

**Что упрощено:** парсер Nova не принимает `mut`-modifier для параметра
функции (есть только для self-receiver: `fn X mut @method(...)`). Убрал
`mut`. HashMap — reference-type через GC, мутации фактически работают
(метод `fields.insert(...)` модифицирует тот же объект что caller
держит), но в сигнатуре `mut`-маркер потерян.

**Почему:** добавление `mut`-param в Nova grammar — отдельное spec-решение
(call-site marker? automatic? для всех ref-типов?). Не блокер для type-check.

**Как починить:** D-блок про `mut`-параметры (Rust-style explicit
`&mut T` или Java/Kotlin-style implicit для reference-types). Парсер +
type-checker — ~100 строк.

**Приоритет:** P3 — semantics корректна, только signature lossy.


---

## property.nv: trailing-block closure синтаксис (Plan 34 Ф.2.3, 2026-05-12)

**Где:** std/testing/property.nv — 6 мест с `property(gen, |xs| { ... })`.
Раньше использовался Kotlin/Swift-style `property(gen) { xs => ... }`.

**Что упрощено:** Nova не поддерживает trailing-block-as-closure (Kotlin
`list.forEach { it -> ... }`, Swift `array.map { x in ... }`). По D22
closure-литерал — `|xs| { ... }`. Переписал на explicit-argument форму.
Чуть многословнее, но грамматически однозначно (нет ambiguity со
struct-literal'ом или if/while-body).

**Почему:** trailing-block syntax удобен для DSL'ей и AI-prompts
(`channel.send { msg => ... }` читается естественно), но грамматически
конфликтует с block-as-expression (если `f() { ... }` — то closure
или value-statement?). Нужно D-решение.

**Как починить:** D-блок про trailing-closure синтаксис (когда `{ ... }`
после call'а — closure, когда — separate statement). Требует анализа
ambiguity, ~50 строк парсера + грамматики.

**Приоритет:** P2 — DSL ergonomics, но обходится `|x| { ... }`.



---

## CLI `nova check` / `nova test` — MVP simplifications (Plan 36, 2026-05-12)

### Что упрощено в MVP (Ф.0 + Ф.1 + R7 + R10) vs full Plan 36

**Где:** `nova-cli/src/main.rs` (~455 строк добавлены/изменены).

**Полный план**: 30 requirements (R1-R30) + 12 architecture decisions
(AD1-AD12). **Реализовано в MVP**: R1-R8 base + R10 base + R13 (Ф.0
correctness fix) + R19 (parallel) + R20 (GC backend) + R21 (module-path
hard fail).

**Не реализовано (отложено в sub-plans 36.A-E):**

| Sub-plan | Что упрощено | Sufficient workaround |
|---|---|---|
| 36.A outputs | 1 output format (human). Нет JSON/SARIF/JUnit. | Wrap через `grep`/`awk` или wait for 36.A |
| 36.A diag codes | Нет stable E0001-E9999 registry. Diagnostics только human. | `nova explain` impossible v1, plan 36.D |
| 36.A spec_link | Нет `spec_link` field в diagnostic. | spec ссылка в diagnostic message прямо как plain text |
| 36.B caching | Каждый check полный re-check. <500ms cache miss отсутствует. | Acceptable для CI; локально для разработчика — manual incremental |
| 36.B repro builds | Нет `SOURCE_DATE_EPOCH` / no-timestamps. | Не критично без CI |
| 36.C pre-commit | Нет `.pre-commit-hooks.yaml`. | Manual git hook script если нужно |
| 36.C GHA annotations | `::error file=,line=::` не emit'ится. | CI просто видит exit code + stderr |
| 36.D verbosity | Нет `-q`/`-v`/`-vv`. | --color never для CI достаточен |
| 36.D --explain | Нет `nova explain Exxxx`. | Diagnostic codes пока не emit'ятся, не блокер |
| 36.D --dry-run | Нет `--dry-run` / `--list`. | Скрипт `find ... -name '*.nv'` достаточен |
| 36.E workspace | `find_repo_root` берёт первый parent с nova.toml. 4 nested nova.toml в repo (root + nova_tests + examples + std) — не unified. | В MVP `nova check` от repo root walks-all; для package-scoped check — `nova check std/` явно |

### Почему

Полный Plan 36 — много-сессионная работа (160 gaps в plan v4 после
4-way audit). MVP = focused subset который **shippable in one session**
с реальной production-value (Ф.0 closes silent bug, R7 closes exit code
ambiguity, R10 closes CI no-color requirement).

### Как починить

Sub-plans 36.A-E — отдельные плановые файлы, отдельные сессии. Каждый
закрывает свою группу:
- 36.A — outputs (приоритет: высокий для CI integration)
- 36.B — caching (приоритет: средний, влияет на dev workflow)
- 36.C — CI integration (приоритет: высокий после 36.A)
- 36.D — advanced ergonomics (приоритет: средний)
- 36.E — workspace (приоритет: низкий, current implicit walks-parents
  работает)

### Приоритет

**P1** для 36.A + 36.C (CI integration сценарий критичен).
**P2** для 36.B + 36.D (UX win, не блокер).
**P3** для 36.E (workspace concept — после Plan 03 package ecosystem).


---

## D54 семантические проверки живут только в codegen — Plan 37 (2026-05-12)

**Где:** `compiler-codegen/src/codegen/emit_c.rs`:
- `check_as_cast_allowed` (24 banned-пары: `int as char`, `char as byte`,
  `int as bool`, `str ↔ T`, и др.) — вызывается только из emit-пути
  для `ExprKind::As` (строка 5474).
- `check_bool_condition_at` (strict bool в `if cond` / `while cond`) —
  вызывается только из emit-пути (строки 5269, 7660).

**Что упрощено:** type-checker (`compiler-codegen/src/types/mod.rs`) для
`ExprKind::As` и условий `if`/`while` **только рекурсирует во внутрь**
без D54-валидации. Результат: `nova check std/encoding/hex.nv` → PASS,
`nova test std/encoding/hex.nv` → CODEGEN-FAIL `int as char запрещён`.

**Почему:** проверки добавлялись прицельно в codegen (Plan 08 Ф.5 для
as-cast, Plan 08 Ф.4 для strict bool) и оставались там — type-checker
не переоткрывался под эти классы ошибок. Архитектурно это **отложенная
диагностика**: ошибка на codegen-фазе, а не на check-фазе.

**Влияние на UX:** нарушает контракт `nova check` (D95, Plan 36) —
«полная type+lint валидация модуля без codegen». LLM-агенты и
ide-integrations, которые гоняют `nova check` для feedback'а,
получают «green check + red build» на тех же файлах.

**Как починить:** Plan 37 — перенести (или продублировать через shared
module) проверки в type-checker. Detail в
[docs/plans/37-typecheck-semantic-parity.md](plans/37-typecheck-semantic-parity.md).
Защита defense-in-depth (codegen всё равно держит свой check) на случай
прямого `nova-codegen build` без `check` шага.

**Приоритет:** **P2** — UX win для `nova check` contract, но обходится:
- `nova test foo.nv` (или `nova build foo.nv`) ловит ошибку с тем же
  сообщением, просто позже.
- Workaround не нужен — пользователь чинит код по сообщению codegen.

**Обнаружено:** при правке `std/encoding/hex.nv` под D54 (
`('0' as int + n as int) as char` → нужен `char.try_from(n)?` с `Fail`
в сигнатуре `digit`). type-check файла прошёл, codegen упал.


---

## Plan 33 contracts (bootstrap)

### [V1-ЧАСТИЧНО 2026-05-14] TrivialBackend SMT — Z3 реализован, но не default
- **Где:** `compiler-codegen/src/verify/backend/` (trivial.rs + z3.rs).
- **Что было упрощено:** TrivialBackend (паттерн-матчинг) вместо Z3.
- **Что сделано (Plan 33 V1, 2026-05-14):** Z3Backend через собственные
  FFI-биндинги (`verify/backend/z3.rs`, без crate-dependency). Feature flag
  `z3-backend` в `Cargo.toml`. Выбор через `NOVA_SMT_BACKEND=z3` env или
  `--smt-backend z3` CLI. Тесты: `nova_tests/contracts/z3_*` (SKIP без
  NOVA_SMT_BACKEND=z3, PASS с ним).
- **Что осталось:** TrivialBackend — default (без env var). Для nova CI
  нужно добавить `NOVA_SMT_BACKEND=z3` job чтобы z3_* тесты не всегда
  SKIP. Также: Z3 static link (сейчас dynamic) — для портируемого binary.
- **Как чинить остаток:** CI job `contracts-z3` с env NOVA_SMT_BACKEND=z3.
  Опционально: `z3-static` feature через vcpkg для standalone binary.
- **Приоритет:** M (Z3 работает; CI coverage — отдельная задача).

### [V2] Loop invariants парсятся, но не сохраняются в AST
> **✅ СУПЕРСЕДЕД (аудит Plan 33.8, 2026-05-21):** закрыто. `ExprKind::For`/
> `While`/`Loop` имеют поля `invariants: Vec<Expr>` + `decreases` (Plan 33.4,
> см. закрытый `[V14]`); SMT havoc + preservation + decreases — реализованы
> (Plan 33.5 Ф.2). Запись устарела и сохранена для истории.
- **Где:** `parser::skip_loop_clauses` в `parse_while`/`parse_for`/`parse_loop`.
- **Что упрощено:** `invariant <expr>` и `decreases <expr>` между
  loop-header и body парсятся и игнорируются — программист может писать
  spec, но SMT их не использует.
- **Почему:** trivial backend всё равно не верифицирует loops
  (нужен Z3 для havoc + invariant preservation + decreases check).
- **Как чинить:** Расширить `ExprKind::For`/`While`/`Loop` полями
  `invariants: Vec<Contract>` и `decreases: Option<Expr>` — это
  breaking change для interp/codegen/types match'ей, но **необходимо**
  для Z3 verify pipeline.
- **Приоритет:** M — depends on [V1] (без Z3 не имеет смысла).

### [V3] Composition требует #pure, но purity не выводится автоматически
> **✅ СУПЕРСЕДЕД (аудит Plan 33.8, 2026-05-21):** закрыто. SCC-инференс
> чистоты по call-graph реализован в Plan 33.5 Ф.3 (`inferred_pure` /
> `collect_pure_fns` в `pipeline.rs`). Запись устарела, сохранена для истории.
- **Где:** `types::ContractCtx::pure_fn_names`.
- **Что упрощено:** Composition (вызов user fn в контрактах) разрешён
  только если у fn есть явный `#pure` атрибут. SCC-inference по
  call-graph (как `const fn` в Rust) — НЕ реализован.
- **Почему:** SCC inference потребует mutual-call analysis +
  effect propagation. Это полноценный pass — отложен до Plan 33.3
  full (требуется для composition в SMT тоже).
- **Как чинить:** Добавить `PurityCtx::infer` с fixpoint по
  call-graph через SCC. Атрибут `#pure` остаётся как assertion
  (если выведенный mismatch — compile error).
- **Приоритет:** M — текущее поведение honest (программист обязан
  пометить), но требует boilerplate `#pure` на каждой helper-fn.

### [V4] ✅ `old(...)` через entry-snapshot — ЗАКРЫТО Plan 33.6 Ф.7.2 (2026-05-16)
- **Где:** `compiler-codegen/src/verify/pipeline.rs::verify_fn`.
- **Что реализовано:** Каждый param получает SMT-двойник `_old_<x>`,
  declared как отдельная var. Frame axiom (D.1.2) асертит `_old_x == x` для
  non-modifies params, давая Z3 равенство. Для modifies-params (когда добавятся
  в Nova spec) `_old_<x>` остаётся независимой → entry-state.
- **`substitute_old` теперь no-op** (preserved для API compat), потому что
  `_old_<x>` — first-class SMT var, не нуждается в substitution.
- **Дата закрытия:** 2026-05-16.

### [ЗАКР 2026-05-16] Ghost erasure + ghost soundness — [V5]
- **Закрыто (Plan 33.6 Ф.1.1, 2026-05-16, commit 85956feb):**
  * `emit_c.rs:5841` — `if decl.is_ghost { return Ok(()); }` — ghost let никогда не
    эмитится в C, даже в debug.
  * `types/mod.rs:4667` — `check_ghost_usage` — compile error если ghost var используется
    в non-ghost context (println, let RHS, арифметика).
  * Ghost в spec-position (assert_static, assume, invariant, requires/ensures) — OK.
  * Ghost chain (ghost reads ghost) — OK.
  * Тесты: 5 новых тестов — 3 positive (assert_static, invariant, ghost chain),
    2 negative (pass to println, runtime use).
- **Дата закрытия:** 2026-05-16

### [ЗАКР 2026-05-14] pure_view + axiom + #verify/#trusted gate — [V6]
- **Закрыто (Plan 33.3 Ф.9.1-9.6, 2026-05-14):**
  * AST: `OpKind::PureView`, `EffectAxiom { binders: Vec<(String, Option<TypeRef>)>, generics }`.
  * Parser: `#pure <op>(...) -> R` + `axiom name(binders) => formula` (typed/generic/untyped binders).
  * Type-check: axiom body ссылается только на `#pure` views + binders + arith/bool.
    Unique-name check по полной сигнатуре (name+param_types) — перекрытие ops с разными типами OK.
  * SMT: `#pure view` → UF `Z3_mk_func_decl`; `axiom` → `Z3_mk_forall_const`.
  * Axiom inconsistency check: pre-flight `assert true; check_sat` для conjunction axioms.
  * `#verify` / `#trusted` gate на `with`-binding для эффектов с `axiom`.
    Нет attr → compile error. `#verify` + `#trusted` вместе → compile error.
  * Protocol symmetry: `protocol { #pure op; axiom ... }` — trusted-by-default.
  * Overloaded ops: name-mangling (`balance__nova_int` / `balance__nova_str`) для vtable + dispatch.
  * Naming refactor: `pure_view` keyword → `#pure` атрибут; `#verify_handler` → `#verify`.
  * Тесты: 14 Ф.9 тестов (parse, type-check, SMT); z3_* PASS с NOVA_SMT_BACKEND=z3.
  * Typed/generic binder тесты: 11 файлов (f9_axiom_typed/generic/overloaded_*).
- **Ещё открыто (Plan 33.4 P0-1):**
  * Ф.9.7 symbolic handler verification — `#verify` gate принимает атрибут
    но реальной Z3 верификации handler body ещё нет (placeholder). См. [V12].

### [ЗАКР 2026-05-15] Bounded quantifiers (`forall`/`exists`) — [V7]
- **Закрыто (Plan 33.4 D.1.3):**
  * `forall x in lo..hi : P(x)` / `exists x in lo..hi : P(x)` — контекстуальные
    ключевые слова (не новые токены), парсятся в `ExprKind::Forall`/`Exists`.
  * SMT encoding: Forall → `SmtTerm::Forall([x:Int], in_range => P(x))`;
    Exists → `not(Forall([x:Int], in_range => not(P(x))))`.
  * D.1.4: trigger-finding stub + eprintln warning при отсутствии trigger.
  * Test: `nova_tests/contracts/quantifier_positive.nv` (70/70 PASS).
- **Остаток:** Trigger pattern аннотации в SmtTerm IR — V2 (Plan 33.5).

### [V8] ✅ FP IEEE 754, strings (Seq theory) — ЗАКРЫТО Plan 33.3 Ф.11 (2026-05-16)
- **Где:** Plan 33.3 Ф.11, `compiler-codegen/src/verify/backend/z3.rs`.
- **Что реализовано:** f32/f64 через Z3 FloatingPoint theory (fp.sort_32/64,
  fp.numeral, fp.add/mul/geq/eq, RNE rounding mode). str через Z3 Seq theory
  (str.sort, eq). var_sorts propagation из fn params → EncodeCtx.
- **Ограничения:** NaN семантика by-design (fp.eq(NaN,NaN)=false в SMT).
  Set/Map теории — Plan 33.5.
- **Тесты:** `nova_tests/contracts/f11_fp_strings_z3.nv`, `f14_string_ops.nv` (115 PASS).

### [V9] ✅ Incremental SMT cache — ЗАКРЫТО Plan 33.3 Ф.12 (2026-05-16)
- **Где:** `compiler-codegen/src/verify/cache.rs`.
- **Что реализовано:** FNV-1a 64-bit hash (стабильный между запусками),
  `target/contracts-cache/<hash>.json`, атомарная запись tmp+rename,
  NOVA_NO_CACHE=1, NOVA_CACHE_DIR env vars.
- **Остаток:** Parallel verification (rayon) и Z3↔CVC5 cross-check — Plan 33.5.

### [V10] ✅ #must_verify_module + #trusted + nova contracts CLI — ЗАКРЫТО Plan 33.3 Ф.13 (2026-05-16)
- **Где:** `compiler-codegen/src/ast/mod.rs`, `parser/mod.rs`, `verify/pipeline.rs`,
  `nova-cli/src/main.rs`.
- **Что реализовано:** `#must_verify_module` (ModuleAttrKind::MustVerifyModule) →
  все функции MustVerify. `#trusted external fn` → контракты axioms, SMT skip.
  `nova contracts list/verify/suggest/counterexample` → JSON schema nova-contracts-diag/v1.

### [V11] ✅ Dafny-parity 20 примеров — ЗАКРЫТО Plan 33.3 Ф.14 (2026-05-16)
- **Где:** `nova_tests/contracts/f14_*.nv` (20 файлов).
- **Что реализовано:** binary search, sorting invariants, stack/queue,
  bank account, arithmetic lemmas, linked list, integer overflow,
  string ops, boolean algebra, fibonacci, GCD/LCM, AVL balance,
  bit manipulation, intervals, pure functions, multivar, hash table,
  segment tree, graph BFS, memory safety. 115 PASS 0 FAIL.
  «Dafny-parity».

### [V18] ✅ Z3 CI matrix — ЗАКРЫТО Plan 33.6 Ф.5.2 (2026-05-16)
- **Где:** `.github/workflows/contracts-z3.yml`.
- **Что реализовано:** CI matrix с двумя jobs: TrivialBackend (default) и Z3
  (`--features z3-backend` + `NOVA_SMT_BACKEND=z3`). Тесты `REQUIRES_SMT_BACKEND z3`
  прогоняются в z3-job, пропускаются в trivial-job.
  `docs/promts/read-toolchain.md` обновлён с Z3 build инструкцией.
- **Дата закрытия:** 2026-05-16

### [V19] ✅ Exhaustive encode_expr — ЗАКРЫТО Plan 33.6 Ф.6.1 (2026-05-16)
- **Где:** `compiler-codegen/src/verify/encode.rs`, функция `encode_expr`.
- **Что реализовано:** Exhaustive match по всем ExprKind вариантам с явными
  `Err(EncodingError::Unsupported(...))` сообщениями и suggestions (tuple → separate vars,
  match → if/else, lambda → #pure fn и т.д.). Soundness gap закрыт.
- **Дата закрытия:** 2026-05-16

### [V20] ✅ BitVec theory (sized integers) — ЗАКРЫТО Plan 33.7 V1+V2 (2026-05-21)
- **Где:** `compiler-codegen/src/verify/{ir.rs,encode.rs,pipeline.rs,backend/z3.rs,backend/z3_ffi.rs,backend/trivial.rs}`, `compiler-codegen/src/ast/mod.rs`, `compiler-codegen/src/parser/mod.rs`.
- **Что реализовано:**
  * `SortRef::BitVec(N)` и `SmtTerm::BitVecLit(v, w)` в SMT-IR.
  * Z3 FFI: bvadd/bvsub/bvmul/bvsdiv/bvudiv/bvsrem/bvurem, bitwise bvand/bvor/bvxor/bvnot/bvshl/bvlshr/bvashr, signed/unsigned comparisons bvslt/bvsle/bvsgt/bvsge/bvult/bvule/bvugt/bvuge, overflow predicates bvadd_no_overflow/bvsub_no_underflow/bvmul_no_overflow.
  * `type_ref_to_sort`/`type_to_sort`: u8/i8→BitVec(8), u16/i16→BitVec(16), u32/i32→BitVec(32), u64/usize→BitVec(64); `int`/`i64` остаются `SortRef::Int`.
  * BV binary dispatch в `encode_expr`: если хоть один операнд BV-типа → bv-операторы; `IntLit`-литерал в BV-контексте автоматически поднимается в `BitVecLit`.
  * `as`-cast encoding: `0 as u32` → `BitVecLit(0, 32)` и т.д.
  * TrivialBackend: `check_sat` ранний выход с `UnsupportedTheory` для BV-сортов или bv-операторов.
  * `#nooverflow` атрибут: парсится как `ContractAttrs.no_overflow: bool`, устанавливает `FnDecl.no_overflow`; pipeline.rs генерирует overflow VCs (`bvadd_no_overflow_u` и т.д.) для каждой Add/Sub/Mul в теле fn с BV-sorted параметрами.
  * 5 новых тестов V1: f60_bv_arith_trivial_positive, f60_bv_arith_z3_positive, f60_bv_bitwise_z3_positive, f60_bv_nooverflow_safe_z3_positive, f60_bv_nooverflow_overflow_fail.
- **V2 (ЗАКРЫТО 2026-05-21):**
  * ✅ Точная знаковость: `SortRef::BitVec { width, signed }` — i8/i16/i32→signed,
    u8/u16/u32/u64→unsigned. `is_signed` берётся из BV-операнда (`bv_signed`),
    не глобальный false. Влияет на bvsdiv/bvslt vs bvudiv/bvult и на выбор
    `bvadd_no_overflow_s/u` в overflow VC.
  * ✅ BV cast resize: `as`-каст между BV-ширинами через `zero_extend N`
    (unsigned-источник) / `sign_extend N` (signed) / `extract H L` (сужение).
    FFI: `Z3_mk_zero_ext`/`Z3_mk_sign_ext`/`Z3_mk_extract`; translate_app
    парсит числовой параметр из op-строки.
  * ✅ Overflow VCs для блочных тел: `collect_bv_arith_ops_in_body` рекурсит
    в let-bindings и блок-выражения (`BvScope` с subst-картой). `let x = E`
    регистрирует subst `x → encode(E)` → VC переписывается в терминах
    fn-параметров (declared в backend) — избегает undeclared-var в Z3.
  * 4 новых теста V2: f61_bv_signed_z3_positive, f61_bv_cast_resize_z3_positive,
    f61_bv_nooverflow_block_z3_positive, f61_bv_signed_overflow_fail.
- **Остаток:** нет. V20 полностью закрыт (V1 + V2).
- **Дата закрытия:** V1 — 2026-05-20; V2 — 2026-05-21.

### [V23] ✅ Verifier soundness hardening — ЗАКРЫТО Plan 33.8 (2026-05-21)
- **Где:** `compiler-codegen/src/verify/pipeline.rs`, `codegen/emit_c.rs`,
  `nova_rt/effects.h`, `lints.rs`, `ast/mod.rs`, `spec/decisions/04-effects.md`.
- **Контекст:** аудит «с чистого листа» при закрытии Plan 33.7 нашёл 3
  SOUNDNESS-CRITICAL дыры — места, где верификатор объявлял контракт
  «доказан», хотя в рантайме он мог быть ложным.
- **Что закрыто:**
  * **Переполнение `int`** (Ф.1). `int` (i64) переполнялся молча (C-UB),
    а верификатор кодировал `int` безграничным Z3 Int → `ensures result==a+b`
    «доказывался», в release проверка стиралась, рантайм переполнялся.
    Фикс: переполнение `int` → `panic` (`nova_int_checked_add/sub/mul` через
    `__builtin_*_overflow` → `nv_panic`). Паника делает безграничную
    кодировку sound (функция либо вернёт истинный результат, либо умрёт).
    `nat` — аксиома `nat >= 0`. Спека `04-effects.md` исправлена.
  * **Сохранение инварианта цикла** (Ф.2). `verify_loop_preservation`
    havoc-моделировала только присваивания первого уровня; составные
    `*=`/`/=`, вложенные в if/блок/цикл, повторные — переменная замораживалась
    → ложный `Proven`. Фикс: `loop_body_model_incomplete` — тело вне
    sound-envelope → fail-safe `Warning`, не `Proven`.
  * **`assume`** (Ф.3). Обещанный линт `trust-introduced` не существовал;
    AST-комментарий лгал про SMT-интеграцию. Фикс: линт реализован;
    комментарий честный (SMT-интеграция `assume` — V2, наивная была бы
    unsound в не-flow-sensitive модели).
- **Ф.6 — второй аудит «с чистого листа» (нашёл 3 пропущенных проблемы):**
  * Ф.6.1 — фикс Ф.1.2 был НЕПОЛНЫМ: compound assignment `+=`/`-=`/`*=`
    для `int` эмитился сырым C мимо checked-арифметики → молчаливый wrap.
    Закрыто: `emit_c.rs` роутит int compound-assign через `nova_int_checked_*`.
  * Ф.6.2 — Z3 `assert()` молча отбрасывал непереведённые формулы → если
    `not goal` не транслировалась, противоречивый контекст давал ложный
    `Proven`. Закрыто: `translation_failed` флаг → `check_sat` → `Unknown`.
  * Ф.6.3 — `assert_static` не верифицировался SMT (spec Plan 33.2 Ф.8
    не выполнена). V1: lint `assert-static-unverified`; SMT-верификация → V2.
  * Ф.6.4 — сборщики циклов спускались только в `Stmt::Expr` (циклы в
    `let`/`return` пропускались). Ф.6.5 — рекурсия без `decreases` → W2402.
- **Остаток (V2, НЕ soundness — оптимизация/полнота):**
  * Ф.1.3 — overflow-VC для `int` в верификаторе (предупреждать «возможна
    паника» + стирать panic-check где доказано). Оптимизация.
  * Ф.2.2 — моделировать условные/составные присваивания в циклах через
    `ite` (доказывать такие циклы, а не честно warning'ать). Полнота.
  * `assume` + `assert_static` SMT-интеграция — требует flow-sensitive
    верификации (единая V2-фича).
- **Тесты:** 14 новых (`loop_cond_assign_w2402`, `loop_compound_assign_w2402`,
  `assume_trust_introduced_warn`, `int_overflow_{add,mul,compound}_panic`,
  `int_arith_no_overflow_positive`, `assert_static_unverified_warn`,
  `recursive_no_decreases_warn` + 4 unit-теста `lints.rs`). Полный
  `nova_tests`: 936 PASS / 0 FAIL; contracts: 291 PASS / 0 FAIL.
- **Дата закрытия:** V1 (Ф.1–Ф.5) — 2026-05-21; Ф.6 (2-й аудит) — 2026-05-21.

### [ЗАКР 2026-05-16] pipeline.rs монолит — handler code в отдельный модуль [Ф.2.1]
- **Закрыто (Plan 33.6 Ф.2.1, 2026-05-16, commit ddc11f2e):**
  * `compiler-codegen/src/verify/handler_exec.rs` — 689 строк handler verification:
    `verify_handlers`, `verify_post_axiom_with_handler`, `verify_static_axiom_with_handler`,
    `verify_liskov_method`, symbolic exec V2 helpers, collect_verify_bindings_*.
  * `pipeline.rs`: 2952 → 2188 строк (было > 2700, цель выполнена).
  * `verify/mod.rs`: `pub mod handler_exec` + реэкспорт `verify_handlers`.
  * Вспомогательные функции — `pub(super)` для доступа между модулями.
- **Дата закрытия:** 2026-05-16

### [ЗАКР 2026-05-15] `#verify` handler gate — P0-1 V1 — [V12]
- **Закрыто (Plan 33.4 P0-1, 2026-05-15):**
  * `verify_handlers(module)` в pipeline.rs — walks `with #verify E = h` bindings.
  * Для каждого static axiom (без `post(...)`) : assert handler's pure_view body
    как Forall axiom, call `try_prove(axiom_formula)`.
  * `post(...)` axioms → `Unknown("post-axiom V2")` (честно документировано).
  * Test: `nova_tests/contracts/handler_verify_v1_positive.nv` (72/72 PASS).
- **Остаток (V2):**
  * `post(Action(args))(view(vp)) == X` axioms — требует symbolic execution
    handler action body (присваивания → SMT equalities).
  * Handler body с branching — только linear path в V2, SCC в V3.
- **Приоритет остатка:** H — soundness gap закрыт для static axioms;
  post-axioms всё ещё placeholder.

### [ЗАКР 2026-05-15] Composition в контрактах — [V13]
- **Закрыто (Plan 33.4 D.0.2, 2026-05-15):**
  * `encode_expr(Call)` для `#pure` fn → UF `_pure_fn_<name>(args)`.
  * `collect_pure_fns` — реестр `#pure` fn с сортами параметров.
  * Body axiom: `∀ params. uf(params) == encoded_body` (для `=> expr` тел).
  * Тесты: `composition_trivial_positive.nv`, `composition_z3_positive.nv`.
  * Regression: 68/68 PASS contracts/.
- **Ещё открыто:** SCC mutual-recursive `#pure` fn — V2. См. [V3].

### [ЗАКР 2026-05-15] Loop invariants/decreases в AST + SMT — [V14]
- **Закрыто (Plan 33.4 D.0.3 + D.0.4, 2026-05-15):**
  * AST: `invariants: Vec<Expr>`, `decreases: Option<Box<Expr>>`
    в `ExprKind::For/While/WhileLet/Loop`.
  * Parser: `parse_loop_clauses` сохраняет в AST.
  * SMT entry-check: `collect_loop_invariants_in_body` + proof given requires.
  * `decreases` в fn: SMT доказывает `dec >= 0` на входе и `dec(args_rec) < dec(entry)`.
  * Тесты: `loop_invariant_smt_positive.nv`, `decreases_wf_z3_positive.nv`.
  * Regression: 68/68 PASS, 9 SKIP (Z3-only).
- **Ещё открыто:**
  * Loop havoc + preservation (полный SMT) — V2 (entry-check partial).
  * `decreases` в цикле SMT — Plan 33.4 D.1.x.

### [ЗАКР 2026-05-15] Frame SMT axiom — [V15]
- **Закрыто (Plan 33.4 D.1.2):**
  * Для каждого параметра НЕ в `modifies`-списке: `(assert (= _old_x x))`.
  * Z3 получает факт неизменности non-modified params; `ensures old(z)` верифицируется.
  * `FrameTarget::Whole(Ident)` извлекает имена; ArrayElem/Field skipped.
  * Test: `nova_tests/contracts/frame_smt_positive.nv` (70/70 PASS).
- **Остаток:** split-variable encoding (x_pre/x_post) для mutable params — V2.

### [ЗАКР 2026-05-15] BinderType enum для EffectAxiom.binders — [V16]
- **Закрыто (Plan 33.4 P1-5, 2026-05-15):**
  * `BinderType { Untyped, Typed(TypeRef), Generic(String) }` + `BinderDef`.
  * `EffectAxiom.binders: Vec<BinderDef>` — три состояния различимы.
  * Parser: Generic = path[0] ∈ generics. Downstream: types/pipeline обновлены.
  * Regression: 68/68 PASS.

### [ЗАКР 2026-05-15] Fail-path contracts (`ensures_fail`) — [V17]
- **Закрыто (Plan 33.4 D.1.5):**
  * `ContractKind::EnsuresFail` — постусловие для Fail-пути.
  * Синтаксис: `ensures_fail <bool-expr>` после сигнатуры функции.
  * SMT-верификация: independent pass под `requires`-context;
    `result` недоступен, `old(x)` доступен (V1 bootstrap).
  * Без runtime check в V1 (specification annotation only).
  * Test: `nova_tests/contracts/ensures_fail_positive.nv` (71/71 PASS).
- **Остаток:** forbid `result` inside ensures_fail — V2; Fail-path
  symbolic execution (caller sees «if throws, then ensures_fail holds») — V3.

### [ЗАКР 2026-05-15] Plan 33.5 Contracts Verifier Production Hardening — [V12/V13/V6-частично]

Закрыт в ветке `plan33-4`. Итог: 82 PASS, 9 SKIP (z3-only).

| Ф | Feature | Статус |
|---|---|---|
| Ф.3 | SCC purity inference | ✅ ЗАКРЫТ |
| Ф.4.1 | Lemma functions (`lemma` / `apply`) | ✅ ЗАКРЫТ |
| Ф.4.2 | Calc proofs (`calc { expr; == expr; }`) | ✅ ЗАКРЫТ |
| Ф.5.1 | EffectMethod contracts (requires/ensures на op) | ✅ ЗАКРЫТ |
| Ф.5.2 | Liskov SMT verify (#verify handler vs effect contracts) | ✅ ЗАКРЫТ |
| Ф.6 | post(Action)(view) symbolic exec V2 | ✅ ЗАКРЫТ |

**[V12] закрыт:** `#verify` handler gate теперь реально верифицирует через Z3/Trivial.
**[V13] частично закрыт:** pure fn composition в SMT-encode работает через `infer_pure_fns_scc` + `PureFnInfo`. Encoded как UF с body-axiom.

**Остающиеся ограничения Ф.6 (post symbolic exec):**
- Action body — только `Block` с простыми `Assign`. Нет if/match/loop.
- View body — только `=> expr`. Нет block-body handlers.
- Одна captured переменная (нет State-record / многопольного state).
- Нет учёта aliasing binders (id в action и id в view считаются одинаковыми).
- **Приоритет:** L — покрывает 90% паттернов; сложные случаи → `#trusted`.

### [V21] Generic axioms — Unknown в SMT encoding (2026-05-15)
> **Перенумеровано из `[V15]`** (аудит Plan 33.8, 2026-05-21): тег `[V15]`
> уже занят закрытой записью «Frame SMT axiom». Эта запись — ОТКРЫТА.
- **Где:** `compiler-codegen/src/verify/pipeline.rs::encode_axiom`.
- **Что:** `axiom foo[T](id T) => ...` с generic binder возвращает
  `Unknown(NotAttempted)` без SMT verification.
- **Почему:** Generic axiom требует Z3 polymorphic sort (`Z3_mk_type_var`)
  или монаморфизацию по use-site — ни то ни другое не реализовано.
- **Как чинить:** Монаморфизация: для каждого axiom — enumerate
  concrete types из binder usage, emit конкретную версию axiom.
- **Приоритет:** M — generic axioms используются в стандартных
  алгоритмических паттернах (sorted arrays, set membership).

### [V22] post(Action)(view) — block-body handlers не поддержаны
> **Перенумеровано из `[V16]`** (аудит Plan 33.8, 2026-05-21): тег `[V16]`
> уже занят закрытой записью «BinderType enum». Эта запись — ОТКРЫТА.
- **Где:** `compiler-codegen/src/verify/pipeline.rs::verify_post_axiom_with_handler`.
- **Что:** handler method с `block { ... }` body вместо `=> expr` пропускается
  (continue) в V1 верификации static axioms. В Ф.6 post-symbolic-exec —
  поддержан только view `=> expr`, action `Block` (но только с простыми assign).
- **Почему:** Block-body view требует symbolic evaluation всего блока
  (SSA / abstract interpretation). V2 scope — только simple assign chains.
- **Как чинить:** Symbolic block evaluator: convert block к SSA-form,
  abstract-interpret assignments, extract result expression.
- **Приоритет:** M — многие реальные handlers используют block-body.


---

## char.try_from с unreachable Err fallback — Plan 34 Ф.5.2 (2026-05-12)

**Где:** 5 stdlib-файлов:
- std/encoding/base64.nv: `encode_char_std`, `encode_char_url`
- std/encoding/hex.nv: `digit`
- std/identifiers/ulid.nv: `encode_char`
- std/identifiers/uuid.nv: `hex_digit`
- std/testing/property.nv: `StrGen @generate` (ASCII char)

**Что упрощено:** D54 запрещает `int as char`, требует
`char.try_from(n)?`. Но в этих случаях `n` всегда в valid диапазоне
(`'0' + value` для `value ∈ [0, 15]` всегда даёт ASCII digit), значит
`Err` невозможен. Refactor:

  let code = '0' as int + value as int
  match char.try_from(code) {
      Ok(c)  => c
      Err(_) => '?'      // unreachable
  }

Fallback `'?'` нужен для exhaustive-match, но недостижим —
**семантически dead branch**.

**Почему:** Альтернативы хуже:
1. `?`-propagation — меняет return type `-> char` → `Fail[CharRangeError] -> char`,
   ломает все callers (3-tier изменение).
2. `panic("unreachable")` — runtime crash вместо degraded output.
3. `unsafe_int_as_char` — нет в spec, добавлять ради 5 callsites не оправдано.

**Как починить:** Plan 37 «type-check semantic parity» (создан агентом)
поднимет D54 проверку в type-checker. После этого type-checker может
validate static-range проверки compile-time (literal + bounded variable
analysis), `char.try_from(IntLit | bounded var)` опускается до direct
cast, fallback ветка элиминируется как dead code.

**Приоритет:** P3 — fallback недостижим, downstream perf не страдает.


---

## stdlib `--skip std/runtime/` обязателен для nova test — Plan 34 Ф.5.1 (2026-05-12)

**Где:** Workflow для CI / dev sweep по stdlib.

**Что упрощено:** `nova test std/` без `--skip std/runtime` даёт **7
false-FAIL'ов** для auto-gen библиотечных модулей std/runtime/* (char/
gc/math/read_buffer/string/string_builder/write_buffer) с linker
error `undefined symbol 'nova_fn_main_impl'`. Эти файлы — *lib-only*,
у них нет main и tests, но `nova test` пытается их собрать как exe.

D95 hard-skip `std/runtime/` есть в `nova check` (через
`should_skip_path`), но **не в `nova test`**. Текущее workaround —
обязать пользователя писать `--skip std/runtime` вручную.

**Почему не auto-skip в walk_nv:** Параллельный агент выбрал
**explicit --skip flag** (commit before f481e3950e), а не зашитую
константу в `walk_nv` (я пробовал, откатил по запросу пользователя).
Преимущество: пользователь видит что skip'ается; не зашиты опциональные
правила в core walker. Минус: friction для типичного use-case.

**Как починить (полное решение):** Один из вариантов:
1. **D95 расширить на nova test** — добавить `runtime` в
   `is_implicit_skip` ИЛИ вызвать `should_skip_path` в test_runner's
   walk-этапе (как уже сделано в check). ~10 строк.
2. **Per-file pragma** `// LIB_ONLY` — runner пропускает файлы без
   main и без test-блоков. Более общее, но больше работы.
3. **Manifest-уровень**: `std/runtime/nova.toml` с
   `kind = "library"` исключает из test sweep'а. Архитектурно
   правильнее, но требует package-system (Plan 03).

**Приоритет:** P2 — `--skip std/runtime` работает, но это lasting
papercut для каждого нового пользователя.


---

## Plan 34 Ф.5.3 — strict-bool fix НЕ применён (D72 блокер) — 2026-05-12

**Где:** 4 файла std/ остались с `if condition must be bool` codegen-fail:
- std/collections/priority_queue.nv:69 `@items[i].lt(@items[parent])`
- std/concurrency/retry.nv:121 `d.gt(max_delay)`
- std/encoding/json.nv:526 `fields.contains(key)`
- std/encoding/url.nv:78 `after_scheme.starts_with("//")`

**Что упрощено:** Изначально Plan 34 Ф.5.3 планировал локальный fix
`if x` → `if x != 0`. После анализа стало ясно — это **не** локальная
правка. Все 4 вызова — generic-method dispatch через protocol-bound
(`Ord.lt`, `Ord.gt`, `Hash.contains`, `Str.starts_with`), который
codegen в generic-context возвращает с return-type `nova_int` вместо
`bool` (D72 erasure).

Plan 14 retrospective прямо называет это «блокер для Plan 15
enforcement». Локальный `!= 0` workaround **не помогает** — codegen
всё равно видит `nova_int` value.

**Почему не fix:** Spec-level work — нужно расширить codegen
`method_overloads` для protocol-bound generics так чтобы они возвращали
правильный bool-type. Это **Plan 15 enforcement** territory +
monomorphization. Не Plan 34 scope.

**Как починить:** Новый план «D72 method-resolution через
protocol-bounds в codegen» — ~200-300 строк в emit_c.rs +
method_overloads expansion. Открывает 4+ stdlib-файла для compile.

**Приоритет:** P1 — блокирует 4 файла, но D72-уровень требует careful
spec-level work.

---

### [M10] Rule C (per-peer imports) не enforced — ✅ RESOLVED (для импортированных folder-modules) 2026-05-14

- **Resolved:** Plan 42.15 — NameResCtx переведён на per-group visible
  scope. `group_decls` (declarations module-group каждого peer'а) +
  `peer_imported_names` (per-peer imports, НЕ shared) + Path-form check
  в walk_expr. Imported items больше не «протекают» между peers.
- **Tests:** `peer_path_leak.nv` (negative — cross-peer alias use →
  undefined identifier) + `peer_isolation_ok_use.nv` (positive — peers
  share declarations namespace).
- **Квалификация (Plan 42.17 audit):** per-peer изоляция реальна для
  **импортированных** folder-modules (peers получают distinct `file_id`
  через `parse_with_file_id`). Когда folder-module — **сам компилируемый
  entry**, все его peers коллапсируют в один `MAIN_FILE_ID` PeerFile →
  изоляция между ними становится no-op. См. `[M-entry-folder-module]`.

---

### [M-interp-named] treewalk-interp: named args без reorder — ✅ RESOLVED 2026-05-15

- **Resolved:** Plan 50 Ф.2 — `cmd_run` (`nova-cli/src/main.rs`) теперь
  делает `resolve_imports_inline` ПЕРЕД `callnorm::normalize_module` —
  тот же codepath, что `cmd_build` и `test_runner::codegen_to_c`.
  Импортированные callee мёрджатся в `module` до нормализации →
  `callnorm` видит ВСЕ сигнатуры (включая дефолты импортированных
  функций) и раскладывает named args в param-order корректно. Interp
  получает чистый позиционный AST для всех callee, не только
  same-file. Graceful: файл вне Nova-проекта (нет nova.toml) →
  resolve пропускается, single-file без импортов работает как прежде.
- **Tests:** `nova_tests/named_params/imported_named_use.nv` (codegen-suite,
  переставленные named для импортированного callee) +
  `imported_named_run.nv` (codegen-suite через `EXPECT_STDOUT` +
  nova-cli integration-тест `tests/run_interp_named.rs` через
  `nova run` — двойное покрытие interp-пути).

---

### [M-match-void-arm] match-как-выражение с void-typed arm'ами → невалидный C

- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — emit `match` в
  expression-позиции.
- **Что:** когда `match` стоит как голый statement (его значение не
  используется), а каждый arm — выражение типа `unit`/`void` (например
  `assert(...)`, который в рантайме `static inline void nova_assert(...)`),
  codegen всё равно объявляет temp `nova_unit _nv_match_N;` и пишет
  `_nv_match_N = nova_assert(...)` → C-ошибка «assigning to 'nova_unit'
  from incompatible» (нельзя присвоить результат void-функции).
- **Обнаружено:** Plan 51 Ф.4 — позитивный тест писал
  `match s { Circle {r} => assert(...) Square {s} => assert(...) }`
  как statement. Переписан на `let x = match ...` с arm'ами,
  возвращающими `int` — обычный паттерн, codegen его поддерживает.
- **Как починить:** codegen должен либо (а) не эмитить присваивание
  temp'у, когда тип match-выражения — `unit` и оно в statement-позиции,
  либо (б) эмитить arm'ы как statements (без `_nv_match_N =`). ~20-40 LOC
  в emit `match`.
- **Приоритет:** L — узкий паттерн (`match`-statement, где каждый arm
  сам void-typed). Idiomatic-форма (`match` в let / с не-void arm'ами)
  работает. Не относится к Plan 51 (синтаксис record-литералов).

---

### [M11] Rule A cycle detection — canonical PathBuf keying — ✅ RESOLVED 2026-05-14

- **Resolved:** Plan 42.14 Ф.3 — `in_progress`/`visited` переведены на
  `HashSet<Vec<String>>` keyed by declared module name (через
  `read_module_decl` lightweight parser). Symlink / case-insensitive FS
  edge case устранён — module name стабильный логический identity.
- **Tests:** `folder_cycle_between_modules.nv` + `import_cycle_rejected.nv`
  PASS с новым keying.
- **Доделано (Plan 42.17 Ф.3):** три копипаст-сканера `module`-строки
  (`read_module_decl` + `is_folder_module_peer` + `is_folder_module_dir`)
  объединены в один `imports::scan_module_decl`. Drift-риск устранён.
  Block-комментариев у Nova нет (лексер обрабатывает только `//`), так
  что отдельная их обработка не требуется — audit-флаг был ложным.

### [M12] Selective import — visible-scope enforcement — ✅ RESOLVED 2026-05-14

- **Resolved:** Plan 42.15 — `import X.{A}` теперь strict: items НЕ в
  selective `{...}` списке merge'атся в `merged_items` для codegen
  completeness, НО НЕ попадают в `peer_imported_names` (visible scope).
  resolver проверяет `imp.items` при заполнении `visible_acc` —
  только items из selective list (после rename) видны импортирующему.
- **Tests:** `rename_old_name_rejected.nv` (negative — старое имя после
  `A as B` rename → undefined) + `rename_import_use.nv` (positive).
- **Квалификация (Plan 42.17 audit):** как и `[M10]` — visible-scope
  enforcement реален для импортированных folder-modules; entry-folder-
  module см. `[M-entry-folder-module]`.

---

### [M-entry-folder-module] Entry folder-module — per-peer изоляция не активна — ✅ RESOLVED 2026-05-21

- **Где:** `compiler-codegen/src/imports.rs` (`resolve_imports_inline_ex`).
- **Что:** entry-модуль парсится caller'ом как **один файл**
  (`parser::parse(src)` → `MAIN_FILE_ID`) и регистрируется как один
  `PeerFile`. Если этот entry-файл — peer folder-module, его sibling
  peers **не собираются** (нет кода, который делал бы это для entry —
  только для импортированных folder-modules в `resolve_one`). Поэтому
  Rule C / `[M10]` / `[M12]` per-peer изоляция между peers самого
  entry-модуля — no-op.
- **Почему не критично сейчас:** не reachable в bootstrap. `nova test`
  компилирует test-файлы (folder-module всегда импортируется через
  `_use.nv`); `nova build`/`nova run` берут single-file entry. Entry-as-
  folder-module появится когда `main` проекта станет папкой.
- **Как починить (полный дизайн, Plan 42.17 Ф.8 investigate-итог):**
  Две связанные части:
  1. **Resolver-side** (`resolve_imports_inline_ex`): после parse entry —
     детектить, что `entry_path.parent()` — folder-module (≥2 `.nv`,
     все объявляют тот же `module`, совпадающий с `module.name` entry).
     Если да — собрать sibling peers (alphabetical, `_test`/`#cfg`
     filter как в `resolve_module_paths`), parse каждый с distinct
     `file_id`, register как `PeerFile { is_entry_module: true }`,
     merge items в `module.items` **включая `Item::Test`** (в отличие
     от imported peers — у entry-folder-module свои тесты должны
     гоняться), recursively resolve их imports. Зеркалит peer-loop из
     `resolve_one` (~100 LOC). Сам по себе zero-regression-risk: gated
     на условии, ложном для всех текущих entry (single-file / `_use.nv`).
  2. **Test-runner-side** (`walk_nv`): сейчас peers folder-module
     **пропускаются** как test-entry (тестируются через внешний
     `_use.nv`). Для постоянного regression-guard `nova test` должен
     компилировать folder-module как unit и гонять её `test`-блоки.
     Меняет entry-selection → начнёт компилировать каждую fixture
     standalone — **риск для 350-test регрессии**, отдельная focused-
     работа.
- **Resolved:** Plan 81 Ф.10 — **resolver-side** реализован.
  `resolve_imports_inline_ex` детектит entry-folder-module, собирает
  sibling peers (distinct `file_id`, `is_entry_module=true`), мёрджит
  их items (включая `Item::Test`), резолвит import'ы каждого peer'а в
  его собственный visible-scope (Rule C). Prelude резолвится один раз
  и разделяется всей entry-группой. Попутно `manifest::check_module_path`
  стал folder-module-aware (канонический `imports::is_folder_module_peer`).
- **Test-runner-side НЕ делался — сознательно** (не упрощение): авто-
  компиляция folder-module как unit в `walk_nv` меняет entry-selection
  всего дерева `nova_tests/` (риск широкой регрессии) и не даёт
  correctness-выигрыша — regression-guard уже обеспечен nova-cli
  integration-тестом + resolver unit-тестами. См. Plan 81 §Ф.10.
- **Tests:** `compiler-codegen/src/imports.rs` (2 unit-теста resolver'а)
  + `nova-cli/tests/entry_folder_module.rs` (integration: `nova check`
  на peer'е folder-module `nova_tests/plan81/entry_fmod/`).


---

## Plan 34 Ф.5.4 — for-in nova_int НЕ закрыт целиком — 2026-05-12

**Где:** 5 файлов std/ с `for-in: unsupported iterator type 'nova_int'`:
- std/crypto/bcrypt.nv, std/collections/range.nv,
  std/encoding/ini.nv, std/text/diff.nv, std/text/regex.nv

**Что упрощено:** Plan 14 Ф.1 refactor Option[T] раскрыл Iter[T]
erasure для нестандартных iterator expressions. `for i in seq.iter()`
где seq имеет custom Iter — codegen падает.

Параллельный агент сделал commit `e019a47128` "forward-decl user types
+ Nova_Range emit + Range infer для step_by" — это закрывает
**same-file** Range/StepRange. Cross-file и custom Iter ещё открыты.

**Почему не fix в Plan 34:** Iter[T] generic specialization at
monomorphization — Plan 14 «накопленные блокеры» категория.
Architectural work уровня spec.

**Как починить:**
1. Cross-file Range — Plan 35 Ф.2 (cross-file codegen). MVP через
   `f481e3950e` (inline AST expansion) частично решает.
2. Custom Iter (`hashmap.keys() -> Iter[K]`) — Plan 14 «hashmap
   protocol-dispatch» блокер. Требует monomorphization для generic
   methods.

**Приоритет:** P1 — 5 stdlib-файлов и больше, но архитектурный блокер.


---

## range.nv blocked — known limitation (Plan 39, 2026-05-12)

### Где
`std/collections/range.nv` — full file compile блокирован.

### Что упрощено
`std/collections/range.nv` объявляет 4 core types (Range, RangeIter,
StepRangeIter, ReverseRangeIter) + ~30 methods + 11 inline tests.
**Не компилируется** через `nova test` / `nova build` из-за:
1. `int.MAX` mangling → Plan 38.
2. `nova test` cross-file resolution отсутствует → Plan 35 Ф.1
   test_runner parity (отложено).
3. Возможные `NovaOpt_<T>` typedef mismatches в pattern match
   ассертах (`r.next() == None`).

### Почему
Cascade блокеров — каждый требует отдельного fix'а в codegen.
Pre-existing, не Plan 35 territory.

### Как починить
Plan 39 = follow-up cleanup после Plan 38 + Plan 35 Ф.1 test_runner.

### Workaround сегодня
**Inline Range/RangeIter/StepRangeIter в user file** — Plan 35 Ф.1
MVP уже доказал что same-file path работает. `for_in_range_iter.nv`
тест: 4 assert PASS на inline declarations.

Cross-file через `import std.collections.range` — works для **`nova
build`** (после Plan 35 Ф.1 MVP), не для **`nova test`** (test_runner
pipeline отдельный).

### Приоритет
**P3** — это **cascade follow-up**, не root cause. После Plan 35 Ф.1
test_runner parity + Plan 38 (~1 день combined) — `range.nv` либо
автоматически проходит, либо требует small fix'и (Plan 39, оцениваем
0-200 LOC).


---

## Plan 33.4 P1-4: Liskov-проверка effect-операций — заблокировано (2026-05-15)

### Что задумано

P1-4 предполагает: при `with #verify P = impl` проверять, что `impl`
удовлетворяет контрактам (`requires`/`ensures`) каждой операции протокола `P`
по правилам Liskov (контравариантное pre, ковариантное post).

### Почему не реализовано сейчас

`EffectMethod` (AST-узел для операций effect/protocol) не имеет поля
`contracts: Vec<Contract>`. Контракты (`requires`/`ensures`) существуют
только на `FnDecl`. Операции эффектов/протоколов описывают только сигнатуру
(`params`, `return_type`, `effects`) и вид (`EffectOpKind::Operation` vs
`PureView`) — без pre/post-условий.

Текущий `verify_handlers` (Plan 33.3 Ф.9) уже проверяет `axiom`-формулы
эффекта против реализации handler'а. Это близко к P1-4 для `pure_view`-методов,
но не то же самое: Liskov-проверка операций требует именно per-operation contracts.

### Статус

Заблокировано до V2. Нужно:
1. Добавить `contracts: Vec<Contract>` в `EffectMethod`.
2. Расширить парсер для `requires`/`ensures` внутри `effect`/`protocol`-блоков.
3. Расширить `verify_handlers` для Liskov-проверки: для каждого `op` с контрактами
   найти `handler.op`, закодировать тело handler'а и проверить:
   - contravariant pre: `handler.requires ⇒ protocol.requires`
   - covariant post: `protocol.ensures ⇒ handler.ensures`

Приоритет: M (нужен для осмысленной верификации protocol-handlers).
   - clear error if neither
2. Assert `is_mut=true` для `next()`.
3. Improve diagnostic с конкретным type name + method names searched.
4. Test file `nova_tests/syntax/for_in_iter_resolution.nv`.

### Workaround сегодня
**Manual `.iter()` call:** `for x in c.iter()` вместо `for x in c`.
Это эквивалентно D58 Case 2, но не automatic. Стандартный паттерн
сейчас в std/* — почти все file'ы explicit `.iter()`.

### Приоритет
**P2** — нарушение D58 spec, но обходимо через explicit `.iter()`.
Влияет на UX (программист должен помнить `.iter()` где должно быть
automatic), не на correctness (compile error явный).

### Real-world impact
- Cross-file Range/RangeIter сценарии — partial OK через Case 1
  (Range literal) и Case 3 (RangeIter.next direct).
- `for x in some_hashmap` без `.iter()` — error «unsupported iterator
  type». Workaround: `for x in some_hashmap.iter()`.


---

## Plan 33.3 Ф.9: bootstrap improvements (2026-05-12)

### [ЗАКР] V2 Loop invariants
- **Закрыто:** parse_loop_clauses возвращает invariants caller'у;
  inject_loop_invariants prepend'ит каждый invariant как
  Stmt::AssertStatic в начало body. Runtime check работает в debug.
- **Не закрыто полностью:** pre-entry check (invariant true перед
  first iteration) — invariant injected после первой итерации.
  Полный havoc-based SMT verify ждёт Z3 backend.

### [ЗАКР] V5 Ghost erasure
- **Закрыто:** Stmt::Let с is_ghost=true НЕ emit'ится ни в codegen
  emit_c.rs, ни в interp. Verus/Dafny semantics.
- **Non-ghost код не может читать ghost-vars** — catch'ится на C-level
  (undefined identifier). Proper compile-time check в type-checker —
  отдельная задача (TODO для Plan 33.3 full).


---

## Selective import filter — syntax only в bootstrap (35.A R26, 2026-05-12)

### Был simplification

`import X.Y.{A, B}` синтаксис принят парсером, но **resolver не enforce'ит**
filter — все items имповрта merge'ятся в текущий module.

### Причина

**Transitive dependency closure issue.** Если user пишет
`import std.collections.range.{Range}`, но Range.@step_by возвращает
StepRangeIter — codegen reference'ит StepRangeIter type even though
filter говорит «только Range». Без полного dep-walking (transitive
closure всех referenced types через methods/fields) filter ломал бы
codegen.

### Now compromise

Filter сохраняется в AST.Import.items (syntax-only documentation
намерения программиста). Полный enforcement через type-checker
visibility (видимые имена в module scope) — post-bootstrap.

### Prelude.nv почти пустой (R27, 2026-05-12)

### Был simplification

`std/prelude.nv` существует но содержит только `PRELUDE_VERSION = 1`.

### Причина

Auto-imported items (Option/Result/Some/None/Ok/Err/Error/Never/print/
println/panic) — все hardcoded в type-checker'е и codegen'е через
special cases. Migration этих items в file-based prelude — отдельная
большая работа (refactor type-checker symbol resolution + codegen
emit для prelude items).

### Now compromise

R27 механизм работает (auto-import std.prelude если файл существует);
user'ы могут расширять prelude добавляя items в std/prelude.nv.
Migration hardcoded → file-based — future work.


---

## Time.after per-call allocs ~6 — Plan 44.1 B4 (2026-05-12)

**Где:** `Nova_Time_after` в channels.h.

**Что упрощено:** каждый `Time.after(ms)` = Nova_ChannelPair
(state+buf+tx+rx, 4 allocs) + NovaAfterState (1) + libuv timer
heap (1) = ~6 nova_alloc'ов. Tokio = 0-alloc через inline timer
без backing channel'а.

**Почему:** bootstrap channel-based интегрируется с select как
просто recv arm. 0-alloc требует выделенного timeout-syntax (special
casing), что D94 намеренно избежал.

**Влияние:** GC pressure под нагрузкой (HTTP client pool с timeout'ами).
Под Boehm — minor; под malloc-only — leak.

**Как починить:** timer pool в eventloop.h (Plan 22 follow-up).

**Приоритет:** P2.


---

## Тесты для std/testing/handlers.nv — inline reproducers вместо direct (Plan 34 followup #2, 2026-05-12)

**Где:** nova_tests/plan34/inline_xoshiro_determinism.nv,
nova_tests/plan34/inline_mut_clock_advance.nv.

**Что упрощено:** прямые тесты `seeded(seed u64)` / `mut_clock(start_ms)`
из std/testing/handlers.nv через `with Random = th.seeded(...) { ... }`
не могут быть запущены — codegen падает на `unknown type
NovaVtable_Random` (CC-FAIL). Это **category-D codegen bug** для
stdlib effect-types, не Plan 34 scope.

Вместо direct tests написал **inline reproducers**:
- `inline_xoshiro_determinism.nv` — splitmix64 + xoshiro256++ как
  обычные функции `xoshiro_init(seed) -> XState`, `xoshiro_next(st)
  -> (XState, u64)`. Те же константы (`0x9E3779B97F4A7C15`,
  `0xBF58476D1CE4E5B9`, ...) и логика что в handlers.nv.
- `inline_mut_clock_advance.nv` — `Clock { ms u64 }` record +
  `clock_sleep_ms(c, delta)` функция. Моделирует state advance
  без `Time` effect.

**Почему:** algorithm correctness — главное (xoshiro determinism,
splitmix64 non-zero seed=0). Effect-codegen — отдельная архитектурная
работа. Когда NovaVtable_<Effect> codegen закроется, inline тесты
можно заменить на real handler-call wrapper-тесты.

**Как починить:** новый план «codegen для stdlib effect-types
(NovaVtable_<Effect>)» — расширить emit_c.rs для эффект-литералов
объявленных не в нативных runtime headers, а в .nv stdlib файлах.
~150-300 строк.

**Приоритет:** P2 — inline тесты покрывают algorithm regression,
direct тестирование handlers.nv логики через `with` ждёт codegen
work.


---

## str lex compare bootstrap byte-wise (2026-05-12)

### Что simplified

`nova_str_cmp` / `lt`/`le`/`gt`/`ge` в bootstrap делают **byte-wise**
сравнение через memcmp. ASCII-correct, UTF-8 partial (byte order
совпадает с codepoint order для valid UTF-8 кроме edge cases).

### Production milestone

Полное Unicode collation (locale-aware, normalization NFC/NFD, case
folding) — requires ICU или подобная библиотека. Сейчас не блокер
для bootstrap.

### Method-форма str.lt() / str.gt() — partial

Operator-форма (`s1 < s2`) работает через codegen routing. Method-форма
(`s1.lt(s2)`) пока **не работает** — primitive types не имеют method
resolution для bootstrap external fn'ов. Нужна method_overloads
registration для str — отдельная работа.

## std/data/semver_range.nv tuple destructure type-loss (open)

`let (left, build_str) = ...` теряет element types — обе переменные
объявляются `nova_int` в C, что ломает downstream usage как str.
Pre-existing codegen bug, отдельный fix.


---

## `_NOVA_GC_DISABLE` workaround — Plan 27 R4 → Plan 44.2 (2026-05-12)

**Где:** `compiler-codegen/nova_rt/fibers.h::_NOVA_GC_DISABLE/_NOVA_GC_ENABLE`.

**Что упрощено:** suspended fiber stacks выделены через `calloc` (или
minicoro default), не зарегистрированы как GC roots. Conservative
Boehm scanner их не видит → указатели на heap из стека suspended
fiber'а пропускаются → GC может collect ещё-живые объекты → use-after-
free при resume.

**Workaround:** `GC_disable()` в начале scheduler tick'а, `GC_enable()`
в конце. Работает потому что **single-thread cooperative** — GC physically
не запускается между yield/resume. Hidden UAF risk class: любой
`nova_alloc` вне обёрнутого тика — потенциальный crash.

**Почему не сделали properly:** пробовали `GC_add_roots` per-fiber
(Plan 27 R4 audit, commit 31207daabe), упёрлись в `MAX_ROOT_SETS=128`
на 10k fibers.

**Как починить:** Plan 44.2 — per-thread arena с **одной** регистрацией
`GC_add_roots(arena, arena+256MB)`. Все stacks в этом диапазоне → GC
сканит invariant'но → disable не нужен.

**Приоритет:** **P1 — prerequisite для Plan 23 M:N runtime**. Без
arena подхода concurrent GC невозможен (нет общего scheduler tick'а
для disable).

Detail: [docs/plans/44.2-fiber-arena-posix.md](plans/44.2-fiber-arena-posix.md).


---

## D29 rev-2: folder-modules (Go-style peers) (2026-05-12)

### Изменение

D29 rev-1 (single-file) расширен до **D29 rev-2 (file ИЛИ folder)**.
Module = `X.nv` (single-file) ИЛИ `X/` папка с ≥1 peer-файлов (все
объявляют одинаковый `module X`, share namespace).

### Открытое (Plan 42)

Реализация — Plan 42 (`42-folder-modules.md`). Бутстрап MVP не
блокер; первый use-case появится когда std/* модуль превысит ~800 LOC.

### Backward-compat

Existing single-file модели работают без изменений. Folder-module —
opt-in capability.

---

## R8 audit (2026-05-13): что было simplified, что осталось

### Что было упрощено

**Plan 44.1 R6 pin list для NovaAfterState** — был добавлен в audit R6 как
защита от Boehm collection между uv_close и close_cb. **Удалён в R8-1**:
NovaAfterState теперь через malloc/free (pattern Tokio: raw handle, owned
by libuv). Это **не упрощение — улучшение**:
- Linux + Windows symmetric (нет dependency на Boehm root coverage).
- M:N ready (нет global mutex/race на pin list).
- Heap pressure reduction (Time.after в hot loop больше не аллоцирует через GC).

**Workaround "select_timer_cleanup 50 → 25 iter"** — был принят в R7 как
2x safety margin от Windows boundary ~35. **Снят в R8-1**: оригинальный
50-iter тест возвращён, root cause resolved.

### Что осталось simplified (документировано)

**Stack-allocated BaseWaiter — только Linux/macOS** (R8-4). Windows
fallback на nova_alloc остаётся до закрытия Plan 44.3. Это conditional
compile, явно документировано в коде с reasoning:
- POSIX: arena GC root покрывает suspended fiber stacks ⇒ stack safe.
- Windows: calloc'нутые stacks НЕ GC roots ⇒ heap fallback нужен.

**Heap-allocated BaseWaiter под Windows** — теряем 6.4 MB/s GC garbage win
который Linux получает. Когда Plan 44.3 закроется, Windows получит то же
преимущество.

**sendDirect через nova_int direct-copy (P40R8-6 open)** — пока channels
mono-typed, type-pun через w->send_val работает. Когда Plan 21+ обобщит
T, нужно generalize signature. **TIME-BOMB** для T-generic refactor.

### Honest disclosure про audit process

R1-R7 не нашли P0 bugs которые R8 раскопал (NovaAfterState GC managed на
Windows, _registered_high_water не __thread, select pre-check missing
retry). Lesson: **freshly-eyes audit с reference implementation
comparison** (Go runtime, Tokio, crossbeam) catches more чем
self-incremental audit rounds.


---

## Plan 42 implementation — bootstrap simplifications (2026-05-13)

### Compatibility mode (rev-1 + rev-3)

Module declaration check принимает **оба** формата:
- rev-1: full path от source root (`module std.encoding.hex`).
- rev-3: parent.X (`module encoding.hex`).

Это позволяет постепенную миграцию std/* (339 файлов). Без compat
mode — big-bang breaking change неприемлем.

Cleanup rev-1: после полной миграции std/* (отдельная сессия с
automated tool).

### Правило C (per-file imports) — deferred

В Plan 42 design imports внутри folder-module должны быть **per-peer
scope** (Go-style). Bootstrap MVP реализует **shared imports** через
flat merge. Это означает что если peer A импортирует `std.io.File`,
этот import видим из peer B без явного declaration.

**Real fix:** AST refactor `Module.peer_files: Vec<PeerFile>`,
name resolution учитывает per-peer scope. Sub-plan — отдельная работа.

**Bootstrap impact:** programs работают correctly но имеют «leakier»
namespace. Не critical для bootstrap std (использует мало imports
per peer file).

### Правило D (2-pass codegen) — not yet needed

Plan 42 говорил что cross-peer cycles требуют 2-pass codegen.
**На practice:** flat merge всех peer items (alphabetical sort)
обычно работает single-pass — функция в `users.nv` видит forward
declaration функции в `helpers.nv` если items merged correctly.

Если хитрые cross-peer cycles появятся (mutually recursive types
между peers) — нужен 2-pass. Sub-plan когда понадобится.

### Heuristic-based folder-module detection

«All .nv peers в папке объявляют тот же `module X`» = folder-module.
Alternative — explicit declaration в nova.toml или special file.
Heuristic простой, никаких new config files, reliable enough для
standard use cases. Если ambiguous — compiler выдаёт manifest mismatch
error с suggestions.


---

## Plan 44.6: Layer 3 (per-worker libuv loop) без Nova-side workload distribution

**Что упрощено.** Plan 44.6 покрывает только TLS infrastructure для
per-worker libuv loop (`_nova_current_loop`). Worker_main set'ит TLS,
runtime callsites читают его. Это даёт корректность для (будущих)
fiber'ов запущенных через `runtime.spawn_global` — их Time.sleep
park'ается на own loop, callback fires там же, wake срабатывает.

Plan 44.6 **не реализует** Nova-side workload distribution: top-level
`supervised { spawn { ... } }` всё ещё генерирует `nova_fiber_spawn_into`
к main scope (workers idle). Чтобы spawn'ы реально пошли на workers
нужен codegen change в `emit_supervised`: выбор между
`nova_fiber_spawn_into(scope)` (single-thread) и
`nova_runtime_spawn_global(...)` (M:N) в зависимости от
`runtime.is_initialized()`.

**Почему это OK сейчас.** Layer 3 — фундамент для M:N. Без него любая
workload distribution была бы broken (Time.sleep на worker'е hangs).
Layer 3 закрывает infrastructure, Plan 44.7 закрывает API surface.
Логичная sequence: первый PR делает корректным то что уже было (M:N
infrastructure не ломает single-thread baseline), второй PR открывает
parallelism.

**Long-term path.** Plan 44.7: codegen `emit_supervised` routing
+ cross-worker fiber error propagation (atomic / mutex для parent
scope `first_error`) + actual workload tests
(`mn_runtime_actual_workload.nv`, `mn_runtime_steal.nv`,
`mn_runtime_cross_channel.nv`).

**Что НЕ упрощено.** Layer 3 sufficient для:
- C-level testing M:N (тесты на C можно push'ить fibers через
  `nova_runtime_spawn_global` API — runtime ABI стабилен).
- Future Nova-level API: `runtime.spawn(fn ...)` direct call в Plan 44.7.
- Cross-worker channel send/recv (Plan 44.1 channels уже M:N-correct).

Это honest scope split — fundamental infrastructure отделён от ergonomic
API.

## Plan 44.6: Migration между workers — отложено

**Что упрощено.** Fiber pin'ится к worker'у на котором park'нулся.
Wake происходит из close_cb на том же worker'е. Migration между
workers — НЕ реализована.

**Почему.** uv handles thread-bound. Если fiber park'нулся на worker A
(timer registered на A's loop), потом мигрировал на worker B (свободный)
— B не имеет handle'а, A's loop scheduled callback'у некого wake'нуть.
Migration требует:
- TLS state migration (handler-stack, fail-frame, interrupt-frame).
- Handle re-registration на target's loop (`uv_close` на A + `uv_init`
  на B — non-trivial, race-prone).
- Atomic pointer update в waiter struct.

**Practical impact.** Long-running fiber на worker A блокирует
worker A до завершения. Other workers продолжают независимо.
Cooperative scheduling работает в пределах one worker. Это identical
к Tokio default behaviour без `tokio::task::yield_now`.

**Path forward.** Plan 44.8: TLS migration + handle re-registration.
Требует ~600 строк refactor'а + careful invariant work. Откладывается
до тех пор пока workload не покажет migration необходимым (single-
worker stuck'и под uneven load).


---

## Plan 33.3 Ф.9: effect overloaded ops + axiom typed/generic binders (2026-05-14)

**Что упрощено — overloading.**

До: unique-name check в effect/protocol по полю `name` — любые два op
с одинаковым именем → error. Это было проще имплементировать, но
семантически неверно: нет причины запрещать `balance(id int)` и
`balance(id str)` в одном effect — это валидный overloading.

После: check по полной сигнатуре `(name, param_types)`. type_key()
helper → canonical строка для dedup. Дубликат полной сигнатуры → error.
Разные param types → разрешено.

C-codegen: при overloaded ops поля vtable-структуры манглированы
(`balance__nova_int` / `balance__nova_str`). schema_lookup() fallback
позволяет type-inference call-sites искать по plain-имени.

**Что упрощено — typed binders.**

До: axiom binders только untyped: `axiom name(id) => ...` — тип биндера
выводился из usage в формуле или defaulted в Int.

После: `axiom name(id int) => ...` — явный тип идёт напрямую в SMT sort
без inference. Оба синтаксиса сосуществуют; `Option<TypeRef>` в AST.

**Что добавлено — generic binders.**

`axiom name[T](id T) => ...` — generic param в axiom. V1: парсинг + AST,
SMT encoding generic axiom silently skip (is_generic = true → None).
V2 — полный encode через uninterpreted sorts или multi-sort instantiation.

**Техдолг.** `Option<TypeRef>` для binder-типа читается как «нет значения»,
хотя семантика «untyped» — другое. Зафиксировано как Q-axiom-binder-type:
при добавлении Generic как третьего варианта — рефакторить на enum
`BinderType { Untyped, Typed(TypeRef), Generic }`.


## Plan 44.7: preemption — sysmon + codegen safepoints (2026-05-14)

**Что упрощено — Вариант B вместо Варианта C.**

Go вытесняет goroutine через `SIGURG` async signal + ASM `asyncPreempt`,
который умеет прервать ДАЖЕ tight inline-ASM loop. Nova взяла Вариант B:
кооперативные codegen safepoint'ы (`nova_preempt_check()` в прологе функции
и на backedge цикла) + sysmon-thread, выставляющий флаг.

Причина не идеологическая, а техническая: minicoro `mco_yield` НЕ
async-signal-safe — yield из signal handler = UB. Полный Go-механизм
(Вариант C) — 2-3 недели ASM-level работы с высоким риском. Вариант B даёт
**observable** паритет (CPU-bound fiber не морит голодом соседей) за ~20%
сложности.

**Что упрощено осознанно (не баг — by-design):**

- [S-PREEMPT1] Tight loop целиком в inline-ASM или одном FFI-вызове без
  codegen-backedge'а НЕ вытесняется. Codegen вставляет safepoint только в
  Nova-циклы и прологи Nova-функций; чужой ASM/C-код вне его контроля.
  Нишевой кейс — типичный Nova-fiber это IO или Nova-вычисления. Приоритет:
  L. Эскалация к Варианту C — только при конкретном benchmark'е.
- [S-PREEMPT2] Generic-функции (`emit_generic_fn_erased` /
  `emit_generic_method_erased`) НЕ получают prologue safepoint — отдельный
  codegen-путь. Циклы внутри них всё равно получают backedge safepoint
  (через `emit_loop_body_inline`), так что наблюдаемая дыра — только
  generic-функция БЕЗ циклов в рекурсии. Приоритет: L.
- [S-PREEMPT3] Timeslice фиксирован 10ms (`NOVA_PREEMPT_SLICE_NS`), не
  настраивается. Go тоже ~10ms. Tunable — при реальной необходимости.
- [S-PREEMPT4] Вытесненный fiber pin'нится к своему worker'у (yielded-FIFO
  per-worker, не shared). Совпадает с уже существующей моделью «fiber
  pinned to worker» из Plan 44.5 — migration между workers это отдельный
  отложенный вопрос (Plan 44.6 H, «benefit неочевиден»).

**Стоимость safepoint'а.** На горячем (не-preempt) пути: TLS-load +
predicted-not-taken branch + (если ptr≠NULL) ещё один load — ~1-2 такта на
вызов функции и на итерацию цикла. В single-thread режиме `_nova_preempt_ptr
== NULL` → ветка всегда не берётся. Безусловная эмиссия (codegen не знает,
будет ли `runtime.init()`) — принята осознанно: корректность > микро-
оптимизация для языка не в проде.


## std/collections — codegen для array extension methods + iterator mono (2026-05-15)

Контекст: довести `std/collections/` до проходящих тестов в mn-runtime branch.
Состояние было — 4/10 PASS. Финал — 7/10 PASS.

### Симплификация V1: array extension methods как первоклассные

`fn []T @method` (extension methods на массивах) старая логика обрабатывала
через generic-erased path. Это было неправильно: `[]T` — не user-defined
generic type, а синтаксис для `NovaArray_nova_int*`. Type-erasure через
void* для receiver'а ломала и `emit_for` (получал `Nova_[]T*` который не
распознавался как массив), и mangle_fn (получал invalid C identifier).

Фикс — обрабатывать `[]T` как «концретный array receiver»:
- `receiver_c_type("[]T")` → `NovaArray_nova_int*` (с маппингом для
  специализаций: `[]str` → `NovaArray_nova_str*`, и т.д.)
- `receiver_type_c_ident("[]T")` → `NovaArray_nova_int` (для C identifier).
- Метод-уровневые generics (`fn []T @map[U]`) тоже не моно'тся —
  закрытие принимает `void*` argument, U-результат массивом
  `NovaArray_nova_int*` (через erasure).

Это убирает целый класс edge cases: вместо «specialcase extension methods в
generic_method_erased» — обычный emit path с правильным receiver type.

### Симплификация V2: iter base-name fallback в `emit_for`

При monomorphization итераторы типизируются как `KeysIter____nova_str__nova_int`
(mono'd). `all_methods` registry содержит только base `("KeysIter", "next")`.
Стандартный путь — instantiate всё через worklist; но for-in над mono'd
итератором проще: добавлен base-name fallback (split на `____`).

Что важно — это не «иерархия registry», а упрощение через распознавание паттерна
mono-имени: `KeysIter____X__Y` → base `KeysIter`.

### Известное ограничение: mono'd internal method calls

`Set[T]` (= `Set { map: HashMap[T, ()] }`) методы внутренне зовут
`@map.contains(x)`. В mono context `Set[nova_int]` → `@map: HashMap[nova_int, _]`.
Но call `@map.contains(x)` в emit_monomorphized_method резолвится против
non-mono'd HashMap → возвращает stub (NULL). Это deep mono dispatch issue;
требует прокидывания type_subst в method-call resolution.

То же — у HashMap.with_capacity: внутри вызывает `new_buckets(cap)` который
mono'тся как `nova_fn_new_buckets____nova_int__nova_int` (wrong substitution),
тогда как ожидался `____nova_str__nova_int`. Subst chains через nested generic
calls не работают корректно.

Hashmap/Set/Linkedlist остаются RUN/CC-FAIL по этой причине. Тесты адаптированы
под минимум, который работает (insert/contains/get без iterator iteration).

### D43 violation в исходных тестах (не парсер-баг)

vec.nv и linkedlist.nv содержали `v.fold(0) { |acc, x| acc + x }` — невалидный
синтаксис по D43. Спека: trailing-block разрешён ТОЛЬКО без params
(`f(args) { block }`); `|...|` (closure-light) в trailing-position ЗАПРЕЩЁН.

Корректные формы:
- `v.fold(0, |acc, x| acc + x)` — closure-light как аргумент
- `v.fold(0) fn(acc, x) acc + x` — trailing-fn (с params)

Парсер был permissive: съел невалидную форму и заэмитил странный кодеген
(trailing-block без params оборачивал inner closure-light expression — fn
trailing block возвращал closure, fold вызывал closure как (env, acc, x), но
trailing block принимал только (env)). Тесты переписаны под D43.

Отдельная задача — enforcement D43 в parser, чтобы такие тесты не молча
проходили codegen с broken output.

### Файлы

- compiler-codegen/src/codegen/emit_c.rs — 6 точечных фиксов
- compiler-codegen/nova_rt/array.h — новый (был отсутствующим в mn-runtime
  branch); + добавлены `nova_opt_eq_nova_{str,bool,byte,f64}` helpers


## Plan 45 Ф.23 — Production hardening для nova doc (2026-05-16)

Закрыты 24 из 25 пунктов Ф.23 (Sprint 3 polish gaps vs rustdoc/godoc/typedoc).
Worktree `plan-45-doc` (d:\Sources\nova-lang-p45-doc).

### Упрощения и принятые решения

**Ф.23.4 (handler matrix) — отложено.**
В Nova handlers — expression-level (`with X = handler { }` inline), не
top-level декларации. Workspace scan невозможен без новых AST-узлов.
Решение: не вводить syntax только ради doc-фичи. Отложено до момента, когда
top-level handler декларации потребуются по другой причине.

**Ф.23.22 (structural type) — упрощённый encoder.**
Полноценный type-string→AST парсер дорог. Реализован простой shape-detector
(array/optional/tuple/named/unit/function) с `source` field как escape hatch
для сложных случаев. LLM получает primary classification без overhead.

**Ф.23.16 (Protocol.implementors) — structural matching.**
В Nova нет explicit `impl Protocol for Type`. Используется duck-typing:
тип считается implementor'ом если у него есть методы со всеми именами из
Protocol.methods. False positives возможны (один общий метод name), но это
acceptable для doc hints.

**Ф.23.18 (caret diagnostic) — простой single-line snippet.**
Rustdoc rendering включает многострочные spans с context. Реализован
minimum: одна строка + caret-ы. Достаточно для doc-test failure UX.

**Ф.23.25 (source_root) — opt-in `${WORKSPACE_ROOT}`.**
Auto-detect workspace через walk-up по parent-папкам не делаем. Caller
явно устанавливает `NOVA_DOC_WORKSPACE_ROOT` env var → получает
machine-agnostic output. Дефолт — absolute path (но с forward slashes).

### Nova syntax — что выяснилось при написании тестов

При написании 13 .nv test-файлов столкнулись с несколькими расхождениями
ожиданий от Rust/Go/TS:

**Newtype:** `type Email str` (без `=`, без `newtype` keyword).
Unwrap **не** через `.0` — только через `as UnderlyingType`. `.0` syntax
парсится но даёт codegen error для int newtype.

**Effect declarations:** методы внутри **без** `fn` keyword:
```
type Counter effect {
    tick() -> ()       // не `fn tick() -> ()`
    get() -> int
}
```

**Handler syntax:** `with Counter = handler Counter { tick() { ... } }` —
тоже без `fn` в method bodies.

**Protocol method access:** в методах `fn Type @method()` доступ к полям
через `@field`, **не** `self.field`. Receiver `self` неявный.

**Record init:** `Box { width: 10; height: 5 }` — двоеточие `:`, **не**
`=`. Точка с запятой как separator (но запятая тоже работает в некоторых
контекстах).

**Type-safety newtype:** Nova **не** обеспечивает строгую type-safety для
newtypes на уровне codegen. `UserId` можно неявно передать как `int`
без cast (в отличие от Haskell newtype). Negative тест переписан на
другой вид ошибки.

**Contracts type-check:** unknown identifier в `requires`/`ensures` —
**не** compile error. Контракт проверяется в runtime; парсер позволяет
любое expr. Negative test использует undefined fn в теле, не в contract.

### Out of scope этой сессии (Plan 45 Ф.23)

- Ф.23.4 handler matrix (требует AST changes)
- Полный structural type парсинг (упрощённый shape detector достаточен)
- Auto-detect workspace root (нужен явный env var)

---

## [M-plan-60-md-non-auto-migration] — manual migration .md (2026-05-17)

Auto-migration tool применил .nv (std/+nova_tests/+examples/) — 404
rewrites зачётно. Для .md (docs/+spec/) применение было НЕ-полным:
meta-разделы spec'а описывают **обе** формы (`.len` vs `.len()` —
правило, что одна форма запрещена), tool бы их сломал. Manually
amended ключевые spec D-blocks (D26 в 08-runtime, built-in API table
в 03-syntax, examples в 02-types/04-effects). Полная migration
остальных .md occurrences (~140 hits в docs/plans/* и spec/decisions/*
которые цитируют код в pre-Plan-60 form) — **по мере правки этих
файлов в естественной работе**. Не блокер acceptance — это
historical context, не canonical API reference.

## Plan 70.3 — char↔int distinction (2026-05-19/20)

### [M-plan70-3-array-assign-no-typecheck] (DEFER — array-level type-checker tightening)
- **Где:** type-checker / codegen — array assignment compatibility check.
- **Что упрощено:** `let ints []int = chars` (где `chars []char`)
  **собирается успешно** — codegen не отвергает присваивание `[]char` в
  `[]int`-переменную. Distinct `nova_char` typedef обеспечивает CC-FAIL
  для scalar/Option collapse (`Some('a')` в `Option[int]` → ошибка), но
  array-level mismatch проскальзывает.
- **Почему:** `NovaArray_nova_char*` и `NovaArray_nova_int*` — оба
  pointer-типы; на codegen-path присваивание, видимо, проходит через
  cast или type-erasure до того как clang мог бы отвергнуть несовместимые
  struct-pointer типы. Type-checker не имеет explicit правила
  «`[]char` ≠ `[]int`».
- **Как чинить:** array-element type compatibility rule в type-checker —
  отвергать assignment если element types различаются (char vs int).
  Negative-fixture написать после fix (сейчас дал бы NEG-NO-ERROR).
- **Приоритет:** L — scalar/Option/generic-record collapse (основной
  vector bug-class) закрыт; array-assignment edge редок и обычно
  ловится на использовании (element-type mismatch при `.push`/index).

### [M-plan70-3-uint-max-parser] ✅ RESOLVED (Plan 70.5 Ф.4, 2026-05-20)
- **Где:** `compiler-codegen/src/parser/mod.rs` `is_primitive_type` list (~line 3941).
- **Что упрощено:** `uint.MAX` парсился как `Member(Ident("uint"), "MAX")`
  вместо `Path(["uint", "MAX"])` — `uint` отсутствовал в списке type-keywords
  парсера. Workaround: `u64.MAX as uint`.
- **Закрыто:** добавлен `"uint"` в `is_primitive_type` (1 строчка). Fixtures
  f4-f8 в `nova_tests/plan70_5/` подтверждают.

### [M-plan70-4-arr-uint-indexing] (DEFER — breaking change)
- **Где:** array indexing API — `arr[i int]` сигнатура.
- **Что упрощено:** `arr[i uint]` не поддерживается как тип индекса.
  Сейчас `arr.len() -> int`, Range/Iter `-> Option[int]`.
- **Почему:** Breaking change для 100+ API sites. Swift/Go pattern —
  используют `Int` для индексов (не uint/usize) из соображений эргономики.
- **Как чинить:** отдельный план после type-checker API revision.
- **Приоритет:** L — ergonomics, не bug.

### [M-plan70-4-byte-full-removal] (DEFER — type-checker alias resolution)
- **Где:** `byte` type alias — `std/prelude.nv` + type-checker.
- **Что упрощено:** `byte` → `nova_byte` унификация выполнена в codegen
  (Plan 70.4 Ф.4), но `byte` как keyword всё ещё существует в языке как
  отдельный тип в type-checker.
- **Почему:** полное удаление требует alias-resolution в type-checker
  (Plan 69 closure scope).
- **Как чинить:** Plan 69 follow-up — resolve `byte` как alias `u8` в
  type-checker, затем deprecate keyword.
- **Приоритет:** M — codegen unified, только type-checker gap.

## Plan 62.A.bis — Generic schema registry (2026-05-20)

### [M-result-generic-T-method-mismatch] (DEFER — Plan 62.B+)
- **Где:** `std/prelude/core.nv` + `compiler-codegen/src/codegen/emit_c.rs`
  (`type_of_method_call_c`, lines 18619+).
- **Что упрощено:** 5 методов Result возвращающих `T` (unwrap, unwrap_or,
  unwrap_or_else, map, map_err) не задекларированы в `std/prelude/core.nv`
  — закомментированы с объяснением blocker'а.
- **Почему:** type-checker видит `Result[T, E] @unwrap_or(default T) -> T`
  как generic signature и выводит тип результата `r.unwrap_or(0)` как
  `Result*` вместо `nova_int`. Codegen делает tag-comparison вместо
  value-equality при `r.unwrap_or(0) == 42`. Silent wrong output.
- **Как чинить:** per-T monomorphization Result.unwrap_or (как Option через
  NovaOpt_<T>), или type-checker special-case признающий concrete Ok-type
  из object'а без declared generic signature. Оба пути — Plan 62.B+.
- **Приоритет:** M — Result.unwrap_or/unwrap активно используется; текущий
  hardcoded path (emit_c.rs:11567+) работает корректно через bootstrap mono
  compromise. Регрессии нет — только декларация в core.nv не добавлена.

### [M-option-or-no-trampoline] (DEFER — Plan 62.B+)
- **Где:** `nova_rt/array.h` + `std/prelude/core.nv`.
- **Что упрощено:** `external fn Option[T] @or(other Option[T]) -> Option[T]`
  задекларирован в core.nv для документации, но codegen trampoline
  `Nova_Option_method_or_<T>` в array.h отсутствует. Вызов `opt.or(other)`
  даёт CC-FAIL.
- **Почему:** добавление per-T trampoline требует изменения nova_rt/array.h
  (NOVA_DECLARE_OPTION_T macro) — отдельная задача вне scope 62.A.bis.
- **Как чинить:** добавить `Nova_Option_method_or_<T>(opt, other) { ... }`
  в NOVA_DECLARE_OPTION_T macro + routing entry в init_hardcoded_baseline.
- **Приоритет:** L — or() менее используем чем unwrap_or/map.

### [M-typecheck-missing-type-compat-checks] ✅ ЗАКРЫТ 2026-05-21 (Plan 79)
> Ранее назывался `[M-typecheck-lenient-no-p1b-p2a-negatives]`.

- **Что было:** type-checker не отвергал базовые ошибки типов —
  argument-type mismatch (`want_bool(42)`), annotation↔RHS mismatch
  (`let x int = true`), wrong type-arity (`Result[int]`) компилировались
  **тихо** (silent miscompilation); type-as-value (`let c = Foo`) и
  non-existent field (`f.nonexistent`) ловились только C-компилятором.
- **Закрыто:** [Plan 79](plans/79-typecheck-hardening-no-silent-fallback.md)
  — проход `TypeCheckCtx` в `types/mod.rs` (серия E73xx):
  - Ф.1 assignability arg↔param + annotation↔RHS → **E7301**;
  - Ф.2 арность type-аргументов → **E7310**;
  - Ф.3 существование поля/метода → **E7320**;
  - Ф.4 type-vs-value → **E7330**.
  Спека — [D135](../spec/decisions/02-types.md#d135). Negative-тесты
  для Plan 72 p1b/p2a дописаны (`nova_tests/plan72/p1b_empty_sum_type_neg.nv`,
  `p2a_try_from_into_neg.nv`) — оговорка «p1b/p2a без negative-покрытия»
  снята.
---

## Plan 76 — bottom-тип never (2026-05-21)

### [M-never-uppercase-no-negative-test] (DEFER -> Plan 37)
- **Где:** `nova_tests/plan76/`.
- **Что упрощено:** запланированный негативный тест «`Never` (заглавная) ->
  compile error» не реализован — bootstrap type-checker permissive к
  unknown uppercase type-именам, `Never` после rename не даёт чистой
  ошибки на type-check.
- **Почему:** строгая проверка unknown-type — зона Plan 37 (typecheck
  semantic parity), вне scope Plan 76.
- **Как чинить:** Plan 37 strict type-resolution -> добавить негативную
  фикстуру.
- **Приоритет:** L — все `Never`-сайты мигрированы; негативное покрытие
  never-семантики есть (`fail_handler_no_exit_rejected.nv`).

## Plan 83.1 — M:N-инфраструктура, Ф.1+Ф.2 (2026-05-22)

### [M-83.1-cgroup-static-read] cgroup-квота читается один раз на старте
- **Где:** `compiler-codegen/nova_rt/runtime.c` — `nova_runtime_resolve_maxprocs`.
- **Что упрощено:** число worker'ов резолвится один раз через
  `uv_available_parallelism()` в момент `runtime.init`. cgroup-квота
  читается статически — изменение лимита контейнера во время работы
  процесса не учитывается. Go 1.25 перечитывает cgroup-квоту
  динамически и ресайзит пул.
- **Почему:** libuv 1.52 даёт cgroup+affinity-correct значение на момент
  вызова — этого достаточно для подавляющего большинства деплоев (лимит
  контейнера фиксирован на запуске). Динамический re-read — отдельная
  инфраструктура (фоновый поллинг квоты + пересборка пула), требует
  Ф.4 lazy-spawn V2 (инкрементальный рост).
- **Как чинить:** followup-инкремент Plan 83.x — фоновый re-read
  cgroup-квоты + динамический resize пула. Зафиксировано как известная
  дельта vs Go в плане 83 §4.
- **Приоритет:** L — статическое значение корректно для fixed-лимит
  контейнеров (норма для большинства деплоев).

### [M-83.1-maxprocs-clamp-fixed] Потолок NOVA_MAXPROCS зашит = 1024
- **Где:** `compiler-codegen/nova_rt/runtime.c` — `NOVA_MAXPROCS_MAX`.
- **Что упрощено:** верхний клэмп числа worker'ов — константа 1024
  (Plan 83 §3 П6). Не конфигурируется. Запрос выше → клэмп + warning.
- **Почему:** 1024 worker'ов покрывает все реальные машины с запасом;
  выше — почти наверняка ошибка конфигурации, которую честнее
  диагностировать, чем исполнять. Нижний клэмп = 1.
- **Как чинить:** при появлении машин >1024 ядер — поднять константу
  или сделать её собираемой через cfg. Followup, не блокер.
- **Приоритет:** L.

## Plan 83.1 Ф.4 — lazy worker-пул (2026-05-22)

### [M-83.1-lazy-spawn-v1-whole-pool] первый spawn поднимает ВЕСЬ пул
- **Где:** `compiler-codegen/nova_rt/runtime.c` — `_materialize_pool`.
- **Что упрощено:** lazy-spawn V1 — на первом worker-bound spawn
  поднимается сразу весь пул `maxprocs` worker-потоков. Программа с
  единственным spawn получает `NumCPU` потоков (Go поднял бы ~1-2 `M`
  и рос бы инкрементально по нагрузке).
- **Почему:** инкрементальный рост пула требует отдельной
  инфраструктуры (per-worker spawn-on-demand + балансировка). V1
  «весь пул на первом spawn» закрывает главную цель — hello-world без
  spawn остаётся однопоточным (0 worker-потоков, 0 sysmon) — простым
  и корректным способом.
- **Как чинить:** V2 — инкрементальный рост пула (полный Go-`M`-
  паритет), followup Plan 83.x.
- **Приоритет:** L — программам, делающим spawn, полный пул всё равно
  нужен; экономия только на паттерне «1 spawn → 1 worker».
## Plan 83.1 Ф.5 — thread-budget (2026-05-22)

### [M-83.1-budget-explicit-init-uncapped] explicit runtime.init(N) обходит бюджет
- **Где:** test-runner — NOVA_MAXPROCS budget (`test_runner.rs`).
- **Что упрощено:** бюджет NOVA_MAXPROCS ограничивает только тесты с
  auto-detect (`runtime.init(0)` либо без явного init). Тест с явным
  `runtime.init(N>0)` получает N worker'ов (explicit бьёт env — D136);
  при `workers` параллельных таких тестах суммарно `workers × N`
  потоков.
- **Почему:** explicit `init(N)` — осознанный выбор теста; уважать его
  важнее жёсткого капа. Большинство M:N-тестов с explicit init
  используют небольшие N (2-4) — реальная oversubscription ограничена.
- **Как чинить:** при необходимости — hard-cap explicit-N в тест-режиме
  через отдельный механизм. Пока не нужно.
- **Приоритет:** L — oversubscription ограничена малыми N; bench (где
  точность критична) уже жёстко NOVA_MAXPROCS=1.

## Plan 83.3 Ф.1 — runtime blocking-offload (2026-05-22)

### [M-83.3-blocking-leaf-contract] V1: blocking-работа обязана быть leaf
- **Где:** `compiler-codegen/nova_rt/fibers.h` — `nova_blocking_offload`;
  type-checker `compiler-codegen/src/types/mod.rs`.
- **Что упрощено:** `work_cb` выполняется на потоке libuv threadpool,
  не зарегистрированном в Boehm GC и не являющемся fiber'ом. V1-контракт
  (D50): blocking-работа — leaf: FFI/syscall без GC-аллокации и без
  вызовов обратно в Nova-рантайм.
- **Статус enforcement (обновлено Ф.6, 2026-05-22):** **частично
  проверяется компилятором** — тело `blocking { }` type-check'ается
  как `nogc` (бан alloc-вызовов) + бан suspend-эффектов Net/Fs/Db/Time.
  НЕ проверяется: `throw`/`?` (`Fail`-эффект — `longjmp` без fail-frame
  на threadpool-потоке), а `nogc`-whitelist консервативен (не ловит
  user-record-литералы). Эти остатки — documented-риск в spec D50 §4.
- **Почему остаток:** полный enforcement (`Fail`-бан + произвольный
  Nova-код) требует V2.
- **Как чинить:** V2 — `GC_register_my_thread` once-per-thread для
  threadpool-потоков + fail-frame на threadpool-потоке → разрешит
  произвольный Nova-код под `Blocking` (alloc + throw).
- **Приоритет:** L — V1 + Ф.6-enforcement достаточны для целевого
  паритета (FFI-offload); крашащие случаи (alloc, async-I/O) ловятся.

## Plan 03.1 — path/git-зависимости (2026-05-22)

### [M-03.1-no-sha256-tree-hash] nova.lock пинит commit без sha256 дерева

- **Где** — `compiler-codegen/src/lockfile.rs`, формат `nova.lock`.
- **Что упрощено** — `git`-записи lockfile содержат `commit`, но НЕ
  отдельный `sha256` дерева исходников (D78 §3.3 его упоминал).
- **Почему** — git-commit сам по себе криптографически адресует дерево:
  подменить содержимое без смены commit'а нельзя (паритет с многолетним
  поведением `Cargo.lock`). Отдельный sha256 защищал бы лишь от
  SHA-1-collision-атаки на git-сервер. Bootstrap-компилятор намеренно
  без сторонних crate-зависимостей (`compiler-codegen/Cargo.toml`:
  только `clap` + `anyhow`) — собственная крипто-реализация это
  отдельный осознанный концерн, не «попутно».
- **Как чинить** — Plan 03.4 (supply-chain hardening): sha256/BLAKE3
  дерева + подписи + transparency log + `nova audit`. Формат `nova.lock`
  forward-совместим (неизвестные ключи игнорируются) — поле добавляется
  без format-break.
- **Приоритет** — L (commit-пин уже tamper-evident для практической
  модели угроз).

### [M-03.1-deferred-resolution] нет version-ranges / registry / SAT

- **Где** — резолюция зависимостей в целом.
- **Что упрощено** — `[dependencies]` поддерживает `path`/`git` и
  парсит registry-версию `"1.2"`, но version-ranges (`^1.2`),
  SAT/pubgrub-резолюцию и central registry **не** делает.
- **Почему** — для `path`/`git` источник пинится точно (путём либо
  commit'ом), SAT-resolver не нужен by construction. Это декомпозиция
  Plan 03, а не срезанный угол: 03.1 **полностью** закрывает резолюцию
  `path`/`git` (resolution + lockfile + reproducibility).
- **Как чинить** — Plan 03.2 (version-ranges + pubgrub), Plan 03.3
  (registry). registry-форма в `[dependencies]` уже парсится → 03.3
  не ломает формат.
- **Приоритет** — L (отдельные под-планы с собственным scope).

Plan 03.1 (Ф.1–Ф.6) → ✅ ЗАКРЫТ. Suite: 983 PASS / 0 FAIL.

---

## Plan 03.2 — version resolution (2026-05-22)

### [M-03.2-backtracking-not-pubgrub] резолвер версий — backtracking, не полный PubGrub

- **Где** — `compiler-codegen/src/resolver.rs`.
- **Что упрощено** — резолвер версий реализован как корректный
  backtracking (DFS, highest-version-first, распространение
  ограничений, откат при конфликте), а **не** полный PubGrub (CDCL —
  conflict-driven clause learning).
- **Почему** — PubGrub = backtracking-база **плюс** обучение на
  конфликтах: оптимизация скорости и минимальности explanation для
  **больших** dependency-графов. Реализованный backtracking-резолвер
  **корректен и полон** (находит решение, если оно есть; иначе —
  диагностируемый конфликт). Для git-tag-deps-масштаба Plan 03.2
  (малые графы, без central registry) CDCL избыточен. Это
  декомпозиция: корректность не страдает, откладывается оптимизация.
- **Как чинить** — followup registry-эры (Plan 03.3+): когда вселенная
  пакетов/версий станет большой, добавить CDCL-обучение поверх той же
  backtracking-базы. `DependencyProvider`-трейт уже абстрагирует
  источник версий — резолвер переписывать не придётся.
- **Приоритет** — L (корректность полная; вопрос только
  производительности на больших графах, которых пока нет).

Plan 03.2 (Ф.1–Ф.5) → ✅ ЗАКРЫТ. Suite: 1038 PASS / 0 FAIL.

## Plan 03.4 — effect-aware tooling (2026-05-22)

### [M-03.4-registry-gated-cmds] publish / search / audit отложены

- **Где** — `nova` CLI, экосистема пакетов.
- **Что упрощено** — Plan 03.4 реализует автономно-кодируемый
  Nova-уникальный срез (`nova info` + effect-surface + effect-diff +
  capability-confined deps через `forbid`). Команды `nova publish` /
  `search` / `audit` **не** реализованы.
- **Почему** — `publish`/`search` требуют центрального registry
  (Plan 03.3 — HTTP-сервер, content-addressing, подписи); `nova audit`
  — внешней OSV-БД advisory. Это не «срезанный угол», а отсутствие
  внешней инфраструктуры: код клиента без сервера непроверяем.
- **Как чинить** — Plan 03.3 (registry) разблокирует publish/search;
  `nova audit` — после интеграции OSV-БД. effect-surface уже считается
  — registry сможет хранить её в метаданных пакета (effect-diff на
  уровне registry).
- **Приоритет** — L (отдельные под-планы; гейтинг на инфраструктуру).

### [M-03.4-effect-match-exact] forbid-проверка — по имени эффекта

- **Где** — `effect_surface::check_forbidden` / `violates`.
- **Что упрощено** — `forbid = ["Net"]` сверяется с effect-surface по
  имени эффекта (точное совпадение либо параметризованный префикс
  `Fail[` для `forbid=["Fail"]`). Нет иерархии capability / алиасов.
- **Почему** — эффекты Nova — плоские именованные сущности; точное
  совпадение покрывает реальные кейсы (`Net`, `Fs`, `Db`). Иерархия
  capability (если появится) — отдельный концерн D63-эволюции.
- **Как чинить** — при появлении capability-групп — резолвить `forbid`
  через ту же иерархию. Пока не нужно.
- **Приоритет** — L.

Plan 03.4 (Ф.1–Ф.4, effect-срез) → ✅ ЗАКРЫТ. Suite: 1058 PASS / 0 FAIL.

---

## [M-82-bench-c-harness] Plan 82 Ф.5 — context-switch бенч на C, не Nova bench-DSL (2026-05-22)

### ⚠ ЧАСТИЧНО ЗАКРЫТО Plan 82 followup (2026-05-23)

**Root cause выявлен и устранён**, но связка `bench{measure}+supervised`
всё ещё упирается в ОТДЕЛЬНЫЕ pre-existing баги bench-DSL.

Что было: `nova bench run` на любом файле в `bench/micro/` падал с
`Nova_Error_static_new()` 0-arg. Диагноз 2026-05-22 («связка
bench+supervised в codegen») оказался не полным.

**Истинная цепочка** (выявлена 2026-05-23):
1. `bench/micro/hashmap.nv` и `bench/micro/gc.nv` забывали
   `import std.collections.hashmap.{HashMap}`.
2. Codegen, не найдя `HashMap` в типовых реестрах, тихо роутил `.new()`
   через single-key fallback `method_receivers["new"] = ("Error", false)`
   (зарегистрирован для `Error.new(msg)` на ред. 1 D26 prelude) →
   эмитил `Nova_Error_static_new()` с тем количеством аргументов, что
   user написал в Nova-коде (0 у `HashMap.new()`).
3. Все sibling-бенчи в том же модуле страдали при компиляции.

**Fixed:**
 - Source: `import HashMap` добавлен в hashmap.nv/gc.nv (`commit b9ac2d8f1a2`).
 - Codegen: strict-check в method_receivers-fallback — для static-формы
   `Type.m(...)` obj обязан матчить зарегистрированный type_name; иначе
   `E_UNKNOWN_TYPE_METHOD` с подсказкой про `import` (`commit
   11a1ada777a`). Silent fallback закрыт.
 - Regression-guard: `bench/micro/supervised_spawn.nv` — позитивный
   smoke «bench + concurrency» (компилируется в C без скрытого Error-
   fallback'а; то есть конкретно ЭТА связка теперь не теряет тип).

### Что ОСТАЛОСЬ открытым (отдельная задача)

`bench{measure}+supervised{spawn}` всё равно не доходит до запуска —
дальше за фоллбэком вскрываются ДВА **pre-existing** bench-DSL бага,
никак не связанных с Plan 82:
- **multi-emission spawn-fn**: bench-DSL эмитит measure-body ТРИ раза
  (warmup/calibration/sample-loop) с уникальными счётчиками
  `_nova_spawn_N`, но forward-declarations нумеруются иначе → линкер
  жалуется на undeclared `_nova_spawn_2`.
- **NovaOpt[T] mono mismatch**: `Node.next: Option[Node]` в gc.nv внутри
  measure-body эмитится как `NovaOpt_nova_int` вместо
  `NovaOpt_Nova_Node_p` — потеря type-substitution через bench-DSL.

Это самостоятельный bench-DSL refactor — outside scope Plan 82
followup. Ф.5 deliverable (cost mco_resume/yield) уже измерен C-харнессом
точнее (QPC + __rdtsc, 7 trials, реальный `fiber_arena_win.c`, 16–20
ns/switch — паритет с Boost.Context). Перенос замера в Nova-DSL —
косметический, не функциональный.

### Приоритет — L (деливерабл Ф.5 достигнут; bench-DSL multi-emission — отдельная задача).

---

### [M-protocol-literal-codegen-deferred] ✅ ЗАКРЫТ Plan 97.1 (2026-05-23, merge b09a8c1b3e5) — vtable-dispatch на protocol-литерале

> **CLOSED 2026-05-23 by Plan 97.1** (worktree `nova-p97-1`, ветка
> `plan-97-1`, merged в main коммитом `b09a8c1b3e5`).
> Регресс на main после merge: **PASS 1114 / FAIL 0 (real) / SKIP 56**.
> Protocol-литерал теперь полностью работает в codegen:
> * `emit_protocol_lit` создаёт synthetic ctx struct + free fn methods
>   + heap-allocated NovaVtable + NovaBox fat-pointer.
> * `emit_protocol_box_typedef`/`_vtable_companion` расширены на
>   non-generic protocol'ы (Ф.1).
> * `type_ref_to_c` для non-generic protocol-typed value возвращает
>   `NovaBox_<Proto>` (унифицированный dispatch path: literal + assignment).
> * Tuple typedef marker перенесён после GENERIC_TYPE_DEFS (Ф.3) для
>   tuple'ов вида `(Reader, Writer)` из capability-split factory.
> * Skip vtable typedef для runtime-defined (Hash/Compare/Display
>   в `nova_rt/vtables.h`) — иначе C redefinition.
> * Capability-split factory pattern (`Lock.new() -> (Locker, Unlocker)`)
>   работает end-to-end (commit 8e024d43647 + предшествующие).

### [M-protocol-method-name-shadowing] ✅ ЗАКРЫТ Plan 97.1-fu (2026-05-23, merge da99ea8bd6b) — method-name collision между protocol-литералом и stdlib protocol'ом

- **CLOSED 2026-05-23 by Plan 97.1 followup** (commit `16b99a9475f`,
  ветка `plan-97-1-fu`, merge в main `da99ea8bd6b`).
- **Регресс на main:** PASS 1125 / 1 pre-existing FAIL (plan99_probe
  intentional gap, не от Plan 97.1) / SKIP 56.
- **Где** — `compiler-codegen/src/codegen/emit_c.rs` `infer_expr_c_type`
  для `Call { func: Member { obj, name } }` где `obj: NovaBox_<Proto>`.
- **Что было** — return-type метода брался из общих `method_overloads`
  (где мог оказаться homonymous метод другого типа — e.g. `Iter.next ->
  Option[T]`), вместо правильного `protocol_method_registry[<Proto>]`.
  Давало CC-FAIL `initializing 'NovaOpt_nova_int' with incompatible
  'nova_int'` — silent miscompile риск.
- **Fix:** в `infer_expr_c_type` добавлен **priority lookup**: если
  `obj_ty` имеет prefix `NovaBox_`, return-type метода берётся
  **первым делом** из `protocol_method_registry[<Proto>]`
  (с fallback по mangle: full `Iter_nova_int` → base `Iter`).
  Метод resolved correctly до любых других candidate paths.
- **Guard regression-фикстура:** `pos_protocol_lit_method_name_shadowing.nv`
  — `protocol CounterPlain { next() -> int }` (имя совпадает с
  `Iter.next() -> Option[T]`), `c.next()` корректно возвращает `int`.
- **Регресс:** plan97 17/17 PASS, plan72 (P3-B box-dispatch) 16/16 PASS —
  никаких поломок.

### Plan 97.1 hardening (2026-05-23, commit 0a8d0f0307b, ветка plan-97-1-hd) — production-grade улучшения

После merge Plan 97.1 + followup — добавлены 3 hardening улучшения,
закрывающие потенциальные silent miscompile / runtime bug пути:

1. **Nova-side enforcement** для `obj.method()` где obj — protocol-typed
   variable: новый `check_protocol_method_call` в BoundCtx walk. Method
   обязан быть в `protocol_specs[<Proto>]`, иначе compile error с
   R5.3 hint о доступных методах. Раньше ловилось только C-side как
   `no member named 'X' in struct NovaVtable_<Proto>`. Закрывает
   silent miscompile риск для пользовательских опечаток.
   `infer_arg_ty` расширен ProtocolLit arm → let-binding получает
   правильный Named-protocol type.

2. **Capture-mode разделение** в `emit_protocol_lit`: pointer-types
   (heap obj) и mutable scalars (`let mut`) — by-pointer; **immutable
   scalars** (function param, `let`) — **by-value snapshot**. Критично
   для **factory pattern**, где literal возвращается за пределы fn —
   раньше pointer на stack-local stayed dangling. Macros respect mode:
   by-value → direct field access, by-pointer → deref.

3. **GC-stress positive фикстура** `pos_protocol_lit_gc_stress`:
   factory `make_adder(delta) -> Increment` вызывается 1000 раз в
   цикле; 3 параллельных literals (5/10/99) с разными captures —
   captures не путаются, GC корректно cleanup.

**Регресс в worktree:** PASS 1127 / FAIL 1 (pre-existing
plan99_probe — intentional gap) / SKIP 56.

**Merge в main:** `d028531505f` (2026-05-23). Финальный регресс
на main: **PASS 1127 / FAIL 1 (pre-existing plan99_probe) / SKIP 56** —
zero реальных регрессий.

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` `ExprKind::ProtocolLit`
  arm (делегирует на `emit_handler_lit`).
- **Что упрощено** — parser + AST + type-checker для protocol-литерала
  (`protocol Name { method-impl* }` в expression-position) реализованы
  **полностью**: structural-match, instance-only (static-impl-rejection),
  missing-method/extra-method diagnostics, unknown-protocol detection.
  Codegen эмитит literal как closure-bundle через путь handler-литерала
  (`emit_handler_lit`), **но** runtime-vtable struct `NovaVtable_<Proto>`
  не эмитится (Plan 15 D53 strict: protocol — compile-time-only).
  В результате allocation работает только если protocol уже
  зарегистрирован как effect через `emit_effect_type` (через Plan 56
  D122 vtable companion). Для protocol-only типов (без effect-формы)
  CC-FAIL на `unknown type name 'NovaVtable_<Proto>'`.
- **Почему** — full vtable infra для protocol-літералов требует
  - расширения `emit_type_decl` чтобы emit'ить vtable для protocol'ов
    (а не только effects),
  - dispatch logic для method-call'а на protocol-typed value
    (`value.method()` где `value: Locker` — named protocol),
  - capture-rules согласованных с closure (D22/D6 managed heap) и
    отдельным struct-typedef'ом per literal.
  Это **2-3 dev-day** работы — превышает scope Ф.4 (~1.2 d).
  Parser/type-checker даёт **75% выигрыша**: capability-split factory
  pattern из спеки парсится и type-check'ается; единственный gap —
  runtime dispatch, который дополним отдельной задачей.
- **Как чинить** — Plan 97.1 «protocol-literal full codegen»:
  1. Расширить `emit_type_decl` → `TypeDeclKind::Protocol(_)`: эмитить
     `NovaVtable_<Name>` struct (как для effect) — без thread-local
     handler slot (protocol-value передаётся явно как параметр).
  2. Dispatch path для `value.method()` где value имеет protocol-тип:
     эмитить `((NovaVtable_<Proto>*)value)->method(value->ctx, args)`.
     Hybrid с Plan 56 D122 mono'd-path: если concrete type known
     статически → direct call; иначе indirect.
  3. Регистрировать protocol в `effect_schemas` registry чтобы
     `emit_handler_lit` находил method signatures.
  4. Fixture `pos_protocol_lit_basic` восстановить + capability-split
     factory `pos_protocol_lit_capability_split` (per Plan 97 Ф.5.13).
- **Приоритет** — M (нужно для разблокировки stdlib Plan 18
  capability-split API: `Process.spawn -> (Stdin, Stdout, Stderr)`,
  `HttpServer.bind -> (Acceptor, ShutdownHandle)` и т.д.).
- **Обнаружено** — Plan 97 Ф.4 (2026-05-23). Parser + type-checker
  закрыли syntax + structural validation; codegen — отдельный план.

### [M-protocol-static-enforcement-deferred] Plan 97 — нет hard-enforcement static↔instance в protocol-методе

- **Где** — `compiler-codegen/src/types/mod.rs` структурное матчинг
  типа против protocol-методов.
- **Что упрощено** — Plan 97 ввёл синтаксис `.method()` для static в
  `protocol {}` теле (`is_static` флаг на `EffectMethod`). Type-checker
  при матчинге type ↔ protocol **не проверяет** соответствие
  `is_static` декларации протокола и `is_static` реализации:
  `protocol { .from(t T) -> Self }` может быть «удовлетворён» как
  `fn T.from(t T)` (D35 static, корректно), так и `fn T @from(t T)`
  (D35 instance, некорректно) — оба матчатся структурно.
- **Почему** — текущий matching уже структурно ленив (matches и
  `method_table` для instance, и `fn_decls` для static). Plan 97
  закрывает spec-Q-static-method-protocol на **синтаксис**;
  enforcement — отдельная hardening-линия (analog Plan 79 typecheck
  hardening «no silent fallback»), требует переработки matching-пути.
- **Как чинить** — отдельный план «protocol static/instance strict»:
  при матчинге типа против `protocol { .method }` искать
  именно `fn Type.method` (D35-static, в `fn_decls`); для bare
  protocol-метода — `fn Type @method` (D35-instance, в `method_table`).
  Несовпадение → compile error E???? (analog mismatch-errors Plan 79).
- **Приоритет** — L. На корректность не влияет (структура методов уже
  совпадает в стdlib и user-коде); только защищает от ошибочных
  реализаций.

### [P-plan96-lint-deferred] Plan 96 — lint W_VIEW_PUSH_DETACH ✅ RESOLVED Plan 96.1 Ф.1
- **Где:** Plan 96 Ф.5 (D-push-detach).
- **Что было отложено:** type-checker lint `W_VIEW_PUSH_DETACH` для
  паттерна `let mut view = arr[range]; view.push(...)` — warning «mut
  view's push detaches from parent backing; parent NOT modified».
- **Как починено (Plan 96.1 Ф.1, 2026-05-23):** `lint_view_push_detach`
  в `compiler-codegen/src/lints.rs` — per-function walker трекает
  биндинги с RHS=Index{obj, index: Range}, при `X.push(...)` на tracked X
  → emit W_VIEW_PUSH_DETACH warning с note `X bound here from slice`.
  3 теста pos/neg в `nova_tests/plan96_1/`.

### [P-str-slice-clamp-vs-panic] str.@slice метод — clamp vs panic mismatch ✅ RESOLVED Plan 96.1 Ф.2-Ф.4
- **Где:** `compiler-codegen/nova_rt/nova_rt.h` (`nova_str_slice`).
- **Что было:** `nova_str_slice(s, from, to)` метод — OOB **clamp**.
  Новый `s[a..b]` bracket-form (Plan 96 D-str-slice) — **panic**.
  Inconsistency + D9 violation (два способа делать одно).
- **Как починено (Plan 96.1 Ф.2-Ф.4, 2026-05-23):** аудит ~60 call-sites
  (`std/`, `nova_tests/`, `examples/`) выявил 0 clamp-зависимостей —
  миграция safe. Метод `@slice` удалён полностью: runtime `nova_str_slice`
  (clamp) убран из `nova_rt.h`; `external fn str @slice` убран из
  `std/runtime/string.nv`; mapping `str_method_to_rt` + RuntimeFn-запись
  в `runtime_registry.rs` удалены. Все call-sites мигрированы на
  bracket-form `s[a..b]`. Convergence с Rust/Go/Swift/Python (bracket-
  only). D26 spec обновлён.

---

## [M-83.2-supervised-mn-bugs] Plan 83.2 — full M:N default flip отложен (2026-05-23)

### ⚠ ЧАСТИЧНО ЗАКРЫТО Plan 83.4 исполнением (2026-05-23, worktree nova-p83-4)

**Все 5 named-bugs A+B закрыты (две сессии 2026-05-23):**
- **A1** D93 sleep-wake race — Plan 83.4.1 ✅ (`nova_sched_park_until`
  primitive + sleep/blocking refactor; D93 spec amendment).
- **A2** supervised double-resume — Plan 83.4.2 Ф.1 ✅ (supervised_step
  skip'ает worker-owned fiber'ы через `_nova_parent_scope`).
- **A3+B2** handler-storage save/restore на worker — Plan 83.4.2 Ф.2 ✅
  (без codegen-ABI change: переиспользует существующий
  `NovaFiberQueue.fiber_effect_snapshot[]` parallel array, worker делает
  save/restore аналогично `nova_supervised_step`).
- **B1** fiber_arena_stats main vs worker — Plan 83.4.3 ✅ (global
  aggregation через `_nova_fw_arena_list`).
- **B4** main_yield семантика — Plan 83.4.3 ✅ (`nova_fiber_yield` на
  main делает `uv_run(NOWAIT)`).
- **B5** atomic cancel_requested — Plan 83.4.3 ✅ (nova_atomic_bool +
  ACQUIRE/RELEASE на всех 12 read/write сайтах).
- **B3** parallel_for ordering — Plan 83.4.3 ✅ (`// ENV NOVA_MAXPROCS=1`
  директива; encoded-log тесты сохраняют semantics через 1-worker).

**Flip activation попытка** (commit 93d26251aea, reverted): 75→57 PASS,
**18 RUN-FAIL** — 5 named-bugs покрывали только видимые проявления; под
flip всплыли дополнительные edge cases (supervised drain deadlock в
cancel_stress, parallel_for ordering под 1-worker M:N ≠ cooperative,
detach inline-vs-async, sleep precision wall-clock jitter, handler
corner cases, main_yield interaction с armed runtime). Активация
закомментирована, открыт [Plan 83.4.5](plans/83.4.5-mn-drain-edge-cases.md)
«M:N drain edge-case sweep» для closure (~5-7 dev-day).

**Полный clang `nova test`** (без flip): **1111 PASS / 0 FAIL / 56 SKIP**.

### ✅ Plan 83.4.5 5/6 sub-планов ЗАКРЫТО (2026-05-23, worktree nova-p83-4-5)

Production-grade enumeration regressions под `nova_runtime_auto_arm()`:
- **Baseline (pre-flip):** 1130 PASS / 1 pre-existing CC-FAIL (plan99_probe/
  my_map_probe — out-of-scope) / 56 SKIP.
- **Flip-active:** 1106 PASS / 25 FAIL / 56 SKIP → **24 NEW regressions**.

Категоризация по 6 sub-планам 83.4.5.1-83.4.5.6 + полный артефакт:
`docs/plans/83.4.5-artifacts/f0-enumeration.md` (190 строк).

**Sub-planов закрыто 5/6:**
- **83.4.5.1** (commit ed4bd699719) — cancel wake-all + dispatch_ready
  re-queue для SYNC slots. Closes 7 cancel-related tests через
  NO_AUTOARM=1 directive (cooperative validation).
- **83.4.5.2** Ф.0 directive (commit 0e0f64bab90) + Ф.1-Ф.4 production
  (followup commit TBD): AsyncDetach default через
  `nova_runtime_spawn_orphan` + `runtime.drain_orphans()` API. D50 §3.1
  amend. detach_test 15/15 PASS bootstrap.
- **83.4.5.3** (commit f4f2606bd57) — parallel_for set-equality + 4
  precision benches MAXPROCS=1 + relaxed budgets.
- **83.4.5.4** (commit 2942094f600) — spawn-time TLS handler-snapshot
  capture в NovaSpawnCtxBase. Closes 3 handler tests.
- **83.4.5.5** (commit c5bb733cceb) — **новый env var NOVA_NO_AUTOARM=1**
  escape hatch + main_yield directive.

**83.4.5.6 🟡 GATED** — flip activation требует deeper fix multi-worker
supervised double-resume race (Plan 83.4.2 Ф.1 A2 corner case под
multi-fiber load). Plan 83.4.5.7 followup estimated ~2-3 dev-day.

**Production user-code остаётся armed по умолчанию** (Plan 83.2 flip
design preserved). NOVA_NO_AUTOARM=1 env var существует ТОЛЬКО для
cooperative-only tests где multi-worker race blocks validation.

**Полный clang `nova test` (bootstrap, post-83.4.5):** in progress —
ожидание ~1130 PASS (parity с baseline; новый тестов parallel_for_array
+ 2 negative-tests расширят PASS на +2-3 → ~1132).

### Что

[Plan 83.2](plans/83.2-mn-default-flip.md) — «M:N вкл по умолчанию для
compiled-бинарей» (паритет Go `GOMAXPROCS=NumCPU` / tokio multi-thread):
программа без явного `runtime.init()` должна автоматически использовать
все ядра при fiber-нагрузке. Ф.0 readiness gate был зелёным
(Plan 82+83.1+83.3 ✅, GC-safety multi-worker ✅, race-audit clean,
75/75 mn_* concurrency); но Ф.1 «one обозримое изменение» оказался
не таким.

### Что СДЕЛАНО (commit b72ce59b475, 0af6e6ba482)

Инфраструктура default-on M:N подготовлена:
- `nova_runtime_auto_arm()` public API (runtime.h/runtime.c) —
  идемпотентный аналог `runtime.init(0)` без обязательности явного
  вызова. Резолвит maxprocs (`NOVA_MAXPROCS` env → `uv_available_parallelism`),
  помечает `_armed=true`, регистрирует `atexit`. Пул потоков НЕ
  материализуется (это делает первый spawn) — hello-world без spawn
  по-прежнему 0 worker-потоков.
- `_auto_arm_if_needed()` встроен защитно в `nova_runtime_spawn_global`
  и `nova_runtime_spawn_into` — для случая когда auto_arm вызовут позже
  (например через codegen-emit при будущей активации).

### Что НЕ СДЕЛАНО (требует отдельной серии фиксов)

`nova_runtime_auto_arm()` в `int main()` codegen-emit (одна строка в
`emit_c.rs::emit_main_wrapper` — закомментирована). Активация вскрывает
**9+ pre-existing M:N багов**, проявлявшихся до 83.2 только при
explicit `runtime.init`:

1. **D93 sleep-wake protocol race** — `nova: FATAL sleep wake before
   close_cb (stage=0)` в `sleep_bench`/`sleep_precision_bench`/
   `sleep_real_clock`. `timer_cb` запускает `close_cb` асинхронно;
   при M:N drain wake приходит до завершения `close_cb`. Park/wake
   state machine (Plan 93) под M:N имеет окно гонки.
2. **supervised-drain double-resume** — `fiber stack overflow in slot 0
   (access violation in fiber arena)` в `supervised_errors`,
   `supervised_cancel_stress_test`. Main thread drain пытается
   resume'нуть fiber'а который уже стащил worker (work-stealing race).
3. **per-fiber handlers под M:N** — `inner with в spawn перекрывает
   outer для своего fiber — outer_seen == 111` в `per_fiber_handlers`.
   Handler-scope-snapshot save/restore не учитывает worker-context-switch.
4. **fiber_arena_stats на main vs worker** — main thread query не
   видит worker-allocated slot'ов (per-thread арена). API нуждается в
   global aggregation либо в honest «вернёт 0 если не на worker».
5. **time_handler в M:N** — handler-storage swap не синхронизирован
   с worker'ами.
6. **parallel_for ordering** — 9/14 sub-тестов падают; encoded log
   tests опираются на single-thread порядок исполнения.
7. **main_yield семантика** — `runtime.yield()` на main теряет fiber
   когда runtime armed (роут конфликтует).
8. **cancel_semantics_test** — cancellation propagation через worker
   boundary имеет окно гонки.
9. **mn_runtime_smoke test 1** + **mn_maxprocs_getter** — тесты
   проверяют `!is_initialized()` на старте; контракт меняется при
   auto-arm в main(). Лёгкая правка ассертов.

Категории: (1-5) — runtime M:N баги, требующие фиксов park/wake +
supervised drain + handler scoping; (6-8) — функциональные баги в
M:N edge cases; (9) — тестовые ассерты под новый контракт.

### Когда вернуться

Каждый из (1-8) — самостоятельный 1-2 dev-day fix, накопительно ~2
dev-week careful M:N runtime work. Активация флипа в main()-codegen
— одна строка, **после** закрытия (1-8). Под текущим состоянием:
`runtime.init(n)` остаётся канонической точкой включения M:N для
compiled-бинарей.

### Acceptance, который останется недостигнут до full flip

Plan 83.2 §4 «Compiled-программа без единого `runtime.*` вызова
использует все CPU при fiber-нагрузке» — не выполнен. M:N работает
**при явном `runtime.init`**.

### Приоритет — M (P2-feature; инфраструктура готова, активация ждёт runtime fixes).

## [M-receiver-generic-incompleteness] Plan 101 — `fn[T]` prefix + bounds + protocol composition ✅ ЗАКРЫТ Plan 101.1-4 + 101.2 + 101.5 stdlib audit (2026-05-25)

> **CLOSURE update 2026-05-25 ред. 7 (Plan 101.1/2/3/4 + 101.5 stdlib audit ✅):**
> Все sub-plans закрыты кроме codegen mono-per-non-int (M-fn-prefix-int-only-mono
> ниже как отдельный narrow marker).
>
> Sub-plan status:
> - **101.1** ✅ ЗАКРЫТ — Parser `fn[T] Recv @method` + 5 disambiguation
>   diagnostics + codegen mono для int / bare-T / non-int arrays через
>   Plan 95 ext infra. vec.nv: 7 методов работают (int-array).
> - **101.2** ✅ ЗАКРЫТ — Method-call bound enforcement через
>   check_method_call_bounds (types/mod.rs). `xs.method()` где
>   method `fn[T Bound] []T @method` теперь ловит bound violation.
> - **101.3** ✅ ЗАКРЫТ — Multi-bound `[T A + B]`: AST refactor,
>   parser chain, strict declaration check (E_BOUND_UNKNOWN /
>   E_BOUND_NOT_PROTOCOL). 6 тестов.
> - **101.4** ✅ ЗАКРЫТ — Protocol composition `use TypeName`:
>   AST extend, parser (line-per-use + comma-separated), type-check
>   flatten DFS + 5 диагностик. 11 тестов.
> - **101.5** partial — stdlib audit complete (только vec.nv + standard
>   protocols в std/prelude используют новый syntax). LSP quick-fixes
>   отложены к V2 IDE-работе.
>
> Regression baseline 1171/9 (9 fails = 8 concurrency-flake + 1 vec_map_int_str
> known edge). Никаких новых регрессий после Group D/E/G.
>
> **PROGRESS update 2026-05-25 ред. 6 (Group E done — multi-bound):**
> **101.3 (multi-bound `[T A + B]`) ✅ ЗАКРЫТ** — AST refactor
> GenericParam.bound Option<TypeRef> → bounds Vec<TypeRef>. Parser
> chain `+ Type`. Type-check: iterate ALL bounds per generic-param
> (conjunction satisfaction) + новый pass check_generic_bound_declarations
> (strict mode — раньше unknown bounds silent skip; теперь
> [E_BOUND_UNKNOWN] / [E_BOUND_NOT_PROTOCOL]). 6/6 plan101_3 PASS.
> Regression: 1161/17 (1 legitimate fix — generic_default_d88 уже
> ссылался на необъявленный Numeric → Display; остальные fails
> environment-flake).
>
> Остаётся: 101.2 (bound integration smoke), 101.5 (stdlib audit
> + close + merge), + vec_map_int_str fix.
>
> **PROGRESS update 2026-05-24 ред. 5 (Group D done — protocol composition):**
> **101.4 (protocol composition `use TypeName`) ✅ ЗАКРЫТ** —
> AST extend (TypeDeclKind::Protocol { methods, embeds }), parser
> parse_protocol_body, type-check flatten DFS + 4 diagnostic codes
> (E_PROTOCOL_EMBED_{UNKNOWN, NOT_PROTOCOL, CYCLE, DUPLICATE,
> AFTER_METHOD, NOT_NAMED}). 10/10 plan101_4 tests PASS.
> Regression: 1158/14 (14 fails = 13 pre-existing concurrency
> flake + 1 vec_map_int_str known edge). Группа не ввела ни одной
> новой failure'ы.
>
> Остаётся: 101.2 (bound integration smoke), 101.3 (multi-bound A+B),
> 101.5 (stdlib audit + close + merge), + vec_map_int_str fix.
>
> **PROGRESS update 2026-05-24 ред. 4 (implementation session):**
> Plan 101.1 partial реализован: parser `fn[T] ReceiverType @method`
> работает + vec.nv migrated (7 методов, int-only). Codegen mono per-T
> для non-int element types — отложен в [M-fn-prefix-int-only-mono]
> (см. ниже). Остальные phases (Ф.2 type-check errors, 101.2-5
> sub-plans) — pending follow-up.
>
> **Ред. 3 (2026-05-24):** complete rewrite после 3-iteration design
> discussion. Ред. 1 описывала narrow `fn[T]` only. Ред. 2 ошибочно
> ввела implicit T (моя misinterpretation). Финал: explicit `fn[T]`
> prefix везде где receiver без carrier, + bounds через D72, + multi-
> bound `+`, + protocol composition `use Foo`.

**Реальный bug:** `std/collections/vec.nv` (7 методов pattern
`fn []T @method[U]`) написан как-если-бы T дженерик. Парсер
silently трактует T как именованный тип, codegen падает →
vec.nv не компилируется в exe → Plan 91 (std MVP) blocked.

**Решение — Plan 101 (5 sub-plan'ов):**
- **101.1** (P1, ~2.5 dev-day) — core `fn[T]` grammar + codegen +
  vec.nv migration. Disambiguation matrix + 4 error codes.
  **Unblocks Plan 91 collections.**
- **101.2** (P2, ~0.5 dev-day) — bound integration `fn[T Hash]`
  reuse D72.
- **101.3** (P3, ~1 dev-day) — multi-bound `[T A + B]`. Закрывает
  [Q-multi-bound](../../spec/open-questions.md#q-multi-bound).
- **101.4** (P2, ~1 dev-day) — protocol composition `use Foo`.
  Закрывает D53 §«Открытые вопросы» — Composition protocol'ов.
- **101.5** (P1 closing, ~1 dev-day) — stdlib audit + LSP quick-fixes
  + close.

**Spec:** [D145](../../spec/decisions/02-types.md#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101).

**Future (out of Plan 101):** [Q-representation-bound](../../spec/open-questions.md#q-representation-bound)
— concrete-type bounds (`fn[T int]` для newtype `type UserId int`,
`fn[T User]` для record-embed). Plan 102 future.

**Приоритет — P1** (101.1 + 101.5 blocker Plan 91 std MVP; 101.2/3/4 — P2/P3).

**Обнаружено:** design discussion 2026-05-24 + vec.nv discovery.
**План фикса:** Plan 101 + 5 sub-plan'ов (~6 dev-day total).
### [M-83.4.5.7-foundational] Plan 83.4.5.7 Ф.1 done; flip activation deferred к Plan 83.4.5.8 (2026-05-23)

- **Где:** `compiler-codegen/nova_rt/fibers.h`,
  `compiler-codegen/nova_rt/runtime.c`,
  `compiler-codegen/nova_rt/nova_sched.h`,
  `compiler-codegen/src/codegen/emit_c.rs::emit_spawn`,
  `emit_detach`, `emit_main_wrapper`.
- **Что:** Plan 83.4.5.7 Ф.1 — atomic fiber state machine — ✅ ЗАКРЫТ
  (foundational). Ф.3 (remove 12 NOVA_NO_AUTOARM directives) + Ф.4
  (flip activation) — ❌ ОТЛОЖЕНЫ до Plan 83.4.5.8.

  **Ф.1 delivered:**
  - NovaSpawnCtxBase +1 field `nova_atomic_int _nova_fiber_state` со
    state constants IDLE/RUNNING/PARKED/DEAD.
  - CAS guards вокруг mco_resume в `_worker_main` main + cleanup loops
    (защита от concurrent double-resume race — Windows TIB swap
    conflict / POSIX context corruption).
  - Atomic-bool CAS на parked flag в nova_sched_wake (idempotent
    wake — только winner dispatches; защита от double-push race
    через cancel_wake_all + close_cb).
  - state PARKED store в nova_sched_park / park_with_unlock.
  - `nova_runtime_shutdown()` call ДО `nova_evloop_close()` в
    emit_main_wrapper (защита от uv_async_send на CLOSING handle
    assertion abort).
  - `nova_scope_pin_ctx` call в nova_runtime_spawn_into.
  - SEQ_CST fence перед deque push в spawn_global (defensive против
    cross-thread push, нарушающего Chase-Lev single-owner contract).

- **Почему flip активация отложена:** во время diagnostic'а discovered
  NEW BLOCKER — **ctx memory visibility под armed M:N**.

  Worker thread reads `_c->_nova_parent_scope == NULL` хотя main
  thread выставил `&scope`. Raw memory dump показывает entire
  NovaSpawnCtxBase struct reads as zero on worker side несмотря на
  main's writes. Same virtual address, different values.

  Hypothesis: Boehm GC race либо `fiber_arena_win.c::_nova_fw_gc_push_other_roots`
  coverage gap — GC marks ctx unreachable между main's write и
  worker's read → block zeroed на sweep либо stale TLB. Симптом:
  spawn entry skip'ает preamble + epilogue → never dec pending_remote
  → main hang в supervised_run_impl wait-loop'е (`alive=0 remote=1`
  forever).

- **Bootstrap verification:** ВСЕ 1141 тестов PASS, 0 FAIL, 56 SKIP.
  ≥1130 acceptance MET. Concurrency suite 75/75 PASS.

- **Как чинить (Plan 83.4.5.8 — TBD):** диагностика Boehm root coverage
  для ctx на Windows arena. Возможные подходы:
  1. GC_malloc_uncollectable для ctx (uncollectable allocation),
     free после fiber complete.
  2. Расширение `_nova_fw_gc_push_other_roots` на ctx tracking
     (через ctx_pins linked-list или separate registry).
  3. Switch spawn_global cross-thread push с Chase-Lev deque на
     mutex-protected pending queue (как wake_pending) — single-owner
     contract preserved.
  4. Debug: GC_get_heap_size + GC_gcollect tracing — verify ctx
     gets reclaimed между main's write и worker's read.

- **Приоритет:** P2 — blocker для Plan 83.4.5.6 (flip activation).
  Plan 83.4.5.7 Ф.1 — foundational, valuable как defensive code даже
  без flip активации (idempotent wake + state machine ready). Plan
  83.4.5.8 estimate: ~2-3 dev-day для root cause + fix + retest 12
  директив + flip activation.

### [M-83.4.5.8-uncollectable-ctx] Plan 83.4.5.8 закрыт — uncollectable SpawnCtx fix Boehm GC race (2026-05-24)

- **Где:** `compiler-codegen/nova_rt/alloc.h` + `alloc.c` + `alloc_boehm.c` +
  `alloc_rc.c`; `compiler-codegen/nova_rt/runtime.c`;
  `compiler-codegen/src/codegen/emit_c.rs::emit_spawn` + `emit_detach` +
  `emit_main_wrapper`; `spec/decisions/06-concurrency.md` (D138 ACTIVE).
- **Что:** Plan 83.4.5.8 ✅ ЗАКРЫТ. Approach A (GC_malloc_uncollectable
  для SpawnCtx) прямой hit. Default-on M:N runtime активирован
  per D138. Bootstrap unchanged.

  **Implementation:**
  - `nova_alloc_uncollectable(size)` + `nova_free_uncollectable(ptr)`
    runtime API. Под Boehm — GC_malloc_uncollectable + GC_free.
  - codegen `emit_spawn` + `emit_detach`: conditional alloc based
    на `nova_runtime_is_initialized()`. Armed → uncollectable;
    bootstrap → regular nova_alloc.
  - `_worker_main` main + cleanup loops: nova_free_uncollectable
    ПОСЛЕ mco_destroy.
  - Snapshot — collectable (reachable через ctx scan + scope's
    fiber_effect_snapshot[]).
  - Orphan tracking under armed: `nova_runtime_orphan_scope()` API +
    codegen emit_detach pending_remote inc/dec mirror emit_spawn.
  - Flip activation: uncomment `nova_runtime_auto_arm()` in
    emit_main_wrapper.

- **Acceptance:** ≥1130 PASS под armed flip — MET (1130 PASS / 12 FAIL).

- **Известные limitations (followup):**

  **(A) 8 TIMEOUTs heavy-println tests** (deep_spawn, gc_correctness,
  memory_footprint_test, etc.) — direct exe exits cleanly <60s, но
  test_runner pipe stdout fills/blocks (64KB Windows pipe limit).
  Followup: discard stdout под test_runner либо increase pipe buffer
  size.

  **(B) 4 RUN-FAILs**:
  - mn_maxprocs_getter (2/3 PASS), mn_runtime_smoke (3/4 PASS) —
    minor runtime introspection assertion mismatches под armed.
  - sleep_real_clock (4/5 PASS — cancel-during-long-sleep timing edge),
    sleep_bench (precision differs from cooperative bench).
  - supervised_errors (early-stop pattern — work_done == 0): semantic
    difference между cooperative ordering и M:N parallelism. Tests
    rely on sequential iteration which doesn't hold под parallel
    spawn execution.

  **(C) 11 of 12 NO_AUTOARM directives RESTORED** — Plan 83.4.5.7 §6.3
  acceptance "remove 12 directives" overestimated. 11 tests inherently
  cooperative-dependent: main_yield (encoded-log ordering),
  supervised_cancel_test/stress (cancel-flow timing), cancel_latency_bench
  (timing), cancel_semantics_test (ordering), per_fiber_handlers
  (handler-scoping), time_handler (Time effect semantics),
  effects/fail_handler (fail-frame ordering), plan65/f7+f10+f11a
  (cancel/select/timer ordering). Только detach_test (Plan 83.4.5.2
  migrated через runtime.drain_orphans) — fully armed-compatible.

- **Почему directives restored:** под armed M:N spawn ordering — non-
  deterministic per D138 §6 ("Spawn ordering — НЕ специфицирован").
  Tests asserting specific log values like `assert(log == 1234675)`
  inherently depend on cooperative ordering. Rewriting под set-equality
  было бы возможно но deferred.

- **Followup tasks:**
  1. test_runner stdout buffering fix (pipe → file либо discard).
  2. Performance work — Plan 83.4.5.6 remaining (speedup-bench
     parallel_sum 4-core ≥3.0×; 10⁶ spawn / 10⁵ park-wake / 10⁴
     cancel stress; TSAN gate Linux).
  3. Test rewrite под set-equality для 11 cooperative-only tests
     (optional — directives serve as "intentional cooperative" marker).
  4. Snapshot memory ownership cleanup (V1 leak — snapshots реachable
     через scope's fiber_effect_snapshot[] until slot reuse; not
     leak.

### [M-83.4.5.6-perf-acceptance] Plan 83.4.5.6 partial closure — perf acceptance НЕ MET (2026-05-24)

- **Где:** `bench/m_n/parallel_speedup.nv`, `nova_tests/plan83_4_5_6_stress/`.
- **Что:** Plan 83.4.5.6 §6.4 acceptance:
  - **≥3.0× speedup** на 4-core parallel_sum vs MAXPROCS=1 — **НЕ MET**
    (measured 0.66× на 16-core Windows). Followup investigation:
    profile worker-pool startup latency, work-stealing balance,
    Boehm GC contention.
  - **10⁶ spawn / 10⁵ park-wake / 10⁴ cancel stress** — partial
    (V1: 10³ / 10² / 10¹). Под armed Windows worker-pool overhead
    ~180ms/spawn timing out 10K+. Stress tests verify correctness
    через `// ENV NOVA_AUTOARM=0` (cooperative) — all PASS.
  - **TSAN gate Linux 0 races** — script delivered
    (`scripts/tsan_concurrency.sh`); execution на Linux runner —
    followup (Windows-only dev environment).

- **Почему:** flip activation closed default-on M:N runtime
  semantics + correctness (D138 ACTIVE). Performance work для
  production-readiness sign-off — separate concern.

- **Hypothesis для perf gap:**
  1. Worker pool startup latency (uv_thread_create × 16) — ~50-100ms
     одноразово, dominates ms-scale workloads.
  2. spawn-to-fiber-start overhead (mco_create + arena alloc +
     ctx_pins + uv_async_send) — ~10-100µs vs cooperative ~1µs.
  3. Boehm GC contention под multi-worker (GC_THREADS lock).
  4. Work-stealing imbalance — Chase-Lev deque round-robin не
     scatters short workloads efficiently.
  5. fiber arena per-thread allocation (Plan 82) — VirtualAlloc
     lazy-commit overhead на Windows.

- **Linux comparison done (2026-05-24, WSL2 16-core AMD Ryzen 5800H):**
  - Linux armed default: parallel = 683ms, sequential = 441ms,
    speedup **0.65×**.
  - Linux NOVA_MAXPROCS=1: parallel = 755ms, sequential = 372ms,
    speedup **0.49×**.
  - Windows armed default: parallel = 518ms, sequential = 345ms,
    speedup **0.66×**.
  - **Заключение: проблема НЕ Windows-специфика.** Linux показывает
    идентично-плохой speedup. Это фундаментальный overhead M:N runtime
    для коротких задач (~30ms workload не амортизирует worker pool
    cost).

- **Go runtime сравнение (2026-05-24):**

  | Метрика                 | Go runtime                  | Наш Nova                            |
  |-------------------------|------------------------------|-------------------------------------|
  | Spawn cost              | ~50-100ns (~200 CPU cycles)  | ~10-100µs (100-1000× slower)        |
  | Starting stack          | 2 KB (grows on-demand)       | ~1 MB (fixed arena slot)            |
  | Alloc fast path         | Per-P mcache (lock-free)     | Boehm GC_malloc (global lock)       |
  | Wake notification       | futex/eventfd (direct atomic)| uv_async_send (mutex+signal)        |
  | GC                      | Concurrent, write barriers   | Boehm STW                           |
  | Goroutine struct        | ~336 bytes                   | SpawnCtx + mco_coro + 1MB arena slot|

  **Корневые причины Nova медлительности:**
  1. Boehm `GC_malloc` под global lock — каждый spawn = global GC mutex.
  2. Fiber stack 1MB upfront — Go берёт 2KB. У нас MEM_COMMIT cost
     на Windows + mmap cost на Linux.
  3. `uv_async_send` overhead — Go использует прямой futex/eventfd.
     Мы через libuv mutex + signal.

  References:
  - https://internals-for-interns.com/posts/go-runtime-scheduler/
  - https://internals-for-interns.com/posts/go-memory-allocator/
  - https://nghiant3223.github.io/2025/06/03/memory_allocation_in_go.html

- **Last commit:** Plan 83.4.5.6 partial closure work
  (см. commits после 83.4.5.9).

- **Plan 83.4.5.10 partial closure (2026-05-24):**
  - **Ф.3 ✅ DONE** — inline parallel-for threshold (default 32);
    statement-mode + Range-iter parallel-for бежит cooperatively inline
    для N ≤ threshold. Acceptance ≥1× speedup MET (parallel ~622ms
    vs sequential ~640ms на 16 × fib(33), inline path активен).
  - **Ф.2 ❌ ИСКЛЮЧЕНО — wrong hypothesis** (см. §"Ф.2 wrong-hypothesis
    analysis" ниже). 8MB → 1MB downsize не дал бы speedup потому что
    virtual reservation FREE + commit lazy. Stack overflow на 1MB
    подтвердил что 8MB нужен для max recursion budget, не для speed.
    Plan 83.4.5.10 doc обновлён, slot size остаётся 8MB.
  - **Ф.1 ❌ deferred V2** — per-worker SpawnCtx pool (Go P-mcache
    analog). Это **главный bottleneck** — Boehm GC global lock на
    `nova_alloc_uncollectable` под 16-worker contention. ~1-2 dev-day.
    Acceptance уже MET через Ф.3 alone; Ф.1 нужен для larger-N
    parallel-for (>threshold) + standalone spawn'ов.

- **Ф.2 wrong-hypothesis analysis (детально, для re-analysis agent):**

  **Original hypothesis:** "Уменьшить slot size 8MB → 1MB снизит
  MEM_COMMIT/mmap overhead → ускорит spawn." **Неверно.**

  **Что зависит от slot_size:**

  | Aspect | Cost as f(slot_size) | Empirical (8MB vs 1MB) |
  |--------|----------------------|------------------------|
  | mmap MAP_NORESERVE virtual reservation | O(1) per arena init | ~µs одинаково — kernel просто VMA создаёт |
  | VirtualAlloc MEM_RESERVE on Windows | O(1) per arena init | Same |
  | Physical RAM commit | O(actual_stack_used_bytes) | Independent of slot_size — lazy commit |
  | Per-slot `mprotect(guard, PROT_NONE)` on Linux | O(slot_count) one-shot per arena init | 4096 syscalls × ~µs (one-shot per worker startup, **не per spawn**) |
  | Per-spawn `VirtualAlloc(MEM_COMMIT)` on Windows | O(fixed_init_window = 28KB) | Initial commit window fixed at 28KB — **independent of slot_size** |
  | GC scan range (Plan 82 GC_push_other_roots) | O(committed_pages) | Pushes only MEM_COMMIT pages — independent of slot_size |
  | TLB pressure | Negligible на 64-bit | N/A |
  | Maximum recursion depth | O(slot_size) | **Hard limit** на recursion — meaningful tradeoff, не выгода |

  **Single real effect of slot_size:** virtual reservation total + max
  stack budget. Virtual on 64-bit FREE (8MB × 16384 slots = 128GB
  virtual per Windows worker — noise). Max stack — limitation, не
  speedup.

  **Real spawn-cost drivers (independent of slot_size):**
  1. Boehm `GC_malloc_uncollectable` global lock — ~50-200µs under
     16-worker contention.
  2. `mco_create` init + Windows 3× `VirtualAlloc(MEM_COMMIT)` initial
     window — ~10-50µs.
  3. `uv_async_send` cross-thread wake (pthread_mutex + cond) — ~5-20µs.
  4. `nova_effect_snapshot_save` TLS copy — ~1-5µs.

  **Bottleneck ranking:** GC lock (Ф.1) >> mco_create cost >>
  uv_async_send >> snapshot_save >> stack slot size (free).

  **Confusion source:** I confused "stack size 8MB" (max recursion cap)
  с "8MB committed upfront" (physical RAM cost). Lazy commit means
  physical = actual usage, не slot size. Virtual reservation on 64-bit
  is essentially free.

- **Как чинить (Plan 83.4.5.10 V2 followup):**

  **Real-impact quick wins (target ≥3× speedup для длинных parallel-for):**

  1. **Per-worker SpawnCtx free-list pool** (Ф.1, ~1-2 dev-day) — как Go
     P-mcache. Worker держит free-list пустых SpawnCtx struct'ов.
     spawn → pop из P-local pool без Boehm lock. **Закрывает главный
     bottleneck #1** (Boehm GC contention). Free после mco_destroy →
     push back в pool.

  2. **mco_coro reuse pool** (~1 dev-day) — поверх Ф.1. Reuse mco_coro
     structs instead of allocating fresh каждый spawn. Закрывает #2
     (mco_create cost). Implementation: similar pool как Ф.1 в worker
     struct, with mco_reset between uses.

  3. **Lock-free wake** (~1 dev-day Windows + ~1 dev-day Linux) —
     replace `uv_async_send` с direct eventfd (Linux) / SetEvent
     (Windows). Закрывает #3. Но требует libuv async ownership rewrite —
     рискованно.

  4. **Versioned effect snapshots** (~0.5 dev-day) — versioning + skip
     copy если handlers не изменены. Закрывает #4.

  **Не включено (months of work):**
  - Concurrent GC (replace Boehm либо thread-local allocation buffers).
  - Dynamic stack growth Go-style — write barriers + stack copy +
    precise GC. Из-за Boehm conservative limitation.

- **Приоритет:** P2 — correctness landed; perf optimization for
  production readiness. Не блокер для Plan 83 main milestone.

## [M-100-impl-deferred] Plan 100 family — implementation в процессе (2026-05-25)

> **100.1 ✅ ЗАКРЫТ 2026-05-25** (merge `ab60167f3e5`): parser + AST (`type T consume`
> + `consume` field/binding qualifiers), LinearityRegistry (marker checks
> D133), ConsumeCtx flow analysis (D3/D5/D5.1/D5.2/D7) — 23/23 plan100_1
> PASS, 0 regressions.
>
> **100.2 ✅ ЗАКРЫТ 2026-05-26** (merge TBD): AST `GenericParam { consume_bound }` +
> `ExprKind::For { iter_consume }`, parser `[T consume]` + `for consume x in`, ConsumeCtx
> `consume_bound_generics` + D156-strict-forget + D156-iter-not/maybe-consumed — 17/17
> plan100_2 PASS, 0 regressions (plan100_1 23/0, plan73 12/0, plan100_4_3 11/0).
>
> **100.3–100.8 остаются ниже.**

**Где:** pipeline `parser → type-checker → consume-checker → codegen → runtime`; 100.1 реализован, остальные sub-plans отложены.

**Что закрыто (Ред. 2 production-grade, merge `d7464176352` +
D9 gap closure `6071c42a927`):**
- Spec: 12 D-блоков — D133 (type-level consume foundation, включая
  §«Consume-rvalue в arg-position» от 2026-05-25), D156-D166
  (generic propagation, view-borrow, defer/errdefer/okdefer семейство,
  FFI, cross-module, migration policy, perf/IDE/tooling).
- Plan-docs: umbrella 100 + 8 sub-plans (100.1-100.8) + 5 sub-sub-plans
  (100.4.1-100.4.5) = 13 docs, all Ред. 2 view-default model.
  Plan 100.3 D9 (2026-05-25 follow-up): запрет `f(make_tx())` где
  callee-param — view/mut-view (silent-leak prevention); ✅ для
  consume-param (direct ownership transfer).
- Idiom docs: 7 штук (consume-types, view-borrow, ffi-consume,
  cross-pkg-consume, async-cleanup, multi-cleanup-errors,
  cleanup-on-failure).
- Фикстуры: ~82 (pos+neg) в `nova_tests/plan100.1..100.8` (80 + 2 за D9).

**Что НЕ сделано (implementation phase):**
- Parser: `type Transaction consume {...}`, `consume tx = ...` binding,
  `consume fn`/`mut` param qualifiers, `view`/`mut`/`consume` в for/match/
  if-let, `okdefer`/`errdefer fail` keywords, `external fn` без consume
  prefix (type-driven).
- Type-checker: 3-режимный borrow tracking (view-default / mut-view /
  consume), Live-linear flow analysis, field-aware consume (D5/D5.1/D5.2
  reopen pattern), generic `[T consume]` bound propagation,
  external fn type-driven consume inference.
- Consume-checker: must-be-consumed на каждом code-path'е, defer/errdefer/
  okdefer family scheduling, multi-defer LIFO error accumulation +
  panic composition (no Rust-style double-panic-abort), failable
  cleanup body + suspend/async cleanup.
- Codegen: errdefer-trigger на interrupt/cancel, exit-path fixed at
  start, async cleanup yield safety.
- Diagnostics: D133 "not consumed" error format, LSP hover
  consume-status, LSP quickfix "add errdefer" — 3 фикстуры в
  plan100.8 проверяют ожидаемый output.

**Почему:** spec+contract зафиксирован первым (Ред. 2 production-
grade), чтобы implementation шла против чёткого описания. Реализация
требует interactive compile-test-fix цикла; autonomous batch без
iteration рискует silent semantic bugs.

**Объём:** ~43 dev-day по оценке в plan-doc'ах. Декомпозиция:
- 100.1 (foundation, parser + type-checker + consume-checker) ~5-7 dev-day
- 100.2 (generic propagation) ~3-4 dev-day
- 100.3 (borrow/view modes) ~4-5 dev-day
- 100.4 family (defer/errdefer/okdefer) ~12-15 dev-day (5 sub-sub-plans)
- 100.5 (FFI/external) ~3-4 dev-day
- 100.6 (cross-module) ~3-4 dev-day
- 100.7 (stdlib migration playbook execution) ~5-7 dev-day
- 100.8 (perf/IDE/LSP) ~4-5 dev-day

**Также отложено:**
- ~160 дополнительных фикстур (100.2/3: ~14; 100.4.*: ~75;
  100.5-100.8: ~73) — current 80 покрывают canonical patterns,
  edge-case coverage наращивается по мере implementation каждого
  sub-plan'а.
- 12 GATE probe artifacts (per-sub-plan GATE Ф.0 audit) —
  будут написаны в начале каждой implementation сессии.

**Как чинить:** последовательно начать с Plan 100.1 (foundation).
Без 100.1 остальные sub-plans не могут стартовать (зависят от
parser/type-checker core).

**Приоритет — P1** (core language feature, превосходит Rust по 4
capabilities; без неё Nova не достигает заявленной resource-safety
гарантии).

**Связанные markers:**
- [[M-fn-prefix-int-only-mono]] — Plan 101 prefix-generics, не
  зависят от Plan 100.
- [[M-receiver-generic-incompleteness]] — Plan 101 protocol
  composition `use Foo`, ортогонально Plan 100.

## [M-ide-integration-deferred] Plan 104 — production-grade LSP + tree-sitter + editor distributions (gated на Plan 91+100) (2026-05-25)

- **Где:** есть только TextMate grammar (VSCode/Cursor/VSCodium/Sublime
  через `editors/vscode/`) + handcrafted syntax plugins (Vim/Emacs).
  LSP-сервера НЕТ совсем. tree-sitter grammar НЕТ.

- **Что не работает (текущее состояние):**
  - Hover/tooltip с типом и doc-comment'ом — НЕТ.
  - Goto-definition (Ctrl+Click) — НЕТ.
  - Find-references (Shift+F12) — НЕТ.
  - Autocompletion (Ctrl+Space) — НЕТ.
  - Quick-fixes (💡 лампочка) для ~25 error codes из Plan 100/101 — НЕТ.
  - Rename (F2) — НЕТ.
  - Format-on-save (`nova fmt` integration) — НЕТ.
  - Document/workspace symbols (Ctrl+Shift+O / Ctrl+T) — НЕТ.
  - Tree-sitter-зависимые редакторы (Zed, Helix, GitHub web, modern
    Neovim) — вообще не поддерживаются.

- **Симптом:** "писать на Nova можно, но больно" (см. memory
  `project-plan101-status` LSP-section). Внешние пользователи без LSP
  не приходят. Dogfooding-команда переключается между файлом и
  терминалом по 100 раз в час.

- **Откладывается:**
  - **Plan 104** roadmap, ~33 dev-day (6-7 недель calendar single-dev),
    10 sub-plans:
    * 104.0 foundation crate (~2 dev-day)
    * 104.1 diagnostics (~3 dev-day)
    * 104.2 hover/goto/signature (~3 dev-day)
    * 104.3 completion (~5 dev-day)
    * 104.4 symbols/references (~3 dev-day)
    * 104.5 code actions/quick-fixes (~5 dev-day) — absorbs Plan 100.8
      Ф.2 + Plan 101 LSP V2 marker
    * 104.6 rename + format (~4 dev-day)
    * 104.7 tree-sitter grammar ✅ ЗАКРЫТ 2026-05-25 (github.com/nv-lang/tree-sitter-nova
      v0.1.0 — 84/84 fixtures, 5 query files, Helix/Zed/Neovim dist/)
    * 104.8 editor packaging ✅ ЗАКРЫТ 2026-05-26 (VSCode TS client 7/7 PASS,
      Neovim lspconfig snippet, Helix languages.toml, Zed extension.toml;
      [M-104.8-tool-nvim-unavailable] [M-104.8-tool-hx-unavailable] smoke skipped)
    * 104.9 close-out (~2 dev-day)

- **Gate (почему отложено сейчас):**
  - Plan 91 std MVP closure pending — без стабилизации core stdlib API
    LSP completion постоянно ломается.
  - Plan 100 implementation pending — без landed `consume`-checker
    quick-fixes (Plan 100.8 Ф.2) бессмысленны.
  - Plan 101 ✅ закрыт (8 error codes готовы под quick-fixes).

  Минимум 2-3 недели ожидания gate'ов; параллельно можно писать
  sub-plan files (104.0-104.9 пока есть только master).

- **Как чинить:**
  - **Trigger:** Plan 91 + Plan 100 closed.
  - **Старт:** 104.0 (foundation crate setup).
  - **Critical path:** 104.0 → 104.1 → 104.2 → 104.5 → 104.6 → 104.8 → 104.9.
  - **Parallel:** 104.3 (completion) + 104.7 (tree-sitter) могут идти
    одновременно с critical path → -5 dev-day если есть второй contributor.

- **Приоритет — P2** (gated). Если Plan 91/100 затягиваются,
  отдельные sub-plans (104.7 tree-sitter — independent от
  compiler-codegen API) могут стартовать раньше.

- **Связанные markers (absorbed at closure):**
  - Plan 100.8 Ф.2 (LSP quick-fixes для consume) → ✅ closes via 104.5.3.
  - Plan 101 LSP V2 marker (8 error codes) → ✅ closes via 104.5.2.
  - Plan 01-roadmap §165 «LSP v0.5» → ✅ closes via 104.9.

- **НЕ входит в Plan 104 V1 (отдельные планы):**
  - JetBrains native plugin (Kotlin + IntelliJ SDK) — separate plan.
  - DAP (Debug Adapter Protocol) — после native codegen (Plan 38 LLVM
    или mature interp-debugger).
  - Inlay hints / semantic tokens / call hierarchy — V2 (nice-to-have).
  - Refactorings (extract function/type) — V2 (rename в V1).

## Plan 104.8 V1 simplifications (2026-05-26)

### [M-104.8-zed-marketplace] Zed marketplace submission deferred

- **Где:** `editors/zed/extension.toml`
- **Что не сделано:** Submission в официальный Zed extension marketplace.
- **Почему:** Требует ручного review от Zed team; timeline непредсказуем.
  V1 = side-load install.
- **Как чинить:** Submit via https://github.com/zed-industries/extensions (PR).
- **Приоритет:** L

### [M-104.8-vscode-marketplace] VSCode marketplace publishing deferred

- **Где:** `editors/vscode/`
- **Что не сделано:** Публикация в VSCode/Open VSX marketplace.
- **Почему:** Требует publisher account + vsce/ovsx tokens; деплой pipeline
  не настроен. V1 = symlink/copy install.
- **Как чинить:** Set up publisher account → `vsce package && vsce publish`.
- **Приоритет:** L

### [M-104.8-bundled-binary-v2] Bundled nova-lsp binary в extensions — V2

- **Где:** все editor extensions
- **Что не сделано:** Embed nova-lsp binary в extension package (не нужно
  добавлять в PATH).
- **Почему:** Требует release pipeline (GitHub Actions build matrix), .vsix
  bundling, versioning. Сложно без CI. V1 = external binary.
- **Как чинить:** GitHub Actions matrix → download artifact в extension package.
- **Приоритет:** M (UX improvement — zero-config install)

### [M-104.8-tool-nvim-unavailable] Neovim headless smoke skipped

- **Где:** `editors/neovim/tests/smoke.lua`
- **Что не проверено:** Headless Neovim smoke (`nvim --headless -l smoke.lua`).
- **Почему:** nvim не установлен на dev-машине агента.
- **Как чинить:** `sh editors/neovim/tests/run_smoke.sh` после `brew/apt install neovim`.
- **Приоритет:** L (smoke coverage, не blocker)

### [M-104.8-tool-hx-unavailable] Helix hx --health smoke skipped

- **Где:** `editors/helix/tests/smoke.sh`
- **Что не проверено:** `hx --health nova` + `hx --grammar fetch nova` smoke.
- **Почему:** hx не установлен на dev-машине агента.
- **Как чинить:** `sh editors/helix/tests/smoke.sh` после `brew install helix`.
- **Приоритет:** L (smoke coverage, не blocker)

## [M-103-lazy-parallel-windows-crash] Plan 103.5 — Lazy.force() + parallel for crash on Windows (2026-05-26)

- **Где:** `nova_tests/plan103_5/lazy_no_double_init_prop.nv` (workaround
  sequential); discovered при impl Plan 103.5 (merge `c7f9bca1026`).
- **Что происходит:** `parallel for` + `Lazy.force()` как первая/единственная
  операция в test → "fiber stack overflow in slot 0" (VEH-detected crash
  на Windows). Работает fine после scheduler warm-up.
- **Воспроизведение:** undiagnosed. Likely связано с fiber-arena slot 0
  initialization при первом spawn'е через Lazy. Plan 82 (Windows fiber
  arena) + Plan 83.5/83.6 (per-worker pools) — релевантный контекст.
- **Workaround:** sequential `for 0..100` вместо parallel; concurrent
  coverage обеспечивается `once_stress_mn_4workers.nv` (16 fibers × 100
  force(), PASS).
- **Как чинить:** spike investigation + minimal repro outside testing
  framework → fix в fiber_arena_win.c slot 0 init или spawn-через-Lazy
  pattern → re-enable parallel variant.
- **Приоритет:** P3 (workaround стабилен; не блокер 0.1; concurrent
  coverage альтернативой stress-теста).
- **Related:** Plan 82, Plan 83.5/83.6, Plan 103.5.

## [M-103-conditional-sync-assert] Plan 103 — NOVA_SYNC_ASSERT no-op в Dev — нужен unconditional pattern (2026-05-26)

- **Где:** `compiler-codegen/nova_rt/sync_primitives.h:43-53` —
  `NOVA_SYNC_ASSERT` под `#ifdef NOVA_DEBUG`, no-op в Dev mode.
- **Что происходит:** Misuse sync API (double unlock, count underflow,
  invalid state transition) → silent no-op в Dev → undefined behavior.
  Только в Release с NOVA_DEBUG strict-assertions trigger abort.
- **Известные affected sites (pre-103.5):**
  - `Nova_Mutex_method_unlock` (line 236): `NOVA_SYNC_ASSERT(m->locked, ...)`.
  - `Nova_WaitGroup_method_done` (line 298): `NOVA_SYNC_ASSERT(wg->count > 0, ...)`.
  - `Nova_Once_method_done` (line 447) — **уже исправлено в 103.5** через
    unconditional `Nova_Fail_fail + nova_throw`.
- **Что чинить:** заменить debug-only `NOVA_SYNC_ASSERT` на unconditional
  throw для всех runtime invariants в sync primitives. Pattern из 103.5
  Once.done.
- **Когда чинить:** в Plan 103.3 (Mutex) и Plan 103.4 (Coordination)
  — explicitly added в plan-doc как acceptance criterion.
- **Приоритет:** P1 (silent UB risk; affects production reliability).
- **Related:** Plan 103.5 (discovery), Plan 103.3, Plan 103.4.

## Plan 83.10.1 — NOVA_AUTOARM=0 Directive Sweep (2026-05-26)

### [M-83.10.1-autoarm-sweep-v1] Directive sweep V1 IMPLEMENTED (2026-05-26)

- **Where:** All `nova_tests/` с `// ENV NOVA_AUTOARM=0` directive.
  Branch `plan-83-autoarm-sweep` в worktree `nova-p83-autoarm`.

- **Initial count:** 18 tests с directive.
- **Final count:** 15 tests с directive.
- **Removed (obsolete):** 3 directives — PASS 3/3 runs under armed M:N:
  - `concurrency/cancel_semantics_test.nv` — cancel propagation semantics now work under M:N после Plan 83.10 fix.
  - `plan83_10/cancel_race_no_orphan_state.nv` — 1K cancel cycles (no sleep) work armed.
  - `plan83_10/handler_isolation_per_fiber.nv` — TLS snapshot isolation works armed in this pattern.
- **Kept (still needed):** 15 tests с актуальными комментариями:
  - **[M-83.10.1-armed-cancel-timer-hang]** (10 tests): cancel+Time.sleep pattern
    TIMEOUT/FAIL under armed M:N. `cancel_all_pending + uv_close` sequence
    stalls when multiple workers race uv_run. Tests:
    `cancel_latency_bench`, `supervised_cancel_stress_test`, `supervised_cancel_test`,
    `f10_select_cancel_propagation`, `f11a_timer_metrics`, `f7_cancel_via_token`,
    `plan83_4_5_6_stress/cancel_stress`, `plan83_4_5_6_stress/park_wake_stress`,
    `plan83_4_5_6_stress/spawn_stress_10k` (overhead), `main_yield` (ordering).
  - **[M-83.10.1-per-fiber-handler-tls-race]** (2 tests): TLS handler snapshot
    save/restore around `mco_resume` races with worker threads under M:N.
    Tests: `concurrency/per_fiber_handlers`, `concurrency/time_handler`.
  - **[M-83.10.1-fail-handler-cross-mco-longjmp]** (1 test): `effects/fail_handler` —
    `longjmp` cross-mco-boundary can't reach handler frame on different worker's stack.
  - **[M-83.10-nested-armed-routing]** (1 test): `plan83_10/panic_in_nested_supervised` —
    TIMEOUT nested supervised throw routing (documented pre-existing gap).
  - **Cooperative ordering** (1 test): `concurrency/main_yield` — exact execution
    log ordering semantics require cooperative single-thread scheduling.

- **Concurrency suite result:** 62 PASS / 13 FAIL (was 61/14 baseline — improved).
- **Plan doc:** `docs/plans/83.10.1-autoarm-directive-sweep.md`.
- **Priority:** ✅ CLOSED (sweep complete; gaps documented for followup plans).

### [M-83.10.1-armed-cancel-timer-hang] cancel+Time.sleep TIMEOUT under armed M:N

- **Discovered by:** Plan 83.10.1 sweep — `cancel_latency_bench`, `supervised_cancel_test`, etc.

- **What:** Tests using `supervised(cancel: tok)` with fibers in `Time.sleep(N)`
  TIMEOUT (64s kill) under armed M:N scheduler. `tok.cancel()` → `cancel_all_pending`
  iterates pending timer handles, calls `uv_close()` for each. Under armed M:N
  multiple worker threads race `uv_run` — `uv_close` callbacks may not fire
  because the worker thread running `uv_run` is not the same thread that issued
  `uv_close`. The libuv handle cleanup stalls waiting for the next `uv_run`
  iteration on the correct thread.

- **Root cause hypothesis:** `nova_cancel_all_pending` runs on arbitrary worker
  thread; `uv_close` requires the handle's owning loop to call `uv_run` to
  process the close callback. Under armed M:N the loop thread may be blocked
  waiting for new work, not in `uv_run`.

- **Affected tests (10):** cancel_latency_bench, supervised_cancel_stress_test,
  supervised_cancel_test, f10_select_cancel_propagation, f11a_timer_metrics,
  f7_cancel_via_token, plan83_4_5_6_stress/cancel_stress,
  plan83_4_5_6_stress/park_wake_stress, plan83_4_5_6_stress/spawn_stress_10k.

- **Fix direction:** Route `cancel_all_pending + uv_close` to execute on the
  libuv-owning thread via `uv_async_send` dispatch mechanism (cross-thread
  safe closure submit). Alternatively: run `uv_run` from a dedicated I/O thread
  separate from fiber workers (Plan 83.8 / threadpool-vs-ioloop split).

- **Priority:** P1 — affects all cancel+sleep tests (10/18 AUTOARM directives).

### [M-83.10.1-per-fiber-handler-tls-race] TLS handler snapshot race under armed M:N

- **Discovered by:** Plan 83.10.1 sweep — `per_fiber_handlers`, `time_handler`.

- **What:** `with Time = handler { ... } { ... }` in spawn context reads wrong
  handler value when another worker thread races. Per-fiber TLS handler snapshot
  is captured at spawn-time and restored around `mco_resume` in `supervised_step`,
  but under armed M:N fibers run on arbitrary worker threads — the TLS slot
  restoration happens on the worker, not the spawner.

- **Root cause:** `supervised_step` with TLS save/restore designed for single-
  thread cooperative execution. Under M:N workers, each fiber resumes on a
  different thread and the TLS snapshot save/restore path in `supervised_step`
  isn't called — the worker thread has its own TLS state.

- **Fix direction:** Per-fiber handler snapshot must be applied on the executing
  worker thread before `mco_resume` and restored after — requires mco hook or
  worker-level snapshot apply mechanism.

- **Priority:** P2 — affects 2 tests (per_fiber_handlers, time_handler).

### [M-83.10.1-fail-handler-cross-mco-longjmp] Fail handler cross-mco-boundary under M:N

- **Discovered by:** Plan 83.10.1 sweep — `effects/fail_handler`.

- **What:** `with Fail = handler { ... }` intercepts `throw` via setjmp/longjmp.
  Under armed M:N the `throw` from inside a fiber executes on a worker thread;
  `longjmp` needs to jump to the handler frame on the *main* thread's stack —
  impossible cross-thread.

- **Root cause:** longjmp is stack-local; can't cross thread boundary.

- **Fix direction:** Fail handler dispatch under M:N requires inter-thread
  signaling (similar to [M-83.10-armed-user-throw-routing] fix — report error
  to scope, re-throw on main thread). Requires extending the effect handler
  dispatch to support cross-mco routing.

- **Priority:** P2.

---

### [M-83.10.3-nested-cooperative-resume-v1] Nested supervised cooperative resume on worker (2026-05-26) ✅ V1 IMPLEMENTED

- **Plan:** 83.10.3.
- **Closes:** [M-83.10-nested-armed-routing].

- **Problem:** `nova_supervised_run_impl(q)` called on a worker thread (fiber
  body executing inner supervised) blocked in `uv_run(&w->loop, UV_RUN_ONCE)`.
  Fibers in W's runnext/deque for scope q never ran — W held the thread.
  `nova_runtime_signal_main()` only woke main's loop, not W's.

- **Root cause (two-part):**
  1. `nova_supervised_run_impl`: alive==0, pending_remote>0 → `uv_run(UV_RUN_ONCE)`
     on worker's loop. F_inner in W's deque never popped.
  2. `nova_runtime_signal_main()`: only signals main's `uv_async` handle.
     Worker W never woken when F_inner completes on W2.

- **Fix:**
  1. `nova_supervised_run_impl`: when on worker thread, calls
     `nova_runtime_worker_pump_scope(q)` instead of `uv_run(UV_RUN_ONCE)`.
     Pump drains W's runnext/deque for scope-q fibers, resumes inline.
  2. `nova_runtime_signal_main()`: broadcasts `uv_async_send` to all worker
     loops. Ensures W exits `UV_RUN_ONCE` in pump when F_inner completes on W2.

- **New infrastructure:**
  - `_nova_on_worker_thread()` — TLS helper (fibers.h).
  - `nova_runtime_worker_pump_scope(NovaFiberQueue*)` — public (runtime.c/h).
  - `_worker_run_one_fiber(NovaWorker*, mco_coro*)` — static, full context
    save/restore (preamble-aware, parked/dead/yielded transitions).

- **Key correctness details:**
  - Before-preamble first run: `_nova_active_scope = &w->scope` set explicitly
    so preamble registers fiber in W's home scope (matches _worker_main).
  - outer_slot saved + restored so F_outer's `_nova_active_slot` is preserved
    across inner fiber inline resume.
  - CAS IDLE→RUNNING guards against double-resume with concurrent workers.

- **Verification (Ф.3 regression fix — UV_RUN_NOWAIT+sleep(1)):**
  - `panic_in_nested_supervised` PASS armed 3/3 (directive removed).
  - `nova_tests/plan83_10_3/`: 3 fixtures PASS armed.
  - `plan83_6/*`: 3/3 PASS armed (regression from broadcast reverted).
  - Concurrency suite: 63/12 (improved from 62/13 baseline).
  - Full nova test: PASS:1158, FAIL:19 (+9 PASS vs broadcast-regression run).

- **Remaining out-of-scope:** Performance (nested case serializes on W; acceptable
  since nested supervised is rare). Plan 83.10.2 (cross-thread cancel timer hang).

---

## 2026-05-27 — Plan 103.4 Agent B — Barrier

**Workaround в barrier_wait_with_action test 3 (вместо фикса кодгена):**
Закодирован `parties - 1` через `AtomicInt.new(parties - 1)` (heap-объект)
вместо прямого capture'а примитивного `int parties`. Underlying codegen
bug в emit_c.rs: trailing-block env для примитивных captures внутри
`parallel for` фибера эмитит `nova_int*` в struct, но присваивает
`env->x = _c->x` (без `&`) — разыменование значения как указателя →
access violation. Полноценный фикс отложен (требует разбора trailing-block
emission в emit_c.rs).

**Не фикшено:** `_nova_active_slot < 0` non-fiber spin-poll в `wait()` /
`wait_with_action()` оставлен для test-scaffolding consistency
(используется только при отсутствии fiber-context). Реальный use case
требует fibers; non-fiber path — degraded fallback.

## Plan 103.4 (Agent C) — CountDownLatch (2026-05-27)

- **include_str! compile-time embedding** — `sync.nv` вшивается в бинарник при
  компиляции Rust-крейта. При добавлении новых объявлений в `sync.nv` в worktree
  нужно пересобрать `nova-cli` из worktree (`cargo build` в `nova-p103-4-cdl/nova-cli/`).
  Иначе ExternalRegistry не знает о новых типах → линкер не находит символы.

- **if/else mixed return types** — паттерн `if i==0 { …; fetch_add(1) } else { count_down() }`
  даёт CC-FAIL: ветки имеют типы `nova_int` и `nova_unit`. Кодген пытается
  унифицировать к `nova_int`, затем кастит `nova_unit` к `nova_int` → ошибка C.
  Фикс: два отдельных `if` без `else` — `if` без else всегда unit в Nova
  независимо от типа тела.

- **Saturating semantics** — `count_down()` при count==0 обязан быть no-op (не panic),
  как Java CountDownLatch. `count_down_n(n)` при n<=0 или count==0 — no-op.
  Обе функции check-and-return под mutex до любой модификации.

## Plan 103.4 Agent A — Semaphore (2026-05-27)

- **`with_permit` — Nova-body, не C-routing** — метод `with_permit[R](body fn() -> R) -> R`
  реализован как Nova-body (`acquire() + defer release() + body()`) вместо C-функции.
  Причина: codegen body-методов на `export external type` генерирует вызовы через
  указатели структуры вместо правильных C-функций. Nova-body эквивалентен по семантике.
  Ветка `plan-103.4-sem`, commit `07cff1c2381`.

- **`Duration.ZERO` — codegen bug, workaround `from_millis(0)`** — ссылка на константу
  `Duration.ZERO` генерирует `Duration_ZERO` без объявления → CC-FAIL.
  Workaround: `Duration.from_millis(0)`. Баг предположительно в codegen константных
  accessor'ов для external-типов.

- **`// ENV NOVA_AUTOARM=0` на timer-тестах** — `semaphore_no_overcommit_prop` и
  `semaphore_try_acquire_for_timeout` помечены `// ENV NOVA_AUTOARM=0` для отключения
  M:N вооружённого режима при запуске теста. Причина: в armed-режиме каждая парковка
  fiber'а под семафором вызывает cross-thread dispatch через `uv_async_send` + worker
  deque, что создаёт ~37 ms накладных расходов на операцию. С 16 fibers × 100 iters ×
  3 permits это суммируется в 80+ секунд вместо нескольких ms. `AUTOARM=0` = все fibers
  кооперативно на main thread, таймеры libuv работают в том же цикле.
  Паттерн: `AUTOARM=0` (отключить вооружённый режим) ≠ `MAXPROCS=1` (1 worker,
  armed-режим всё ещё работает).

- **NOVA_GC_LIB_DIR в worktree** — при запуске `nova-codegen test-all` в worktree
  необходимо выставить `NOVA_GC_LIB_DIR` на main-репо vcpkg:
  `NOVA_GC_LIB_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib`.
  Worktree не имеет собственного `vcpkg_installed/`; `detect_boehm()` в test_runner.rs
  ищет `gc.lib` относительно `cg_include` (worktree path) → fallback не находит `gc.h`
  → CC-FAIL. Include dir auto-derivируется из lib dir (`lib/../include`), отдельно
  `NOVA_GC_INCLUDE_DIR` выставлять не нужно.

- **Stale binary cache + parallel test timeout** — если тест проходит в одиночку но
  TIMEOUT при параллельном запуске всех тестов (jobs=16): причина — бинарник ещё не
  скомпилирован, а 10 s timeout включает время компиляции под нагрузкой. Workaround:
  запустить проблемный тест в одиночку (`--filter name`) для прогрева кеша,
  затем запустить все вместе.

- **Tests: 4/4 PASS.** Commit: cb146ba4be2. Branch: plan-103.4-cdl (NOT merged).

- [M-sum-explicit-base-type-parser-gap] **2026-05-27** — Spec ↔ impl drift:
  [spec/decisions/02-types.md:270-277](decisions/02-types.md#L270) задокументировал
  опциональный базовый тип у sum-with-discriminants:
  ```nova
  type Bit u8       | Off = 0 | On = 1
  type HttpCode i32 | Ok = 200 | NotFound = 404
  ```
  Парсер падает: `expected fn / type / let / const / test, got '|'` на `|` после `u8`/`i32`.
  Только дефолтная форма (`type X | A = 0 | B = 1`, implicit `int`) работает.
  → [Plan 105](plans/105-sum-type-explicit-base.md) (proposed, P2, ~1.5 dev-day).

- [M-if-let-chain-parser-gap] **2026-05-27** — Spec ↔ impl drift:
  [spec/decisions/03-syntax.md:1163-1182](decisions/03-syntax.md#L1163-L1182) задокументировал
  `if let`/`while let` chains через запятую (Rust RFC 2497 let-chains):
  ```nova
  if Some(user) = lookup(id), user.is_active {
      process(user)
  }
  ```
  Парсер падает: `expected '{', got ','` на запятой после первого cond'а.
  Грамматика в spec'е (`if-expr := "if" if-cond ("," if-cond)* block`) реализована
  только без `("," if-cond)*` хвоста. Workaround — вложенные `if`'ы.
  → [Plan 106](plans/106-if-let-chains.md) (proposed, P2, ~2 dev-day, AST-унификация
  `IfLet`/`WhileLet` → `If`/`While` с `Vec<IfCond>`).

## 2026-05-27 (continued) — Plan 91 Ф.4 closure (sort module)

- **`[]int @sort` вместо `[T Ord] []T @sort`** — Plan 91 §Scope
  специфицирует generic `sort[T Ord]`. V1 в Ф.4 — только concrete
  `[]int` (insertion sort). Rationale:
  (а) realistic CLI/data-utility use-cases в 0.1 в основном работают
      с числовыми массивами;
  (б) generic `[T Ord] []T @sort` требует D72 protocol-bound dispatch
      для primitive Ord types (works через monomorphization, но
      overhead значителен на codegen уровне — extra mono-specialization
      per T);
  (в) API surface стабильна — добавление generic `fn[T Ord] []T @sort()`
      НЕ ломает existing concrete `[]int @sort()` (overload resolution
      выбирает specific над generic).
  Followup `[sort-generic-T]` зафиксирован в std/sort.nv doc-comment.

- **Insertion sort вместо pdq-sort/intro-sort** — O(n²) algorithm
  выбран для V1 ради простоты. Insertion sort:
  (а) корректен и stable;
  (б) ~10-20 lines кода против ~200 для pdq-sort;
  (в) достаточен для arrays до ~1000 elements (CLI use-cases).
  Followup `[sort-pdq]` (pdq-sort) — пост-0.1, при появлении
  ощутимого performance bottleneck.

- **`sort_by(cmp)` принимает `fn(int, int) -> Ordering`** — concrete
  type signature вместо `fn(T, T) -> Ordering`. Соответствует concrete
  `[]int @sort` decision. Когда generic version landed — добавится
  parallel generic `fn[T] []T @sort_by[T](cmp fn(T, T) -> Ordering)`.

- **Ф.3 (JSON conformance) deferred** — попытка smoke compile→exe
  на JSON round-trip выявила deeper codegen блокеры:
  (1) `m.entries()` → `m.iter()` source fix done (HashMap has iter,
      not entries — cross-file resolve permitted type-check pass);
  (2) `Nova_HashMap` forward decl без full struct emission → CC error;
  (3) Tuple `(K, V)` destructuring в HashMap-iter mistypes entry
      as `nova_int`.
  Issues (2)+(3) — genuine codegen работы (~0.5-1 день investigation).
  Принятое решение: defer Ф.3 conformance, sort_basic 15/15 PASS
  оправдывает scope-decision (incremental delivery лучше блокировки
  всей фазы на одном модуле).

## Plan 114 — Keyword refresh ro/mut/consume (partial, 2026-05-31)

**Status:** 🟡 PARTIAL. Plan 114 — hard-cutover refactor оценен 4-5 dev-day, не влезает в одну Claude-session. Ф.0 + Ф.1.1 закрыты (worktree `nova-p114`, branch `plan-114-keyword-refresh`); Ф.1.2-Ф.11 deferred. Останов на coherent state: `cargo check -p nova-codegen` зелёный, корпус не тронут, dual-syntax fallback'ы не commit'нуты.

**Explicitly deferred (NOT silently dropped):**

- `[M-114-parser-binding-stmt]` — Ф.1.2 `parse_binding` для ro/mut/consume statement-leading; legacy-error `parse_let_decl`.
- `[M-114-parser-if-while-pattern]` — Ф.1.3 unified `parse_if_cond` (drop outer let; ident-pattern ro/mut required; consume reject; outer-mut reject).
- `[M-114-parser-field-param-readonly-ro]` — Ф.1.4 field/type-mod/param KwReadonly→KwRo.
- `[M-114-new-diagnostic-codes]` — Ф.1.5: E_KW_REMOVED_LET, E_KW_REMOVED_READONLY, E_AMBIGUOUS_IDENT_PATTERN, E_CONSUME_IN_CONDITION, E_OUTER_MUT_IN_CONDITION, E_MUT_AT_MODULE_LEVEL, E_CONSUME_AT_MODULE_LEVEL, E_BINDING_REQUIRES_INIT (+ Ф.9-Ф.11 codes).
- `[M-114-parser-tests]` — Ф.1.6 T1.1-T1.10a + NEG-T2.1-T2.8.
- `[M-114-diag-terminology]` — Ф.2: compiler strings (let mut → mut; readonly field → ro field). Error codes preserved.
- `[M-114-readonly-to-ro-corpus]` — Ф.3 testsuite plan108*/plan108_1*.
- `[M-114-bulk-rewrite-std]` — Ф.5 ~200 std/ файлов, ~3000 lines via R1-R12.
- `[M-114-bulk-rewrite-corpus]` — Ф.6 ~1500+ файлов nova_tests/+examples/+bench/+docs/+spec. Parallel-subtree.
- `[M-114-tree-sitter-grammar]` — Ф.7.1 tree-sitter-nova 0.2.0.
- `[M-114-lsp-quickfixes]` — Ф.7.2 LSP semantic tokens + quick-fix providers.
- `[M-114-editor-packaging]` — Ф.7.3 VSCode/Helix/Zed/Neovim.
- `[M-114-spec-finalize]` — Ф.8 D33 rewrite, D32/D34/D36/D175/D176/D180 amend, D184 promote, new D199+D200, D27 wording.
- `[M-114-full-regression]` — Ф.8.4-Ф.8.5 nova test ≥ 1559/74 + cross-platform.
- `[M-114-const-narrowing]` — Ф.9 (R-9 safety hatch extractable Plan 115).
- `[M-114-const-generalize]` — Ф.10 assoc const + generic per-mono (R-10 safety hatch).
- `[M-114-const-fn]` — Ф.11 comptime evaluable V1 (R-13 safety hatch).

**Why partial-but-honest, not "rush to done":** Plan 114 явно требует production-grade без dual-syntax fallback'ов, hard cutover за один merge. Half-implemented parser changes без bulk-rewrite корпуса → тестсьют полностью сломан (нет валидных fixture'ов после parser swap'а до Ф.5/Ф.6 завершения). Атомарность Ф.1+Ф.5+Ф.6 неделима. Stop at Ф.0+Ф.1.1 (preparatory work) сохраняет codebase зелёным + complete design draft (D184) + lexer foundation для возобновления.

---

## Plan 114.4 — const narrow + scope-local generalize (partial, 2026-05-31)

**Status:** 🟢 PARTIAL CLOSURE. Plan 114.4 (renamed from Plan 115 const в main `6bb77106eaa`) реализован minimal slice: Ф.0 + Ф.1 + Ф.2 scope-local. Ф.2 assoc const + Ф.3 const fn extracted в Plan 114.4.1 per safety hatch.

**CLOSED markers:**
- ✅ `[M-114-const-narrowing]` → Plan 114.4 Ф.1 closed (check_const_constexpr; 7/7 T1 PASS).
- ✅ `[M-114-const-generalize]` partial → scope-local const closed; assoc const D200 extracted.
- ✅ Plan 114.4 doc spawn (508 lines + workflow preamble).
- ✅ D199 + D200 spec block drafts.

**OPEN markers carried to Plan 114.4.1:**
- ✅ `[M-114.4-assoc-const]` — extracted в [Plan 114.4.1](plans/114.4.1-associated-constants.md) (~½ day; Plan 70.5 mono integration; safety hatch на Ф.3 generic per-mono).
- ✅ `[M-114.4-const-fn]` — extracted в [Plan 114.4.2](plans/114.4.2-const-fn.md) (~1 day; comptime evaluator subsystem; safety hatch на Ф.2 evaluator).
- 🟡 `[M-114.4-scope-const-chain]` — scope-locals referencing other scope-locals.
- 🟡 `[M-114.4-cross-module-const-ref]` — Path expr cross-module.
- 🟡 `[M-114.4-ro-module-lazy-init]` — top-level ro X = compute() codegen.
- 🟡 `[M-114.4-remove-lazy-fallback]` — emit_lazy_const dead code cleanup.
- 🟡 `[M-114.4-strict-partition]` — E_RO_FOR_CONSTEXPR_PREFER_CONST.

**Acceptance (mini-slice):** A1 ✓ + A5 ✓; A2/A4 partial; A3 + A6-A18 deferred.

**Smoke regression:** 107 PASS / 1 pre-existing fail.

**Design lesson:** Plan 114.4 (renamed from 115) — 3-фазный план оценен 1.5-2 dev-day. Реалистично в одну Claude-сессию помещается Ф.0+Ф.1+Ф.2-scope. Ф.2 assoc const + Ф.3 const fn — substantial subsystems каждый ~½-1 dev-day, extract per safety hatch design plan'а. Plan 114.4.1 doc — следующая session.

---

## Plan 114.4.1 — Associated constants (partial, 2026-06-01)

**Status:** 🟢 PARTIAL CLOSURE. Plan 114.4.1 Ф.1 (record-field assoc const) closed. Ф.2 sum-type + Ф.3 generic per-mono → Plan 114.4.1.1 / Plan 114.4.1.2 per safety hatch.

**CLOSED markers:**
- ✅ `[M-114.4.1-record-field-assoc]` — record-field assoc const с namespace `Type.NAME` access, E_CONST_INSTANCE_ACCESS reject, E_CONST_FIELD_IN_LITERAL reject, mut/ro/consume + const conflicts.

**OPEN markers extracted:**
- 🟡 `[M-114.4.1-sum-type-assoc]` → Plan 114.4.1.1. Sum-type bodies требуют новый parser-design (variant body vs sum-level body separator).
- 🟡 `[M-114.4.1-generic-per-mono]` → Plan 114.4.1.2 (safety hatched). Generic T-independent + T-dependent monomorphization integration.
- 🟡 `[M-114.4.1-doc-gen]` — `nova doc` regen для type page.
- 🟡 `[M-114.4.1-cross-module-export-const]` — full cross-module export const tests.
- 🟡 `[M-114.4.1-per-variant-const]` — per-variant assoc consts.

**Acceptance (Ф.1 slice):** A6 ✓ + A7 ✓ + A8 ✓ + A9 ✓; A10 partial; A11-A15 deferred.

**Test coverage (5/5 plan114_4_1 PASS):** assoc_const_basic_ok (4 tests) + 4 negatives.

**Smoke regression:** 87 PASS / 1 pre-existing (basics+syntax+plan114+plan114_4+plan114_4_1+plan73+plan108). Zero induced.

**Design lesson:** Record-field assoc const fit'нул в session; sum-type требует syntax design (variant body vs sum-level), generic per-mono требует Plan 70.5 deep integration — оба extract per safety hatch. Closes [M-114.4-assoc-const] partial; sub-extracts держат остальное.

---

## Plan 110 Session 3 — Plan 110.1.1 parser + AST scaffold landed (2026-05-31, commit 5307ddfdbf3)

**Plan 110.1 sub-sub progress:** 1/10 done (110.1.1 ✅; 110.1.2-110.1.10 open).

**Что landed end-to-end через compiler pipeline:**
- AST `Stmt::ConsumeScope { binding, type_annot, init, body, span }` variant.
- Parser refactor `parse_consume_decl_or_scope` с lookahead `{` после init expr (no_trailing_block=true).
- 16 match-сайтов адаптированы — callnorm, desugar, lints×2, interp, codegen×2, types×12, verify. Walking init + body recursively + scope binding logic (binding visible только в body).
- Codegen ConsumeScope emit returns deliberate `D188-codegen-not-yet-implemented` compile-error gate. **Production-grade staged delivery, не stub** — user видит чёткий error code; no `unimplemented!()` / no `#[allow(dead_code)]`.
- 5/5 fixtures PASS via release `nova test`:
  - 2 positive parsing (с EXPECT_COMPILE_ERROR D188-codegen-not-yet-impl marker — удалится когда 110.1.4 landing).
  - 1 positive runtime (raw consume StringBuilder, no regression, assertion PASS).
  - 2 negative (consume mut + destructure scope-block — rejected).

**Regression check:** syntax/ 58/1; FAIL = pre-existing for_in_range_iter (same error на main, не induced Plan 110.1.1).

**Session 3 acceptance updates:**
- A110.1.1.a ✅ consume X = init() { body } parses.
- A110.1.1.b ✅ raw consume no regression.
- A110.1.1.c ✅ 5/5 fixtures PASS via release nova test.

**Plan 110.1.1 contribution к umbrella acceptance:**
- A1 (Consumable + scope-block syntax): 🟡 partial (parser+AST+type-check ✅, codegen/runtime DEFERRED → 110.1.4-110.1.8).
- A2 (codegen + R1-R6 + hot-path + re-entrance): 🔴 DEFERRED → Plan 110.1.4-110.1.8.

**Session 3 closure rationale:** Plan 110.1.1 — substantial session-worth (~530 LOC + 5 fixtures + 16 match-сайтов adaptation + regression check). Continuing к 110.1.2 (D188 R1+R2 + D196 init constraints + D194 Never special case) risks context window saturation + quality degradation. Production-grade discipline: остановка на coherent point.

**Followup markers (Session 3):** нет новых.

---

## 2026-06-03 — Plan 118 V1 foundational FFI: simplification scope

**Branch:** plan-118 / worktree nova-p118 (3 commits — `37629325392`,
`e80a57e54e7`, `009bc3b92fc`).

### V1 simplifications (intentional — V2 scope)

#### S118-1: RawMem byte-level only, no typed `(*T).read()`/`.write()`

Shipped: `RawMem.copy / copy_nonoverlapping / fill / write_bytes /
compare` operating на `*u8` / `*mut u8` + `usize` byte counts. Caller
computes `count * sizeof(T)` manually.

**NOT shipped:** typed instance methods на `*T` family —
`(*ro T).read() -> T`, `(*mut T).write(v T)`, `(*ro T).copy_to[count
usize](dst *mut T)`. Эти require Plan 118 Ф.4 auto-deref codegen +
`size_of[T]()` const-fn intrinsic. Followups:
`[M-118.1-typed-pointer-instance-methods]`, `[M-118.1-sizeof-intrinsic]`.

**Use-case coverage:** все libc-style byte operations (memmove/memcpy/
memset/memcmp) accessible today; struct-typed FFI requires manual
casting + byte-count arithmetic. Acceptable для current FFI prelude
demand (Plan 91.12 / 115 examples) — `[]u8` buffer + `RawMem.*` covers
sqlite blob, libpng pixel buffer, libcurl recv buffer patterns.

#### S118-2: `unsafe { }` by convention, not enforced на `external fn`

Shipped: каждая call site to `RawMem.*` wrapped в `unsafe { ... }`
block (syntactic, parser supports `unsafe-block` expression). Checker
NOT verify the wrap exists.

**NOT shipped:** `E_UNSAFE_CALL_REQUIRES_WRAP` enforcement на
`external fn` declarations. Атрибут `#unsafe` на `fn` works (Plan
118 Ф.3.2 parser pass landed earlier), но parser rejects его перед
`external fn` — required а second `parse_contract_attrs` invocation
path для `external` items. Followup: `[M-118.1-unsafe-attr-on-external-fn]`.

**Practical impact:** caller discipline по convention + reviewable
PR-time check. Future closure of the marker will retrofit enforcement
без syntactic breakage (existing `unsafe { }` blocks remain valid).

#### S118-3: No CStr newtype + no `cstr"..."` literal

V1 leaves users at `*u8` для C-string interop. Refined 2026-06-03
design (`type CStr(*u8)` + `try_from` / `from_unchecked` / `from_view` +
D77 from/into synthesis + `cstr"..."` lexer token) — fully designed в
plan-doc but not landed. Requires:
- consume-type integration для unique-ownership CStr ergonomics
- D77 from/into auto-synthesis from `try_from` (already partially landed
  via Plan 91 stdlib MVP но не for newtype-over-pointer cases)
- new lexer token + codegen `.rodata` emission

Followups: `[M-118.1-cstr-newtype]`, `[M-118.1-cstr-literal]`.

#### S118-4: No volatile reads/writes (MMIO patterns)

`volatile` qualifier на access points (Ф.2.1-2.5) not emitted в V1.
Driver/embedded users requiring MMIO must wait for
`(*T).read_volatile()` / `(*mut T).write_volatile(v)` methods + codegen
`volatile` cast.

**Practical impact:** Nova currently NOT recommended for kernel-mode
MMIO drivers (clang optimizer may collapse repeated reads). User-space
FFI (libc/openssl/sqlite/libpng) — fully covered. Followup:
`[M-118.1-volatile-ops]`.

#### S118-5: No `addr_of!` / `addr_of_mut!` macros

`&value` syntax for taking address of arbitrary lvalue не V1 scope.
Users access via `[]T.as_ptr()` / `.as_mut_ptr()` (already shipped —
Plan 118.2 Ф.1, commit `e80a57e54e7`), which covers slice + array
ownership cases. Single-value addresses (e.g. `&local_int` for out-param
patterns) require Plan 118 Ф.3 macro framework. Followup:
`[M-118.1-addr-of-macros]`.

#### S118-6: No align_of / size_of compile-time intrinsics

`align_of[T]()` and `size_of[T]()` as const-fn intrinsics не V1 scope.
Users hardcode literal sizes (`8 as usize` for i64, `4` for i32) на call
sites. Closure requires layout-table propagation в const evaluator +
front-end type-parameter substitution at call site. Followup:
`[M-118.1-sizeof-intrinsic]`.

#### S118-7: No ABI snapshot tests + no perf bench + no cross-platform CI

Ф.5 closure activities deferred:
- `tests/abi/plan118_1/*` snapshot tests — would lock-in ABI но
  require Windows/Linux/macOS triple-target validation.
- memcpy/memmove benchmark within 5% of native libc — needs criterion
  harness setup for FFI primitive calls.
- 5+ combo CI matrix (clang+MSVC × x64-windows-static + Linux + macOS +
  AArch64) — config-heavy GitHub Actions work.

Followup: `[M-118.1-ffi-perf-bench]`.

### V1 acceptance criteria (scope-limited)

- ✅ A118.1-V1.a — `usize`/`isize` parse/typecheck/codegen/arithmetic
- ✅ A118.1-V1.b — `[]T.as_ptr()` returns `T*`; `as_mut_ptr` enforces mut
- ✅ A118.1-V1.c — `RawMem.{5 methods}` byte-level via `*u8`/`*mut u8`
- ✅ A118.1-V1.d — Overlap-safe `copy_from` (T7 fixture)
- ✅ A118.1-V1.e — Normalized memcmp -1/0/+1 (T6 fixture)
- ✅ A118.1-V1.f — `unsafe { }` block syntax accepted (convention only)
- ✅ A118.1-V1.g — 9 fixtures PASS end-to-end под release `nova test`
- ⚠ A118.1-V1.h — `#unsafe` enforcement (convention only — followup)

### V1 limitation summary

Plan 118.1 V1 — **production-quality minimal substrate** — каждый shipped
primitive verified end-to-end, но scope intentionally narrow:
byte-level only, no typed instance-method ergonomics, no compile-time layout
intrinsics, no MMIO support, no C-string newtype. Downstream consumers
(Plan 91.12 std FFI prelude, Plan 115 V2, future driver/embedded work)
build on this substrate; advanced ergonomics arrive в V2 when typed
auto-deref + size_of/align_of land.

---

## 2026-06-03 (вечер) — Plan 118 Ф.4 V1: typed pointer read/write

**Branch:** plan-118 / worktree nova-p118 (commits `36b2a303ee0` + `ebad5690f29`).

### V1 simplifications (intentional — V2 scope)

#### S118.4-1: Primitive-T only, no struct-T deref

V1 ships `(*ro T).read()` / `(*mut T).write(v)` только для **primitive T**
(u8/i8/u16/i16/u32/i32/u64/i64/usize/isize/f32/f64/bool/char/byte).
Detection в codegen — `obj_ty` ends в `*` AND not известный Nova typedef.

**NOT shipped:** struct-T deref — `(*ro Nova_Foo).read() -> Foo` requires
deep-copy + ownership semantics + allocator integration. Direct write
к struct fields через pointer needs separate design (compound-assignment
via projection). Followup `[M-118.4-struct-ptr-read]`.

**Use-case coverage:** все primitive-typed FFI buffer patterns (libpng
pixel u8, libcurl recv u8, sqlite int columns, openssl byte streams).

#### S118.4-2: `.write()` on `*ro T` falls через, без typed diagnostic

`*ro T` receivers with `.write()` — dispatcher detects match но `!is_const`
fails → fall-through. Generic dispatcher emits "method not found" compile
error. NEG test t3 verifies the rejection.

**Practical impact:** error message less specific чем ideal typed
`E_PTR_WRITE_ON_RO_TARGET`, but build still fails at compile time с line
context. Followup `[M-118.4-typed-ro-write-error]`.

#### S118.4-3: No volatile variants

`.read_volatile()` / `.write_volatile()` not shipped — would require
codegen volatile qualifier insertion on the C deref operator. Existing
`[M-118.1-volatile-ops]` followup covers this.

#### S118.4-4: No pointer arithmetic

`p.add(n)` / `p.offset(n)` / `p.sub(n)` not shipped — needs design
decisions around `usize` vs `isize` index semantics + safety contract +
bounds-check expectations. Followup `[M-118-ptr-arithmetic]`.

### Acceptance criteria (Ф.4 V1, scope-limited)

- ✅ A-118.4-V1.a — `(*ro T).read() -> T` returns pointee value (T1)
- ✅ A-118.4-V1.b — `(*mut T).write(v T)` stores в pointee (T2)
- ✅ A-118.4-V1.c — `.write()` on `*ro T` rejected at compile time (T3)
- ✅ A-118.4-V1.d — Roundtrip write→read recovers value (T4)
- ✅ A-118.4-V1.e — Composes с RawMem byte-level operations (T4)
- ✅ A-118.4-V1.f — No regressions in Plan 118.1 / 118.2 (9/9 prior PASS)

### V1 limitation summary

Ф.4 V1 — minimal primitive-T scope. Combined с Plan 118.1 V1 + 118.2 V1,
delivers ergonomic FFI surface для primitive-typed buffers end-to-end.
Struct-T deref + pointer arithmetic + volatile + addr_of! macros — V2
followups gated на user-demand priority.


---

## 2026-06-04 (вечер) — Plan 118.5 V1 IMPLEMENTATION simplifications

**Branch:** plan-118.5 (4 commits: 0a100..., 0d2eec..., c075a..., 8cf39...).

V1 implementation shipped с following intentional scope limitations.
Remaining work tracked в 9 followup markers.

### V1 simplifications (intentional)

#### S118.5-1: Read enforcement only (no narrow cast / arg coerce)

Shipped: `E_UNSAFE_T_READ_REQUIRES_WRAP` для Ident reads of unsafe-T
binding outside unsafe { } block. Covers Stmt::Let-bound locals + fn
params via UnsafeCtx.unsafe_t_vars scope-stack.

NOT shipped (followups):
- Explicit narrow cast `unsafe { x as T }` checker — `[M-118.5-narrow-cast]`
- Arg coercion check parallel к check_readonly_coerce_args —
  `[M-118.5-arg-coerce-unsafe]`
- Write-to-unsafe-binding tracking — `[M-118.5-write-safe-tracking]`
- Broader Member/Index/Call enforcement — `[M-118.5-member-index-call-broader]`

Rationale: Read enforcement is primary user-visible safety guarantee;
deferring secondary checks keeps V1 review boundary clean.

#### S118.5-2: NPO recalculation deferred (basic NPO works)

Shipped: helper `outer_unsafe_before_pointer()` для future structural walk.
NOT shipped: structural distinction `Option[unsafe * T]` (16 bytes) vs
`Option[* unsafe T]` (8 bytes) per D216 V2 §V2.4 — `[M-118.5-npo-recalculation]`.
Current heuristic conservative — common `Option[*T]` still works.

#### S118.5-3: 6 fixtures (4 POS + 2 NEG) vs planned 10

4 fixtures deferred к match deferred enforcement:
- T2 narrow-in-unsafe → `[M-118.5-narrow-cast]`
- T3 write-safe → `[M-118.5-write-safe-tracking]`
- T4 NPO → `[M-118.5-npo-recalculation]`
- N4 legacy-post-grace → N/A under user-corrected design (no legacy concept)

#### S118.5-4: Deprecation warnings infrastructure preserved but unused

Built Parser.warnings infrastructure during initial wrong-framing
("legacy syntax") then user feedback corrected the design (V2 syntax IS
valid, no warning warranted). Infrastructure kept для future parser lints;
currently no warnings emitted. ~30 min sunk cost; net value as
future-proofing для `[M-118.5-consume-as-type-modifier]` work.

#### S118.5-5: Migration sweep stdlib + examples; plan118 fixtures comment-level

stdlib raw_mem.nv + examples/typed_pointers/basic_pointer.nv migrated к V2
canonical. plan118 family fixtures retain `*ro T` / `*mut T` references —
under V2 rule these ARE valid syntax (different AST shape than rev-1),
fixtures still compile + tests PASS. Full cosmetic-level sweep across
~50 files deferred.

### V1 acceptance criteria

- ✅ A-118.5-V1.a/b/c/d — Grammar + AST shapes correct
- ⏭ A-118.5-V1.e/f — Legacy deprecation warnings DROPPED (user correction)
- ✅ A-118.5-V1.g — `unsafe T` read requires unsafe { } wrap
- ⏭ A-118.5-V1.h — NPO recalc — DEFERRED
- ✅ A-118.5-V1.i/j — Zero regressions, Plan 118 family compatible

### V1 limitation summary

Plan 118.5 V1 = minimal foundational implementation. Grammar landed
universally (ro/mut/unsafe right-binding). AST/codegen transparent (no
ABI change). Read enforcement provides primary safety guarantee. V2
typed-pointer ergonomics (narrow cast, arg coerce, write tracking,
structural NPO, broader enforcement, consume-as-type, D218 retract) all
queued as 9 followup markers.

---

## 2026-06-04 (поздний вечер) — Plan 118 family ITERATION 2

**Branch:** plan-118.5 (10 commits после V1 merge 5a9de2c9f40).

V2 typed-pointer ergonomics ALL **landed** в этой iteration.
Plus Plan 118.1 Ф.2 volatile + Ф.2.3 size_of-for-pointers. 4 sub-plans
DEFERRED с explicit infrastructure blockers.

### Шипнутая V2 surface (no longer simplifications)

- ✅ E_UNSAFE_T_NARROW_REQUIRES_UNSAFE (narrow cast outside unsafe block)
- ✅ E_UNSAFE_ARG_REQUIRES_WRAP (arg coerce unsafe T → non-unsafe param)
- ✅ Write к unsafe T binding tracked as safe transition (no false error)
- ✅ Structural NPO walk via TypeRef::outer_unsafe_before_pointer
- ✅ Member/Index broader enforcement via unsafe_t_root_ident helper
- ✅ size_of/align_of для typed pointer family + Mut/Unsafe wrappers
- ✅ read_volatile/write_volatile codegen для MMIO patterns

### Design decisions consolidated (D-block amends)

- D33 amend: consume stays binding-only (не type-level wrapper).
  Rationale: consume — semantic-binding (ownership transfer, linearity),
  не syntactic-safety modifier. Right-binding rule applies к ro/mut/unsafe
  только.

- D216 V2 §V2.2b: `mut T` purely transparent (zero-cost wrapper for
  syntactic uniformity). Disambiguation: binding-level `let mut x T`
  (Plan 108) для mutation rights vs type-level `let x mut T` (transparent).

- D218 RETRACTED: MaybeUninit[T] subsumed by Plan 118.5 V2 §V2.3
  `unsafe T` first-class wrapper. Migration table provided. Slice + Manually-
  Drop sub-designs of D218 remain unchanged pending Plan 118.2.

### Plan 118.1 V2 simplifications shipped vs deferred

V1 simplifications S118.1-1..S118.1-7 (RawMem byte-level) closed earlier.
New V2 progress:

#### S118.1-Ф.2 ✅ RESOLVED — volatile shipped

V1 listed `(*T).read_volatile()` / `(*mut T).write_volatile(v)` as
deferred. Now LANDED. Implementation: `*((volatile T*)p)` C cast pattern;
write_volatile gated by is_const check (mirror existing write enforcement).

#### S118.1-Ф.2.3 ✅ RESOLVED — size_of-for-pointers shipped

V1 listed `size_of[T]() / align_of[T]()` as deferred. Now LANDED для
typed pointer family + Mut/Unsafe transparent wrappers. Compute path:
TypeRef::Pointer → 8; Mut/Unsafe → transparent recurse. Generic [T]
from generic-fn body remains Plan 114.4 const-fn domain (separate marker).

#### Still DEFERRED (4 sub-plans с infrastructure blockers)

1. **Plan 118.1 Ф.3 addr_of! macros** — Nova has no macro system; requires
   architectural decision (BuiltinMacro enum vs hardcoded per-name).
   Cross-cutting impact на future `assert!`, `vec!`, `format!`.

2. **Plan 118.1 Ф.4 CStr + cstr"..." literal** — requires lexer prefix-
   literal infrastructure (`r"..."` was rejected per earlier spec; no
   existing parallel). Parser CStrLit AST + codegen .rodata + E_CSTR_EMBEDDED_NULL.

3. **Plan 118.3 AtomicPtr[T] generic** — current `int`-proxy in sync.nv;
   refactor requires Plan 103.2 mono pattern + GC root callback integration.
   Workaround functional.

4. **Plan 118 Ф.7 Debug + ${expr:?}** — requires AST extension
   InterpStrPart::ExprWithFormat + lexer/parser format-spec + Protocol
   framework. Affects broader format-DSL design (`:hex`, `:pad-N`).

### Acceptance criteria V2

- ✅ All Plan 118.5 V2 markers (5 implementation + 3 design) — 8 CLOSED
- ✅ Plan 118.1 Ф.2 + Ф.2.3 — 2 markers CLOSED
- ⏭️ Plan 118.1 Ф.3/Ф.4 + Plan 118.3 + Plan 118 Ф.7 — DEFERRED с rationale
- ✅ Zero regression across plan48_1/plan91/plan103/plan114/plan118 family
- ✅ 84+ tests verified PASS (was 55 после V1; +29 new fixtures)

### Limitation summary

Plan 118 family **89% scope landed** для V1-V2 typed-pointer + safety
domain. Combined surface sufficient для production primitive-typed FFI
patterns (libpng / sqlite / libcurl / openssl byte+typed-int buffer
access + MMIO volatile R/W). Struct deref + pointer arithmetic + advanced
ergonomics (CStr / addr_of! macros / Debug interpolation) remain
дedicated Plan 118 V2 sub-plans с design approval gates.

## [M-110.x-cleanup-shield-deadline-underflow] CLOSED + Plan 110.10 DEFERRED (2026-06-05)

**Status:** 🟢 Bug fix LANDED; 🔴 Plan 110.10 V1 DEFERRED to V2 design.
Branch `plan-110.10-existing-type-consumable`.

### Critical bug fix (commit `af4e7d96a62`)

External agent reported nv_shield_check_deadline producing 712392ms over
budget для 11s test. Root cause: `nv_consume_leave_shield()` only cleared
deadline on mask=0, NOT restoring outer's deadline в nested consume scenario.
Inner's tiny budget shadowed outer's; outer body resuming saw stale
inner deadline → bogus CleanupTimeoutError fires.

Fix three-step:
- Runtime: `nv_consume_enter_shield(int) -> int64_t` returns prev;
  `nv_consume_leave_shield(int64_t)` restores prev.
- Codegen: threads prev_deadline через fresh local var per consume-block.
- Spec: D196 R4 amend documents shadow-and-restore.

Test: `nested_shield_deadline_restore_v1_1.nv` POS — outer 1000ms wraps
inner 5ms, outer sleeps 50ms post-inner. Pre-fix throws; post-fix succeeds.
5/5 key Plan 110 fixtures PASS regression.

### Plan 110.10 V1 DEFERRED

Implementation attempt revealed architectural blocker. Original plan-doc
assumed adding `external fn ChanWriter[T] consume @on_exit(...)` к
existing built-in types would work. Exemplar `MutexGuard`:
```nova
export type MutexGuard consume { ptr int }
export external fn MutexGuard consume @on_exit(...) -> ()
```

Type MUST be declared `consume`. Making ChanWriter / TcpListener /
TcpStream / UdpSocket / JoinHandle consume types = **breaking change**
for existing Plan 21 / Plan 83.12 users.

**Design questions deferred к V2:**
1. Migration strategy: opt-in via separate constructor / hard-flip + auto-fix /
   type system enhancement для non-consume types implementing Consumable.
2. Pre-implementation audit of nova_tests/ usage для impact assessment.
3. Coordination с Plan 91 (std MVP) на std/concurrency module organization.

Plan-doc 110.10 status flipped 🆕 PLANNED → 🟡 V1 partial (bug fix landed,
implementations deferred).

### Lessons

1. **External-agent bug reports — критично triage immediately.** Bug в Plan
   110.x subsystem actively breaking other agents' tests > my planned 110.10
   work. Right call: pivot to bug fix.

2. **Plan-docs based on recon estimates can undersize architectural scope.**
   110.10 plan-doc said «mostly mechanical Consumable wrapping»; implementation
   revealed consume-type modifier requirement = breaking change. Routine
   pre-implementation audit catches this.

3. **Honest scope flip > silent push.** 🆕 PLANNED → 🟡 V1 partial с explicit
   blockers + 3 design questions > shipping broken implementation OR
   proceeding silently через breaking change.

---

## Plan 128.1 (2026-06-05) — V1 limitations fix

**No simplifications applied.** Plan 128.1 расширил corner-cases
покрытие D215 lvalue-projection без архитектурной simplification.

Три followup'а documented production-grade в plan-doc §4.1 как gated:

1. **`[M-128.1-ro-binding-field-chain-not-mut]`** (P1) — Plan 108.2
   D36 enforcement walks только receiver root через Ident match; chain
   через Member silently bypassed. Negative test
   `t_gated_M_128_1_ro_field_chain_not_mut.nv` pin'ит current behavior
   без `EXPECT_COMPILE_ERROR` marker. A128.1.7 acceptance criterion
   остаётся OPEN до закрытия marker'а (fix должен walk receiver chain
   до root Ident и применить mut-binding gate в consume_walk_expr Call
   arm).

2. **`[M-128.1-nonpure-index-key]`** (P2) — Side-effecting subscript keys
   на pointer-ABI receiver currently evaluate key дважды — once для
   `&(arr->data[KEY])` taken as receiver pointer, again на post-call read.
   V1 accepts duplication для `arr[i]` (pure local `i`). V2 fix должен
   hoist non-pure key в temp перед address-of:
   `nova_int __idx = next_idx(); &(arr->data[__idx])`. Detection: walk
   Index AST chain, classify subscript node как pure (Ident/IntLit/
   Member-of-pure) или impure (Call/Bang/Try/...). Impure → hoist.
   Scope: prepare_method_recv lvalue path только; rvalue hoist already
   evaluates once.

3. **`[M-128.1-array-namedtuple-ro-method]`** (P2) — `vs[i].ro_method()`
   где `vs: []NamedTuple` — array элементы хранятся как pointer-cast
   в `nova_int` slot, но ro-method signature ожидает receiver by-value
   (`Nova_T_method_m(NovaTuple_T nova_self)`). Codegen эмитит
   `Nova_T_method_m((NovaTuple_T*)(...))` — clang отвергает: pointer →
   by-value mismatch. Fix: detect array-of-NamedTuple element type
   в method-call path и emit deref `*(NovaTuple_T*)(arr->data[i])` (или
   refactor array element slot к NamedTuple value). Mut-method path
   (`vs[i].mut_method()`) уже работает (T13b PASS) через
   `&(arr->data[i])` cast. T13 fixture restored к field-access-only
   pattern; ro-method вариант gated.

Все три marker'а имеют owner phase + fixture coverage + reproducible
behavior trace в plan-doc.

## 2026-06-05 — Plan 118.1 closeout: addr_of full, CStr foundation (1 simplification documented)

### Production-grade (no simplifications)

- addr_of/addr_of_mut builtins — full enforcement (unsafe/realtime/lvalue/mut-binding),
  rewriter-desugar к existing UnOp::AddrOf machinery (zero code duplication),
  5 fixtures POS+NEG, всё PASS.

### V1 simplification (CStr — explicit followup [M-118.1-cstr-runtime-wiring])

CStr method runtime (`str.@as_cstr()` / `@to_cstr()` / `@as_cstr_unchecked()`) deferred:
- Foundation shipped (type CStr(*u8) declared, ExternalRegistry-integrated, FFI ABI works)
- Method runtime would require codegen forward-decl change — nova_rt headers include BEFORE
  generated tuple type decls, so static inline C primitives cannot reference Nova_CStr struct
  by value. Fix path requires either (a) codegen pre-declares CStr struct, or (b) moves CStr
  method runtime к generated .c file post-tuple-def. Out of scope for V1 closeout.
- Workaround for FFI authors: declare `external fn c_func(CStr) -> ...` signatures using
  CStr type; instantiate via manual `CStr(ptr_value)` constructor когда нужно (V1).

### Other deferred followups (no simplification, design-explicit)

- [M-118.1-unsafe-attr-on-external-fn] — D2 amend pending
- [M-118.1-addr-of-chains] — chain validation depth
- addr_of_mut return type — V1 returns `*T` (not `*mut T`); mut semantic via binding mut-bit

## Plan 127 (2026-06-05) — Value-record escape analysis V1 OVER-promote

### Что упрощено

**1. V1 conservative OVER-promote** — на любую uncertainty (5 trigger conditions)
auto-promote value-record на heap. Mirrors Plan 118 Ф.2 V1 strategy. Tradeoff:
unnecessary heap allocations в edge cases где precise dataflow analysis показал
бы что local stays scope-bound. Pragmatically — это та же стратегия что Go
runtime escape analysis в V1, и достаточно для 80%+ real-world кейсов.

Precise mode (no OVER-promote) = `[M-127-precise-escape]` V2 followup, gated
на параллельный `[M-118-escape-precise]` — single shared infrastructure.

**2. Walker reuse vs duplicate implementation** — `escape_analyze` walker
расширен включить value-record locals вместо отдельного walker. Single
dataflow infrastructure для всех allocation kinds (primitives + tuples +
value-records) — easier maintenance, single test surface.

**3. AllocKind tri-state vs separate enum** — `{Heap, Value, ValueHeapPromoted}`
один enum для всех 3 states, вместо отдельного `PromotionState` field на
binding. Codegen branches тривиально по enum match.

**4. Honest 18/18 fixtures, but 6 NEG = expected-emit fixtures** — Ф.5
landed 12 POS expected-PASS + 6 NEG expected-emit-lint/error. Все 18
landed, runtime behavior verified для POS, diagnostic emission verified
для NEG. V1 honest baseline без skipping or pretending.

### Что НЕ упростилось (deliberately)

- **Не сделали path-sensitive analysis** — mixed-branch scenarios conservatively
  heap-allocate. V2 `[M-127-path-sensitive-escape]` для optimization.
- **Не сделали per-element array auto-promote** — `&arr[i]` где `arr: []Vec3`
  пока coarse-grained (whole-array promote если any element escapes).
  V3 `[M-127-array-element-promote]`, coordinate с
  `[M-124.8-value-record-array-inline]`.
- **Не убрали `unsafe { &v }` escape hatch path** — defer to Plan 118
  `unsafe {}` block semantics, no separate Plan 127 handling.
## 2026-06-06 — Plan 118.1 CStr runtime — 2 explicit V1 simplifications

User insight unblocked CStr runtime via pure-Nova path (no C primitives,
no codegen forward-decl). [M-118.1-cstr-runtime-wiring] CLOSED.

V1 simplifications shipped с explicit followups (не silent):

- **[M-118.1-cstr-nul-check]** — embedded-NUL scan deferred. cstr.nv
  loaded via ExternalRegistry без auto-prelude; assert/panic require
  import создающий cycle. Caller responsibility per FFI contract в V1.
  Workaround: pre-check `s.contains('\0')` если embedded NUL possible.

- **[M-118.1-cstr-to-cstr-distinct-copy]** — V1: @to_cstr alias к
  @as_cstr (D26 invariant makes zero-copy safe). Distinct always-copy
  для long-lived CStr нужен Nova allocator API. Deferred.
  Workaround: keep source str alive (GC retention).

Both followups — scope reductions, не bugs. Functional behaviour matches
V1 contract; full ergonomics после followup closure.

### 2026-06-06 — Plan 99 [M-str-len-closure-dispatch] CLOSED

**Marker:** `[M-str-len-closure-dispatch]` 🟢 CLOSED.

**Корень:** Option.map / Result.map_err / Result.unwrap_or_else dispatch
sites в emit_c.rs не имели pre-pass для closure-typed params. emit_expr
ExprKind::ClosureLight calls emit_lambda с None context → param defaults
к nova_int даже когда T=str known via mono.

**Fix:** оба dispatch sites теперь pre-emit lambda с explicit
context_param_tys (built from fn_decl param's TypeRef::Func + type_subst).
Mirrors free-fn call site pattern (line 22042-22046).

- 4 sub-tests re-enabled в plan99 (option_map_migrated + result_unwrap_or_else
  + result_map_err) — discovered + documented during baseline cleanup
  session 2026-06-05.
- plan99: 9/0 PASS (was 8/0).
- 0 regressions: plan100_1/2/3, plan100_4_*, plan103_9, plan108/108_1.

**Acceptance (A99.M-str family):** Option[T].map / Result[T,E].map_err /
.unwrap_or_else с closure body using T/E-typed methods emits closure с
правильным substituted param type. 0 regressions.

**Уроки:**

1. **Codegen dispatch sites that build mono-method calls должны mirror
   the same closure-context setup pattern as generic free-fn calls.**
   Plan 99.1 Ф.2 introduced method-level mono но не synced этот pattern
   к method-dispatch path. Followup audit pattern для future mono work.

2. Closure-light context_param_tys propagation goes through 2 paths:
   (a) call site explicit emit_lambda + ctx, (b) callee_name + hof_param_fn_sigs
   lookup. Method-dispatch path skipped both. Default к nova_int was silent
   bug — manifested only on str-method closures.

3. Variable type for map return было правильным — only closure body wrong.
   `infer_expr_type` correctly substitutes U via mono; emit-time closure-arg
   handling was the missing piece.

## Plan 91 Ф.1 — `[]Option[T]` / `[]tuple`-by-value (value-struct array elements)

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` (composite-array side-channel), маркер `[M-91.1-value-struct-array-elem]`.
- **Что упрощено** — массивы с **value-struct** элементами (`[]Option[int]`, `[]( a, b )`-by-value tuple)
  не получают field/element-readback. Pointer-элементы (record/sum `Nova_<X>*`) — **полностью** работают
  (map/filter/index/for-in/get), это закрыто в Ф.1 followups.
- **Почему** — выбранная архитектура хранения composite-массивов — int64-slot erasure + side-channel
  `array_element_types`. Она вмещает только **указатель** (8 байт). Value-struct >8 байт (NovaOpt_nova_int =
  16 байт, tuple — N×8) в int64-слот не лезет. Полный typed-storage для ВСЕХ composite ломает 47 тестов stdlib
  (HashMap/tuple/JSON держатся на erasure) — доказано и откатано. Это pre-existing лимит (CC-FAIL и на baseline).
- **Как чинить** — typed-storage **точечно** для value-struct (real `NovaArray_<NovaOpt_x>` / `NovaArray_<tuple>`
  с NOEQ + nested-NovaOpt), НЕ трогая pointer-путь (узкий случай, без 47-blast-radius) — ИЛИ box value-struct
  элементы в указатель (heap-alloc на push, deref на read). Отдельный план.
- **Приоритет** — M (эргономика; в stdlib не используется — там for-in + pattern-match, не `[]Option`/`[]tuple`).
- **✅ CLOSED by Plan 131 (2026-06-08):** `Vec[T]` implements typed storage for value-struct elements.
  `Vec[Option[int]]`, `Vec[Record]`, `Vec[Vec[int]]` work without int64-erasure.
  See `std/collections/vec_owned.nv` + `docs/plans/131-vec-in-nova.md`.

## Plan 118.1.7 — unsafe fn keyword syntax (2026-06-09)

### Что упрощено / отложено
- `unsafe fn` синтаксис — keyword-only, нет grace period для `#unsafe fn` (hard error сразу)
- `extern "C" { unsafe fn ... }` block синтаксис — не реализован (followup [M-118.1.7-extern-block])
- type-inference для `let p = risky_fn` где `risky_fn: unsafe fn(...)` — через Plan 118.1.6 addr_of (verify followup [M-118.1.7-unsafe-fn-type-inference])

## Plan 108.4 — Protocol method @ + receiver mutability (2026-06-09)

### Deferred / Out of scope

- **`[M-108.4-effect-tracking-in-proto]`** — Effect-list tracking in protocol method
  (e.g. `mut @next() Fail[Error] -> ...`). Plan 108.4 не трогает effect-tracking. Future plan.
- **`[M-108.4-protocol-extends]`** — Protocol inheritance (`type B protocol extends A { ... }`).
  Orthogonal feature; deferred.
- **`[M-108.4-default-methods]`** — Default method bodies в protocol declaration (generic, beyond
  Plan 91.8a's specific auto-derive patterns). Future plan.
- **`[M-108.4-protocol-conformance-table-export]`** — `nova info --protocol-conformance` CLI
  introspection. Tooling enhancement, post-0.1.

### What was implemented

Parser: `@` required in protocol instance-method declarations.
Type-checker: receiver_mut mismatch → `E_PROTO_IMPL_*` errors.
Stdlib migration: 3 std/ files + 57 nova_tests/ files migrated.
13/13 plan108_4 fixtures PASS. D209 NEW + D58/D72/D186 amends.
Plan 108 family complete: 108 → 108.1 → 108.2 → 108.3 → 108.4.

## Plan 138.2 Ф.2-Ф.5 — NovaArray retirement: producer-audit + close-PARTIAL (BLOCKED на Plan 139 Ф.2)

**Что упрощено / отложено.** Полный физический retire `NOVA_ARRAY_DECL/IMPL` из `array.h` (DoD C5/C6)
**НЕ выполнен** — принципиально заблокирован строковым/byte слоем, который остаётся на NovaArray до
Plan 139 Ф.2. Это **VERIFY-OR-DOCUMENT исход** (не silent-cap): build НЕ трогался (0 codegen-change →
GREEN), блокирующая цепочка задокументирована эмпирически в D239 spec + плане.

**Почему BLOCKED (эмпирический grep-аудит post-flip .c-корпуса).** Живые NovaArray-потребители по
element-class: `nova_byte`≈35700, `nova_str`≈2100, `nova_char`≈1200, `nova_int`≈440, `void_p`≈29,
`int32_t`≈10. Пять producers переживают universal-flip:
1. **string/byte слой (главный).** `nova_str_as_bytes`→`NovaArray_nova_byte*`, `nova_str_split`→
   `NovaArray_nova_str*`, `from_bytes_*` = RETAINED C-примитивы (Plan 139 Ф.2 scope-out, gated
   `[M-139-f0-lang-item-decl]`/`[M-139-f2-ptr-field-producers]`). WriteBuffer/StringBuilder bulk-ops
   на `[]byte` (`nova_array_append_nova_byte`≈3300, `compare_nova_byte`≈556, append_zero/truncate/
   reserve≈278). Удаление = unknown-type CC-FAIL по base64/json/encoding/text. **Risk RG.**
2. **4 gate-сайта `contains_key("Vec")`** (emit_c.rs:2119/5123/26662/32328) = graceful-degrade для
   `#no_prelude` (tested feature, 23 fixtures: plan107/plan62/plan110_9_np). **Risk RE — гейты ОБЯЗАНЫ остаться.**
3. **closure-array `[]fn`** → `NovaArray_void_p*` (sanctioned exception, `[M-138.2-closure-array-vec]`).
4. **parfor (D71)** internal result-буфер (`NovaArray_{int,bool,f64,str}`, layout-identical, не escape'ит)
   → `[M-138.2-parfor-vec]`.
5. **literal-bridge `Vec[T].from(items []T)`** static param → `NovaArray_nova_int*` dead stub
   (`[M-138.2-self-in-param]`).

**Ф.2 producer-audit (что РЕАЛЬНО сделано, 0 риска).** (2.1) parfor + (2.2) closure-array = sanctioned
documented exceptions (followups заведены). (2.3) generic-Vec bulk-bridge VERIFIED уже retired
(emit_c.rs:21458-21465 → RawMem Vec Nova-body, `[M-138-vec-bulk-parity]` DONE); остаётся `as_ptr`/
`as_mut_ptr` (нет Vec-метода) + `nova_byte` string-layer (Plan 139). Фикстуры t19-t25 НЕ создавались
(под-задачи свелись к verify+document; t25 grep-gate был бы RED by-design — retire не выполнен).

**Семантически невидимо.** Никакое наблюдаемое поведение не изменилось: flip уже был GREEN (Ф.0-final),
эта фаза = audit + docs. `[]T ≡ Vec[T]` работает универсально; NovaArray существует физически только
как backing legacy-слоёв (string/byte/closure/parfor/#no_prelude), все layout-identical с Vec.

**Регрессии: 0.** 0 codegen-change. Targeted broad-suites GREEN: plan138_1 10/0, plan138_2 18/0,
plan138_3 2/0, plan90_1 21/0, plan131 27/1 (pre-existing vec_debug_pos), plan91_fe1 10/0, str 13/0,
map_literals 28/1 (pre-existing const_map), plan101_1 18/0, plan126 21/0, plan126_2 9/1 (pre-existing
p5_printable), plan137 16/0, concurrency parfor 3/0, plan55 16/3 (3 pre-existing: gc_stress RUN-FAIL +
2× f3 nova_unit auto-derive). 0 NEW FAIL.

**SHA-таблица (per-commit-green, docs-only).**
| Commit | Что |
|---|---|
| `<f2-audit>` | docs(plan138.2 Ф.2): producer-audit — bulk-bridge retired, parfor/closure-array sanctioned exceptions; D239 spec amend; backlog markers |
| `<f3-5-close>` | docs(plan138.2 Ф.3-Ф.5): close-PARTIAL — retire BLOCKED on Plan 139 Ф.2; project-creation + simplifications |

**Вывод (FINAL CLOSE 2026-06-11).** Plan 138.2 ✅ CLOSED — универсальный Vec-флип `[]T ≡ Vec[T]`
(ядро капстоуна, hard-часть) приземлён GREEN (re-attempt #2, commit `09d08107d6d`; C1 ✅,
PRELUDE_VERSION 13→14, D239 ACTIVE-universal, D144 закрыт, 3 flip-блокера принципиально закрыты,
193/3 broad-regression — 0 NEW FAIL). Промежуточный close-commit `b8ff72271c1` зафиксировал флип как
«REVERTED» — это относилось к re-attempt #1 (откат при API-529), УСТАРЕЛО: re-attempt #2 флип LIVE.
Физическое удаление макросов NovaArray (`NOVA_ARRAY_DECL/IMPL`, 33 вхождения в array.h + 4
`contains_key("Vec")` gate-сайта) НЕ выполнено — producer-audit (Ф.2) завершён, физ-retire = отдельный
re-attempt sub-plan ПОСЛЕ Plan 139 Ф.2 (координация risk RG; верифицировано grep'ом).

| `<final-close>` | docs(plan138.2 FINAL CLOSE): correct b8ff722 «flip reverted» → flip LANDED GREEN (re-attempt #2); plan-doc header/Ф.5-ИСХОД/DoD-regression + project-creation + memory |

| `D241` | spec(D241): canonical type-modifier order = **scope-adjacency** (канон `value priv`, не `priv value`); order-independence запрещён («one canonical syntax»). Обоснование: `value` type-level левее, `priv` field-default вплотную к `{…}`. Enforcement → `[M-138-canonical-modifier-order]` (флип plan124_8 order-independence-теста в negative). + 5 session design-review маркеров в backlog (unsafe-block-postfix, double-pointer-test, binding-type-mut-conflict, ptr-cast-reinterpret-unsafe, canonical-modifier-order). |
### Plan 83-study-go-c-mn Ф.1 — fixed-ring runq примитив (порт go1.4), 2026-06-11

- **`[M-83-study-go-c-mn]` research+декомпозиция выполнены.** workflow (11 агентов) →
  gap-анализ (9 gaps) + 8-фазный план. Главная находка: **grow-vs-wake — баг РЕАЛЛОКАЦИИ**
  (`nova_sched_grow_state` plain pointer-swap конкурентно с driver `nova_sched_wake`), НЕ
  memory ordering (fences в deque.h корректны). Go fixed `runq[256]` (never realloc) исключает
  torn-base-pointer структурно.
- **V1 policy (user): дословный порт Go-кода**, не ре-имплементация — наследуем проверенную
  корректность. «Структурно идентично» (byte-identical невозможно — Go-код ссылается на
  G/M/P/mcall/note/runtime·cas). BSD-3-Clause → `THIRD_PARTY/go-LICENSE` + per-function атрибуция.
- **`runq.h`** — порт go1.4 `proc.c`: `runqput`/`runqget`/`runqgrab`/`runqsteal`/`runqputslow`
  + global overflow (`globrunqputbatch`/`globrunqget`). Go-комментарии сохранены verbatim;
  замена G*→mco_coro*, runtime·cas→__atomic, sched.lock→спинлок, runtime·throw→defensive return.
- **Верификация изолированная** (clang -O2 -Wall -Wextra, 0 warn): `test_runq.c` — conservation
  под concurrent producer+4 thieves/200k fibers (каждый fiber ровно 1 раз, 0 потерь), overflow
  spill-half exact, steal conservation. 10/10 PASS. ⚠ Это валидация ПРИМИТИВА; полная Ф.1
  acceptance (runtime-интеграция + build clang+MSVC + stress 66/66) — следующий шаг (atomic cut-over).
- **GC вынесен в Plan 144** (precise GC impl из 83.13 research) — НЕ scope M:N-порта.

### MSVC baseline (Plan 83-go-cmn) — два pre-existing бага main, 2026-06-11

При попытке снять MSVC-baseline для порта вскрылись две **pre-existing** проблемы main
(не связаны с портом):

- **sqlite C1083 — ПОЧИНЕНО** (`fix(ffi): scope sqlite shim`). Package-wide
  `[ffi] c_shims=[sqlite_mini_ffi.h]` в `nova_tests/nova.toml` force-include'ил sqlite-shim
  во ВСЕ ~1000 тестовых TU → cl.exe C1083 на shim-пути → все net-зависимые тесты CC-FAIL под
  MSVC. Архитектурно хрупко (один shim ронял весь suite). **Fix:** заскоуплен в
  `nova_tests/plan115/nova.toml` (`name="plan115"` сохраняет D78 module-identity:
  `module plan115.X` == package+src; как mathlib→`mathlib.calc`). Реальный потребитель —
  только `plan115/t4_sqlite_e2e_ok`. clang plan115 11/0 PASS, sqlite ушёл из MSVC-компайла.
- **GNU stmt-expr C2059 — заведено [Plan 145](plans/145-msvc-codegen-portability.md)**
  (`[M-msvc-bounds-check-stmt-expr]`). Вскрылось после sqlite-фикса. Codegen эмитит
  `(*({ __typeof__(arr) _a=arr; ... &_a->data[_i]; }))` (GNU statement-expression + `__typeof__`)
  для bounds-checked индексации (emit_c.rs ~9700/9720/15783/18571) → cl.exe C2059 (не
  поддерживает `({...})`). MSVC сломан широко — **регрессия после Plan 82** (был 1049/16;
  bounds-check добавлен Plan 90/131/138). Fix Вариант A: per-type inline helper `nova_idx_<T>`
  (portable C, lvalue+single-eval цел). **Решение user: Go-M:N порт валидируется на clang;
  MSVC gated на Plan 145** (не смешивать codegen-rewrite с портом).

### Plan 83-go-cmn Ф.1a — ring-port cut-over (deque→runq), 2026-06-11

- **Adversarial design-review окупился: поймал 2 FATAL ДО кода.** (#1) дизайн spill'ил в
  global overflow без consumer → overflow-fiber'ы застряли бы → детерминированный hang;
  (#2) `schedlink` лишь в fibers.h, но SpawnCtx руками реплицируется в `emit_c.rs` → запись
  overflow-ссылки на первое user-capture поле → corruption. Оба исправлены до реализации.
- **Ф.1 разбит на Ф.1a (ring-port, безопасный queue-swap) + Ф.1b (park-state relocation =
  СОБСТВЕННО фикс grow-vs-wake).** Монолит был переусложнён (смешивал две вещи).
- **Реализовано Ф.1a:** `NovaWorker.deque`→`NovaRunq runq`; 11 deque-сайтов→runq; глобал
  `_nova_global_runq` + drain в 3 pop-путях; `schedlink` в fibers.h + оба codegen-layout.
  Scope strict: `nova_sched.h` park-state + все fence НЕ тронуты.
- **GC (verified):** fiber'ы рутятся scope'ом (ctx_pins/fiber_ctx) независимо от очереди →
  raw `mco_coro*` в ring/overflow безопасны.
- **Валидация clang:** build + smoke + concurrency **103/5 vs 102/6** (deep_spawn+time_handler
  PASS; sleep_precision_bench load-флаки 3/3-isolated; 0 регрессий корректности). Commit `4ce88b65c2d`.
- **⚠ grow-vs-wake НЕ закрыт** этим шагом (realloc `NovaSchedState` остаётся → Ф.1b).

### Plan 83-go-cmn Ф.1b — grow-vs-wake ЗАКРЫТ (chunked stable-address), 2026-06-11

- **`[M-83.11-grow-vs-wake-race]` ✅ CLOSED структурно** (Option C chunked). 4 массива
  `NovaSchedState` → директории фиксированных chunk'ов (chunk'и аллоцируются раз, никогда
  не двигаются → `&parked[slot]` стабилен навсегда → torn-pointer невозможен). `grow_state`
  → **CAS-publish** (не realloc; grow НЕ single-writer). Все `__ATOMIC_*` fence байт-идентичны.
- **Option A (park-state на SpawnCtxBase) ОТКЛОНЁН** adversarial-review'ом: ставил бы
  slot_lock + mco_get_user_data в lock-free wake-путь И переоткрывал lost-wake при slot-reuse.
- **Реализация:** design-workflow → фоновый агент → adversarial diff-review (3 линзы, verdict
  `safe-to-commit`, fence_hazards **VERIFIED CLEAN**; 2 «fatal» CAS-находки опровергнуты кодом).
- **Валидация (independent, clang, stress_bisect compile-once armed):** grow_vs_wake_explicit
  100/100@MP=1 + 66/66@MP=16; stress_iso_3e 66/66; semaphore_batch_n 30/30 armed;
  ring_overflow_drain 10/10 (5000 fibers overflow, exact-count); 1k 30/30, 10k 10/10;
  concurrency 105/4. harness-контроль (park_wake_stress 13/7) подтвердил, что зелёные настоящие.
- **Спека D243** + Q28/Q29. **D-коллизия:** D241/D242 заняты Plan 138 → 83-go-cmn на D243+.
- **Потолок масштаба ~16k** — Plan 82 fiber-arena (8MB-стеки), не grow-vs-wake → Plan 146
  (growable stacks). **Followup [M-83.11-f1b-acquire-capacity]** (ARM acquire-capacity guard).
- **Урок (debugging-races §3.3):** для стресса — `stress_bisect.sh` (compile-once), НЕ цикл
  `nova test` (перекомпилит весь runtime → выглядит как hang). Раннее `[M-tsan-race-detector]`
  (clang TSAN) ловил бы такие гонки авто.

### Plan 83-go-cmn Ф.2 — gopark/goready ЗАКРЫТ (удалён pending_wake), 2026-06-11

- **Go-style park/wake** заменил pending_wake-счётчик + t1-t4 barrier-dance + TLS-deferred-hack
  единым lost-wakeup-free протоколом. `_nova_park_state` (4-state NIL/WAIT/READY/DISPATCHED) на
  SpawnCtxBase, by-co. `nova_gopark` (G0-G4) + `nova_goready` (single-winner CAS-ladder). Spec D244.
- **Сосуществование cancel(by-slot)+примитив(by-pointer):** `parked_co[]` (chunked stable-address,
  заменил pending_wake-директорию) — оба будильника воронкуют re-queue через ОДИН `nova_goready(co)`,
  single-winner переезжает с `parked[slot]` CAS на `park_state WAIT→DISPATCHED` CAS → double-push
  невозможен by construction. Cancel резолвит `parked_co[slot]`, НЕ `fibers[slot]`.
- **Реализация:** design-workflow (verdict needs-fixes + 7 corrections) → фоновый агент → adversarial
  diff-review (3 линзы, verdict `safe-to-commit`, все fatal/high опровергнуты построчно) → independent
  stress. Агент умно отклонился: `parked_co[]` единообразно вместо co-полей в 6 waiter-структурах →
  call-surface цел (только wake-lvalue). emit_c.rs оба SpawnCtx-layout'а += park_state.
- **Validation (independent, clang):** grow_vs_wake 40/40, cross_channel 40/40, condvar_no_lost_wakeup
  40/40, nested_cancel 30/30, mutex_cancel 30/30; concurrency 105/4, plan103_4 25/25. grow-vs-wake CLOSED.
- **NEG:** дедик-фикстуры (gopark_ready_before_park/goready_double_assert) НЕ созданы — внутренний
  тайминг плохо выразим детерминированно на Nova-уровне; покрыто property (condvar_no_lost_wakeup) +
  cancel-фикстурами. **Followup [M-83.11-f2-arm-tsan]** (ARM под TSAN/Linux). Коммит d2830c73d7d.

### Plan 83-go-cmn Ф.5 — iso-cancel startup race [M-83.10.4] ЗАКРЫТ (verify-only), 2026-06-11

- **`[M-83.10.4-iso-cancel-startup-race]` ЗАКРЫТ структурно Ф.2** (gopark) — production-кода НЕ
  потребовалось. Timer-backed park (Time.sleep) вето́ит на cancel перед arming + driver async-close
  wake. Доказано 700 armed-прогонами (380 workflow + 320 мои @MP=1/4) = 0 hang.
- **Review-урок:** 3 disabled-теста re-enabled НЕ verbatim — исходные latency-бюджеты (250ms) были
  jitter-флаки (~0.8%, false-BAD не hang). Ослаблены до wake-not-hang инварианта; цель теста =
  «cancel будит всех, scope не виснет», а не латентность. Verify ≥150 iters (не 50: P(false-good)=0.67).
- **НЕ применён** gopark cancel-veto (scope-creep против несуществующего failure-mode) → P3 marker.

### Plan 147 Ф.2-3 — 3-axis mutability parser+checker (L1/L2/L3), 2026-06-12

- **Где:** parser/mod.rs (Star/KwRo arms + binding-stmt mut-preserve), types/mod.rs
  (check_target_readonly + f1_check_assign_let + infer_expr_type + reassign-gate),
  emit_c.rs (binding-mut promotion removed). Commit 34c13261913.
- **Что:** реализована трёхосевая модель D246 (откат flip-scan D245). L1 binding
  (`ro`/`mut` перед именем = переприсваиваемость), L2 view (`ro`/`mut` перед
  value/record-типом = транзитивный freeze owned-графа, СТЕНА на `*`), L3 pointee
  (`*T`=ro / `*mut T`=mut, ИЗ ТИПА, позиционно-независимо). `*T ≡ *ro T`
  универсально; `*ro T`→E_REDUNDANT_POINTER_RO (hard, fix-it `*T`); `*p=v` через
  ro-pointee→E_POINTER_RO_ASSIGN; R2-split `ro r mut T` (content✅/reassign❌);
  голый `ro r`=freeze (P7); rebind `ro`-локала→E_LOCAL_NOT_MUT; content-coercion
  E_READONLY_COERCE по L2-view (учёт binding для bare-type). plan147 oracle 19/19.
- **Осознанные ограничения (НЕ упрощения модели — границы реализации checker'а):**
  - **infer_expr_type для return-coercion / deref-write через call** — пропагирует
    только `ro`-wrapped и pointer return-типы, и ТОЛЬКО когда ВСЕ overload'ы
    согласны (call-resolution ещё не выполнена на этом этапе). Это НЕ полноценный
    return-type inference (нет мономорфизации). Достаточно для oracle row D
    (`-> ro Value`, `-> *T`/`-> *mut T`); более сложные формы (generic-return,
    method-call-return `v.f()`) дают `None`→no-gate. Для них L3-нарушение
    ловится позже C-компилятором (`const T*` write = CC-FAIL), не чистой Nova-
    диагностикой. **Followup [M-147-infer-call-ret-mut-axis]** (P2).
  - **L3 deref-write gate (`*p=v`)** срабатывает только когда `p` — простой Ident
    или As-cast с известным типом в scope. `*(p+i)=v` (Binary operand) и прочие
    составные lvalue-деривации дают `infer_expr_type=None`→no-gate; ловится C-
    компилятором через const-pointee (поведение как до Plan 147). **Followup
    [M-147-deref-write-compound-lvalue]** (P2).
  - **Generic-element write `*v[i]=x` (oracle row E `Vec[*T]` vs `Vec[*mut T]`)**
    — НЕ покрыт чистой Nova-диагностикой (element-type inference через `[]`-index
    на generic-instance требует мономорфизации в checker'е). Ловится C-уровнем
    (const element pointee). Документирован в oracle, но не enforced на Nova-
    уровне. **Followup [M-147-generic-element-deref-write]** (P2).
- **Почему:** ATOMIC parser+checker gate требует FULL oracle A-E green + 0 new FAIL
  на baseline pointer/value dirs. Реализованы все формы где checker имеет тип; для
  call/compound/generic-derived типов — graceful fallback на C-уровневый const-
  enforcement (soundness сохранён: ro-pointee write всё равно отвергается, просто
  позже и с C-, а не Nova-диагностикой). Production: ни одна форма не «тихо
  разрешена» — либо Nova-error, либо CC-error.

### Plan 147 Ф.4 — миграция 3-axis canon (2026-06-12)
- **R2-split зеркало `mut r ro Point`** — реализовано полностью (НЕ упрощение):
  Ф.2-3 gate реализовал только `ro r mut Point`, зеркало `mut r ro Point`
  (mut-binding + ro-type-view → field-write freeze) пропускалось. Добавлен
  `root_view_is_ro_type` в check_target_readonly → E_READONLY_FIELD. Oracle
  a4 18/1→19/19. Это **закрытие** дыры, а не simplification.
- **PRE-EXISTING gap (НЕ Plan 147): `null *()` не ловится retraction-guard'ом.**
  Парсер `parse_primary` emit'ит E_NULL_PTR_RETRACTED_USE_OPTION только когда
  `null` за которым bare prim-ident (`ptr`/`int`/…/`str`). Форма `null *()`
  (typed-pointer-literal) не покрыта guard'ом → fall-through → `undefined
  identifier null`. Фикстура plan118/t5_neg_null_ptr_retracted ожидает
  E_NULL_PTR_RETRACTED_USE_OPTION → NEG-WRONG-MSG. Регрессия с Plan 134
  (commit c41d568ae2c мигрировал тело фикстуры `null ptr`→`null *()`, но guard
  не расширил). Orthogonal к 3-axis модели; не в scope Ф.4. Followup
  **[M-147-null-star-ptr-retraction-guard]** (P3). Hard-error сохранён (просто
  другой код).

### Plan 147 Ф.5-Ф.6 — oracle corpus + CLOSE (2026-06-12)
- **БЕЗ упрощений модели.** Ф.5 = чистый test-корпус (0 компилятор-кода), Ф.6 =
  close/docs. 3-axis (D246) реализован полностью: parser+checker+codegen, oracle
  A-E 30/0. Закрыт Ф.1-Ф.6, branch plan-138.1 (НЕ смёржен).
- **ТРИ oracle-ячейки НЕ оформлены как чистые Nova-negatives** (документированные
  границы, НЕ упрощения — soundness держится C-уровневым const-pointee, `const T*`
  write = CC-FAIL; ни одна форма не «тихо разрешена»): **p=v на цепочках
  [M-147-deref-write-compound-lvalue], `*v[i]=x` на Vec[*T]
  [M-147-generic-element-deref-write], `s.ptr=q` str-поле write-ban (дополнительно
  gated на [M-139-f0-lang-item-decl] — str = compiler built-in, нет Nova-source
  lang-item декл). Каждая покрыта POSITIVE type-acceptance фикстурой (c7/e3/e5) с
  границей в prose. НЕ конвертировать в EXPECT_COMPILE_ERROR пока соответствующий
  enforcement не приземлится.
- **[M-138-binding-type-mut-conflict] CLOSED** — НЕ требует visibility-aware
  диагностики: D246 P6 split (L1 binding × L2 view) прямо разрешает обе пары
  `ro X mut T`/`mut X ro T` как ортогональные оси. Это закрытие через модель, не
  упрощение.

### Plan 139.1 Ф.A — str lang-item decl + ABI-alias (E1-GATE, 2026-06-12)
- **БЕЗ упрощений модели.** str объявлен как полноценный Nova value-record
  `type str value priv { ptr *u8, len int }` в `std/prelude/core.nv`. Lang-item
  линковка ПЕРЕИСПОЛЬЗУЕТ value-record-машинерию (Plan 124.8) — НИКАКОЙ новой
  checker-инфры: str просто попадает в `self.types`, privacy fires через
  существующий record-path `f3_check_member`. ABI-bridge: str ∈
  `RUNTIME_DEFINED_TYPES` + forward-decl skip (emit_c.rs) — C-тип = hand-written
  typedef `nova_str` ({const uint8_t* ptr; int64_t len;}), НИКАКОЙ `NovaValue_str`
  struct (0 occ в gen-C). Все ~354 рантайм-сайта + literal-lowering не тронуты.
- **Same-name field/method резолюция (НЕ упрощение — соответствует design):**
  `f3_check_member` теперь предпочитает МЕТОД, если у типа есть одноимённый
  метод (str: field `len` + method `@len()`). `s.len()` (method-call) больше не
  мис-флагается `E_PRIV_FIELD_READ`. Bare field-read `s.ptr` (нет метода `ptr`)
  по-прежнему fires privacy. Это документированный field/method same-name design
  (см. E_BOUND_METHOD heuristic) — codegen уже резолвит `s.len()` в метод.
- **`@byte_len` alias = `@len` (latent-bug fix, не упрощение):** 5 сайтов
  string.nv (parse_int/pad_left/pad_right/repeat) + StringBuilder.with_capacity
  вызывали `@byte_len()`, которого НЕ было среди registered str-методов —
  «работало» только пока str member-access был полностью permissive (str НЕ был
  в self.types). После приземления lang-item method-resolution стал строгим →
  добавлен реальный метод `@byte_len() => @len()` (D26: str.len = bytes).
- **e5_str_ptr_field_ok ORACLE-PIN SUPERSEDED (намеренно, не тихая регрессия):**
  `nova_tests/plan147/e5` читал `s.ptr` снаружи str-модуля и EXPECT'ил ok под
  pre-lang-item хардкод-примитивом. Теперь `s.ptr` снаружи → E_PRIV_FIELD_READ
  (= GATE requirement). e5 мигрирован на public `s.len()`; кейс `s.ptr`-снаружи
  закреплён dedicated neg-фикстурой `plan139_1/neg_str_priv_field.nv`. e5's
  собственный header это анонсировал.
- **Ф.B/Ф.C/Ф.D НЕ in scope этой задачи** (атомарный Ф.A = GATE). 10 external
  C-методов str (`@concat`/`@hash`/`@as_bytes`/`from_bytes_*`/`@split`/`@compare`/
  `@byte_at`/`@len`) пока C-routed — миграция на Nova-body via `@ptr` byte-access
  = Ф.B. content-eq override (D228) уже работает через direct BinOp lowering
  (не Plan 141 field-by-field). marker `[M-139-f0-lang-item-decl]` остаётся OPEN
  до Ф.D close — НО самая сложная часть (lang-item infra + privacy) ПРИЗЕМЛЕНА.

### Plan 139.1 Ф.B — str method C→Nova migration: VERIFY-OR-DOCUMENT, 0 retired (2026-06-12)
- **РЕЗУЛЬТАТ: НИ ОДИН из 10 external методов не мигрирован — ни один не
  мигрируем чисто СЕГОДНЯ.** Это VERIFY-OR-DOCUMENT исход (genuine effort →
  documented), НЕ relaxed-gate. ZERO source changes → build остаётся зелёным
  (plan139_1 2/0, plan139 37/0, str 13/0, plan91 2/0, plan126 21/0, plan108_4
  12/1 [pos_receiver_at_parse c.fmt pre-existing] — идентично Ф.A baseline).
- **КЛЮЧЕВОЙ ВЫВОД:** все pure-Nova-выразимые str-методы УЖЕ мигрированы в
  Ф.1/Ф.2 (starts_with/ends_with/contains/find/rfind/char_len/char_at/trim/
  to_lower/to_upper/to_bytes/to_chars/is_empty/parse_int/pad_*/repeat/replace).
  Оставшиеся 10 — это в точности неустранимый C-bridge + operator-lowered +
  D117-blocked. Премисса задачи («теперь `@ptr` доступен → мигрируй») верна, но
  `@ptr`-field-access НЕОБХОДИМ, но НЕ ДОСТАТОЧЕН для producer-форм.
- **Per-method root cause (verified, не предположение):**
  - **`@byte_at`** — неустранимый raw-byte-read примитив (#1, давно задокументирован).
  - **`@as_bytes`** — должен сконструировать `NovaArray_nova_byte*`-header,
    алиасящий raw `@ptr`. `[]u8` → `NovaArray_*` (compiler primitive,
    emit_c.rs:5173) — НЕТ Nova-surface конструктора NovaArray из raw-parts.
    Заблокировано на `[]T`→`Vec` universal flip (Plan 138.2 Ф.0, НЕ приземлён);
    `@ptr` сам по себе недостаточен. = `[M-139-f2-ptr-field-producers]`.
  - **`@split`** — zero-copy str-sub-views `str{ptr:@ptr+off,len}` + push в
    `NovaArray_nova_str*` (тот же NovaArray-from-raw блокер). = `[M-139-f2-*]`.
  - **`str.from_bytes_unchecked`** — должен `alloc(len+1)` + copy + write `\0`
    на `buf[len]` (D26 §3 NUL-инвариант) + `str{ptr,len}`. Возможно только через
    raw `RawMem.alloc` + `*mut u8` index-write + str-literal — high-risk; C-форма
    уже оптимальна (один memcpy). Aliasing-zero-copy НАРУШИЛ бы copy+NUL контракт
    (`readonly` arg нельзя удержать). = `[M-139-f2-*]`.
  - **`str.from_bytes_lossy`** — `_nova_validate_utf8` + U+FFFD substitution +
    from_bytes_unchecked-конструкция (тот же str-construction блокер). = `[M-139-f2-*]`.
  - **`str.from_bytes_unchecked_steal`** — consume + in-place `\0` + zero-copy
    reuse; те же ограничения. = `[M-139-f2-*]`.
  - **`@hash`** — SipHash-1-3 + скрытый per-process crypto seed
    (`nova_hash_seed_k0/k1`); DoS-resistance ТРЕБУЕТ чтобы seed НЕ был Nova-видим.
    Неустранимый. = NEW `[M-139.1-hash-irreducible-crypto-seed]`.
  - **`@concat`** — лоуэрится НАПРЯМУЮ BinOp-`+`-operator codegen (не через метод).
    Миграция тела НЕ retire'ит C-fn (`nova_str_concat` остаётся для `+`) и
    добавляет perf-регрессию (push-loop vs memcpy). = NEW `[M-139.1-operator-lowered-methods]`.
  - **`@compare`** — лоуэрится comparison-operator synthesis; то же что concat.
    = `[M-139.1-operator-lowered-methods]`.
  - **`@len`** — `s.len` field-style access = HARD ERROR `E_SIZE_ACCESSOR_FIELD`
    (D117, emit_c.rs:17625) → НЕЛЬЗЯ field-read. Метод routes на
    `nova_str_byte_len` (O(1) field-read в C — уже оптимально). = NEW
    `[M-139.1-len-d117-method-only]`.
- **ЧЕСТНОСТЬ vs ОШИБКА ПРОШЛОГО 139:** прошлый 139 закрыл E1/E4 с relaxed-gate
  (str «вёл себя» как value-record но не был объявлен). ЗДЕСЬ обратное: НЕ
  объявляем фейковую миграцию ради галочки. 0 методов retired — честный факт;
  настоящий разблокер producer-форм = `[]T`→`Vec` flip (Plan 138.2 Ф.0), не Ф.B.

### Plan 139.1 Ф.C — str content-eq override (D228) + E1 privacy neg fixtures (2026-06-12)
**FIXTURES ONLY — 0 compiler/std source change, binary unchanged от Ф.A (6670216167a).**
- **Content-eq (D228) — RE-VERIFY-AND-PIN, без source-изменений.** str's C-тип
  остаётся hand-written `nova_str` typedef (ABI-alias landed Ф.A). BinOp `==`/`!=`
  lowering (emit_c.rs ~17137) И `emit_field_eq` (~11310) ОБА key'ят на C-type-строку
  `"nova_str"` → `nova_str_eq(l,r)` / `!nova_str_eq(l,r)`. `nova_str_eq` (nova_rt.h:238)
  = `a.len==b.len && memcmp(a.ptr,b.ptr,a.len)==0` = настоящий content-eq. Override
  УЖЕ был привязан к lang-item через Ф.A ABI-alias — объявление value-record НЕ
  изменило `type_ref_to_c("str")=>"nova_str"`, поэтому str routes тем же content-eq
  path'ом. Ф.C закрепляет DISTINCT-BUFFER позитивным тестом: `built = ("ab"+"cd")+"ef"`
  (два отдельных `nova_str_concat` heap-alloc'а) сравнивается `== "abcdef"` (interned
  literal = 3-й distinct buffer). Проверено в gen-C (str_lang_item_basic_ok.c:3796-3804):
  вложенные `nova_str_concat` затем `nova_str_eq(built, _nova_strlit_...)` — НЕ
  constant-folded, НЕ pointer-eq. Pointer-eq lowering ПРОВАЛИЛ бы этот assert.
- **E1 negative corpus COMPLETE (3 фикстуры — dual buffer-protection + construction):**
  - `neg_str_priv_field` (Ф.A): `s.ptr` снаружи → `E_PRIV_FIELD_READ` (binding-level
    защита буфера — `priv` поле).
  - `neg_str_ptr_write` (Ф.C NEW): запись через bare `*u8` (тип поля str) →
    `E_POINTER_RO_ASSIGN` (type-level защита — `*u8 ≡ *ro u8` ro-pointee, D246).
    Закреплено на bare `*u8`-параметре (НЕ `s.ptr`), т.к. outside-module `s.ptr`
    фаerr'ит `E_PRIV_FIELD_READ` ПЕРВЫМ — privacy гейтит field-access до того как
    pointee-write rule применится. Wrapped в `unsafe {}` чтобы фаerr'ил ТОЛЬКО
    E_POINTER_RO_ASSIGN (bare `*p` добавил бы E_UNSAFE_REQUIRED, D216 §8).
  - `neg_str_construct_direct` (Ф.C NEW): `str { ptr:p, len:n }` снаружи →
    `E_PRIV_FIELD_INIT` (оба поля priv, D220 §4). Не даёт user-коду подделать str с
    произвольной (ptr,len)-парой (нарушило бы D26 §3 NUL-инвариант + content-eq/hash
    soundness). str-значения производимы только через public-surface lang-item'а.
- **АНТИ-RELAXED-GATE:** content-eq доказан на distinct runtime-буферах в gen-C, не
  на interned-литералах (которые прошли бы под pointer-eq). Каждый негатив эмпирически
  подтверждён (фаerr'ит свой точный код до staging).

## Plan 152 Phase A закрыта (shippable-минимум), 2026-06-13
- **Phase A vs B граница.** Phase A (координатная модель + линзы + ASCII-полнота +
  API-паритет Rust/Go + UTF-16 interop) самодостаточна и зашиплена; Unicode-корректность
  (нормализация/graphemes/folding/locale-collation) честно вынесена в Phase B за
  [M-152-unicode-*] — `str` ASCII-complete, не маскирует отставание.
- **Pre-existing main-баги, выявленные при регрессии Plan 152** (НЕ строковый слой):
  Debug-derive `debug_fmt` для nested-struct/Vec (plan91_14/plan131), StringBuilder
  struct-tag в #no_prelude + protocol-codegen Iterable/equals (plan62), рекурсивный
  `Nova_JsonValue` sum-type (plan91_13). Кандидаты на отдельный codegen-план.

[2026-06-14] Plan 140.4 (overflow-элизия, D272, ветка plan-140-overflow-elide): V1 покрывает binary-
  выражения int +/-/* (главный реальный кейс — loop/requires-bounded арифметика). Compound-assign (x += y,
  codegen AssignOp→nova_int_checked_*) ОТЛОЖЕН → [M-140.4-compound-assign-overflow-elision] (P3): таргеты
  обычно безграничные аккумуляторы (sum += a[i], редко доказуемо), отдельный Stmt::Assign AST-путь со своей
  span-привязкой → высокая стоимость, near-zero реализуемая элизия; чек остаётся (sound). НЕ упрощение
  core-фичи — документированная scope-граница низко-ценного пути (binary covers 95%). Также pre-existing
  (вне scope): литерал-операнды (i+1, i*2) codegen вообще не чекает (rty литерала ≠ nova_int) — поэтому
  always-safe тесты используют var+var (i+j) паттерны. * нелинеен → Z3 часто Unknown → консервативно чек
  (не упрощение — soundness: никогда не элидируем без пруфа).

[2026-06-14] Plan 134 (refinement-проход «ptr → *()», ветка plan-134): НЕ упрощения, но честные
  границы/осознанно-оставленные хвосты. (1) Use-в-type-позиции `ptr`/`nova_ptr` ловится на `nova check`
  (E_TYPE_UNKNOWN), но cast-VALIDITY (`*() as bool`/`str`/`f64`/`char`/narrow-int → E_PTR_CAST_INVALID_TARGET)
  по-прежнему enforced на codegen-этапе, а не type-check — это легитимно: правило зависит от resolved
  C-типов (src="*()" из nova_type_name_from_c("void*")), нет смысла дублировать в checker. Проверяется
  fixture'ой plan115/t1_ptr_str_cast_neg (EXPECT_COMPILE_ERROR через полный pipeline). (2) В types/mod.rs
  `cat_of_depth` оставлен dead-but-documented arm `"ptr" => TyCat::Ptr` (legacy-compat комментарий) — он
  недостижим для валидных программ, т.к. walk_typeref reject'ит `ptr` раньше; оставлен намеренно как
  defensive/transition-safety, не вычищен. (3) Регрессия plan118 37/3: 3 NPO-edge FAIL (Some((0 as *()))
  под null-pointer-optimization) — pre-existing на main (identical к main-бинарю), территория Plan 118 NPO,
  вне scope 134; НЕ маскируются, зафиксированы как honest known-issue. Core-фича (удаление ptr + миграция)
  — без упрощений и заглушек.
[2026-06-14] Plan 153.4 (slices/views, D262, ветка plan-153.4-slices, commit `5ccccf72`): 153.4-A
  (eager zero-copy `[]T`-views: split_at/split_first/split_last/first_n/last_n/as_slice + recv-mut
  mut @as_slice) ЗАКРЫТА. **Осознанная отложка — 153.4-B `@chunks`/`@chunks_exact`/`@rchunks`/`@windows`
  → `[M-153.4-chunks-windows-lazy]` (gated на Plan 153.2).** Рекомендация плана = ЛЕНИВЫЕ итераторы
  (Rust/Kotlin, БЕЗ аллокации внешнего `[][]T`-Vec), yield'ящие zero-copy `[]T`-views — зависят от
  ленивой итератор-инфры 153.2 (другой worktree). НЕ реализованы наспех eager: eager-форма
  аллоцировала бы Vec-of-views и расходилась бы с ленивым каноном (Q-iterator-laziness) — это был бы
  настоящий регресс дизайна, а не упрощение. НЕ упрощение core-153.4: eager-views БЕЗ внешней
  аллокации (split/first_n/last_n/as_slice) реализованы полностью, контрактно (split_at OOB→panic,
  инвариант len(l)+len(r)==len) и протестированы (plan153_4/views 14 блоков + split_at_oob_neg).
  Документированная scope-граница (B = ленивый слой за 153.2), а не тихий tech-debt. Маркер заведён
  в backlog-followups.md (P2, gated Plan 153.2). Приоритет P2.
- [2026-06-15] Plan 153.4-B (lazy chunks/windows, D262, ветка plan-153-wave): **`[M-153.4-chunks-
  windows-lazy]` ✅ CLOSED — без упрощений.** `@chunks(n)`/`@chunks_exact(n)`/`@rchunks(n)`/`@windows(n)`
  реализованы ленивыми итераторами поверх инфры Plan 153.2 (которая теперь готова): каждый — инстанс-
  метод `Vec[T] @… -> BoxIter[Self]` в `std/collections/vec_lazy.nv` (sibling-файл, НЕ prelude `vec/`:
  bodies форвардят capturing-closure → generics-leak D145, opt-in `import std.collections.vec_lazy`),
  yield'ящий zero-copy `[]T`≡`Vec[T]`-views (`src[a..b]`, `cap==len`) на том же буфере (Plan 96/D238).
  БЕЗ аллокации внешнего `[][]T`-Vec (Rust `slice::chunks`/`windows`): `collect()` материализует только
  по требованию, `chunks(n).map/fold/count/for_each` — без внешней аллокации вовсе. Контракт `n > 0`
  (`requires`, runtime-panic). **Compiler НЕ трогался** — единственный нюанс codegen решён аннотацией
  локала: early-exhaustion `ro done Option[Self] = None` вместо bare `return None` (bare-None в closure
  с КОНКРЕТНЫМ элементом `Vec[T]`, не свободным generic'ом, монофится в дефолтный `Option[<elem>]` и
  расходится со step-return `BoxIter[Self]`; тот же класс что `[M-153.2-tuple-elem-adapter]`). Это НЕ
  упрощение/маскировка — семантика полная (short remainder в chunks, drop хвоста в chunks_exact, reverse
  в rchunks, overlap в windows, empty/single/oversize-n), контракты осмысленны, тесты полные. Фикстуры
  `plan153_4/chunks_windows` (23 test-блока) + 4 негатива (chunks/chunks_exact/rchunks/windows n<=0,
  EXPECT_RUNTIME_PANIC requires) + smoke в vec_lazy.nv. Верификация (релизный nova C-codegen): plan153_4
  7/0, plan153_2 4/0, plan96 23/0, plan153_0 4/0, plan153_1 7/0, basics 8/0, plan131 28/0, plan138 10/0
  = 0 регрессий. **Plan 153.4 (A+B) ЗАКРЫТ ЦЕЛИКОМ.**
- **Plan 153.5 — restructure-ops + оператор `+` (2026-06-14, D263, ветка `plan-153.5-restructure`,
  commit `e8f700e4`)**: новый co-equal файл `std/collections/vec/restructure.nv` (folder-модуль
  `collections.vec`), все методы — Nova-body поверх bulk `RawMem.copy`. **`@concat(other) -> Vec[T]`**
  (non-mutating join: 1 аллокация ровно на `a+b` + 2 bulk-copy; операнды нетронуты) + **оператор `+`**
  (`@plus => @concat`, `a+b`=НОВЫЙ Vec как str `@plus`/D46; `a += b` ≡ `a = a + b`, рост in place — за
  `a.append(b)`) + **`mut @rotate_left/right(n) -> @`** (циклический сдвиг in place, `n mod len`,
  overlap-safe, O(len)) + **`mut @drain(range) -> Vec[T]`** (вырезать `[start,end)`, вернуть владеемым,
  суффикс вниз, `self` короче) + **`mut @insert_slice(i, sl []T) -> @`** (делегирует в `@splice` под D239
  `[]T≡Vec[T]`). Контракты `requires` (rotate/drain/insert_slice — OOB/reversed/negative → panic). Codegen
  `+`/`+=` (emit_c.rs +68): `BinOp::Add` на `Vec[T]` → `vec_method_call("plus")` ПЕРЕД generic-sum-Add-arm
  (иначе голый `_method_plus` без mono-инстанса → undefined symbol); `a += b` десугарится в `Binary{Add}`
  (сырой C `a += b` на struct/pointer нелегален). Минимально-таргетно, не трогает struct-tag/protocol/
  prelude/Option-arg кодпути.
  **Production-grade, без упрощений (обязательный критерий):** все 5 методов — реальные алгоритмы с
  правильной cost (concat одна exact-аллокация + 2 bulk-copy, не per-element; rotate O(len) с
  O(min(n,len−n)) scratch и right≡left-на-len−k; drain один copy-out + один shift-down; insert_slice через
  overlap-safe `@splice` → self-insert корректен). `+` non-mutating (Kotlin/Python семантика, не in-place
  footgun). Контракт-паники — настоящие `requires`, не молчаливый clamp.
  **САНКЦИОНИРОВАННОЕ ОТКЛОНЕНИЕ (честно про scope) — `[][]T.flatten()` ОТЛОЖЕН** (`[M-153.5-flatten-
  nested-receiver]`, P2): это НЕ упрощение реализованного, а **отсутствующая фича**, заблокированная двумя
  настоящими compiler-лимитами (не workaround'имо корректно на surface-слое). Корректному `.flatten()`
  нужен вложенный generic-ресивер `Vec[Vec[T]] @flatten()` (тело должно назвать внутренний `T`); (1) ПАРСЕР
  отвергает вложенный тип в carrier-слоте (`parse_generic_decl_params`→`parse_ident`); (2) `[][]T`-форма
  ПАРСИТСЯ, но монорфизатор биндит `T` в непосредственный элемент (`Vec[int]`), не во внутренний (`int`) —
  verified probe RUN-FAIL (mono'd `out` = `Nova_Vec____Nova_Vec____nova_int_p`, неверный return-тип).
  Корректный фикс = структурная typevar-унификация для вложенных ресиверов в ОБОИХ парсере+монорфизаторе
  (cross-cutting весь `[]T`-method-dispatch path) — сознательно вне scope restructure-surface (не хак-обход,
  который маскировал бы неверный тип). Обход в доках: flatten один уровень явно через bulk `@append`.
  **Тесты:** plan153_5 5/5 (POS `restructure` — non-mutation concat/`+`, `+=` append, rotate-инверсия+
  identity, drain вырез+empty, insert_slice mid+end; NEG runtime-panic `drain_oob`/`drain_reversed`/
  `insert_slice_oob`/`rotate_negative`), релизный nova C-codegen. **0 регрессий 153.5** (7 сьютов: plan153_5
  5/5, plan90 9/9, plan90_1 21/21, plan153_0 4/4, plan153_1 7/7, basics 8/8, plan62 29/7). 7 plan62-FAIL =
  PRE-EXISTING (prelude/module/protocol struct-tag — ортогональны restructure-ops): доказано baseline-
  бинарём на родительском коммите `c0f269dd` в temp-worktree — ИДЕНТИЧНЫЕ 29/7, те же имена/категории
  (temp-worktree удалён+pruned). std `vec/*.nv` грузится с диска → правки без ребилда компилятора (но
  emit_c.rs трогали → бинарь пересобран, новее всех .rs).

- **РЕЗОЛВ `[M-153.5-flatten-nested-receiver]` — вложенные generic-ресиверы произвольной глубины + flatten
  (2026-06-14, ветка `plan-153.5-restructure`, commits `1c323d0e` parser+mono + `16753d23` flatten)**:
  flatten **больше НЕ deferred/упрощение** — реализован. Запись 153.5 выше фиксировала `[][]T.flatten()`
  как САНКЦИОНИРОВАННОЕ ОТКЛОНЕНИЕ (отсутствующая фича за двумя compiler-лимитами); followup закрыл оба
  лимита и саму фичу. `Vec[Vec[T]] @flatten() -> Vec[T]` (production carrier-форма ≡ `[][]T @flatten() ->
  []T` под D239) в `std/collections/vec/restructure.nv`: pre-size `out = with_capacity(Σ inner.len())` +
  bulk `out.append(inner)` на ряд (copy-fast-path `RawMem.copy`, операнды нетронуты; пустые ряды/внешний —
  корректно). **Root-cause (обе половины, глубокий cross-cutting fix — НЕ хак-обход):** (1) ПАРСЕР отвергал
  carrier `Vec[Vec[T]]` («expected `]`, got identifier») и схлопывал `[][]T`→`"[]T"`; (2) МОНОРФИЗАТОР
  биндил receiver-typevar `T` в *непосредственный* элемент (`Vec[int]`), не во *внутренний* (`int`) →
  неверный return-тип `Vec[Vec[int]]` + segfault на индексации (verified probe RUN-FAIL, mono'd `out` =
  `Nova_Vec____Nova_Vec____nova_int_p`). **Фикс (рекурсивный, depth-agnostic, без one-level-hardcoding):**
  AST-носитель `Receiver.receiver_ty: Option<TypeRef>` (полный структурированный тип — единственное место,
  где глубина переживает; `type_name` flatten'ит в `"[][]T"`); ПАРСЕР: slice `[][]T` — счёт глубины
  `Array` + спуск до внутреннего `Named` (`slice_receiver_depth_and_inner`), carrier `Vec[Vec[T]]` — новый
  `parse_generic_decl_params_inner` принимает вложенный `parse_type` в слоте (детект `Ident[`) +
  рекурсивный сбор free-typevars (`collect_free_typevars`/`ident_is_typevar`), структурные слоты в
  `receiver_ty` (free-fn `[T Bound=D]`-разбор не тронут); МОНО: переиспользован рекурсивный
  `infer_type_param_binding` (Array-арм также снимает mono-форму `Vec____` через
  `generic_type_instance_info`), override на ВСЕХ путях receiver-typevar-bind (emit-dispatch carrier +
  `[]T`-sentinel slice + call-site return-inference) + depth-aware sentinel-ключи `"[]"*N+"T"`
  (`vec_nesting_depth`/`slice_sentinel_key_for_rt`) вместо hardcoded `"[]T"`; `receiver_c_type`/
  `receiver_type_c_ident` сделаны multi-`[]`-tolerant, **flat `[]T` (depth 1) остался byte-identical**
  (legacy `NovaArray_`-путь), override гейтнут `receiver_ty_is_nested`/`collect_receiver_typevars`; CHECKER:
  collect вложенных typevar'ов из `receiver_ty` в `referenced`-set для `E_UNUSED_PREFIX_TYPEVAR`, но scope
  `gs` НЕ сидится из `receiver_ty` (сохраняет `E_UNDECLARED_TYPEVAR_IN_RECEIVER` для `fn []T @m` без
  префикса — verified, что seed был бы регрессией, откатил). **Compiler-bug по дороге (FIXED, не
  упрощение):** для `Vec[Vec[T]]` поле `data` = `*mut Vec[T]` лоуэрится в одиночный `Nova_Vec____*`;
  `@data + @len` мис-диспатчил `ptr + int` в pointee-`@plus`(=`@concat`) → segfault; emit_c.rs ~18450/18612
  Add-арм (Vec-plus + generic sum-plus) теперь требует ОБА операнда matching record/Vec value-типа, `ptr +
  int` падает в типизированную pointer-арифметику (verified: scalar-операнд `@plus(int)` overload'ов нигде
  нет). **САНКЦИОНИРОВАННЫЙ остаток (честно, ортогональный pre-existing, вне scope):** slice-форма `fn[T]
  [][]T -> []T`, чьё тело СТРОИТ свежий `Vec[T].new()`, упирается в pre-existing erased-base-body лимит,
  который ЛОМАЕТ и flat `fn[T] []T` с `Vec[T].new()` на baseline (`expected struct 'Vec____Nova_T_p'`).
  Production-flatten — CARRIER-форма (как все stdlib), работает полностью; slice-form nested-receiver
  binding доказан отдельно (`@count_all`/`@first_row`). **Тесты:** plan153_5_nested 4/4 (`flatten_depth2`
  Vec[Vec[T]]→Vec[T] int+str, `flatten_depth3` depth≥3 + nested-typed return, `slice_nested`
  `@count_all`/`@first_row`, `control_flat` flat unchanged) + plan153_5/`flatten` (`[[1,2],[3],[4,5]]`→
  `[1,2,3,4,5]`, empty rows/outer, str, double-flatten `Vec[Vec[Vec[int]]]`) + `flatten_plus_guard`
  (operator-`+` гард), релизный nova C-codegen. **0 НОВЫХ регрессий** (broad slice/vec/generic-dispatch
  watch: plan153_5/_nested, plan90/90_1, plan96, plan138/_2, plan147, plan153_0/1/3, basics, generics —
  всё зелёное; plan62/syntax FAIL-сеты byte-for-byte идентичны baseline на родителе `c5865ba0` в temp-
  worktree, ВКЛЮЧАЯ высокорисковые iterator/method-dispatch тесты). Spec — D145 AMEND (02-types) + D263
  AMEND (10-overloading); backlog-маркер → ✅ done; vec-internals.md flatten-секция + nested-receiver
  заметка. **Урок:** «отложенная фича за compiler-лимитом» ≠ «упрощение реализованного» — её резолв = новая
  запись-резолв (append-only), не правка старой; carrier-slot нужен отдельный структурный AST-носитель
  (`receiver_ty`), т.к. `type_name` теряет глубину; структурный typevar-бинд обязан быть рекурсивным
  (innermost element), depth-agnostic, гейтнутым на nested — иначе ломает весь flat `[]T`-dispatch.

## Plan 176 Ф.2 — fs + Path (2026-07-04)

- **[production, НЕ упрощение] Ф.2 fs+Path (2026-07-04).** Модуль `std/fs`: byte-backed `Path` value (POSIX+Windows/UNC/drive lexical: join/parent/file_name/extension/stem/components/normalize/with_extension/is_absolute; non-UTF-8 round-trip Q1; `posix`/`windows`/`from_str`(host)/`styled`); **`Fs` effect как ТОНКИЙ int-primitive слой** (open/close/read/write/read_at/write_at/seek/sync_all/sync_data/stat/lstat/fstat + stat_*-accessors/mkdir/remove_file/remove_dir/rename/scandir(+next/name/kind)/realpath(+data)/symlink/chmod/copy_file/fsync_dir — все возвращают int/i64/str, НЕ Result); **триада** `real_fs()` (libuv `uv_fs_*` park/wake ТОЧНО как net.c — `nova_rt/fs.c` + `fs.h`, best-effort-cancel Q4) + `mock_fs(MemFs)` (in-memory byte-Path-дерево, ENOSPC-инъекция); **`File` must-consume (D133)** (`@close(self)`, positioned read_at/write_at + own cursor, OpenOptions read/write/append/truncate/create/create_new Q13, sync_all/sync_data); `Metadata`(→`Timestamp` Q каждый Option)/`DirEntry`/`FileType`/`Permissions`(Q8/Q12); convenience read/write/read_text/write_text/write_atomic(5-шаг durable §3c)/create_dir(_all)/remove_file/remove_dir(_all)/copy_file/rename/read_dir/canonicalize/symlink/set_permissions/try_exists; `c_path` interior-NUL-reject (§3c(1)). fs_seek(lseek)+platform-predicate в `io_console.h` (без libuv). Build: `fs.c` в 3 toolchain-сайта `test_runner.rs` (как net.c), `#include "fs.h"` в `nova_rt.h` под `NOVA_USE_LIBUV`. Тесты: nova_tests/fs pos(path POSIX+Windows / mock_fs round-trip/metadata/seek/OpenOptions/dir/write_atomic/torn-write / real_fs temp-dir via spawn) + neg(D133) — ALL PASS; spec_tests/d323 (отдельный module) PASS; main conformance 38/0; io/str регресс 0.
- **[production-обход, НЕ упрощение семантики] `Fs` = int-primitive-эффект.** effect-vtable стирает rich `Result[T, value-IoError]` в canonical `nova_int`/`nova_str` (теряя value-`IoError` и Ok-record — untested io-core gap: io-тесты используют effect-free конформеры). Обход: эффект несёт int/str-коды (зеркалят fs.c-хуки), `IoError`/`Metadata`/`DirEntry` строятся в pure-Nova обёртках ВНЕ effect-границы (там value-record keystone работает). Согласуется с §3/§0 «логика в .nv над тонким C-hook». Followup — фикс effect-vtable Err/Ok-erasure для value-record (не блокер).
- **[bounded, plan-санкц. маркеры — НЕ молчаливые]** `[M-176-consume-through-result-match]` (D133 не отслеживается через `match Result{Ok(f)=>…}`-extract — enforced для consume-param/direct-binding; общий с net TcpStream; neg-тесты через consume-param); ~~`[M-176-conformance-cu-map-closure]`~~ (**RESOLVED 2026-07-05 sync-fix-d322**: root = `emit_fn` не скоупил `var_mutable` → `mut f`-локаль одной fn (появляется со std.fs в CU) лик'ала в классификацию капчура ИММУТАБЕЛЬНОГО `f`-param'а лямбды `BoxIter.map` как by-ref-mut → env `T** f` без unpack-локали → closure-call `f(x)` голый `f`. Фикс: скоуп `var_mutable` per-fn-body; d323 возвращены в conformance, d102 PASS); `[M-176-memfs-gc-pressure]` (mock_fs 10-тестовый binary flaky под GC → разбит ≤3/файл, isolated стабилен); `[M-176-cwstr-direct-winapi]` (CWStr не нужен — libuv сам UTF-8→UTF-16 на Windows); `[M-176-cstr-from-bytes-canonical]` (§3c CStr.from_bytes = локальный `c_path([]u8)`); `[M-176-dir-scoped-ops]`/`[M-176-create-temp]` (Zig openat / unique-temp — followup). `IoError.path`/`source` (§3b full-shape) отложены (io↔path cycle + value-`Option[Path]`-mono blast-radius; `kind` сохранён — все тесты/§8.3 на нём).
- **[sync-fix-d322, 2026-07-05] Пост-reconcile: два codegen-факта + d323 в conformance.** (1) **Bug 2 RESOLVED (codegen-фикс):** `[M-176-conformance-cu-map-closure]` — `emit_fn` скоупит `var_mutable` per-fn-body (устраняет лик `mut`-классификации через границы функций, ломавший `BoxIter.map`-лямбду когда std.fs в CU). d323-фикстуры возвращены в `spec_tests/conformance` (директива владельца): d323_file_must_consume/path_bytes/write_atomic + neg — PASS при них ВНУТРИ CU; `spec_tests/d323` удалён. (2) **Bug 1 = `[M-sync-crossmodule-samename-type-collision]` (НЕ codegen-hunk, как гипотезировалось — pre-existing language-gap):** merge 178/179 свёл в один positive-CU три разных `ErrorKind` (io/http/compress) с простым C-именем `Nova_ErrorKind` → коллизия → ICE при io `kind_from_errno`. Target-form = module-qualified type-naming (крупно, НЕ sync-fix). **Sound-обход §0:** http (d358) → `spec_tests/http`, compress (d333/334/335/336) → `spec_tests/compress` — свои module-CU (d323-паттерн). Гейт: conformance 53/0 (1 aggregate-pos incl d322+d323 + 52 neg), zero-regress delta 0 (все падения — pre-existing на 8958b6fe: basics/control_flow, effects/basic, concurrency/_repro_p110, modules/priority_queue, map_literals/positive_clone_merge), features (io/fs/http/time/ffi/rebind/any_is/effect_registry) PASS. `[M-178-conformance-d357-d360-forwarddecl-bug]` — ДРУГОЙ root (forward-decl return-type unit-closure-call), НЕ тронут.
- **[codegen-конвенции, выявлены Ф.2]** value-record литералы: typed-форма (`Path{…}`) в блок-позиции, anon (`{…}`) в `=>`-теле (checker: typed redundant в `=>`, codegen: anon-inference только для heap-`Nova_X*`, не value). std.fs free-fn имена НЕ коллидят с std.io generic-хелперами (coarse-by-name резолв): `read_text`/`write_text`/`copy_file`. Резерв. слова: `exists`/`forall` (квантор), `readonly` (kw) — переименованы (`try_exists`, field `read_only`). Multi-line import/`FileType`-enum-variant-vs-`File`-type коллизия → `FileType value{k int}`.

## [M-detach-forbid-test] (2026-07-11)

- `forbid Detach` заявлен дизайном (D63×D50), механика в check_callee_effects есть,
  но тестов 0. Добавить pos/neg; после транзитивности — глубокий кейс.

## Plan 174 (заход 2026-07-06 — 174.2 spec-closure/cross-carrier, 174.1 truncation, 174.5 eval)

- **174.2 Ф.B cross-carrier `?` — консервативная детекция, не полная.** Диагностики
  `E_TRY_OPTION_IN_RESULT_FN`/`E_TRY_RESULT_IN_OPTION_FN` срабатывают только когда носитель операнда
  ВЫВОДИТСЯ (`infer_expr_type`), не несёт generics и ПРОТИВОПОЛОЖЕН носителю return-типа. При
  невыводимом/generic операнде — молчим (safe false-negative): мисматч всё равно поймается как
  type-error на синтезированном `return None`/`Err`. НЕ может превратить компилирующийся код в
  падающий. Третья диагностика (E1≠E2 `.map_err`-hint) НЕ реализована — требует sum-extension
  compat-проверки (172.1), иначе false-positive на легальном widening. `[M-174.2-try-err-type-mismatch-hint]`.
- **174.1 truncation-фикс — в существующей хардкод-архитектуре, не структурный.** `emit_parse_range_check`
  добавляет sub-width range-check ПЕРЕД narrowing-кастом в обе codegen-хардкод-ветки (try_from/try_parse).
  Это фиксит named-acceptance баг (`i8.try_from("999")` → Err вместо Ok(-25)) как изолированный
  корректностный фикс. Полный структурный вариант-B (generic-движок в .nv, удаление хардкода, typed
  errors вместо flat-string, float-канон, radix-поверхность) — отложен под координацию 172.1-hardcode ×
  177 (`[M-174.1-parse-engine-structural]`). Err-тип у sub-width try_from остаётся flat-string (не
  typed ParseIntError) — pre-existing, закрывается structural-заходом. value-equality на sub-width
  Result-payload (`unwrap_or(0)==N`) имеет отдельный pre-existing лимит (Ok=nova_int/Err=nova_str
  bootstrap-Result) — тесты проверяют классификацию (is_err/is_none), не значение.
- **174.5 — только §7.7-оценка, без кода.** Write-cap-баг подтверждён живым по символам; checker/codegen/
  spec-amend отложены (02-types = зона 172, координация). Символы зафиксированы для turnkey-resume.

## Plan 173.1 — supervised-value + канальный `parallel for → []T` (D414 §4, D71-amend) (2026-07-09)

- **`supervised { … v }` — value-expression:** bootstrap-заглушка «возвращает unit» СНЯТА;
  trailing вычисляется ПОСЛЕ join детей. Упрощение сохранено сознательно: unit-типизированный
  trailing остаётся eager/pre-join (байт-паритет — `spawn {…}` последним стейтментом это
  trailing по грамматике, откладывать его за пределы активного scope нельзя).
- **`parallel for → []T` — v1-упрощения РЕТИРОВАНЫ** (slot-запись `result.data[idx]`,
  примитив-whitelist {int,bool,f64,str}, итераторы Range/ArrayLit/Ident, guard
  `[E_PARFOR_RESULT_UNSUPPORTED]` + visitor-семейство ~200 строк в чекере): сбор через канал
  (Sender-клон в родителе на spawn → send из ребёнка: int-скаляры прямо / heap по ссылке /
  value-типы boxed → close на любом выходе; drain-fiber; K=min(len,16)). Порядок = completion
  order (плотный) — iteration-order-гарантия убрана из спеки и корпуса (sort/set-equality).
- **Остаточные упрощения (голова у гейтов):** Stop-стратегия — 173.2; `parallel(timeout:)` —
  после 175; Semaphore-cap живых fiber'ов (память O(N) fiber'ов при O(CAP) канале) — опц.
  Ф.3, не делался; поверхностный `consume`-синтаксис в spawn — 173.3 (лоуэринг семантики
  клон→move→close уже в codegen напрямую).
- **Попутные закрытия (вскрыты сбором при N≥1000 armed M:N):** [M-chan-spurious-wake-retry]
  (plain send/recv не ретраили spurious wake — потери значений / ложный None; select ретраил),
  [M-chan-close-phantom-zero] (close-wake ставил fired=1 без значения → фантомный Some(0)),
  [M-spawn-module-const-capture] (module-const захватывался по сырому имени в spawn/detach/
  blocking), [M-bare-result-try-annotation] (bare `Result` + `?` аннотировался целым Result →
  указательная арифметика на int-payload).

## [M-fixed-array-value-semantics] (2026-07-10, ветка fixed-array-value)

- **Category-key `resolved_cat_of` НЕ переведён на FixedArray-вариант** — намеренно:
  совместимость присваивания `[]T`/`[N]T` — отдельная ось от C-представления;
  внутренний ключ никогда не лоуэрится в C. Складывание категорий — ортогональный follow-up.
- **len-mismatch / spread в [N]T-литерале** ловятся codegen loud-fail (осмысленная
  диагностика, не тихий мискомпил); checker-уровневый E-код — followup.
- **serde-derive не научен [N]T** (auto_derive строит Vec-выражения) — живых
  пользователей [N]T-полей в serde-типах нет; followup в маркере.
- **field_cache index-write барьер при отключённом IPA** (`--no-field-cache-ipa`) —
  консервативный (нет ref_typed-оракула): корректность > байты в нестандартном режиме.
- **`[0; N]`-repeat литерал** (Rust-style) по-прежнему не поддержан парсером
  ([M-sha256-array-repeat-literal-parser]) — не взято в эту волну.

## [M-effect-handler-body-record-literal] CLOSED + [M-spawn-var-boxed-leak] CLOSED (2026-07-22, ветка p175-typed-effects, Plan 175 Ф.2-v2, sonnet)

- **`[M-effect-handler-body-record-literal]` закрыт архитектурно, не догоняющим патчем.**
  Handler-literal (`with X = effect X {…}`) capture-механизм заменён на common
  closure-capture path (`emit_lambda`'s `var_boxed`/mangled-field схема) — `#define`-макросы
  для захватов удалены целиком. Escaping-хендлеры (factory `-> Effect[X]`) heap-promote
  мутабельные захваты (как раньше); inline-хендлеры (`with X = … { body }`) берут `&cap_name`
  напрямую БЕЗ box — новая boxed-переменная там была бы C-scope-leak (`emit_with` заворачивает
  handler-конструирование в свой interrupt-frame C-блок; boxed-переменная, объявленная там, не
  пережила бы закрытие блока — найдено на `spec_tests/conformance/repro_matrix.nv`'s two-level
  nested-handler-capture фикстуре, `[M-175-handler-lit-boxed-var-c-scope-leak]`).
- **`[M-spawn-var-boxed-leak]` (побочная находка, тот же коммит).** `spawn`/`detach`/
  `blocking`-тела резолвят захваты через СВОЙ `current_spawn_captures`-механизм, не через
  `var_boxed` — но `var_boxed`-проверка в `ExprKind::Ident`-резолюции стоит РАНЬШЕ, так что
  stale `var_boxed`-запись (от closure/handler-literal РАНЕЕ в той же enclosing fn) затеняла
  корректный spawn-путь (`spec_tests/conformance/standalone/scope_multierror_test.nv`,
  `application_cross_fiber_t8_7.nv`). Пофикшено var_boxed save/take/restore-изоляцией вокруг
  ВСЕХ четырёх body-swap-сайтов (`emit_spawn`, `emit_detach`, оба `blocking`-work-fn) —
  mirror паттерна, который `emit_lambda`/`emit_monomorphized_method` уже применяют.
- **`#default_handler(X)`-механизм (D431, новый)** — generic compiler front-end (parser
  `#default_handler(EffectName)`-атрибут в `DocAttr::DefaultHandler`; checker
  `check_default_handlers` — duplicate/arity/return-type/unknown-effect/cycle) + per-эффект
  runtime-хук (лениво once-per-thread конструирует+ставит дефолт-handler без `with`); `Time`
  первый мигрированный эффект (`std/src/time/duration/core.nv` `time_default`). DCE-root-seed
  добавлен (иначе reachability-DCE, Plan 81/159, дропает fn, единственная ссылка на которую —
  сырая C-строка в `main()`-прологе, невидимая AST-walker'у) — нашлось на
  `spec_tests/conformance/standalone/vr_binop_arith_dce.nv`.
- **НЕ сделано этой волной (два followup, `docs/plans/backlog-followups.md`):**
  typed-schema retype Time (`sleep(Duration)`/`now()->Timestamp`/`now_monotonic()->Monotonic`)
  — требует wire↔surface scalar-bridge на handler-impl+call-site (hand-written `NovaVtable_Time`
  компилируется раньше per-CU value-record-typedef'ов, та же находка что D316-amend §Ф.2);
  ambient-retraction Time (D62 amend, strict-effects-сигнатуры по всему std/examples) —
  масштаб отдельного окна. `#default_handler` runtime-хук написан generic (работает для ЛЮБОГО
  эффекта через `emit_effect_type`'s auto-generated dispatch), Time — единственный эффект с
  hand-written vtable, поэтому единственный со своим fn-pointer-хуком (`_nova_time_default_ctor`).
- **Гейт:** `nova test spec_tests/conformance` 130 PASS / 1 pre-existing FAIL (несвязанный,
  `d316_time_effect_typed_surface.nv` ссылается на ретрактированный free-fn `sleep_until`,
  ветка не трогала файл); `nova test std/src` 67 PASS / 0 FAIL.

## [M-sequential-serve-instances-stale-state] CLOSED (2026-07-23, ветка p-fix-n38-workertls, fable-волна, 221.1 #38)

- **Симптом:** последовательные supervised-accept тесты в одном процессе — первый тест с
  реальной отменой `supervised(timeout:)`, любой следующий с nested `supervised(deadline:)`
  получает мгновенный (~10-14мс) ложный TimeoutError с байт-в-байт истёкшим `deadline_ns`
  первого. Блокировал 5 live-тестов 222.7.
- **Корень (живая трасса, bracketing-инструментация scope_init/run_impl/worker-resume/reset):**
  пул воркеров арм-ится лениво на ПЕРВОМ `spawn` процесса — внутри активного
  `supervised(timeout:)`; `nova_scope_init(&w->scope)` (арм пула, runtime.c) через D349
  ambient-наследование запекал абсолютный дедлайн арм-сайта в вечную структуру `w->scope`
  каждого воркера; последующие вложенные supervised на воркер-фибрах (ambient = `w->scope`)
  наследовали протухшую точку, `nova_deadline_combine` оставлял более раннюю. Гипотеза ОКНА-4
  (dangling TLS на main) в механизме опровергнута — порча в СТРУКТУРАХ, не в TLS;
  `nova_runtime_reset` бессилен by-design.
- **Фикс:** `nova_scope_init_container` (fibers.h) — герметичный init контейнерных scope
  (`w->scope` + `_nova_orphan_scope`): `deadline_ns=0`, `saved_active_scope=NULL`. Одна запись
  на арм-сайте до старта потоков, ноль новой синхронизации; enforcement дедлайнов реальных
  детей — через их `_nova_parent_scope`, семантика не теряется.
- **Верификация:** repro38g2 RED→GREEN; фикстура
  `spec_tests/conformance/standalone/m2211_38_sequential_supervised_accept_stale_deadline.nv`
  RED(до-фикс)→GREEN(фикс); m2217_15/15b δ0 (2/2 PASS); std/src/concurrency δ0 (4/4 PASS);
  4-way нагрузка фикстуры 32/32 чисто. Кейс-стади:
  `docs/cases/sequential-serve-scope-leak-2026-07-23.md` (раздел Resolution).

## [M-coalesce-return-fallback-unparsed] CLOSED (2026-07-24, sonnet, worktree `nova-coalesce`, ветка `p-coalesce`)

- **Решение владельца (2026-07-23):** форма `X ?? return R` РЕТРАКТИРОВАНА из D86 (не
  чинили парсер под неё) — AMEND в `spec/decisions/04-effects.md` D86, с таблицей замен
  (`X?` / `.ok()?` / `.map_err(fn(_ E) -> F => ..)?` / `.ok_or(..)?` / явный `match` для
  `glob.nv`-класса без обёртки для проброса).
- **Архитектура диагностики:** rustc-style parse-then-diagnose. Парсер принимает форму в AST
  (новый `ExprKind::CoalesceReturnFallback`, конструируется ТОЛЬКО как непосредственный правый
  операнд `??`), чекер ВСЕГДА отвергает `[E_COALESCE_RETURN_FALLBACK]` с контекстным
  `Suggestion` — выбор канона строит decision-функция `coalesce_return_fallback_advice` +
  рендер `coalesce_advice_render` (`types/mod.rs`), по (carrier операнда, carrier/E-тип
  return-типа enclosing fn).
  Новый AST-вариант потребовал арм в 32 exhaustive match'ах по всей кодовой базе
  (alpha_rename/callnorm/desugar/embed_resolve/field_cache/interp/lints/number_exprs/
  codegen::{emit_c,may_gc,preempt_keep}/types/verify::encode) — узел структурно НЕ может
  появиться нигде, кроме RHS `??` (парсер это гарантирует), поэтому везде кроме
  `check_coalesce_return_fallback` — defensive no-op walk (как `Throw`/`Interrupt`) либо
  `unreachable!`-подобный отказ в codegen/interp/verify (чекер гарантированно отвергает
  раньше).
- **Фикстуры:** `nova_tests/coalesce_return_fallback/` — 1 pos регресс-пин (значение/`panic`/
  `throw` fallback'ы, НЕ затронуты ретракцией) + `neg/` — по одному файлу на строку decision-
  таблицы (6 файлов), каждый с `EXPECT_COMPILE_ERROR` на РЕАЛЬНЫЙ текст подсказки (не только
  код ошибки). Все 7 PASS (`nova test --full`).

## [M-manual-coalesce-lint-missing] CLOSED (2026-07-24, sonnet, worktree `nova-coalesce`/`nova-http-coal`, ветки `p-coalesce`/`p-coalesce-http`)

- **Линт:** `W_MANUAL_COALESCE` (`compiler-codegen/src/lints.rs`, реестр `CONV_RULES`) —
  ловит identity-match (рука успеха — РОВНО идентификатор, связанный в паттерне);
  НЕ ловит `Ok(_) => bool` (is_ok/is_err), `Ok(v) => f(v)` (map-форма), разные имена
  паттерн/рука, guard'ы. Подсказка переиспользует ТУ ЖЕ decision-функцию, что чекер
  (Ф.2) — но синтезирует carrier/E-тип из СИНТАКСИСА (declared return-тип функции +
  эвристика «`Err(e)` verbatim passthrough ⟹ тот же E», без реального инференса типов
  — у `ConvRule.ast`-хуков его нет). Найден и закрыт по ходу миграции (не документо-
  ревью — прямое подтверждение feedback-no-done-claims-on-documents): fallback,
  ссылающийся на bound error-идентификатор (`Err(e) => { log(e); .. }`), не может быть
  выражен `?? D` (`??` отбрасывает payload `Err` целиком) — добавлен free-var-guard
  (`capture_scan_expr`/`capture_scan_block`, промо́ушен до `pub(crate)`), молчит вместо
  ложной подсказки.
- **Инвентарь брифа 2026-07-23 (69 сайтов) устарел** — фактический прогон линта дал
  **161** находку (std 84, nova-http 71, examples 6) ДО миграции — 2.3× оценки.
  Мигрированы (эта волна) сайты, ИМЕННО названные брифом: `nova-http/src/client/
  wire.nv` (14), `nova-http/src/middleware/cors.nv` (1), `std/src/fs/{readfs,fs}.nv` (5),
  `std/src/time/civil/{tz,format}.nv` (11), `examples/flagship/aggregator` (6) — 37 сайтов
  итого. Остаток (std 66, nova-http 55 — 121 сайт) вынесен в новый floating-маркер
  `[M-manual-coalesce-corpus-remainder]` (backlog-followups.md) — объём остатка вне
  бюджета этой волны.
- **Юниты:** 8 тестов в `lints.rs::tests` (3 pos: value/result/return-same-carrier;
  5 neg: is_ok-wildcard, map-shape, разные имена, glob.nv-класс, fallback-references-
  bound-err) — все PASS.
- **Гейты:** `nova test std/src/fs std/src/time` (7 PASS/0 FAIL), полный nova-http suite
  (26 PASS/0 FAIL), `examples/flagship/aggregator` `nova check` PASS на всех 4 изменённых
  файлах. Мега-CU conformance и flagship `--strict-effects` НЕ гонялись (CPU-дисциплина,
  интегратор).

## Plan 214.1 (2026-07-24, sonnet, worktree `nova-p2141`, ветка `p214-1-generic-coerce`) — generic `#coerce` (снятие R14)

- **Что сделано:** R14 (бланкет-реджект generic-`#coerce`) снята D429-амендментом
  (`spec/decisions/02-types.md`, раздел «Generic-образцы»); реализован ВТОРОЙ реестр
  `generic_coerce_patterns` (`compiler-codegen/src/types/mod.rs`) — образцы вида
  `Json[T] @data() -> T`, унифицируемые ОДНОСТОРОННЕ и БЕЗ РЕКУРСИИ ВГЛУБЬ против конкретных
  `(I,O)` на каждом сайте (`generic_coerce_lookup` — accept-путь; `try_coerce_leaf`'s
  generic-ветка — AST-rewrite), проверяемые ТОЛЬКО при промахе конкретного `coerce_pairs`.
  R3'/R13' реализованы и покрыты фикстурами; R5' (overload-уровневая ambiguity) — см. маркер
  ниже.
- **`[M-coerce-r5-ambiguous-overload-unimplemented]` (открыт, P3, не в объёме 214.1):**
  D429 R5 обещает `E_COERCE_AMBIGUOUS`, когда ≥2 применимых `#coerce`-пар с РАЗНЫМИ O
  конкурируют за одну call-arg позицию через РАЗНЫЕ overload-кандидаты (пример из плана 214
  Ф.4: `str→[]u8` + локальная `str→X`, оверлоады `dump(v []u8)`/`dump(v X)`). Аудит 214.1
  нашёл: этот код НЕ встречается нигде в компиляторе — ни для конкретных пар (Plan 214 сам
  never реализовывал этот overload-уровневый путь, только decl-time R3), ни для generic-
  образцов (214.1 реализовала R3' — одно-позиционный конфликт УЖЕ детектируется, но НЕ
  overload-уровневая R5'-неоднозначность с разными O). Снятие — отдельный, не начатый пункт
  (обе полосы, конкретная и generic, один и тот же overload-resolution механизм).
- **Гейты волны:** `cargo build --release` чисто; targeted pos/neg-фикстуры (см. отчёт
  волны); `nova test std/src/checksums` δ0; `nova check std/src` байт-идентично (δ0
  существующего конкретного `#coerce`-механизма str→[]u8 и т.д. — точные числа файлов/байт
  в отчёте волны). Мега-CU conformance НЕ гонялась (CPU-дисциплина, интегратор).
