<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 ВОЛНА-1 — разведка-зонды (капстоун-подготовка): чекпойнт
(sonnet, worktree `nova-196probe`, ветка `p196-probes`)

**Родитель:** [196-capstone-notes.md](../196-capstone-notes.md) / [196-capstone2-notes.md](196-capstone2-notes.md)
§«рекомендация следующей волне». **Задание:** (1) phase-1c pre-scan репро для
`B10m_ident_empty_fallback`; (2) для каждого из 4 терминалов
(`B11al_panic_method_p67`/`B12q_panic_path_p67`/`B12r_panic_path_no_method_seg`/
`B12s_panic_path_no_parts`) — минимальная red-фикстура + заметка о нужном
чекер-фиксе. БЕЗ правок компилятора — только новые тест-файлы + заметки.
**База:** main `78503bf5d`.

---

## 0. Инфраструктура

- Свой release-бинарь: `cargo build --release --manifest-path nova-cli/Cargo.toml`
  (3м26с) → `nova-cli/target/release/nova.exe`.
- Свой debug-бинарь (для `NOVA_TRACE_ICR=1` подтверждения): `cargo build
  --manifest-path nova-cli/Cargo.toml` (1м41с) → `nova-cli/target/debug/nova.exe`.
- `libuv`-submodule пуст в свежем worktree → скопирован из `d:/Sources/nv-lang/nova`
  (main repo) + `.git` внутри удалён (иначе конфликт submodule-состояния).
- Env: `NOVA_GC_LIB_DIR`/`NOVA_INCLUDE_DIR`/`NOVA_GC_INCLUDE_DIR` →
  `d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/{lib,include}`
  (главный репо — vcpkg_installed НЕ копировался, воркtree читает из main).
- **Фикстуры-репро живут в `spec_tests/fixtures/known_red/`** (не
  `spec_tests/conformance/standalone/`, как буквально предлагало задание) —
  разведочное решение: `known_red/` физически СНАРУЖИ `spec_tests/conformance/`
  (README.md там же: «карантин вне дефолт-гейта»), поэтому `nova test
  spec_tests/conformance` (рекурсивный обход, ОДНА команда, авторитетный гейт)
  их не подхватывает вообще. `standalone/` — отдельный COMPILE-UNIT (не красит
  ДРУГИЕ файлы), но он ФИЗИЧЕСКИ ВНУТРИ `spec_tests/conformance/` и рекурсивный
  `nova test spec_tests/conformance` его обходит — permanently-red фикстура там
  дала бы «1 failed» в summary авторитетного гейта. `known_red/` — уже
  установленная конвенция именно для permanently-red репро (3 файла там же до
  этой сессии), с README-реестром маркеров. Каждый файл — `module
  known_red.<name>` (D78: путь модуля = `<родитель-папки>.<имя-файла>` для
  файла НЕ в `standalone/`).

---

## 1. Phase-1c pre-scan репро — `B10m_ident_empty_fallback`

**Вывод: РЕПРО ЕСТЬ, подтверждено эмпирически (CC-FAIL + `NOVA_TRACE_ICR=1`
ICR-HIT).** Ветка НЕ мертва по построению — легитимный phase-1c pre-scan путь
реален и сегодня мискомпилирует корректный Nova-код.

### 1.1 Механизм (по коду)

`B10m`'s doc-комментарий (emit_c.rs ~51775): «In phase-1c pre-scan the
function registry is not yet populated — return empty so callers degrade to
nova_unit». «Phase-1c» — не отдельная именованная фаза чекера (`f1_expr` в
types/mod.rs — это ДРУГОЕ, «phase 1» чекер-обхода); термин взят из ОРИГИНАЛЬНОГО
коммита `c8b2f94e1` («fix(172.1 P67 ФАЗА2): infer_expr_c_type_legacy —
устранить серию паник»), commit message явно называет источник: «Phase-1c
param seeding: временно добавлять params в var_types перед `return_type_c()`
для expression-body fn без explicit return type — fix для `fn d45_double(x
int) => x * 2`». Т.е. «phase-1c» = сам ЭТАП emit_c.rs, где КОДОГЕН (не чекер)
пере-derive'ит C-тип функции ДО того, как остальные регистры кодогена
(`user_fn_sigs`/`type_aliases`/…) заполнены целиком.

Три релевантных прохода emit_c.rs (`emit_module`), все итерируют
`module.items` в ТЕКСТОВОМ порядке файла:

1. **D84 overload-registration** (~6402) — заполняет `method_overloads`,
   НЕ `user_fn_sigs`.
2. **Pre-seed** (~7027-7043, Plan 209 Ф.3 фикс) — `if
   !self.user_fn_sigs.contains_key(&f.name) { … self.user_fn_sigs.insert(f.name,
   (params, self.return_type_c(f))) }` — **write-once**, один проход, БЕЗ
   повторной попытки. Существует специально для класса «module-level `ro
   NAME = free_fn()` initializer» (см. доккомментарий ~6990-7026: чекер НЕ
   аннотирует callee module-level `ro`-инициализатора так же, как внутри тела
   fn — Channel 2 для этого класса пуст, кодоген обязан re-derive сам).
3. **"Реальный" `emit_fn_forward_decl`** (~14282+, вызывается позже для
   КАЖДОЙ fn) — **безусловный** overwrite `user_fn_sigs.insert(...)` при
   КАЖДОМ вызове (не гейтирован `contains_key`).

