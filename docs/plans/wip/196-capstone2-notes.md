<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — КАПСТОУН (`infer_call_ret_c`): БАТЧ-2 чекпойнт (sonnet, worktree `nova-196cap2`, ветка `p196-capstone2`)

**Родитель:** [196-campaign-map.md](196-campaign-map.md) §«Зона FROZEN». **Предшественник:** [196-capstone-notes.md](../196-capstone-notes.md)
(батч-1, реестр 50→48). **База старта:** main `cb409a927`. **Синк в процессе сессии:** main `726e734af`
(p196-rtbuf-producers, 4 новых Q1/Q6-продюсера в `types/mod.rs`, слит ПОСЛЕ старта этой сессии).

---

## Итог одной строкой

Синк с `726e734af` подтвердил все 6 предсказанных изменений трафика (B10f/B10m/B10h/B10l закрыты-или-похудели,
B01/B11a/B11d живы с урезанным трафиком). Детач+panic батч на 4 кандидата (B10f/B10h/B10l/B10m):
**B10h_newtype_constructor и B10l_named_tuple_constructor СНЯТЫ** (0 паник на std/src/math + collections/time/
encoding + standalone + aggregator); **B10f_user_fn_sigs паника СРАБОТАЛА** на `examples/flagship/aggregator`
(`splitmix64_step`, ret_ty=`uint64_t` — тот самый 206/splitmix64 прецедент из CLAUDE.md) — откат, ветка живая;
**B10m_ident_empty_fallback НЕ тронута** — собственный doc-комментарий документирует легитимную причину
(phase-1c pre-scan caller, registries ещё не заполнены by construction), 0-хит на моём корпусе не той же силы
доказательство, что для остальных armов (мой корпус не специфично стрессует phase-1c). **Реестр: 48 → 46.**

**Побочная находка (СРОЧНО сообщена координатору, НЕ моя зона — types/mod.rs):** синк вскрыл РЕГРЕССИЮ,
введённую именно `726e734af` — `spec_tests/conformance/d45_inferred_return_type.nv` (сам regression-тест на
inferred-return из expression-body) теперь **CODEGEN-FAIL** изолированно. Корень: коммит `ba9a8a2f3` (Producer
Q1 "bare free-fn declared return"), хунк `None => Some(ResolvedType::Unit)` конфлирует «нет явного `->`» с
«возвращает Unit» для expression-body функций (`fn f(x) => x > 0`), чья РЕАЛЬНАЯ (инферированная) сигнатура —
`bool`, не `Unit`. `assert(d45_is_positive(1))` получает `Unit` вместо `bool` → `E_NO_MATCHING_OVERLOAD`. Не
правил (не моя зона), сообщил координатору немедленно (SendMessage). **Это блокирует авторитетный гейт
(`spec_tests/conformance` — весь top-level `spec_tests/conformance/*.nv` компилируется ОДНИМ mega-CU, ЛЮБОЙ
тест из этой директории сейчас контаминирован d45's ошибкой) — красный = стоп до fast-follow фикса.**

---

## 1. Синк + пересборка

`git merge main` (fast-forward `cb409a927` → `726e734af`, 7 файлов, 953+/37- — только `types/mod.rs`
(165 строк, 4 продюсера) + доки). Debug+release бинари пересобраны (`cargo build [--release]
--manifest-path nova-cli/Cargo.toml`, `CARGO_TARGET_DIR=C:/nova-build-cache/196cap2`).

## 2. Реконфирмация 6 предсказанных изменений (fresh census, post-merge)

| Ветка | Corpus | Пре-мерж | Пост-мерж | Вердикт |
|---|---|---|---|---|
| B10f_user_fn_sigs | std/src/time | HIT (2) | 0-HIT | закрыт на time/collections/encoding **но НЕ на aggregator** (см. §3) |
| B10m_ident_empty_fallback | std/src/time | HIT (side-effect) | 0-HIT | закрыт на time (но см. §3 — оставлен) |
| B10h_newtype_constructor | std/src/math, standalone | HIT | 0-HIT | ЗАКРЫТ (снят) |
| B10l_named_tuple_constructor | std/src/math | HIT | 0-HIT | ЗАКРЫТ (снят) |
| B01_turbofish_member_generic_type | std/src/collections | HIT | HIT (не изменился) | жив, урезанный трафик (не измерял Δ) |
| B11a_array_static_method | std/src/collections | HIT | HIT (не изменился) | жив, урезанный трафик |
| B11d_typed_pointer_methods | std/src/collections | HIT | HIT (не изменился) | жив, урезанный трафик |

Корпус: `std/src/collections --skip lru_test` (12/0/6skip), `std/src/time` (6/0/1skip, 1 RUN-FAIL таймингова
флака `units_test` — PASS в изоляции, не регрессия), `std/src/math` (3/0/2skip), `std/src/encoding` (8/0/7skip),
`standalone` (30 файлов, 30/0), `examples/flagship/aggregator --strict-effects` (release, чисто). Все PASS δ0
относительно пре-мерж переписи (кроме флаки).

## 3. Детач+panic батч (B10f/B10h/B10l/B10m) — протокол, факты

1. **Detach:** все 4 арма получили `panic!("[196-capstone2 DETACH] <id> недостижимой не оказалась — репро: …")`
   вместо своих `return`/tail-expr. Пересборка debug+release.
2. **Прогон:** collections/time/math/encoding/standalone — **0 паник**. `aggregator --strict-effects`
   (release) — **PANIC** на B10f: `name="splitmix64_step" ret_ty="uint64_t"`.
3. **Разбор находки:** `splitmix64_step` — bare free-fn call, ЖИВОЙ трафик в флагман-примере, которого
   p196-rtbuf-producers' Producer (bare-free-fn declared-return) НЕ покрывает (вероятно gs-гейт/single-candidate
   дисциплина отклоняет форму — не расследовал глубже, не моя зона чинить продюсер). Это ТОЧНО прецедент
   test-conventions.md 206/splitmix64: conformance не ловит app-регрессии, только флагман-сборка ловит.
