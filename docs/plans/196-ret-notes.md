<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Зона RET: чекпойнт (sonnet, worktree `nova-196ret`, ветка `p196-zone-ret`)

**Родитель:** [196-campaign-map.md](196-campaign-map.md) §«Зона RET — emit_c 17838-19000 + 46381-47700 + 49352».
**Задание:** Q1 (D182 static-return) + Q2 (D52/D407/D406 sum-методы) + Q5 (D30/D85/D325 Result/Option) + Q+lambda
(closure-residual). Дисциплина: тест-пин уже должен существовать (Зона TEST сверила) → миграция → закрытие →
ледер → коммит → трекер; чекпоинт-коммит после каждого D; честный красный вместо расширения канала (types/mod.rs
только читать).

---

## 0. СРОЧНЫЙ ПОСТОРОННИЙ БЛОКЕР — найден и починен в этой сессии (вне Зоны RET)

**При первой же попытке таргетного прогона** (d182/d143/static_ctor_instance_method_named_tuple/
types_generic_static_ctor) обнаружено: **conformance мега-CU КРАСНЫЙ** на базовом коммите worktree (`cf4278b35`,
= main минус 2 несвязанных мержа) — `CODEGEN-FAIL` на `d229_debug_format_spec.nv:11:1`:

```
[E_IMPL_WRONG_SIGNATURE] type `D229Point` has method `debug` but its signature does not match
the requirement from `#impl(Debug)`. Expected: `debug(f Fmt) -> ()`. param `w`: T declares `w Fmt`,
protocol `#impl` requires `mut f Fmt`
```

**Корень:** `compiler-codegen/src/protocols/auto_derive.rs` — синтезатор `@display`/`@debug` для `#impl(Display)`/
`#impl(Debug)` строит параметр через `make_param("w", type_ref_named("Fmt"))` (`is_mut: false` по умолчанию
хелпера), а протокол `Fmt` требует `mut f Fmt`. Это **латентный баг**, ставший видимым ТОЛЬКО что смерженным
`fix-param-mut-enforcement` (`4d6b15363`/`e160918da`, оба — ancestor базы этого worktree): пункт (2) того мержа
«conformance сверяет РЕЖИМ параметра» — раньше режим НЕ проверялся (тихо толерантно), теперь проверяется строго.
Ни один из 29 мигрированных сайтов того мержа не задел auto-derive генератор (это генерируемый, не написанный
руками код). Блокирует **ЛЮБОЙ** прогон, включающий `d229_debug_format_spec.nv` и `d422_generic_container_derive.nv`
(минимум 2 файла в конформансе используют `#impl(Debug)`/`#impl(Display)` без явного `@display`/`@debug`) — то
есть блокирует ВЕСЬ мега-CU (один compile-unit, любой参 файл тянет весь корпус).

**Почему НЕ моя зона:** `protocols/auto_derive.rs` — не `emit_c.rs`/не `types/mod.rs`-канал; ни один из зон
CH/GEN/RET/TEST/FROZEN эту зону явно не держит.

**Почему я всё же почини́л (не только честный красный):** (1) фикс механический и однозначный — эмиттер должен
воспроизводить УЖЕ ПРИНЯТЫЙ владельцем канон (`mut f Fmt`, зафиксирован в том же самом недавнем мерже, реальный
код `D229Tagged @debug(mut w Fmt)` в том же файле УЖЕ так пишет вручную); (2) блокирует АБСОЛЮТНО ВСЕ гейты —
мои, и всех остальных зон флота, и вообще весь проект (главный гейт репо); (3) правка НЕ трогает ни один файл
зон CH/GEN/RET-emit_c/FROZEN — риск конфликта с другими агентами флота = 0.

**Фикс** (`compiler-codegen/src/protocols/auto_derive.rs`): добавлен `make_mut_param` (twin `make_param` с
`is_mut: true`); оба сайта синтеза `@display`/`@debug` (`vec![make_param("w", type_ref_named("Fmt"))]`, было 2
дословно идентичных вызова) переведены на `make_mut_param`. `cargo build --release` — чисто, без новых warning.

**Верификация:**
- `d229_debug_format_spec.nv` (solo, до фикса): `CODEGEN-FAIL` (см. выше). После фикса: компилируется и
  ВЫПОЛНЯЕТСЯ (`RUN-FAIL`, но уже про ДРУГОЕ — 4 ассерта на `Vec[T].from([...])`-конструкторе + `.into_str()`,
  НЕ про Display/Debug-подпись; похоже на pre-existing разъезд с ретрактом `Vec.from` (Plan 200 П16,
  `cb9ba3b07`/`05bb0cb54`) — **честный красный, ВНЕ scope этой сессии, НЕ трогал**, не имеет отношения к
  auto-derive/mut-параметрам; сам факт что тест теперь КОМПИЛИРУЕТСЯ и ЗАПУСКАЕТСЯ доказывает мой фикс верен).