Чекер-продюсер (types/mod.rs ~8477-8518, Zone CH/`p196-rtbuf-producers`,
коммит `ba9a8a2f3`, «Q1/static-return producer: bare free-fn call declared
return») **намеренно молчит** (`None`) для callee с `FnBody::Expr` без
явного `-> T` (комментарий ~8505-8509: «this producer doesn't do that
inference, so it must stay silent (None) rather than lie Unit») — то есть
Channel 2 НИКОГДА не покрывает bare-call к expr-body unannotated free fn,
вне зависимости от того, где сидит вызов.

**Цепочка бага:** `helper` (expr-body, без `->`) зовёт `compute` (тоже
expr-body, без `->`), `compute` объявлена ПОСЛЕ `helper` в файле. Верхнеуровневый
`ro y = helper(21)`.
- Прогон **прохода 2** (pre-seed) в текстовом порядке: `helper` первым →
  `return_type_c(helper)` → `infer_expr_c_type(compute(x))` →
  `infer_call_ret_c`'s `Ident`-каскад → `user_fn_sigs.get("compute")` МИСС
  (проход ещё не дошёл до `compute`) → **`B10m_ident_empty_fallback`** →
  `""` → `return_type_c` конвертирует в `"nova_unit"` → `user_fn_sigs["helper"]
  = (…, "nova_unit")` **ЗАПИСАНО НАВСЕГДА** (write-once guard).
  Далее `compute` обрабатывается (второй в проходе) без forward-зависимости
  → корректно `"nova_int"`.
- Проход, эмитящий `ro y = helper(21)`'s storage (~7044+, СРАЗУ после
  pre-seed) читает `user_fn_sigs.get("helper")` = **`"nova_unit"` (ещё
  неисправленное)** → объявляет `_nova_const_y_value` как `nova_unit`.
- **Проход 3** (`emit_fn_forward_decl`, ПОЗЖЕ, безусловный overwrite) при
  повторном вычислении `helper` находит `compute` УЖЕ в `user_fn_sigs`
  (корректно `nova_int` от прохода 2) → `helper` **самоисцеляется** до
  `nova_int` — но `y`'s storage-тип УЖЕ объявлен раньше и не пересматривается.
- Результат: `helper` в собственной C-декларации — `nova_int` (верно), но
  `_nova_const_y_value` — `nova_unit` (СТАРОЕ неверное значение) →
  инициализатор `_nova_const_y_value = nova_fn_..._helper(21);` (nova_int) в
  `nova_unit`-слот → **CC-FAIL**.

### 1.2 Фикстура + подтверждение

`spec_tests/fixtures/known_red/p196_b10m_phase1c_probe.nv` (module
`known_red.p196_b10m_phase1c_probe`):

```nova
fn helper(x int) => compute(x)
fn compute(x int) => x * 2

ro y = helper(21)

test "…" { assert(y == 42) }
```

**Release-бинарь** (`nova test p196_b10m_phase1c_probe.nv`):
```
CC-FAIL  p196_b10m_phase1c_probe.c:9154:25: error: assigning to 'nova_unit'
from incompatible type 'nova_int' (aka 'long long')
```
Сгенерированный `.c` (до очистки): `static nova_unit _nova_const_y_value;`
(строка 1208) vs `static nova_int nova_fn_..._helper(nova_int x);`
(корректный forward-decl `helper`, строка 1696) vs инициализатор `_nova_const_y_value
= nova_fn_..._helper(((nova_int)21LL));` (строка 9154) — ТОЧНО совпадает с
разбором §1.1.

**Debug-бинарь + `NOVA_TRACE_ICR=1`** — прямое подтверждение:
```
[ICR-HIT] B10m_ident_empty_fallback
[ICR-HIT] B10f_user_fn_sigs           ← найденный (но неверный) lookup "helper" на ro-сайте
… (B01/B06/B06b/B07/B07r/B10j×2/B11a/B11d — прелюдия-шум, Vec-инстанциация)
```

### 1.3 Вывод / рекомендация

**B10m НЕ кандидат на снос** — легитимный, живой путь, ПОДТВЕРЖДЁННО
воспроизводимый (не гипотеза). Фикс (вне мандата разведки):
(а) научить Q1-продюсер (types/mod.rs ~8510) реально инферить expr-body
callee (body-инференция вместо молчания) — закрыло бы Channel 2 для ЭТОГО
класса целиком, ИЛИ (б) сделать pre-seed-проход (emit_c.rs ~7027) итеративным
до фикспоинта (или topological order по call-графу свободных фн) вместо
одного write-once прохода в текстовом порядке файла. Оба — правки ВНЕ
frozen-зоны `infer_call_ret_c` (types/mod.rs + окрестности emit_c.rs ~7027,
не сама функция ~50418-52546).

---

## 2. Терминалы (4) — red-зонды

Контекст: `B12q_panic_path_p67`/`B11al_panic_method_p67` были ЖИВЫМИ до
волны-3 (`196.5-stage-d-wave3-notes.md` §2) на конкретных формах
(`T.deserialize` generic-body Path-call / `str.until`), которые волна-3
ПОЧИНИЛА (чекер-продюсер для `deserialize`, prelude-методы `str.until`/`.after`
для D251). Ре-замеры (wave-3/4, capstone, capstone2) — 0 hit на всём
доступном корпусе с тех пор. Задание этой волны: НЕ полагаться на «раньше
было живо», а сконструировать НОВЫЕ минимальные фикстуры, бьющие каждый из 4
терминалов СЕГОДНЯ, если такой путь вообще достижим конструированием (не
обязательно естественным кодом).

(заполняется по ходу зондирования — см. ниже)
