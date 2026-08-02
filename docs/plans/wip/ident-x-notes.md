# [M-202-ident-x-module-alias-collision] — рабочие заметки

Worktree: `d:/Sources/nv-lang/nova-identx`, ветка `p-fix-ident-x`. Модель sonnet.

## Статус: КОРЕНЬ НАЙДЕН И ПОДТВЕРЖДЁН, ФИКС ПРИМЕНЁН, регресс-тест зелёный (2026-07-20)

Итог (кратко, полный отчёт — в финальном сообщении задачи):
- Гипотеза маркера (auto_derive.rs) НЕ подтвердилась — auto_derive.rs везде
  использует префиксованные синтетические имена (`__nv_*`), голого `x` нет.
- Настоящий источник: `std/src/collections/vec_iter/core.nv` (строки
  647-674, `@min`/`@max`) и зеркально `std/src/collections/vec_lazy/core.nv`
  (613-641) — рукописный std-код, `Some(x) => { if x.compare(best) < 0 {
  best = x } }` внутри GENERIC-тела (`T Compare`). `match_arm_bindings`
  (types/mod.rs ~10630) не резолвит `scrut_ty` для generic `Option[T]`
  scrutinee `@next()` внутри ЭТОГО generic-тела → `x` не попадает в `scope`
  → `f1_check_call`'s `ExprKind::Member` dispatch (~11510-11574, НЕ
  10553-10617 — нумерация из маркера устарела) падает через
  `scope.contains_key(prefix)` guard на `imported_modules.contains(prefix)`
  (глобальный на весь CU) → ложный E7401.
- Эмпирически подтверждено ДО фикса (нужный `NOVA_STD_PATH`/`NOVA_GC_*`/
  `NOVA_CG_INCLUDE`/`NOVA_RT_DIR` env override для scratch-пакета вне
  реального repo-root — см. ниже): `nova build` на пакете
  `import a.neg.x.{who}` дал РОВНО
  `[E7401] no function 'compare' in module 'x'` на
  `std/src/collections/vec_iter/core.nv:654` и `:669`.
- Фикс: переименовал `x` → `cand` в обоих файлах (`vec_iter/core.nv`,
  `vec_lazy/core.nv`), обе фн `@min`/`@max`, + doc-комментарий со ссылкой на
  маркер. **auto_derive.rs и types/mod.rs НЕ тронуты.**
- Полезная находка про инструментарий: `nova build`/`test` резолвит
  toolchain/std/rt/libuv ЧЕРЕЗ CWD (`find_repo_root()`), а РЕЗОЛВ ИМПОРТОВ
  конкретного пакета — через путь ФАЙЛА (`find_package_dir`, walk up от
  файла). Значит для repro-пакета ВНЕ репы: `.current_dir(<реальный repo
  root>)` + путь к файлу пакета ВНЕ репы — даёт рабочий std/toolchain БЕЗ
  env-override костылей (кроме `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` — этот
  worktree конкретно без vcpkg_installed, exFAT-ограничение, см. memory
  project-worktree-nova-test-setup).
- Регресс-фикстура: Rust-интеграционный тест (прецедент lint_deny.rs /
  plan204_local_toml_and_replace_gate.rs) —
  `nova-cli/tests/ident_x_module_alias_collision.rs`, 2 теста:
  `nova_build_does_not_false_positive_e7401_on_module_named_x` (репро,
  билдит+запускает бинарь, проверяет "x-module" в выводе) +
  `nova_build_control_non_x_module_name_always_works` (контроль на
  `neg.helper`, всегда обязан проходить). Оба ЗЕЛЁНЫЕ
  (`cargo test --release --test ident_x_module_alias_collision`).

## Гейт — статус
- [x] Регресс-тест (nova-cli/tests/ident_x_module_alias_collision.rs) — 2/2
      PASS (`cargo test --release --test ident_x_module_alias_collision`).
      Закоммичено (cb7b6119b).
- [x] standalone-CU derive-фикстуры (d230_clone_deep_autoderive,
      d422_generic_container_derive, neg/n1_no_impl_no_autoderive_neg) —
      `PASS: 3 FAIL: 0`.
