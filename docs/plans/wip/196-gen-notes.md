<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Зона GEN: чекпоинт-заметки (sonnet, worktree `nova-196gen`, ветка `p196-zone-gen`)

**Родитель:** [196-campaign-map.md](196-campaign-map.md) §2 «Зона GEN — emit_c 19485-21432».
**Назначение файла:** непрерывность при обрыве сессии + сырые находки для интегратора (приёмка
по-прежнему ПО КОДУ, интегратором; статусы D-трекера в `196.3-wave2-d-driven.md` НЕ трогал —
это привилегия интегратора при приёмке).

---

## Сделано в этой сессии (коммиты на `p196-zone-gen`)

1. **`c7c7f127e`** — fix `[M-novavtable-read-write-pointer-collision]` (добавка к зоне, backlog
   P2, ЗАКРЫТ). Guard `B11d_typed_pointer_methods` (emit_c.rs, эмиссия ~36083 + инференс-двойник
   ~51490) исключал `NovaArray_`, но не `NovaVtable_` (`"NovaVtable_X*".starts_with("Nova_")` ==
   false — символ на позиции 4 это `V`, не `_`). Нуль-арный `.read()`/одноарный `.write(v)` на
   ЛЮБОМ handler-значении (`NovaVtable_<Eff>*`) мисдиспатчился в typed-pointer-deref вместо
   `B11ac_novavtable_effect`/direct-handler-call. Фикс — явное исключение в ОБОИХ guard'ах.
   Пин расширен в `d61_effect_handler_direct_call.nv` (`D61Guard` effect с буквальными
   `read`/`write` op-именами + тест). Изолированная верификация: PASS 472 FAIL 12 (все 12 —
   pre-existing, см. §2 ниже; ни один не касается d61/NovaVtable).
2. **`3a5f252a3`** — docs: маркер снят из `backlog-followups.md`, закрытие залогировано в
   `simplifications.md`.
3. Стрей `.git`-файл внутри `compiler-codegen/nova_rt/libuv/` (пропущен `robocopy /XD .git` —
   XD исключает только ДИРЕКТОРИИ, а это был gitlink-ФАЙЛ) ломал ВСЕ git-команды в этом worktree
   (`fatal: not a git repository: .../.git/modules/...`) — удалён (`rm -f`, не коммитился, не
   часть репозитория). **Для будущих worktree-агентов: после robocopy libuv всегда проверять
   `ls compiler-codegen/nova_rt/libuv/.git` — если это ФАЙЛ (не каталог), удалить его явно.**

## Пре-существующая, НЕ моя регрессия — НЕ трогал (стоп-волна владельца)

При изолированной верификации (conformance-папка минус `d229_debug_format_spec.nv`, временно
вынесенный и ВОЗВРАЩЁННЫЙ на место после прогона) — `PASS 472 FAIL 12`. Среди 12: и `d229`
(до выноса), и (в самом прогоне) `app_effect_basic_t8_1` — ОБА об одном и том же:
`[E_IMPL_WRONG_SIGNATURE]` на auto-derived `Debug`/`Display` (`D229Point`/`D422gPoint`) —
сгенерированный derive-метод несёт `w Fmt` (bare), а новый канон (мердж
`4d6b15363 fix-param-mut-enforcement`, самый свежий на момент старта этой сессии) требует
`mut f Fmt` буквально. Похоже, auto-derive codegen для Debug/Display не обновлён под новый
канон одновременно с мерджем — систематическая регрессия (минимум 2 файла), НЕ specific to
d229. Координатор уже проверяет это отдельно на main — **не чинить в Зоне GEN** (чужая
стоп-волна). Остальные 10 из 12 — TIMEOUT (`standalone/f*` — известный host-contention флак,
см. `feedback-large-tests-stored-not-in-regress`) + 1 `NEG-NO-ERROR` (`i64_clamp_no_overload_neg`,
не связан).

## Q10 (D119/D122, `resolve_mono_type_args`/`resolve_method_level_subst`) — код-migration УЖЕ
## завершена; закрытие блокировано fallback≠0, вне scope Zone GEN

Проверил по коду (grep всех call-сайтов):
- `resolve_mono_type_args` (~19668, legacy-движок, ~400 строк, doc-комментарий уже фиксирует
  Tier-2/Q10-вердикт) вызывается ТОЛЬКО из `resolve_mono_type_args_ch` (~20097, propose-then-
  verify wrapper) — единственная точка входа. Оба out-of-zone `emit_call`-сайта (~39149, ~39440)
  УЖЕ идут через `_ch`. Внутри frozen-зоны сайт (если есть) звал бы legacy напрямую — не нашёл
  такого (frozen-зона candidate coordinates съехали, см. §0 карты).
