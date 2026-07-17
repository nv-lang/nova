<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 Facet B — priv(file) free-fn resolve bleed (диагноз + STOP)

Точечный заход фасета B плана 196 (единый FnDecl-резолв). `priv(file)` free-fn
с одним именем протекает между peer-файлами folder-CU — нарушение D307 §1/§3.

**СТАТУС: диагноз завершён, фикс НЕ закрыт — оказался ШИРЕ ожидаемого (3-й сайт:
generic-mono dispatch). СТОП по протоколу. Merged-CU НЕ тронут (2 файла остались
в `standalone/`). Код-правки откатаны — clean tree, только эти заметки.**

---

## Репро (доказано на до-фиксовом бинаре)

Вернул 2 карантинных файла Класса-2 в merged conformance CU
(`git mv standalone/{method_call_never_static,scalar_only_empty}.nv` в
`conformance/`, `module standalone.X` → `module spec_tests.conformance`).

Полный `nova test --positive --compile-error spec_tests/conformance`:
```
RUN-FAIL spec_tests/conformance/app_effect_basic_t8_1
  FAIL: static-method-never in else picks int from then — assert failed: pick(true) == 42
  FAIL: scalar-only record constructs and reads       — assert failed: pick(s) == 5
SUMMARY: PASS: 465  FAIL: 1  SKIP: 12
```
Bleed = **runtime wrong-value** (НЕ compile-error). Синтетический изолированный
мини-CU (4 файла, вне корпуса) НЕ воспроизводит — нужен полный контекст.

---

## Первопричина (НАЙДЕНА по сгенерированному C — точная)

В merged-CU есть 3 одноимённых `priv(file)`-free-fn `pick`/`pf_dispatch`:
две КОНКРЕТНЫЕ (`pick(bool)`, `pick(Scalars)`) в разных файлах + одна ГЕНЕРИК
(`priv(file) fn[T SignedInt] pick(x T) -> T => x`) в третьем.

Сгенерированный C (`app_effect_basic_t8_1.c`) показывает:
- ОПРЕДЕЛЕНИЯ верны и file-дискриминированы:
  `nova_fn_..._f243_pf_dispatch(nova_bool)` → 111/222 (конкретная, файл A),
  `nova_fn_..._f244_pf_dispatch(nova_int)` → n*10+7 (конкретная, файл B).
- НО ВЫЗОВ в тесте файла A эмитит
  `nova_fn_..._f243_pf_dispatch____nova_bool(true)` — это ГЕНЕРИК-мономорфизация
  (тело `=> x`, identity), НЕ конкретная A-перегрузка. Возвращает `true`(=1) ≠ 111.
- Аналогично файл B: вызов `..._f244_pf_dispatch____nova_int(3)` → identity → 3 ≠ 37.

**Диагноз:** резолв вызова free-fn выбирает ГЕНЕРИК `pick[T]` вместо более
специфичной КОНКРЕТНОЙ file-local перегрузки, И именует mono по file_id
ВЫЗЫВАЮЩЕГО файла (f243/f244), а не по файлу генерика (f245). Т.е. дефект — в
**generic free-fn mono-dispatch пути** (codegen), НЕ в:
- (1) checker overload-фильтре `f1_check_call` (`resolved_callees`), и НЕ в
- (2) codegen `method_overloads`-регистрации (`emit_c.rs:6145`).

Оба «очевидных» сайта я пофиксил (см. ниже) — bleed НЕ ушёл → сайт третий.

---

## Что было испробовано (обе правки корректны, но НЕДОСТАТОЧНЫ — откатаны)

