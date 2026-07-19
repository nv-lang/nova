# План 214 Ф.1-Ф.3b — рабочие заметки (sonnet, worktree nova-214, ветка p214-coerce)

Базовая ревизия: 6411de6f3 (main). Карта — `docs/plans/214-coerce-attribute.md`
(ревью-7, финал), спека — D429 `spec/decisions/02-types.md:15547`.

## Статус: разведка инфраструктуры (in progress), реализация ещё не начата

### Найденные choke-points (сверено с d55-coercion-notes.md, ветка p-fix-d55-type-directed, уже влита в main)

- **Accept-side (checker):** `compiler-codegen/src/types/mod.rs::assignable_direct`
  (~13939) — единый choke-point для call-arg/let-const/element (element — через
  `ArrayLit`-арм внутри `assignable_direct`, рекурсирует `assignable` per-элемент,
  ~13968). Литерал-армы (StrLit и т.д.) — с 14093. `is_bytes_slice_rt` helper ~17251,
  `array_elem_type` ~17231. `assignable` (~13881) — обёртка над `assignable_direct`,
  добавляет single-wrapper fallback через `single_wrap_candidates` (~17295) +
  `wrap_kind_of_expr`/`wrap_kind_of`. `assignable_direct` НЕ покрывает return —
  задокументированный пробел (return не идёт через `assignable` вообще, комментарий
  ~6843 "No `assignable` runs here").
- **Rewrite-side (AST):** `MapLitAnnotator` (~33002), `walk_expr` (~33281) — уже
  прокидывает `expected: Option<&TypeRef>` через call-arg/let/const/return(частично)/
  array-element/record-field/if-else/match-arm позиции (общий D55-проход). Вызывает
  `try_wrap_leaf` (~33224) для sum-variant/newtype leaf-обёртки. **Кандидат для моей
  вставки:** добавить `try_coerce_leaf` рядом, вызываемую из `walk_expr` СРАЗУ ПОСЛЕ
  `try_wrap_leaf` (single-wrapper первичен — R11 — должен матчить/блокировать раньше,
  но R11 на самом деле проверяется на этапе ДЕКЛАРАЦИИ #coerce, не на call-сайте: если
  пара уже покрыта single-wrapper, #coerce на неё не зарегистрируется вовсе → на
  call-сайте конфликта в рантайме проверки нет, окна просто НЕ пересекаются по паре).
  Note: `try_wrap_leaf` работает только на ЛИСТЬЯХ (IntLit/FloatLit/BoolLit/StrLit/
  Ident-с-известным-типом) — `#coerce` должен работать на ЛЮБОМ expr с типом I, не
  только на листьях (значение `sb` — Ident произвольного типа, `foo().bar` — Member
  expr и т.п.) — нужен `infer_expr_type`, не только `var_types`-lookup.
- **Return-position codegen:** `emit_c.rs::emit_expr_with_target_type` (~28408) — уже
  общий choke-point для let/return/assign/array-elem C-target-typed emission (per
  d55-notes). Для #coerce НЕ нужен новый codegen — rewrite вставляет обычный
  `Call`-node (`s.bytes()`), который эмитится СУЩЕСТВУЮЩИМ путём метод-вызова — значит
  рецепт "codegen ничего нового" (§Ф.2 плана) означает: rewrite ДОЛЖЕН случиться ДО
  emit_c (т.е. в types/mod.rs AST-mutating проходе, тот же, что MapLitAnnotator), не
  внутри emit_c.

### Name-gated костыль (Ф.2 снос)

`synthesize_write_str_lit_bytes_coercion` (переименован в d55-волне в
`synthesize_bytes_lit_call_args`) — pre-pass в `emit_call`, `emit_c.rs`. Читает
`method_overloads`-реестр (recv_type_name, method_name)->Vec<MethodSig>. Снести
ПОСЛЕ того, как #coerce-rewrite покрывает call-arg-позицию (то есть #coerce
call-arg вставка обязана произойти РАНЬШЕ этого pre-pass либо вместо него — рекомендованный
путь: #coerce rewrite в types/mod.rs (pre-codegen AST pass) переписывает
`w.write("literal")` в `w.write("literal".bytes())` ДО того, как emit_c вообще видит
call — тогда `synthesize_bytes_lit_call_args` становится мёртвым кодом, сносим.

### #coerce атрибут — парсинг

TODO: проверить, как парсер обрабатывает произвольные `#foo`-атрибуты на fn (есть ли
общий механизм `Item::Fn.attrs: Vec<Attr>` или каждый атрибут захардкожен отдельным
полем). Следующий шаг разведки.

## План действий (обновляется по ходу)

1. Атрибут `#coerce` — парсер + AST-поле на FnDecl.
2. Реестр CoercePair (I,O)->FnDecl, sig-scan (аналог существующих sig-scan проходов).
3. Валидации (см. план) — где строится реестр (вероятно отдельная фаза в types/mod.rs
   до основного f1-чекера, т.к. нужен ДО assignable/walk_expr).
4. Accept-side: assignable_direct/assignable — общий expr (не только литерал) с типом I
   в позиции O, через реестр.
5. Rewrite-side: try_coerce_leaf в MapLitAnnotator (или отдельный проход).
6. Ф.2 снос костыля.
7. Ф.3 std pометки + миграция сайтов.
8. Ф.3b линт.
9. Гейты.

Чекпоинт-коммиты — после каждого пункта, в этот файл дописываю вердикт перед
коммитом (per задание).