- `resolve_method_level_subst` (~20961) имеет channel-first ВСТРОЕННЫЙ (не отдельный `_ch`-враппер
  — сразу читает `node_substs[call_id]` на входе, ~21002-21035) с legacy Steps 1/2/2f/3 как
  fallback. 5 прямых вызывающих (33916/34458/34726/37442/37738) — все уже зовут ЭТУ функцию
  (единственную), не легаси напрямую.
- **Вывод: «флип на `_ch`» (консьюмер-сторона в emit_c.rs) уже СДЕЛАН на 100% — нечего мигрировать
  дальше на этом уровне.** Кампанийная карта («ДОЖАТЬ: завершить флип →_ch») была написана до
  этого состояния или имела в виду именно снос legacy-тела, не флип вызовов.
- **Снос legacy-тела заблокирован количественно, не качественно.** `196.5-stage-c2-notes.md`
  (уже на main, коммит `714a0a781`) даёт точные числа ПОСЛЕ композиции: B1
  (`resolve_mono_type_args_ch`) fallback 784→193 (−75%, но НЕ 0); B2 (`resolve_method_level_subst`)
  true-remaining-fallback 842→~22 (−97%, но НЕ 0). Ноль — обязательное условие приёмки (5)
  («тихий legacy-fallback ЗАПРЕЩЁН» ловушка §4.3) — до 0 остаток закрывают ТОЛЬКО новые
  producers node_substs для форм Source 2+/2f/3 (arg-type inference без turbofish/closure-return-
  bound/structural-bound) — это канал-РАСШИРЕНИЕ (types/mod.rs), **Зона CH**, не Zone GEN.
- **Действие:** НЕ трогал (нечего мигрировать в emit_c.rs; снос legacy — за Zone CH + полным
  corpus-гейтом, вне scope одного sonnet-захода). Статус в трекере оставляю как есть (🔄/Q10) —
  интегратору решать, обновлять ли формулировку кампанийной карты («флип завершён,
  осталось расширение канала», не «завершить флип»).

## Q9 (`infer_type_param_binding`×3 + `infer_protocol_structural_binding`) — ВОЗМОЖНАЯ
## переклассификация, требует решения интегратора/владельца

Кампанийная карта относит эти 4 функции к «→ чекер» (D16/D53/D72/D42/D355). Проверил по коду:
эти функции — НЕ ТОЛЬКО потребляются legacy-движками (`resolve_mono_type_args` Source 2/2b,
`infer_generic_static_ctor_ret` ~19507) — они ТАКЖЕ являются прямой зависимостью
**`resolve_instance_call_subst`** (~20795), которая, по ЕЁ ЖЕ doc-комментарию, — «ЕДИНЫЙ
POST-mono резолвер» W1-i.B, уже ФЛИПНУТЫЙ на authoritative (SHADOW-verified 0 mismatch,
196.5) и являющийся ЦЕЛЕВОЙ (не legacy) архитектурой для frozen-зоны carrier/method-level subst.

Другими словами: `infer_type_param_binding`/`infer_protocol_structural_binding` — это чисто
СТРУКТУРНЫЕ C-строка↔TypeRef подстановочные примитивы (даётся УЖЕ конкретный C-тип, извлекаются
под-биндинги матчингом формы) — ближе к rustc'овскому «mono = подстановка» (не переинференс
семантики из AST), и НОВАЯ архитектура их СОХРАНЯЕТ, а не заменяет. Это отличается от Q10-класса
(`resolve_mono_type_args`/`resolve_method_level_subst`), которые ОРКЕСТРИРУЮТ (решают, ОТКУДА
брать биндинг — turbofish/arg/closure-return/structural), а не просто применяют уже-решённое.

**Не трогал** (не моё решение — переклассификация меняет кампанийную карту; интегратору/владельцу
на рассмотрение). Если переклассификация верна, Q9 может НЕ требовать отдельной чекер-миграции —
функции остаются как разделяемый lowering-примитив для ОБЕИХ (старой и новой) архитектур.

## D239 (`compute_array_elem_type_for_obj` / `channel_array_elem_c`) — не трогал,
## подтвердил прежний вердикт (ЕДИНСТВЕННЫЙ вызов, риск реален)