4. **Откат B10f:** восстановлен оригинальный `return ret_ty.clone()`, убран panic, добавлен doc-комментарий
   документирующий находку (репро + вывод «ЖИВАЯ, не трогать»). Пересборка + повторный `aggregator` прогон —
   **чисто, 0 detach-паник** (проверял с B10h/B10l/B10m ЕЩЁ детач-панящими — ни одна не сработала на aggregator
   после отката B10f, подтверждая, что panic на B10f был ЕДИНСТВЕННОЙ причиной прежнего краха).
5. **B10m — решение НЕ удалять:** в отличие от B10h/B10l (чистые "if condition → return", безопасно
   конвертируемые в permanent REMOVED), B10m — БЕЗУСЛОВНЫЙ tail-fallback всего `Ident`-каскада, и его СОБСТВЕННЫЙ
   doc-комментарий («In phase-1c pre-scan the function registry is not yet populated — return empty so callers
   degrade to nova_unit») документирует ЛЕГИТИМНУЮ (не malformed-input) причину существования: ранняя фаза
   компиляции, где registries (`user_fn_sigs`/`type_aliases`/`generic_fns`/…), от которых зависит ВЕСЬ этот
   каскад, ещё НЕ заполнены by construction — не баг, архитектурная особенность фазы. Мой корпус гоняет обычный
   full-pipeline порядок (registries уже заполнены к моменту инференции) — 0-хит там НЕ то же доказательство
   мёртвости, что для B10h/B10l (чьи гейты не зависят от фазы). Оставлена живой (осторожность, не отговорка) —
   рекомендация следующей волне: точечный repro, вызывающий этот путь ИМЕННО из phase-1c pre-scan, прежде чем
   пытаться снова.
6. **Финал:** B10h/B10l получили постоянные REMOVED-комментарии (стиль остальных снятых веток файла, со ссылкой
   на `726e734af`/`90328e908` + доказательства). B10f/B10m восстановлены к оригинальному поведению +
   документирующий комментарий (без panic).

## 4. Финальная верификация (после отката/финализации)

Debug+release пересобраны с финальным состоянием (B10h/B10l удалены, B10f/B10m — оригинал + doc).
`collections+time+math+encoding` (28/0/16skip, 1 флака units_test PASS-в-изоляции), `standalone` (30/0),
`aggregator --strict-effects` (release, чисто, 16.86s). Конфликт-маркеров нет (`grep` перед коммитом).

**Коммит:** `bd797d770` — `fix(codegen): [196-capstone2] B10h/B10l сняты (p196-rtbuf-producers), B10f/B10m
реконфирмированы живыми`. **Реестр: 48 → 46.**

