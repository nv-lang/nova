# Багфикс-волна 187 — чекпоинт прогресса

Worktree: `d:/Sources/nv-lang/nova-p187` (ветка `bugfix-187-wave`).

## База (Шаг 0)

Команда: `nova test --positive --compile-error --timeout 300 spec_tests/conformance` (release-бинарь волны, без `--jobs`).

**Результат базы: PASS 472  FAIL 0  SKIP 14.** Самый долгий тест — `app_effect_basic_t8_1` (~1255s), нормально (известный тяжёлый тест).

## Таблица багов

| Баг | Приоритет | Статус | Коммит |
|---|---|---|---|
| БАГ 0 serde-encode-pointer-op-regression | P0 | ЗАКРЫТ (фикс + красно-зелёная фикстура, standalone-CU) | этим коммитом |
| БАГ 1 http-serde-setcookie-serialize-collision | P1 | не воспроизведён локально (нет пакета nova-http; 5 минимальных форм — зелёные) | — |
| БАГ 2 errorkind-parsejsonerror-variant-collision | P2 | фикс + фикстура зелёные (таргетно), коммит следующий | — |
| БАГ 3 nested-spawn-scope-var-cc-fail | P2 | codegen-половина закрыта (фикс + фикстура); >16 вложенно-порождённых детей на скоуп — за [M-173.0-R2] (рантайм-подложка, вне волны) | — |
| БАГ 4 monotonic-now-bare-binding-ice | P2 | НЕ воспроизвёлся (закрыт попутно 67717dcb1/747a79c65); регресс-фикстура добавлена | — |
| БАГ 5 spawn-throw-segfault | P2 (re-verify) | НЕ воспроизвёлся на свежем бинаре; регресс-фикстура добавлена | — |

## Диагноз БАГ 0 (одной строкой)

`register_mono_method_instance` не публиковал type-qualified `fn_ret_<конкретный-ресивер>_<method>` для mono-инстанциаций receiver-own-generic blanket-методов (`fn[T] T @to_str() -> str`) → `infer_call_ret_c` падал в receiver-блайнд name-only `fn_ret_to_str` (last-wins), который в serde-CU перезаписывал `[]u8 @to_str() -> Result[...]` → примитивный `.to_str()` мистайпился в Result-форму (= указательную) → детектор `p + i` Plan 70 на `+`-конкатенации. Щель латентная с Plan 152.4.3, ВСКРЫТА слиянием 174.2 (58d4358c1/41f3550b5: `str.from(scalar)` удалён → примитивы перешли на blanket-`to_str()`).

## Попутные гейт-находки (вне 6 багов, задокументированы)

- Bare-name коллизии типов в чекер-реестре (`self.types` без модуль-квалификации): `import std.encoding.serde` в общий conformance-CU втягивает test-peer файлы serde (их локальные `User`/`Shape`/`DeError`) и молча подменяет одноимённые типы других фикстур (E7320 на std-методах). Обойдено размещением serde-фикстуры в standalone (свой CU); сами существующие фикстуры НЕ тронуты. Отдельная будущая работа.
- [M-187-d182-turbofish-new-nameonly-collision]: ложный D102 на `Type[A,B].new(позиционно)` при serde в CU — фикс отдельным коммитом (types/mod.rs).

## Финал

(заполняется в конце: финальный conformance δ0, nova check std, cargo build)