- [x] флагман (examples/flagship/aggregator) — `nova build` ЗЕЛЁНЫЙ,
      `built: aggregator.exe (23.57s)`, только pre-existing warnings
      (W_DEP_PATH_NO_RELEASE / W_PARAM_TYPE_POS_MUT / unused-import — не
      мои файлы). NB: без явного `--strict-effects` в этом прогоне (флаг
      есть у `nova test`, не у `nova build` для standalone примера) — сам
      флагман собирается std/examples гейтом отдельно; здесь цель —
      подтвердить, что мой std-фикс не сломал flagship, что подтверждено.
- [x] std/checksums + std/collections зелёные — `PASS: 16 FAIL: 0 SKIP: 9`
      (SKIP = compile-only модули без test/main, ожидаемо). Включает САМИ
      исправленные `std/src/collections/vec_iter/core` и `vec_lazy/core` —
      оба PASS.
- [x] маркер закрыт в docs/plans/backlog-followups.md (✅ РЕШЕНО).
- [x] финальный коммит + отчёт (этот файл больше не обновляется).

## ИТОГ: маркер закрыт, все гейты зелёные.

## Ниже — исходные заметки разведки (для истории, актуальность см. выше)

## Что подтверждено чтением кода (ДО реального repro-запуска)

- `types/mod.rs::f1_check_call` (сейчас ~11267, было ~10553-10617 в маркере —
  нумерация сдвинулась) — `ExprKind::Member { obj, name }` арм ~11510-11574:
  1. `check_instance_overload(...)` — резолвит instance-overload'ы (не источник бага).
  2. `let ExprKind::Ident(prefix) = &obj.kind else { return }` — obj должен быть
     голым идентификатором.
  3. `if scope.contains_key(prefix) { return }` — ЛОКАЛ перекрывает модуль → это
     instance-метод, не module-call. **Это единственная защита от коллизии.**
  4. `if !self.imported_modules.contains(prefix) { return }` — не импортированный
     модуль → instance-метод.
  5. Иначе ищет `self.sig.fn_decls.get(name)` как СВОБОДНУЮ функцию модуля
     `prefix` → если нет — `[E7401] no function '{name}' in module '{prefix}'`.
  - `self.imported_modules` — судя по всему ГЛОБАЛЬНЫЙ набор на весь compile
    unit (не per-file), т.е. импорт модуля `x` ГДЕ УГОДНО в CU регистрирует
    `"x"` в этом сете для ВСЕХ файлов CU.

- **Гипотеза ПЕРЕСМОТРЕНА относительно маркера**: маркер подозревал
  auto_derive.rs синтезированные однобуквенные плейсхолдеры. Прочитал
  `protocols/auto_derive.rs` `synth_compare_record_body` (~918) и
  `synth_compare_sum_body`/`variant_bind_pattern` (~1432, ~1235) — ВСЕ
  синтезированные биндинги там ИМЕЮТ ПРЕФИКС (`__nv_cmp_N`, `__nv_a_<field>`,
  `__nv_ta`/`__nv_tb` и т.п.), голого `x` НЕТ нигде в auto_derive.rs. Так что
  прямой коллизии с деривератором per se похоже не подтверждается буквальным
  чтением — либо коллизия НЕ в auto_derive.rs, либо нужно ещё поискать.

- **Новая находка — вероятный настоящий источник**: `std/src/collections/vec_lazy/core.nv`
  и `std/src/collections/vec_iter/core.nv`, функции `@min()`/`@max()`
  (generic `BoxIter[T Compare]`):
  ```
  export fn BoxIter[T Compare] mut @min() -> Option[T] {
      mut best = match @next() { Some(first) => first, None => return None }
      while true {
          match @next() {
              Some(x) => { if x.compare(best) < 0 { best = x } },
              None => return Some(best),
          }
      }
      Some(best)
  }
  ```
  (min: строки ~613-626, max: ~628-641 в обоих файлах vec_lazy/vec_iter).
  `x` — ГОЛОЙ идентификатор, пользовательского кода не требует.

