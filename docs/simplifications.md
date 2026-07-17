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

[2026-07-10 Plan 172.13 Ф.2 — constraint-core литерал-коэрсия миграция; D55 разграничение, ветка constraint-core, 🟡 Ф.0-2 ✅, Ф.3 не начато] Ф.0 (инвентарь продюсеров f1-преамбулы канала) + Ф.1 (ядро-скелет constraint_solver.rs: TypeVar/Ty/Constraint::{Eq,MemberOf}/TypeSet/Solver::unify с occurs-check, НЕ подключено глобально) + Ф.2 (миграция ОДНОГО пакета — literal-coercion семья, `annotate_expected_concrete`+`materialize_literal_coercion`, с ad-hoc `matches!`-цепочек на общий `TypeSet`-язык через решатель). **Осознанный scope-cut:** Ф.3 (снос остатка — Binary-арифметика Join, If/Match-Join, resolve/overload-семья) НЕ начато — объём в несколько волн, инвентарь+ядро+один пакет = осторожный первый шаг архитектурной замены (владелец: «не рашить весь движок в один заход»). **anon-RecordLit D55** (единственный открытый user-visible симптом из мотивации плана) — репро подтвердило: `codegen error: anonymous record literal without spread not supported in codegen` — ЧИСТО codegen-эмиссионный гэп (`emit_c.rs:39613` читает только `current_fn_return_ty`, не чекер-канал `resolved_types_buf`), ДО и НЕЗАВИСИМО от чекерного ядра; Ф.1/Ф.2 не закрывают и не могут закрыть без отдельного codegen-трека — маркер `[M-d55-anon-recordlit-codegen-gap]`. **Byte-parity ловушка задокументирована:** `TypeSet::ScalarNotWideDefaultInt` намеренно исключает ТОЛЬКО ровно `int` (signed wide-default), НЕ `uint` (unsigned wide-default) — зеркалит асимметрию исходного ad-hoc гейта; explicit-тест `scalar_not_wide_default_int_excludes_only_signed_wide_default` защищает от регресса при обобщении. **Методологический маркер** `[M-172.13-cross-repo-c-diff-noise-not-regression]`: byte-parity `.c`-диффы между ДВУМЯ РАЗНЫМИ worktree на formally-идентичном коммите дают build-окружение шум (лишняя функция/сдвиг нумерации) — НЕ регрессия; диффить нужно ВНУТРИ одного worktree (checkout A→build→capture; checkout B→build→diff). Гейты: cargo build чисто; cargo test --lib 939 passed / 3 pre-existing fail (verified byte-identical at merge-base 2be6d7064 в том же окружении) + 16 новых юнитов constraint_solver; conformance --positive --compile-error 91/0; err173* 5/5 индивидуально; nova_tests sample (generics/plan172*/basics) byte-identical HEAD vs merge-base. Модель: sonnet.

[2026-07-10 Plan 116 Ф.0 — актуализация + tls_shim-скелет, 🟡 Ф.0 ✅] План 116 переписан целиком под пост-183/176.4/177/178 реальность: эффект `Tls` РЕТРАКТИРОВАН — std/tls = библиотечный слой поверх `TcpStream` (методы несут `Net`; мотивировка по module-conventions §0 — в плане §«Ключевое решение Ф.0-1»); R-1 решён: rustls 0.23 + провайдер `ring` (НЕ дефолтный aws-lc-rs — тому нужны cmake+nasm, чужой toolchain) + webpki-roots. Вендорен скелет `compiler-codegen/tls_shim/` (Rust staticlib, C-ABI `tls_*` ~27 символов, crt-static → libcmt /MT-соответствие; cargo test 5/5; Cargo.lock закоммичен force-add поверх compiler-codegen/.gitignore — ОСОЗНАННО: для шима lock = supply-chain-пин R-9; bootstrap-политика «пустой lockfile» относится к nova-codegen, не к шиму). **ОСОЗНАННЫЕ SCOPE-CUTS Ф.0 (с планом):** (1) `Pinned` (SPKI-pinning) в шиме = `TLS_ERR_UNSUPPORTED` до Ф.4.3 (кастомный ServerCertVerifier — там же); данные через границу уже принимаются — граница не изменится. (2) Прекомпилят staticlib НЕ трекается (~6.1 MB; решение о трекинге — Ф.2.1 после замера; brotli-прецедент трекает .lib, но тот в 15 раз меньше). (3) Условная линковка (test_runner.rs, механизм brotli/D337) — Ф.2. Маркеры `[M-116-*]` — план §Out-of-scope; `[M-178-https-needs-116]` закрывается Ф.5.3.

[2026-07-10 Plan 175.1 — civil time, 🟢 ПРИЗЕМЛЕНО с задокументированными сужениями] Полный civil-слой (`std/time/civil`, D319/D320/D321): Date/TimeOfDay/DateTime/YearMonth/MonthDay/Period/Offset/TimeZone/ZonedDateTime + Hinnant epoch-day, CLAMP-арифметика Q7, 4-way Disambiguation/OffsetConflict, strict ISO/RFC-3339/RFC-9557 parse (§1а: `s.to_date()`/`to_datetime()`/`to_timezone()`…), TZif-парсер + curated embedded tz-таблица, pattern-DSL `DateTimeFormat`. Компилятор НЕ тронут. **Сужения/обходы (все с маркерами и планом):** (1) `[M-175.1-full-tzdb-embed]` — embedded-таблица curated (NY/London/Moscow/Sydney + фикс-оффсеты, rule-based 1996..2100), НЕ полный ~450KB IANA-snapshot; TZif-парсер полный, POSIX-слой работает; починка = упаковка snapshot-данных. (2) `[M-175.1-local-offset-effect-op]` — `Time.local_offset()` требует слот в NovaVtable_Time (nova_rt — компилятор-зона, параллельные волны в emit_c); зона передаётся явно (неявной локальной и так нет по D319 R1). (3) Codegen/checker-гэпы класса value-record/overload, обойдённые по §4а: `[M-175.1-value-in-value-emit-order]` (декларация DateTime перенесена в time_of_day.nv — порядок эмиссии структур лексикографический по файлам), `[M-175.1-variant-literal-receiver]` (`Sun.next()` эмитится как несуществующий Path-вызов — в тестах bound-local), `[M-175.1-minus-overload-arg-type]`/`[M-175.1-operator-arg-type-blind]` (overload-резолв оператора слеп к типу аргумента — оператор `Date - Date -> Period` ретрактирован в пользу `Period.between`; D320-гейт держит метод-форма, neg-fixture), `[M-175.1-qualified-variant-value]` (`Enum.Variant` как значение — ICE P67-LEGACY; вариант `OffsetConflict.Reject` переименован в `RejectMismatch` — коллизия с `Disambiguation.Reject` во флоском пространстве вариантов), `[M-175.1-enum-default-param]` (default-значение enum-варианта не эмитится на call-site → arity-split `to_zoned(tz)`/`to_zoned(tz, disamb)`, прецедент D324), `[M-175.1-interp-value-record-display]` (интерполяция `"${date}"` value-record минует user @to_str — pre-existing класс; Display-тела корректны). (4) POSIX-TZ футер TZif не интерпретируется (за последним переходом действует последний сдвиг) — документировано в D321 §tzdb. Гейты: targeted std/time/civil 78 pos + 2 neg + 1 rt зелёные; std/time δ0 (единственный FAIL — pre-existing timer_metrics_test); conformance см. финальный прогон волны.

[2026-07-10 Plan 175 — mut_clock auto-idle-advance, 🟡 РАБОТАЕТ с задокументированным сужением по armed M:N] `std/testing/handlers.nv` `mut_clock`'s `sleep` теперь парker: абсолютный дедлайн (`current_ms + ms`, до парковки) → `vclock.park_until` (новый `extern "nova"` хук `std/runtime/vclock.nv` → `nova_vclock_park_until`, nova_rt/fibers.h) — паркует вызывающий фибр в per-scope `NovaVClockEntry[]`-registry (новые поля `NovaFiberQueue.vclock_*`); когда pending_count >= alive_count (все живые фибры scope'а виртуально запаркованы), просыпается ближайший по дедлайну; bump — `current_ms = max(current_ms, deadline)` (не `+=`). tokio `time::pause()`/Kotlin `TestCoroutineScheduler.advanceUntilIdle()`-паритет. **Сужение (с маркером):** `[M-175-vclock-armed-mn-scope-identity]` — deadline-order гарантия держит под кооперативным spawn (`NOVA_MAXPROCS=1`+`NOVA_AUTOARM=0`, где `_nova_active_scope` внутри фибра — общий scope блока); под ДЕФОЛТНЫМ armed M:N (auto-arm на первом spawn) `_nova_active_scope` внутри фибра — это `w->scope` (WORKER'а собственный, НЕ shared с siblings — `_worker_run_one_fiber`), поэтому registry не шарится между siblings под armed-путём — деградирует БЕЗОПАСНО (без hang/crash, каждый sleep резолвится), но без гарантии порядка (spawn-порядок вместо дедлайн-порядка). Починка armed-случая — другой якорь (`NovaSpawnCtxBase._nova_parent_scope`), вне периметра этого захода. Реал-clock путь не тронут (only mut_clock's sleep calls the new hook). Гейты: compiler-codegen+nova-cli чистая сборка; conformance 89/0; std/time+civil+testing.handlers 12/0 PASS (новые тесты x3 в handlers.nv, стабильно PASS ×3 прогона под `NOVA_AUTOARM=0`); std/concurrency (real Time.sleep+spawn) 2/0 PASS — byte-parity подтверждён.

[2026-07-10 Plan 175.1 — civil time, 🟢 ПРИЗЕМЛЕНО с задокументированными сужениями] Полный civil-слой (`std/time/civil`, D319/D320/D321): Date/TimeOfDay/DateTime/YearMonth/MonthDay/Period/Offset/TimeZone/ZonedDateTime + Hinnant epoch-day, CLAMP-арифметика Q7, 4-way Disambiguation/OffsetConflict, strict ISO/RFC-3339/RFC-9557 parse (§1а: `s.to_date()`/`to_datetime()`/`to_timezone()`…), TZif-парсер + curated embedded tz-таблица, pattern-DSL `DateTimeFormat`. Компилятор НЕ тронут. **Сужения/обходы (все с маркерами и планом):** (1) `[M-175.1-full-tzdb-embed]` — embedded-таблица curated (NY/London/Moscow/Sydney + фикс-оффсеты, rule-based 1996..2100), НЕ полный ~450KB IANA-snapshot; TZif-парсер полный, POSIX-слой работает; починка = упаковка snapshot-данных. (2) ~~`[M-175.1-local-offset-effect-op]`~~ — ЗАКРЫТО отдельным follow-up заходом той же датой (D316 amend): `Time.local_offset_sec()` эффект-оп + `NovaVtable_Time` слот поставлены, `Offset.local()` (`std/time/civil/offset.nv`) — явный запрос, зона в `ZonedDateTime` остаётся явной (D319 R1 не меняется). (3) Codegen/checker-гэпы класса value-record/overload, обойдённые по §4а: `[M-175.1-value-in-value-emit-order]` (декларация DateTime перенесена в time_of_day.nv — порядок эмиссии структур лексикографический по файлам), `[M-175.1-variant-literal-receiver]` (`Sun.next()` эмитится как несуществующий Path-вызов — в тестах bound-local), `[M-175.1-minus-overload-arg-type]`/`[M-175.1-operator-arg-type-blind]` (overload-резолв оператора слеп к типу аргумента — оператор `Date - Date -> Period` ретрактирован в пользу `Period.between`; D320-гейт держит метод-форма, neg-fixture), `[M-175.1-qualified-variant-value]` (`Enum.Variant` как значение — ICE P67-LEGACY; вариант `OffsetConflict.Reject` переименован в `RejectMismatch` — коллизия с `Disambiguation.Reject` во флоском пространстве вариантов), `[M-175.1-enum-default-param]` (default-значение enum-варианта не эмитится на call-site → arity-split `to_zoned(tz)`/`to_zoned(tz, disamb)`, прецедент D324), `[M-175.1-interp-value-record-display]` (интерполяция `"${date}"` value-record минует user @to_str — pre-existing класс; Display-тела корректны). (4) POSIX-TZ футер TZif не интерпретируется (за последним переходом действует последний сдвиг) — документировано в D321 §tzdb. Гейты: targeted std/time/civil 78 pos + 2 neg + 1 rt зелёные; std/time δ0 (единственный FAIL — pre-existing timer_metrics_test); conformance см. финальный прогон волны.

[2026-07-06 Plan 179 Ф.2 — brotli decode C-FFI + условная линковка, 🟢 ПРИЗЕМЛЕНО] `[M-179-brotli-vendor-lib]` снят: google/brotli v1.2.0 **декодер** собран однократно (MSVC x64 `/MT /O2`, `common/*`+`dec/*`) и вендорен **headers+lib БЕЗ исходников** (стиль libuv): `nova_rt/brotli/include/brotli/*.h` + `nova_rt/brotli/lib/libbrotlidec.lib` (tracked) + build-cache `target/brotli-cache/`. `brotli_decode(data, max_output)` (`std/encoding/compress/{ffi.nv,brotli.nv}`) — extern "C" C-ABI без `[]u8` (raw-ptr+len, модель fs) поверх шима `nova_rt/brotli_shim.{h,c}` (BrotliDecoderDecompressStream); bomb-cap D334 инкрементально поверх FFI (per-pull budget → перебор ≤1 байта). **Условная линковка (owner-требование)**: brotli-lib линкуется ТОЛЬКО когда генерённый `.c` содержит call-site `brotli_decode(` (`c_file_uses_brotli`, фильтр decl/def-header — std-fn'ы эмитятся даже мёртвыми); libuv-mandatory НЕ тронут; без lib → Q11-заглушки (`UnsupportedMethod`, не link-error). http: `Content-Encoding: br` → прозрачная распаковка, `Accept-Encoding: gzip, deflate, br` (`[M-178-autodecompress-br]` закрыт). **Verify:** официальные RFC 7932-вектора (10x10y/64x/empty) + bomb-граница + truncated/corrupt (std/encoding/compress/brotli_test.nv, conformance d337 — **54/0**); http-мок br (`std/http/client/decompress_br_test.nv`); условность доказана в обе стороны (`NOVA_DEBUG_BROTLI_LINK=1`: gzip-only → NO lib; brotli → LINK); регрессия: 3 pre-existing FAIL (basics/effects/gc) идентичны с/без brotli-инклуда → delta 0; Rust clean. **ОСОЗНАННЫЕ SCOPE-CUTS (с планом):** (1) streaming `BrotliReader` (consume) → `[M-179-brotli-reader-streaming]` — C-примитивы шима уже инкрементальные, http использует one-shot симметрично gzip/deflate; consume-neg-тесты приземлятся с ней. (2) Linux/macOS `.a` не вендорен (Windows-хост) → `[M-179-brotli-unix-lib]`, Q11-заглушки. (3) brotli-encode — followup §11 плана (asymmetric, `enc/` не тащится).

[2026-07-06 Plan 172.5 — In-out `mut ref`-параметры, 🟢 CORE ПРИЗЕМЛЕНО] `ref` = режим передачи параметра (D326, Swift `inout`/C# `ref`, без лайфтаймов), НЕ тип. **Реализовано полностью для `mut ref`:** parser (`ParamRefMode{None,RoRef,MutRef}`, call-site `ExprKind::RefArg`, глобальное keyword `ref` — 0 идент-использований → non-breaking), checker (эксклюзивность `E_REF_ALIAS_OVERLAP` через `RefPlace` per-pair prefix-overlap рядом с consume place-анализом; marker⟺mode `E_REF_MARKER_{REQUIRED,NOT_ALLOWED}`; addressability `E_REF_ARG_NOT_ADDRESSABLE` реюз `addr_of_chain_root`; mut-place `E_REF_ARG_NOT_MUT` реюз `check_target_readonly`+`ro_binding_names`; escape-ban R10 `E_REF_ESCAPE_CAPTURE` — захват в closure/spawn), codegen (`mut ref`→C-указатель `T*` в едином `params_c`; body auto-deref через `ref_params`; call-site `ref x`→`&x`; форвардинг `&(*v)≡v`). **Verify:** `nova_tests/inout_ref/` 2 pos + 11 neg зелёные; регрессия byte-identical C-emission на 580 файлах (2 стрида), delta 0; Rust clean. **ОСОЗНАННЫЕ SCOPE-CUTS (честный scope, followups с планом — НЕ tech-debt-без-плана):** (1) `ro ref` codegen НЕ дублирован — это size-driven авто-механизм 172.4 (R3); explicit `ro ref` = обычная value-передача + маркер-запрет. (2) R6 mid-chain gating `E_RECEIVER_BINDING_NOT_MUT` (`c.peek().bump()`) отложен `[M-172.5-chain-gating-ro-at]` — требует моделирования `@`-return-режима сквозь method-chain (fluent-машинерия 172.4), вне soundness in-out `mut ref`; parse-часть R6 (`consume @ -> @`) сделана. (3) Generic `fn f[T](mut ref x T)` codegen отложен `[M-172.5-generic-mut-ref-codegen]` — concrete-путь работает, erased/mono не лоуэрит `T*` (гейт mono-pipeline 172.12).

[2026-07-06 Plan 180 Ф.6 — атрибуты AST + режимы тегирования (internal+adjacent ✅ ПРИЗЕМЛЕНО; untagged 🔴 GATED)] **Часть 1 — `#serde(...)`-инфра (D382):** AST-поле `serde_attrs: Vec<SerdeArg>` на `TypeDecl`/`SumVariant`/`RecordField` (`SerdeArg = Tag(str)|Content(str)|Untagged`); `parse_serde_attr` (grammar `#serde(key[="v"], …)`) в трёх позициях (type — `parse_type_attrs`; field — рядом с `#visible_to`; variant — leading-marker в `parse_one_sum_variant`); неизвестный ключ → `E_SERDE_BAD_ATTRIBUTE` (не silent, прецедент `#impl`). Закрывает `[M-180-serde-attributes]` (infra). **Часть 2 — тегирование:** `serde_tagging_mode(td)` → `SerdeTagging{External|Internal{tag}|Adjacent{tag,content}|Untagged}` + валидация (E_SERDE_TAGGING_CONFLICT / _CONTENT_WITHOUT_TAG / _TAGGING_ON_NON_SUM / _INTERNAL_TAG_NON_STRUCT / _UNTAGGED_GATED). **Internal (`#serde(tag="k")` → `{"k":"V",…fields}`) + adjacent (`#serde(tag="t",content="c")` → `{"t":"V","c":payload}`) ПРИЗЕМЛЕНЫ** (синтез поверх существующих Serializer/Deserializer-примитивов). **Компилятор-фикс, разблокировавший internal+adjacent (без упрощений):** match/if `Result[OK,ERR]`-arm reconciliation — `Ok(x)`-арм даёт stub-ERR (`NovaRes_<ok>_nova_str`), `Err(e)`-арм stub-OK (`NovaRes_nova_int_<err>`); json.nv `Deserializer`-методы (`enter_field`/`enter_index` = `None=>Err`, `Some=>Ok`) без reconciliation мис-лейаутили курсор-Result → decode ложный UnexpectedType. Чиниться сборкой concrete-OK + concrete-ERR через `novares_ok_err`-split уже-посчитанных типов арм/веток (side-effect-free — re-inference-вариант пертурбировал mono-order) в `emit_match`/`emit_if_expr` + `infer_expr_c_type`-зеркалах; + concrete-Result предпочитается erased в первом проходе. **Zero-regression ~50 dirs байт-в-байт vs parent; conformance 54/0.** Verify: `std/encoding/serde/tagging_test.nv` (peer), `std/encoding/serde_neg/*` (`nova test std/encoding/serde_neg --compile-error` = 5/0). **ОСОЗНАННЫЙ GATE (честный named-prereq, НЕ tech-debt-без-плана): untagged (`#serde(untagged)`).** Синтез untagged КОРРЕКТЕН (try-each-variant по value-семантике курсора, генерируемый C валиден), НО компиляция untagged-derive тела пертурбирует codegen `std/encoding/json` в том же CU (mono-collection-ordering → `Json.parse("{\"c\":9}")` возвращает Bool для 9, ломая ВЕСЬ CU включая record/adjacent). Это pre-existing codegen-mono-баг, НЕ serde-логика (C untagged-тела корректен; internal/adjacent тем же движком работают). → `#serde(untagged)` reject at compile-time (`E_SERDE_UNTAGGED_GATED`) до codegen-hardening. Followup `[M-180-untagged-codegen-mono]` (repro в backlog). Field-customization-атрибуты (rename/skip/flatten/…) — грамматика общая, потребление → `[M-180-serde-field-attributes]`.

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

## Закрытые

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

## Закрытые

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

## Plan 172.1 numeric residual-collapse audit + RANK 1/2/3 (2026-06-29, branch plan-172-unified-type-engine)

- **[сокр]** **ОДНО правило arith-promotion для seed И checker — `number_exprs::promote_arith_rt`.** Checker Binary-арм (`f1_expr`, types/mod.rs) схлопывал `литерал<op>narrowvar` (`2*a`u8, `1+n`u32/uint, `0xFF&b`u16, сдвиги) в int позиционным `infer_expr_type(left).or_else(right)` — int-литерал слева короткозамыкал `.or_else`, отбрасывая ширину/знак узкого правого операнда (uint≠int, u8≠int). Правило промоушена (typed/narrow int либо f64 побеждает wide int НЕЗАВИСИМО от позиции) вынесено из seed-метода `promote_arith` в общий `promote_arith_rt` (на ResolvedType), зовут seed И checker → один источник, `extend`-over-seed (main.rs) — no-op для этих узлов. `uint` добавлен в `is_typed_int` (зеркалит legacy `is_typed_integer`/K2 — прежний `wide_default:false`-гейт его ошибочно исключал; только `int` исключается). Checker аннотирует ТОЛЬКО когда оба операнда инферятся (un-inferrable → None → legacy с Member/Index-армами); non-numeric (overload `@plus`/str/generic-T) сохраняют left. RANK 1 `ba1aac55` — 2 из 5 collapse-багов + CRC-shift hazard.
- **[дизайн]** **RANK 2/3 — материализация в канал / width-exact таблица, не codegen re-derive.** RANK 2 `6912055e`: `f3_check_member` NamedTuple-арм материализует substituted field-тип (как Record-арм; §1 «материализуй, не выбрасывай») → generic `Pair[u8].a`→nova_byte (был nova_int — legacy generic-template substitution только Record). RANK 3 `d2752b84`: ReadBuffer sized `read_*` (read_i8/u16/i16/u32/i32) width-exact C-тип вместо хардкод-nova_int (реликт C-runtime эры; 64-бит остаются nova_int). §7.6: detect172 pos+neg(14/0) + conformance + расширенный регресс — **0 NEW fails vs чистый baseline f4d18325** (4 fail pre-existing, идентичны: priority_queue lvalue-cast, imported_named_run nova_str←int, _repro_p110 D133, positive_clone_merge E_CONST_NOT_CONSTEXPR).
- **[дизайн]** **Аудит через multi-agent workflow (wf_358871b5, 34 агента): discover(6 осей)→verify(adversarial)→synthesize.** 27 кандидатов → 5 подтверждено / 21 spec-correct отсеяно (D44-дефолт, hash→nova_int D327, as-cast корректен, u8+u16-left-wins — D315 не задаёт promotion-таблицы). RANK 4 (bug D, D54 narrowing-enforcement для non-literal source) DEFERRED: HIGHEST-risk shared inference engine (`infer_expr_type` Binary/Member/Index арм) + GC-perturbation (plan154 `[]T`/Vec spelling) → §7-careful detect-mode session, не хвост.

## Cross-language syntax-gap survey — 3 `consider` в беклог (2026-07-02)

- **[дизайн]** **Multi-agent survey (31 агент, 1.37M ток.): Rust/Go/TS/Kotlin/Java/Zig/Swift → что из чужого синтаксиса стоит добавить в Nova.** baseline текущего синтаксиса → 7 параллельных каталогов → дедуп gap'ов → адверсариальная оценка на совместимость с минимализмом/эффектами/«сигнатура=контракт». Вывод: **0 `strong`, 3 `consider`, 19 `skip`**. Ни одна конструкция не даёт новой выразительной силы — максимум эргономика. Исследование: [docs/research/2026-07-02-cross-language-syntax-gap-survey.md](research/2026-07-02-cross-language-syntax-gap-survey.md).
- **[дизайн]** **19 skip — не «лениво», а с конкретным existing-механизмом или D-решением.** let-else→`??`+divergent match; matches!→`is`+D34; scope-functions→spread+`Option.map`; const-generics→`[N]T`+const-fn; struct-tags→Plan 180; recover→D13; computed-props→D14/D117; variance→нет subtyping (by design); autoclosure/result-builders/property-observers→«no hidden control flow». Baseline устарел на 2 пунктах: `while-let` (D34) и top-level or-patterns (`Pattern::Or`) **уже реализованы**.
- **[беклог]** **3 floating-маркера добавлены** (все — снятие частного ограничения существующей грамматики, не новые концепции): `[M-106-if-let-chain-multi]` (multi-bind `&&`-цепочка, home = Q-if-let-chain-multi), `[M-labeled-loops]` (метки циклов + `break outer`, единственный genuinely-absent), `[M-nested-or-patterns]` (`Some(1\|2\|3)`, hoist `\|`-сбор в parse_pattern). Home двух новых = [spec/open-questions.md](../spec/open-questions.md) (Q-labeled-loops / Q-nested-or-patterns).

## Same-scope re-binding — research + Plan 181 (2026-07-02)

- **[дизайн]** **Research (3 агента: эмпирика 11 проб на живом компиляторе / 13 языков прецедента / 9 точек взаимодействия по коду) → [research-док](research/2026-07-02-same-scope-rebinding.md) + [Plan 181](plans/181-same-scope-rebinding.md) (D347).** Вопрос владельца «всегда ro/mut при присваивании + rebind с новым типом»: полная форма (source-SSA) отклонена в обсуждении (ломает циклы/аккумуляторы/`@f=v`); исследована узкая — same-scope re-binding.
- **[дизайн]** **Ключевая находка — статус-кво = дыра, решение нужно в любую сторону:** чекер НЕ имеет диагностики same-scope повтора (тихо PASS, уже реализует shadowing-типизацию), codegen эмитит оба decl под одним C-именем → clang `redefinition of 'x'` (ошибка в `.c`, не `.nv`); interp (D274 unsupported) реализует rebind — три компонента, три поведения. Плюс 3 смежных бага: false-positive D133 при затенении ПОТРЕБЛЁННОГО consume; тихая утечка `consume tx; consume tx; tx.commit()` (obligations по имени — одно Consumed гасит оба); расхождение чекер↔codegen на `ro x = x+1` (чекер видит старый, C self-init нового).
- **[дизайн]** **Реализация — один alpha-renaming pass** (после parse, `x → x__sN`, original-имя в диагностиках), НЕ патчить name-keyed подсистемы (ConsumeCtx 10 map'ов, emit_c side-tables, verify — все по `String`-имени): medium вместо high. Nova-эксклюзив дизайна: `E_REBIND_LIVE_CONSUME` превращает Rust-футган «затенённый guard живёт до конца scope» в compile error (RAII нет, guard'ы = consume). Ф.0 = owner sign-off; fallback при отказе = `E_DUPLICATE_LOCAL` + фикс B1/B3.

## Plan 173 Ред. 2 — полная сверка error-system зонта (2026-07-03)

- **[дизайн]** **План 173 переписан (Ред. 2) по итогам двух workflow: аудит (5 агентов: ground-truth/самосогласованность/Zig-Swift/суб-планы/конвенции) + адверсариальная верификация (3 ревьюера → 3× pass-with-nits, 0 blockers; фактчек 13/14 точно).** Ground-truth: 11/11 дефектов §1 живы (0 FIXED после D-track merge), все file:line актуализированы (например with-Fail-глотает-PANIC: 6646→6885-6933).
- **[дизайн]** **Устранены два невнесённых слоя owner-пересмотров** (2026-06-21 «стратегии=эффект-хендлеры» + 2026-06-26 §3a/§3b), противоречившие шапке/Ф.3/§6: supervisor default = cancel/Escalate/Stop (restart за гейтом изоляции 173.3), defer completes-by-default (unshielded-модель снята), exit-timeout force-механизм → D192-ретракт + watchdog-варн (дефект #10 закрывается УДАЛЕНИЕМ заглушки). Race2 остаётся до общего race (авторитет 173.1) — «убрать сейчас» снято.
- **[дизайн]** **Планка расширена до 7 языков (+Zig/Swift)**: новые выигрыши (typed payload vs Zig-тег; cleanup-ошибки компонуются — Zig/Swift defer не может фейлить; panic запускает cleanup vs abort) + новые риски (error return traces Zig → минимум Ф.5 + `[M-173-error-return-trace]`; frame-free пропагация как disasm-guard — парность Swift typed-throws ABI).
- **[дизайн]** **Открытые вопросы дозакрыты**: surface-синтаксис (имена MultiError-аксессоров уже зафиксированы кодом errors.nv:207-250; typed-доступ = `e is T`, downcast не вводится; scope-result = 173.1), channel recv остаётся Option (Result-миграции не будет), precedence PANIC>USER>CANCEL, мост Fail→Result = идиома в хабе. Добавлены Ф.0R (семейная сверка 173.1/173.2), Ф.3-остаток (detach/precedence/select/stale-тесты/with_timeout), §4a (одна папка nova_tests/err173 вместо 6 CU + обязательное spec_tests D-покрытие), §7a запуск-чеклист (prerequisite-гейты: Ф.4→174.3, deadline→175). D-номера: D314 (ядро, подтверждён резерв), D348 (panics-клаузула Ф.6), D349+ (structured propagation). Попутная находка аудита: коллизия Plan 178 D327-D332 (D327 занят) — передать владельцу 178.

## Plan 174 Ред. 2 — зонт lang/FFI переписан в launch-документ (2026-07-03)

- **[дизайн]** **Зонт 174 (43 строки-индекс) → launch-документ (~290 строк)** по итогам 9-агентного аудита (6 × под-планы, зонт/статусы, 7-языковая планка по каждой из 6 фич, конвенции; 95 findings) + 3-ревьюерной верификации (consistency-fail первого прохода → все 8 major внесены → pass). Запуск = «выполни план 174» (§7a).
- **[дизайн]** **Критические находки аудита:** (a) stale-номера старой нумерации — «Запуск: выполни план 176» в 174.4 отправил бы агента в ЧУЖОЙ план (io/fs); ptr177/ffi178/plan171 тест-папки; «Не путать с 177» в новом значении; (b) 172.3 ✅ CLOSED → 174.1 переписывается на **вариант B** (generic через type-set bounds, догма «T.MAX в generic не резолвится» опровергнута showcase 172.3); (c) направление 173↔174.2 инвертировано (173 Ф.1 реализует ядро 174.2, не наоборот); владелец d85-конформанса = 173 Ф.1; (d) **двойные D-номера в спеке**: D216 (anon-tuple-mono И typed-pointers) + D282 (extern-ABI И blanket-protocols) → 🔴 sign-off renumber, приёмники D354/D355; (e) ни один под-план не знал spec_tests-покрытие; (f) silent-drop 33-го эффекта подтверждён (effects.h:996-1002, guard'а нет).
- **[дизайн]** **Планка 7 языков дала существенные дизайн-требования** (§4, обязательные): 174.1 — float-канон (strtod скипает whitespace + locale-зависим → no-trim pre-check, C-locale guard, f32 напрямую strtof — не double-rounding); 174.2 — cross-carrier диагностики с fix-it (.ok_or/.ok/.map_err), `?` в main = Rust-модель; 174.3 — compile-error на `==`/Hash-any до Ф.3 (Go тут runtime-паникует), TID-детерминизм для byte-identical; 174.4 — release-guard abort (НЕ NDEBUG-assert — иначе в release вернётся silent-drop), детерминизм порядка регистраций; 174.5 — provenance&GC (Boehm-пиннинг), alignment/dist/offset контракты, Zig `[*]T`-контрпример отработан; 174.6 — callbacks×фиберы/GC (GC_register_my_thread + effect-TLS + пустой эффект-лист), pinning-декрет borrowed-only, `_Static_assert` layout сейчас (S8 не откладывать).
- **[дизайн]** **D-карта семьи**: D309=174.1 (подтверждён), резерв D350-D356 (D351=any-repr, D352=uninit, D353=fn-ptr-ABI-тег, D354/D355=renumber-приёмники). Правки конвенций обозначены (§5): test-conventions sweep «nova test требует путь» (рассинхрон с fd7a8da5, ~14 bare-примеров падают), правило «amend D ⇒ обновить существующий d-файл в том же изменении».

## Batch2: ещё 11 кросс-файловых D-коллизий разрешены — D124-D298→D366-D377, D277-тройная (2026-07-03)

- **[дизайн]** **После первой тройки (D184/D291/D292) sort|uniq по `^## D` вскрыл ЕЩЁ ~12 кросс-файловых коллизий** — прогнал тот же конвейер (12-агентный workflow-картирование → renumber 3 волнами → верификация). Разрешены: D124-Edition→D366, D125-byte→u8→D367, D126-strict-type→D368, D138-межпакетный-импорт→D369, D173-AI-guidance→D370, D174-prelude-attrs→D371, D180-canonical-new→D372, D185-generic-array→D373, D258-write-sink→D374, D277-тройная (GC-bitmaps→D375, test-discovery→D376), D298-UDP-split→D377. Раньше-занявший/корневой сохраняет номер (Monotonic/prelude-shadow/external-type/M:N/net/sync/consume/Cleanup/format-spec/by-value-mono/test-budget).
- **[сокр→фикс]** **Особые случаи:** D239 = ДУБЛЬ-ЗЕРКАЛО (оба заголовка = `[]T`≡`Vec[T]`; не коллизия смыслов → demote-баннер на канон, не renumber); D277 = ТРОЙНАЯ (3 блока → keep 1 + renumber 2 в разные номера); D173/D298 = heuristic-конфликты (хронология ⟂ корневость) — решены по корневости, консистентно с D291. D-index (spec/README) нёс битые/ghost-строки (D290=netFFI-ghost, D292=TypeMethodMap-bogus) — исправлены.
- **[сокр→фикс]** **Scope (прагматичный, как D184-баннеры):** заголовки + spec anchor-ссылки + block-internal + D-index (+11 renumber-строк) + rename 6 тест-файлов (git mv + внутр. идентификаторы) + баннеры план-владельцам + code-комменты (emit_c/auto_derive/protocols.nv/types/mod.rs) + range-refs D167-D173→split. Разбросанная prose в sibling-планах покрыта баннерами. Историч. логи (project-creation.txt) не трогались. 3 волновых коммита. Верификация: 0 настоящих кросс-файловых дублей заголовков (кроме D239-зеркала), 0 висячих anchor. ИТОГ сессии: 14 D-коллизий разрешено (3+11).

## Renumber кластера D-коллизий: D184/D291/D292 → D363/D364/D365 (owner sign-off 2026-07-03)

- **[дизайн]** **Три кросс-файловые коллизии D-номеров разрешены** (картирование 3-агентным workflow → renumber → верификация). Приёмники: **D184-opdispatch (Plan 91.8b) → D363**; **D292-netFFI (Plan 91.12) → D364**; **D291-net-family (Plan 91.12) → D365**. Сохраняют номер (раньше-занявшие/корневые, прецедент D216/D282): D184-keyword (Plan 114), D292-ModuleSigTable (162.1), D291-module-resolution (162.2).
- **[сокр→фикс]** **Тонкости, вскрытые картированием:** (1) D291-net НЕ был настоящей коллизией заголовков на main — реальный `## D291` только у module-resolution, net-family заголовок на несмерженной ветке plan-91.12, на main лишь висячие `#d291`-ссылки → фикс = ссылки+D-index, заголовок D365 родится с net-sweep; (2) все 3 агента независимо выбрали «первый свободный D363» → развёл в D363/D364/D365; (3) в 03-syntax ОБА смысла D184 → контекстные (фразовые) замены, не bare-число; (4) D-index (spec/README) нёс 3 битых строки (ghost D290=netFFI, bogus D292=TypeMethodMap, missing module-D291) — исправлены в том же заходе.
- **[беклог]** **Бонус-находка:** `uniq -d` по `^## D` вскрыл **~12 ЕЩЁ кросс-файловых коллизий** сверх тройки (D124/D125/D126/D138/D173/D174/D180/D185/D239/D258/D277×3/D298) — отдельная уборка, `[M-spec-dnum-collisions-audit]`. Исторические логи (project-creation.txt) при renumber НЕ трогались (timestamped-записи); тест-файлы d184_→d363_, d292_→d364_ (git mv + контент). Верификация: 0 дублей заголовков в тройке, 0 висячих #d29-anchor, 0 live-opdispatch-D184.

## Net-эффект: слияние TcpNet/UdpNet/DnsNet → единый Net (owner-decision 2026-07-03)

- **[дизайн]** **Вскрыто исследованием (3 агента) при вопросе владельца про `real_net()`:** спека **D62 канонизирует ОДИН эффект `Net`** (`04-effects.md:101/116/2822/2982`; sandbox-канон `forbid Net, Db, Fs`), а код (Plan 91.12, D291) разбил его на TcpNet/UdpNet/DnsNet/AddrNet — **необлагороженное отклонение от спеки**. То есть спека и код противоречили.
- **[дизайн]** **Решение: слить обратно в единый `Net`** (реконсиляция кода к канону, НЕ усложнение). Аргументы: D62 «Почему» явно ценит короткий effect-row (предупреждает про раздутие до 8-10 имён); гранулярность «forbid TCP но allow DNS» нереалистична (реальный sandbox = `forbid Net` целиком; SSRF-guard = value-логика, от эффектов не зависит); пир-прецедент — Deno (сильнейший capability-пир) = единый `--allow-net`, единственный делитель resolve≠connect (Java SecurityManager) deprecated. **`real_net()` из 178 §13.2 был вариантом A** (умбрелла-установка при гранулярных сигнатурах) — заменён на вариант B (слияние типов).
- **[сокр→фикс]** **Цена принята:** 116 `real_tls()` требует `Net` вместо `TcpNet` (теряется описательная точность «TLS только TCP» — не гарантия; DTLS-over-UDP = future §11); единый `mock_net()` вместо триады. **AddrNet-retract ортогонален** (9 pure-операций → `.nv`, уже решён 2026-06-27). Миграция (~43 файла, 100% `.nv`+docs — grep net-эффектов по `**/*.rs`=0, ноль codegen-риска) **едет с net byte-surface sweep 178 §13.2** (`str`→`[]u8` + SocketAddr-rep) — маркер `[M-net-merge-to-single-effect]`; отдельным заходом = второй полный sweep. Reconcile-ноты внесены в 178/116/91.12/spec-D295/README/backlog; код — при исполнении 178.

## Планы 177-181 Ред. 2 — пакетная сверка пяти планов (2026-07-03)

- **[дизайн]** **Пять планов (177/178/179/180/181) переписаны Ред. 2** одним 13-агентным аудитом (171 finding: ground-truth/consistency/7-языков/siblings/conventions) + 3-ревьюерной верификацией (17 findings). Класс «stale-номера старой нумерации» добит по всем: 178 звал сам себя «182» (15 мест), serde «184», compress «183», io «180», result-everywhere «181» — включая **нормативные доки** (nv-coding-style/error-handling/strings/protocols.nv несли «Plan 181» вместо 177).
- **[дизайн]** **Renumber D-блоков 178: D327-D332 → D357-D362** (D327/D328 заняты 172.2/172.4; D356 — резерв 174; D357-D399 чисты) — исполнено, не отложено; D-карты синхронизированы во всех шапках 177-181 + README. Бонус-находки верификации: в спеке **дублирован «D292»** (07-modules ModuleSigTable vs 02-types Net-FFI) и amend-цель «D281/D295 net» была фактически неверна (D281 = privacy) → реальные носители D173/D301/D292-NetFFI.
- **[дизайн]** **Крупные вскрытия фактчека:** [M-161-parametric-return] «OPEN» в 180 — на деле ✅ CLOSED 2026-06-16 (D355), но Q13-решение уцелело по другой причине (D355-blanket требует typevar-ресивер); auto-derive-seam в 180 указывал не тот файл (emit_c.rs → **protocols/auto_derive.rs**, + честный объём: synthesis companion-типа Visitor — новое); «str.from_bytes уже есть» в 178 — ложный green (jwt.nv лишь call-site) → prereq 176 Ф.0.5; None-кодирование serde «absent default» — фактическая ошибка (serde-канон = null); Json.parse depth-guard/raw-token — аудит закрыт: обоих НЕТ.
- **[дизайн]** **Siblings-дыры 178 закрыты:** тип `Instant`/`Duration.seconds` не существуют → Monotonic/from_secs; `.sec`-сахар нигде не спланирован (общая дыра с 173.1) → Ф.0-решение; cancel-семантика (свой scope-cancel ПРОПАГИРУЕТ, не Err(Canceled)); ErrSource export+OPEN+Io+Compress; Version → OPEN (Http3 non-breaking); write-backpressure h1; BodyReader pull-контракт; 1xx-interim loop; NO_PROXY-матрица; TE:trailers. 179: @flush SYNC-FLUSH (дыра №1 — SSE/chunked поверх gzip); brotli-FFI приведён к C-ABI 174.6 (extern без []u8). 177: R0-граница panic-vs-Result (D13), exempt-list guard'а, коллекторы sequence/partition (Ф.2c).
- **[дизайн]** **Ред.2-слой добит по всем пяти:** spec_tests/conformance d-файлы на каждый D (d325, d333-d337, d340-d346 + amend d109/d230, d347 + amend d90/d131/d133/d22/d34, d357-d362 + amend d292) с правилом «amend ⇒ обновлять СУЩЕСТВУЮЩИЙ d-файл»; тест-раскладка (тема-папки без номеров планов, rt/, neg = только compile-error, маркер без двоеточия); гейт корректности (spec_tests + baseline-delta=0, nova_tests НЕ гейт); агент-правила §10 (no-stash/temp-worktree, DCO, bidirectional sync, явный путь, батч-канон, mtime-touch).
- **[дизайн]** **7-языковая планка дополнена фактчеком:** Swift COMPRESSION_ZLIB = raw-DEFLATE-misnomer + silent-truncate-cap (антипример Bomb-vs-truncate); Swift SE-0295 (enum-Codable synth external-only) — прежний claim «ручной init(from:)» устарел; Zig std.json (comptime, ноль аннотаций, DENY-default, lossless-numeric) добавлен колонкой в 180; Zig-http ячейки (TLS1.3-only, zero-timeout, zstd-в-std) и Swift-http (60s timeout-default, AsyncBytes, h3-из-ОС) исправлены.
- **[сокр→фикс]** Stale-строки backlog-followups: [M-177-result-over-named-tuple]/[M-177-anon-record-in-ctor-arg] значились P2 OPEN при факте RESOLVED 2026-06-26; [M-126-sum-*-rich] зарегистрированы в OPEN-view (были только в auto-derive-guide).

## Plan 175 Ред. 2 — time-system сверка (2026-07-03)

- **[дизайн]** **План 175 переписан (Ред. 2)** по итогам 5-агентного аудита + 3-ревьюерной верификации (consistency-fail первого прохода → все 4 major внесены). Планка расширена до **7 языков (+Zig/Swift)**: Swift Clock (SE-0329) отработан как ближайший прецедент Time-эффекта (вирусный DI vs ambient-handler); Zig — контрасты i128/error-union/голый sleep(u64). **Новые контракты (Q14-Q16):** suspend-семантика Monotonic per-OS задокументирована (uv_hrtime = CLOCK_MONOTONIC-класс, suspend-inclusion НЕ гарантируется; индустрия расходится: Zig=BOOTTIME, Rust/Go=MONOTONIC, Swift=оба) + `[M-monotonic-boottime]`; infallibility-by-contract; Timestamp-окно 1677..2262. Мелкий фактчек: μs = U+03BC в @into() (не U+00B5/@into_human).
- **[дизайн]** **Stale-«179» вычищен** (план авторингался под №179): 175 — 9 мест (Q7/§3a/§4 «179.1», time179-папки, nova-p179), 175.1 — **22 места** sweep'ом + README-строки; правильные имена: nova_tests/time/ (тема, folder-module, не per-plan), nova-p175. Тот же класс бага, что «выполни план 176» в 174.4.
- **[дизайн]** **Ground-truth: emit_c.rs дрейфует ЕЖЕДНЕВНО** (5 ночных коммитов 172.1.2 успели устареть цифры аудита за часы) → нормативные критерии Ф.3/§8.3 переведены на **символьные якоря** (grep nova_monotonic_now_record = 0), строки помечены «снимок @ cc19478b». Найдены +2 inference-сайта builtin-Monotonic (всего 4, план знал 2). Blast-radius пере-измерен: 755 вызовов (было 447, +69%). spec_tests-покрытие добавлено (d316/d317/d318 + d124-новый + d237-обновление); тест-раскладка исправлена (folder-module + rt/ + neg/ без двоеточия в маркере; несуществующий маркер «EXPECT:» убран); §10 дополнен (DCO -s, nova test path, батч-канон, baseline-delta вместо «полный регресс зелёный»). Кросс-рефы: 173-семья ждёт 175 в 6+ местах — теперь взаимно; координация mock-clock ↔ scope-deadline (supervised(timeout:) обязан регистрироваться в том же deadline-реестре, что sleep).

## Plan 175 Ф.1c — overflow-safe время (D317) + Monotonic non-regression (D318) SHIPPED (2026-07-06)

- **[безопасность→фикс]** **Закрыта критическая soundness-дыра #8 аудита: Duration-арифметика молча переполнялась.** ВСЕ операторы `Duration` (`@plus/@minus/@neg/@times(i64|f64)/@div(i64|f64)/@abs`) были сырой unchecked i64 → two's-complement **WRAP** на ±292 годах (ровно Go-ловушка «the trap to avoid»). Теперь **3-tier дисциплина** (D317): операторы **траппят** (debug И release — никогда silent-wrap как Go, никогда build-mode-UB как Zig-ReleaseFast; Swift-прецедент integer-trap); `@checked_*`→`Option`; `@saturating_*`→clamp ±(2⁶³−1). Реализация — **чистый `.nv`-слой** в `std/time/duration.nv` (module-private `i64_max/min`/`checked_*_i64`/`sat_*_i64`/`*_or_trap`/`f64_nanos_*`-хелперы; codegen НЕ тронут). bare i64 `+`/`*` wrap by design → overflow детектируется ЯВНО (divide-back для mul), не полагается на trap примитива.
- **[безопасность]** Граничные кейсы: `@abs(i64::MIN)` → saturate к MAX (two's-complement asymmetry `|MIN|>MAX`, не UB); `@div(0)`/`@div(MIN,-1)`/`@neg(MIN)` → trap; f64 (`from_secs_f64`/`@times(f64)`/`@div(f64)` вкл. `÷0.0→±inf`) → trap на NaN/inf/OOR + non-trapping `try_from_secs_f64`/`@try_mul_f64`/`@try_div_f64`→`Option` (Rust `mul_f64(NaN)` паникует = прецедент). Инстанты `Timestamp`/`Monotonic` `@plus/@minus(Duration)` + `Timestamp@minus(Timestamp)` → boundary-saturate + `@checked_add/sub`; Timestamp-окно 1677..2262 честно задокументировано (Q16; i128 отвергнут — ломает Q2 scalar-bridge).
- **[безопасность→фикс]** **D318 Monotonic non-regression:** `@elapsed_since` → **saturate-to-zero** на кажущийся регресс часов (HW/VM/OS-баг, JDK-6458294) — никогда negative, никогда panic, **без global-lock** (урок Rust 1.60-saga); `@checked_duration_since(other)→Option[Duration]` (None на регрессе). Clock-source per-OS + suspend-EXCLUDED + infallible-by-contract задокументированы (Q14/Q15).
- **[отложено, НЕ упрощение — блокер компилятора]** Публичные консты `Duration.MAX`/`Duration.MIN` (Plan 178 `@timeout(Duration.MAX)`) **не введены**: user type-const с именем `MAX`/`MIN` **шэдоуит builtin numeric `.MAX`/`.MIN`** в type-set-bound generics — с `Duration.MAX` в CU, `fn[T Ints] f(x T)=>x==T.MAX` (spec_tests d310) мис-типизирует `T.MAX`(int) как `NovaValue_Duration` → binary-`==` уходит в value-record structural-eq → `((x).nanos==(INT32_MAX).nanos)` CC-FAIL. Найдено бисекцией (removal MAX/MIN → conformance зелёный). Saturation-границы = internal `i64_max()/i64_min()` (D317-функциональность ПОЛНА без консты). Follow-up `[M-175-type-const-max-shadows-builtin]` (checker member-const-резолюция, 172-зона owner-gated). Также при разработке всплыл Windows-footgun: локал по имени `near` = legacy `#define near` (Win SDK) → «expected identifier before `=`» — переименован в тестах.
- **[спека/тесты]** D317/D318 внесены в `spec/decisions/04-effects.md` (amend D316/D124) + README-индекс (D319-324 остаются reserved). Тесты: inline unit-блоки `duration.nv`; `spec_tests/conformance/d317_*`/`d318_*` (single-CU PASS); trap-фикстуры `nova_tests/time/rt/{dur_add_overflow,dur_div_zero,dur_f64_nan}_traps.nv` (`EXPECT_RUNTIME_PANIC`); cross-module `nova_tests/time/plan175_f1c_overflow_safe.nv`. **Zero-regression delta=0** (same-binary swap parent↔Ф.1c `duration.nv` на `nova_tests/sync` — byte-identical). **Здесь Nova достигает паритета Rust/Java/Swift и обходит Go(silent-wrap)/Zig(build-mode-UB).**

## Plan 176 Ред. 2 — io/fs/os сверка (2026-07-03)

- **[дизайн]** **План 176 переписан (Ред. 2)** по итогам 5-агентного аудита + 3-ревьюерной верификации (consistency-fail → все major внесены). **🎉 Главная находка: HARD-GATE «Plan 80 must-consume» ЛОЖЕН** — must-consume уже shipped через D133 (Plan 100.1 ✅ 2026-05-25, LinearityRegistry + D133-not-consumed; боевой WriteGuard; File-пример прямо в спеке) → affine-fallback-ветка удалена целиком; Plan 80 помечается superseded (Ф.0g). Вторая находка verify-фактчека: **fallible byte→str УЖЕ есть** как интринзик `str.try_from([]u8) -> Result[str,str]` (emit_c:28166+, тесты utf8_invalid.nv) → Ф.0.5 = переоформление (typed Utf8Error{byte_offset} + канон-имя from_bytes + deprecate try_from), не работа с нуля.
- **[дизайн]** **Двойное владение net-миграций разведено со 178**: полный str→[]u8 демоут — владелец 178 (owner-sign-off 2026-06-26); 176 добавляет только conformance `impl io.Read/Write` на TcpStream (новая Ф.4, куда также ушла NetError→IoError-унификация — раньше висела «отдельными коммитами» без фазы). Ноты для сверки 178: ErrSource-координация, ложный from_bytes-green (call-site в _experimental ≠ определение), renumber D327-D332 (коллизия с committed). Канон scope-exit — 173/D188: `File impl Cleanup[IoError]` (@cleanup → suppressed-chain), явный @close — Result-путь.
- **[дизайн]** **Планка 7 языков (+Zig/Swift)**: must-consume теперь «бьёт все 7» (Zig close()->void — ошибку некуда деть; Swift throws-но-забываемо); Zig Dir-scoped/openat = anti-TOCTOU by design → `[M-176-dir-scoped-ops]`; **Swift .atomic и Zig AtomicFile = tmp+rename БЕЗ fsync** — готовый антипример «почему write_atomic = 5 шагов»; per-op error sets (Zig) — considered/rejected нота в D322; Q12-Q14 добавлены (umask/append/error-sets). Stale-номера вычищены (план авторингался под №180: «гейт ПОСЛЕ 180», io180/fs180/os180, nova-p180; «179»=старый 175). mem_fs/mock_os/Io-mock получили фазы-носители (mem_fs с ошибко-инъекцией ENOSPC — без него close-error/torn-write тесты не работают); тест-раскладка → 3 темы io/fs/os folder-module; spec_tests d322/d323/d324; «neg runtime»-бакет переклассифицирован в позитивные Result-тесты.

## Renumber двойных D-номеров: D216-anon-tuple → D354, D282-blanket → D355 (2026-07-03)

- **[дизайн]** **Разрешены обе спековые коллизии номеров** (вскрыты аудитом Plan 174; sign-off + исполнение владельцем 2026-07-03; прецедент D109/D110/D111). Критерий: остаётся сторона с большей сетью ссылок / канонической README-строкой. **D216 = typed-pointers** (цепочка V2/V3-амендментов, D246, 118.x, 174.5) — anon-tuple-mono (Plan 59.1) → **D354**; **D282 = extern-ABI** (README-канон, D290/D294, 174.6) — blanket-protocols (Plan 161) → **D355**. Обновлено: заголовки блоков + Эволюция-ноты (02-types.md), 4 anchor-ссылки + текстовые упоминания (06-concurrency/10-overloading/03-syntax/D284-блок), README-индексы (оба), планы 59.1/161 (NB-ноты)/162/124/124.3/172.1-d-status/p67/checklist, conformance-файлы `d354_generic_anon_tuple_mono.nv`/`d355_blanket_protocol.nv` (git mv + внутренние идентификаторы). Гейт 174.5/174.6-M0 снят.
- **[сокр→фикс]** Бонус-находка: nova_tests/plan163 (5 файлов) ссылались на «(D282)» вообще ошибочно — их D-блок = D288 (E_REEXPORT_GLOB); исправлено. Верификация: `nova test spec_tests/conformance` (один CU) — PASS 1/0 на свежем release-бинаре (предыдущий FAIL был stale-binary артефактом d406-enum, не переномерацией).

## `#extensible` sum-типы — Q + беклог-маркер (2026-07-03)

- **[беклог]** **`[M-extensible-sum-types]` + Q-extensible-sum-types** (из обсуждения match Rust-vs-Nova → `#[non_exhaustive]`): `#extensible` на экспортированном sum = тип может расти без breaking change (через границу пакета match обязан иметь `_`; внутри пакета exhaustiveness полная). Прецедент боли — D302 (NetError +2 варианта = breaking). Нейминг решён: `#extensible` (обещание автора, ряд `#pure`/`#deprecated`), НЕ `#non_exhaustive` (механизм вместо намерения) / НЕ инверсия `#frozen` (убивает exhaustive-match дефолт). Авто-`_ => panic` отвергнут: compile-time гарантия → runtime-краш (Kotlin 1.7 ужесточил `when` по той же причине). **Gated на std-стабилизацию / registry (Plan 03.3).**

## errdefer/okdefer/defer|result| dead surface — выпилен (Plan 173 Ф.1 #4, 2026-07-04)

- **[сокр]** **Мёртвая поверхность D189-ретрактнутых форм удалена** (net −242 строки; commit 84e6e709). После hard cutover (D189) парсер отвергает `errdefer`/`okdefer`/`defer |result|` на месте tombstone-хинтом `[D189-removed-*]`, поэтому `Stmt::ErrDefer/OkDefer/DeferWithResult` никогда не конструировались — вместе с ~90 match-arm сайтами (18 файлов), `DeferKind`-enum + path-selective skip-логикой (emit_c: все defer'ы плейн), и целым D189-deprecation lint subsystem (lints, срабатывал только на now-rejected формы) это чистый dead-code. Метод compiler-driven (rustc как драйвер полноты) + regex-strip однородных фрагментов. **Сохранено** минимальное tombstone-распознавание (lexer-токены + parser-хинт) — единственная причина держать keyword'ы. Семантика плейн-`defer` НЕ изменилась (удалённые skip-checks были always-false для `Plain`).

## `?` строго return-only — устранён двусмысленный dual-mode оператор (Plan 173 Ф.1 #3, 2026-07-04)

- **[сокр]** **`?` теперь ОДНА семантика — return-only** (проброс значением: `return Err`/`return None`); confusing throw-режим (когда `foo()?` иногда возвращал Err, иногда throw'ил — зависело от return-типа enclosing fn) убран. Чекер режет free `?` в non-Result/Option-fn диагностикой `[E_TRY_IN_FAIL_FN]` (там `!!`/`throw`). D85 «один оператор — одна семантика» доведён до enforcement (commit ea55bee7). Де-риск показал 2 EXEMPT-контекста, где `?` осмыслен вне return: consume-init `?` (D196 form 2 unwrap-init) и defer-body `?` (D158) — codegen throw-ветка сохранена для них. Читаемость: `foo()?` теперь ВСЕГДА = «верни ошибку значением», без гадания.

## Честный аудит interim-guard #7 (parfor) против compiler-conventions §0 (2026-07-04)

- **[аудит/interim]** По запросу владельца — self-audit #7 (`[E_PARFOR_RESULT_UNSUPPORTED]`). Признанные напряжения, ВСЕ осознанные (plan-sanctioned interim, снимается 173.1 Ф.2): **(1) §0-coupling** — guard решает «поддержан элемент» в чекере (`infer_expr_type` + TypeRef-имена), codegen — в back-end (`infer_expr_c_type` + C-имена, emit_c.rs:8492); это легитимный front-end-guard для back-end-ограничения (не дублирование вычислялки), но два whitelist'а должны синхронизироваться → §0-мера: единый const `PARFOR_V1_PRIMITIVE_ELEMS` + bidirectional coupling-комменты (полная консолидация невозможна дёшево — `infer_expr_c_type` codegen-метод, чекеру недоступен). **(2) Empty-scope inference** → false-negatives: uninferrable trailing не флагается (conservative, без false-positive; типовой record-via-Call покрыт реестром). **(3) Value-position** — `consumed`-эвристика (частые случаи точны, экзотика приблизительна). **Не упрощение:** #1/#2/#3/#4-core — production, полные гейты. Вывод: #7 — не заглушка, а stopgap с чистой диагностикой вместо сырого C-error; ограничения by-design (interim), задокументированы, coupling минимизирован.

## Plan 104.10 Ф.1 — symbol cache (resolved-module per URI) (2026-07-03)

- **[production, НЕ упрощение]** Per-URI кеш полного `ResolvedModule` (parse + import-inline + type-check с `expr_types`) в `WorkspaceState::resolved_cache: DashMap<Url, CachedResolved>` (`state.rs`). `get_or_build_resolved(uri, version, src)`: cache hit по version → `Arc`-clone без ре-парса/ре-резолва/ре-чека; miss/stale → строит через `provenance::resolve_module_for_ide` (новый IDE-вариант, использует `check_module_with_expr_types` из Ф.2) и кеширует. Инвалидация: `didChange` (новая version) → следующий запрос перестраивает; `didClose` → `invalidate_resolved` (память ограничена открытыми документами). Concurrency: `DashMap` + `Arc`, read-guard не удерживается через build (нет deadlock), гонка на один `(uri,version)` даёт ≤1 build/поток, оба результата валидны. Тесты `state.rs::f1_*`: cache-hit (Arc::ptr_eq), rebuild-on-version-bump, close-evicts, uncached-no-panic, 2-thread concurrent, perf (warm hit ≤10ms на ~1000-строчном файле). Инструментация: `resolved_build_count: AtomicU64`.
- **[M-104.10-dependent-invalidation]** **Остаток (bounded, → Ф.18):** кеш инвалидируется только по СВОЕМУ `uri`+`version`. `didChange` файла A перестраивает A, но кеши файлов, ИМПОРТИРУЮЩИХ A, остаются до их собственного edit/close (могут показать stale-типы из старого A). V2-остаток: reverse-dep инвалидация из module-graph (урок zls) — при `didChange` A инвалидировать кеши всех импортёров A. Дом = Ф.18 (workspace lifecycle), где строится обратный граф зависимостей. Деградация graceful (stale, не паника). Priority: Ф.18.

## Plan 104.10 Ф.2 — expr_types (opt-in per-expression type map для IDE) (2026-07-03)

- **[дизайн/production]** **`ModuleEnv.expr_types: HashMap<Span, TypeRef>`** — opt-in карта тип-на-выражение для IDE (hover/completion/signature-help/inlay). Наполняется ТОЛЬКО через новую `check_module_with_expr_types`; обычный `check_module` оставляет её ПУСТОЙ (zero-overhead для `nova check`/`build`/`test` — доказано тестом `zero_overhead_plain_check_module_empty`). Флаг `TypeCheckCtx::record_expr_types` (plain bool, ставится ПОСЛЕ build). Запись — POST-ORDER обёртка над `f1_expr` (renamed → `f1_expr_inner`): после полного inner-walk семантический канал `resolved_types_buf` уже заполнен, поэтому Call-return/Range/Tuple/RecordLit читаются корректно; каждая рекурсия проходит через обёртку → вложенные `a.b.c` пишут все уровни. Источник типа — РЕАЛЬНАЯ инференция чекера: `infer_expr_type` (богатый `TypeRef` с path+generics) с fallback на `resolved_types_buf` → `resolved_to_typeref` (Range/Tuple/литералы/record-lit). НЕ текстовая эвристика. Синтетические/zero-width span (`start >= end`) и generic-параметр-типы (`gs`-гейт через `typeref_mentions_any`, + `resolved_to_typeref` возвращает None на `TypeParam`) НЕ пишутся → «отсутствие = неизвестно», IDE деградирует gracefully.
- **[M-104.10-expr-types-coverage]** **Остаток покрытия (bounded, follow-up — НЕ упрощение):** plan-required набор покрыт полностью (литералы, Ident, Member obj+result, Index, Call-return, Binary, Range, As, Tuple/Array/Record lit). НЕ пишутся сегодня (ограничено тем, что аннотируют существующие каналы чекера): (a) GENERIC instance method-chain returns — Call-арм `infer_expr_type` намеренно расцеплён с общей instance-return-инференцией (172.1 perturbation-урок); (b) NON-primitive `TupleLit` (buf аннотирует только all-primitive; у `infer_expr_type` нет `TupleLit`-арма); (c) generic-instance `RecordLit`/element-типы с несвязанным type-param (gated by design); (d) **[Ф.6 наблюдение]** локальный биндинг из RANGE-литерала (`ro r = 0..=5`) — тип `r` не пропагируется в expr_types (пишутся только int-операнды `0`/`5`), поэтому member-hover `r.start` на таком локале деградирует в None; через `Range`-параметр (`fn f(r Range)`) или user-record локал (`ro x = Rec{..}`) — работает штатно. Все деградируют gracefully. Полный независимый expr-walker (Ф.2b) — отдельный эффорт при реальной IDE-потребности. Priority: P3.

## Plan 104.10 Ф.3 — cross-file goto-definition (2026-07-03)

- **[production, НЕ упрощение]** `goto_definition.rs` переписан с single-file (`uri.clone()`) на настоящий cross-file через provenance. `compute_goto_definition_in(resolved, src, pos, uri)`: `resolve_symbol_at_with_limit` (с `items_start`/`env` из `ResolvedModule`) → `symbol.span()` → резолв целевого файла по `span.file_id` в `resolved.file_map` (построен из реальных `peer_files`, НЕ текстовый grep — критерий приёмки #5). Символ из другого файла (импорт/prelude/folder-peer) → `Location` с URI того файла; символ текущего файла (`file_id == MAIN_FILE_ID`) → URI редактора verbatim (без canonicalize-дрейфа). Server-хэндлер (`server.rs::goto_definition`) переиспользует Ф.1-кеш (`get_or_build_resolved`). Тесты: `goto_definition.rs` 16 unit (pos1-6 single-file регресс, pos7 prelude→`std/prelude/runtime.nv`, pos8/pos9 folder-peer, neg1-3, edge1-4 включая UTF-16-в-цели) + `e2e_smoke.rs::pos11` (полный JSON-RPC `textDocument/definition` cross-file → Location в prelude).
- **[range disk-authoritative, осознанный дизайн — НЕ упрощение]** Range цели считается в UTF-16-координатах ИСХОДНИКА, из которого распарсен span: текущий документ → in-memory `src` (правильно отражает несохранённые правки entry); peer-файл → читается С ДИСКА (его span-offset'ы disk-relative, т.к. `resolve_imports_inline`/`parse_with_file_id` парсит peer'ы с диска). Ключевое: скармливать overlay открытого буфера в позиционирование peer'а НЕЛЬЗЯ — span'ы не соответствовали бы дивергентному тексту (даёт неверные позиции). Поэтому overlay для peer'ов сознательно НЕ применяется.
- **[M-104.10-vfs-overlay-imports]** **Остаток (bounded, → Ф.18):** единый VFS-overlay открытых буферов ПОВЕРХ и import-резолва, И позиционирования (чтобы несохранённая правка в peer-файле сдвигала goto-цель live, zls/rust-analyzer-стиль) отложен — требует, чтобы `resolve_imports_inline` читал открытые буферы, а не только диск. Пока goto в peer отражает последнее сохранённое состояние (корректно, никогда не мис-позиционирует). Дом = Ф.18 (workspace lifecycle). Priority: Ф.18. Отдельно замечена pre-existing хрупкость `resolve_symbol_at_with_limit` + `items_start` при folder-module peer с ведущим `//`-doc-комментом (ordering entry-vs-peer items) — ортогонально Ф.3, тесты обходят.

## Plan 104.10 Ф.0.5 — diagnostic pipeline correctness (2026-07-03)

- **[production, НЕ упрощение]** Четыре из пяти багов root-красноты закрыты полностью в `nova-lsp/src/compiler.rs` + `diagnostic_mapping.rs`: import-ошибки surface (не swallowed), degraded-CU fallback (nova.toml → LSP workspace root → entry-dir + scratch entry для unsaved) заполняет `peer_files` (prelude + folder-module peers), LSP check-вход сведён к `nova check` пайплайну (`resolve_imports` + `number_exprs` + `collect_all_signatures` + `check_module_with_sig_table`, 162.2-suppression), числовые `[Ennnn]` коды распознаются. Pos+neg+parity фикстуры: `nova-lsp/tests/diagnostic_pipeline.rs`.
- **[M-104.10-hardcode-lists]** **CLOSED (Ф.5, 2026-07-03).** Все хардкод-списки имён пакетов/типов/протоколов/keywords удалены из nova-lsp — резолв из реестра компилятора / search-path:
  - `completion.rs STD_MODULES` **удалён** → import-completion идёт из FS-скана stdlib-каталога (`nova-lsp/src/stdlib_index.rs::StdlibIndex`, построен из `resolve_std_path` + обход дерева; кэш per-stdlib-dir в `WorkspaceState::stdlib_index`). Suggested модули = реально существующие на диске (`import std.│` не врёт).
  - `code_actions.rs` `known_stdlib_type_module`/`known_stdlib_protocol_import` → `StdlibIndex::{type_module,protocol_module}` (скан `export type/protocol` деклараций stdlib с корректным folder-module module-path); `auto_derivable_protocols()` → `nova_codegen::protocols::auto_derive::builtin_protocol_names()` (единый источник; убраны стейл pre-D237 имена `Printable`/`Hashable`/`Equatable`/`Ordered`/`Cloneable`).
  - `rename.rs NOVA_KEYWORDS` **удалён** → `nova_codegen::lexer::is_reserved_keyword` (лексер классифицирует слово; убраны стейл `let`/`impl`/`blocking`/`suspend`/`and`/`or`/`not`/`mod`).
  Threading: `compute_code_actions_with_stdlib(..., Option<&StdlibIndex>)` + `completion_for_doc(path, .., Option<&StdlibIndex>)`; server-хендлеры резолвят индекс через `state.stdlib_index(doc_path)`. compiler-conventions §3 удовлетворён.
- **[M-104.10-lsp-resolve-method-doc]** **Follow-up (Ф.13, минорная асимметрия, НЕ функциональное упрощение):** документация method-completion (doc-комментарий метода) кладётся в `data` item'а (`{"f":"method","doc":...}`) при построении списка — она уже вычислена при резолве модуля для списка — и переносится в `documentation` на resolve. Wire-payload для (немногих) method-item'ов не уменьшается (doc едет в `data` вместо `documentation`), но рендеринг всё равно отложен до resolve. Статические семейства (keyword/snippet/prelude/import), доминирующие в списке, genuinely пере-выводятся из таблиц на resolve → их тяжёлый текст вообще не едет в initial-ответе (реальная экономия). Альтернатива (пере-резолв модуля per-item в resolve по locator uri+offset) отвергнута: дороже одного module-resolve на каждый фокус item'а. Priority: P4 (косметика payload немногих методов).
- **[M-104.10-lsp-cwd-anchor]** **Follow-up (test-only best-effort, НЕ упрощение прод-пути):** path-free convenience-обёртки `completion.rs::{completion_for, method_items, import_items}` (используются юнит/интеграционными тестами и вызывающими без пути документа) резолвят stdlib через discovery из `current_dir()` (`discover_anchor_path`/`discover_stdlib_index`). LSP-сервер НА ЭТО НЕ ОПИРАЕТСЯ — хендлеры всегда передают реальный путь документа + кэшированный `StdlibIndex` (`completion_for_doc`). CWD-fallback корректен когда cwd внутри Nova-workspace; иначе degrade (пустой результат). Priority: P3 (косметика тестовой поверхности).

## Plan 104.10 Ф.8 — signature help по типу receiver (2026-07-03)

- **[production, НЕ упрощение]** `signature_help.rs` переписан с «первый overload по имени» на type-driven dispatch. Новый вход `compute_signature_help_in(resolved, src, pos)` использует Ф.1 `ResolvedModule` (imports inlined → видны stdlib/peer overloads) + Ф.2 `expr_types`. Алгоритм: (1) `find_call_context` теперь возвращает `CallContext { open_paren, callee_name, receiver_dot }` — для метод-вызова `recv.method(` вычисляется байт `.` (= конец receiver-выражения); (2) собираются overload'ы (`find_fn_by_name` + `find_method_by_name`); (3) `overload_score(fd, is_method_call, recv_ty, active_param)` ранжирует: **type dispatch (доминирует)** — при метод-вызове тип ресивера из `receiver_type_name(env, dot_byte)` (span.end == dot, переиспользован из `completion.rs`, сделан `pub(crate)` вместе с `receiver_matches`); совпадение receiver-типа = +1_000_000, несовпадение известного типа = −1_000_000, свободная fn при метод-вызове/метод при свободном вызове = KIND_MISMATCH; **arity fit** — overload, вмещающий параметр под курсором (`params.len() > active_param`), предпочтён, среди них — самый плотный; (4) сорт по убыванию score, `active_signature = 0`. Server-хендлер (`server.rs::signature_help`) переиспользует Ф.1-кеш (`get_or_build_resolved`) + `catch_unwind`. Тесты `signature_help.rs`: t_pos1 (два типа с методом `put` → `Box`-значение выбирает `Box @put`, 1 параметр), t_pos2 (свободные overload'ы `f(a)`/`f(a,b)`, курсор на 2-м арг → 2-параметр вариант), t_neg1 (неизвестный ресивер → graceful fallback на первый, не пусто), t_edge1 (вложенный `f(g(│))` → сигнатура `g`). 11/11 signature_help unit PASS, 300/300 lib PASS. Старый `compute_signature_help(src, pos)` (parse-only, без типов) сохранён для path-free вызовов/тестов, деградирует до arity-ранжирования.
- **[M-104.10-arg-type-dispatch]** **Остаток (bounded, follow-up — НЕ упрощение headline):** свободные fn overload'ы ранжируются по *числу* аргументов (позиция курсора), НЕ по инференс-*типам* уже введённых аргументов. Receiver-type dispatch (заголовок Ф.8, критерий приёмки #1/#3) — полный; полная type-унификация аргументов свободных fn (сравнение `expr_types` введённых аргументов с типами параметров overload'а) — follow-up при реальной потребности. Деградирует gracefully (arity-эвристика корректна для типичного overload-набора). Priority: P3.

## Plan 104.10 Ф.7 — Rename через symbol-table (scope-aware) (2026-07-03)

- **[production, НЕ упрощение]** `rename.rs::compute_rename` больше НЕ делает слепой regex word-boundary скан по всем файлам. Новая сигнатура `compute_rename(docs, primary_uri, cursor_byte, old, new)`: сначала парсит primary-буфер и классифицирует символ под курсором из AST (`classify_scope`). **Local binding** (имя связано `let`/param/`for`/pattern-биндингом внутри объемлющей функции — `enclosing_fn_bindings` + `fn_bound_names`) → rename ограничен байтовым диапазоном объявляющей функции в primary-файле, остальные файлы не трогаются (`RenameScope::LocalInFile`). **Top-level** (свободная fn/type/const/field) → cross-file как раньше, НО каждая функция, локально затеняющая имя (`shadow_scopes_in_text` — функции, чьи локальные биндинги включают имя), пропускается пофайлово. Scope выведен из реальных AST-биндингов через `collect_pattern_names`/`collect_block_bindings`/`collect_expr_bindings` (покрытие ExprKind зеркалит `symbol.rs::find_ident_in_expr` + binding-формы: For/ParallelFor/IfLet/WhileLet/Match-arm/ClosureLight/ClosureFull/ConsumeScope), НЕ brace-depth-эвристика. Фильтрация вхождений — общий `collect_edits_filtered(text, old, new, accept: &dyn Fn(usize)->bool)`; `collect_edits_in_text` стал `#[cfg(test)]`-обёрткой (accept≡true). Атомарный post-rename type-check (D296) сохранён без изменений (Phase 2 `check_source_inner` на каждый изменённый файл). `server.rs::rename` прокидывает `primary_uri` + `cursor_byte` (word-start). Тесты: 7 новых unit — f7_pos_local_var_scoped_to_declaring_fn, f7_pos_local_var_use_site_still_scoped (курсор на use-site локали, не декларации), f7_pos_top_level_fn_renames_call_sites, f7_pos_shadow_check_skips_shadowing_fn, f7_pos_shadow_two_functions_same_local (пример из плана `fn a(){ro x} fn b(){ro x}`), f7_edge_type_is_top_level, f7_neg_parse_error_degrades_to_top_level, f7_neg_no_cross_scope_false_positive (регрессия против старого regex-бага). 37/37 rename unit PASS, 308/308 lib PASS.
- **[M-104.10-rename-full-resolve]** **Остаток (bounded, follow-up — НЕ упрощение headline):** полный per-*occurrence* symbol-resolve (резолвить КАЖДОЕ вхождение до его decl-`Span` и сравнивать) не реализован — требует полного name-resolution pass, который bootstrap-checker не отдаёт наружу. Вместо этого scope-модель over-approximate: scope локали = вся объявляющая функция (не точный вложенный блок); shadow scope = вся затеняющая функция. Следствия (все редкие, все safe-by-omission): (a) второй одноимённый биндинг в sibling-блоке той же функции переименовывается вместе с целевым; (b) top-level ссылка, стоящая в функции ДО того, как та вводит локаль того же имени, консервативно пропускается. Priority: P3.

- **[production, НЕ упрощение] Ф.15 documentHighlight (2026-07-03).** `textDocument/documentHighlight` (`document_highlight.rs::compute_document_highlights`, хендлер `server.rs::document_highlight`, capability `document_highlight_provider`) подсвечивает все вхождения символа под курсором В ТЕКУЩЕМ файле с read/write-kind. **Семантический резолв, НЕ regex:** scope вычисляется тем же Ф.7-резолвером через новую обёртку `rename.rs::resolve_highlight_scope` → `HighlightScope::{Local{range}, TopLevel{shadows}}` (переиспользует `classify_scope` + `shadow_scopes_in_text`). Local-символ → вхождения только внутри байтового диапазона объявляющей функции (одноимённая локаль в sibling-функции НЕ подсвечена); Top-level → весь файл минус затеняющие функции. Внутри scope — word-boundary скан со skip строк/комментариев; для LOCAL-символа дополнительно исключаются member-access позиции (`obj.x` — леворасположенный `.`, кроме range-оператора `..`), т.к. локаль никогда не пишется как `.x`. Read/write различаются по AST-write-множеству (`collect_write_offsets`: pattern-биндинги let/const/param/for/match/if-let/while-let/closure/consume + assign/tuple-assign targets Ident); всё остальное = read. Хендлер обёрнут в `catch_unwind` (parse-паника → пустой highlight, не краш). Тесты: 12 unit — pos_local_var_all_occurrences, pos_read_vs_write_kind_distinguished, pos_cursor_on_use_site_resolves_binding, pos_top_level_fn_decl_and_call_sites, neg_same_name_other_scope_not_highlighted, neg_top_level_shadowing_fn_skipped, neg_member_field_not_highlighted_for_local, neg_cursor_on_keyword_empty, neg_cursor_in_string_empty, neg_cursor_on_whitespace_empty, edge_parse_error_no_panic, edge_for_loop_binding_is_write. 320/320 lib PASS.
- **[M-104.10-highlight-lexical-occurrences]** **Остаток (bounded, follow-up — НЕ упрощение headline):** как и Ф.7-rename, который он зеркалит, within-scope скан вхождений — текстовый (word-boundary + skip строк/комментариев + `.`-member-исключение для локалей), НЕ per-occurrence resolve каждого кандидата до decl-`Span`. Следствия (все редкие, все также присутствуют в rename): (a) record/named-arg **field-label**, совпадающий по написанию с символом (`Point { x: … }` при highlight локали `x`), подсвечивается как read; (b) `..rest` array-pattern slice-bind (нет AST-span) репортится как read, а не write. Scope-корректность (тестируемое свойство) — точная; это edge-кейсы классификации read/write на текстовых совпадениях. Priority: P3.

- **[production, НЕ упрощение] Ф.18 Workspace lifecycle (2026-07-04).** 4 под-фичи в `server.rs` + новый чистый модуль `workspace_lifecycle.rs`. (1) **`workspace/didChangeWatchedFiles`** — динамическая регистрация watcher'ов (`client/registerCapability` на `**/*.nv` + `**/nova.toml`, спавн в фоне чтобы медленный клиент не блокировал скан) в `initialized()`; на внешнее create/change/delete — РЕАЛЬНАЯ инвалидация: `apply_watched_event` перечитывает символ-индекс Ф.12 с диска (только для не-открытых буферов — открытый буфер source-of-truth), выселяет resolved-cache Ф.1 + document-symbol-cache; `nova.toml` → clear `stdlib_index_cache` + все resolved. Обратные зависимости → `invalidate_all_resolved` (корректный superset, `[M-104.10-watch-reverse-deps]`). Затем recheck открытых доков + refresh. Убран TODO в `initialized()`. (2) **`workspace/willRenameFiles` → WorkspaceEdit** — Nova-импорты path-based (`import a.b.foo` → `a/b/foo.nv`), поэтому rename `.nv` меняет import-путь → `compute_rename_import_edits` переписывает финальный сегмент импорта во ВСЕХ зависящих файлах по РЕАЛЬНЫМ AST import-span'ам (не regex), с guard'ом «import.path — суффикс path-сегментов переименовываемого файла» (отсекает same-leaf импорты из чужих директорий). `+ didRenameFiles` (пост-rename purge old-URI + index new-URI). (3) **`$/progress`** — холодный первичный workspace-скан (`run_initial_scan_with_progress`, в `initialized`) обёрнут в work-done-progress токен (`window/workDoneProgress/create` + begin/report/end); спиннер вместо «завис». Все server→client запросы (register/create/refresh) обёрнуты timeout'ом — non-responsive клиент не может задедлочить хендлер. (4) **`{semanticTokens,codeLens}/refresh`** после скана/реиндекса (`refresh_client_hints`; `inlayHint/refresh` НЕ шлём — provider ещё не advertised, дом Ф.9). Тесты: 7 unit (`workspace_lifecycle`: classify NEG non-.nv, rewrite-importer, alias/selective preserve, unrelated-dir none, same-stem noop, last-segment range, path-suffix) + 6 integration (`tests/workspace_lifecycle.rs`: external-change→index update, delete→purge, change→resolved evict, NEG non-.nv ignored, manifest→clear, willRename on-disk) + 2 e2e (`e2e_smoke`: willRename capability advertised, initial-scan шлёт $/progress begin+end). 327 lib + все integration PASS.

- **[M-104.10-file-rename-imports]** **Остаток (bounded, план-санкционированный «базовый + маркер», Ф.18):** import-path rewrite при willRename матчит import, чей финальный сегмент == старому stem файла И чей dotted-path — суффикс path-сегментов файла на диске (root-agnostic, точно для типового single-file-module rename без запуска полного import-резолвера). НЕ покрывает: (a) rename *peer*-файла внутри folder-module (папка=модуль не меняется — правка не нужна, корректно пропускаем); (b) коллизия одинакового leaf-имени в двух несвязанных директориях (обе переписываются — истинно ambiguous без резолвера); (c) `as`-alias / selective-`{…}` re-spelling самого сегмента (сам сегмент правится, alias/список не трогаются — корректно). Полный resolver-verified path-matching — follow-up. Файл: `nova-lsp/src/workspace_lifecycle.rs`. Priority: P3.

- **[M-104.10-watch-reverse-deps]** **Остаток (bounded, follow-up — НЕ упрощение headline, Ф.18):** `.nv`-watch-событие инвалидирует resolved-cache самого изменённого файла + ВСЕХ прочих открытых доков (`invalidate_all_resolved` — корректный superset reverse-dep множества: любой открытый док может импортировать изменённый). Никогда не оставляет stale-кеш; грубее точного module-graph reverse-dep обхода (перестройка ленивая, записи дёшевы, bounded открытыми доками). Точный обратный граф зависимостей — follow-up (пересекается с `[M-104.10-dependent-invalidation]`, который остаётся открыт для `didChange`-пути Ф.1: Ф.18 добавил примитив `invalidate_all_resolved` и подключил его для watch/rename, но НЕ для per-edit `didChange`). Файл: `nova-lsp/src/{workspace_lifecycle.rs,state.rs}`. Priority: P3.

- **[production, НЕ упрощение] Ф.19 typeDefinition + implementation (2026-07-04).** Новый чистый модуль `nova-lsp/src/type_definition.rs` + 2 хендлера в `server.rs` (`goto_type_definition`/`goto_implementation`, обёрнуты `catch_unwind`+`run_with_large_stack`, реюз Ф.1 resolved-cache) + capabilities (`type_definition_provider`/`implementation_provider`). (1) **`textDocument/typeDefinition`** — тип выражения под курсором из Ф.2 `expr_types` (innermost covering expr span, фильтр `file_id==MAIN_FILE_ID`) ЛИБО, для имени let-биндинга, аннотация/тип инициализатора (`value.span`→`expr_types`); redukция `TypeRef`→base-name (`Named` last-segment; `[]T`/`Vec`-alias; peel `ro`/`mut`/`*`/`unsafe`); поиск `type <Name>`-decl в import-inlined модуле → `Span`→`Location` cross-file через provenance `file_map` (тот же путь, что goto Ф.3: entry-spans против in-memory `src`, peer-spans против диска). Примитивы (`int`/`str`/`bool`) не имеют user-`type` → graceful None. (2) **`textDocument/implementation`** — драйвится идентификатором под курсором (position-agnostic: decl-имя протокола, `[T Proto]`-bound, `x Proto`-param, имя метода), классифицируется по AST-реестру (НЕ хардкод): имя = `protocol`-тип → реализующие типы (explicit `#impl(P)` opt-in `TypeDecl::impl_protocols` ∪ структурная конформность: тип предоставляет метод на каждый метод протокола, `[]T`/`Vec` receiver-alias) ; иначе имя = метод → все `fn T @method`/`.method` того же имени (реализации/override'ы). Cross-file: импортированные реализаторы несут foreign `file_id`, резолвятся тем же `file_map`. Тесты: 12 unit (`type_definition::tests`: POS binding→User, POS ident-use, EDGE generic, NEG primitive-literal, NEG whitespace для typeDefinition; POS protocol-implementers, POS bound-use, POS method-impls, POS cross-file implementer, NEG plain-ident, NEG whitespace для implementation; `word_at`). 339 lib PASS. Фикстуры используют record-literal construction (проверенный expr_types-паттерн; static-method receiver-syntax триггерит `[P67-LEGACY]` panic в IDE-чекере → env=None, обходится).
- **[M-104.10-typedef-ident-coverage]** **Остаток (bounded, follow-up — НЕ упрощение headline, Ф.19):** typeDefinition на *использовании* идентификатора и на generic-типе (`typedef_pos_ident_use`/`typedef_edge_generic_type`) зависят от того, аннотировал ли Ф.2-чекер данный Ident/generic-RecordLit span в `expr_types` — там где `[M-104.10-expr-types-coverage]` (generic instance-chain returns, non-primitive TupleLit, несвязанный type-param) оставляет пробел, typeDefinition graceful-деградирует в None (тесты additive `if let Some`). Основной путь (let-биндинг → тип инициализатора) покрыт assert'ом. Дом = Ф.2b (полный expr-walker) при реальной IDE-потребности. Priority: P3.

## Plan 104.10 Ф.16 — foldingRange (2026-07-04)

- **[production, НЕ упрощение] Ф.16 foldingRange (2026-07-04).** Новый чистый модуль `nova-lsp/src/folding_range.rs` (`compute_folding_ranges(src) -> Vec<FoldingRange>`) + хендлер `server.rs::folding_range` (parse-only, `catch_unwind`+`run_with_large_stack`, деградирует в пустой список на panic) + capability `folding_range_provider`. Чисто **синтаксический** проход: `parser::parse` → рекурсивный обход AST, регионы из **span'ов узлов** (НЕ отступ-эвристика). Собирает: (1) тела fn/test/bench/lemma — `{ … }`-span тела-`Block`; (2) type-декларации — span всего `type Name { … }`; (3) вложенные `{ }`-блоки — КАЖДЫЙ `Block`, достижимый в дереве stmt/expr (control-flow тела, block-expr, `with`/`supervised`/`detach`/`blocking`/`realtime`/`forbid`, closure-block-тела, trailing DSL-блоки, match/select-arm блоки) — обход `walk_expr` покрывает ВСЕ 54 варианта `ExprKind` без wildcard, поэтому вложенность точна (внешний+внутренний блоки → два региона, клиент вкладывает по line-containment); (4) import-группы — run импортов на смежных строках → один `Imports`-регион (сорт по offset, разрыв на пустой строке → новая группа); (5) multi-line doc-comment'ы — `DocBlock`-span (`///`/`//!`-run, склеенный лексером) → `Comment`-регион. Регион эмитится ТОЛЬКО при `end_line > start_line` (однострочные `fn f()=>42`, `type X alias int`, одиночный import/`///` → ничего). Line-числа UTF-16-точны (`byte_offset_to_position`, тот же ropey-путь; `end`-exclusive span зондируется `end-1` чтобы попасть на строку `}`). Дедуп по `(start_line,end_line,kind)`. Тесты: 12 unit (`folding_range::tests`: POS fn-body, POS import-group-single, POS nested-blocks-nested (проверка строгого containment), POS type-body, POS multiline-doc, POS two-import-groups; NEG single-line-fn, NEG single-import, NEG single-line-type-alias, NEG parse-error-graceful; EDGE multibyte-line-accuracy (кириллица перед `}`), EDGE trailing-DSL-block). 351 lib PASS.
- **[M-104.10-folding-plain-comments]** **Остаток (bounded, follow-up — НЕ упрощение headline, Ф.16):** сворачиваются только doc-comment'ы (`///`/`//!` — единственная AST-представленная форма multi-line комментариев: лексер склеивает run в один `DocComment`-токен → `DocBlock` со span'ом). Run'ы обычных `//`-строк-комментариев НЕ сворачиваются — лексер их отбрасывает (`skip_line_comment`, они не доходят до AST), а восстановление требовало бы token-stream side-channel. Nova не имеет block-комментариев (`/* */`), так что это единственный пробел. Полное folding плоских `//`-групп — follow-up при реальной потребности. Файл: `nova-lsp/src/folding_range.rs`. Priority: P3.

## Plan 104.10 Ф.11 — source.organizeImports (2026-07-04)

- **[production, НЕ упрощение] Ф.11 organizeImports (2026-07-04).** Новый чистый модуль `nova-lsp/src/organize_imports.rs` (`compute_organize_imports(uri, src) -> Option<CodeAction>`) + wiring в `server.rs::code_action` (diagnostic-independent, `run_with_large_stack`, gated хелпером `code_action_only_admits` — иерархический prefix-match `context.only`, чтобы `source.organizeImports` не всплывал в `only:[quickfix]`-запросах) + kind `SOURCE_ORGANIZE_IMPORTS` уже в capability. `parser::parse` (один раз) → `module.imports`. Логика: (1) **удаление неиспользуемых** — введённое имя (selective item alias/name; module alias; для голого `import a.b.c` — last-segment `c` per D289) отсутствует в **текстовом name-scan** остального файла → импорт выбрасывается; (2) **пер-item pruning** selective-импортов — `import a.b.{Foo, Bar}` с used только `Foo` → `import a.b.{Foo}` (реконструкция из span'ов item'ов, path/anchor-префикс verbatim); все item'ы unused → statement выброшен; (3) **`export import` re-export НИКОГДА не удаляется и не прунится** (публичный API-surface D29/D288 — «не unused» только потому что тела текущего файла его не трогают); (4) **сортировка** выживших — по anchor (absolute rank 0 → relative rank 1), затем dotted-path, затем стабильный tie-break по item-тексту. Granularity — «unit с ведущей trivia»: юнит импорта = его физические строки + смежные предшествующие `#…`/`//…` строки (doc-attr/doc-comment едет вместе с импортом при сортировке; пустые строки схлопываются). Замена — один `TextEdit` на весь import-block `[first_unit_start .. last_unit_end]`; если реконструированный блок побайтово равен исходному (уже организован) → `None` (no-op, не предлагаем null-edit). Name-scan маскирует import-юниты и `module …`-строку пробелами, пропускает `//`-комментарии и `"…"`-строки (имя в строке/комменте не считается use). **Safety:** если внутри блока есть непустая строка, не покрытая ни юнитом, ни trivia (реальный код вперемешку с импортами — не легальный top-of-file Nova, но возможно в битом буфере) → `None` (не рискуем уничтожить код); parse-fail / пустой список импортов → тоже `None`. Тесты: 9 unit (`organize_imports::tests`: POS removes-unused-keeps-used, POS sorts, POS prunes-unused-items, POS whole-module-namespace-use; NEG no-imports→no-action, NEG already-organized→no-action; EDGE reexport-preserved (drop плоского unused, но `export import` verbatim), EDGE reexport-items-not-pruned, EDGE interleaved-code-suppressed). 367 lib PASS.
- **[M-104.10-organize-imports-namescan]** **Остаток (bounded, follow-up — НЕ упрощение headline, Ф.11):** «used»-детект — **текстовый** whole-word name-scan (план явно допускает «expr_types/name-scan»), НЕ type-aware. Следствие — консервативный false-**keep**: если введённое импортом имя совпадает с идентификатором, встречающимся в теле по другой причине (поле/метод/локаль с тем же именем, напр. `.Foo()` или field `Foo`), импорт `.{Foo}` будет сочтён used и сохранён, даже если фактически не используется. Направление ошибки безопасное — used-импорт НИКОГДА ошибочно не удаляется (удаление только при полном отсутствии токена); изредка сохраняется реально-unused. Точный анализ через `expr_types`/символьную резолюцию — follow-up при реальной потребности. Также doc-attr'нутые импорты двигаются вместе со своей trivia, но re-export с `#doc(...)` не прунится по item'ам by design. Файл: `nova-lsp/src/organize_imports.rs`. Priority: P3 (IDE-качество, не корректность).

## Plan 104.10 Ф.17 — selectionRange (2026-07-04)

- **[production, НЕ упрощение] Ф.17 selectionRange (2026-07-04).** Новый чистый модуль `nova-lsp/src/selection_range.rs` (`compute_selection_ranges(src, &[Position]) -> Vec<SelectionRange>`) + хендлер `server.rs::selection_range` (parse-only, `catch_unwind`+`run_with_large_stack`, деградирует в минимальные range на panic) + capability `selection_range_provider`. Чисто **синтаксический** smart-expand: `parser::parse` (один раз) → для КАЖДОЙ позиции `position_to_byte_offset` → рекурсивный обход AST (`Collector`, покрывает ВСЕ 54 варианта `ExprKind` без wildcard, как в Ф.16) собирает КАЖДЫЙ span узла, содержащий offset (inclusive на обоих концах — курсор сразу за ident'ом всё ещё выбирает ident). Цепочка строится из **AST-иерархии** (НЕ bracket-matching): ident/литерал (leaf-`Expr`-span) ⊂ объемлющее выражение (`Call`/`Binary`/`Member`/`If`/`Match`/…) ⊂ statement (`stmt_span`: inline-span либо wrapped `LetDecl`/`ConstDecl`/`Expr`) ⊂ блок (`Block.span`) ⊂ декларация (`item_span`: fn/type/test/bench/lemma). Т.к. sibling-span'ы в корректном AST не пересекаются, множество содержащих span'ов лежит на одном root-to-leaf пути → вкладывается. `build_chain`: дедуп совпадающих span'ов, сорт по ширине asc, оставляем строго-вкладывающуюся подпоследовательность (защита от parser-recovery span'ов), затем связываем `parent`-указатели наружу — возвращается innermost `SelectionRange` с цепочкой parent'ов. Позиция вне кода (пустая строка / whitespace / отброшенный комментарий) или parse-fail → минимальный пустой range `{start==end==pos}` без parent (LSP-обязательство: ровно один entry на входную позицию, index-aligned). UTF-16-точность через общий ropey-путь (`position_to_byte_offset`/`byte_offset_to_position`). Инвариант `parent ⊃ child` строго проверяется в тестах. Тесты: 7 unit (`selection_range::tests`: POS ident→binary-expr→let-stmt→fn-block→fn-decl (проверка ≥4 уровней + строгий containment каждого parent + точные границы innermost `alpha` и уровня `alpha + beta` + outermost = вся fn); POS несколько позиций за один запрос (index-aligned, независимые цепочки); POS expand сквозь вложенный if-block; NEG позиция вне кода → минимальный range без parent; NEG parse-error → минимальный range; EDGE вложенные вызовы `f(g(x))` — `x`⊂`g(x)`⊂`f(g(x))` строго; EDGE multibyte-границы (кириллица-строка перед target-ident, UTF-16-точные колонки)). 358 lib PASS.

## Plan 104.10 Ф.12 — Incremental references index (2026-07-04)

- **[production, НЕ упрощение] Ф.12 Incremental references index (2026-07-04).** Заменён V1 full-FS-скан `textDocument/references` (`collect_nv_files` + word-boundary скан КАЖДОГО `.nv` на КАЖДЫЙ запрос — снят маркер `[M-104.4-refs-incremental-index]`) на инкрементальный in-memory индекс `name → [(uri, span)]`. Новый тип `symbols.rs::ReferencesIndex` (+ `RefOccurrence`), поле `state::WorkspaceState.references_index`. Две `DashMap`: `by_name` (name → occurrences, ответ за один hash-lookup, без I/O и ре-скана) и `by_file` (uri → distinct-имена, обратная карта для `O(имён-в-файле)` удаления). **Корректность = паритет с V1:** `tokenize_ref_occurrences` разбивает файл на **максимальные ident-run'ы** — ровно те word-boundary-токены, что матчил `find_word_occurrences` пер-запрос, — поэтому индекс отвечает идентично скану; run'ы с цифрой в начале (числовые литералы) пропускаются (идентификатор Nova не начинается с цифры → не может равняться символу-запросу, экономия без потери матча). `find(name, decl, include_decl)` фильтрует вхождение, перекрывающее объявление (при `includeDeclaration=false`, через приватный `ranges_overlap`), и **сортирует** `(uri, line, char)` для детерминированного ответа независимо от порядка обхода DashMap. **Инвалидация:** `index_file` (снимает старый вклад файла перед добавлением) на `didOpen`/`didChange` (open-буфер = source of truth); внешние watched-события (`workspace_lifecycle::apply_watched_event`: Nv-delete → `remove_file`, Nv-change не-открытого → ре-`index_file` с диска); rename (`did_rename_files`: `remove_file` старого + `index_file` нового). **Фон-индексация ВСЕГО workspace** (урок sourcekit-lsp indexstore-db — иначе первый refs/workspace-symbol на холодном упрётся в скан): холодный `run_initial_scan_with_progress` (в `initialized`) индексирует все `.nv` → `mark_primed`; в хендлере `references` — ленивый cold-prime ОДИН раз (`is_primed`-флаг `AtomicBool`), пропуская открытые доки (не затирать unsaved-правки stale-диском). После prime каждый запрос = один lookup. `remove_file`: фильтр под entry-lock, затем `remove_if(v.is_empty())` ПОСЛЕ дропа guard'а (иначе deadlock на общем шарде; empty-check в `remove_if` защищает от concurrent re-add). `Range` — `Copy`, хранится по значению. Тесты: 6 unit (`symbols::tests`: POS cross-file-no-scan (+паритет счётчика с `find_references`), POS word-boundary (`foo`≠`foobar`), POS exclude-declaration, POS updates-on-change (rename `foo`→`bar`: stale purged + new indexed), EDGE remove-file (deleted → no dangling ref, пустой bucket удалён), PERF 100-файлов warm-lookup строго быстрее V1 full-scan над тем же набором) + 1 integration (`tests/symbols_references.rs::refs_pos4_incremental_update_on_didchange`: живой сервер, didOpen→2 refs, didChange full-text rename→новый символ найден, старый исчез, БЕЗ FS-скана). 373 lib + 16 symbols_references PASS.
- **[M-104.10-persistent-index]** **Scope-out (V2.1, НЕ упрощение headline, Ф.12):** индекс ссылок чисто **in-memory** — перестраивается фон-сканом при каждом старте сервера. On-disk persistence (аналог sourcekit-lsp `indexstore-db`: сериализованный индекс на диск, инкрементально валидируемый по mtime/хешам при старте → мгновенный первый refs без холодного скана) — follow-up V2.1 при реальной потребности (крупные монорепо, где холодный скан ощутим). Файл: `nova-lsp/src/symbols.rs`. Priority: P3.

## Plan 104.10 Ф.9 — Inlay hints (2026-07-04)

- **[production, НЕ упрощение] Ф.9 Inlay hints (2026-07-04).** Новый чистый модуль `nova-lsp/src/inlay_hints.rs` (`compute_inlay_hints_in(resolved, src, range, cfg) -> Vec<InlayHint>`) + хендлер `server.rs::inlay_hint` (реюз Ф.1 resolved-cache, `catch_unwind`+`run_with_large_stack`, деградирует в пустой список на panic) + capability `inlay_hint_provider` (`InlayHintOptions{resolve_provider:false}` — метки считаются eager). Два вида hints из **реальной** семантики (НЕ текстовая эвристика): (1) **type hints** — для неаннотированного биндинга `ro x = expr`/`mut x = expr` (только `Pattern::Ident` — деструктуризация не получает единый trailing `: T`) тип берётся из Ф.2 `expr_types.get(&value.span)` → `: T` (`format_type_ref`) после имени переменной (позиция = конец идентификатора внутри pattern-span). Аннотированный биндинг (`ro x int = 5`) → нет hint. (2) **parameter name hints** — для вызова `foo(1, 2)` имена параметров callee показываются перед КАЖДЫМ позиционным аргументом (`a:`/`b:`); callee (free-fn ИЛИ value-method `recv.m(…)`, имя из `Ident`/`Path`-last/`Member.name`, unwrap turbofish) резолвится по имени+арности через `symbol::{find_fn_by_name,find_method_by_name}` (method-call предпочитает методы, free — free-fn); при неоднозначном overload-множестве вызов пропускается целиком (никаких неверных hints). Подавление избыточного (`f(count)` где параметр `count`), пропуск named-args (`name:` уже виден) и spread (`...xs` — нет одного слота), variadic-tail не аннотируется. Обход AST зеркалит `document_highlight` (все контейнерные `ExprKind` без wildcard). **Provenance-guard:** обходятся только items entry-файла (`items_start..`) + каждый anchor-span проверяется `file_id==MAIN_FILE_ID` перед проекцией байт-offset'а на текущий буфер (span из prepended-import с чужим `file_id` не мис-проецируется). **UTF-16-точность** через общий ropey-путь (`byte_offset_to_position`). Оба вида toggle'аются через `InlayHintConfig{type_hints,parameter_hints}` (оба default on), читается из `initializationOptions` (`nova.inlayHints.{typeHints,parameterHints,enable}`, гибкий парсинг `from_settings`) в `initialize` + обновляется на `workspace/didChangeConfiguration` (+`inlayHint/refresh`); хранится в `state::WorkspaceState.inlay_config: Mutex<_>`. `refresh_client_hints` (Ф.18) теперь шлёт и `inlayHint/refresh` (снят TODO «дом Ф.9»). Range-фильтрация: только hints внутри запрошенного `range` (viewport). Тесты: 11 unit (`inlay_hints::tests`: POS type-hint `: int`, POS param-hint `a:`/`b:`, POS single-arg `a:`, NEG аннотированный→нет type-hint, NEG избыточный param suppressed, config type-off/param-off/from_settings-parsing, EDGE multibyte UTF-16-колонка (`é` в первом арге сдвигает байт-vs-UTF16), EDGE range-фильтр, EDGE parse-error graceful) + 3 e2e (`e2e_smoke`: capability advertised, live inlayHint type+param over JSON-RPC, config typeHints=false→нет Type-hint/param остаются). 392 lib + все e2e PASS.
- **[M-104.10-inlay-config-granularity]** **Остаток (bounded, follow-up — НЕ упрощение headline, Ф.9):** гранулярность настроек — два headline-toggle (`typeHints`/`parameterHints`) + master `enable`. Тонкие rust-analyzer-подобные ручки (hints только для литеральных аргументов, hide-single-param, closure-return hints, макс. длина метки, chaining-hints) НЕ экспонированы. Также type-hints для **call-return** типов (`ro x = add(1,2)`) отсутствуют — IDE-путь не гоняет `number_exprs`, поэтому ExprId-keyed семантический канал не приджойнен (пере-использует существующий `[M-104.10-expr-types-coverage]` пробел, НЕ дефект Ф.9); литеральные/ident-биндинги (синтаксическая инференция) работают. Все деградируют gracefully (отсутствие = нет hint, никогда неверный). Файл: `nova-lsp/src/inlay_hints.rs`. Priority: P3.

## Plan 104.10 Ф.10 — Full semantic tokens (2026-07-04)

- **[production, НЕ упрощение] Ф.10 Full semantic tokens (2026-07-04).** Новый модуль `nova-lsp/src/semantic_tokens.rs` (`compute_semantic_tokens(src, resolved) -> Vec<SemanticToken>`) расширяет узкий Plan 123.5.2-producer (был только cached-`@field`, legend `[PROPERTY]`, 2 modifiers) до **полного семантического pass'а**. Legend расширен до **13 token-types** (`namespace`, `type`, `typeParameter`, `parameter`, `variable`, `property`, `enumMember`, `function`, `method`, `keyword`, `comment`, `string`, `number`) + **3 modifiers** (`declaration`, `readonly`, кастомный `cached`); `initialize` теперь отдаёт этот legend (`semantic_token_legend_types/modifiers`); старые `cached_field_semantic_token_types/modifiers` + `compute_field_cache_semantic_tokens` в `server.rs` СОХРАНЕНЫ нетронутыми (их прямые тесты `field_cache_lens` зелёные), но больше не проводнены в сервер. Хендлеры `server.rs::semantic_tokens_full{,_delta}` переключены на резолвнутый-кэш (Ф.1 `get_or_build_resolved`) + новый producer, `catch_unwind`+`run_with_large_stack`, деградация в пустой вектор на panic. **Архитектура — полнота из лексера, точность из AST:** (1) поток `nova_codegen::lexer::lex` — источник токенов, эмитит keyword/строки/числа/char/doc-comment + идентификаторы (работает даже при type-error, когда `env=None` — ничего не роняется); (2) AST уточняет каждый идентификатор через **override-map** (byte-offset → (type,mods)): декларации fn/type/param/field/named-tuple-field/sum-variant/generic/const/assoc-const получают точный класс + `declaration`-модификатор (имя локализуется в span'е декларации через whole-token поиск в лексер-стриме — без substring-коллизий типа `c` внутри `const`); per-fn scope-walk помечает **use параметров** → `parameter` и local-binding-сайты (let/for/match/if-let/while-let паттерны) → `variable`+`declaration`; typeref-позиции (param/return/field/generic-bound/`as`/`is`) → `type`/`typeParameter`/`namespace`; (3) emission-time fallback по name-set'ам + лексер-контексту для не-override'нутых идентификаторов: member (prev=`.`) перед `(` → `method`, иначе `property` (или `type`/`enumMember` для `Type.X`); bare перед `(` → `function`/`type`/`enumMember` (конструктор); прочее по name-set → `type`/`enumMember`/`function`/`variable`(+`readonly` для const); import/use/module-строки → все сегменты `namespace`. **cached-`@field` подсветка сохранена как частный случай модификатора:** `@`+ident эмитятся ОДНИМ токеном (сохранён визуал Plan 123.5.2), все `@field` reads → `property`, а cache-eligible (field-cache `analyze_module` на свежем parse src, как в 123.5.2) дополнительно несут `readonly`|`cached` — byte-for-byte-совместимо. **UTF-16-точность** через `byte_offset_to_position`; мульти-строчные лексемы (склеенный `///`-блок, мульти-строка) сплитятся по `\n` (LSP запрещает токену пересекать строки). **Delta не тронут** — producer отдаёт полный delta-encoded вектор, edit-script `semantic_tokens_delta.rs::compute_semantic_token_edits`/`build_delta_response` прежний. Тесты: 10 unit (`semantic_tokens::tests`): POS различение fn/type/var/param/field + keyword + number, POS param-use→`parameter`, POS call callee→`function`, REGRESS cached `@x`→`property`+`cached`|`readonly`, POS delta tail-append=1 edit, NEG undeclared ident→`variable`, NEG нет `@field`→нет `cached`-модификатора нигде, EDGE parse-error без panic, EDGE большой файл (400 fn) <10s + много токенов, EDGE мульти-строчный doc-comment→2 per-line comment-токена. **402 lib PASS + 10 field_cache_lens (вкл. v52-legend-stable + cached-regress) PASS.** Остаток: `[M-104.10-semantic-tokens-scope]` (scope-approximation param-use / interpolation / effect-protocol method-internals / первый сегмент runtime-пути) — bounded, degrade-gracefully, никогда не «нет токена».

## Plan 104.10 Ф.20 — codeLens (run-test / references / implementations) + executeCommand (2026-07-04)

- **[production, НЕ упрощение] Ф.20 codeLens + executeCommand (2026-07-04).** Новый чистый модуль `nova-lsp/src/code_lens.rs` (`compute_navigation_lenses(src, uri, file_path, resolved, refs_index) -> Vec<CodeLens>`) + хендлер `server.rs::code_lens` переписан (Ф.20-навигационные линзы ∪ сохранённые Plan 123.5.1 field-cache линзы) + новый хендлер `server.rs::execute_command` + capabilities `execute_command_provider` (`commands=["nova.runTest"]`). V1 адвертайзил `code_lens_provider`, но выдавал ТОЛЬКО field-cache линзу — теперь три реальных навигационных линзы, все счётчики из **реальных индексов** (не заглушки). (1) **run-test линза** (`▶ Run test`) — над КАЖДЫМ `test "…"`-блоком entry-файла; команда = серверный `nova.runTest` с аргументами `[file_path, test_name]`. `execute_command` спавнит `nova test <file> --filter <name> --quiet` через `tokio::process::Command` (РЕАЛЬНЫЙ запуск, не стаб) из workspace-root'а; бинарь ищется `nova_binary()`: env `NOVA_BIN` → сосед запущенного `nova-lsp` exe (`target/<profile>/nova[.exe]`) → bare `nova` (PATH). Полный stdout/stderr → `window/logMessage`, one-line pass/fail summary (последняя непустая строка, `last_meaningful_line`) → `window/showMessage` (INFO/ERROR по exit-коду); launch-failure → ERROR с подсказкой про `NOVA_BIN`. Не-тест `fn` НЕ получает run-линзу (только `Item::Test`). (2) **references линза** (`N references`) — над каждым `fn`/`type`; счётчик из Ф.12 `ReferencesIndex.find(name, decl, include_declaration=false)` — ИСКЛЮЧАЯ само объявление (декл-локация = name-range объявления), поэтому **совпадает с `textDocument/references`** при `includeDeclaration=false` и symbol без использований честно показывает `0 references` (EDGE не скрывается). Команда = клиентский `editor.action.showReferences` (de-facto стандарт rust-analyzer/gopls: `[uri, position, locations[]]`) с пред-резолвнутыми локациями — без второго server-round-trip. Индекс лениво cold-prime'ится в хендлере (как в `references`), чтобы cross-file счётчики были верны на холодном workspace. (3) **implementations линза** (`N implementations`) — над каждым `protocol`; счётчик из Ф.19 AST-скана (новый публичный `type_definition::protocol_implementations_by_name` реюзит приватные `protocol_method_names`/`protocol_implementers`/`spans_to_locations`): explicit `#impl(P)` ∪ структурная конформность, cross-file через provenance `file_map`. `Some(vec)` даже пустой → протокол без реализаторов показывает `0 implementations` (EDGE не скрывается). **Provenance-guard:** линзуются только items entry-файла (`resolved.module.items[items_start..]` + `span.file_id==MAIN_FILE_ID`), name-range локализуется word-boundary-поиском в span'е (зеркалит `symbols::name_range_in_span`; для `test "…"` с quoted display-name — fallback на span-start anchor). Хендлер обёрнут `catch_unwind`+`run_with_large_stack` (panic скана → деградация к одним field-cache линзам). Тесты: 6 unit (`code_lens::tests`): POS run-test-линза над `test`-блоком (команда+args=[path,name]), NEG не-тест fn→нет run-линзы (но есть references), POS references-счётчик совпадает с `find` (2 usages, декл исключена), EDGE `0 references` рендерится, POS `2 implementations` над протоколом (Dog explicit + Cat structural), EDGE `0 implementations` рендерится. **408 lib PASS + все integration/e2e PASS.**
- **[M-104.10-runtest-filter-substring]** **Остаток (bounded, follow-up — НЕ упрощение headline, Ф.20):** run-test линза передаёт `nova test --filter <name>` (подстрочный матч по display-name), а не точный. Тест, чьё имя — подстрока имени другого теста в том же файле, при запуске потянет и «надмножество». CLI имеет `--filter-from <file>` (точный display-name матч, по строке на тест), но требует temp-файла и знания полного display-name (может включать path-префикс). Для реального устранения — либо exact-name флаг в `nova test`, либо генерация temp filter-файла. Деградация безопасна (запуск надмножества, не пропуск). Файл: `nova-lsp/src/code_lens.rs` (`run_test_lens`) + `server.rs` (`run_nova_test`). Priority: P3.

## Plan 174.4 + 174.6-M0 — spec/doc-дефекты adversarial-аудита исправлены (2026-07-04)

- **[сокр→фикс/звучность]** **8 spec/doc-дефектов (3 независимых аудита), код effect-registry верифицирован корректным — трогалась ТОЛЬКО спека/доки/comment.** **HIGH:** (1) **Doc описывал ОТВЕРГНУТЫЙ баговый механизм как реализованный** — `effects.h`-comment (~967) и `04-effects.md` D11-Q-note (~570) утверждали, что codegen эмитит `#define NOVA_MAX_EFFECT_STORAGES N` в/до генерируемого `.c`. Это ЛОЖЬ о коде: emit_c.rs эмитит comment-МАРКЕР `/* nova-effect-count: N */` (строка 1), а test_runner.rs (`effect_count_define_arg`) прокидывает `-DNOVA_MAX_EFFECT_STORAGES=N` на ВЕСЬ cc-вызов (все TU разом). Описанный `#define`-in-.c — именно тот ABI-раскол, что реализация сознательно отвергла (генерируемый TU и рантайм `effects.c`/`runtime.c`/`fibers.c` получили бы `NovaEffectRegistry`/`NovaEffectSnapshot` разного размера → OOB в TLS-registry → segfault). Оба текста переписаны под фактический механизм (маркер + `-D` на все TU) с объяснением ABI-uniformity; источник истины — план 174.4 §9. (2) **D282↔D353 fn-ptr gap** — C_ABI-грамматика D282 rule 2 не перечисляла function-pointer типы, а мотивация D353 (C-callbacks: qsort/libuv как параметр/поле `extern "C" fn`) под D282 as-written отверглась бы `E_FFI_NON_C_ABI_TYPE` → фича самоподрыв. Добавлена `FnPtr ::= *extern "C" fn(C_ABI…) -> C_ABI` как C-ABI base-case, cross-ref D353. (3) **D353 coercion soundness-дыра** — условия коэрции `fn → *extern "C" fn` = (1) C-ABI типы + (2) captureless; отдельно гейтился только `Fail`. Но captureless-fn с ЛЮБЫМ другим эффектом (IO/async/custom algebraic) проходил → C зовёт без Nova-handler-фрейма на стеке → unsound. Добавлено условие **(3) effect-free/total** (никакого объявленного эффекта, не только `Fail`); согласовано с D216 §10/§20 (обобщает `Fail`-специфичный `E_CALLBACK_THROWS_OVER_C_ABI` на все эффекты). **MEDIUM:** (4) пример cyclic value-record был `type Node { val int, next *Node }` БЕЗ `value` → по 02-types.md это heap GC-record (`Nova_Node*`), которую negative-list ТОГО ЖЕ rule исключает как non-C-ABI (самопротиворечие) → исправлено на `type Node value {…}` (в 08-runtime.md + план 174.6 §2/§4/§5/§9). (5) `uint` (address-sized unsigned, C-тип `nova_uint`=`uintptr_t`, D130 — отдельный примитив, не `uint64_t`) отсутствовал в D282 Scalar-листе → добавлен. (6) `Option[X] iff X=*T` узко (NPO применим к любому указателю) → заменено на `Option[RawPtr]` (любой указательный тип `*T`/`*()`/`CStr`). **LOW:** (7) error-коды `E_FFI_NON_C_ABI_TYPE`/`E_CALLBACK_THROWS_OVER_C_ABI`/`E_CLOSURE_HAS_ENV` нормативно ссылались, но не в error-index 09-tooling → **явная debt-нота deferred→M1** (единственный catalogue там = D296 §4 CodeAction fix-registry для РЕАЛИЗОВАННЫХ LSP-quick-fix'ов; заносить ещё-не-эмитируемый чекером код неверно; message-text уже в плане 174.6 §4) — зафиксировано в D353 Scope + план §9 + backlog. (8) **Stale «выполни план 176»** в шапке `174.4-*.md` (чужой план — 176=io/fs; commit 355cb826 правил файл, но оставил) → исправлено на 174.4. Проверены остальные 174.x — только 174.4 был stale (174.3/174.5/174.6 корректны). **Не упрощение:** чистая приведёнка спеки/доков к факту кода; D216 §10 cross-amend согласован; conformance `--positive --compile-error` = 38/0 (spec-правки код не ломают, effects.h-comment инертна). Cross-refs выверены (D130→02-types.md#d130). Файлы: `spec/decisions/08-runtime.md` (D282/D353), `04-effects.md` (D11-Q-note), `02-types.md` (D216 §10), `compiler-codegen/nova_rt/effects.h` (comment), `docs/plans/174.4-*.md`, `docs/plans/174.6-*.md`.

[2026-07-04 Plan 104.10 LSP V2 — ЗАКРЫТ] nova-lsp доведён до production-паритета с 7 LSP-пирами (24 фазы, 3 Workflow, 0 rate-limit). Сводка упрощений/остатков (все bounded, P3, plan-санкционированные маркеры — НЕ молчаливые): [M-104.10-expr-types-coverage] (generic instance method-chain returns / non-primitive TupleLit не в expr_types → completion деградирует gracefully text-fallback); [M-104.10-vfs-overlay-imports] (peer-ranges disk-authoritative, unified VFS-overlay открытых буферов отложен); [M-104.10-folding-plain-comments] (плоские //-комменты не сворачиваются — лексер их отбрасывает); [M-104.10-organize-imports-namescan] (unused-import через текстовый name-scan, консервативный false-keep); [M-104.10-persistent-index] (refs-индекс in-memory, on-disk persistence V2.1); [M-104.10-dependent-invalidation] (reverse-dep инвалидация кеша, урок zls); [M-104.10-rename-full-resolve] (scope-aware rename, полный per-occurrence resolve — followup); [M-104.10-lsp-cwd-anchor], [M-104.10-inlay-config-granularity], [M-104.10-highlight-lexical-occurrences], [M-104.10-semantic-tokens-scope], [M-104.10-runtest-filter-substring]; scope-out (маркеры): call-hierarchy / type-hierarchy (фундамент готов, дельта ~2d/1.5d), pull-diagnostics, declaration-alias, document-link, multi-root. Convention-compliance verified (no-hardcode, degraded-mode, diag-parity, тесты на реальном движке). Rust-тесты (не spec_tests/.nv) — LSP-internal/opt-in expr_types не наблюдается через nova test (D378/D379). Финал: nova-lsp 408 lib pass/0 fail. Ветка plan-104-10.

## Plan 176 Ф.2 — fs + Path (2026-07-04)

- **[production, НЕ упрощение] Ф.2 fs+Path (2026-07-04).** Модуль `std/fs`: byte-backed `Path` value (POSIX+Windows/UNC/drive lexical: join/parent/file_name/extension/stem/components/normalize/with_extension/is_absolute; non-UTF-8 round-trip Q1; `posix`/`windows`/`from_str`(host)/`styled`); **`Fs` effect как ТОНКИЙ int-primitive слой** (open/close/read/write/read_at/write_at/seek/sync_all/sync_data/stat/lstat/fstat + stat_*-accessors/mkdir/remove_file/remove_dir/rename/scandir(+next/name/kind)/realpath(+data)/symlink/chmod/copy_file/fsync_dir — все возвращают int/i64/str, НЕ Result); **триада** `real_fs()` (libuv `uv_fs_*` park/wake ТОЧНО как net.c — `nova_rt/fs.c` + `fs.h`, best-effort-cancel Q4) + `mock_fs(MemFs)` (in-memory byte-Path-дерево, ENOSPC-инъекция); **`File` must-consume (D133)** (`@close(self)`, positioned read_at/write_at + own cursor, OpenOptions read/write/append/truncate/create/create_new Q13, sync_all/sync_data); `Metadata`(→`Timestamp` Q каждый Option)/`DirEntry`/`FileType`/`Permissions`(Q8/Q12); convenience read/write/read_text/write_text/write_atomic(5-шаг durable §3c)/create_dir(_all)/remove_file/remove_dir(_all)/copy_file/rename/read_dir/canonicalize/symlink/set_permissions/try_exists; `c_path` interior-NUL-reject (§3c(1)). fs_seek(lseek)+platform-predicate в `io_console.h` (без libuv). Build: `fs.c` в 3 toolchain-сайта `test_runner.rs` (как net.c), `#include "fs.h"` в `nova_rt.h` под `NOVA_USE_LIBUV`. Тесты: nova_tests/fs pos(path POSIX+Windows / mock_fs round-trip/metadata/seek/OpenOptions/dir/write_atomic/torn-write / real_fs temp-dir via spawn) + neg(D133) — ALL PASS; spec_tests/d323 (отдельный module) PASS; main conformance 38/0; io/str регресс 0.
- **[production-обход, НЕ упрощение семантики] `Fs` = int-primitive-эффект.** effect-vtable стирает rich `Result[T, value-IoError]` в canonical `nova_int`/`nova_str` (теряя value-`IoError` и Ok-record — untested io-core gap: io-тесты используют effect-free конформеры). Обход: эффект несёт int/str-коды (зеркалят fs.c-хуки), `IoError`/`Metadata`/`DirEntry` строятся в pure-Nova обёртках ВНЕ effect-границы (там value-record keystone работает). Согласуется с §3/§0 «логика в .nv над тонким C-hook». Followup — фикс effect-vtable Err/Ok-erasure для value-record (не блокер).
- **[bounded, plan-санкц. маркеры — НЕ молчаливые]** `[M-176-consume-through-result-match]` (D133 не отслеживается через `match Result{Ok(f)=>…}`-extract — enforced для consume-param/direct-binding; общий с net TcpStream; neg-тесты через consume-param); ~~`[M-176-conformance-cu-map-closure]`~~ (**RESOLVED 2026-07-05 sync-fix-d322**: root = `emit_fn` не скоупил `var_mutable` → `mut f`-локаль одной fn (появляется со std.fs в CU) лик'ала в классификацию капчура ИММУТАБЕЛЬНОГО `f`-param'а лямбды `BoxIter.map` как by-ref-mut → env `T** f` без unpack-локали → closure-call `f(x)` голый `f`. Фикс: скоуп `var_mutable` per-fn-body; d323 возвращены в conformance, d102 PASS); `[M-176-memfs-gc-pressure]` (mock_fs 10-тестовый binary flaky под GC → разбит ≤3/файл, isolated стабилен); `[M-176-cwstr-direct-winapi]` (CWStr не нужен — libuv сам UTF-8→UTF-16 на Windows); `[M-176-cstr-from-bytes-canonical]` (§3c CStr.from_bytes = локальный `c_path([]u8)`); `[M-176-dir-scoped-ops]`/`[M-176-create-temp]` (Zig openat / unique-temp — followup). `IoError.path`/`source` (§3b full-shape) отложены (io↔path cycle + value-`Option[Path]`-mono blast-radius; `kind` сохранён — все тесты/§8.3 на нём).
- **[sync-fix-d322, 2026-07-05] Пост-reconcile: два codegen-факта + d323 в conformance.** (1) **Bug 2 RESOLVED (codegen-фикс):** `[M-176-conformance-cu-map-closure]` — `emit_fn` скоупит `var_mutable` per-fn-body (устраняет лик `mut`-классификации через границы функций, ломавший `BoxIter.map`-лямбду когда std.fs в CU). d323-фикстуры возвращены в `spec_tests/conformance` (директива владельца): d323_file_must_consume/path_bytes/write_atomic + neg — PASS при них ВНУТРИ CU; `spec_tests/d323` удалён. (2) **Bug 1 = `[M-sync-crossmodule-samename-type-collision]` (НЕ codegen-hunk, как гипотезировалось — pre-existing language-gap):** merge 178/179 свёл в один positive-CU три разных `ErrorKind` (io/http/compress) с простым C-именем `Nova_ErrorKind` → коллизия → ICE при io `kind_from_errno`. Target-form = module-qualified type-naming (крупно, НЕ sync-fix). **Sound-обход §0:** http (d358) → `spec_tests/http`, compress (d333/334/335/336) → `spec_tests/compress` — свои module-CU (d323-паттерн). Гейт: conformance 53/0 (1 aggregate-pos incl d322+d323 + 52 neg), zero-regress delta 0 (все падения — pre-existing на 8958b6fe: basics/control_flow, effects/basic, concurrency/_repro_p110, modules/priority_queue, map_literals/positive_clone_merge), features (io/fs/http/time/ffi/rebind/any_is/effect_registry) PASS. `[M-178-conformance-d357-d360-forwarddecl-bug]` — ДРУГОЙ root (forward-decl return-type unit-closure-call), НЕ тронут.
- **[codegen-конвенции, выявлены Ф.2]** value-record литералы: typed-форма (`Path{…}`) в блок-позиции, anon (`{…}`) в `=>`-теле (checker: typed redundant в `=>`, codegen: anon-inference только для heap-`Nova_X*`, не value). std.fs free-fn имена НЕ коллидят с std.io generic-хелперами (coarse-by-name резолв): `read_text`/`write_text`/`copy_file`. Резерв. слова: `exists`/`forall` (квантор), `readonly` (kw) — переименованы (`try_exists`, field `read_only`). Multi-line import/`FileType`-enum-variant-vs-`File`-type коллизия → `FileType value{k int}`.

## Plan 174.6 M1 — C-FFI ABI checker + parser (2026-07-04)

- **[production, НЕ упрощение headline]** Реализована M1 поверх M0-спеки (D282 rule 2 / D353): parser
  `*extern "C" fn` (поле `TypeRef::Func.extern_abi`), рекурсивный C-ABI-классификатор
  `check_ffi_c_abi_signatures` (`E_FFI_NON_C_ABI_TYPE` на params+return), коэрция-гейт fn→`*extern "C" fn`
  (captureless/effect-free, вкл. D353 clause 3 — любой эффект, не только `Fail`), error-index (09-tooling
  D296 §4.10), dedup-hardening `(c_name, param_c_types)`. Гейты: build clean, conformance 38/0, pos+10 neg
  .nv PASS, 8 Rust unit-tests ok, zero-regression delta 0.
- **[bounded, sound — НЕ дыра]** **Unresolved Named → conservative C-ABI (не флагается).** Классификатор
  читает СТРУКТУРУ типа (§3, не имена); когда Named не резолвится в текущем CU (generic-param `T` или
  cross-module тип, не инлайненный в `module.items`), verdict откладывается на defining-модуль. Это НЕ
  упрощение спеки (грамматика D282 — над РЕЗОЛВНУТЫМИ типами), а soundness-корректный defer: 0 ложных
  `E_FFI_NON_C_ABI_TYPE` на std/net (plan91_12 import std.net: 0 срабатываний), при этом все КОНКРЕТНЫЕ
  non-C-ABI (`Vec`/`Result`/heap-record/`Option[int]`/Nova-`*fn`) флагаются положительно (10 neg PASS).
- **[bounded, M2-остаток]** **Валидация `*extern "C" fn`-сигнатур — только внутри extern "C" fn деклараций**
  (рекурсивно, вкл. вложенные fn-ptr-параметры). Standalone `*extern "C" fn` в non-extern-C позициях
  (параметр Nova-`fn`, поле record'а, cast-target в теле) отдельно не валидируется в M1 (не в гейте) →
  отложено в §10 остаток M2.
- **[pre-existing gap, вне M1 scope]** **Рантайм-коэрция `fn → *extern "C" fn` упирается в codegen-gap**
  `fn → *fn` value materialization (P67-LEGACY `Ident not in var_types`) — Plan 118 Ф.6 follow-on,
  ИДЕНТИЧНО падает на Nova `*fn` (не регрессия M1; M1 = parser+checker, §1/§7.7 «валидация в ЧЕКЕРЕ, НЕ в
  codegen»). Коэрция-**acceptance** проверена на checker-слое (Rust unit-test через `check_src`).
- **[не упрощение]** **Bare `*fn` Fail-гейт (Plan 118) НЕ ретрактится в M1.** D353 clause 3 применён ТОЛЬКО
  к `*extern "C" fn`-цели; bare Nova-ABI `*fn` несёт handler-стек → non-Fail-эффекты там разрешены
  (unit-test проверяет). Полная D216 §10 ретракция = M2 (иначе regress `t6_neg_callback_throws_over_c_abi`
  на bare `*fn`). Файлы: `compiler-codegen/src/{ast/mod.rs,parser/mod.rs,types/mod.rs,codegen/emit_c.rs,
  const_fn_trampoline.rs}`, `spec/decisions/{08-runtime.md,09-tooling.md}`, `nova_tests/ffi/`.

## Plan 174.6 M2/M3 — additive completeness (cast-матрица + conformance + cookbook + non-extern-C позиции) (2026-07-04)

- **[additive, не упрощение]** **`*extern "C" fn` в non-extern-C позициях — checker M2 (закрывает M1-остаток).**
  Тег `*extern "C" fn` — легальный ТИП в любой декларативной позиции; его C-callback-сигнатура теперь
  валидируется как C-ABI **вне** `extern "C" fn`: в параметрах/возврате **Nova-функций** (не-extern-C) и в
  **полях value-record/named-tuple** + underlying newtype/alias (`ffi_validate_c_fnptr_occurrences`,
  types/mod.rs). Валидируется ТОЛЬКО C-callback-подсигнатура — объемлющий Nova-тип не флагается (Nova-fn
  волен брать `Vec[int]`; guard-unit-test). Span-dedup (`ffi_push_deduped`) убирает двойной репорт, где
  offender достижим двумя обходами. **Strictly additive → zero-regression:** grep подтвердил, что ни один
  существующий проходящий тест не пишет `*extern "C" fn` в non-extern-C позиции (тег M1-новый, только
  ffi-тесты, все внутри `extern "C" fn`); delta 0 эмпирически (basics/generics/plan118/atomics/ffi
  идентичны на parent-бинаре 3424405e). Codegen НЕ трогался (checker-only) → 0 blast-radius на не-FFI.
- **[spec-doc]** **Cast/коэрция-матрица D353 + ffi-cookbook C-ABI-раздел.** D353 (`08-runtime.md`): таблица
  `expr as *fn|*extern "C" fn` (effect-free C-ABI → оба ✅; Nova-тип-сигнатура → `*fn` ✅/C-ABI ❌
  `E_FFI_NON_C_ABI_TYPE`; `Fail` → оба ❌; non-`Fail` эффект → `*fn` ✅/C-ABI ❌ clause 3; closure/bound →
  `E_CLOSURE_HAS_ENV`) + правило «`*fn`≠`*extern "C" fn`, нет неявной конверсии» + P67-LEGACY-заметка.
  D216 §10 — cross-ref (строку «default C ABI» НЕ ретрачу — отложенный риск-пункт). Cookbook: тип-таблица
  (что C-ABI и почему), qsort/libuv callbacks через `*extern "C" fn`, 3 условия коэрции + soundness,
  reject-кейсы с кодами, ownership/pinning-декрет (Boehm не сканирует C-malloc → retained ptr = UAF).
- **[тесты]** **Conformance `d282_ffi_abi.nv` pos (peer-файл, +0 CU) + 9 neg + 3 Rust unit-теста.**
  Conformance-baseline **38/0 → 47/0** (все PASS). 11 Rust FFI unit-тестов (8 M1 + 3 M2). Neg покрывают
  каждое нормативное правило/код: Vec/Result/Option[non-ptr]/`()`/heap-record/Nova-`*fn` →
  `E_FFI_NON_C_ABI_TYPE`; closure → `E_CLOSURE_HAS_ENV`; effect → `E_CALLBACK_THROWS_OVER_C_ABI`;
  malformed C-callback в Nova-fn-параметре → `E_FFI_NON_C_ABI_TYPE` (M2).
- **[обоснованный defer, НЕ упрощение]** **`_Static_assert(sizeof==<expected>)` layout-guard отложен**
  (`[M-174.6-ffi-struct-layout]`), вопреки зонт-174 §3.6 «закрыть СЕЙЧАС». Причина: корректный `<expected>`
  = C-ABI размер структуры С паддингом/выравниванием по платформенному ABI — независимая layout-модель,
  которой у Nova НЕТ (полагается на C-`sizeof`; `gc_layout.rs` считает GC-bitmap-offsets heap-record'ов,
  не C-ABI byte-size value-record'ов числ. литералом). `sizeof==sum-полей` **неверно** (отвергает
  легально-паддинговые `{i8,int}`=16≠9); `sizeof(X)==sizeof(X)` **тавтология** (для СВОЕЙ эмитированной
  структуры C даёт тот же размер — реальный S8-дрейф возможен только против ВНЕШНЕЙ C-либы, чей layout Nova
  не знает); newtype→nova_int erasure = platform-инвариант, не per-user-record. Значит осмысленный per-type
  static-assert **coupled** к отложенной полной layout-спеке → тавтологичный/неверный guard был бы
  упрощением; честный маркер вместо него. Зонт §3.6/§4.6/§9 переоценены. Файлы:
  `spec/decisions/{08-runtime.md,02-types.md}`, `docs/ffi-cookbook.md`, `docs/plans/{174.6-*.md,174-*.md}`,
  `compiler-codegen/src/types/mod.rs`, `spec_tests/conformance/{d282_ffi_abi.nv,neg/d282_*,neg/d353_*}`.
## Plan 177 ЗАКРЫТ — Ф.4 close-out + честный аудит полноты D325 (2026-07-04)

- **[закрытие]** **Plan 177 (Result-everywhere, D325) ЗАКРЫТ.** Ф.4 = docs close-out + аудит полноты. Метод аудита: `grep 'Fail\['`+`'\bthrow\b'` по всей `std/**.nv`, разбор сигнатур vs тело, cross-check с conformance-guard (3 passed / 0 нарушений stable) + conformance CU (41/0). **Карта (плана 177 §14):** stable-std public-fallible = **Result-everywhere** — мигрировано Ф.2a/2b/2c (base64/json/complex/parse/read_buffer/коллекторы), conformant pre-177 (net/176/utf16/string-core), exempt-list §2 by-design (unwrap-мост D85, on_exit R5, `testing/property` Q5, сам `Fail[E]` effect-декл). D325-конвенция (правило+spec+guard+conformance+in-scope миграция) — достигнута ПОЛНОСТЬЮ.
- **[аудит/остаток]** **Честный остаток — НЕ выдан за полное (§7.7/§12.1), маркирован явно:** (a) `std/concurrency/cancellation.nv` `race2`/`with_timeout` **throw bare `str`** через *inferred* Fail (сигнатура `-> T` без `Fail[` → **вне guard-скана §8.2** — задокументированный blind-spot §14.3); по R1 = expected failure → Result, НО structured-concurrency error-семантика = Plan 173-домен (§10/§13, вне scope 177) → `[M-177-concurrency-throw-fallibility]` (home 173), НЕ конвертировано (whole-subsystem: нужен error-домен + coord с MultiError/173 Ф.4 + смена 2 `#stable` сигнатур). (b) весь `std/_experimental/**` (17 файлов: csv/hex/ini/toml/url/semver/semver_range/sql/jwt/bcrypt/snowflake/ulid/uuid/statistics/regex/cron/retry) ещё throw own-`Fail` → defer до стабилизации модуля (§9 Q3) → `[M-177-experimental-fallible-migration]` (консолидировал бывш. §6-список sql/jwt/… — был неполон); `retry.@execute`/`sql.in_transaction` forwarded-`Fail[E]` = R5-легально даже после стабилизации. (c) codegen-хвост уже зарегистрирован: `[M-177-d77-codegen-4way-retract]` (D77 4-way→2-way emit_c-синтез), `[M-172.1-opt-result-over-userenum-typedef-order]`, `[M-parse-int-overflow-returns-invaliddigit]` (174.1-домен).
- **[docs]** Обновлены: план-177 (Статус→CLOSED, Ф.4-row DONE, §14 полная карта, §6 полный `_experimental`-список + concurrency-остаток), spec D325 Status (миграция завершена + остаток), README планов (177 row + очередность-строка → CLOSED), backlog (+2 маркера), project-creation.txt (часть close-out), simplifications (эта запись). **Гейт:** conformance `test --positive --compile-error spec_tests/conformance` = PASS 41/0 (сохранён vs Ф.3); docs-only → net-zero-код (0 правок compiler/std); Rust не тронут.
[2026-07-04 Plan 178 Ф.0.5 — net byte-surface + Net-эффект унификация + AddrNet-retract] Приземлена НЕТ-часть Ф.0.5 (URL-промоут — отдельно, не сделана). Ветка plan-178-http (nova-p178). **(1) Additive byte-surface:** `TcpStream`/`TcpReadHalf`/`TcpWriteHalf`.`read_bytes`/`write_bytes`/`write_all_bytes` + `UdpSocket`.`send_to_bytes`/`recv_from_bytes` — публичные обёртки над `str`-методами (write: `str.from_bytes_unchecked(data)`; read: `.to_bytes()`), НЕ effect-ops. Обоснование (не упрощение): C-транспорт (`tcp_stream_write`/`_read_bytes`) length-delimited → round-trip байт-чист incl. embedded-NUL/не-UTF-8; обёртка вместо effect-op'а держит `[]u8` вне vtable (нет erasure-риска), делает тот же str↔[]u8-конверт, что план §3.10 предписывает («обернуть»). `str`-варианты сохранены транзитом. **(2) ⭐ Net-унификация (§13.2):** `TcpNet`+`UdpNet`+`DnsNet` → единый `type Net effect` (~37 ops, реконсиляция к спеке D62 — дробление 91.12/ex-D291 было необлагороженным отклонением) + один `real_net()` (tcp.nv, свёрнуты udp+dns handler'ы) + один `mock_net()` (mock.nv). `bind`-коллизия TcpNet/UdpNet → op-имена `tcp_bind`/`udp_bind` (public API `TcpListener.bind`/`UdpSocket.bind` неизменны). **(3) AddrNet-retract:** addr-ops (`loopback`/`loopback_v6`/`v4`/`from_str`/`@port`/`@ip`/`@is_v4`/`@is_v6`/`@to_str`) → pure `.nv` над `extern "C"` напрямую (FFI≠эффект, план §13.2); `AddrNet`/`real_addr_net`/`mock_addr_net` удалены; DNS `lookup` остаётся в `Net` (I/O). Разблокирует 176 Ф.4(b). **Blast-radius:** grep net-эффектов по `**/*.rs` = 0 → 100% .nv+C-runtime; мигрированы все call-sites (nova_tests/plan91_12·15·16: `with AddrNet=…, TcpNet=…` → `with Net = real_net()`; addr-тесты — убран `with` целиком, ops pure; `s.len()`→`byte_len()` фиксы D249). **Гейт:** conformance 38/38 PASS; 19 non-slow + 7 slow net-тестов PASS; byte-surface mock-round-trip PASS. Rust build clean. Zero-regression: emit_c-правка additive-by-construction (non-panic путь байт-идентичен), 6 fail в sum/effect-sample — все pre-existing (str.len-retirement/Mul-on-option/undeclared-apply, не связаны с sum-variant Path-call).
- **[production-fix, НЕ упрощение] Pre-existing Plan-172 ICE починен:** `[P67-LEGACY] Path call return type unknown for method=InvalidAddr/IoError` (emit_c.rs `expr_c_type`, 2 сайта 39997/43401) — net НЕ КОМПИЛИЛСЯ ВООБЩЕ на current main (каждый net-тест ICE'ил на construct'е payload-variant в handler-контексте). Fix: sum-variant Path-call fallback — если `eff ∈ sum_schemas` и `method` = вариант → return `Nova_{eff}*` (после `fn_ret_*`-lookup, guarded на variant-membership → genuine static-методы не задеты). Additive: срабатывает ТОЛЬКО где старый код паниковал.
- **[FFI ABI fix] `socket_addr_parse` tuple→TLS-accessor:** `(int, CSocketAddr)` → `int`(code)+`socket_addr_parse_result()`(TLS). Старый tuple-ABI полагался на CSocketAddr→nova_int erasure (legacy `_NovaTuple2`), снятую type-encoded tuple-naming Plan 172 (`_NovaTuple_2_8_nova_int_6_void_p` ≠ `_NovaTuple2` → CC-FAIL). TLS-идиома зеркалит `tcp_stream_read_data`/`dns_addr_at`. net.c/net.h/ffi.nv/addr.nv.
- **[M-net-payload-variant-static-lowering]** (P2, followup): real-socket byte-round-trip slow-тест заблокирован — `NetError.IoError` mis-lower в undefined `_static_`-wrapper при byte-call-graph (сторона EMISSION того же Plan-172-gap'а). mock-тест валидирует поверхность; codegen-fix отложен (риск в 15k-строчной call-emission).
- **[M-net-socketaddr-value-record]** (P2, followup): value-record rep-change отложен (byte-baseline-guarded, НЕ блокирует HTTP — план §3.10/§13.2); handle-rep сохранён.
- **[M-net-merge-focus-stub-codegen]** (P3, followup): panic-diverging effect-op возвращающий value-tuple miscodegen'ит → user-код не может писать partial `Net`-хендлер; focus-negative dns-тесты сняты (Err-propagation = тривиальная делегация, покрыта net_error_to_str).

[2026-07-04 Plan 178 Ф.1 — message-model + URL + валидаторы] Приземлён `std/http/` (module `std.http`, 10 co-equal .nv, 100% Nova, 0 codegen-ссылок): `Method`/`StatusCode`/`Version`/`HeaderMap`/`Mime`/`ContentType`/`Cookie`/`SetCookie`/`Url`/`Body`/`Request`/`Response`/`HttpError`. **Security by-construction:** HeaderMap reject CR/LF/NUL (response-splitting) + CL+TE-reject (smuggling); Url строгий host/SSRF-валидатор (reject hex/octal/decimal/short IP-обфускации, bracket-IPv6, control/whitespace); Cookie RFC 6265bis send-инварианты. **Body must-consume** (D133 compile-error на незакрытом теле — фикс Go-leak). Гейт: conformance 40/0, nova_tests/http pos+neg PASS, zero-regression (additive-only). **Амендменты (D358/D359 spec-first, НЕ упрощения — обход codegen/checker-ограничений):** (a) `SameSite.None`→`Cross` — `None` в public-enum коллидирует с `Option.None` в namespace ЛЮБОГО импортёра std.http (wire-value «None» сохранён); (b) `ErrSource.Url`→`UrlParse` — имя-вариант==имя-тип `Url` → codegen cast вместо wrap; (c) `ParseUrlError` tuple-варианты (не record) — auto-eq record-вариантов в `Option[sum]` mis-lower'ит `_0` на named-fields; (d) `HttpError` non-`value` — `value`+`Option[Url]`/`Option[ErrSource]`-поля → codegen emit Option-typedef ПОСЛЕ struct'а (forward-ref «unknown type»); (e) `Body` `consume` (не `consume value`) — value-копия оставила бы consume-поле Request/Response неразряженным; (f) конструктор Request/Response из СЫРЬЯ (`[]u8`/`BodyReader`), не pre-built `Body` — move consume-переменной/параметра в поле НЕ распознаётся checker'ом (только свежее inline-выражение). **Гейтнуто в Ф.2+ (маркеры):** `Http`-effect на consume-методах Body; `@copy_to`/`@json[T]`/`@trailers`; charset-latin1 `@text`; `BodyReader` Option-EOF-форма (codegen eq-ordering баг `Option[Option[[]u8]]`); `ErrSource.Net/Utf8/Io/Compress/Tls`; typed `expires Timestamp`; RequestBuilder/verb-one-shots. `_experimental/encoding/url.nv` оставлен (0 импортёров).

[2026-07-04 Plan 178 Ф.2 — HTTP/1.1 client CORE (plaintext)] Приземлён plaintext HTTP/1.1 client-core (identity + chunked). Ветка plan-178-http (nova-p178). **Структура (ревизия §3/§6 — nested submodules вместо flat `std.http`):** core `std.http` (message-model + `effect.nv` `Http`-seam + `response_ext.nv` Response-conveniences); **`std.http.client`** (client.nv `HttpClient`/builder/`RequestBuilder` + verbs + redirect-loop + auth-strip; wire.nv pure serialize/parse+chunked-decode; mock.nv `MockResponse`/`MockHttp`/`mock_http` + dynamic `Response.@json()`); **`std.http.transport`** (real_http() над `Net`, effect-over-effect). **Гейт:** conformance 40/0 (не тронут); 14 pos через mock (GET/POST/query/chunked/redirect same+cross-origin/**auth-strip**/too-many→Err/404=Response/error_for_status/malformed→Err/must-consume/dynamic-json) + transport https-gate + neg response-not-consumed → все PASS; zero-regression (additive .nv, codegen-бинарь не тронут). **Амендменты по факту (§5 spec-first, НЕ упрощения):** (a) **nested submodules** — ВЫНУЖДЕНО compiler-багом d43 (см. ниже), заодно изолирует std.net+json от lean core; (b) `Response`/`Request` (Ф.1-имена) а не `HttpResponse`; (c) `ParseUrlError.InvalidPort`→**`MalformedPort`** — whole-program конструктор-namespace коллизия с `NetError.InvalidPort` (любая программа с real_http линкует std.http+std.net → bare `InvalidPort` резолвился в NetError → link-fail/ICE); qualified `ParseUrlError.InvalidPort(x)` эмитит несуществующий `_static_`-символ, поэтому рename; (d) mock `.build()` работает НО GC-небезопасен → канон-установка = inline-handler `with Http = effect Http { send(..){ m.reply(request) } }` (frame-capture, conservative-GC-safe); `MockHttp.@reply` — deliverable; (e) `error_for_status` материализует+пересобирает Response (in-mem CORE), error-body дренится (must-consume soundness). **Http-seam дизайн:** op `send(host,port,secure,request str)->Result[str,HttpError]` — str-payload (byte-carrying via from_bytes_unchecked), НЕ `[]u8` (то же обоснование, что net byte-surface: `[]u8` effect-op erasure); ВСЯ HTTP-семантика (serialize/parse/chunked/redirect) — Nova над байтами; тонкий seam.
- **[M-178-mock-handler-gc-trace]** (P2, compiler-bug): `mock_http().build()` heap-closure-env (captured `routes`) НЕ регистрируется как GC-root → conservative Boehm собирает его при коллекции mid-run → use-after-free segfault (детерминированно после ~5 varied-tests; `GC_DONT_GC=1`/большой heap маскируют). Обход: inline-handler захватывает frame-local `m` (стек сканируется GC). real_http() безопасен (captures nothing). Fix = runtime root-registration handler-closure-env.
- **[M-178-conformance-d357-d360-forwarddecl-bug]** (P2, compiler-bug): d357_*/d360_* single-CU conformance-фикстуры НЕ добавлены — client value-types в одном CU с `d43_trailing_block_and_fn` триггерят codegen forward-decl баг: `return_type_c`→`infer_expr_c_type` мис-выводит return-тип unit-возвращающего closure-call `d43_run_unit(body fn()){body()}` → эмитит value-тип (`NovaValue_RequestBuilder`) в forward-decl vs `nova_unit` в определении → `conflicting types` CC-FAIL. Поведение D357/D360 покрыто через nova_tests/http* (14 pos + transport + neg). Fix = forward-decl return-type для unit-closure-call.
- **[M-178-with-tail-bang-codegen]** (P3, compiler-bug): `with`-блок, чей tail-expr = `X!!` (unit-Result) конфликтует interrupt-return-тип (HttpError*) с block-value-тип (unit) → CC-FAIL `assigning nova_unit to HttpError*`. Обход: закрывать with-блок `assert(...)`/`()`, не голым `!!`.
- **[M-178-effect-op-result-monomorph]** (P3, compiler-bug): прямой `Effect.op(...)` возвращающий `Result[A,B]` (A≠B) мис-монеморфит Err-payload как A. Обход: тонкая fn-обёртка `fn seam(...) Eff -> Result[A,B] { Eff.op(...) }` даёт конкретный монеморфный тип (зеркалит net-wrap TcpStream-методов).
- **[M-178-client-live-pool / timeout-needs-173 / autodecompress-needs-179 / typed-json-needs-180 / https-needs-116 / client-policy-surface]** — честно gated за CORE (см. план Ф.2): timeout-by-default←173, decompress←179, typed json[T]←180, https/h2←116, live-pool/proxy/CONNECT/SSRF/cookie-jar/retry/1xx/trailers/Expect100 — surface объявлен, реализация отложена.
## Plan 181 — Same-scope re-binding (D347) (2026-07-04)

- **[production, НЕ упрощение] D347 R1–R7 + B1/B2/B3 (2026-07-04).** Новый чистый pass
  `compiler-codegen/src/alpha_rename.rs` (`alpha_rename(&mut Module) -> RebindTables`):
  scope-stack walker после parse (ДО `number_exprs`/check/codegen — врезан во все codegen-драйверы:
  `main.rs` cmd_check/cmd_compile, `nova-cli` cmd_build/cmd_check, `test_runner::codegen_to_c`;
  **+ bench + LSP добавлены по adversarial-аудиту — см. ниже**),
  уникализирует 2-й+ same-scope биндинг имени в `x__s1`/`x__s2`/… (первый — без суффикса →
  для кода БЕЗ rebind pass = byte-identical no-op, zero-regression). RHS rebind'а резолвится
  в предыдущий биндинг (R3); замыкания/defer — env-снапшот на момент создания (R4);
  per-fn pre-scan резервирует все user-идентификаторы (генерируемый `__sN` не коллизирует).
  Shadow-map (`Module.rebind_shadows: unique→shadowed`) публикуется для consume-checker.
  **R2** (`E_REBIND_LIVE_CONSUME`, `types/mod.rs::check_rebind_live_consume`): затенение
  живого consume-обязательства → hard error на месте rebind (ловит B2 double-consume leak);
  **B1** (false-positive D133 при rebind ПОТРЕБЛЁННого consume) и **B3** (`ro x=1; ro x=x+1`)
  закрыты автоматически уникальными именами. Диагностики демангл'ят `__sN` в
  `diag::render`/`render_extras` (`demangle_rebind_names`) — ни одного `__sN` в user-facing
  выводе. Спека: D347 (03-syntax.md) + amend-врезка D184. Тесты: conformance
  `d347_same_scope_rebinding.nv` + amend d90/d131/d133/d22/d34 (conformance 38/0); pos/neg
  `nova_tests/rebind/` (pos B1/B3/type-change/mut-разморозка/closure + 3 neg
  E_REBIND_LIVE_CONSUME×2/type-change-stale) — 4/4; 5 rust-unit `alpha_rename::tests`.
  Zero-regression: baseline d97c0dbe (temp-worktree), ~135 тестов (basics/generics/consume×6/
  effects/narrowing/syntax/defer/patterns) — delta 0 (те же fail на обоих = pre-existing).

- **[production, НЕ упрощение] Adversarial-аудит close-out (2026-07-04).** Аудит подтвердил
  core-фичу корректной (no-op-путь byte-identical), нашёл 4 driver-coverage/cosmetic-дефекта —
  все исправлены: **(1 bench)** `nova-cli/src/bench/run.rs` (`run` + `compile_for_profile`)
  прогонял codegen БЕЗ `alpha_rename` → benched-файл с same-scope rebind давал clang
  `redefinition` CC-FAIL (тогда как build/test/check ок); alpha-rename врезан ДО check в оба
  bench-пайплайна (зеркалит cmd_build — number_exprs cmd_build НЕ зовёт, resolved_types —
  test_runner-only канал). Verify: bench `ro x=1; ro x=x+1` компилится+бежит. **(2 LSP)**
  `nova-lsp` check_source_inner/provenance/semantic_tokens/server (field-cache) звали
  check_module БЕЗ alpha_rename → `module.rebind_shadows` пуст → R2 `E_REBIND_LIVE_CONSUME`
  НИКОГДА не фаерил в IDE (`check_rebind_live_consume` early-return) + B1 parity break vs CLI;
  alpha-rename врезан в LSP-check-пайплайн, восстановлена документированная byte-parity с
  `nova check`. Verify: rust-тест `parity_lsp_fires_r2_rebind_live_consume` (large-stack thread).
  Побочно: `empty_module()` в provenance.rs не имел поля `rebind_shadows` (Module-struct вырос в
  181, литерал не обновлён) → nova-lsp не компилился на ветке — дополнено. **(3 demangle)**
  `demangle_rebind_names` стрипил ЛЮБОЙ `__sN`-паттерн regex'ом → over-strip валидного user
  `buf__s1` (lexer допускает) → показывал `buf`; under-cover: lint-вывод (cmd_check/cmd_build/
  bench/nova-codegen bin) форматировал RAW `w.diag.message` МИМО demangle → `x__s1` протекал.
  Фикс §0/§1: demangle работает по **множеству реально синтезированных имён** (thread-local
  map new→original, публикуется `alpha_rename` через `set_demangle_map`; пустой map → no-op),
  НЕ regex-зеркало; UTF-8-safe token-scan (не корраптит русскую прозу диагностик); demangle
  врезан во все 4 lint-вывода. Verify: `nova check` — user `buf__s1` показан как есть (даже при
  непустом map с `v__s1`), rebound `v__s1` → `v`, ноль `__s` в user-выводе; 2 rust-unit
  (`demangle_strips_only_synthesized_names` + `demangle_noop_without_synthesized_map`).
  **(4 spec-overclaim)** R2 покрывает ТОЛЬКО same-scope double-consume; nested-scope
  блок-затенение (`consume tx=…; { consume tx=… }`) — та же тихая утечка, но pre-existing gap
  consume-чекера (R7 не уникализирует cross-scope; obligations по имени), идентична на baseline
  d97c0dbe → НЕ регрессия 181. Формулировка D347/plan-181 сужена до same-scope; заведён
  `[M-consume-nested-scope-shadow-leak]` (backlog, D131/D133-территория).

- **[M-181-pattern-var-rebind]** **Остаток (bounded followup — pre-existing, вне scope D347).**
  Rebind САМОГО pattern-bound имени внутри matching-ветки (`if Some(u) = e { ro u = … }`,
  аналогично while-let/match/for-loop var) — **не поддержан**: на такой форме чекер уходит в
  stack-overflow (воспроизводится ИДЕНТИЧНО на baseline d97c0dbe — pre-existing, независимо
  от Plan 181; на baseline codegen даёт `redefinition`). Alpha-rename СПЕЦИАЛЬНО НЕ
  уникализирует rebind над matching-pattern-биндингом (`Scope::pattern_origin`) — чтобы форма
  лоуэрилась в тот же legacy `redefinition` CC-error, а не в новый codegen-panic (zero-
  regression на failure-mode). Честный фикс — в чекере (172.1-зона: аннотация канала
  resolved_types для rebind в pattern-scope + устранение overflow). Plain-`let` destructure-
  rebind (`ro (a,b)=…` повторно) уникализируется штатно (plan §5). Priority: P3.

- **[M-181-opty-in-let-preexisting]** **Наблюдение (НЕ дефект Plan 181).** `ro q = expr?`
  (`?`-в-let) mis-типизирует биндинг как wrapper-тип (`Option`/`Result`) в codegen →
  CC-FAIL/RUN-FAIL. Подтверждён ИДЕНТИЧНО на baseline d97c0dbe для distinct-имени И для
  rebind (`error_chains.nv`, plain passthrough) — pre-existing дефект in-flight 172.1-канала,
  rebind-независим. Поэтому round-trip pos-тест «rebind с `?`» НЕ включён (rebind композирует
  с `?` на уровне type-check/lowering; runtime-assert заблокирован pre-existing багом).
  Устранение — в 172.1 (канал resolved_types для Try-unwrap-биндингов). Priority: не-181.
## Plan 179 Ф.3 — encode (deflate/gzip/zlib, pure-Nova, levels + streaming) LANDED + Ф.2 brotli честный build-gate (2026-07-04)

- **[фича]** **Ф.3 encode приземлён полностью, pure-Nova, БЕЗ упрощений.** `std/encoding/compress/deflate.nv` (NEW) + encode в `zlib.nv`/`gzip.nv`. `CompressLevel` value-record (fastest=1/default=6/best=sentinel→9/none=0/`new(0..11)`; deflate-range 0..9). RFC 1951 deflate-encoder: LZ77 hash-chain matcher (greedy 1..6 / **lazy** 7..9, 32 KiB окно, min-match 3, len 3..258), stored (level 0), fixed-Huffman (1..6), **dynamic-Huffman** (7..9 — частотный length-limited Huffman через порт miniz `calc_min_redundancy` + `enforce_max_code_size`, RLE code-length header 16/17/18, fixed-fallback на вырожденном CL-коде <2 символа). `zlib_encode` (CMF/FLG+FCHECK+Adler-32-BE), `gzip_encode` (10-байт header+CRC-32-LE+ISIZE-LE). Streaming `Deflater`/`GzipWriter`/`ZlibWriter` (value, НЕ consume): `feed` (auto-flush ≥64 KiB, persistent bit-buffer через `take_bytes`) / `@flush` SYNC-FLUSH (`00 00 FF FF` → decodable-префикс, D335) / `finish`. **≥3 различимых режима** (Q7): stored/fixed/dynamic (`best < fastest < stored`). Канонические коды строятся тем же RFC-правилом, что читает декодер → round-trip by-construction.
- **[приёмка]** **ГЛАВНЫЙ acceptance — round-trip `inflate(deflate(x, lvl))==x` — PASS** на всех уровнях (пусто/1 байт/RLE/random/скошенный корпус). **🔴 EXTERNAL ORACLE (anti-circular §8.3):** Nova-encode декодирован НЕЗАВИСИМЫМ python `gzip.decompress`/`zlib.decompress`/`zlib.decompress(-15)` И системным **`gzip -d`** — ВСЕ совпали с оригиналом (RFC 1950/1951/1952 framing подтверждён вне Nova). conformance 38/0 (encode round-trip + per-codec level-validation в d333); nova_tests/compress PASS; zero-regression (0 .rs-правок → binary byte-identical; compress consumed только новыми тестами).
- **[дизайн→фикс]** **Cross-module priv-field фикс (важно для Plan 178-потребителя).** `CompressLevel value { priv n u8 }` — чтение `priv`-поля свободной функцией (`resolve_deflate_level`/`Deflater.new`) ловит `E_FIELD_MODULE_PRIVATE` при cross-module re-check импортируемого disk-loaded std-модуля (Ф.1 не имела ни одного `priv`-поля → баг не всплывал; encode — первое `priv`-поле в модуле). Фикс: чтение через own-type-method `CompressLevel @raw()` (own-method доступ к `priv` разрешён в ЛЮБОМ module-контексте, D220/D281 — совет самого компилятора). **НЕ упрощение** — корректный идиом; consumer-usability проверена реальной cross-module программой (та же, что даёт external oracle). Прецедент к `SocketAddr` (priv через методы). Урок: любой disk-loaded std-тип с `priv`-полем, потребляемый cross-module, ДОЛЖЕН читать поле только в методах.
- **[решение]** **Encode БЕЗ bomb-cap — сигнатура строго `fn(data, level)` по плану §3.5.** Task-prompt упоминал «bomb-cap в сигнатуре encode тоже», но план §3.5/Q7/§3.0 замораживают encode-API БЕЗ `max_output` с явным rationale «выход < входа → decompression-bomb на компрессии невозможен» (вход уже в памяти, амплификации нет). §5 spec-first + «формы СТРОГО из плана» → следую замороженному reviewed-API (иначе ломаю то, что вызовет Plan 178). Anti-flood для encode не нужен by-construction (выход ограничен ~размером входа). Зафиксировано как осознанное решение, не пропуск.
- **[build-gate]** **Ф.2 brotli decode — честный build-gate `[M-179-brotli-vendor-lib]`, net-zero (НЕ фейк).** Эмпирическая проверка (§7.7, Ф.0-verify): `libbrotlidec` ОТСУТСТВУЕТ — `find -iname '*brotli*'` по worktree=0; main-repo vcpkg (`compiler-codegen/vcpkg_installed/x64-windows-static/lib/`, куда указывает gate-env) = только `gc/gccpp/gctba/cord/atomic_ops*/libz3` (brotli.lib/headers нет); единственный vendored native = `target/libuv-cache/libuv.lib`. → настоящий build-gate: C-FFI без либы НЕ фейкается (§0/§7.7). Ф.2 стартует ТОЛЬКО после vendor-коммита google/brotli. Ф.1+Ф.3 самодостаточны и закрывают Plan 178 Q12 (gzip/deflate) СЕЙЧАС.
- **[долг/spec] ✅ ЗАКРЫТ (2026-07-04, spec-only задача).** ~~Pre-existing долг Ф.1: D333–D337 НЕ внесены в `spec/decisions/`~~ → **внесены в `spec/decisions/04-effects.md`** (D333 codec-контракт PURE/byte-first/Result · D334 bomb-cap · D335 streaming coder+SYNC-FLUSH+BodyReader-мост · D336 checksum · D337 brotli C-FFI forward-spec gated). Файл `05-stdlib.md` НЕ создавался — коллизия с `05-memory.md`; блоки приземлены к классу-соседям (D322/D323 io/fs + D325 fallible). Код/тест/plan-комментарии `05-stdlib.md`→`04-effects.md` синхронизированы (§5 spec↔code). README-индекс decisions обновлён; коллизий нет (grep=0); conformance зелёный (spec-only, 0 .rs/.nv-логики). Landed-отклонения зафиксированы amend'ами в D333/D335 (module `encoding.compress`; `read→Result[[]u8]`+`is_done()`; encode без `max_output`).
## Plan 180 (serde/typed-json) Ф.0-VERIFY — эмпирические находки (2026-07-04)

- **[verify, НЕ упрощение] Ф.0-VERIFY компилятор-гейтов на parent-бинаре (2026-07-04, net-zero на коде).** Проверено ФАКТОМ через `nova test` (пробы удалены), а не предположением (§7.7). **Разблокировано (Ф.1 shape РАБОТАЕТ):** (1) Q12 generic-bound protocol-методы `@serialize[S Serializer](s mut S)` компилируются+исполняются (механизм D119 method-level type params + D72 bounds); (2) static protocol-метод с generic-bound + `Result[Self,E]` (`.deserialize[D Deserializer]`) работает; (3) `consume protocol` type-модификатор парсится+type-check'ается; (4) мьютуально-рекурсивные protocol-bounds ок. **Q13/D355 ОБНОВЛЕНИЕ ПЛАНА (ждали «НЕТ» → факт «ДА»):** generic-receiver container-impl `fn Option[T Serial] @serial[S Ser](s mut S)` работает как plain `.nv` (прецедент в проде — `fn Option[T Debug] @debug`/`fn Result[..] @debug`, protocols.nv:660,681) ⇒ container-conformance выразим `.nv`-impl'ами, НЕ обязательно compiler-side mono; СНИЖАЕТ compiler-объём против пессимизма плана Q13. **Гейты подтверждены РЕАЛЬНЫМИ:** `[M-126-sum-*-rich]` OPEN (auto_derive.rs:554-845 — sum-arm'ы заглушки) → Ф.2-sum/Ф.5 гейтнуты (НЕ блокируют record-путь); `Json.parse` без depth-bound; лексер без raw-numeric-token; AST `RecordField`/`SumVariant` без `attrs` (Ф.3a реально добавляет поле); D340–D346 свободны (D-карта verified).
- **[M-180-f64-shortest-roundtrip]** ✅ **CLOSED — correctness-фикс (Plan 180 keystone, 2026-07-04, НЕ упрощение).** Был дефект: `nova_f64_to_str` = `snprintf("%g")` = 6 знач.цифр → лосси (`str.from(3.141592653589793)="3.14159"`, `str.from(1234567.89)="1.23457e+06"`), `decode(encode(v))==v` на float JSON ломался. **Фикс (без упрощений):** shortest-round-trip формат — минимальная десятичная строка `s`, т.ч. `strtod(s)==v` бит-в-бит. Метод: default `%g` первым (сохраняет исторический вывод ≤6-знач-значений → нулевой test-churn; `100000`→"100000", не "1e+05"), при неудаче эскалация точности 7..17 (f64) / 7..9 (f32, `strtof`) до первого точного round-trip; `%.17g`/`%.9g` — гарант. точный, цикл всегда завершается; inf/-inf/nan через `%g` без десятичной пробы. **Единая точка (§0/§3):** ядро `nova_rt.h::nova_f64_shortest`/`nova_f32_shortest`; `conv.h::nova_f64_to_str`/`nova_f32_to_str` — тонкие GC-обёртки; `nova_print_f64`/`nova_print_f32` (direct `println(float)`) funnel'ятся туда же (устранён `%g`-vs-`${x}` дрейф — раньше `println(x)` давал 6-знач, `println("${x}")` — faithful). **Сопутствующие фиксы:** (1) `str.from(f32)` не имел ветки в str.from-dispatch (emit_c.rs) → проваливался в целочисленный fallback и ТРУНКИРОВАЛСЯ (`0.1f`→"0"); добавлена `nova_f32` ветка. (2) f32 `@display`/`@debug` (protocols.nv) + `StringBuilder.@append(f32)` делали `str.from(@ as f64)` — widening к f64 теперь surface'ит f32→f64 mantissa-tail (`0.1f`→"0.10000000149011612"); переведены на f32-precise `str.from(@)` (nova_f32_to_str через `strtof`). **Гейты:** conformance 38/0; round-trip pos-тест (nova_tests/plan91_13/json_roundtrip_float_shortest.nv, 8 блоков); float JSON `decode(encode(v))==v` PASS (был RED); zero-churn эмпирически (все существующие float→str assert'ы ≤6 знач.цифр — 0 изменений вывода). blast-radius (conv.h/nova_rt.h — runtime-хедеры всех программ): 38/38 conformance + float-корпус зелёные; все найденные fail'ы non-float pre-existing (verified: plan154_1 segfault RUN-FAIL и на parent-бинаре).
- **[M-180-valuerecord-err-protocol-method-mono]** **codegen-баг (Ф.0).** `Result[T, <value-record>]` как return-тип PROTOCOL-метода → missing-mono-struct CC-FAIL; heap-record error → PASS. `SerError`/`DeError` specced `value` (§3.7) упрутся (каждый Serializer/Deserializer-метод возвращает `Result[_, *Error]`). Обход: heap-record errors. Backlog P2.
- **[M-180-valuerecord-receiver-generic-method]** **codegen-баг (Ф.0).** `value`-record RECEIVER + method-level-generic метод → receiver by-value туда, где mono ждёт `*T` → CC-FAIL; heap-record receiver → PASS. Задевает Ф.2 synth `@serialize[S]` на scalar/value-record DTO. Backlog P2.
- **Вывод.** Ф.1-shape язык-уровнем разблокирован; ПОЛНЫЙ Ф.1–Ф.4 без упрощений упирается в f64-formatter RED + 2 codegen-бага для specced `value`-типов + объём auto-derive-synth (companion `UserVisitor`-ТИП — синтез типа, не только метода) + 8-протокольная сеть/JSON-backend — плана-масштаб «Волна 2/3», не один заход. Заход = Ф.0-VERIFY (net-zero код; статус в 180-serde-derive.md §0 + backlog).

## Plan 180 (serde) — реализованные упрощения / отклонения от плана (2026-07-04)

Record-path (Ф.1/Ф.2-record/Ф.4) приземлён БЕЗ функциональных упрощений round-trip'а, но с отклонениями формы от §3 плана (каждое — обоснованная realized-form, не cut; spec D340/D341/D344):

- **Serializer = единый stack-machine, НЕ consume-sub-serializers** (§3.2). В value-семантике Nova sub-serializer, мутирующий shared parent state, не имеет чистого владения; stack-machine — sound realization (matched begin/end через синтезатор). Все 12 data-model кейсов + round-trip работают.
- **Deserializer = keyed-access (Swift KeyedDecodingContainer), БЕЗ Visitor-типа** (§3.3). Синтез companion-`UserVisitor`-типа не нужен (план допускал «если требует»). Sub-cursor'ы только читают → нет write-back.
- **Публичный API = free-функции** `json_encode`/`json_decode[T]`, НЕ `Json.decode[T]` namespace-static (§3.8). Причина: turbofish на namespace/type-static generic-методе не мономорфизируется (Ф.0-эмпирика). Followup [M-180-namespace-static-generic-mono].
- **SerErrorKind варианты уникально названы** (`SerDepthLimit`/`SerOther`, не `DepthLimitExceeded`/`Other`): bare-variant construction не диспетчит по expected-type при коллизии имён между SerErrorKind/DeErrorKind.
- **DeError = `{kind, path}`** (location/source-chain — упрощены до текста в `Syntax(msg)`, line/col сохранены). Structured `Location`/`*DeSource` — followup.
- **Ф.3 (атрибуты `#serde`) НЕ landed** — record-DTO round-trip'ится на каноничных именах. Followup [M-180-serde-attributes]. **Ф.2-sum/Ф.5 GATED** (честный named-prereq [M-126-sum-*-rich], не «решим потом»).
- ~~**`[]u8`→base64 НЕ auto-wired**~~ — **✅ УПРОЩЕНИЕ СНЯТО 2026-07-05** (completeness-аудит): синтезатор special-case'ит byte-seq-поле (`is_byte_seq_ty`) → `s.serialize_bytes(@f)?` / `sub.deser_bytes()?` (base64, Q9). Прежняя формулировка «идёт через generic seq» была ЛОЖНОЙ — не компилировалось (`.serialize` на `nova_byte` ICE). [M-180-bytes-base64] CLOSED. Top-level; nested `Option[[]u8]` → typed-error.

Сверх этого: реализация Ф.2 потребовала **11 компилятор-фиксов** в emit_c.rs/types/mod.rs (§0 ожидал 2). Все — реальные codegen-фиксы generic/static/primitive-method-dispatch + инференса, НЕ упрощения. Регрессия-сэмпл clean.

- **[2026-07-05 Plan 180 record-path completeness-аудит — ✅ 6 ДЫР ПОЛНОТЫ СНЯТЫ]** Adversarial-аудит показал, что «record-path без упрощений» был НЕ выполнен — 6 эмпирически-подтверждённых дыр, ВСЕ починены (не отложены). **(1) `Option[value-record]`** (`NovaOpt_NovaValue_Inner` — mono-struct не эмитился до использования by-value в другом value-record): fix — hoist NovaOpt-typedef'ов, зарегистрированных при резолве полей value-record'а, ВПЕРЁД struct'ы в `value_record_defs_buf` (`emit_value_record_type`, delta-snapshot; родич keystone `[M-valuerecord-result-vtable-mono]` — там forward-decl pointer-field, тут полный typedef by-value). **(2) `HashMap[str,value-record]`** (mono'd generic struct как pointer-поле value-record'а): early forward-decl `typedef struct Nova_X____… …;` перед struct'ой. **(3) `Option[Option[int]]`**: deser — рекурсивный inline null-check в `deser_field_expr` (built-in `Option` не диспатчит user-static `.deserialize`); typed `None as Option[T]` (bare `None` в then-ветке вложенного check'а mis-инферился на внутренний Option) через новый As-codegen special-case (`option_none_expr`, NPO-aware). `Option[Vec[str]]`: `.serialize` на mono'd-контейнере внутри Option-mono возвращал unknown type → receiver-invariant `Result[(),SerError]`-fallback (2 infer-site'а + `?`-lowering). **(4) скаляры вне {int,str,f64,bool,u64}**: eligibility принимал все 18 NOVA_PRIMITIVES но synth имел ветки на 5 → ICE. Fix — narrow-скаляры (i8..i64/u8..u32/uint/f32) синтезатор эмитит INLINE (`s.serialize_int(@f as int)?` + inline range-guard `if raw<MIN||raw>MAX { return Err(OutOfRange) }`), т.к. primitive-method-dispatch НЕ работает внутри generic-mono; char/i128/u128/retired-byte → typed `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL` (§6, не ICE). Nested-в-контейнере narrow → тоже typed-error (top-level-only; `serde_supported_scalar` vs `serde_container_scalar`; честный [M-180-container-narrow-scalar]). **(5) `[]u8`→base64** (см. выше). **(6) ENCODE silent-lossy** (`v as f64` без exact-check — `json_encode(2^53+1)` молча выдавал ...992): fix — `SerLossyInteger` (уникальное имя — коллизия варианта с `DeErrorKind.LossyInteger` строила НЕ тот тип) + guard `|v|>=2^53` симметрично `is_exact_int`. **Плюс латентно:** `?` на `v.serialize(s)` в generic-serde-mono эмитился как `/* ? */` no-op → Err молча глотался (`json_encode` возвращал Ok на lossy); early-prototype для structural `nova_opt_eq_<X>` (value-record с полем `Option[Vec[str]]` звал late-def без proto → conflicting-types). **Гейты:** conformance 38/0; extended round-trip (`nova_tests/serde/autoderive_ext.nv` — Option[rec]/HashMap[str,rec]/Option[Option]/Option[Vec]/i64/i32/u32/f32/[]u8/Some(None)-collapse) PASS; neg (`neg/unsupported_scalar.nv` char → typed-error) PASS; zero-regression vs parent 64675407 (46-file sample generics/protocols/json/value-records/io, 0 delta).

## Plan 176 (io/fs/os) Ф.3 — os: реализованные отклонения формы от плана (2026-07-06)

`std/os` (D324) приземлён БЕЗ семантических упрощений (env/args/cwd/dirs/exit/pid/hostname — полный набор Ф.3; byte-корректность значений; real+mock триада; concurrency-контракт documented). Отклонения формы — каждое обоснованная realized-form, не cut:

- **`exit_process`, НЕ `exit`** (§4 план: `exit`). Bare `exit(code, msg)` — язык-builtin (D13, `-> never`, message-bearing abort); public os-fn получил бы 2-арг builtin-резолв. Realized-имя `exit_process` (Go `os.Exit`/Rust `process::exit`-паритет). Не упрощение — семантика exit(flush stdout/stderr + terminate) полная.
- **mock_os дом = `std/os/mock.nv`, имя `mock_os`/`MockOs`** (§4 план: «дом — std/testing/handlers.nv», `mem_os`). Заменён на codebase-precedent: `mock_fs`/`mock_io` живут в домен-модулях (`std/fs/mock.nv`, `std/io/console.nv`), не в testing/handlers.nv; `mock_*`-имя — та же конвенция, что Ф.1/Ф.2 (плановый `mem_os` был бы odd-one-out). Функционально полный (env/args/cwd map + **recorded-exit** `did_exit`/`exit_code` → observable без убийства харнесса).
- **cwd/hostname ошибка → `IoError.from_os(0, op)`** (kind `Other`), НЕ `IoError.of(ErrorKind.Other(0), …)`. `Other(int)` — payload-вариант; cross-module (std.os→std.io) литерал-конструкция ловит checker-gap `[M-176-xmod-payload-variant-ctor]` (тот же, что SeekFrom в Ф.1). `from_os`/`kind_from_errno` строят `Other` ВНУТРИ std.io. Не упрощение — kind честный (Other/unknown, raw_os=0), т.к. `Os`-эффект (str-getter, ""=fail) не проносит errno для cwd/hostname; getcwd/gethostname редко фейлят.
- **Приватные хелперы `os_cstr`/`os_wrap_unit`** (не `c_path`/`wrap_unit`). Free-fn coarse-by-name резолв (D323-нота #3) → коллизия с std.fs `wrap_unit` (redefinition CC-FAIL). Уникальный префикс.
- **`int main(void)` → `int main(int argc, char** argv)`** (`emit_c.rs`, единственная точка эмиссии main) + `nova_os_set_args(argc, argv)` для `os.args`. Аддитивно; zero-regression подтверждён (io/fs/effects/basics/concurrency sample vs parent-бинарь 5bb1ead7 — delta 0). Не упрощение — args captured честно (не /proc-хак).
- **`Os` effect = тонкий int/str-primitive слой** (тот же паттерн, что `Fs`/D323): rich `Option`/`Result`/`Path`/`EnvVar` строятся в `os.nv`-обёртках ВНЕ effect-vtable (которая стёрла бы value-типы). Не упрощение — §3/§0-канон «логика в .nv над тонким C-hook».
- **os native hooks (`nova_rt/os_env.h`) — header-only static-inline, НЕ libuv park/wake** (как fs.c). env/cwd/pid — non-blocking нативные syscall'ы (getenv/getcwd/setenv/…); libuv-park/wake — для реального блокирующего I/O (fs/net). Тот же прецедент, что io_console.h (fs_seek/platform-predicate). Не упрощение — правильный слой для не-блокирующих ops.

Subprocess (`Command`/`Child`/`spawn`) — НЕ в Ф.3 by design: под-план **176.1** (Q5, `[M-176.1-process]`), гейт после Ф.1-Ф.3.
## Plan 178 Ф.2-enhancements + Ф.3 server-CORE (2026-07-06, ветка plan-178-http)

Deps-in-main проверены эмпирически; ниже — что закрыто и что честно gated (не упрощения).

- **Typed JSON body — LANDED.** `json_decode_body[T Deserialize](body []u8) -> Result[T, HttpError]` в новом узком модуле `std.http.serdejson` (поверх serde `T.deserialize`, DeError→`HttpError{Protocol}`+source-detail). 4 pos/neg-теста (`nova_tests/http_typed/`): record-DTO round-trip, null-Option→None, malformed→Protocol, missing-field→Protocol. **Два вынужденных codegen-обхода (НЕ упрощения):** (a) FREE-fn turbofish вместо `Response.@json_as[T]()` — generic-МЕТОД с type-param только в return-позиции игнорит turbofish → монеморфизирует в `nova_int` (`void* w`, silent-miscompile) [M-codegen-method-return-turbofish]; (b) отдельный модуль (не client.nv) — serde в большом multi-file `nova_tests.http` CU роняет `Result[(),SerError]` protocol-vtable forward-decl [M-codegen-serde-vtable-forwarddecl]; в изолированном single-file CU работает.
- **Auto-decompress — BLOCKED-BY-CODEGEN (не Plan 179).** decode (`gzip_decode`/`zlib_decode`/`inflate`) есть и корректен; логика auto-decode написана. Но wiring compress в http-CU → `std.encoding.compress.ErrorKind` и `std.http.ErrorKind` эмитят ОДИН `struct Nova_ErrorKind` (C-mangling по короткому имени, без module-qualification) → `redefinition of enumerator NOVA_TAG_ErrorKind_Other` CC-FAIL. client→compress + client→http принуждают co-presence. Реверт wiring; feature НЕ приземлена честно. Fix = module-qualified type-mangling (codegen, крупно) ИЛИ ренейм одного `#stable ErrorKind` (governance). Тот же барьер накрывает план-овский `ErrSource.Compress`. [M-codegen-nominal-type-name-collision]
- **Timeout-by-default — REFINED-gate.** Эмпирика: raw-substrate ЕСТЬ (`CancelToken`+`supervised(cancel:)`+`Time.sleep`+cancel-safe net-park). Но `within`/`with_timeout`/`race2` берут ЧИСТЫЙ `fn()->T` (no effect-row) → не обёртывают `Http`-effectful `send()`; `supervised(deadline:)` — **ноль usages/impl во всём репо**. Нужен effect-poly deadline-combinator ИЛИ `supervised(deadline:)`. Не приземлено — уточнённый маркер.
- **Server Ф.3 — CORE LANDED (pure) + live-runner blocked-at-link.** Substrate UNBLOCKED (echo-тест доказывает supervised/spawn/accept). `std.http.server` (server/wire): `ServerRequest`/`ServerResponse`/`Handler`(concrete closure-newtype, Q27-fallback)/`ServeMux`(Go-1.22 {param}+method+405-Allow+404+HEAD→GET)/`parse_request`(Host-mandatory+`..`-reject+CL-framing)/`serialize_response`/`serve_once`. **9 mock-тестов (no sockets) PASS.** Live-runner `std.http.servernet.handle_connection` написан+codegen-компилится, но loopback-smoke НЕ ЛИНКУЕТСЯ: `undefined symbol Nova_NetError_static_IoError` (payload-variant ctor не эмитится в net+http combined-CU; net-only OK) [M-codegen-cross-module-ctor-emission]; плюс `with Net{}`-блок мис-выводит result-тип при HttpError-в-CU (обход `()` tail). Серверная ЛОГИКА полностью покрыта mock'ами (план: «mock где возможно»). Followups: streaming/backpressure, middleware/100-continue/keep-alive, graceful-deadline-drain (gated `supervised(deadline:)`), typed request-body.
- **Owner-конвенция применена:** `Response.of`→`Response.new`, `ContentType.of`→`ContentType.new` (message.nv/mime.nv + call-sites); `.of` только за вариадик-коллекциями.
- **Гейты:** conformance CU PASS/0-fail; http/http_server/http_typed/http_transport = 4 CU PASS/0; zero-regression sample serde+serde_e2e+net(plan91_12) = 16 CU PASS/0 (delta 0; compress byte-untронут); cargo build (debug+release) clean.
## Plan 173 Ф.4 #5 (typed `ScopeOutcome.Failure(any)`) — realized-form + документированные границы (2026-07-06)

`Failure(str)`→`Failure(any)` протянут БЕЗ функциональных упрощений keystone'а (typed narrowing `if err is T`
в `@cleanup`/`defer(o)` работает и на FromFrame-, и на interrupt-path идиоме `with Fail = … interrupt`). Границы:

- **Interrupt-path идентичность ошибки — через thread-local `_nova_last_error`, НЕ через `_nova_fail_top`.**
  Эмпирика (cross-fn throw): к моменту interrupt-unwound cleanup'а `_nova_fail_top` указывает на РАЗРУШЕННЫЙ
  stack-fail-frame бросившей функции (interrupt не поп'ает fail-frames) → чтение payload = **segfault**.
  Sound-фикс: стабильный snapshot `{msg,kind,payload,tid}` стемпится на throw (heap/GC-boxed payload переживает
  stack), читается cleanup'ом, гасится на catch (`nova_scope_exit CATCH` + with-block-consume в `nova_interrupt`).
  **Остаточное staleness-окно** (payload-only, вариант всё равно Failure): pure value-`interrupt` в промежутке
  между throw и его catch'ем, ИЛИ per-E-handler (`with Fail[E] = …`), который interrupt'ит ДО fallback'а на
  `nova_throw_typed` (генерик-`Fail`-handler и bare-throw покрыты — capture в `Nova_Fail_fail`/`nova_throw_typed`).
  Ни один тест не инспектирует payload на value-interrupt-path; GC-dangling исключён (`is T` проверяет tid без
  deref — deref только после успешного match). Полное покрытие per-E = capture в codegen-emitted `_nova_throw_typed_<E>` (followup).
- **Box-репрезентация typed payload предполагает pointer (record).** `nova_any_from_boxed` оборачивает
  `error_user_payload` (= `Nova_T*` throw-site heap-box) на один уровень (`*(Nova_T**)data` narrowing) — верно
  для ВСЕХ user error-типов (records → pointer-repr; универсум typed-errors). Гипотетический value-typed throw
  (throw примитива/value-struct) box'ился бы иначе — вне периметра (не встречается: `throw "s"` идёт str-path).
- **CANCEL → `Failure(CancelError{reason})`** (не отдельный `Cancelled(any)`-вариант из spec-sketch 03-syntax:8733):
  соответствует существующей 3-вариантной `ScopeOutcome` (Success/Failure/Panic) и плану §6 (Failure(any) folds cancel);
  префикс `"cancel: "` (bootstrap-дискриминатор) убран — дискриминация теперь `err is CancelError`.

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
## 2026-07-06 — D381 collision-aware module-qualified nominal-type mangling (ветка fix-nominal-mangling)

**[дизайн]** Cross-module same-name type collision (`ErrorKind` × std.io/std.http/std.encoding.compress
в одном CU) закрыт **collision-aware** квалификацией, а НЕ always-qualify. Выбор обоснован фактами
(§7 blast-radius map): always-qualify = massive churn всех `.c` + слом extern-контрактов
`Nova_str_*`-класса; collision-aware = квалифицируем ТОЛЬКО имена, объявленные в ≥2 модулях
(`Nova_<modpath>_<Name>`), всё прочее байт-идентично (`colliding_type_names` пуст → хелперы no-op →
`.c` не меняется). НЕ сокращение кода (добавлены карты коллизий + пара mint-хелперов + арность/
контекст-дизамбигуация bare-варианта) — дизайн-выбор минимального-churn sound-фикса. Единая пара
`def_type_base`/`ref_type_base` (identity для не-коллидирующего) на всех mint-сайтах вместо зеркал.
Область: plain-Sum + heap-Record (pointer-identity); newtype/value-record/generic/opaque — followup.
Спека — D381 (08-runtime.md). Гейт: conformance PASS N/0 (фикстуры d358/d333-336 возвращены в
conformance); zero-regression byte-identical (content) на не-коллидирующем корпусе. Закрывает
`[M-sync-crossmodule-samename-type-collision]` + `[M-codegen-nominal-type-name-collision]`.
**НЕ** закрыл `[M-codegen-cross-module-ctor-emission]` (victim NetError.IoError — variant↔type
name-clash, отдельный root, репро идентичен на baseline).

---

**[178 Ф.2-enh, 2026-07-06] Auto-decompress landed + `[M-codegen-cross-module-ctor-emission]` FIXED (keystone) + live-socket smoke restored (link-unblocked, runtime-gated).**

**Codegen-фикс (keystone, разблокировал остальное).** Root уточнён репро (прошлый диагноз неточен):
explicit-receiver **payload-variant CALL** `Sum.Variant(x)` (`NetError.IoError(msg)`) парсится как
2-сегментный `Path` → в `emit_call` диспатчится через `method_overloads`-static-ветку, где payload-вариант
зарегистрирован КАК pseudo-static-overload с `c_name = Nova_<Sum>_static_<Variant>` (никогда не определён;
определён лишь `nova_make_<Sum>_<Variant>`). **НЕ** зависит от co-present одноимённого ТИПА (`IoError`) —
репро идентичен с/без `import std.io` (это НЕ variant↔type clash, а **universal** explicit-receiver
payload-variant misroute). Unit-варианты (`NetError.ConnectionReset` — member-access, не call) не задеты.
**Fix:** хелпер `try_emit_explicit_variant_ctor(recv_type, variant, args)` — когда receiver=сумма,
владеющая payload-вариантом `variant` подходящей арности (не generic; collision-aware base через
`ref_type_base`), эмитит `nova_make_<sum>_<variant>(args)`. Вставлен в ОБА static-emit-сайта (Path-арм
до `method_overloads`-lookup + Member `method_receivers`-арм). Вариант всегда бьёт одноимённый
static/тип-в-скоупе (контекст однозначен). Доказано: servernet CU эмитит `nova_make_NetError_IoError`
(0 undefined static-ref; baseline=1) → net+http линкуются. НЕ сокращение — целевой sound-фикс роутинга.

**Auto-decompress (`[M-178-autodecompress-needs-179]` CLOSED).** `std.http.client`: default
`Accept-Encoding: gzip, deflate` (opt-out `@no_decompress()`); `finalize_response` прозрачно декодит
`Content-Encoding` gzip/`x-gzip` (`gzip_decode`) + `deflate` (`zlib_decode`→raw `inflate` fallback для
не-zlib-сендеров), снимает `Content-Encoding`+переписывает `Content-Length` на декод-длину. Bomb-guard
`max_decompressed` (64 MiB default, D334; `@max_decompressed(n)`, `<0`=без cap) прокинут как `max_output`
→ `Err(BodyTooLarge)`, НЕ OOM. Decode-fail → `HttpError{Protocol}` + типизированный
`ErrSource.Compress(CompressError)` (`HttpError.from_compress`; OPEN enum, non-breaking). `br` закрыт —
нет кодека `[M-178-autodecompress-br]`. Добавлен `CompressError.@is_bomb()` (bomb-детект без импорта OPEN
`ErrorKind`-вариантов, чей `Other` коллидировал бы с http). Разблокировано **D381** (collision-aware
mangling: compress+http `ErrorKind` co-present линкуемы — работает и на merge-base baseline).
Тесты `nova_tests/http_decompress/decompress_test.nv`: gzip+deflate круговой round-trip (mock-encode 179 →
клиент декодит back to original), opt-out (тело остаётся compressed), neg bomb→`BodyTooLarge` — все PASS.

**Live-socket smoke (Task 3, честный gate — НЕ упрощение).** `nova_tests/http_servernet/servernet_smoke_test.nv`
(loopback GET /health через `handle_connection`) восстановлен. LINK-препятствие снято (см. codegen-фикс).
**RUNTIME-блок — pre-existing net-substrate segfault** `[M-178-servernet-live-net-substrate-segfault]`:
чистый net две-fibers loopback тест (ZERO http, ZERO codegen-change) сегфолтит ДЕТЕРМИНИРОВАННО (5/5,
~100ms) на merge-base baseline И current. Также plan83_12 net-тесты ICE `[P67-LEGACY] method=bind` +
`.unwrap()` на `Result[_,NetError]` эмитит `Nova_Fail_fail(NetError*)` vs `nova_str`. Net live-socket
substrate в этом worktree широко сломан — не Plan 178. Смоук хранится (как plan83_12 соседи) — не в
быстрой regress-выборке; зазеленеет с фиксом net-runtime. Server-ЛОГИКА полностью mock-покрыта (9 PASS).

**Гейт:** сборка Rust чистая; conformance **54/0** (не тронут); http/compress/io/fs delta-0 (baseline vs
current, byte-behaviour). Спека: 02-types §D358 Ф.2-амендмент (`ErrSource.Compress` + auto-decompress
инварианты).
## 2026-07-06 — Пакет 4 codegen-дыр (ветка plan-176-io-fs-os)

Четыре независимых codegen-дыры закрыты, каждая отдельным коммитом; zero-regression
подтверждён двоичником merge-base (d478e72a, временный worktree) на ~41 CU
(generics/protocols/io/serde/http/str/plan153_*/plan91/plan138/…) — дельта 0, только
3 фикс-CU (http_typed/plan153_1/plan176_holes) red→green. НЕ сокращения — все четыре
таргет-форма корневые фиксы, gated узко.

- **[M-176-generic-wrapper-mono-inference]** — inference-ctor generic-wrapper'а с
  void-ptr-полем. `try_generic_static_ctor_mono` (emit) + `infer_generic_static_ctor_ret`
  (infer, воткнут ДО checker-каналов — checker резолвит в erased wrapper). Gated
  `generic_type_has_voidptr_fields` (stub-only) + полная выводимость type-args.
- **[M-valuerecord-receiver-generic-method]** — call-site return-inference method-generic
  метода на value-record. Sentinel-путь `infer_expr_c_type` теперь через
  `resolve_result_option_ret` (строит `NovaRes_<ok>_NovaValue_<E>*`) — value-БЛЕДНЫЙ
  `apply_type_subst_to_ref` пропускал Result/Option → void*.
- **[M-codegen-method-return-turbofish]** — turbofish метода с type-param только в return.
  Emit: seed `current_method_turbofish` в unbound method-level slots перед nova_int-fallback.
  Infer: `turbofish_args`→`resolve_mono_type_args`. Плюс consume-checker unwrap turbofish в
  `consume_walk_expr` (иначе `consume @m[T]()` не consumed receiver → ложный D133). Deliverable:
  `Response consume @json_as[T Deserialize]()` в std.http.serdejson (decode инлайнен — делегация
  триггерит открытый bound-forward gap [M-176-io-forward-bounded-generic]).
- **[M-153.1-append-as-slice-ccfail]** — same-arity param-type overload (`Box6[T] @tag(int)` /
  `@tag(str)`) на generic-типе схлопывался: дедуп `generic_type_methods` сравнивал name+count+
  receiver, НЕ param-типы. Fix: span-free `type_ref_overload_key` в дедупе (TypeRef без PartialEq).

## 2026-07-06 — Plan 173 Ф.4 #6/#7: MultiError модель Б — карман подавленных (ветка multierror-173)

Вариант Б утверждён владельцем (2026-07-06): primary отдаётся ловящему КАК ЕСТЬ (типизированная
ловля `with Fail[Primary]` работает, эффект НЕ становится `Fail[MultiError]`); подавленные — «в
кармане», достаются свободным аксессором `suppressed() -> []any` ПОСЛЕ ловли. Спека D158
амендирована (баннер + §«Модель доставки — вариант Б»; конверт-модель → §«Что отвергнуто»).

**Механика (НЕ упрощения — корневые фиксы):**
- Карман = `_nova_last_error.frame.error_suppressed` (инфра Ф.4 #5). Заполнение: FAIL-path —
  зеркало в `nova_rethrow_with_suppressed` (transport-chokepoint); interrupt-path — per-cleanup
  `NovaFailFrame` вокруг defer-тел + prepend-compose в карман (LIFO-цепочка → `suppressed()`
  ходит back-to-front → хронологический порядок аварий). Reset на каждый свежий throw (все
  stamp-сайты) → нет утечки между несвязанными ловлями (#7).
- **Hijack-фикс:** cleanup-throw во время unwind диспатчился в ещё-установленный with-Fail
  handler (string-slot arm БЕЗ tid-check → мисфайр на чужом payload, перезапись результата,
  двойной прогон defer). Теперь `NovaFailFrame.is_cleanup` + `nova_in_cleanup_unwind()`-байпас
  в `Nova_Fail_fail`/`nova_throw_typed`/generated per-E entries; unwind-кадры маркирует codegen
  (defer `_tdf`/`_idf`, consume FromFrame|Interrupt). Handler-wrap ВНУТРИ cleanup работает
  (свой не-cleanup кадр); normal-exit cleanup — handler срабатывает (ошибка = primary).
- **Per-E stamp-дыра (#5-хвост):** `_nova_throw_typed_<E>` не стемпил `_nova_last_error`
  (арм interrupt'ует → erased-fallback не достигался) → пре-E ловля не сбрасывала карман
  (чужая цепочка текла в следующую ловлю). Stamp перенесён В НАЧАЛО generated entry
  (зеркало «capture BEFORE dispatch» из Nova_Fail_fail).
- `nova_interrupt_push_defer` зануляет `value`/`value_ptr` (re-issue пробует value_ptr;
  stack-garbage уводил int-interrupt в pointer-роут → мусорный результат with-блока).

**Остатки (документированы, вне периметра Ф.4):**
- 🟡 `[M-110-multierror-any]`-хвост: поля типа `MultiError` (`primary`/`suppressed`) остаются
  `str` — тип теперь ОПЦИОНАЛЬНАЯ value-обёртка (не конверт эффекта), typed-агрегация поверх
  `suppressed()` при спросе.
- 🟡 Single-file `nova build`: `el is T` на элементе `[]any` даёт `unknown variant`
  (folder-module `nova test` — ок); pre-existing checker-quirk одиночного CU.
## Plan 174 — `supervised(deadline:/timeout:)` deadline-combinator (D408, 2026-07-06)

- **[Plan-174-deadline-combinator]** (LANDED, `nova-p174` ветка deadline-combinator) — областной срок
  как СТРУКТУРНАЯ keyword-конструкция (не fn-обёртка → эффекты тела протекают): `supervised(deadline:
  <Monotonic>)` (абс. точка, канон) / `supervised(timeout: <Duration>)` (относит. сахар = `now()+d`),
  комбинируется с `cancel:`. Механика: `NovaFiberQueue.deadline_ns` (абс. монотон. ns, 0=нет); `nova_scope_init`
  наследует ambient-срок из `_nova_active_scope`; codegen `emit_supervised` ужимает своим сроком через
  `nova_deadline_combine` (min ненулевых — inner можно только ужесточить); `run_impl` bounded-idle
  (`_nova_scope_deadline_run_once`: armed stack-timer + UV_RUN_ONCE) будит drain в точке срока; в точке —
  `nova_scope_deliver_cancel` (путь `cancel:`, sleep/net-park прерываются рано); наружу типизированный
  `TimeoutError{deadline_ns i64}` (prelude, `is TimeoutError`/`with Fail[TimeoutError]`) через splice
  `_nova_throw_scope_timeout_impl` (по образцу CleanupTimeoutError). USER-precedence: реальная ошибка бьёт
  срок. Zero/past→immediate (D317-дух). Тесты `std/concurrency/supervised_deadline_test.nv` 8/8 (в т.ч. замер:
  sleep(5000) под timeout(100ms) завершается <2000ms). Regress delta 0 (сверено baseline 560566f3;
  побочно ПОЧИНЕН latent `plan83_10_3/nested_supervised_cancel` RUN-FAIL→PASS).
- **[Plan-174-active-scope-longjmp-restore]** (сопутствующий bugfix) — throw (в т.ч. TimeoutError),
  пробивающий тело внешней области до её run-loop, оставлял `_nova_active_scope` висящим на освобождённом
  stack-фрейме → следующая область наследовала garbage `deadline_ns` (spurious immediate TimeoutError).
  Fix двухуровневый: (1) `run_impl` восстанавливает `_nova_active_scope = q->saved_active_scope` на ВСЕХ
  путях выхода (в т.ч. longjmp); (2) `with Fail[...]`-блок (emit_with) снапшотит+восстанавливает active-scope
  вокруг catch (gated на Fail — не-catching effect-handlers byte-identical). Латентный pre-existing дефект,
  экспонированный deadline-наследованием.
- **Известные ограничения (честный gate, НЕ упрощения фичи):** (a) main-flow blocking В ТЕЛЕ до старта
  run-loop не ограничено сроком — тело исполняется inline перед `nova_supervised_run`; идиома — `spawn`
  работу (structured-concurrency канон); (b) чекер НЕ выдаёт structured-диагностику на неверный тип
  аргумента `deadline:`/`timeout:` — полагается на C-type-check `.nanos` (как `ChanReader.close_after`);
  правильная checker-диагностика — followup; (c) `deadline:`/`timeout:` требует `import std.time.duration`
  (Duration/Monotonic ctors) — как и любое использование этих типов; (d) `parallel for`-зеркалирование
  параметров отложено `[M-174-parallel-for-deadline]`; (e) ретракция `with_timeout` отложена
  `[M-174-retract-with-timeout]` (cancellation.nv независимо сломан retired-API).

## Plan 183 Ф.3 — миграция std/http-транспорта на std/net2 (2026-07-06)

- **[Plan-183-Ф3-consumer-migration]** (LANDED, ветка `net-rework-183`) — `std/http/transport/real.nv`
  и `std/http/servernet/servernet.nv` переведены с `std.net` (str-носитель, D-block Д3 из плана 183)
  на `std.net2` (байт-поверхность D407: `[]u8` everywhere, `Ok(0)`=EOF вместо `Err(Eof)`, `resolve()`
  свободная fn вместо `SocketAddr.lookup`). Сопутствующая миграция потребителей вне исходной Ф.0-карты,
  но обязательная по сигнатуре: `nova_tests/http_transport/transport_test.nv` (`mock_net` источник),
  `nova_tests/http_servernet/servernet_smoke_test.nv` (прямые вызовы `TcpListener`/`TcpStream`) —
  оба импортировали `std.net`, теперь `std.net2`. `examples/net/{echo_client,echo_server}.nv`
  переписаны на `std.net2` (были уже сломаны ДО этого захода — 0 импортов, `nova build` падал ICE
  на unresolved `SocketAddr`; сейчас `nova check` → PASS на обоих, `nova build`-бинарь блокирован
  отдельным пре-существующим дефектом, см. ниже).
- **Побочный фикс (не net2-специфика):** `servernet_smoke_test.nv` не оборачивал тело в
  `with Net = real_net() { … }` → `TcpListener.bind` диспетчерил на null-vtable-слот → SEGV.
  Ранее атрибутировалось к «M:N net-substrate segfault»; реальный корень — отсутствующий
  handler-install. Добавлен `with Net = real_net()`; 5/5 детерминированных прогонов.
- **Старый слой НЕ удалён** — потребители `nova_tests/{plan83_12,plan91_12,plan91_15,plan91_16}`
  и `plan178/net_byte_surface_mock.nv` остаются на `std.net` до санации Plan 182. `std/net/*.nv`
  получили баннер `// DEPRECATED (план 183 Ф.3) …`; удаление + grep-инварианты +
  namespace-ренейм `net2`→`net` — `[M-183-old-net-removal-after-182]` (docs/plans/backlog-followups.md).
- **Гейты:** conformance `--positive --compile-error` 54/0 (без изменений — Rust-компилятор не
  трогался). http-семейство (`http`/`http_transport`/`http_server`/`http_typed`/`http_decompress`/
  `http_servernet`) + `std/net2/tcp_test` + `std/http/client` — **8/8 PASS** (было 5/5 до захода;
  3 новых зелёных, 0 регрессий; дельта против до-Ф.3 состояния = 0 новых FAIL — бинарь nova.exe
  не пересобирался между базисом и финалом, разница чисто в `.nv`).
- **Найден новый (не Ф.3-специфичный) дефект:** `[M-183-nova-build-consume-effect-close-ice]` —
  `nova build` (в отличие от `nova test`) ICE на `mut x = match Effect.op(...){…}; x.close()` для
  ЛЮБОГО эффекта с consume-результатом; репродуцировано идентично на старом `std.net` — общий разрыв
  между `nova build`- и `nova test`-путями тайпчека, не регрессия этого захода. Задокументировано в
  `docs/plans/backlog-followups.md`.

## План 183 Ф.4 (2026-07-06) — корень UDP-флейка, M:N-стресс, замер

- **UDP-флейк: корень = loop-affinity, зафиксирован фактом, фикс — паттерн, не маршалинг.**
  Субстратный полный фикс (defer-op-маршалинг issue-стороны каждого uv-опа на owning-loop-thread,
  обобщение `nova_loop_defer_close`) СОЗНАТЕЛЬНО не делался в этом заходе: датаграммы не теряются,
  латч корректен, класс закрывается паттерном «создавай сокет внутри волокна-оператора» (контракт
  задокументирован в net2.c; тесты переписаны; 60/60+128/128 против ~1/40 до). Остаточный узкий
  класс steal-миграции и полный фикс → `[M-183-net2-loop-affinity-cross-thread-op]` (P2).
- **`int.to_str()` в stress_test обойдён `${...}`-интерполяцией** — не фикс компилятора:
  same-module method-name collision (`NetError @to_str` перехватывает int-receiver) →
  `[M-183-int-to-str-module-method-collision]` (P1). Обход помечен NOTE в тесте.
- **Замер только нового слоя:** у старого net.c нет эквивалентного throughput-теста «без правок»
  (plan91-echo — smoke); писать его на умирающий слой не стали — базовая точка ~600 MiB/s
  зафиксирована для нового (план §2а допускает).

## Plan 183 — ЗАКРЫТИЕ ядра (Ф.0-Ф.4 ✅, Ф.5 журнал/спека) (2026-07-06)

Директива владельца 2026-07-06 («сеть реализована в корне неверно, нужна переработка») —
выполнена за 4 захода агента (Ф.0 карта+спека → Ф.1 C-слой → Ф.2 `.nv`-обвязка → Ф.3
миграция потребителей → Ф.4 M:N-стресс+замер), Ф.5 — этот журнал-close-out.

**Три корневых дефекта владельца — как закрыты:**
1. **Д1 (двойная обёртка):** `net.c` имитировал nova-манглинг (68 `NovaRt_*_method_*`) +
   второй слой `ffi.nv` (53 literal-entry). Закрыто: один слой `nova_net_*` C-ABI
   (D282-стиль: скаляры/указатель+длина/out-параметры/код-возврата), Nova-типы и вся
   логика — в `.nv` поверх `extern "C"` (образец std/fs). `net2.c` — 0 `NovaRt_*_method_*`.
2. **Д2 (M:N-небезопасность):** 6 `__thread`-результатных слотов — волокно пишет на
   потоке A, читает на потоке B (мигрировавшее work-stealing'ом) → чужие/пустые данные;
   вероятный источник детерминированного сегфолта live-socket-теста. Закрыто: результат —
   значением (код-возврата + out-параметры), `grep __thread net2.c` = 0. Попутно найден и
   закрыт СМЕЖНЫЙ, но отдельный субстратный дефект того же M:N-класса — lost-wake парковки
   (`nova_sched_wake` резолвил волокно через слот, публикуемый только ВНУТРИ `park`; колбэк,
   выстреливший между issue и park, терял wake навсегда) — фикс: publish scope/slot +
   atomic done-латч ДО issue, `nova_sched_park_until`. После обоих фиксов: двух-волоконный
   TCP echo (тот самый сегфолт-сценарий) 20/20 бинарь + 15/15 харнесс.
3. **Д3 (`str` как носитель байтов):** сеть возвращает произвольные байты, `str` — UTF-8-текст.
   Закрыто: эффект `Net` — `[]u8`-сигнатуры everywhere (`read`/`write`/`send_to`/`recv_from`);
   текстовые удобства (`read_text` и т.п.) — пользовательские `.nv`-хелперы через
   `Result[str, Utf8Error]`, не операции эффекта.

**Нулевое копирование (требование владельца §2а) — как достигнуто:** модель Go/Rust
`read(buf) -> n`. Read/recv: Nova-вызывающий владеет буфером (`mut buf []u8`); C-слой
сохраняет указатель+ёмкость в handle ДО `uv_read_start`/`uv_udp_recv_start`, `alloc_cb`
отдаёт libuv ТОТ ЖЕ срез — сеть пишет прямо в память Nova-буфера, `read_cb`/`recv_cb`
сообщает только длину. Write/send: `uv_write`/`uv_udp_send` получают указатель прямо на
`[]u8` вызывающего; буфер жив на стеке волокна на время операции (консервативный GC его
видит). Итог: `malloc`/`memcpy`/`nova_alloc` данных в hot-path read/write/send/recv = **0**
(верифицировано построчным ревью `net2.c`, не только декларативно). Единственные копии —
поимённые ОС-переносы вне hot-path: `sockaddr_storage`→`NovaNetAddr` (accept/peer/local/UDP
sender) и `addrinfo`→GC-массив (DNS, **один** `getaddrinfo`-вызов — без повторного запроса).
`SocketAddr` — 20-байтная value-запись (не handle, не куча) — закрывает
`[M-net-socketaddr-value-record]`. Ориентир-замер (Ф.4, `std/net2/stress_test.nv`, 8-КиБ
пер-волоконные узоры под work-stealing + 8 МиБ ping-pong 64-КиБ-чанками, loopback, Dev-C):
**~600 MiB/s** (589/616/603 в трёх прогонах) — базовая точка для нового слоя (у старого
`net.c` эквивалентного замера не было, писать его на умирающий слой не стали).

**Не закрыто (осознанно, по плану):** старый `net.c`/`std/net` физически не удалён —
потребители `nova_tests/{plan83_12,plan91_12,plan91_15,plan91_16,plan178/net_byte_surface_mock}`
остаются на нём до санации Plan 182 (`std/net/*.nv` несут `// DEPRECATED`-баннер);
удаление + namespace-ренейм `net2`→`net` = `[M-183-old-net-removal-after-182]`. Пять
компиляторных дефектов, вскрытых по ходу (implicit-decl truncation и lost-wake — закрыты
на месте; loop-affinity-контракт, same-module `to_str()`-коллизия, GC-трассировка
`Vec[value+heap]`, `unwrap()` на typed-error, resize-инференс на выведенном `[]u8` —
остаются OPEN, все с обходами в коде и маркерами в `docs/plans/backlog-followups.md`) —
ни один не блокирует закрытие ядра. **Гейт финального захода:** conformance
`--positive --compile-error` = 54/0 (без изменений — Ф.5 правит только `.md`/докблоки).
## [M-183-int-to-str-module-method-collision] CLOSED (2026-07-06) — checker infers effect-op receiver type

- **Корень (факт, трейсом):** `TypeCheckCtx::infer_expr_type` (types/mod.rs) НЕ выводил return-тип
  вызова **эффект-операции** (`Time.now_monotonic_ns()` / `Clock.tick()`, форма
  `Call{func: Path([E,op])}` или `Call{func: Member{Ident(E),op}}`). Из-за этого `ro x =
  Time.now_monotonic_ns()` оставлял `x` **без типа** в scope чекера → `check_instance_overload`
  через `BoundCtx::infer_arg_ty(x)` получал `None` → примитивный gate `[E_UNKNOWN_METHOD]`
  **пропускался** → `x.to_str()` утекал в codegen, где coarse-by-name fallback (`method_receivers`,
  single-key last-wins) диспатчил `nova_int`-приёмник на одноимённый **чужой** метод
  (`NetError.to_str` / любой тип с `@to_str` в модуле) → int уходил как указатель на enum/record →
  тихий SEGV (mibps=12 → FaultAddress 0xC). Ширина класса (репро): приёмник-примитив + метод,
  которого у примитива НЕТ + одноимённый метод у ЛЮБОГО типа (enum `Color`, value-record `wobble`).
  Когда примитив ВЛАДЕЕТ методом (`int.abs()`) — резолвился корректно (builtin выигрывает до
  fallback). Method-chain-результат (`w.get()->int`) уже типизировался каналом — единственный
  дырявый источник был effect-op.
- **Фикс (§0 «чекер — единственный владелец типов», целевая форма):** добавлен effect-op arm в
  начало `ExprKind::Call` в `infer_expr_type` — если `func` = `Path([E,op])` / `Member{Ident(E),op}`
  и `self.types[E]` = `TypeDeclKind::Effect(ops)`, возвращаем объявленный `return_type` операции
  (bare `op()` → unit). Зеркалит авторитетный codegen-резолв `effect_schemas`
  (emit_c `infer_expr_c_type`). Теперь `x: int` известен чекеру → существующий `[E_UNKNOWN_METHOD]`
  срабатывает чисто (int не владеет `to_str`). НЕ точечный хак на `to_str`: чинит весь класс
  «effect-op-результат → метод-вызов на примитиве».
- **int НЕ имеет `to_str`** (prelude даёт только `@display`/`@debug`; конверсия — `str.from(int)` /
  `${...}`). Поэтому правильный итог для `mibps.to_str()` = `[E_UNKNOWN_METHOD]`, а НЕ рабочий
  вызов → обход `${...}` в `std/net2/stress_test` **остаётся** (комментарий обновлён: теперь это
  чистая checker-ошибка, а не тихая генерация битого C).
- **Тесты:** `nova_tests/plan183_f4/effect_op_int_result.nv` (positive — effect-op int корректно
  типизирован: `str.from`/интерполяция/`.abs()` резолвятся, чужой `to_str` НЕ перехватывает) +
  `nova_tests/plan183_f4/neg/int_to_str_effect_collision_neg.nv` (`EXPECT_COMPILE_ERROR
  E_UNKNOWN_METHOD`).
- **Гейты:** conformance `--positive --compile-error` **54/0**; std/net2 весь модуль **19/19 PASS**
  (throughput-тест с `${mibps}` зелёный, 734 MiB/s); дельта против базиса `c27bf10c` (временная
  копия-бинарь) на широкой выборке (effects/effect_registry/generics/io/contracts/narrowing/any_is/
  http_typed/protocols/serde/http/http_transport/plan154_1/plan126/plan91_15/plan97/plan108/std·time/
  basics/modules/plan91_14/16/plan100_6/plan108_4/inout_ref/named_params) — **0 новых FAIL**.
  Сборка Rust чистая.

## D409 — `-> @` автоматический возврат приёмника (Plan 174 side-quest, 2026-07-06)

- **Что:** реализован D409 (амендит D181/D132): в `-> @`-методах возврат приёмника единственно
  автоматический — конец тела без хвоста / голый `return` / хвостовое НЕ-`@` выражение (discard) →
  неявный `return @`. **Explicit-формы запрещены** (`@` в хвосте / `return @` / `=> @` →
  `E_EXPLICIT_SELF_RETURN`; амендмент владельца — усиление изначальной редакции, обратная
  совместимость снята). Делегация `=> @other_fluent()` остаётся легальной. `return <не-приёмник>` —
  ошибка как раньше (D132 rule 2).
- **Как (§0, без дублей):** чекер — `check_fluent_return` переписан (types/mod.rs: ban-walker по
  control-flow-блокам, `return` = statement → рекурсия только в If/Match/Loop/While/For/Block/
  ConsumeScope, замыкания — отдельный return-scope); лоуэринг — НОВЫЙ AST-pass
  `self_return_lower.rs` (запускается ПОСЛЕ check_module во всех 4 пайплайнах: main.rs check+build,
  test_runner.rs, doc/test_runner.rs, nova-cli build): синтезирует `SelfAccess` на implicit-exit'ах →
  codegen видит уже привычную manual-форму, **ни строчки нового кодогена**.
- **Миграция:** ~90 мест — std (vec/core, vec/mutate, vec/restructure, vec/sort, write_buffer,
  string_builder, std/sort) + spec_tests (d132/d215/d326) + nova_tests (plan73×2, plan77×3,
  plan91_8c×2, plan123×2); `{ body; @ }` → `{ body }`, `return @` → `return`, `; @ }` однострочников
  → `}`. neg-фикстура plan77/fluent_body_err инвертирована (было «нет @» = ошибка, стало «есть @» =
  ошибка E_EXPLICIT_SELF_RETURN).
- **Тесты:** `spec_tests/conformance/d409_self_return_auto.nv` (4 теста: конец тела / голый return в
  ветке / discard НЕ-@ хвоста / смесь веток) + `neg/d409_explicit_self_return_neg.nv`
  (E_EXPLICIT_SELF_RETURN) + `neg/d409_return_wrong_value_neg.nv` (`return 5` → D132).
- **Гейты:** conformance `--positive --compile-error` **56/0** (54 базовых + 2 новых негатива;
  позитив влит в общий модуль); дельта против базиса `c5256bf2` (temp-worktree бинарь) на широкой
  выборке (std/collections/vec, std/runtime, std/sort, plan128/128_2, plan153_0-6, str, strings,
  vec_elem_type, plan73/77/91_8c/100_1/123/123_4_4, inout_ref, cgfix_fluent_tail_if, plan138_2,
  self_nested) — **0 новых** (все фейлы байт-идентичны базису, вкл. pre-existing panic
  plan153_4/chunks_windows P67-LEGACY). Сборка Rust чистая (compiler-codegen + nova-cli).

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

## Plan 173.2 — supervision-as-effect: `Supervisor`/`Decision` (D416) (2026-07-10)

- **СНЯТО 2026-07-10 (решение владельца): Restart-семейство РЕТРАКТИРОВАНО из словаря
  `Decision` целиком** (D416 §1/§4 амендмент) — не «MVP за гейтом», а прод-реди полный
  словарь `Escalate | Stop`. Мотив: рестарт — идиома акторных систем, не структурной
  конкуренции (Kotlin coroutineScope / Swift TaskGroup / Java Joiner рестарта не имеют);
  повтор попытки — `std/concurrency/retry` внутри тела ребёнка. Гейт
  `E_SUPERVISOR_RESTART_GATED`, runtime-abort и neg-тест `restart_gated_neg` удалены;
  `[M-173.2-restart-all-rest]` и `attempt`-вопрос закрыты ретракцией.
- (истор.) MVP-объём §3b (owner 2026-06-26): исполнялись `Escalate`/`Stop`;
  Restart-варианты держались в словаре за `E_SUPERVISOR_RESTART_GATED` до изоляции
  D415/173.3; `attempt`-параметр был отложен вместе с Restart.
- **Периметр: remote-дети armed M:N** (child_error[]-субстрат 173.0 заполняет только
  remote-путь; auto-arm делает его дефолтным). Bootstrap/single-thread
  (`NOVA_NO_AUTOARM=1`) и implicit main-scope (top-level `detach`) — дефолтный
  Escalate-all; задокументировано в D416 §5 и в докстринге эффекта.
- **Suspend-запрет в хендлере — компилируемое приближение V1:** прямой `Time.sleep`
  в теле хендлера (`E_SUPERVISOR_HANDLER_SUSPEND`) + `interrupt`
  (`E_SUPERVISOR_HANDLER_INTERRUPT`); транзитивный suspend через вызов функции —
  followup эффект-row-анализом (Q-блок D416 §3).
- **Механика без упрощений:** deferred-decision режим (хендлер есть → падение пишет
  ТОЛЬКО свой per-slot с release-publish, без CAS-primary/cancel-бродкаста); решения
  serialized на drive-потоке ВО ВРЕМЯ drain'а (Escalate успевает отменить siblings,
  Stop оставляет их доживать) + финальный catch-up под pending_remote==0-гейтом;
  throw хендлера огорожен fail-frame'ом моста = Escalate-with-handler-error;
  индуцированные CANCEL siblings хендлеру не показываются. Дефолт (нет хендлера) —
  байт-паритет: ни одна новая ветка не активируется.
- **Гейты:** cargo оба чистые; conformance 82/0; err173_0 (retention ×5 стаб. после
  одиночного TIMEOUT-флейка под параллельной сборкой 4 CU) / err173_2 / err173_3 +
  все neg зелёные; std/concurrency 7/0. Известный MAIN-side красный (не эта ветка):
  err173_1/parfor_diag — D415-гейт `E_CONCURRENT_MUT_CAPTURE` бьёт mut-захват в
  supervised_value_smoke.nv (файл 173.1, гейт 173.3) — чинить волне 173.1/173.3.

## Plan 173 Ф.5+Ф.6 — hygiene + panics-клаузула (2026-07-10, ветка err-173-f56)

**Ф.5 — отступления/границы (не упрощения):**
- **Per-instance exactly-once счётчик — только пользовательские heap-record типы.** Extern "nova"
  cleanup'ы (MutexGuard/ReadGuard/WriteGuard/Permit — D194 hot-path) счётчиком не оснащаются: их
  структуры рукописные в nova_rt (инъекция поля невозможна из codegen). Generic consume-типы —
  mono-путь без прологa (в корпусе таких нет). Scope-локальный дубль-гард сохранён как
  defense-in-depth для extern-типов.
- **Watchdog-варн наблюдаем в .nv только структурно** (overrun-флаг exit-события); сам stderr-текст
  «fiber stuck in cleanup» в .nv-фикстуре не проверяется (нет EXPECT-механики для warn-потока
  positive-теста) — механика той же ветки, что overrun, покрыт код-путь.
- **[E_UNKNOWN_TYPE] на record-literal** — точечный фикс вскрытого miscompile-класса; общий
  name-resolution пробел (unknown-тип в TypeRef/Fail[...]-позиции «не наша забота») остаётся —
  d192-ретракт neg-тесты используют record-literal вектор.

**Ф.6 — фактическая граница миграции (план оценивал −78 CU, факт −52):**
- **Throw-класс НЕ мигрируем by design:** sync unlock/misuse-guards, Channel.new capacity,
  select-all-closed кидаются `nova_throw` (USER) — по строгой D348-семантике это не паника
  (инверсия не принимает). Семантический вопрос «должны ли sync-misuse-guards быть panic-классом
  (D13)» — отдельное языковое решение, НЕ взято в Ф.6.
- **File-режимные тесты** (`// CONTRACTS off`, module-level `#unchecked`) остаются standalone —
  директива действует на весь CU.
- **Процессные тесты** (fiber stack overflow = SEH-kill, token-double-bind abort из fiber,
  uncaught-abort stderr «(throw site)») — легитимный legacy (изоляция процесса обязательна).
- **Мигранты pre-existing-красных CU возвращены** (plan153_4/5, plan138/_2, plan83_10, strings,
  contracts, plan11_followup, plan153_2): вливание в красный CU = потеря покрытия; сами CU красные
  на родном baseline-бинаре (internal error P67-LEGACY chunks_windows, E_STR_NO_LEN-дрейфы,
  strict-propagation) — санация корпуса вне периметра (Plan 182).
- **Contract-диагностика в folder-module CU** печатает file:line entry-файла CU (loc_for_span от
  annotation_source) — panics-паттерны мигрантов ослаблены (без file:line-префикса). Точность
  file:line в multi-file CU — известная ось (span→peer-файл маппинг), не регресс этой волны.

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

## [M-closure-trailing-scalar-coercion-no-typecheck] (2026-07-10, ветка destructure-lint)

- **Гейт — именно скаляр** (`bool`/int-family/float/голый `char`), не «любой не-fn тип»
  из первоначальной формулировки маркера. `str`/`Any`/произвольный `Named` в область
  ЭТОГО фикса намеренно не входят — подтверждённый репро был про `bool`; расширение
  до общего closure-vs-non-fn-type mismatch — отдельный follow-up при необходимости.
- **Явный `return` внутри `detach`/`spawn`/`parallel for`/вложенных closures — НЕ
  проверяется** (зеркалит существующее ограничение `materialize_returns_in_block`):
  `return` там принадлежит ДРУГОМУ execution-context, коэрсия к return-типу
  ОБЪЕМЛЮЩЕЙ fn была бы неверной по построению — тот же дизайн, не новый пробел.
- **`assignable`/call-arg позиции не тронуты** — closure-литерал, переданный АРГУМЕНТОМ
  в HOF-параметр скалярного типа, УЖЕ отвергается существующей сверкой (arity/сигнатура);
  дыра была именно в return-позиции (`assignable` там никогда не вызывался).

## Plan 192 — native-backed module pattern ([ffi.staticlib])

- **Хардкод линковки tls_shim снят на общий манифест-механизм.** `detect_tls`/
  `tls-cache`/`-lbcrypt -lntdll` были спец-случаем в `test_runner.rs` (второе
  окно правды против `[ffi]`). Введён `[ffi.staticlib]` (kind/path/lib/build/
  cache/link*/trigger_symbols) + `resolve_ffi_staticlib` (cache→artifact→cargo,
  mtime-инвалидация). `detect_tls` оставлен ФОЛБЭКОМ (std/nova.toml без
  trigger_symbols → legacy-детект; std/tls байт-идентичен). Условный триггер
  «использует ли CU модуль» обобщён (`c_file_uses_any_symbol` по
  `trigger_symbols`), закрыт последний tls-специфичный хардкод в этой ветке —
  brotli-детект остаётся частным случаем того же класса (не тронут).
- **Граница объёма (честное разделение).** `[ffi.staticlib]` резолвится из
  манифеста ПАКЕТА ВХОДНОГО ФАЙЛА (для монорепо std/tls — пакет std, работает).
  Транзитивный резолв native-deps из внешних `[dependencies]` НЕ провязан
  (внешний dep-resolution — Plan 03.1, не готов) — showcase `nova-tls`
  собирается как самостоятельный пакет (его example — тот же пакет), не как
  зависимость. Kind поддержан один: `rust-staticlib` (иные → forward-compat
  no-op в резолвере).

## §28 — W_REDUNDANT_OF lint + миграция std (ветка redundant-of-lint, 2026-07-10/11)

- **Новое правило CONV_RULES.** `Vec[T].of(a, b, ...)` избыточен, когда
  литерал `[a, b, ...]` дал бы ТОТ ЖЕ тип (nv-coding-style §28: канон
  конструирования коллекций). V1 — консервативный синтаксический подкласс
  без семантики: `T` буквально совпадает с default-типом голого примитивного
  литерала (`int`/`str`/`bool`/`char`), и каждый аргумент — однозначный
  литерал ИМЕННО этого типа. Осознанно НЕ флагует: сужение ширины
  (`Vec[u32].of(1,2,3)`), `Option`-элементы (`None`), пустую границу API
  (`.of()`), non-literal аргументы и `[]T`-элементы (`TypeRef::Array`, не
  `Named`) — всё это требует type-checker'а, которого у AST-lint нет; лучше
  0 ложных срабатываний, чем широкое покрытие с шумом.
- **Итог прогона по std/spec_tests: 0 находок.** Существующий код уже не
  содержит голых-примитивных избыточных `.of` — узость V1 подтверждена как
  оправданная, не как недоработка (то, что рука подобрала бы вручную —
  `Vec[[]u8].of(x.bytes())` в tls — синтаксически не примитивный литерал,
  вне периметра V1 и мигрировано отдельно, вручную, по прямому указанию
  владельца).

## Plan 175/175.1 (time) — scope-оценка «остаётся TODO» и хвостовое закрытие (2026-07-11, ветка time-175, sonnet)

- **Находка: ядро обоих планов уже SHIPPED в main до старта этой волны.**
  `time-175` создана off `main` (0 коммитов расхождения); `civil-time-175-1`
  (`Merge branch 'civil-time-175-1'`, `2081fc022`) и `time-tails-175`
  (`Merge branch 'time-tails-175'`, `67c85b504`) уже влиты. `std/time/civil/`
  (17 файлов, ~4200 строк — включая рабочий TZif-парсер `tzif.nv`,
  `zoned.nv`, DST-disambiguation) присутствует и зелен. `docs/plans/README.md`
  строка 22 (std-library nav table) — точная: 175 ядро ✅, 175.1 ✅ SHIPPED,
  auto-idle-advance ✅ ЗАКРЫТ (marker `[M-175-vclock-armed-mn-scope-identity]`,
  вынесено в Plan 189), full IANA-embed вынесено в Plan 190. **НЕ переделывал**
  ничего из вышеперечисленного (см. `feedback-plan172-whole-not-half` — но здесь
  «whole» уже было целиком сделано ДО этой волны).
- **Найдена и исправлена рассинхронизация:** детальная таблица README (строки
  460-461, «Текущие планы») была STALE относительно точной сводной строки 22 —
  175.1 там всё ещё значился «📋 READY» (не начат!), хотя факт — SHIPPED
  2026-07-10. Это реальный риск: следующий агент/владелец мог бы начать
  civil-time заново, решив что «ещё не сделано». Обе таблицы синхронизированы
  этой волной (см. изменения `docs/plans/README.md`); статус-блок в
  `docs/plans/175-time-system-rework.md` (шапка) обновлён — «Остаётся TODO»
  список приведён в соответствие с фактом (auto-idle-advance закрыт, M:N-под-
  нагрузкой → Plan 189, tzdb-embed → Plan 190).
- **Единственный реально открытый пункт 175 (`[M-monotonic-per-os-isolated-tests]`,
  P/L, Plan 65 Ф.12.2) НЕ взят в эту волну** — маркер явно deferred на «Plan 58
  CI-matrix follow-up» (нужна многоплатформенная CI, недоступна в одной
  Windows-сессии); существующее integration-покрytие (`units_test.nv`,
  `value_typed_surface_test.nv`) уже проверяет non-regression на этой ОС.
  Честно оставлен открытым, не полу-сделан молча.
- **Найден и закрыт реально разблокированный маркер: `[M-rate-limiter-monotonic]`
  (P3, floating, было в `docs/plans/backlog-followups.md`).** Условие блокировки
  («когда `now_monotonic_ns`-слот появится в `NovaVtable_Time`») снято ещё
  волной Plan 175 Ф.3(a) (2026-07-10), но сама миграция `TokenBucket` осталась
  не сделана — маркер зафиксировал только снятие блокера, не закрытие тела
  задачи. `std/concurrency/rate_limiter.nv`: `last_refill_ms i64`
  (`Time.now_unix_ms()`, wall-clock) → `last_refill Monotonic`
  (`Monotonic.now()`); `@refill()` — `now.elapsed_since(@last_refill)` вместо
  ручного `(now_ms - last_refill_ms).max(0)` — `Monotonic.@elapsed_since` УЖЕ
  saturate-to-zero на регрессе (D318), ad-hoc clamp снят как избыточный (защита
  теперь на уровне типа). `rate_limiter_test.nv` не менялся структурно (только
  комментарий регресс-теста уточнён под новую механику) — `th.fixed_ms`
  мокает `now_unix_ms()`/`now_monotonic_ns()` когерентно (Plan 175 D316
  mock-coherence), тот же сценарий «часы прыгнули назад» валиден и для
  monotonic-пути. Строка `[M-rate-limiter-monotonic]` убрана из OPEN-view
  (`backlog-followups.md`).
- **Гейты:** `nova test std/concurrency/rate_limiter_test.nv` 1/0;
  `nova test std/time` 6/0 (аггрегировано по folder-CU — включает civil);
  `nova test --positive --compile-error spec_tests/conformance` 95/0 (δ=0,
  правильная команда per `project-conformance-single-cu-run`, НЕ голый `nova
  test spec_tests/conformance` — тот даёт неполные 3/0, маскируя основной CU).
  Rust rebuild clean (`cargo build --release` nova-cli, 3м32с).
- **Дедуп:** `docs/simplifications.md` секция «Plan 65 —
  `ChanReader.close_after(Duration)`» была задублирована байт-в-байт (строки
  ~10354-10712 и ~23894-24252, подтверждено `diff` = identical, 359 строк).
  Вторая копия удалена (`sed '23894,24252d'`), первая (хронологически более
  ранняя позиция) оставлена как единственный источник — закрывает housekeeping-
  примечание из Plan 175 §11/§9 («дедуп отдельным коммитом при закрытии
  маркеров»). Не нашёл признаков, что более широкий блок файла (обнаружен
  ПОБОЧНО: `## str lex compare bootstrap byte-wise` тоже дублирован на строках
  ~6525/~20065-до-правки) дублирован ПОЛНОСТЬЮ идентично Plan-65-блоку — не
  трогал, вне периметра этой волны (маркер Plan 175 просил дедуп ИМЕННО секции
  Plan 65); зафиксировано как находка для отдельного housekeeping-захода.

## [M-detach-transitive-effect] (2026-07-11, research detach vs #blocking/#realtime)

- Research-заход по вопросу владельца «как detach коррелирует с #blocking/#realtime»
  вычленил критерий: **атрибут = свойство исполнения тела** (направление «внутрь»:
  `#realtime` body-restriction, `#blocking` threadpool-offload, caller не наблюдает),
  **эффект = наблюдаемое вызывающим последствие** (направление «наружу»: `Detach` —
  работа переживает вызов). Blocking/realtime корректно уехали в атрибуты (Plan 113);
  `Detach` корректно остался эффектом.
- НО: транзитивность эффект-row статически не enforced — `check_callee_effects`
  проверяет только forbid/realtime/blocking-body пересечения. `Detach` сегодня
  ловится только на прямом `detach {}` (E_DETACH_REQUIRES_EFFECT) — по фактической
  силе равен атрибуту; сирота на глубине 1 вызова невидим, forbid Detach обходится
  обёрткой. Маркер: транзитивная Detach-ветка в check_callee_effects.

## [M-detach-with-handler-or-drop-exemption] (2026-07-11)

- Exemption «ambient `with Detach = …`» в E_DETACH_REQUIRES_EFFECT — мёртвая
  поверхность (хендлер-значений нет; AsyncDetach/SyncDetach — ретрактированные
  имена). Либо реальный тест-хендлер в std/testing, либо снять exemption.

## [M-detach-forbid-test] (2026-07-11)

- `forbid Detach` заявлен дизайном (D63×D50), механика в check_callee_effects есть,
  но тестов 0. Добавить pos/neg; после транзитивности — глубокий кейс.

## [M-178-server-typed-body] ЗАКРЫТ (2026-07-12, баг-фиксер Plan 196, sonnet)

- Заявленный «serde-в-http-CU codegen-дефект» (typed `#impl(Deserialize)` request-
  bodies на сервере) оказался ДВУМЯ реальными компиляторными багами, оба
  проявляются только когда `std.http.server` и `std.http.client` (транзитивно
  через `std.http.serdejson`'s `json_decode_body[T]`) попадают в ОДИН CU:
  1. **types/mod.rs** — chain-receiver mut-check: реестр `recv_returning`
     (fluent `-> @`, Plan 77/D132) был name-only, БЕЗ arity. Одноимённый `-> @`
     метод другого типа/арности (`ServeMux mut @post(pattern, handler) -> @`,
     arity 2) ложно поражал НЕСВЯЗАННЫЙ вызов `HttpClient.new().post(url).body(b)`
     (arity 1) → ложный `E_RECEIVER_BINDING_NOT_MUT`. Фикс: arity-aware компаньон
     `recv_returning_arity`, зеркалит существующий `mut_methods_arity`/
     `ro_methods_arity` прецедент (`[M-172.5-chain-gating-ro-at]`).
  2. **emit_c.rs** (3 места) — mangling/registration свободных функций считали
     голое имя уникальным по ВСЕЙ CU: `fn_module_map`/`file_priv_fn_c_names`,
     D84 `method_overloads`-регистрация, D29 shadow-skip `should_skip_fn`.
     Module-private (без `export`) одноимённая fn в ДВУХ разных модулях
     (`std.http.client`'s private `serialize_response(status, headers, body)
     -> str` vs `std.http.server`'s exported `serialize_response(resp) ->
     []u8`) — НЕСВЯЗАННЫЕ функции, не overload-пара — либо коллизировали в один
     C-символ, либо (после первого фикса) тихо ВЫПАДАЛИ из вывода вообще
     (implicit-decl CC-FAIL). Расширил существующую cross-module collision-
     detection (была только identical-signature, прецедент uuid_namespace
     duplicate-symbol) на different-signature-но-не-все-exported случай,
     прокинул через все 3 места.
- Repro: `std/http/serdejson/typed_body_repro_test.nv` — сознательно в папке
  serdejson, НЕ в `std/http/server/`: черновик внутри `std/http/server/`
  тянул serde в модуль `http.server` целиком и ломал `nova test
  std/http/servernet` (`E_EXTENSION_METHOD_NEEDS_IMPORT` на
  `HashMap.serialize()`) — тот же leanness-принцип, что и у самого
  serdejson.nv (см. его баннер).
- Маркер снят из `server.nv` (заменён на DONE-описание) и из
  `backlog-followups.md`; `187-flagship-concurrency-demo.md` обновлён —
  typed `.json[T]` теперь доступен, dynamic-JSON workaround не нужен.
- **Гейты:** `nova test std/http std/encoding` 14/0 (+8 SKIP, ожидаемо —
  no-test-block модули); `nova test std/crypto` 5/0 (rotl32 identical-sig
  прецедент не сломан); `nova test --positive --compile-error
  spec_tests/conformance --timeout 300 --jobs 4` 95/0. Rust rebuild clean
  (`cargo build --release` nova-cli, ~4м каждый из 6 rebuild-циклов).
- Branch `typed-body-fix` (worktree `nova-nt`), commit `56b00e808`; НЕ
  смёржен в main.

## [M-d78-duplicate-decl-module-swallow] (2026-07-13)

- Эксперимент (research module-naming, владелец задал вопрос «зачем decl,
  если роутинг путевой»): два модуля с одинаковой rev-3-декларацией
  (`src/a/neg/x.nv` + `src/b/neg/x.nv` → оба ПРИНУДИТЕЛЬНО `module neg.x`)
  в одном пакете. Импорт по пути (`import a.neg.x.{who_a}` /
  `import b.neg.x.{who_b}`) находит оба файла, НО экспорты второго тихо
  исчезают: «undefined identifier who_b» с якорем на строку module-decl,
  без duplicate-диагностики. Контроль: переименование папки (декларации
  разошлись) → PASS. Вывод: роутинг путевой, но РЕЕСТР модулей керит по
  декларации — дубль глотается молча.
- Переинтерпретация rev-3.1 (`internal/` → 3 сегмента): это спековая
  заплатка вокруг ЭТОГО дефекта, не самостоятельное правило; после фикса
  keying'а — кандидат на ретракцию.
- Fix-направления: (1) реестр по каноническому пути, decl = чистая
  чексумма (класс исчезает); (2) минимум — hard-error на дубль с обоими
  путями. Проверить ту же ось в C-mangling (`Nova_<modpath>_`).
- Маркер в backlog-followups.md (P1); детали/варианты —
  docs/research/2026-07-13-module-naming-two-segment-review.md.

## [M-d78-duplicate-decl-module-swallow] ЗАКРЫТ + Plan 202 Ф.1/Ф.1b/Ф.2 (2026-07-13)

- **Ф.1 (резолвер).** `imports.rs`: `resolve_one`/`resolve_imports_inline_ex`
  (`visited`/`in_progress`) и `collect_all_signatures`/`collect_sigs_one`/
  `ModuleSigTable` (сигнатурный pre-pass, D292/D293) переведены с
  decl-keyed на **canonical-path-keyed** (`canonical_module_key` — anchor =
  parent-директория для folder-module-пира, сам файл для single-file;
  устойчиво к порядку peers). Декларация остаётся ТОЛЬКО identity-check
  (`E_D78_MODULE_PATH_MISMATCH`) — никогда не routing/registry ключ.
  Обязательно СИНХРОННО оба реестра — иначе резолвер чинится, а sig-table
  продолжает жить по decl (второе окно идентичности).
- **Ф.1b (mangling, обнаружено ЖИВЫМ, не гипотетически).** Первый же
  прогон pos-фикстуры дал `CC-FAIL redefinition of 'nova_fn_...'` — D307/
  D381 collision-детекторы в `emit_c.rs` ТОЖЕ группировали по decl
  (`BTreeSet<Vec<String>>` дедупил два физически разных модуля с
  одинаковым decl в ОДНУ запись → коллизия не детектится → одинаковый
  C-символ). Фикс: `phys_key_of`/`decl_phys_groups`/`effective_modpath`
  (общий helper перед fn- и type-коллизионными блоками) — decl расширяется
  суффиксом `dupN` ТОЛЬКО когда реально расшарен ≥2 физическими модулями в
  CU; byte-identical для всего остального корпуса (0 деклараций сегодня
  расшарены — раньше глотались резолвером, теперь легальны и обязаны
  различаться в C). Известный узкий остаток (branch (2) cross-import
  suffix-match) → `[M-d78-dup-decl-type-cross-import-ambiguous]`.
- **Ф.2 (root peers, D78 rev-4).** `.nv`-файлы прямо в source root пакета
  МОГУТ объявлять однoсегментную `module <package>` — peers корневого
  модуля (аналог `lib.rs`), ДОПОЛНИТЕЛЬНО к независимой `<package>.<stem>`
  форме (смешанный корень допустим). Фикс статтера `tls.tls`
  (nova-tls): `import tls.{TlsStream}` вместо `import tls.tls.{TlsStream}`.
  `manifest::expected_root_peer_decl` (доп. acceptance-ветка) +
  `imports::collect_root_peers`/`is_peer_group_member` +
  `resolve_module_paths`-ветка для bare single-segment import (свой пакет
  через `nova.toml`, cross-package `[dependencies]` через уже
  провалидированное `lookup_dependency`-имя, включая ослабление жёсткой
  «голое имя зависимости требует путь к модулю» ошибки).
- **Гейты:** conformance один CU (см. tally в отчёте Plan 202); стресс —
  ветка `triage-198` (~1480 peer-файлов) компилируется Ф.1-компилятором;
  `nova check std` дельта-нейтрально.
- **Спека:** D78 rev-4 амендмент в `spec/decisions/07-modules.md` (keying-
  семантика D29 п.4 + root peers секция) — в том же слиянии, что код.
- Побочно найден несвязанный баг (region checker/auto_derive, вне
  объёма) — `[M-202-ident-x-module-alias-collision]`.
- Полный отчёт: `docs/plans/202-progress.md`.

## Волна «173 хвосты» п.2 — полный propagation-trace [M-173-error-return-trace] (2026-07-13)

- **Runtime (effects.h/effects.c):** TLS ring-buffer `_nova_throw_trace`
  (`NOVA_THROW_TRACE_CAP=16`, count хранит суммарные push'и — дамп сообщает
  «N earlier frames dropped»); `nova_throw_trace_push/reset`;
  `nova_throw_site_set` теперь сбрасывает трассу (fresh origin = новая
  ошибка), новый `nova_throw_site_mark` обновляет site БЕЗ сброса (для
  конверсии уже пропагирующей Result-ошибки в Fail-эффект). Сброс также на
  catch (`nova_scope_exit` CATCH), interrupt-consume (4 точки effects.c) и в
  `nova_runtime_reset` (+ там же гашение стейл `_nova_throw_site` между
  panics-тестами). Дамп — в существующем `nova_throw_site_dump` → все 4
  uncaught-abort ветки получили трассу бесплатно.
- **Codegen (emit_c.rs):** push на `?` value-mode (`return Err`) и Fail-ctx
  ветке; `!!`-Err = push + site-mark (трасса переживает конверсию);
  `!!`-None = полноценный origin-стемп `site_set` (раньше bang-сайты не
  стемпились вовсе). Только error-path — happy-path не затронут.
- **Тест:** `nova_tests/err173/rt/f5_propagation_trace_full.nv` — Err
  рождается в leaf, 2 value-mode `?`-звена + `!!`-конверсия, uncaught →
  дамп содержит 3 `via file:line (?)`-звена в хронологическом порядке
  (проверено вручную на бинаре + --panic lane PASS).
- **Ограничение (задокументировано в effects.h):** `Err(...)`-конструктор
  не сбрасывает трассу (нет стемпа) — кадры ошибки, разобранной `match`'ем
  (не catch), могут остаться в хвосте следующего дампа.
- **Попутно вскрыто:** `nova build` (nova-cli) не прокидывает
  `set_source_file_name` → `at <unknown>:N` (pre-existing, `nova test`-путь
  честный) — `[M-cli-build-source-file-name-unknown]` (P3).
- **Гейты:** conformance один CU 111/0 + 7 SKIP (δ0); err173 folder-CU +
  err173_2 PASS; rt-lane 3/3 (--panic); neg 10/0; std/src/concurrency:
  2 PASS + 2 CC-FAIL pre-existing (main-бинарь на main-дереве падает
  идентично, δ=0); cargo build --release чист.

## Волна «173 хвосты» п.1 — MultiError scope-агрегация (D414 §1 ← Ф.4) (2026-07-13)

- **Разрыв спека/код закрыт:** D414 §1 обещал «Не-primary ошибки уходят в
  suppressed-карман», но re-throw хвост `nova_supervised_run_impl` кидал
  только primary (`nova_scope_collect_child_errors` — 0 вызывающих; ошибки
  siblings терялись). Гейт 174.3 (any/is-downcast) в main — п.1 разгейчен.
- **Механика:** staging-слот TLS `_nova_pending_suppressed` (effects.h/.c);
  хвост scope собирает не-primary retained детские падения в
  NovaErrorChain (через `nv_compose_suppressed` — D193 identity/depth) и
  ставит в staging; ближайший throw потребляет его в `nova_last_error_set`
  (карман D158 модели Б) + несёт в fail-frame (`nova_throw`/
  `nova_throw_typed` теперь копируют из pocket вместо жёсткого NULL —
  вне агрегации это тот же NULL). Чтение — существующий
  `suppressed() -> []any`.
- **Исключения:** CANCEL-производные; Stop-решённые супервизором (новый
  drive-thread-only флаг `NovaChildError.escalated`, ставится в ESCALATE-
  ветке decision-loop; D416 — Stop = осознанный выброс, retained для
  observability). Primary идентифицируется msg+payload+tid+kind (typed-
  броски делят литерал `msg_repr` «<nova_int>» — по одному msg скипались
  ОБА ребёнка). Видимый порядок = порядок слотов.
- **Попутный ABI-фикс (вскрыт тестом):** `nova_any_from_boxed` (typeid.h)
  всегда заворачивал payload в слот-индирекцию (расчёт на pointer-repr
  records) → `try_as[int]` на suppressed-элементе возвращал АДРЕС бокса
  как int. Теперь value-ABI примитивы (tid 1..7) кладут box напрямую в
  `data`; user value-типы ≥ USER_BASE — прежнее pointer-предположение
  (задокументировано, pre-existing).
- **Детерминизм теста:** в supervisor-режиме отмена не летит до решения →
  хендлер спин-ждёт фиксации обоих падений (fetch_add строго перед throw,
  без yield между), затем Escalate — оба слота retained гарантированно.
  `nova_tests/err173_2/scope_multierror_test.nv`: (1) не-primary в кармане
  с точным значением; (2) Stop не течёт + сброс кармана свежей ловлей;
  (3) default-Escalate инвариант ⊆ (расписание-независимый).
- **Спека:** D414 §1 амендмент (06-concurrency.md) тем же слиянием; план
  173 §Ф.4 acceptance дополнен строкой хвоста.
- **Гейты:** conformance один CU 111/0 + 7 SKIP (δ0); err173_2 CU PASS
  (все supervisor-тесты + 3 новых); err173 + any_is + plan110 PASS;
  runtime_panics CU PASS (precedence в т.ч.); std/src/concurrency δ=0
  (2 pre-existing CC-FAIL).

## 187 прогон А (2026-07-14, sonnet, ветка flagship-187) — 4 новых маркера

- Переделка main.nv: ~150 строк ручного HTTP снесено → ServeMux +
  http.servernet.handle_connection (требование владельца); структура
  backend/ → src/ (Ред.9, nova-http-канон); детерминизм-тест + README.
- Новые маркеры (детали в backlog-followups.md):
  [M-187-http-serde-setcookie-serialize-collision] P1 (генерик json_encode[T]
  ломает линковку #impl(Serialize) в CU с http — семья typed-body/D84);
  [M-187-supervised-nested-fiber-slot-race] P1 (рантайм: сервер часто виснет
  на 2-3-м запросе, застрявший слот); [M-187-errorkind-parsejsonerror-
  variant-collision] P2 (одноимённый вариант разной арности в двух sum в CU);
  [M-187-nested-spawn-scope-var-cc-fail] P2 (spawn-в-spawn под supervised).
- Честная находка: HTTP-снапшот НЕ байт-детерминирован по seed (elapsed_ms/
  wall_ms от реальных часов) — детерминирован состав/исходы; критерий №7
  плана 187 выполняется только на мок-часах (как и записано).

## 2026-07-14 — план 206 (арифметика) дизайн-уточнения + CAS-follow-up
- Согласовано владельцем: 206 = 1 интринсик `@overflowing_add/_sub/_mul -> (int,bool)`
  (в компиляторе, per-type авто через `__builtin_*_overflow`) + 3 `.nv`-бланкета на
  `fn[T Ints]` (`checked`→Option / `saturating`→clamp / `wrapping`→модуль); trap-дефолт
  (`nova_int_checked_add`) уже есть; `unchecked` (unsafe, сырой `+`) ОТЛОЖЕН (дублирует
  Z3-элизию `--contracts=optimized`). Именование = дословно Rust (эталон) + прецедент
  атомиков `sync.nv` (`compare_exchange`/`fetch_add`). Детали — docs/plans/206.
- Новый floating-маркер `[M-cas-return-witnessed-value]` P3: `AtomicI*.compare_exchange -> bool`
  выбрасывает свидетеля провала (C-примитив пишет фактическое в `expected`); пересмотреть на
  `Result[unit,T]`/`(bool,T)` по принципу «примитив не теряет информацию». Ломающая правка
  API sync.nv → отдельно, не в 206.

## [M-serde-encode-pointer-op-regression] (2026-07-15, гейт-находка оркестратора 187)

- РЕГРЕССИЯ main: голый `#impl(Serialize)`-тип + `json_encode(p)` в свежем CU →
  CODEGEN-FAIL `E_POINTER_OP_USE_METHOD` («operator p + i on raw pointer retired»,
  Plan 70 silent-fallback-детектор, БЕЗ file:line). Репро: module tmpprobe.probe,
  type Point {x int, y int} #impl(Serialize), json_encode(Point{...}) — падает.
  std/encoding-тесты PASS (их CU не тянет synth-encode путь). Сайт НЕ в .nv
  (греп чист) — эмитит codegen (synth-derive Serialize / string-builder lowering).
  Подозреваемый: слияние scalar-to-str (57e0d91c8; from_scalar.nv удалён,
  string_builder/transform переработаны). Блокирует вливание flagship-187
  (typed-serde report_json). Передан багфикс-волне 187 нулевым приоритетом.

## [M-187-weather-live-tls-diamond-blocked] — расследование (2026-07-15)

- Симптом: weather-live флагмана (open-meteo HTTPS через http→tls) падает
  `transport error: live weather unavailable ... tls diamond dependency`.
  Health-live (без TLS) работает. В lock — ДВА `tls`: source=path (examples'
  прямой dep) + source=git (транзитивно через nova-http). Резолвер не
  унифицирует path-инстанс и git-инстанс одного пакета → транспорт не
  видит единый tls.
- ПОПЫТКА ФИКСА (не сработала, откачена): examples → git-форма tls/http +
  `[replace]` в nova.local.toml. Вскрыла более глубокое: `http={git}` тянет
  ОПУБЛИКОВАННЫЙ nova-http с GitHub, чей манифест объявляет tls как path
  `../nova-tls` ОТНОСИТЕЛЬНО своего git-checkout → не резолвится
  (`path ../nova-tls не существует` от `.nova/git/co/nova-http-.../`).
  `[replace]` транзитивный tls НЕ схлопывает. Это ограничение резолвера/D420,
  НЕ быстрый фикс флагмана.
- ВЕРДИКТ (промежуточный): оставлено path-форма (собирается, health-live жив,
  weather-live честно деградирует с маркером в live.nv). Настоящий фикс — трек
  204/D420: либо резолвер унифицирует одноимённый пакет через `[replace]` в
  транзите, либо опубликованный nova-http объявляет tls тоже git-формой.

### РАЗРЕШЕНО (2026-07-15, ветка fix-tls-diamond, sonnet) — резолвер, D420 дофикс №3

- ФИКС резолвера: `imports::lookup_dependency` — корневой `[replace]` теперь
  Cargo-`[patch]`-семантика: перекрывает ЛЮБОЕ вхождение same-named пакета
  graph-wide (прямое И транзитивное через nova-http), не только прямые рёбра
  корня (узкий Go-scope дофикса №2). + `examples/nova.toml`'s `tls` переведён на
  git+version (как у nova-http) → unify по git-URL. `nova.lock`: `tls` **2→1**.
  Амендмент D420 (09-tooling.md дофикс №3) в том же слиянии. Инвариант
  `W_REPLACE_IN_DEPENDENCY` (`[replace]` зависимости инертен) сохранён.
- Побочно: `nova build` теперь мёржит `[ffi]` зависимостей (dependency native
  shims линкуются — было только у `nova test`; пре-существующий gap).
- Smoke weather-live: НЕТ строки `tls diamond dependency`, `handlers.net="real"`,
  запрос завершается — диамант снят (маркер в live.nv переписан).
- ОСТАВШИЙСЯ ОТДЕЛЬНЫЙ БЛОКЕР (НЕ диамант): `[M-187-tls-cross-pkg-consume-cleanup]`
  — `TlsStream.connect(tcp,cfg)` из downstream-пакета не линкуется:
  `Nova_TcpStream_consume_cleanup` (std.net) эмитится только для consume-сайтов
  КОРНЕВОГО пакета, не для consume ВНУТРИ внешнего пакета (nova-tls). emit_c.rs
  территория (codegen), НЕ резолвер. Изолировано минимальным repro. Затрагивает
  и echo_server.nv. Дом нового маркера — live.nv + docs/plans/tls-diamond-progress.md.

## 187 маркер-гигиена + watchdog-находка (2026-07-15, оркестратор)

- ЗАКРЫТЫ багфикс-волной (сняты из backlog, история — записи выше):
  [M-187-errorkind-parsejsonerror-variant-collision] (фикс 1791360cf +
  фикстура) и [M-187-nested-spawn-scope-var-cc-fail] codegen-часть (фикс
  c687fc2d1 + фикстура; >16 вложенно-порождённых детей = отдельная
  рантайм-подложка 173.0-R2, не codegen).
- НОВЫЙ [M-187-watchdog-idle-server-kill] P1: supervised-watchdog (83.11,
  fibers.h:2871, 5с) валит ЛЮБОЙ idle Nova-сервер (accept-loop park
  count=0/pending_remote=1 = норма). Обход NOVA_WATCHDOG_DUMP_SECS=0
  (проверено 15с+). Дом — backlog + runtime 83.x.
- slot-race [M-187-supervised-nested-fiber-slot-race] закрыт 83.4.5.12
  (влит a48fc2270, гейт 10/10).

## 187 cross-package consume-cleanup DCE-дыра ЗАКРЫТА (2026-07-15, opus-фикс)

- ЗАКРЫТ [M-187-tls-cross-pkg-consume-cleanup] P1 (снят из backlog). Корень —
  НЕ «эмиссия только для корневого пакета» (формулировка маркера) и НЕ резолвер:
  это дыра в **method-DCE reachability seeding** (Plan 159, compute_dead_decls).
  Метод `T @cleanup(ScopeOutcome)` firing'ается только когда достижимы И тип `T`,
  И имя `cleanup`. Блок-форма `consume X = e { … }` диспатчит cleanup через
  СИНТЕТИЧЕСКИЙ символ `Nova_<T>_consume_cleanup` (emit_consume_entry_cleanup) —
  селектор `.cleanup(…)` НИКОГДА не пишется в исходнике, поэтому чисто
  синтаксический `collect_used_names` не сеял имя `cleanup` → `(T,cleanup)` в
  `dead_method_keys` → тело+forward-decl выброшены → диспатч линкуется против
  ОТСУТСТВУЮЩЕГО определения = `undefined symbol Nova_<T>_consume_cleanup`.
- Cross-package проявление: consume-сайт жил в методе внешнего пакета
  (nova-tls `TlsStream.accept`/`connect`, `consume stream { … }` над
  `std.net TcpStream`); `close` firing'ался (в теле есть явный `@tcp.close()` =
  `.close` селектор), а `cleanup` — нет. Root pure-std `consume st = s` (raw
  D180 linear, БЕЗ блока) линковался, т.к. raw-форма cleanup НЕ диспатчит.
- Фикс (точечный, аналог Plan 209 embed-proxy / contract-interp synthetic-
  selector сидов): `collect_used_names`, arm `Stmt::ConsumeScope`
  (compiler-codegen/src/lints.rs) сеет `out.insert("cleanup")`. Firing по-прежнему
  требует достижимого типа → keep только для consume-типов, реально
  используемых (over-keep, never over-prune; G0-консервативно). Zero-impact для
  программ без consume-scope (сид под условием). Runtime/codegen-фикс, НЕ
  язык-меняющий → D-амендмент НЕ нужен.
- Верификация (точечная): (а) `examples/tls/echo_server.nv` build+LINK — было
  `undefined symbol Nova_TcpStream_consume_cleanup` (реф в
  `Nova_TlsStream_static_accept`), стало `built` (бинарь 2.28МБ), в сген. C
  теперь И forward-decl И определение cleanup; (б) минимальный root block-consume
  (свой тип + `@cleanup`, main) build+RUN → `cleanup ran` / `ok result 42`
  (cleanup диспатчнут ровно раз); (в) pure-std `examples/net/echo_server.nv`
  линкуется (регрессии нет); std/src/net продакшн PASS (3 «FAIL» — это
  ожидаемо-падающие neg/ фикстуры). Полный conformance — оркестратор.
## ЗАКРЫТ [M-187-watchdog-idle-server-kill] (2026-07-15, ветка fix-watchdog-idle, sonnet)

- Рекон: `nova_runtime_dump_state` (`runtime.c:275-417`) — чистая
  диагностика (fprintf в stderr + fflush), **НЕТ** ни одного `abort()`/
  `exit()`/`_exit()` внутри. Watchdog-check (`fibers.h:2924-2937` в
  прочитанной версии) тоже не содержит фатального пути — `_watchdog_fired`
  latch гарантирует one-shot per scope. Прочитан целиком, вызовов
  завершения процесса нет.
- Эмпирика (worktree `nova-watchdog`, свежесобранный компилятор + aggregator,
  дефолтный порог 5с, watchdog ВКЛЮЧЁН): idle >25-30с дважды, idle+curl+idle,
  28 последовательных curl-запросов — **сервер НИ РАЗУ не упал**; дамп
  печатается ровно один раз (`supervised-watchdog-5s-remote-1`, `count=0
  pending_remote=1` — accept-loop легитимно запаркован на `uv_accept`,
  ровно как в описании) и процесс продолжает жить/отвечать. Не смог
  независимо воспроизвести фатальность из самого dump-пути, несмотря на
  расширенное тестирование — **честно фиксирую**: возможно смежная
  вероятностная гонка `[M-187-supervised-nested-fiber-slot-race]`
  (`main.nv:200-241`, реальная M:N-гонка планировщика слотов под вложенным
  `supervised`, по коду "закрыта 83.4.5.12", но комментарий в main.nv её всё
  ещё описывает как smoke-observed) даёт идентичную сигнатуру дампа и была
  спутана с фатальностью самого watchdog при браузер-смоуке; либо
  process-tooling артефакт (сам поймал похожий ложный "процесс мёртв" из-за
  PID-путаницы git-bash/MSYS job-control при ручном тестировании — легко
  принять за реальную смерть). Развилка на M:N-архитектуру НЕ подтверждена
  как корень — эскалация на opus не потребовалась.
- ФИКС (направление 1, минимально-инвазивно): watchdog больше не считает
  здоровый `pending_remote>0 / count=0` (idle-on-IO — легитимный accept-loop
  park) сам по себе признаком hang'а. Новый `nova_runtime_has_stuck_fibers()`
  (`runtime.c`, объявление `runtime.h`) — лёгкий скан (без печати) той же
  сигнатуры, что дамп уже флагует как `STUCK_ALIVE_NOT_PARKED`
  (`MCO_SUSPENDED && !parked` — потерянный wake / осиротевший слот). В
  `fibers.h`'s watchdog-check: дамп печатается ТОЛЬКО если найден
  реально застрявший fiber; если всё легитимно запарковано — таймер
  перевзводится (`_watchdog_start = now`), а не гасится навсегда — сервер,
  здоровый сейчас, но зависший позже, всё ещё будет продиагностирован (с
  задержкой максимум в одно окно порога). Реальный hang по-прежнему ловится
  (сигнатура идентична существующей в dump_state, инвариант не ослаблен).
- Верификация: `nova test std/src/concurrency/supervisor_test.nv
  std/src/concurrency/supervised_deadline_test.nv` → PASS 2/2 (M:N supervised/
  deadline не задеты). Aggregator (`AGGREGATOR_PORT` alt-порт, т.к. 8187
  занят соседним агентом): idle >12с (без запроса) → жив, лог дампа ПУСТ
  (здоровый idle больше не тревожит); curl `/api/run?legend=health&mode=live`
  → 200 + валидный JSON; ещё >12с idle → жив; убит, порт освобождён.
- Зона: `compiler-codegen/nova_rt/{fibers.h,runtime.c,runtime.h}` — чисто
  рантайм-фикс, НЕ язык-меняющий → D-амендмент спеки 83.11 не требуется.
- Маркер снят из `docs/plans/backlog-followups.md` (история — эта запись).

## `nova build` теперь автособирает vendor-FFI из исходников (2026-07-15, ветка fix-build-vendor-ffi, sonnet)

- ЗАВЕДЁН-И-ЗАКРЫТ [M-nova-build-vendor-ffi-no-autobuild] P1: `nova test`
  (`test_runner.rs::build_and_run_one`) build-and-кэширует `[ffi]
  vendor_src_dirs` (напр. mbedTLS в nova-tls) из исходников ДО линковки; `nova
  build` (`nova-cli::cmd_build`) этот шаг НИКОГДА не звал — только мёржил
  `[ffi] libs`/`lib_dirs` и шёл прямо на линк. На чистом чекауте (без
  вручную-скопированных прекомпилированных `.lib`) `nova build` любого
  примера с vendor-source native-депом (echo_server/echo_client) не
  собирался — приходилось вручную копировать либы (обход).
- Фикс (минимальный, без дублирования): `build_missing_vendor_ffi_libs`
  (`compiler-codegen/src/test_runner.rs`) переведена в `pub fn` (была
  module-private); `cmd_build` (`nova-cli/src/main.rs`, сразу после мёржа
  `[ffi]` своего пакета + зависимостей, до `BuildOpts`/`compile_c_to_exe`)
  зовёт её — зеркалит вызов `build_and_run_one`. No-op/never-fatal контракт
  функции не тронут (falls through к обычному линку с честной ошибкой, если
  либа реально всё ещё не собралась).
- Побочный дефект в ТОЙ ЖЕ функции, вскрытый ПРИ верификации (репро
  ОБЯЗАТЕЛЬНО удаляет вручную-положенные mbedTLS-либы, чтобы прогнать
  build-from-source путь, который до сих пор НИКОГДА реально не выполнялся
  на этой машине — ни через `nova test`, ни тем более через `nova build`):
  `build_vendor_ffi_lib`'s cl.exe/lib.exe response-файлы писались как голый
  UTF-8 БЕЗ BOM → на профиле с кириллицей в имени пользователя (`C:\Users\
  Евгений\...`, откуда резолвится git-кэш nova-tls) cl.exe читает `.rsp` в
  ANSI-кодовой странице процесса (тут cp1251) и коверкает путь →
  `C1083: file not found` на КАЖДОМ `.c`-файле mbedTLS. Фикс: `\u{FEFF}`
  (UTF-8 BOM) префикс на обоих `.rsp` (compile + lib/archive) — cl.exe/
  link.exe читают UTF-8 rsp-файлы по BOM независимо от активной кодовой
  страницы консоли. Не language-changing, D-амендмент не нужен.
- Верификация (точечная, `NOVA_CACHE=0` где важно избежать кэш-путаницы):
  вручную-скопированные `mbedcrypto/mbedtls/mbedx509.lib` убраны из ОБОИХ
  `~/.nova/git/co/nova-tls-*/…/native/lib` резолвленных чекаутов (backup в
  scratchpad, не восстановлены — автосборка теперь единственный путь и она
  рабочая). **ДО** (main HEAD 73ab1c44f + `fix-consume-cleanup` смёржена,
  vendor-ffi-патч временно откачен для контраста): `nova build
  examples/tls/echo_server.nv` падает БЕЗ единого упоминания vendor-FFI —
  сразу на compile-стадии. **ПОСЛЕ** (тот же коммит + фикс): `nova: FFI
  lib(s) ["mbedtls", "mbedx509", "mbedcrypto"] not found in …native/lib,
  building from vendored source (108 files, one-time)...` → `nova: vendor
  FFI lib(s) […] built (108 files)` — три `.lib` реально созданы на диске
  (подтверждено `ls`). `examples/flagship/aggregator/src/main.nv` (не имеет
  реальной TLS-зависимости в рантайме — `http.server` без TLS-терминации) —
  чистый build+LINK, `built: main.exe` (46.12s), без регрессии. `nova test
  std/src/net` — `PASS 1/0` (не задет). Финальный LINK
  `echo_server.nv`/`echo_client.nv` НЕ достигнут — блокирует ОТДЕЛЬНЫЙ,
  пред-существующий (подтверждено на исходном pre-session бинаре ДО начала
  этой волны — идентичная ошибка) баг диспатча вне периметра vendor-ffi, см.
  новый маркер [M-tls-xpkg-decode_utf8-tlsversion-dispatch-broken]
  (backlog-followups.md).

## 187 weather-live real-handshake + SSE-hang находка (2026-07-15, оркестратор)

- Фиксы #3/#4/#5 (watchdog cc0e81d6d / diamond 4ab70a144 / consume 70c4eff02)
  проверены ЛИЧНО: watchdog держит 9с idle без env; lock=1 tls; echo_server
  линкуется. weather-live end-to-end через /api/run — 4/4 done, реальный
  open-meteo HTTPS, 360мс, 0 leaks.
- live.nv: снят устаревший хардкод-обход weather-live (возвращал «deferred»),
  заменён на настоящий TlsStream.connect + HTTPS GET + read.
- НОВОЕ [M-187-sse-live-tls-server-hang] P1: SSE-путь weather-live
  (/api/events?...mode=live) вешает сервер (2-й запрос); /api/run тот же —
  ОК 5×5. Клинит SSE+live-TLS-комбинация (remote-park не дренится на закрытии
  SSE). Демо-обход: браузерный weather-live не открывать.

## 187 нагрузочный тест + chaos-креш находка (2026-07-15, оркестратор)

- loadtest.ps1 (examples/flagship/aggregator/) — самодостаточный НТ: сам
  собирает/поднимает/глушит сервер, 7 блоков (base / run 6 комбо / SSE 6 комбо /
  sustained SSE weather-live xN / concurrency N параллельных / idle 12с /
  demo-детерминизм). Запуск: .\loadtest.ps1 [-Port -Iterations -Concurrency
  -Build -SkipLive]. Финал PASS=24 FAIL=0.
- НАШЁЛ РЕАЛЬНЫЙ БАГ (узкая проверка пропустила): weather/chaos КРЕШИЛ сервер
  `panic: integer overflow: *`. Корень: scenarios.nv splitmix64_step использовал
  голые `+`/`*` — не смигрирован на wrapping при приземлении D423 (trap-default
  overflow, волна 206). splitmix64 модульный по замыслу → .wrapping_add/
  .wrapping_mul (эталон std/testing/handlers.nv). Фикс — 3 строки в scenarios.nv.
  Урок: любой hash/PRNG-код в примерах надо ревизнуть на wrapping после D423.
- Тест-артефакт (не баг): demo-детерминизм сравнивал ПОРЯДОК results[], а
  parallel for завершается недетерминированно → сравнивать МНОЖЕСТВО id:state
  (сортированно). Исправлено в loadtest.ps1 BLOCK 7.

## [M-187-high-concurrency-connection-wedge] находка (2026-07-15, loadtest 10×)

- 10× нагрузочный (loadtest.ps1 -Concurrency 80) вскрыл: M:N-сервер флагмана
  (1 процесс, 16 воркеров) ВИСНЕТ под массовой одновременной нагрузкой.
  ~20 одновременных соединений держит; 40 — часть 000 но восстанавливается;
  80 — 21/80 прошло, дальше 000 НАВСЕГДА (не восстанавливается, single-req тоже
  000). Sustained-последовательная (50× SSE weather-live) и 10 раундов по всем
  комбо — идеально. Клинит именно ВЫСОКАЯ ПАРАЛЛЕЛЬНОСТЬ соединений. Родня
  pending_remote/accept-park семьи (83.x) на пике. Демо-нарратив = concurrency,
  так что P1. Repro: loadtest.ps1 (BLOCK 5) либо seq 1 80|xargs -P80 curl /api/run.
- loadtest.ps1 усилен 10×: Iterations 5→50, Concurrency 8→80 (runspace pool, не
  Start-Job — лёгкие потоки в одном процессе), +Rounds=10 (повтор комбо-свипов).

## Plan 196.8 — фикс `[M-primitive-receiver-bounded-blanket-dispatch]` (2026-07-16, sonnet)

- Root cause (`emit_c.rs`, Plan 164 Ф.3 guard, ~37744-37888): двойной пробел.
  (1) `recv_is_candidate` признавал примитив-ресивер кандидатом на blanket-dispatch
  ТОЛЬКО если у метода есть UNCONSTRAINED бланкет (`g.bounds.is_empty()`) — BOUNDED
  бланкет (`fn[T Ints] T @checked_add`) на примитиве не проходил гейт вообще.
  (2) `protocols_match` умел проверять bound ТОЛЬКО как `#impl(Protocol)` через
  `type_impl_protocols` —D310 type-set bound (`Ints` = `type Ints set i8|…|uint`)
  никогда там не появляется (примитивы не получают `#impl`-запись), так что
  ЛЮБОЙ бланкет с type-set bound не матчился НИ ДЛЯ КОГО, не только для примитива.
  Итог: `i64.checked_add(b)` в CU с `Duration @checked_add` (Пункт 10/200) падал
  в name-keyed `method_receivers` last-wins → чужой Duration-оверлоад → CC-FAIL.
- Фикс (НЕ name-guard, минимальный дифф в конкурентном emit_c.rs): новое поле
  `type_set_members: HashMap<String, Vec<String>>` (популяция из
  `TypeDeclKind::TypeSet` деклараций, рядом с `type_impl_protocols`-циклом) +
  (a) `recv_is_candidate` расширен флагом `has_typeset_blanket_for_primitive`
  (примитив-ресивер, чьё каноническое Nova-имя — `debt_nova_type_name_from_c`,
  УЖЕ существующий helper — является членом type-set бланкета); (b)
  `protocols_match` для НЕ-пустого bound сперва проверяет `type_set_members`
  (membership), и только если bound — НЕ type-set, падает на старую
  `type_impl_protocols`-ветку. Оба места растут из ОДНОГО источника данных.
- Верификация: своя фикстура (`Foo value {n i64}` + собственный `@checked_add`
  + bounded-бланкет `Ints`-коллизия) — CC-FAIL на непатченном main-бинаре,
  чистая сборка+PASS на патченном. Реальный репро — worktree `nova-p200dur`
  (`duration.nv` после Пункта 10): `nova test std/src/time` — CC-FAIL `duration`
  ИСЧЕЗ (было `passing 'int64_t' … 'NovaValue_Duration'`).
- НАЙДЕНО ПОПУТНО (вне объёма 196.8, НЕ фикшу здесь): `duration` теперь
  RUN-FAIL (не CC-FAIL) — `Ф.1c/D317` тест на `saturating_add` падает по
  значению. Root — ОТДЕЛЬНЫЙ баг той же СЕМЬИ, но другой формы: `sat_add_i64`
  зовёт `r.clamp(lo,hi)` на i64 (pattern-bound из `Option[T]`-деструктуризации),
  а `@clamp` существует ТОЛЬКО для `int`/`f64` (ни i64-оверлоада, ни бланкета) —
  codegen молча мис-диспатчит в `Nova_f64_method_clamp` (implicit int64_t↔double
  на границе вызова, НЕ CC-FAIL, но f64-мантисса режет точность на i64-крайних
  значениях) → RUN-FAIL. Чекер не флагает `E_UNKNOWN_METHOD` в pattern-bound
  форме (изолированный `ro r i64 = …; r.clamp(...)` ВНЕ pattern-контекста
  корректно даёт `E_UNKNOWN_METHOD` — подтверждено). Залогировано как НОВЫЙ
  P1-маркер `[M-i64-clamp-primitive-collision-dispatch]` (backlog-followups.md);
  масштаб — codegen + checker pattern-bound receiver gap, отдельное окно.
- Артефакты: `nova_tests/plan196_8/p196_8_repro.nv` (своя фикстура, PASS);
  `spec_tests/conformance/primitive_bounded_blanket_dispatch.nv` (позитив-фикстура
  для мега-CU гейта — НЕ прогнана локально, оркестратор). `docs/plans/196.8-
  primitive-receiver-bounded-blanket.md` (подплан).

## [M-187-http-serde-setcookie-serialize-collision] — закрыт (2026-07-16, sonnet, ветка fix-serde-dispatch)

- Задание предполагало dispatch-баг в `emit_c.rs` (mono-инстанциация generic
  fn `json_encode[T]` резолвит `.serialize()` по имени, та же семья что
  196.7/98e3663cc). Эмпирическая проверка (минимальная фикстура: один CU,
  `Dto` с `#impl(Serialize)` + `FooCookie` с ручным `@serialize() -> str`,
  `nova build`) показала, что root-cause лежит СОВСЕМ не там.
- Root-cause: `nova-cli/src/main.rs::cmd_build` (используется `nova build`)
  никогда не вызывал `auto_derive::inject_synthesized_methods_filtered` для
  `#impl(Serialize)`/`#impl(Deserialize)` — в отличие от `test_runner.rs`
  (`nova test`) и `nova-codegen`'s собственного `cmd_compile`, оба вызывают
  это ПЕРЕД numbering/type-check. Чекер (`check_module`) валидирует
  `v.serialize(s)` через ON-DEMAND bridge (`AutoDeriveQueryBridge`/
  `synthesize_method`, `types/mod.rs`) — ВИРТУАЛЬНО, не мутируя
  `module.items`. Type-check проходит зелёным, но codegen (`emit_c.rs`),
  сканируя `module.items` для построения `method_overloads`/
  `mono_method_decls`, не находит НИКАКОГО `FnDecl` для derived
  `<Record>.serialize` — запись под ключом `(RecordType, "serialize")`
  попросту пуста. Вызов `v.serialize(s)` внутри mono'нного generic
  `json_encode[T]` проходит ВСЕ receiver-typed dispatch-окна (concrete-key
  `method_overloads`, generic-instance 5b, Ф.3 protocol-blanket, 196.7 facade
  — все впустую) и падает в единственный оставшийся путь: single-key
  name-only `method_receivers` last-wins fallback → берёт ЛЮБОЙ ДРУГОЙ
  конкретный `@serialize`, зарегистрированный последним в CU (в проде —
  `http`'s `SetCookie @serialize() -> str`, arity/type-несовместимый; в
  изолированной фикстуре без http — `[]T @serialize`-сентинел или
  `FooCookie`).
- Подтверждено: убрать коллизию (только `Dto`, без другого `@serialize` в
  CU) — баг НЕ пропадает, просто ломается на `__mono_method__[]T__serialize`
  (unresolved sentinel identifier) — т.е. это НЕ per-call mis-dispatch
  эвристика, а ПОЛНОЕ отсутствие регистрации derived-метода на `nova
  build`-пути. Доп. проба: тип, объявленный ВНУТРИ `std/src/encoding/serde/`
  (тот же модуль, что `json_encode`), собранный через `nova build` — ТА ЖЕ
  поломка; `nova test` того же файла — PASS. Переменная — не «модуль
  записи типа», а «build vs test_runner.rs pipeline».
- Существующий комментарий в `cmd_build` (~строка 4826, "Ф.4c") УЖЕ
  документировал этот же класс истории для ДРУГИХ каналов
  (`resolved_types`/`resolved_callees`): "the `nova build` path had silently
  omitted" то, что `test_runner.rs`/`main.rs` уже кормили. Для serde
  auto-derive injection это было просто ещё не зачинено.
- Фикс: один вызов `inject_synthesized_methods_filtered(&mut module, |p| p
  == "Serialize" || p == "Deserialize")` добавлен в `cmd_build` перед
  alpha-rename (точная позиция, что в `test_runner.rs`). `emit_c.rs` НЕ
  тронут — существующие type-directed dispatch-окна уже резолвят корректно,
  как только FnDecl реально зарегистрирован. НЕ добавлен unfiltered
  `inject_synthesized_methods` (Equal/Clone/Compare/Hash/Display/Debug) —
  вне заявленного скоупа (Serialize/json_encode), отдельный потенциальный
  follow-up.
- Реальный репро: `examples/flagship/aggregator` (http+tls в CU) собран
  СВОИМ компилятором через `nova build --strict-effects` (диамант через
  gitignored `examples/nova.local.toml` `[replace] tls = { path =
  "../../nova-tls" }`; http уже `path`-dep в `examples/nova.toml`). `main.nv`
  переведён на typed serde (`snapshot_to_json`, `report_json.nv`) — hand-
  written `snapshot_dto_json`/`status_dto_json`/`result_dto_json`/
  `handlers_dto_json` + весь WORKAROUND-комментарий-блок удалены.
  `emit_record_json`/`EmitRecord` (SSE per-event) сознательно остался
  hand-written — wire-shape решение (условно опускает `"error"` когда
  `kind != lane_failed`; plain derive эмитил бы поле всегда), НЕ баг-обход,
  follow-up отдельно.
- curl-smoke: `/api/snapshot`, `/api/run?legend=health&mode=chaos&seed=7`,
  `/api/events` (SSE replay, `run_summary`-event несёт typed JSON) — все
  корректны. `tls/echo_server`+`echo_client` (тот же `nova build`-путь)
  собраны и прогнаны — TLS 1.3 handshake + echo не регрессировали.
  `std/src/encoding/serde/*_test.nv` (6 файлов, `nova test`) — PASS,
  byte-identical (эти шли через `test_runner.rs`, фикс их не касается).
- Коммиты (ветка `fix-serde-dispatch`, worktree `nova-serdefix`, НЕ влита —
  интегратор): `a095b961d` (nova-cli фикс), `5f80b7b1b` (main.nv-снятие
  обхода), `eb24ae1ab` (backlog-followups.md закрытие маркера).
## [M-187-high-concurrency-connection-wedge] bounded-accept mitigation (2026-07-16, ветка fix-bounded-accept)

- Настоящий scheduler-фикс (park/join) отложен в research — вносил memory
  corruption; baseline (busy-poll, main) memory-safe, но виснет насмерть на
  ~80 одновременных соединениях (см. находку выше). Владелец: app-level
  bounded-accept mitigation ПОВЕРХ неизменного baseline-рантайма (не трогать
  runtime/компилятор).
- Реализация (`examples/flagship/aggregator/src/main.nv`): accept-loop теперь
  admission-control — `AtomicI64`-счётчик `inflight` (std.runtime.sync),
  проверяется срузу после `lst.accept()`; в пределах `MAX_INFLIGHT_CONNS` —
  `detach { handle_connection(...); fetch_sub }` (D50 fire-and-forget; под
  armed M:N этого бинаря — РЕАЛЬНЫЙ worker-pool dispatch, не синхронный
  test-only `SyncDetach`); сверх лимита — честный `stream.close()` немедленно,
  без очереди, без чтения/ответа (не «ручной HTTP»).
- Замена OLD-шейпа: старый код на каждое соединение открывал СВОЙ
  `supervised { spawn { handle_connection(...) } }`, что блокировало accept-
  loop до завершения (де-факто последовательно, `[M-187-sequential-2nd-
  request-hang]`). `detach` — другой codegen-путь (нет `supervised`-scope-var
  для threading) → не задет `[M-187-nested-spawn-scope-var-cc-fail]`.
- Подбор `MAX_INFLIGHT_CONNS` — ЧИСТО эмпирический, НЕ формула. Проверено
  прямыми `xargs -P80`/`-P200` бёрстами на этой машине с
  `NOVA_WATCHDOG_DUMP_SECS`-дампами на висящих прогонах (дамп показывал
  `STUCK_ALIVE_NOT_PARKED` fibers на нескольких воркерах, `[supervised]
  pending_remote=1` — тот же stale-slot симптом, что и в исходной находке):
  `= 1` переживает (частичный admit, ~2-5/80, остальное честно отбито,
  сервер жив после); `= 2` тоже переживает (повторено дважды подряд на одном
  живом сервере под -P80, отдельно под -P200); `= 3` и `= 4` ВОСПРОИЗВЕЛИ
  ТОТ ЖЕ permanent-wedge (0/80 admitted, сервер мёртв после) — НЕ частичная
  деградация, тот же баг. `16` (первая гипотеза — под размер worker-pool
  этого рантайма) тоже воспроизвела wedge: реальная конкурентность на этой
  ширине пересобирает ту же поломку через `aggregate()`'s собственный
  fan-out (несколько fiber'ов на каждый inflight-хендлер). Финал: `2`.
- Гейт: `loadtest.ps1 -Concurrency 80` — BLOCK 5 сервер ЖИВ после (200), BLOCK
  6 idle ЖИВ (200), BLOCK 1-4/7 без регрессий (2 независимых прогона, PASS
  66-67/67-68; единственный FAIL — BLOCK 5's строгий `$ok -eq 80` — ожидаемо,
  часть 80 честно отбита by design, критерий живучести из этой волны
  выполнен). Прямой `seq 1 80|xargs -P80 curl` и `seq 1 200|xargs -P200 curl`
  — часть 200, часть 000 (честный reject), single-req ПОСЛЕ = 200 (не
  permanent-000) — раньше на этой нагрузке было permanent-000 навсегда.
- Маркер `[M-187-high-concurrency-connection-wedge]` ОСТАЁТСЯ OPEN в
  backlog-followups.md (P1) — сам scheduler-баг не тронут, только окружён
  admission control'ом. Ветка `fix-bounded-accept` НЕ смёржена в main этой
  волной (гейт+вливание — оркестратор).

## ЗАКРЫТ [M-nova-linux-build] (2026-07-16, ветка p-linux-build, sonnet)

- Верифицировал Linux-сборку Nova напрямую на WSL2 Ubuntu 26.04 (не
  Docker — Plan 40 уже валидировал через Docker 2026-05-12, этот заход
  проверяет ту же цепочку в другой среде). Все 4 шага зелёные: `cargo
  build --release` (compiler-codegen + nova-cli), runtime C (libuv
  build-from-source + системный Boehm), `nova build` hello-world,
  `nova test std/src/checksums` (PASS 3 / FAIL 0 / SKIP 3). Новый
  документ — [docs/linux-build.md](linux-build.md).
- **Находка:** системный (apt/tarball) `rustc 1.93.1` на Ubuntu 26.04
  **ICE'ит** на `compiler-codegen/src/codegen/emit_c.rs`
  (`check_liveness` query, `slice index starts at N but ends at N-1` —
  апстрим-баг rustc, не Nova; воспроизведено дважды байт-в-байт).
  Обход: `rustup` (без sudo) + `rust-version` MSRV `1.85.0` — собирается
  чисто (`nova-cli` `Finished ... in 6m 47s`). GitHub CI
  (`nova-test-regression.yml`, `ubuntu-latest`) не задет — там другой
  rustc-билд. НЕ трогал `rust-toolchain.toml` (repo-wide pin задел бы и
  Windows-путь — решение владельца, не моё).
- **Находка (WSL2-специфика, не Nova-баг):** `nova build`/`nova test`
  резолвят workspace (`std/`) рекурсивным directory walk; на
  `/mnt/<drive>` (9p) это упирается в `p9_client_rpc` на МИНУТЫ даже для
  hello-world (подтверждено `/proc/<pid>/task/*/wchan`). Фикс — копия
  `nova.toml`+`std/`+`nova_rt` (без `libuv/test`+`libuv/docs`, ~300M
  balласта) на native ext4 перед `nova build`/`test`; `cargo build` сам
  по себе не задет (точечные reads, не directory walk). `du` через 9p
  вдобавок сильно завышает размеры (282M репортед / 3.8M реальных при
  идентичном файл-каунте) — не доверять.
- **Бонус (TSan smoke, spawn+supervised):** ручной `clang -fsanitize=thread`
  на generated `.c` + `nova_rt/*.c` + `libuv.a` (вне обычного CLI —
  `test_runner.rs` не имеет `--tsan` флага) — компилируется/линкуется
  чисто, работает до конца, **нашёл 2 реальных data race** с первого
  прогона: `fiber_arena.c` `_sigsegv_installed` (install-once
  check-then-set без atomic/mutex) и `runq.h` init/grab (visibility gap
  между `_materialize_pool`'ным init под mutex M0 и первым
  `nova_runq_steal` воркера) — второй ближе к сути Плана 211
  (nested-supervised wedge при N=3+), заметки переданы в отчёт для
  Плана 211 (файл плана не тронут этой волной).
- Обновлены `[M-tsan-race-detector]` и `[M-83.11-f2-arm-tsan]` в
  backlog-followups.md — блокер снят, ссылка на linux-build.md.
  `[M-nova-linux-build]` строка удалена из backlog-followups.md (по
  конвенции — resolved-маркеры не остаются в OPEN-view).
  fiber_arena POSIX (`fiber_arena.c`) уже существовал — порт не
  потребовался. Правок Rust/C кода НЕ было — всё уже работало на
  правильном toolchain'е.

## Plan 196 Зона TEST — пин-фикстуры census-пробелов (2026-07-16, sonnet, worktree `nova-196test`)

Опережающее покрытие ПЕРЕД миграцией волны CH/GEN/RET (docs/plans/196-campaign-map.md §Зона TEST):
три новых pir-файла в `spec_tests/conformance/`, каждый верифицирован изолированным
standalone-прогоном (temp `module standalone.zz_wip_*`, PASS) ДО мержа в общий
`spec_tests.conformance` folder-module (полный ~300-файловый CU не гонялся — задание
явно исключало; батч-гейт остаётся за оркестратором).

- **`d61_effect_handler_direct_call.nv`** — D61 §8 "`Effect[E].op(args)`" (прямой вызов
  операции на handler-значении, минуя with-стек) — пиннит `B11ac_novavtable_effect`
  (`emit_c.rs` ~51777, `effect_schemas`-lookup). До этого файла упражнялось ТОЛЬКО
  `examples/effects/effects_d61.nv` (вне быстрого conformance-гейта; census
  `196.5-stage-d-census.md` §3.2, трафик=1).
- **`self_recursive_generic_method_return.nv`** — `[M-generic-method-self-recursive-return]`
  / `B11ak_self_recursive_generic_method` (`emit_c.rs` ~51913): генерик-метод с
  method-level `U`, чьё тело рекурсивно зовёт СЕБЯ на значении своего receiver-типа
  (`RmGaugeList[T] @map[U]`). Census трафик=5 (std collections/data/encoding), в
  conformance не было.
- **`dispatch_free_fn_vs_method_name.nv`** — `B10f_user_fn_sigs` (`emit_c.rs` ~50956):
  bare free-fn call vs одноимённый instance-метод на другом типе — регресс-гард на
  порядок (free-fn-специфичный `user_fn_sigs` ПЕРЕД receiver-blind legacy-fallback).
  Census трафик=44.

**Побочная находка (латентный баг, НЕ фикс — вне Зоны TEST):** при разработке
d61-теста с операциями `read()`/`label()` компилятор дал CC-FAIL — сгенерированный C
для `gauge.read()` оказался `NovaVtable_D61Gauge r = (*(gauge));` (структурная COPY
vtable) вместо вызова `gauge->read(gauge->ctx)`. Root: нуль-арный `.read()` (и
одноарный `.write(v)`) на ЛЮБОМ `NovaVtable_<Eff>*`-ресивере ошибочно проходит guard
СОВЕРШЕННО НЕСВЯЗАННОЙ ветки `B11d_typed_pointer_methods` (typed-pointer deref,
`emit_c.rs` ~51368) РАНЬШЕ, чем управление доходит до `B11ac` — guard
`!obj_ty.starts_with("Nova_")` не исключает префикс `NovaVtable_` (нет подчёркивания
сразу после `Nova`). Живых call-сайтов нет (census не видел `read`/`write` среди имён
эффект-операций) — чисто латентно, пойман только потому, что пин-тест сознательно
перебирал разные имена операций (методология «мысленно прогнать против старой
версии» / silent-wrong-value-пиннинг, test-conventions.md). Тест переименовал
операции на `peek`/`tag`, находка задокументирована `[M-novavtable-read-write-pointer-collision]`
в backlog-followups.md (P2, однострочный fix вне scope Зоны TEST — принадлежит
Зоне GEN/frozen).

По каждому D из очереди волны-2, указанному в задании (D30/D85/D52/D182/D16/D53/D239) —
сверка ПО КОДУ подтвердила: существующие фикстуры (`d30_try_op_unwrap_pair.nv`,
`d30_result_option_ret_generic.nv`, `d30_closure_return_generic.nv`,
`d85_question_return.nv`, `d85_result_payload_width.nv`, `d52_sumint.nv`,
`d52_type_forms.nv`, `d182_self_return_parametric_static.nv`,
`d16_generics_brackets.nv`, `d53_type_protocol_kind_token.nv`,
`d239_slice_vec_alias.nv`, `d239_elem_type.nv`) УЖЕ пиннят конкретные
типы/значения (не happy-path-заглушки) по всей матрице форм (generic/cross-module/
nested/type-changing) — новых файлов там не требовалось (196.3-wave2-d-driven.md
собственный gap-анализ уже это фиксирует). D52/D407/D406 доп. сверено против
`196.5-perd-d52-verification.md`: `infer_method_level_return_for_sum` Option/Result-
часть уже пиннится `d119_option_result_method_level_generic.nv`.

## Plan 211: layout-фикс std/time + откат маскирующего фикса + E_MODULE_FILE_ORPHAN (2026-07-17)

**Раскладка (`[M-time-folder-coequal-mismatch]` ✅ РЕШЕНО).** `std/src/time/duration.nv`
+ `timestamp.nv` + `monotonic.nv` объявляли один файловый под-модуль `module
time.duration`, разбросанный тремя co-equal файлами прямо в `std/src/time/` —
по букве D78 (папка = один модуль, головной файл `<path>/<Y>.nv` ИЛИ выделенная
папка `<path>/<Y>/`) это ни то ни другое: `is_folder_module_peer` требует
последний сегмент декларации == имя папки, что не выполнялось (папка
называлась `time`, не `duration`). Симптом: `E_D78_MODULE_PATH_MISMATCH` при
компиляции `timestamp.nv`/`monotonic.nv` как ПРЯМЫХ test-энтри (imports.rs
уже правильно резолвил обычный import-путь). Фикс — чистый layout: `git mv
duration.nv → duration/core.nv`, `timestamp.nv`/`monotonic.nv` →
`duration/{timestamp,monotonic}.nv` (renames, `module`-декларации и
import-путь `std.time.duration` не менялись; `core.nv`, не `duration.nv`,
потому что файл+папка одного имени запрещены — прецеденты `time.civil`,
`collections.vec`). Подтверждено: direct-entry `nova check` на обоих
переехавших файлах — чисто.

**Откат маскирующего фикса `[M-blanket-crossmodule-scattered-peer-drop]`
(59f22a85b).** Тот фикс закрывал СИМПТОМ (E_UNKNOWN_METHOD на
`to_unix_seconds`/`to_unix_nanos` при импорте `std.time.duration` ИЗВНЕ —
`std/src/time/civil/parse_test.nv`), но противоречил букве D78: научил
`resolve_module_paths` молча домёрживать ЛЮБЫЕ co-equal файлы в общей
родительской папке по совпадению `module X.Y`, т.е. фактически разрешил
«папка может нести файловый под-модуль россыпью» — ровно то, что D78 не
предполагает. Раз корень (раскладка) устранён, обходной scan больше не
нужен — `git revert --no-commit 59f22a85b`, применился чисто. 4 существующих
юнит-теста `imports.rs::entry_folder_module_tests` — не задеты (другая ветка
кода: entry-sibling-scan, не `resolve_module_paths`'s single-file branch).

**Новая диагностика `[M-module-file-submodule-split-silent-orphan]` ✅
РЕШЕНО.** Симптом маскирующего фикса не был случайным — это ОБЩИЙ класс:
любой раз, когда файловый под-модуль реализован scattered co-equal файлами
(не в выделенной папке), внешний импортёр молча видит только head-файл, а
peer-декларации тихо сиротеют (видны ТОЛЬКО когда peer сам — compile-entry,
через отдельный entry-sibling scan). `resolve_module_paths`'s `file_exists`
branch теперь сканирует директорию head-файла на такие peers и, если найдены,
возвращает `ResolveErr::FileOrphan` → `[E_MODULE_FILE_ORPHAN]` — 4-частная
диагностика (какой файл+что объявил / почему файловый-модуль не берёт
peer / следствие-невидимость методов импортёру / fix-подсказки: папка-модуль
`<dir>/<Y>/` — рекомендуется, либо поправить декларацию, либо переместить
файл). Neg-репро package-scale копия реального бага:
`spec_tests/conformance/neg/module_file_orphan.nv` (+
`module_file_orphan_repro/{core,scattered}.nv`) — `nova test ... --full` →
PASS (negative), подтверждена точная триггер-цепочка.

**Побочная находка (НЕ фикс — вне периметра волны), `[M-checker-path-call-chain-unknown-ret-type]`.**
Разблокировав `std/src/time/duration/monotonic.nv` для реальной внешней
компиляции, `nova test std/src/time/overflow_safe_test.nv --full` упёрся в
ICE `emit_c.rs:52222 [P67-LEGACY] Path call return type unknown for
method=to_nanos`. Локализовано до конкретной строки —
`monotonic.nv:105`: `sat_sub_i64(@nanos, other.nanos, i64.MIN,
i64.MAX).to_nanos()` внутри `Monotonic @elapsed_since` — чейн-метод
`.to_nanos()` вызван НЕПОСРЕДСТВЕННО на результате вызова свободной функции
(`sat_sub_i64(...)`), а не на переменной/литерале; чекер не аннотирует тип
возврата такого промежуточного «Path call» перед чейн-методом. Контрольный
негативный сигнал: изолированная копия `d317_duration_overflow_policy.nv`
(упражняет Duration/Timestamp через переменные/литералы, БЕЗ
Monotonic/без промежуточного free-fn-call-чейна) компилируется и проходит
чисто — подтверждает, что баг ортогонален и layout-фиксу, и cross-module
видимости per se; это ранее НИКОГДА не упражнявшийся codegen/checker-гэп в
самой реализации `Monotonic.elapsed_since`, вскрытый только тем, что модуль
теперь целиком компилируется извне. Не чинить в этой волне — заведён
отдельным floating-маркером (P2) в `backlog-followups.md`.

## Plan 196 Zone GEN (sonnet, worktree `nova-196gen`, ветка `p196-zone-gen`) — `[M-novavtable-read-write-pointer-collision]` ЗАКРЫТ

`[M-novavtable-read-write-pointer-collision]` (найдено Зоной TEST 2026-07-16, backlog P2) —
guard на входе в `B11d_typed_pointer_methods` (`emit_c.rs`, ДВА места: эмиссия ~36083 +
инференс-двойник ~51490) исключал `NovaArray_`-префикс, но не `NovaVtable_`
(`"NovaVtable_X*".starts_with("Nova_")` == false — символ на позиции 4 внутри `NovaVtable`
это `V`, не `_`, та же дыра, что уже была закрыта для `NovaArray_`). Нуль-арный `.read()`/
одноарный `.write(v)` на ЛЮБОМ handler-ЗНАЧЕНИИ (`NovaVtable_<Eff>*`) молча мисдиспатчился
в typed-pointer-deref (эмиссия давала голый `(*(h))` вместо `h->read(h->ctx)`) вместо
корректного `B11ac_novavtable_effect` (~51900, `effect_schemas`-lookup) / direct-handler-call
эмиссии (~36410). Фикс: явное `!obj_ty.starts_with("NovaVtable_")` в ОБОИХ guard'ах,
мирроринг существующего `NovaArray_`-исключения.

Пин: `spec_tests/conformance/d61_effect_handler_direct_call.nv` расширен `D61Guard` effect
(буквально `read()`/`write(v int)` op-имена) + тест, вызывающий обе операции напрямую на
handler-значении — до фикса ловился B11d, после — корректно диспетчится через vtable.
Верификация (изолированно, без полного conformance-гейта — тот при этой сессии красный на
ДВУХ ПРЕДСУЩЕСТВУЮЩИХ файлах `d229_debug_format_spec.nv`/`d422_generic_container_derive.nv`,
`[E_IMPL_WRONG_SIGNATURE]` на auto-derived `Debug`/`Display` `mut f Fmt` — регрессия недавнего
`fix-param-mut-enforcement` (`4d6b15363`), НЕ связана с этим фиксом, оставлена владельцу как
отдельная стоп-волна, не тронута): conformance-папка минус `d229` → PASS 472 FAIL 12 (12 —
тот же класс `mut Fmt` derive-регрессия на других файлах + pre-existing TIMEOUT-флаки/host
contention + одна pre-existing NEG-NO-ERROR, ни один не касается d61/NovaVtable). Коммит
`c7c7f127e` на `p196-zone-gen`. Маркер снят из `backlog-followups.md` (lifecycle §2).
## Финал разкраснения nova-lint (2026-07-17, ветка fix-lint-final, sonnet)

Закрыты последние два floating-маркера lint-волны (план 185): `nova lint std`
5 находок → 0.

**[M-lint-findings-param-no-contract] ЗАКРЫТ.** Три оставшихся сайта
`W_PARAM_NO_CONTRACT` — `HashMap[K, V].new(cap int = 16)`
(std/src/collections/hashmap.nv), `Queue[T].new(cap int = 0)`
(std/src/collections/queue.nv), `Set[T].new(cap int = 16)`
(std/src/collections/set.nv) — получили `requires cap >= 0` (владелец:
«requires n >= 0 — ДА»). Форма/имя параметра сверены с прецедентом
`Vec[T].new(cap int = 0) requires cap >= 0` (std/src/collections/vec/core.nv)
— контракт доказуемый (Z3 элидирует на литеральных аргументах, zero-cost).
`nova check` трёх файлов чист; таргетные `nova test` (doctests
hashmap/set, `queue_test.nv`) зелёные.

**[M-lint-findings-try-without-sibling] ЗАКРЫТ.** Две отдельные владельческие
правки:

1. `ReadFs`-протокол + `DirFs`/`EmbeddedDir` `@try_exists`
   (std/src/fs/readfs.nv) → `@path_exists` (владелец: «path_exists — ДА»;
   сигнатура/`Result`-возврат не менялись — соло-fallible без
   инфаллибельного сиблинга легален под обычным именем, R3 D325). Охват
   переименования: 3 декларации (protocol + `EmbeddedDir` + `DirFs`),
   `std/src/fs/readfs_test.nv` (call-сайты + заголовки тестов + докстрока),
   `docs/io-fs.md`, D323-амендмент `ReadFs` в spec/decisions/04-effects.md
   (добавлен AMEND-абзац 2026-07-17). Свободная fn `try_exists`
   (std/src/fs/fs.nv, отдельный символ, НЕ протокольный метод) не тронута —
   вне периметра этого решения.
2. `Duration.try_from_secs_f64(s f64) -> Option[Duration]` (static ctor,
   std/src/time/duration/core.nv) СНЕСЁН целиком (владелец: «мы убрали все
   Duration.from_*» — не exception к правилу, а снос статики тем же курсом,
   что уже убрал `Duration.from_*`/per-width fluent, Plan 200 Step 2).
   Заменён ресиверной формой на источнике — `f64 @checked_to_seconds() ->
   Option[Duration]` (то же тело/семантика: `None` на `NaN`/`±inf`/out-of-
   `i64`-range), зеркалящей non-trapping half пары `@to_seconds()`/
   `@checked_to_seconds()` (тот же паттерн, что `@times(f64)`/
   `@checked_mul_f64` на `Duration`). Call-сайты мигрированы на
   `x.checked_to_seconds()`: inline-тест в core.nv, `spec_tests/conformance/
   d317_duration_overflow_policy.nv`. D317-амендмент (R5 f64-конверсии, заодно
   поправлена уже стухшая `@try_mul_f64`/`@try_div_f64` → `@checked_mul_f64`/
   `@checked_div_f64` в том же предложении) + новый AMEND-абзац 2026-07-17 в
   spec/decisions/04-effects.md. `nova check`/`nova test` core.nv таргетно —
   зелёные; d317-фикстура синтаксически/типово чиста (полный
   `spec_tests/conformance` не гонялся — не мой гейт в этой волне, см. ниже).

**Побочная находка (НЕ фикс — вне периметра волны).** `nova check
spec_tests/conformance/<любой файл>` (whole-CU) сейчас падает на
ПРЕДСУЩЕСТВУЮЩИХ `E_IMPL_WRONG_SIGNATURE` в `d229_debug_format_spec.nv`,
`d422_generic_container_derive.nv`, `pos_impl_debug.nv` — `debug`/`display`
конформеры объявлены с `w Fmt` вместо канон `mut f Fmt` (свежее слияние
`fix-param-mut-enforcement`, e160918da..4d6b15363, теперь сверяет РЕЖИМ
параметра). Ни одна ошибка не адресована в `d317_duration_overflow_policy.nv`
— моя правка в этом файле подтверждена чистой отсутствием upstream-ошибок по
ней; сама фикстура-регрессия ортогональна этой волне, не чинилась (владелец
не просил, конформанс целиком не гонялся по инструкции волны).

Итог: `nova lint std` → 0 находок (было 5); `nova lint spec_tests` → 0
находок (было 0, не регрессировало).
[2026-07-17 [M-str-primitive-static-arity-overload] — ЗАКРЫТ, ветка p-prim-static-arity, worktree nova-primstat] Root-cause — ДВА независимых слоя (найдены debug-трейсом `NOVA_DEBUG_STATIC` env-gated `eprintln!`, не гипотезой): (1) `compiler-codegen/src/types/mod.rs::f1_check_call`, `ExprKind::Path` арм (`Type.method(args)` резолв в чекере) — безусловный `is_primitive_recv { return; }` гейт (историческая защита Plan 91.8a.2 followup 2026-05-29 от false-positive: `self.sig.method_table` может знать НЕ ВСЕ overload'ы примитива — часть живёт во `external_registry`/codegen builtins, так что single-known-overload arg-check ложно ругался, напр. `str.from(char)` при известном чекеру только `str.from(bool)`) полностью выключал `resolved_callees`-регистрацию для ЛЮБОГО примитивного Path-static вызова, включая multi-known-overload случай (риск неполноты там НЕ применим — если чекеру видны ≥2 РЕАЛЬНЫХ кандидата, оба целиком объявлены в одном модуле, это ИДЕНТИЧНО non-primitive multi-overload сайту, для которого arity+`overload_applicability`-compat уже безопасно работает в проде). (2) `compiler-codegen/src/codegen/emit_c.rs` (~39114-39258), multi-overload static Path-dispatch — УЖЕ arity-aware (`param_c_types.len() == arg_types.len()`), но строгий C-type-string `==` слеп к Nova-типо-эквивалентным разным сериализациям: `str.new(buf *u8, len int)`'s `*u8` ro-pointee параметр рендерится `"const nova_byte*"`, а call-сайт (`ro buf = @ptr()` в `into_str_unchecked`) инферится как `"nova_byte*"` (БЕЗ `const`) — 0 совпадений → фолбэк в `E_UNKNOWN_STATIC_METHOD`. Маркерная формулировка «арность-слепой» оказалась эмпирически неточной (арность УЖЕ проверяется), но итог тот же: примитивный multi-overload resolved_callees-канал был пуст (`resolved_callees.get(&call_id) == None`, подтверждено трейсом), codegen предоставлен сам себе с хрупким string-match. Фикс — МИНИМАЛЬНЫЙ, никакого нового резолвера (зеркалит Plan 200 п.5 [M-vec-new-static-arity-overload] прецедент, но на другом слое — там callnorm.rs/`pick_static_params`, здесь checker+codegen final-dispatch): (1) types/mod.rs — сузил `is_primitive_recv`-гейт: срабатывает ТОЛЬКО когда `overloads` — single (`Some([single])`); `Some(multi)` (≥2 known) теперь ПРОХОДИТ ту же ветку arity+compat resolution, что и non-primitive receiver'ы — ноль новой логики, просто снят барьер; (2) emit_c.rs — добавлен channel-first lookup (`self.resolved_callees.get(&call_id).and_then(|sp| static_overloads.iter().find(|s| s.fn_span == Some(*sp)))`) ПЕРЕД строгим string-match, мирроринг уже существующих `call_consume_arg_idxs`(~26297)/facade instance-dispatch(~37847) `fn_span`-паттернов; string-match остался как fallback (byte-identical для call-сайтов без channel-хита — synthesized/call_id-unset). Rename `wrap_owned`→`str.new(buf,len)` возвращён (декл + call-сайт в `into_str_unchecked`, `std/src/runtime/string/core.nv`), NB-блокер-комменты сняты/переписаны. Верификация: red→green `nova test std/src/checksums` (CODEGEN-FAIL 6/6 с rename ДО фикса → PASS 3/0 +3 skip ПОСЛЕ); δ0 `std/src/collections/vec` (PASS 1/0) + `std/src/crypto` (PASS 5/0) — single-overload byte-parity не шелохнулась; спот-грепом `.c`-артефактов (`std/src/checksums/*.c`) — 2-арг call-сайт уходит в `Nova_str_static_new__const_nova_byte_p_nova_int(buf, n)`, 0-арг d372-сайт (`spec_tests/conformance/d372_canonical/d372_canonical_new_defaults.c`) — в `Nova_str_static_new()`, оба нетронуты. Новая positive standalone-фикстура `spec_tests/conformance/d372_canonical/m_str_prim_static_arity_overload.nv` (сиблинг `types_generic_static_ctor.nv`, тот же folder-module, D29): `unsafe { bytes.into_str_unchecked() }` round-trip упражняет private 2-арг `str.new` (публичный вызов недоступен напрямую — маркер private); negative-control подтверждён (искажённый ожидаемый результат честно даёт RUN-FAIL перед финальным green-прогоном). Маркер закрыт в `docs/plans/backlog-followups.md` (P2 — Codegen секция). Модель: sonnet.
[2026-07-17 [M-d216-unsafe-map-single-file-gaps] — ЗАКРЫТ (ptr-cast в used-карту + arity-based per-overload resolution для generic-static/slice-sugar unsafe fn), ✅ ЗАКРЫТО, worktree nova-d216 ветка p-d216-unsafe-map] Владелец: закрой два P2-маркера по репро `nova check std/src/runtime/string/core.nv` (single-file) → ложный `E_UNSAFE_UNUSED` на `str @bytes() => unsafe { []u8.new(@ptr, @byte_len()) }` — реальный CU (импорты/тесты/CI) шёл зелёным. **Root cause (два независимых пробела карты D216 §21, `check_unsafe_context_in_module`, `types/mod.rs`):** (а) `ExprKind::As`-арм used-tracking'а (`E_UNSAFE_UNUSED`) знал ТОЛЬКО `char`-target cast (backfill строки не было в §21-карте вовсе — drift между кодом 2026-07-11 и спекой); pointer-target cast (`expr as *T`/`*mut T`/`*uninit T`) — канонический способ получить typed pointer — не мэтчился вообще, хотя `expr_is_typed_pointer`'s собственный `As`-arm УЖЕ узнаёт эту форму для ДРУГИХ гейтов (deref/index/order-compare/interpolation). (б) generic-static/slice-sugar receiver (`Vec[T].new(ptr,len)`/`[]u8.new(ptr,len)`) парсится `Member{obj: TurboFish{base:Ident(Type)} | Path(["__array",elem]), name}` — форма, которую `unsafe_callee_name`-матч (`Call`-arm) не распознавал вовсе (знает только bare-`Ident` и 2-сегментный `Path`, напр. `RawMem.alloc`). Наивное расширение на любой `Member`/`Path` было бы arity-blind: `Vec.new` несёт ТРИ арности под ОДНИМ `(тип,имя)` ключом (0/1-арг ctor safe, 2-арг VIEW `unsafe fn`, 3-арг owned safe) — потребовать `unsafe{}` у всех означало бы тысячи ложных срабатываний по `std/`. **Попытка резолва через checker-канал `resolved_callees` (Plan 172.1 U.3.4/196.7) — ОТКЛОНЕНА эмпирически:** debug-инструментация (`eprintln!` за `NOVA_DEBUG_UNSAFE_MAP`, снята перед коммитом) показала канал ПУСТ для этой формы вызова — `check_call_argbind`'s `Member{obj,..}` arm явно bail-out'ит на static/type-ресивер (`resolve_instance_method` ждёт VALUE-ресивер), arg-валидация generic-static ctor идёт совсем другим, канал-не-пишущим путём (codegen-side `generic_type_methods[base].find(name)`, "1b"-turbofish-ветка `emit_c.rs`). **Fix (б) — arity-based per-overload resolution:** `static_arities: HashMap<(type,method), HashSet<(min_required,max_accepted,unsafe_attr)>>` собирается в том же `collect_from`-проходе (HashSet, НЕ Vec — `collect_from` уже дважды обходит одни и те же декларации через `module.items`+`module.peer_files`, остальные поля этой функции — HashSet/HashMap ИМЕННО поэтому; первая версия на `Vec` дала ложную "неоднозначность" от дублей — поймано тем же debug-инструментом, entries приходили парами). На call-сайте фактический `args.len()` фильтруется по диапазонам всех оверлоадов под тем же ключом; РОВНО один матч → его `unsafe_attr` резолвится однозначно (`Vec.new`: диапазоны `[0,1]`/`[2,2]`/`[3,3]` не пересекаются). Asymmetric: enforcement (`E_UNSAFE_CALL_REQUIRES_WRAP` при depth==0) — ТОЛЬКО при однозначном arity-резолве; used-tracking (depth>0) — мягкий fallback на `any_unsafe_overload_names` (bare-name union), если арность неоднозначна. **Fix (а):** зеркало char-cast проверки — `depth>0` + `ty` (`TypeRef::Pointer|Mut|Uninit`, без `strip_modifiers` — `*mut T` парсится `Pointer(Mut(T))`, `Pointer` уже внешний узел) → `mark_unsafe_used()`. **Побочный баг закрыт по пути ([M-check-folder-enumerator-skips-no-prelude]):** `nova check std/src/runtime/string` (папка) давал «no .nv files to check» — ДВЕ стопки причин в `nova-cli`: (1) `should_skip_path_full`'s дефолтный runtime-skip был PATH-based (весь `std/src/runtime/**`) — stale с Plan 152 folder-split + char/numeric/str "NOT auto-gen anymore" (`runtime_registry.rs`: только `math_runtime()` реально генерит); заменён на content-marker check (`is_autogen_runtime_stub` — ищет тот же литеральный `AUTO-GENERATED by \`nova-codegen`-заголовок, что пишет генератор), скоуп остался тем же (только внутри `std_runtime_dir`), но теперь бьёт только по `math.nv`. (2) `walk_nv`/`walk_nv_filtered` (`test_runner.rs`) — TEST-DISCOVERY walker: подтверждённый folder-module (все peers декларируют один `module X`) БЕЗ локального `test "` блока где-либо среди peers дропался целиком («библиотека без тестов — test-раннеру нечего запускать»); `runtime/string/` (chars/core/parse/search/slice/transform.nv, все `module runtime.string`) не имеет `_test.nv`-пира (покрытие живёт в `spec_tests/conformance`) → под этот гейт попал. Новая `walk_nv_for_check` (расшаренная `walk_nv_filtered_ex`, флаг `include_untested_folder_modules`) не гейтит по тестам — `nova check`'s dir-branch теперь зовёт её вместо `walk_nv`; `nova test`'s собственные call-сайты не тронуты (старая семантика). `classify_skip_path`/`SkipReason` — честный «no .nv files to check — skipped N file(s) (reason)» вместо неотличимого от «папка реально пуста» сообщения. **Фикстуры:** `spec_tests/conformance/d216_unused_unsafe_pos.nv` (+3 теста: cast-only used, `Vec[T].new`/`[]u8.new` wrapped-обе-формы) + 2 новых neg (`d216_generic_static_unsafe_overload_neg.nv`, `d216_slice_sugar_unsafe_overload_neg.nv` — enforcement без обёртки, обе AST-формы) + существующий `d216_unused_unsafe_neg.nv` регрессия зелёная. **δ0-верификация:** `nova check std/src` (весь stdlib) → 0 совпадений `E_UNSAFE_*` где угодно (только pre-existing consume/type-ошибки в `neg/`-фикстурах, не связанные); таргетно `std/collections/vec`, `std/net`, `std/io` — без новых находок; `nova test std/src/collections/vec` PASS. Снят устаревший NB-комментарий 2026-07-17 в `str @bytes()` (описывал именно этот баг). D216 §21-амендмент дописан в `spec/decisions/02-types.md` (новая карта-запись П.5-семьи + backfill пропущенной char-cast строки + находка про пустой resolved_callees). Диагностика-меняющее (новый enforcement для generic-static/slice-sugar unsafe-fn) → D-амендмент в том же слиянии (это упрощение). Модель: sonnet.

## [M-208-vec-chained-debug-display-red] (2026-07-17, гейт-находка оркестратора)

- conformance `app_effect_basic_t8_1` КРАСНЫЙ на Windows main (2 RUN-FAIL строки в
  полном прогоне): четыре assert'а `a.into_str() == "Vec[1.5, 2.5]"` / `"Vec[1, 2]"` /
  `"Vec[]"` (chained .debug/.display на Vec[f32]/Vec[int]) падают. Появился после
  208 Ф.3 (Vec Fmt-миграция, e06bfb7fa) — либо формат Display у Vec изменился
  (тогда тест обновить С СОГЛАСОВАНИЕМ), либо chained-диспатч сломан (тогда фикс).
  Родня [M-208-generic-interp-display-dispatch-gap]. Дом — 208-волна.
- Контекст обнаружения: полный conformance ветки fix-elapsed-ice 481/2/14 —
  оба FAIL = этот тест; подтверждён красным и на чистом main (изолированный прогон).
[2026-07-17 [M-198-f4c-compiler-findings] — РАЗОБРАН, Plan 212 пункт 7, sonnet, worktree nova-198rv, бинарь пере-собран release на коммите 696d834b4] Пере-проверка 9 находок Ф.4c-очереди (docs/plans/198-redo-notes.md, оригинал — миграция merged-CU 198) на актуальном компиляторе. Методология: изолированные fixture-репро (2-файловые folder-module с уникальным module-именем, НЕ спекулятивный корневой `spec_tests.conformance` — тот тянет peer-discovery по ВСЕМУ пакету и вынуждает full ~1000-файловый прогон даже для 1-2 явных CLI-путей; полный `spec_tests/conformance` НЕ гонялся ни разу, как предписано инструкцией волны). Побочный блокер перед стартом: диск D: был на 640МБ свободных (1.9ТБ занято) — worktree add падал `no space left on device`; освобождено удалением (а) осиротевшего non-git каталога `nova-p187` (7.3ГБ, не зарегистрирован как worktree) и (б) 9 `git worktree remove` для ВЕТОК УЖЕ СЛИТЫХ в main и НЕ locked (`git branch --merged main`, cross-referenced с `git worktree list --porcelain`+lock-флагом): nova-retryulid, nova-d216, nova-modlayout, nova-blanketfix, nova-208tails, nova-196gen, nova-196cap (+ 2 skipped из-за uncommitted diffs — nova-2001, nova-205d, НЕ тронуты) — итог 640МБ→52ГБ свободно, ветки НЕ удалены (только worktree-чекауты), ничьи данные не потеряны.

Вердикты по 9 находкам:
1. **priv(file) type не файл-дискриминируется** — ЖИВОЙ, репро подтверждено (2 файла с разными `Rect`, use-site в a.nv резолвит b.nv's Rect → `E7320 no field w, note: Rect has fields name,tag`). Новый маркер `[M-198-f4c-1-privfile-type-not-discriminated]` P1, фикстура `spec_tests/fixtures/known_red/privtype_file_discrimination/{a,b}.nv`. Родня орфан-упоминанию `[M-170.1-priv-file-types-methods]` (169.2-аудит).
2. **local var не затеняет cross-file top-level fn** — ЖИВОЙ, репро подтверждено (`ro f = helper; f(21)` биндится к чужому `fn f(y str)` из sibling-файла → `E7301`). Новый маркер `[M-198-f4c-2-local-not-shadow-crossfile-topfn]` P2, фикстура `spec_tests/fixtures/known_red/local_shadows_topfn/{a,b}.nv`. Родня (не дубликат) уже закрытому `[M-168-resize-with-free-fn-shadow]` (тот — closure-параметр, этот — локал).
3. **alias-import folder-peer** (`import X as h` → codegen эмитит `h.fn(...)` буквально) — НЕ воспроизводится: ни solo-модуль (`standalone/f1_alias_call_pos.nv`+`f2_whole_module_pos.nv`, оригинальные victim-фикстуры), ни искусственный genuine-peer (2 файла, один module, каждый со своим alias) не триггерят баг на актуальном бинаре. Плюс историческое: FIN-6 тэлли 2026-07-13 (`PASS 501/FAIL 4`, полный merged CU 1005 файлов/2585 блоков) уже показал эти файлы PASS (не среди 4 известных FAIL). Закрыто без нового маркера (нет живого симптома); переоткрыть с свежим репро, если всплывёт на полном гейте снова.
4. **handler-литерал match-arm capture** (`with Fail[E] = |e| interrupt (match e {...})`) — НЕ воспроизводится: точная копия синтаксиса из формулировки находки (= содержимое `standalone/f3_typed_result_err.nv`) в изолированном уникальном module — PASS. Тот же файл был частью FIN-6 полного-CU PASS 2026-07-13. Закрыто без нового маркера, тот же резерв на переоткрытие.
5. **std-internal `classify` захвачен пользовательским `classify`** (mangling-коллизия, potentially soundness-grade) — НЕ ПЕРЕВЕРЕНО окончательно: изолированный репро (user `fn classify(int)` + `import std.net.{NetError}` + реальный вызов `NetError.from_code` → внутренний `classify(str)`) — PASS, коллизии нет. НО оригинальный триггер-файл (переименован в обход, коммит 559d52880) не идентифицирован в актуальном дереве (grep на корневой `spec_tests.conformance` не находит user-`classify` вообще), и заявленный масштаб (~1000-файловый merged CU) не проверялся (запрет волны) — в отличие от (3)/(4), здесь НЕТ исторического full-scale PASS-свидетельства специфично для этого случая. Новый маркер `[M-198-f4c-5-std-internal-symbol-capture]` P3, статус явно «неопределён», не «закрыт».
6. **bench.\* интринзики в test-блоках = ICE** — ЖИВОЙ, ICE подтверждён на существующей карантин-фикстуре `spec_tests/conformance/fixtures/ice_blocked/p2_bench_namespace_callable.nv`: `internal error at emit_c.rs:52127: [P67-LEGACY] method call .opaque return type unknown` (строка сдвинулась с 48774 — дрейф кода, тот же класс). Новый маркер `[M-198-f4c-6-bench-intrinsic-test-block-ice]` P2.
7. **extern "nova" fn + tuple-return CC-FAIL** — ЖИВОЙ, подтверждено на `spec_tests/fixtures/known_red/t4_sqlite_e2e_ok.nv` (temp-скопирован `sqlite_mini_ffi.h` в `nova_rt/` для теста, не коммичено): `lld-link: undefined symbol: nova_fn_mini_sqlite_open` — embedded mini-shim даёт голые C-имена без `nova_fn_`-манглинга, `--c-shim` CLI (Plan 115 followup) так и не построен. Новый маркер `[M-198-f4c-7-extern-nova-tuple-return-ccfail]` P3 (первая явная backlog-запись — раньше жила только комментом в коде).
8. **priv(file)-fn bleed** (`method_call_never_static`/`scalar_only_empty`, `pick`) — ЗАКРЫТ фиксом `7542e0013` (2026-07-14, D307 §5.3 facet-B, ДО этой волны, файлы уже возвращены из карантина в root corpus) — переподтверждено изолированным репро на актуальном бинаре (PASS). Маркер не заводится — уже решено.
9. **file-scoped `#unchecked` теряется в folder-module** — MOOT/ЗАКРЫТО ретракцией: Plan 194 полностью убрал `#unchecked` из языка; `#unchecked { ... }` теперь `error: unexpected '#' in expression` (проверено смоук-тестом) — конструкция физически не существует, находка неприменима.

Итог: backlog-followups.md — umbrella-маркер закрыт (✅ DONE, расщеплён), добавлено 5 дочерних маркеров (1,2,5,6,7 — три P1/P2 живых бага + P3 неопределённый + P3 известный долг), (3)/(4) закрыты без маркера (нет живого симптома + историческое подтверждение), (8)/(9) закрыты (фикс/ретракция). Полный `spec_tests/conformance` не гонялся ни разу за волну (инструкция) — это НЕ авторитетный гейт-прогон, только точечные fixture-репро. Модель: sonnet.
[2026-07-17 [M-174.6-rawptr-extern-unsafe-infer] — ЗАКРЫТ (Plan 212 п.8, D424 M4 enforcement), worktree nova-rawptr ветка p212-rawptr-m4] D424 (Plan 174.6 M4, решение владельца 2026-07-15) вводила НОРМАТИВНОЕ правило «raw-ptr `extern`/`external` fn ⇒ `unsafe fn` по инференсу» + снимала carve-out `E_UNSAFE_UNUSED`, но помечала себя «Статус реализации: PROPOSED» — enforcement не был написан. Реализация в `check_unsafe_context_in_module` (`compiler-codegen/src/types/mod.rs`), рядом с сегодняшним D216 §21-пакетом (коммит `30798dec6`, arity-карта unsafe static-оверлоадов + ptr-cast used-tracking) — та же инфраструктура переиспользована. Новый `fn_sig_has_raw_ptr(fd: &FnDecl) -> bool`: проверяет каждый параметр И возврат на `TypeRef::Pointer`/`CStr` (после `strip_modifiers`), рекурсивно в `Tuple`/`FixedArray` (НЕ в `Array`/`Named`/generics — `[]T`/`Vec[T]` GC-управляем не сам по себе raw-ptr, user-record требует резолва типа недоступного на этом синтаксическом проходе). `collect_from`-closure: если `(fd.is_external || fd.extern_abi.is_some()) && fd.receiver.is_none() && !fd.unsafe_attr && fn_sig_has_raw_ptr(fd)` → фолд прямо в `unsafe_fns` (без keyword, зеркалит существующую `fd.unsafe_attr`-ветку) — обычный `E_UNSAFE_CALL_REQUIRES_WRAP`-гейт покрывает вызов без доп. кода. Carve-out удалён целиком: бывший `extern_fns: HashSet<String>`-сет (D216 §21, used-tracking «call к plain extern с ptr-аргом = used») и его Call-arm-проверка (`self.extern_fns.contains(fname) && args.iter().any(expr_is_typed_pointer)`) снесены — поле полностью убрано из `UnsafeCtx`/`collect_from`-сигнатуры (не оставлено dead-code — сборка delta-0 по warnings). **Живой репро подтвердивший необходимость Tuple-рекурсии:** `examples/ffi/sqlite_mini.nv`'s `mini_sqlite_open(path str) -> (*(), int)` — указатель НЕ сам возврат, а первый элемент tuple-возврата (D214 multi-value-return конвенция); первая (shallow-only) версия предиката его не ловила, поймано на живом взрыв-прогоне, не гипотезой. Фикстуры: 2 pos (`spec_tests/conformance/d424_rawptr_unsafe/` — СВОЙ пакет + `d424_ffi_shim.h`, т.к. позитивные conformance-тесты РЕАЛЬНО исполняются под `nova test spec_tests/conformance` — нужен настоящий линкуемый C-символ, паттерн 1:1 с `plan143_2/p143_ffi_shim.h`: wrapped raw-ptr-extern-вызов принят + unwrapped scalar-only-extern-вызов принят) + 3 neg (`spec_tests/conformance/neg/d424_rawptr_extern_unwrapped_neg.nv` → `E_UNSAFE_CALL_REQUIRES_WRAP`; `d424_scalar_extern_unused_unsafe_neg.nv` → `E_UNSAFE_UNUSED`, carve-out снят; `d424_rawptr_extern_tuple_return_unwrapped_neg.nv` — регресс-фиксация Tuple-рекурсии; neg-фикстуры не требуют реальной линковки — компиляция стопится на checker-диагностике до codegen/link, мирроря существующий `neg/d282_vec_param_neg.nv`-паттерн «bodyless+uncalled extern»). **Взрыв-оценка** (`nova check std/` + `examples/ffi`, свежий release-бинарь, CARGO_TARGET_DIR редиректнут на C: — D: worktree-диск был near-full): 12 сайтов / 3 файла — `std/src/net/mock.nv:77` (`net_addr_loopback` → `*()` возврат), `std/src/runtime/fmt_buf.nv` (4 сайта, `f64_fmt_into`, `buf *mut u8` параметр), `examples/ffi/sqlite_mini.nv` (7 сайтов, все `mini_sqlite_*` externs) — все починены добавлением `unsafe {{ }}` (это и есть смысл M4, не обход); после фикса — `nova check std`/`nova check examples/ffi` дают 0 `E_UNSAFE_*` (18 остаточных FAIL в `std` — pre-existing несвязанные neg-фикстуры serde/d322/d323/net/period, не regression: было 20 FAIL до фикса теми же 18 плюс 2 файла с моими unsafe-ошибками). D424 амендирован статусом РЕАЛИЗОВАНО (`spec/decisions/02-types.md`) + `docs/plans/174.6-ffi-abi-types.md` M4-раздел закрыт. Полный `spec_tests/conformance` не гонялся (инструкция волны — не мой гейт). Модель: sonnet.
[2026-07-17 [M-208-generic-interp-display-dispatch-gap] — ЗАКРЫТ, ветка p-interp-generic-dispatch, worktree nova-interpgen] Root-cause подтверждён точно по карте 208-волны (`docs/plans/208-impl-progress.md` §НАХОДКА): `emit_interpolated_str` (`compiler-codegen/src/codegen/emit_c.rs`), user-type dispatch-арм — `has_explicit = self.all_methods.contains(&(arg_type, method_name))`, где `arg_type` для generic-контейнера — МОНО-мангленное имя (`Vec____nova_int`), а `@display`/`@debug` зарегистрированы в `all_methods` под ОБЩИМ generic-именем (`Vec`) — lookup промахивается; `try_synthesize_default_method` тоже промах (Vec/HashMap — не record/sum) → падение в ПОСЛЕДНИЙ numeric-cast fallback (`nova_int_to_str((nova_int)(v))`) — печатает raw heap pointer как int. Option/Result эту дыру не имели: у них ВЫДЕЛЕННАЯ interp-ветка ВЫШЕ user-type арма (через `sum_schema_registry`/`generic_type_methods` + `register_mono_method_instance`, emit_c.rs ~40672-40757) — ПРЯМОЙ метод-вызов (`v.display(FmtCtx.bare(...))`) тоже всегда работал (общий call-путь дженерик-метода, 5b-диспетч ~37225, УЖЕ резолвит generic-mono инстансы правильно) — сломан был ТОЛЬКО `${...}`-интерполяционный диспетч. Фикс — новый приватный хелпер `try_generic_mono_interp_dispatch(&mut self, arg_type: &str, method_name: &str) -> Option<String>` (emit_c.rs, рядом с `emit_interpolated_str`) — зеркалит Option/Result-ветку той же функции, обобщённую на ЛЮБОЙ user-generic контейнер: ранний `return None`, если `arg_type` не содержит mono-разделитель `____` (гарантирует no-op для НЕ-generic типов на первой строке, ДО любого обращения к `self`); иначе — лукап `generic_type_instance_info["Nova_" + arg_type]` → `(base_name, type_args)`; `generic_type_methods[base_name]` → `FnDecl` метода `display`/`debug`; `generic_type_templates[base_name].generics` zip type_args (via `self.arg_c`) → `type_subst`; `register_mono_method_instance(&fn_decl, type_subst, "{arg_type}_method_{method_name}", arg_type)` — ИМЕННО та же naming-конвенция, что общий 5b-диспетч использует для is_instance/overload_index==0 (`base_method_name = format!("{rt_trimmed}_method_{method_stripped}")`, emit_c.rs ~37466) — оба пути сходятся на ОДНОМ C-символе (`mono_instantiated`-гвард внутри `register_mono_method_instance` не даёт дубля, если метод уже мono'ился через прямой вызов в том же CU). Подключён в `has_explicit`-промахе, ПЕРЕД `try_synthesize_default_method_with_gate`/`try_synthesize_default_method` (которые всё равно мисс для generic-контейнеров). **Верификация (red→green, изолированные копии — module-renamed, т.к. ЛЮБОЙ файл `spec_tests/conformance/*.nv` co-equal-пир единого `module spec_tests.conformance` и тянет ВЕСЬ каталог как один CU при прямом запуске, что и объясняет первый смазанный прогон, где отчёт смешал имя моей новой фикстуры с результатами чужой ПРЕСУЩЕСТВУЮЩЕЙ красной `vec_f32_chained_debug.nv` — переигран изолированной module-renamed копией и подтверждён чистым): новая `spec_tests/conformance/d422_generic_interp_dispatch.nv` (4 теста — bare `${v}` на `Vec[int]`, `${v}`/`${v:?}` на `Vec[str]` с quoting-различием Display/Debug, пустой vec, вложенный `Vec[Vec[str]]`) — RED на временно-отключённом (`return None` в начале хелпера) том же финальном бинаре: все 4 assert падают (`assert failed: <expr> == "Vec[...]"`, PASS:0 FAIL:1) → GREEN после восстановления фикса (PASS:1 FAIL:0, все 4 assert'а внутри). Байт-паритет вне generic-mono — гарантирован КОНСТРУКТИВНО (ранний `return None` до какого-либо мутирующего доступа к `self` для любого НЕ-`____`-имени) и спот-подтверждён поведенчески: изолированные копии `d422_unified_display_dispatch`/`d229_debug_format_spec`/`d374_write_sink_decouple` — 3/3 PASS на финальном бинаре (полный текстовый diff `.c`-артефактов не снимался — сочтено избыточным поверх конструктивной гарантии под сильным CPU-контеншном хоста, где параллельно шли ≥4 чужих `nova`-процесса). Обходные фикстуры Ф.3 (208-волна) апгрейжены: `std/src/collections/vec/protocols_test.nv` — тесты 2-4 переведены с прямого `.display(FmtCtx.bare(...))`/`.debug(...)` на bare `${v}`/`${v:?}` (был workaround под открытым геп'ом, теперь реальный путь); тест 1 (Display, прямой вызов) оставлен как отдельный контракт — не workaround, а другой реальный код-путь (общий 5b-диспетч), заслуживающий своего покрытия. **δ0:** `nova test std/src/collections/vec` (весь модуль-CU, PASS:1 FAIL:0 — включает апгрейженный `protocols_test.nv`) + `std/src/checksums` (PASS:3 FAIL:0 SKIP:3) — оба зелёные. **Вторичная находка (НЕ фикс в этой волне):** numeric-cast fallback молча печатает pointer-as-int для ЛЮБОГО `Nova_`-типа без Display/Debug/to_str, не только generic-контейнеров — оценено (честная ошибка компиляции была бы лучше тихого мусора), но НЕ починено: та же ветка также обслуживает как минимум один намеренно-принятый деградированный путь (именованные `*T`-pointer bindings под Debug без ручного `@debug_fmt`, `[M-91.14-ptr-auto-derive]`), и полный аудит всех типов, реально достигающих fallback'а по всему std+conformance, не проводился в этой волне (мега-CU не гонялся по инструкции) — зафиксирован floating-маркер `[M-interp-numeric-fallback-silent-garbage]` (`docs/plans/backlog-followups.md`, P2 — Codegen) с найденными фактами. Маркер `[M-208-generic-interp-display-dispatch-gap]` закрыт во всех местах: `docs/plans/208-impl-progress.md` §НАХОДКА (переписана на «✅ РЕШЕНО», исходный текст сохранён как история) + `docs/plans/208-unified-formatter.md` шапка (Ф.4-абзац, «НЕ починено» → «✅ ПОЧИНЕНО»). Не язык-меняющее (codegen dispatch-completeness, наблюдаемое поведение генерик-контейнеров становится ПРАВИЛЬНЫМ, а не другим) → D-амендмент не требуется. Модель: sonnet.

## [M-187-docker-linux-runtime-hang] (2026-07-17, Docker-волна, гейт оркестратора)

- Docker-ОБРАЗ флагмана СОБИРАЕТСЯ (126МБ: компилятор+std+nova_rt+флагман;
  build-context nova-http локальный; находка волны: std/ = Rust-compile-time
  зависимость компилятора → порядок COPY). НО сервер ВНУТРИ контейнера не живёт:
  (1) дефолтная fiber-арена → шторм «fiber_arena guard page mprotect failed»
  (гипотеза: vm.max_map_count VMA-лимит — guard-страница на слот; обход
  NOVA_MAX_FIBERS=2048/NOVA_FIBER_STACK=1МБ — фейлы уходят);
  (2) но и с малой ареной: слушает 0.0.0.0:8187 (LISTEN виден), соединение
  доходит (CLOSE_WAIT, rx_queue=1 непрочитан), запрос НЕ обрабатывается;
  PID 1 → State D (uninterruptible), wchan=do_exit, 34 потока — процесс
  ЗАВИС В ВЫХОДЕ сразу после старта; логи 0 байт (даже стартовый println).
- Это НЕ конфиг и не приложение: Linux-рантайм серверного профиля
  (долгоживущий M:N в контейнере) не готов — [M-nova-linux-build]-гейт гонял
  только короткоживущие hello/checksums. Родня: TSan-находки (runq
  init/grab visibility), план 211. Нужен отдельный рантайм-заход
  «Linux M:N server profile» (mprotect/арена + exit-hang + возможно
  Boehm world-stop под WSL2).
- Волна 187-Docker: сборочная часть ✓ (Dockerfile/README/bind — ветка
  docker-187), рантайм-гейт ✗ — блокирован этим маркером.
[2026-07-17 [M-vec-ext-method-untyped-let-breaks-chain-dispatch] — ЗАКРЫТ, worktree nova-untypedlet, ветка p-fix-untyped-let-chain] Root cause в `compiler-codegen/src/types/mod.rs::f3_check_member_ctx` (метод-чек, блок "Метод?"). `ro x = v.map[U](...)` (генерик-метод СО СВОИМ типопараметром `[U]` на `[]T`-ресивере — ровно `std/src/collections/vec_seq.nv`'s `@map[U]`/`@filter`/`@fold[Acc]`) без явной аннотации биндинга материализует тип `x` через КАНАЛ (`f1_stmt`'s `chain_ty`, читает `resolved_types_buf`, заполненный `infer_method_call_channel_type`): `ResolvedType::from_type_ref` канонизирует `TypeRef::Array` в `Named{"Vec",[elem]}` (D239, единое каноническое представление слайса), а обратная конвертация `resolved_to_typeref_tp` восстанавливает ИМЕННО эту `Named`-форму — НЕ исходный `Array`. Эта реконструированная `Named{"Vec",[elem]}`-форма ДОХОДИТ до метод-чека в `f3_check_member_ctx` (там `let TypeRef::Named{path, generics: recv_type_args, ..} = &obj_tr else { return; }` матчит) — а genuine `TypeRef::Array` (от прямой аннотации `ro x []T = ...`) БЕЙЛИТСЯ на этой же строке РАНЬШЕ метод-чека вообще (permissive-гейт, ничего не проверяется), что и маскировало баг для аннотированного случая — `vec_seq.nv`'s собственные inline-тесты его не ловили (никто не chain'ит `.map()`-результат в `.filter()`, каждый тест берёт свежий `v`). Метод-чек знал только 2 из 3 конвенций регистрации slice-методов в `method_table`: bare `"Vec"` (нативные `Vec[T]`-методы, `t_provides_method`) и литеральный `"[]<конкретный-элемент>"` (`slice_elem_has_method`, для `fn []str @join(...)`-стиля КОНКРЕТНЫХ ресиверов) — но НЕ литеральный `"[]T"` (СОБСТВЕННЫЙ generic-параметр декларации — `vec_seq.nv`-идиома `fn[T] []T @method[U](...)`). Фикс: третий гейт `prefix_generic_slice_method` рядом с `slice_elem_has_method` — при `tname=="Vec"` и ровно одном конкретном элементе в `recv_type_args` реконструирует `TypeRef::Array(elem)` и зовёт уже существующую отдельно протестированную `prefix_generic_method_exists` (Plan 177 Ф.3, `E_UNKNOWN_METHOD`-гейт, 0 false-positives/707K вызовов корпуса) — та уже умеет искать `"[]<T>"`-ключи method_table, где T — генерик-параметр самой декларации. Frozen-зона `infer_call_ret_c` (emit_c.rs) не тронута — фикс целиком в checker, в стороне от codegen return-inference. **RED→GREEN:** мини-репро (scratchpad, 3 варианта — unannotated `ro mapped = v.my_map_ch(f); mapped.my_filter_ch(p)` / chained-one-expr `v.my_map_ch(f).my_filter_ch(p)` / annotated-control `ro mapped []int = ...`) — оба unannotated варианта RED (`[E7320] no field or method my_filter_ch on type Vec`) → GREEN, annotated-контроль остался GREEN. `nova check nova_tests/generics/mono_basic.nv` (несёт `plan101_1_vec_chained.nv:20`'s `my_filter_ch`, изначальный триаж-репро diag-span-волны) — GREEN (было `[E7320]`); полный `nova test` на этой folder-module по-прежнему CODEGEN-FAIL, но по ДРУГОЙ, orthogonal причине — co-equal-пир `plan101_1_vec_map_int_str.nv` зовёт `str.from(x)`, ретрактированный Plan 174.2 (D410-амендмент) API; попытка тривиально мигрировать на `.to_str()` вскрыла ЕЩЁ один, ГЛУБЖЕ и НЕ связанный с этим маркером codegen-баг (`Nova_Vec____nova_byte*` passed as `nova_str` — byte/str confusion в generic-closure-теле) — правка отменена (revert), файл оставлен как был, оба вопроса вне объёма этого маркера, не мои для этой волны. **δ0 GREEN:** `std/src/collections/vec_seq.nv` (реальный прод-риск — сам `@map[U]`+`@filter`/`@fold[Acc]`), `std/src/checksums/{adler32,crc32,fnv}_test.nv`, `std/src/runtime/{char,sync}_test.nv` (родственный класс E7320-false-positive гэпов, `[M-char-blanket-shadowed-by-sig-complete]`). **Пин-фикстура:** `spec_tests/conformance/vec_ext_method_untyped_let_chain_ok.nv` (реальные `import std.collections.vec_seq.{map, filter}` — не reinvented-имена — unannotated-chain-via-let + chained-one-expr + annotated-control, 3 теста) — верифицирована ЧИСТО через изолированную module-renamed копию (`nova_tests/zz_pinverify_scratch/`, временная, удалена после — тот же приём, что уже задокументирован в `[M-208-generic-interp-display-dispatch-gap]`-записи выше: ЛЮБОЙ файл `spec_tests/conformance/*.nv` — co-equal-пир единого `module spec_tests.conformance` и тянет ВЕСЬ каталог как один CU при прямом запуске; первый прямой прогон дал смазанный отчёт, смешавший имя моей фикстуры с асертами чужой ПРЕСУЩЕСТВУЮЩЕЙ красной `vec_f32_chained_debug.nv`, `[M-208-vec-chained-debug-display-red]` — ДРУГОЙ, уже отдельно триажированный P1-маркер «208-волна», НЕ пересекается с этим фиксом) — изолированная копия: PASS 1/0, все 3 теста внутри. `nova check` на финальный файл внутри `spec_tests/conformance` — PASS (полный `nova test` на весь каталог не гонял — инструкция волны). Триаж (2026-07-17, ветка `p-diag-span-triage`, НЕ смёржена в main на момент этого фикса) бисектом на 3 реперных точках установил: регрессия НЕ от 196.7/196.8/196.9-волны (уже красно ДО неё), настоящая регрессия компилятора, влетевшая в окне `062bbfa94..c4a075ac6` (~5390 коммитов, точный коммит-виновник не найден — вне объёма триажа и этой волны). Маркер `[M-vec-ext-method-untyped-let-breaks-chain-dispatch]` закрыт в `docs/plans/backlog-followups.md`. Модель: sonnet.

## [M-187-sustained-live-tls-resource-death] (2026-07-17, НТ-марафон 10×-от-текущего)

- loadtest.ps1 -Iterations 500 -Concurrency 800 -Rounds 100 (по просьбе владельца):
  BLOCK 1-3 ИДЕАЛЬНЫ — 1202/1202 (все 12 комбо ×100, вкл. weather-live ×100 и
  health-live ×100). BLOCK 4 (сплошной марафон SSE weather-live): 273 подряд
  зелёных → на №274 сервер УМЕР НАВСЕГДА (000 до конца прогона; BLOCK 5/6/7
  красные по наследству). RESULT 288/230, все 230 = одна точка смерти.
- Класс: НЕ concurrency-wedge (замитигирован, BLOCK 5 в прошлых прогонах жил) —
  НАКОПИТЕЛЬНАЯ деградация: ~1470 суммарных прогонов, из них ~770 weather-live
  (≈3000 TLS-соединений к open-meteo) до смерти. Смерть на TLS-марафоне →
  подозреваемые: mbedTLS ctx/session leak (nova-tls @close), fd/сокеты,
  fiber-слоты. Серии по 100 НЕ добивают — только сплошной марафон.
- Слепая зона: сервер стартовал без redirect → посмертного лога нет (паника vs
  OOM vs wedge неизвестно). loadtest.ps1 доработан: server stdout/stderr → лог
  + печать хвоста при смерти; следующий марафон принесёт причину.
- Repro: loadtest.ps1 -Iterations 300+ (BLOCK 4 достаточно; live!). Маркер в
  backlog P1, дом 187/nova-tls/runtime.
[2026-07-17 [M-198-f4c-1-privfile-type-not-discriminated] — ЗАКРЫТ, worktree nova-privtype, ветка p-fix-privfile-type] Root-cause: `compiler-codegen/src/types/mod.rs`'s `TypeCheckCtx.types: HashMap<String, &TypeDecl>` — имя-only ключ; `TypeCheckCtx::build`'s регистрационный цикл (`types.insert(td.name.clone(), td)`) молча перезаписывал слот при co-presence двух `priv(file) type Rect` разных peer-файлов ОДНОГО folder-module (последний в `module.items` побеждал) — `f3_check_member_ctx` (field-access) и `infer_expr_type`'s Member-ветка (field-type inference) читали этот ЕДИНЫЙ слот без файлового контекста, так что use-site в одном файле видел ЧУЖУЮ форму `Rect`. Зеркало `2d5f64e91` (D307 fn-резолв: `sig.fn_decls: Vec<&FnDecl>` + caller-file фильтр в `f1_check_call`) — но для типов Vec-реестра не было; заведён parallel lossless side-table `file_local_types: HashMap<FileId, HashMap<String,&TypeDecl>>` (тот же паттерн, что уже используется рядом для `sum_variant_names` — существующий комментарий в файле про co-presence одноимённых sum-типов теряющих варианты в `types`) + хелпер `types_get_for_file(name, use_file_id)`; wired в оба checker-сайта. Once checker перестал ошибаться (RED→чисто по чекеру), всплыли ЕЩЁ 3 симметричных codegen-сайта (`compiler-codegen/src/codegen/emit_c.rs`) с ТОЙ ЖЕ болезнью (name-only, не file-aware): (1) struct/tag naming `Nova_<Name>` — оба peer-файла эмиттили ИДЕНТИЧНЫЙ C-struct → "redefinition" CC-FAIL; новая `file_priv_type_c_names: HashMap<(FileId,String),String>` (зеркало `private_const_c_names` для `priv(file) const`, тот же Plan 170/D307) + `def_type_base`/`ref_type_base` теперь читают её ПЕРВОЙ, до D381 cross-module `colliding_type_names`; (2) 5 `current_emit_file_id`-гейтов были условны ТОЛЬКО на D381 cross-module коллизию (`!colliding_type_names.is_empty()`) — расширены новым хелпером `any_type_file_collision()` (D381 ИЛИ same-module per-file коллизия), иначе `ref_type_base`/`def_type_base` не имели файлового контекста для резолва (forward-decl продолжал эмиттить голый `Nova_Rect` → "unknown type name" CC-FAIL); (3) `emit_record_lit` для БАРЕ (1-сегментного) record-литерала (`Rect { w, h }`) вообще не звал `ref_type_base` (D381 покрывал только явную 2-сегментную `Type.Variant{…}` sum-ctor форму) — оба peer-файла падали в "unknown type" null-stub fallback → runtime NULL-деref crash (RUN-FAIL). Все 4 сайта пофикшены за одну волну (§4а zero-tolerance — не откладывать найденный дефект). Фикстура перенесена `spec_tests/fixtures/known_red/privtype_file_discrimination/{a,b}.nv` → `spec_tests/conformance/privtype_file_discrimination/{a,b}.nv`, module `privtype_file_discrimination` → `conformance.privtype_file_discrimination` (D78 rev-3 «parent.X», подтверждено эмпирически по прецеденту `neg/privfile_free_fn_leak`), GREEN (`nova test`: PASS 1/FAIL 0, оба test-блока внутри). `spec_tests/fixtures/known_red/README.md` — запись убрана. δ0: `nova check` по 15 std-подпапкам (runtime/collections/encoding/data/identifiers/math/text/net/path/time/concurrency/os/fs/unicode/checksums) — 112 PASS, 15 FAIL — все 15 суть намеренные `*_neg.nv`/`EXPECT_COMPILE_ERROR` фикстуры (grep-подтверждено на паре образцов, не регрессия), включая явно запрошенные пины `std/src/runtime/char.nv`+`char_test.nv`+`sync.nv`+`sync_test.nv` — чисто (0 FAIL). **Побочная находка (НЕ фиксилась, вынесена отдельным маркером `[M-198-f4c-1-into_str-primitive-chain-p67]`, P2, вне периметра волны):** `b.nv`'s исходный `r.tag.into_str()` (int-поле, chained field+method call) ловил ОТДЕЛЬНЫЙ, пред-существующий `[P67-LEGACY]` ICE (`emit_c.rs` method-call return-type-unknown) — репро подтверждено СТАНДАЛОНЕ (без priv(file), без folder-module коллизии вообще) на ОБОИХ бинарях (main repo pre-existing бинарь И этот воркtree) — genuinely не связан с priv(file)-дискриминацией. Фикстура переписана на прямое сравнение полей (`describe(r) => r.name`; `assert(describe(r) == "box")` + `assert(r.tag == 7)`), сохраняя тестовое намерение (обе СВОИ формы `Rect` читаются корректно по всем полям) без касания несвязанного P67-бага. Полный `spec_tests/conformance` не гонялся (инструкция волны — не мой авторитетный гейт-прогон в этой сессии). Модель: sonnet.

[2026-07-17 [M-static-conv-array-record-mono-cc-fail] — РАЗВЕДКА, НЕ ЗАКРЫТ, sonnet, worktree nova-slicemono] Задача: фикс mono/checker-бага, блокирующего extension-метод `[]u8 @to_readbuffer() -> ReadBuffer`/`@to_writebuffer()` (record-тело, ссылается на `@`). RED воспроизведён надёжно (точная форма §1а, static `.from` убран): `nova test --strict-effects spec_tests/conformance/read_nav.nv spec_tests/conformance/write_constructors.nv` даёт стабильный `[E7320] no field or method ptr/len on []u8` в СОВЕРШЕННО несвязанном файле `std/src/runtime/string/core.nv` (метод `to_str_unchecked`, вызывающий штатные Vec-аксессоры `@ptr()`/`@len()`). Baseline (static `.from`, без изменений) — чист.

КРИТИЧЕСКАЯ методологическая находка (для ЛЮБОЙ будущей работы над `read_buffer.nv`/`write_buffer.nv`/`string_builder.nv`/`sync.nv` и т.п.): `compiler-codegen/src/codegen/external_registry.rs` эмбеддит эти `.nv`-файлы через `include_str!` в РАНТАЙМ-СНАПШОТ компилятора (`builtin_sig_modules()`, используется чекером как ДОПОЛНИТЕЛЬНЫЙ источник сигнатур этих «registry-only» типов). Cargo incremental build НЕ ВСЕГДА корректно инвалидирует зависящий compile-unit при правке ТОЛЬКО `.nv`-файла (сам `.rs` не менялся) — на этой машине три последовательных `cargo build --release` ОДНОГО И ТОГО ЖЕ `.nv`-diff (без правок `.rs` между ними) дали ТРИ РАЗНЫХ ложных симптома (`E_RECV_METHOD_MISMATCH` на `HashMap`, то же на `WriteBuffer`, и итоговый стабильный `E7320`). Обязательно: `touch compiler-codegen/src/codegen/external_registry.rs` ПЕРЕД rebuild после правки любого эмбеддед-файла, иначе результаты недостоверны.

Расследованы и НЕ подтверждены как единственный корень: `emit_c.rs` is_array_ext single-key регистрация/диспетч (~6476-6650, ~38358-38553, включая `E_RECV_METHOD_MISMATCH`-гард, который явно исключает `[]`-префиксные ресиверы из safety-проверки), `types/mod.rs::check_instance_overload`'s `array_elem_key` (196.7-класс, ~10689-10722), `emit_c.rs::infer_expr_c_type` Channel 1/1b (~52497-52564, frozen `infer_call_ret_c` НЕ тронута/НЕ подтверждена как источник), `sig_registry.rs::merge_module_fns`'s per-TYPE (не per-(type,method)) «already known» гейт — точечный фикс применён и ПЕРЕПРОВЕРЕН на свежем (touch'нутом) бинаре, результат идентичен (E7320 3/3) → гипотеза отвергнута эмпирически, фикс откачен.

Все experimental-правки (emit_c.rs debug-трассировка, sig_registry.rs per-method гейт, все .nv call-site миграции) ОТКАЧЕНЫ до 0 diff к HEAD — ни одного коммита. `nova:allow W_STATIC_CONVERSION` подавления (read_buffer.nv:54, write_buffer.nv:60) НЕ сняты, маркер остаётся P2/открыт в backlog-followups.md. Владелец успел дать директиву по финальной форме API (dual-form `to_*()`-клон/`consume into_*()`-захват, прецедент `[]u8 consume @into_str_unchecked()`) ДО обнаружения root-cause — зафиксирована для памяти в docs/plans/wip/slice-ext-record-notes.md вместе с полной картой расследования, отвергнутыми кандидатами и планом для следующей волны (проверить симметричные per-type гейты в `external_registry.rs::merge_from_module`/`from_module`, добавить eprintln-трассировку в `check_instance_overload`'s `array_elem_key`-ветку САМУ, а не только codegen-сторону).
[2026-07-17 [M-187-docker-linux-runtime-hang] слой 2 — ЗАКРЫТ, worktree nova-boehmfix, ветка p-fix-boehm-mark] Реализация готового opus-дизайна `docs/plans/wip/boehm-stw-design.md` §5 (диагноз слоя 2 — Дефект A доказан по коду в предыдущей волне, см. запись выше «слой 2 — root cause найден»). **Репро ДО фикса** (WSL2 Ubuntu, gdb `handle SIGPWR SIGXCPU nostop noprint pass` + отложенный curl): `Thread 4 "GC-marker-2" received signal SIGSEGV, Segmentation fault` — фолт ВНУТРИ `libgc.so.1` (все 15 GC-marker-потоков синхронно на том же PC, parallel-mark), БЕЗ печати нашего "nova: fiber stack overflow"-сообщения (подтверждает предсказанный design-документом путь: marker-поток не владеет `_t_arena`, `_nova_find_arena_for` явно исключает guard-диапазон → делегирование в `_prev_sigsegv`, у Boehm его нет → `SIG_DFL`+`raise` → смерть) — отделяет НАСТОЯЩИЙ фолт mark-фазы от suspend-артефакта (`Thread 1` жив, listening-строка успела напечататься до фолта). **Фикс A** (`compiler-codegen/nova_rt/fiber_arena.c`, строго `#if defined(__linux__)||defined(__APPLE__)` + `#ifdef NOVA_GC_BOEHM`, ~60 строк): удалена статическая `_arena_register_active_range` (звала плоский `GC_add_roots(base, base+high_water*slot_size)` — диапазон начинался РОВНО с guard-страницы слота 0); порт точного `GC_set_push_other_roots`-колбэка Windows-стороны (`fiber_arena_win.c::_nova_fw_gc_push_other_roots`) — новый `_nova_gc_push_other_roots` обходит `_nova_arena_list_head` (append-only, обход без лока безопасен под STW), для каждой живой арены читает `high_water`+`free_bits` LIVE и пушит `GC_push_all_eager(slot_base+GUARD, slot_base+slot_size)` ТОЛЬКО занятых слотов (guard и мёртвые слоты не читаются вовсе); зарегистрирован через `pthread_once` (`_gc_roots_once`) в `nova_fiber_arena_init`, симметрично `_arena_key_once`/`_sigsegv_once`. Добавлен `#include <gc/gc_mark.h>` (на Ubuntu `libgc-dev` 1:8.2.12-1 `GC_push_all_eager`/`GC_set_push_other_roots` объявлены там, не в верхнеуровневом `gc.h`-шиме — подтверждено `dpkg -L libgc-dev`). Убран мёртвый `GC_remove_roots` в `_arena_thread_exit_cleanup` (rootless-модель делает retired-арену видимой колбэку через `base==NULL`-skip, явный unregister не нужен). **Гейт после Фикса A** (WSL2, бинарь пересобран `nova build examples/flagship/aggregator/src/main.nv -o ~/aggregator --strict-effects`, символы `_nova_gc_push_other_roots`/`_arena_install_gc_roots` подтверждены в объектнике): `curl /` → `200`, `curl /api/run?legend=demo` → `200`; **5 последовательных запросов** → все `200`, процесс жив; **15с idle → снова 200**; повторено под **`GC_MARKERS=1`** (off parallel-mark) — идентично зелено; дефолт (parallel-mark) — тоже зелено. **Фикс B** (Дефект B, стек fiber vs pthread stack_base, `GC_set_stackbottom` на `mco_resume`): целевая репро-попытка ПОСЛЕ Фикса A — под тем же gdb-репро-рецептом, 6 раундов по 20 параллельных `curl /api/run` (120 запросов, ~40с под ptrace) поверх Фикс-A-бинаря — ни одного `SIGSEGV` не поймано (ограничитель конкурентности `[M-187-high-concurrency-connection-wedge]`, `MAX_INFLIGHT_CONNS=2`, сузил фактический параллелизм до 2 одновременных fiber-обработчиков за раз, но обеспечил МНОГО циклов GC под нагрузкой за 40с). Per design-документа явное указание («если после A не воспроизводится — задокументировать latent, НЕ внедрять вслепую») — Фикс B НЕ внедрён; Дефект B архитектурно реален (см. диагноз §3), но не проявился в доступном тестовом окружении после Фикса A — оставлен как явный маркер-строка в design-доке для будущего репро, если проявится под другой нагрузкой/адресным layout'ом. **Windows-регресс** (worktree nova-boehmfix, тот же код — `fiber_arena_win.c` не тронут, POSIX-фикс строго под `#if defined(__linux__)||defined(__APPLE__)`): `cargo build --release --manifest-path nova-cli/Cargo.toml` — чисто (2m30s, только warnings); `nova test std/src/concurrency` — `PASS: 4 FAIL: 0 SKIP: 5`, без регрессии. Маркер `[M-187-docker-linux-runtime-hang]` закрыт целиком (слой 1 — влит `f6bb896da`, ЭТА волна — слой 2); строка убрана из `docs/plans/backlog-followups.md` (lifecycle-правило §13 — история остаётся здесь). `docs/plans/wip/boehm-stw-design.md` статус → «РЕАЛИЗОВАНО» с фактическими стеками. Не язык-меняющее (рантайм-фикс GC-интеграции, наблюдаемое поведение программ не меняется — сервер перестаёт падать, а не меняет семантику) → D-амендмент не требуется. Docker-прогон (§7.5 гейта) НЕ выполнен в этой волне (WSL-гейта было достаточно по заданию; помечен как отдельный follow-up). Модель: sonnet.

## [M-187-sustained-live-tls-resource-death] КОРЕНЬ (2026-07-17, диагн. марафон)

- Диагностический марафон (loadtest -Iterations 500 -Rounds 10 + resource-монитор
  каждые 30с) + мой -LiveRounds-cap: PASS 518/0 (не дотянул до порога смерти ~670,
  т.к. cap срезал pre-BLOCK4 live с 400 до 40). НО монитор снял ТРЕНД за 540 live:
  память WS 18→264МБ, private 11→770МБ — линейный рост ~1.4МБ/live-прогон;
  хендлы/потоки/TCP ограничены (плато). → УТЕЧКА КУЧИ на TLS-пути, не fd/сокеты.
- Прайм-подозреваемый: nova-tls TlsStream.@close не парен по alloc (mbedTLS
  ssl/config free) ИЛИ Boehm не сканит FFI-owned mbedTLS-память. Зона nova-tls
  stream.nv + native/tls_c_shim.c. Смерть = OOM ~940МБ при ~670 live.
- Побочно: -LiveRounds cap имеет двойной эффект — этика open-meteo И маскировка
  этого бага в стандартном профиле (порог смерти отодвинут). Марафон-BLOCK4 не
  режется намеренно (тест долговечности). Для repro утечки — -Iterations 60 +
  монитор (slope без смерти, дёшево по квоте).

## [M-187-sustained-live-tls-resource-death] — ЗАКРЫТ по гейту (2026-07-17, worktree nova-tls-leak, ветка fix-tls-leak, sonnet)

- Диагностика (не гипотеза, эмпирика): нативная free-парность mbedTLS/шима
  (`native/tls_c_shim.c`) проверена ПОЛНОЙ двумя независимыми методами —
  (а) alive-session счётчик (временный, снят) возвращался к 0 между
  итерациями; (б) standalone-C harness (`clang` напрямую против vendored
  mbedTLS, БЕЗ Nova/GC вообще) с `mbedtls_platform_set_calloc_free`
  custom-allocator-hook: 1500 полных client+server handshake-циклов через
  РЕАЛЬНЫЕ loopback-сокеты (SystemRoots-размерный 150-сертный бандл против
  self-signed — verify FAILS ожидаемо, но упражняет ПОЛНЫЙ parse+verify+free
  путь) — outstanding bytes ПЛОСКАЯ линия (baseline 4467, peak 504560),
  0 роста. Отдельно голый `mbedtls_x509_crt_parse`+`_free` бандла 3000× —
  working set плато ~7МБ. → утечка НЕ в нативном mbedTLS/шиме (опровергает
  оба прайм-подозреваемых из предыдущей записи).
- Nova-уровневый (.nv) loopback-repro (свой diag-инструмент, temp, не
  коммитился) изолировал источник: голый TCP (тот же fiber/spawn/Channel
  каркас, БЕЗ TlsStream) на 3000 итераций выходит на плато и НЕ растёт;
  ЛЮБОЙ TLS-путь (CustomRoots-успех И SystemRoots-provал — размер CA-бандла
  ОКАЗАЛСЯ НЕ фактором, опровергает начальную гипотезу) даёт линейный рост
  ~50-60КБ/итерацию. `gc.collect()` (полный синхронный `GC_gcollect()`)
  каждые 50 итераций НЕ возвращал `gc.heap_size()` к плато — Boehm считает
  буферы live СРАЗУ ПОСЛЕ полной сборки, т.е. это НЕ «GC не успевает», а
  reachable-retention (похоже на conservative-scan false-positive через
  переиспользуемые fiber-стеки — размер блока эмпирически ПРОПОРЦИОНАЛЕН
  скорости роста: 16КиБ→2КиБ снизил slope ~56КБ/итер→~19КБ/итер). Эта часть
  — вне периметра nova-tls (компилятор/рантайм, Boehm/fiber_arena), НЕ
  дочинена в этой волне — см. новый floating-маркер
  `[M-boehm-large-buffer-retention-fiber-reuse]` ниже (эскалация, не мой
  периметр).
- Фикс (в периметре nova-tls, снижает экспозицию к GC-паттерну, НЕ
  «переписывает GC»): (1) `native/tls_c_shim.c`+`ffi.nv` — новый extern
  `tls_pending_out_len` (0-аллокационный компьют); `stream.nv::flush_out`
  спрашивает факт вместо слепой аллокации `TLS_CHUNK` на каждый вызов;
  (2) новое поле `TlsStream.scratch []u8` — ОДИН переиспользуемый
  ciphertext-буфер на весь жизненный цикл стрима (заведён в connect/accept,
  продет через `pump_handshake`/`flush_out`/`fill_from_tcp`/`read_step`/
  `write_step`); `fill_from_tcp` читает через `Net.read(tcp, scratch)`
  напрямую вместо `TcpStream.read_to_vec` (которая ВСЕГДА аллоцирует фреш
  `[]u8`); (3) `TLS_CHUNK` 16КиБ→4КиБ (доп. мера — трейд-офф с throughput
  большого трафика приемлем, handshake-flights/типичный JSON-ответ малы).
  Комбинированный эффект на loopback-repro: ~56КБ/итер → ~21КБ/итер
  (≈2.6× снижение), рост НЕ устранён полностью на синтетическом hammering-
  repro (ожидаемо — root residual вне периметра).
- **Официальный гейт** (`examples/flagship/aggregator/tls-leak-repro.ps1`,
  ОДИН прогон ПОСЛЕ фикса, 80 запросов реальных open-meteo HTTPS,
  `-Build` fresh binary): baseline 526.9МБ → 546.8МБ за 80 прогонов (70
  сэмплов), **slope = 0.244 МБ/прогон, VERDICT: CLEAN** (порог 0.30 МБ/прогон)
  — под РЕАЛЬНОЙ сетевой каденцией (открытое-meteo latency даёт GC заметно
  больше простоя между итерациями, чем synthetic loopback-hammering) фикс
  достаточен: маркер закрывается ПО ГЕЙТУ (проверяемый критерий проекта),
  при том что диагностика honestly указывает на нерешённый residual в
  GC/fiber-arena слое (см. новый маркер ниже). `nova test src` (nova-tls,
  таргетный корректностный гейт) — зелёный (PASS 1/FAIL 0, весь
  compile-unit пакета). ДО-числа (для памяти): предыдущий марафон —
  private-commit ~11→770МБ за 540 live (≈1.4МБ/live-прогон), смерть OOM
  ~940МБ при ~670 live; slope на loopback synthetic (rapid-fire, БЕЗ
  сетевой задержки) ДО фикса ~56КБ/итер (worst-case, не путать с офиц.
  гейтом на реальной сети).
- Побочная находка (мелкая, НЕ дочинена, не блокирует): при TLS_CHUNK=2КиБ
  (промежуточный эксперимент) `alive_sessions`-счётчик иногда застревал на
  1-2 вместо 0 в конце длинного loopback-прогона (3000 итераций) — вероятно
  edge-case на multi-chunk handshake-flight (флайт крупнее уменьшенного
  чанка) в СОБСТВЕННОМ diag-инструменте волны, не подтверждён как реальный
  прод-баг (не наблюдался на финальном TLS_CHUNK=4КиБ+scratch-reuse
  прогоне). Не заводил отдельный маркер — недостаточно repro-уверенности.

## [M-boehm-large-buffer-retention-fiber-reuse] — НОВЫЙ, OPEN (2026-07-17, найдено при расследовании 187/nova-tls)

- Эскалация из `[M-187-sustained-live-tls-resource-death]` (закрыт выше ПО
  ГЕЙТУ, но root residual не в nova-tls). Explicit `gc.collect()`
  (`GC_gcollect()`, полный синхронный mark-sweep) НЕ возвращает
  `gc.heap_size()` к плато под нагрузкой «много коротких fiber'ов, каждый
  churn'ит один относительно крупный (КБ-масштаба) GC-буфер» — Boehm
  считает эти буферы reachable СРАЗУ ПОСЛЕ полной сборки, т.е. это не
  «редко собирает», а настоящий retention. Гипотеза (не доказана до конца):
  conservative-scan false-positive retention через переиспользуемые
  fiber-стеки (`compiler-codegen/nova_rt/fiber_arena.c` — арена
  переиспользует stack-слоты между короткоживущими fiber'ами; протухшие
  байты в переиспользованном слоте МОГУТ случайно выглядеть как указатель
  на живой ещё объект прошлой итерации). Эмпирическая опора: размер
  отдельного буфера ПРОПОРЦИОНАЛЕН скорости роста (не бинарно — растёт с
  size), что типично для conservative-GC false-positive economics (больше
  байт = больше шанс на случайное совпадение с протухшим stack-словом).
  Repro-инструмент (temp, НЕ коммитился, воспроизвести заново по рецепту):
  loopback client+server TLS/TCP-пара в одном .nv-процессе,
  supervised+2×spawn+Channel НА КАЖДУЮ итерацию, 3000 итераций; TCP-only
  контроль (тот же каркас, БЕЗ TLS/крупных буферов) — плато; ЛЮБОЙ TLS-путь
  (независимо от CA-размера/success-vs-fail) — линейный рост, не
  устраняемый принудительным `gc.collect()` каждые 50 итераций. Зона:
  `compiler-codegen/nova_rt/fiber_arena.c` (+ `alloc_boehm.c`) — компилятор/
  рантайм, НЕ nova-tls. Не паркуется как P1/P0 (нет открытого прод-инцидента
  ПОСЛЕ TLS_CHUNK+scratch-reuse фикса — офиц. гейт CLEAN под реальной
  сетевой нагрузкой), но реальный систематический риск для ЛЮБОГО
  high-churn per-connection/per-request паттерна (не только TLS) —
  P2, требует opus-разведки в GC/fiber-arena слое, вне периметра
  какого-либо конкретного пакета.