Grep подтвердил: ОДИН оставшийся вызов `compute_array_elem_type_for_obj` — Channel-6k fallback
внутри `infer_expr_c_type`'s `ExprKind::Index`-арма (~52776), СРАЗУ после `channel_array_elem_c`
(~52769). Прочитал контекст полностью (52700-52820):
- `ExprKind::Ident`-ветка внутри `compute_array_elem_type_for_obj` дублирует уже-выполненную
  ПРЯМО ВЫШЕ (~52754) проверку `array_element_types.get(name)` для НЕ-self-describing типов
  (`obj_ty_self_describing == false`) — там она мертва (тот же lookup уже провалился). НО для
  self-describing типов (`Nova_Vec____`/`NovaArray_`, когда декодирование по mangled-имени НЕ
  удалось на 52739-52753) ветка ~52754 СОЗНАТЕЛЬНО пропущена (комментарий: side-table «poisoned,
  last-wins across peers») — значит `compute_array_elem_type_for_obj`'s Ident-арм в ЭТОМ случае
  НЕ дубликат, а единственный remaining путь (пусть и «отравленный» по документированной
  причине). НЕ доказано мёртвым — не трогал.
- `Member`-ветка (`debt_compute_field_array_elem_type`, глубокий field-chain `obj.f1.f2.field[i]`)
  — живая: единственный явно нарезанный (`Member{obj:SelfAccess,...}`) кейс перехвачен РАНЬШЕ
  (~52760), но ОБЩИЙ `Member` (не через self) — нет.
- `SelfAccess`-ветка (`@[j]` в generic `[]T`-методе) — живая, не перехвачена нигде выше при
  non-self-describing `obj_ty_pre`.
**Вывод: fallback НЕ провабельно 0-hit без полного мега-CU замера по этим трём формам** — ровно
ловушка §4.1 кампанийной карты (снятие БЕЗ доказательства на ПОЛНОМ корпусе → прошлый откат 93/2).
Полный conformance-гейт вне scope этой сессии (правило «полный conformance НЕ гонять») —
оставил как есть, D239 остаётся 🔄.

## D372 (`infer_generic_static_ctor_ret`) — подтвердил: корректно SEP, ПРЕД-канальный по делу,
## не «недосмотренная миграция»

Единственный вызывающий (~52358) стоит НАМЕРЕННО ДО Channel-1/2 в `infer_expr_c_type` (комментарий
~52326-52334: «Channel 2 would… lower it to the ERASED `Nova_Wrap*` and the LHS local's instance
methods would dispatch to the NULL stubs» — то есть каналы СТРУКТУРНО дают НЕПРАВИЛЬНЫЙ ответ для
stub-only generic-ctor без turbofish; функция существует ИМЕННО чтобы перехватить этот кейс до
канала). Функция сама гейтится `generic_type_has_voidptr_fields` — не общий legacy-fallback,
а узкий pre-channel special-case. «→ чекер»-миграция потребовала бы чекеру знать про mono-
инстанциацию НА call-site (per-call `ResolvedType`, Call-return класс) — то есть глубже канала,
чем Зона CH сейчас строит (§0 карты: канал для Call-return классов "ПОСТРОЕН" для Result/Option/
sum/static, но ctor-stub-erasure — другой класс, не упомянут явно). Не трогал — вне emit_c-only
scope Zone GEN, коллизия с Zone CH (types/mod.rs).

## Рекомендация интегратору

- Q10: обновить формулировку карты (флип-на-канал ЗАВЕРШЁН; снос легаси = функция Zone CH's
  producer-расширения + batch-B2 full-corpus гейт, не отдельная задача Zone GEN).
- Q9: рассмотреть переклассификацию `infer_type_param_binding`×3/`infer_protocol_structural_binding`
  как «остаются» (lowering-примитив новой архитектуры), не «→ чекер» — избежать потраченного
  впустую захода на несуществующую миграцию.
- D239/D372: остаются 🔄/SEP как задокументировано; оба требуют либо полного corpus-гейта
  (D239) либо Zone CH channel-расширения (D372) — не «дожимаемы» одним emit_c-only sonnet-заходом.
- Зона GEN emit_c-only остаток после этой сессии: НЕТ дополнительных безопасных, самодостаточных
  (без Zone CH/полного гейта) изменений, которые я мог бы найти по коду. Если у интегратора есть
  другая гипотеза по конкретному call-сайту — welcome, но я не хочу гадать против ловушек §4
  карты (D239-93/2 precedent) без полного гейта.