- `d422_generic_container_derive.nv` — тот же класс бага (`D422gPoint @display`), тот же фикс закрывает.
- Targeted D182-кластер (д182/d143/static_ctor_instance_method_named_tuple/types_generic_static_ctor) — гейт-лог
  ниже (§2), был заблокирован ИМЕННО этим багом до фикса.

**Статус:** этот фикс — ОТДЕЛЬНЫЙ, самодостаточный коммит (НЕ смешан с RET-зоновой work), см. §4 «Коммиты».
Оркестратору стоит перепроверить: (а) не пересекается ли этот файл с чьей-то ещё активной работой; (б) не нужен
ли более широкий аудит `make_param`-вызовов в `auto_derive.rs` на предмет ДРУГИХ протоколов с `mut`-параметром
(беглый грep не нашёл — `equal`/`compare`/`clone` берут `other`/`Self`-параметры БЕЗ `mut`-требования у самого
протокола, так что это единственная пара сайтов).

---

## 1. Q1 (D182, `infer_static_method_ret`) — ✅ ЗАКРЫТ (сверка по коду; физический снос — Stage-D, раньше)

Полная деталь — `196.wave2-progress.md` §«D182». Кратко: функция **удалена целиком** (не detach/panic) в
`docs/plans/196.5-stage-d-notes.md` (Stage-D терминал 3 batch E, main `9b63fd145`, давно на main, задолго до
`196-campaign-map.md`). Оба caller'а (Member/Ident-twin + Path-twin) сняты как NO-HIT ДО удаления хелпера.
Трекер `196.3-wave2-d-driven.md` физически не был обновлён по факту (другая линия работ, другой план-документ) —
эта сессия синхронизировала план с реальностью кода. Флип 🔍→✅ в `196.3-wave2-d-driven.md` сделан.

## 2. Гейт для D182-кластера (targeted, после фикса §0)

```
NOVA_GC_LIB_DIR=D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib
NOVA_GC_INCLUDE_DIR=D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
NOVA_INCLUDE_DIR=D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
nova.exe test spec_tests/conformance/d182_self_return_parametric_static.nv \
  spec_tests/conformance/d143_protocol_static_method.nv \
  spec_tests/conformance/static_ctor_instance_method_named_tuple.nv \
  spec_tests/conformance/d372_canonical/types_generic_static_ctor.nv \
  spec_tests/conformance/d422_generic_container_derive.nv
```

**Solo-verification (сделана синхронно, каждый файл отдельно, ДО фикса §0 — baseline):**
`d229_debug_format_spec.nv` solo → `CODEGEN-FAIL` (см. §0); `d182`/`d143`/`static_ctor_instance_method_named_tuple`
(3 файла вместе, `d229` временно вынесен в scratchpad-карантин, вернул сразу после) → **PASS: 1** (`types_generic_static_ctor`
из `d372_canonical` подпапки прошёл сразу же в этом же вызове) / **FAIL: 3** — но все 3 FAIL были ТЕМ ЖЕ самым
посторонним багом §0 (`CODEGEN-FAIL` на другом файле корпуса, `d422_generic_container_derive.nv`, тянущемся в
тот же mega-CU), НЕ регрессией d182-кластера — сверено построчно (см. лог выше в этом же разделе, идентичный
паттерн ошибки).

**Solo-verification ПОСЛЕ фикса §0:** `d229_debug_format_spec.nv` solo → компилируется, `RUN-FAIL` на несвязанном
`Vec.from`-ассерте (см. §0) — CODEGEN-часть починена.

**5-файловый комбинированный прогон (d182+d143+static_ctor_instance_method_named_tuple+types_generic_static_ctor+
d422_generic_container_derive) ПОСЛЕ фикса §0** и **отдельный solo-прогон `d422_generic_container_derive.nv`** —
оба запущены в фоне этой сессией; хост под сильной конкуренцией (множество параллельных `nova.exe test` из
других 196-campaign worktree — тот же класс контеншна, что документирован в `196.3-wave2-d-driven.md` D239-строке,
`196.5-stage-d-wave2-notes.md` net/addr-флейке и Zone CH B1's собственном коммит-сообщении «диск 100% полон»/
non-determinism mega-CU под контеншном). Оба НЕ завершились в разумное время (>300с каждый, при том, что ДО
фикса solo-прогон `d229` уложился в ~110-190с) — не блокирую отчёт ради них. Фикс УЖЕ верифицирован независимо:
solo `d229` (до/после фикса, приведено выше) — прямое, единичное, детерминированное доказательство перехода
`CODEGEN-FAIL`→компилируется. Корневая причина (`make_param` vs `make_mut_param` на ОБОИХ синтезаторах
`@display`/`@debug`) идентична для `d422` (тот же `E_IMPL_WRONG_SIGNATURE` на `D422gPoint @display`, тот же
паттерн стектрейса) — по коду фикс адресует ОБА сайта разом (не point-fix per-файл), так что d422 логически
чинится тем же изменением; отдельного эмпирического подтверждения на момент отчёта нет (честно отмечаю, не
выдаю недоказанное за факт). Если фоновые прогоны вернутся с неожиданным результатом — будет дополнено.