- Механизм биндинга scope для match-арма: `f1_expr` `ExprKind::Match` (~9072)
  вызывает `match_arm_bindings(&arm.pattern, scrut_ty.as_ref())` (~10630) и
  ВРЕМЕННО вставляет результат в `scope` ПЕРЕД обходом тела арма (~9094-9098).
  `match_arm_bindings` для `Some(x)` кейса (~10639-10678) требует
  `scrut_ty: Option<&TypeRef>` БЫТЬ Some — если `infer_expr_type(scrutinee=@next(), scope)`
  не резолвится (возможно из-за generic `T` в определении САМОГО generic-метода
  `@min`/`@max`, т.е. чекается ТЕЛО generic-декларации, где `T` ещё абстрактный),
  `scrut_ty` = None → `match_arm_bindings` возвращает `[]` → `x` НЕ попадает в
  `scope` → при проверке `x.compare(best)` внутри арма `scope.contains_key("x")`
  = false → падает на шаг 4 (`imported_modules.contains("x")`).

  **Это ЕЩЁ ГИПОТЕЗА, не подтверждено реальным прогоном** — cargo build
  nova-cli --release в фоне (worktree, ~10 мин), после сборки нужно:
  1. Собрать repro-пакет (образец из research-заметки
     `docs/dev/research/2026-07-13-module-naming-two-segment-review.md` §2а:
     `src/a/neg/x.nv` с `module neg.x`, экспорт `who`).
  2. `nova build` на пакете, ТОЛЬКО импорт `a.neg.x.{who}`, БЕЗ явного
     использования `.min()`/`.max()`/derive — если E7401 воспроизводится
     без этого, значит std САМ ПО СЕБЕ (min/max генерик-тела) — источник, что
     подтвердит гипотезу (std всегда компилируется в `nova build`/`test`, но
     НЕ обязательно в `nova check` — маркер: баг именно в build/test, не check).

## Дальше (план)
1. Дождаться сборки `nova-cli/target/release/nova.exe` в worktree
   (`nova-identx`, фоновая cargo-задача).
2. Repro минимальный пакет, `nova build` → подтвердить E7401.
3. Если подтвердится std-min/max гипотеза — фикс НЕ в auto_derive.rs (маркер
   ошибался в конкретном месте, но общий диагноз «синтезированный/строчный
   голый идентификатор x конфликтует с imported_modules» — верен, только
   источник — std generic-тело, не auto-derive). Варианты фикса:
   (a) переименовать `x`→`__nv_x`/`item` в std vec_lazy/vec_iter min/max
       (дешёвый, локальный, НЕ трогает checker) — это гигиена в ДРУГОМ месте,
       но тот же принцип, что просил владелец («дериватор НЕ должен...» —
       здесь по аналогии: генерик-тело стандартной библиотеки НЕ должно
       использовать имя, которое обычный пользовательский код может
       легитимно импортировать как модуль).
   (b) корневой фикс в checker: `imported_modules` should be per-file, не
       глобальный на CU — дороже, вне зоны (types/mod.rs плотно занят другим
       агентом ~15896/~16486, но dispatch на ~11510-11574 вроде свободен).
   (c) чинить `match_arm_bindings`/`infer_expr_type`, чтобы generic-тело
       корректно резолвило `scrut_ty` для `Option[T]` даже с абстрактным T —
       самое дорогое и рискованное (может задеть другие пути).
   Предпочтение по инструкции — минимальная гигиена, не трогать types/mod.rs
   вообще если можно. Если источник — std, а не auto_derive.rs, зона задания
   расширяется НЕФОРМАЛЬНО (auto_derive.rs "свободен", types/mod.rs — под
   защитой; std/** не упомянут в разделе ЗОНА явно, но задание описывает
   auto_derive.rs как вероятный дом фикса). Нужно перепроверить README/маркер
   ещё раз и решить по факту репро — если репро НЕ требует std-min/max (т.е.
   воспроизводится с ГОЛЫМ импортом без вызова .min()/.max()), тогда это
   железно про std generic-тело, всегда компилируемое, и фикс там.
4. Если гипотеза НЕ подтвердится — искать дальше (grep голых
   однобуквенных `x`/`y` идентификаторов, используемых как receiver
   `.method()`, по всему std + auto_derive.rs).
