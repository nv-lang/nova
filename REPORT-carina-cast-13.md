# Отчёт облачной сессии, шаг 13: синк origin/main dc2617993, emit_flow по `heads`, клетка match №1682, проба guard-фикстур (ветка `p274-carina-cast`)

Вершина до шага — `e956d370`. Linux. Слияние — `79af8747` (родители e956d370, dc261799), запушено.
Строк журнала времени нет.

```
1. СИНК — СДЕЛАН (79af8747). Сторона main взята везде, где её менял main; шаги 9-12, №1682/№1692/№1697 сохранены.
   - emit_c/emit_flow.nv: модель main `heads` целиком (`@print_tested_arm(f, a.body, a.heads, ...)`,
     `@print_chain`); мой `is_cond_of` снят — цепочка к голове guard-блока печатает вынесенное целиком.
     Вынос операнда guard'а в lower/lower_match.nv сохранён (комментарий сведён с main, mono.nv — так же).
   - check/check.nv, binds.nv, typed_bind.nv: расщепление main (`@declared_local_type`, `@type_untyped_bind`);
     моя дверь записанного типа оставлена только для ПРОСТОГО выражения (`@plain_written_init`:
     не match/конструктор/массив/handler — их судит своя дверь по ожиданию). Дверь записывает тип
     инициализатора, только если его не записала его собственная дверь (один писатель на узел).
   - calls.nv / methods.nv / option_rules.nv / literal_rules.nv: обе стороны — форма отложенных аргументов
     main плюс мой `@is_deferred_arg` (литеральные хвосты if/match); оба импорта в literal_rules.nv
     (`literal_digits` и `subst_type`). `LitArg` сравнивается match-помощниками `is_lit_family` /
     `same_lit_family` (D363: `==` на сумме требует Equal), а не `==`. `import_parts` переехал в run.nv —
     calls.nv под лимитом 1000 строк (958).
   - emit_expr.nv — версия main (`@emit_bound_args` в emit_args.nv); в emit_args.nv — правило D488 ветки
     для программы и оболочки одним условием.
   - shell.tpl.c — из main, ПЕРЕГЕНЕРИРОВАН на сведённом оракуле: отличие от main в одной строке (номер
     строки parse.nv в трассе throw). Поэтому база эмиссии +3 против main: база main мерила устаревшую
     оболочку (15269 против 15272) — поднята с летописью.
   - fixtures/typed_local/neg_1.nv (main): пин переведён на текст оракула E7301 — №1609 называет языковую
     ошибку кодом оракула; тест не ослаблен, переведён на новую норму с объяснением в файле.
   - Базы — счёт стража на сведённом дереве, с летописью: surface sem 348 (346 main + две функции №1654),
     subset-debt refusals 137 (138 main минус GUARD_BLOCK_FORM_MSG шага 9).
   - Реестр: мои 8 строк (1644, 1645, 1646, 1653, 1654, 1664, 1692, 1697) получили оговорку о носителе;
     check-registry-entry-shape 343 <= 343. Строке 1654 — «БЛОКИРУЕТ ТЕГ: НЕТ».
/
2. №1682 — КЛЕТКА MATCH ДОБАВЛЕНА: в pipeline/numeric_test.nv две клетки match-значения в поле записи
   (`Pr { b: match k { 3 => n  _ => n }, f: 2.0 }`, целое и f64). Проба на сведённом дереве: все 4 формы
   отказаны (включая match в поле); проба в обе стороны для двери — в 2c43cec6.
/
3. ПРОБА GUARD-ФИКСТУР (сведённое дерево): guard_block_form/pos_1, coalesce_nested/pos_1, try_option/pos_1 —
   ok (смоук: собрано и вывод совпал); try_option/neg_3 — пин держится. fixture-expect 119/119.
/
4. `println(take(if c > 0 { n } else { n }))` на сведённом дереве — ICE НЕТ, вывод совпадает с оракулом
   (k1 №1673 / sweep №1683 пришли с main). Номер не заводил.
/
5. №1685: правка литеральных хвостов (№1697) закрывает 18 из 24 клеток (рукав match и ветка if ×
   привязка/возврат/аргумент × u8/i16/f32); остаются 6 — хвост БЛОКА в привязке и в аргументе — помощнику.
/
ПРИЁМКА (сведённое дерево): мера 0.2 = 16 сообщений (как main), ICE 0; модульные тесты novac
  (pipeline, check, lower, lex, sem) — PASS; fixture-expect 119/119; смоуки value_recv/pos_1-2,
  numeric_positions/pos_1-2, typed_local/pos_1; крейт compiler-codegen --lib 1295/0; фильтры --full
  p1645 4/0, p1646 2/0, p1653 2/0, p1664 1/0, p1692 3/0, p1517 4/0, p1608 16/0, n_match_arm 3/0,
  sentinel 2/0; флагманы 6/6 с --strict-effects; std/examples/novac — новых NARROW/7301/LIT/ARM нет.
  Стражи novac (novac-gate-guards.sh): красные только стражи СООБЩЕНИЯ коммита при прогоне без коммита
  (commit-donor, commit-no-simplification — в merge-сообщении трейлеры есть, хук commit-msg пропустил);
  branch-complete, найденный прогоном, исправлен до коммита (0 == база).
/
КОММИТЫ:
  79af8747 Merge origin/main dc2617993 into p274-carina-cast
  (этот отчёт) docs: thirteenth report of the cloud session on p274-carina-cast
/
ВОПРОСЫ ИНТЕГРАТОРУ: нет. Очередь по письму 04:14 закрыта; жду назначения.
```