## 5. Терминалы (4) — не тронуты

`B11al_panic_method_p67`, `B12q_panic_path_p67`, `B12r_panic_path_no_method_seg`, `B12s_panic_path_no_parts`.
Прочитал точную механику B11al (полный тайл-панический fallback ВСЕЙ `Member`-каскады, emit_c.rs ~52357 текущей
нумерации): закрытие per-branch НЕВОЗМОЖНО без checker-фикса (str.until / deserialize Path-return — оба в
`types/mod.rs`, вне моей зоны и вне мандата этой сессии — карта §1г явно требует «чинить в ЧЕКЕРЕ»). Терминалы
остаются reachable-by-design (панические заглушки для malformed/edge input) до финального сноса функции —
никакого действия не предпринято, не отговорка, а следствие явного запрета на правку `types/mod.rs`.

## 6. Живая-5 (реконфирмация хитов)

- `B10a_ident_println_assert` — HIT (стандартный корпус). Блокер (`ExprKind::With` в `f1_expr` игнорирует
  `bindings[i].handler`, types/mod.rs:9187) НЕ закрыт ни одним из мержей этой сессии — проверил код напрямую
  (`ExprKind::With { body, .. } => { self.f1_block(body, ...) }` — `bindings` по-прежнему не деструктурируется).
- `B11ac_novavtable_effect` — HIT (мой standalone/d-fixture сэмпл, неожиданно шире заявленного "examples/effects"
  корпуса из батча-1).
- `B11ak_self_recursive_generic_method` — HIT (collections/encoding).
- `B03_protocol_default_body_synth`, `B11i_canceltoken_instance` — 0-HIT на моём narrow-сэмпле (включая
  попытку std/src/os+std/src/fs — только 2 test-файла реально запустились там, `d324_os_env_args_cwd_test` +
  `concurrent_stat_test`, оба НЕ дали хит) — НЕ переоткрываю мёртвыми, батч-1/прошлые волны уже подтвердили живыми
  на ПОЛНОМ корпусе (std/os,fs целиком / nova_tests/concurrency), которого я не гонял полностью.

## 7. SHARED (не трогал, вне мандата)

`B11q_novaopt_methods`/`B11r_result_like_methods` (блокер Plan 59 Ф.7.5 D3, typed Result mono — ОБЕ HIT в моём
сэмпле, живой трафик подтверждён), `B12h_path_try_from` (реклассифицирован в тот же класс, HIT), `B10c_unanno_
light_closure` (собственный doc говорит ЖИВАЯ уникальная value-aware ре-деривация, НЕ структурно-блокированная —
подтвердил чтением, HIT), `B11u_voidstar_giveup` (0-HIT ожидаемо, структурно НЕ снимаема — fallthrough safety
net для B11al).

## 8. B07/B07r — композиция НЕ атакована (см. wip/196-rtbuf-notes.md §5)

rtbuf-producers сессия оставила детальную карту для 24 `ch2=true` stub-skip сайтов (0.8% B07's трафика,
найдены ТОЛЬКО на полном conformance mega-CU — вне scope и той сессии, и этой). Композиция правильно
идентифицирована как «продолжение B07's уже вычисленного subst/ret_ty, НЕ отдельная реализация» — риск
side-effect регрессии (`register_generic_instances_in_typeref`, исторический сегфолт при пропуске) слишком высок
без propose-then-verify параллельно с легаси (граница «читать можно, править нельзя» становится зыбкой). НЕ
атаковал в эту сессию: (а) полный conformance заблокирован d45-регрессией (§ниже) прямо сейчас, (б) это
объёмная отдельная работа (~100 строк логики), не батч-3-5 кандидат.

## 9. КРИТИЧЕСКАЯ находка — регрессия d45 от 726e734af (НЕ моя зона, сообщено координатору)

См. итог одной строкой. Детали:
- Репро: `nova test spec_tests/conformance/d45_inferred_return_type.nv` (ИЗОЛИРОВАННО, один файл) →
  `CODEGEN-FAIL`, 3× `E_NO_MATCHING_OVERLOAD` на `assert(d45_is_positive(1))` / `assert(!d45_is_positive(0))` /
  `assert(!d45_is_positive(-5))`.