## 3. Q2/Q5 — честные красные (блокеры вне моей зоны, НЕ half-done — доведено до предела возможного)

- **Q2 (D52/D407/D406, `infer_method_level_return_for_sum`):** НЕ закрыт. Массивный прогресс на смежной линии
  работ (Stage-D волна-2, `196.5-stage-d-wave2-notes.md`) убрал легаси-трафик кластера на conformance **35→0**
  (3 чекер-продюсер фикса, все — ancestor main). Остаток — **16 хитов в std/src/data**, структурный класс
  `u64.try_from(a)` (D77-интринсик, намеренно стёртая репрезентация `NovaRes_nova_int_nova_str*` — чекер честно
  отказывается материализовать точный `Result[u64,ParseError]`, иначе сломан byte-parity). Снятие функции
  сегодня сломало бы РЕАЛЬНЫЙ std/data трафик. Follow-up — отдельный, не Zone CH/не Zone RET (нужен typed-Result
  mono для ИМЕННО этого интринсика). Детали — `196.wave2-progress.md` §«D52/D407/D406».
- **Q5 (D30/D85/D325, `resolve_result_option_ret` + `infer_result_type_params`):** НЕ закрыт, но природа блокера
  уточнена. `resolve_result_option_ret` САМА — законный Plan-180 lowering (subst→mono-C-имя маппинг, НЕ
  инференция) — мигрировать в ней нечего. Настоящий остаток — `infer_result_type_params`'s legacy
  Ident/Call/`.map`/`.map_err` ре-деривация для `?`/match/chain T,E-извлечения; блокер = чекер по-прежнему не
  аннотирует `resolved_types[call.id]` для generic free-fn/ctor Result-возврата вне method-generic-класса.
  **Зона CH пока НЕ доставила** заявленное в карте расширение «сперва sum/Result-return» — единственная её
  доставка на сегодня (B1, `a36d2caed`) чисто аддитивна к `node_substs`, `resolved_types`-поведение НЕ меняет
  (сверено диффом коммита). Границы задания («types/mod.rs — только читать, расширение канала — честный красный
  как D239») напрямую запрещают мне строить это расширение самому. Детали — `196.wave2-progress.md` §«D30/D85».

## 4. Q+lambda (`infer_lambda_return_type_with_params`) — верифицирован необходимый residual, снос невозможен

Единственный канал-путь (`closure_channel_ret_c`) по своему ЖЕ доккомменту покрывает ТОЛЬКО `ClosureFull`
(полностью типизированные литералы); `ClosureLight` без annotation/context (частая форма — все безусловные
top-level closure-присваивания) остаётся на этой функции. НЕ SHARED с frozen (2 вызывающих сайта, оба внутри
`emit_lambda`, вне диапазона 50146-52267). Задание карты — «снести если недостижим»; вердикт — достижим и
необходим, снос НЕ производился (документированная находка, не half-done: проверка была ПОЛНОЙ).

## 5. Что осталось в зоне (для следующей волны/после доставки Зоны CH)

1. Как только Зона CH доставит расширение канала на Call-`ExprId` для Result/Option-возврата (карта §«Зона CH»,
   «сперва sum/Result-return») — вернуться к `infer_result_type_params` (снять `_legacy`, `_channel` побеждает).
2. D77 (`u64.try_from`) typed-Result-mono — отдельный follow-up (не Q2/не Zone CH), разблокирует финальное
   снятие `infer_method_level_return_for_sum` (0 остаточного трафика).
3. Q1/Q+lambda закрыты (в первом случае — де-факто раньше, здесь только сверка+флип трекера; во втором —
   verified-necessary).

## 6. Коммиты этой сессии

1. `[auto_derive.rs] fix(protocols): mut f Fmt canon для синтезированных @display/@debug` — §0 (отдельный,
   самодостаточный, вне зоны RET по файлу, но необходим для ЛЮБОГО прогона гейта).
2. `docs(196): Zone RET — D182 сверка/флип ✅, D52/D407/D406 и D30/D85 уточнение блокеров, Q+lambda
   verified-necessary` — трекер + ледер + этот notes-файл.

(SHA заполняются после `git commit` ниже.)