### Правка 1 (checker, `types/mod.rs` ~10768, ветка `ExprKind::Ident` в `f1_check_call`)
Фильтр кандидатов free-fn по видимости call-site ПЕРЕД arity/type-compat:
```rust
let caller_file_id = base.span.file_id;
let visible: Option<Vec<&FnDecl>> = self.sig.fn_decls.get(n).map(|v| {
    v.iter().filter(|c| !c.file_private || c.span.file_id == caller_file_id)
        .copied().collect()
});
match visible.as_deref() {
    Some([single]) => single,
    Some(multi) if !multi.is_empty() => { /* прежняя overload-логика */ }
    _ => return,
}
```
Корректно (priv(file) free fns не видны из чужого файла в чекере), 0 регрессий,
НО дефект в codegen-mono, не в чекере → bleed остался.

### Правка 2 (codegen, `emit_c.rs` ~6145, регистрация free-fn в `method_overloads`)
При регистрации `current_emit_file_id` не выставлен → `free_fn_c_name` давал
НЕ-дискриминированное имя (+ cross-file param-суффикс), расходясь с
ОПРЕДЕЛЕНИЕМ. Для file-private free-fn брать имя прямо из
`file_priv_fn_c_names[(f.span.file_id, f.name)]` (без суффикса):
```rust
let file_priv_c = if f.file_private && !f.is_external {
    self.file_priv_fn_c_names.get(&(f.span.file_id, f.name.clone())).cloned()
} else { None };
let c_name = if let Some(fp) = file_priv_c { fp }
    else if existing_count == 0 { base_c_name.clone() }
    else { /* param-суффикс */ };
```
Тоже корректно и 0 регрессий, но НЕ трогает generic-mono путь → bleed остался.

Полный пост-фикс гейт с ОБЕИМИ правками: `PASS: 466  FAIL: 1` — единственный
FAIL = тот же merged-CU (pick/pf_dispatch), НИ ОДНОГО нового FAIL → правки
безопасны (export-резолв НЕ задет, корпус не сломан), просто недостаточны.

---

## Настоящее место фикса (для следующего захода — фасет B, generic-mono dispatch)

Где резолвится/эмитится вызов свободной ГЕНЕРИК-функции с моно-инстанцированием
(в `emit_c.rs`: пути с `saved_emit_file_id_mono` ~21530/22812, где
`current_emit_file_id = Some(fn_decl.span.file_id)`; и место выбора
generic-vs-concrete для free-fn call). Нужно:
1. **Приоритет конкретной перегрузки над генериком** для free-fn call, когда
   конкретная file-visible перегрузка подходит по arg-типам (D84: более
   специфичная выигрывает; сейчас генерик перебивает).
2. **File-scope генерик-кандидата**: `priv(file) fn[T] pick` из файла F невидим
   на call-site другого файла (тот же фильтр, что Правка 1, но в generic-mono
   резолве).
3. **Mono-имя генерика — по файлу ГЕНЕРИКА**, не вызывающего (сейчас f243/f244
   вместо f245 — коллизия namespace с конкретными file-local одноимёнными).
Правки 1+2 выше — половина каркаса; довести до generic-mono сайта.

---

## Доп. тесты (готовы, но НЕ закоммичены — воспроизводят bleed → сломали бы merged-CU)

3 позитива D307 §5.3 (co-exist RUNTIME) — трио peer-файлов
`module spec_tests.conformance`, каждый со своим `priv(file) fn pf_dispatch`
(bool / int / generic-T), каждый ассертит СВОЙ возврат (111/222; 37/7; 41/-5).
Красные до фикса — валидная регрессия. Плюс neg-пара
`neg/privfile_free_fn_leak/{a_owner,b_intruder}.nv` (peer зовёт чужой
`priv(file) fn` → `E_FILE_PRIV_LEAK`) — ЗЕЛЁНАЯ (работает и сейчас). Добавить
вместе с полным фиксом. Содержимое — в истории ветки `fix-privfile-fn-scope`.

## Судьба 2 файлов Класса-2
Возвращены в `spec_tests/conformance/standalone/` (byte-identical оригиналу),
merged-CU зелёный. НЕ вносить в merged-CU до фикса generic-mono сайта.