- Корень (types/mod.rs, коммит `ba9a8a2f3`, НЕ правил):
  ```rust
  let rt = match &callee.return_type {
      Some(ret_tr) if !typeref_mentions_any(ret_tr, gs) => Some(ResolvedType::from_type_ref(ret_tr)),
      Some(_) => None,
      None => Some(ResolvedType::Unit),   // БАГ: expression-body без `->` ≠ Unit
  };
  ```
  `fn d45_is_positive(x int) => x > 0` — expression-body (нет `->`), РЕАЛЬНЫЙ тип инферится из тела (`bool`), но
  AST даёт `callee.return_type = None` (нет явной аннотации) → продюсер конфлирует «нет `->`» с «возвращает
  Unit», канализирует ЛОЖНЫЙ `ResolvedType::Unit`.
- Blast radius (грепом `fn NAME(...) => EXPR` без `->` по conformance+std+examples): 4 функции, ВСЕ в ОДНОМ
  файле (`d45_double`/`d45_negate`/`d45_greet`/`d45_is_positive`). Только `d45_is_positive` (bool) даёт ЛОУДНЫЙ
  compile-error (assert требует точный bool); `d45_double`/`d45_negate`/`d45_greet` (int/str) НЕ дают ошибку —
  вероятно ТИХО получают неверный Unit в `==`-сравнениях (не проверял глубже, вне зоны).
- Влияние: `spec_tests/conformance/*.nv` (top-level, ВСЕ файлы) компилируются ОДНИМ mega-CU (эмпирически
  подтверждено: `nova test` на ЛЮБОМ соседнем файле, например `d182_self_return_parametric_static.nv`,
  контаминирован d45's ошибкой с тех пор, как я синканулся). Это блокирует ВСЕ мои D-фикстур-переверификации
  (д119/д122/д16/д355/д402/д43/д109/д315/д354/д143/д239/д30/д85/д52/д372 — top-level conformance, НЕ standalone/)
  до фикса. `spec_tests/conformance/standalone/*` — ОТДЕЛЬНЫЙ CU, НЕ контаминирован (проверено — 30/0 PASS).
- Сообщил координатору немедленно через SendMessage (не стал ждать финального отчёта) — красный авторитетный
  гейт требует немедленного внимания per конвенцию, но фикс в types/mod.rs вне моего мандата.

## 10. Коммиты сессии (ветка `p196-capstone2`, worktree `nova-196cap2`)

1. `bd797d770` — `fix(codegen): [196-capstone2] B10h/B10l сняты (p196-rtbuf-producers), B10f/B10m
   реконфирмированы живыми`.
2. (этот коммит) — `docs(196): capstone2 чекпойнт — B10h/B10l сняты, d45-регрессия флагирована`.

**В main НЕ мёржено.** Push запрещён по заданию.

---

## 11. Рекомендация следующей волне

1. **d45-регрессия (§9) — ПРИОРИТЕТ.** Fast-follow фикс в `types/mod.rs` (Producer Q1 bare-free-fn, коммит
   `ba9a8a2f3`): различить «expression-body без явного `->`» (реальный тип инферится из тела) от «объявлено
   `-> ()` явно» (Unit по праву) — например, консультировать тело выражения через существующий infer-путь вместо
   безусловного `None => Unit`, ИЛИ просто НЕ канализировать (оставить легаси) для функций без явного `->`,
   если инференция тела недоступна в этой точке чекера.
2. **B10m** — точечный phase-1c pre-scan репро прежде, чем пытаться снова (см. §3.5).
3. **Терминалы** — требуют checker-фикса (str.until/deserialize Path-return), вне мандата frozen-агента; нужен
   Zone CH/producers-агент.
4. **B07/B07r** — карта готова (wip/196-rtbuf-notes.md §5), композиция ждёт localization на полном
   conformance (заблокирован d45-регрессией прямо сейчас) + propose-then-verify дисциплину.
5. **Остаток реестра (46):** 4 терминала + 5 живых-широких + 5 SHARED (B11q/B11r/B12h/B10c/B11u_giveup) + 32
   ядра (Core-32, ждут Zone CH channel-расширения) минус то, что producer-сессии уже успели закрыть частично
   (B01/B11a/B11d — reduced traffic, ещё живы). Точной свежей арифметики "32 core" после сегодняшних продюсеров
   не пересчитывал (не задание — они остаются reachable, не 0-хит).
