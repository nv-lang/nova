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

**Вывод: ВСЕ 4 терминала — ЖИВЫЕ (не dead-by-construction), подтверждено
конструированными red-фикстурами.** Ни одна не встречается на естественном
корпусе (std/examples/conformance) — все 4 требуют ДЕЛИБЕРАТИВНО
сконструированного edge-case (незнакомое имя метода на builtin-ресивере /
парсер-жадность на Path-цепочках / вызов через нестандартную callee-форму).
Общий структурный вывод по всей серии зондов: **чекер валидирует
существование метода/пути ПОЛНОСТЬЮ надёжно только для «нормальных» форм
(Member на user-типе с явным `E_UNKNOWN_METHOD`); для ТРЁХ синтаксических
классов — raw-pointer intrinsics, PascalCase Path-цепочки, callee через
произвольное выражение — валидация либо отсутствует, либо неполна, и
дыра всплывает не как честная диагностика, а как ЖЁСТКАЯ ПАНИКА кодогена.**

**Критическая операционная находка (эмпирическая, не предполагалась заданием):
все 4 терминала — `panic!`, который `nova test`/`nova build` НЕ перехватывают
(нет `catch_unwind` вокруг per-file кодогена).** Процесс завершается
`exit=101` («This is a bug in nova. Please report it.») БЕЗ печати
summary — подтверждено на batch-вызове (`nova test <5 файлов>` включая один
панический) и на соло-вызове одного файла: оба разу — тот же обрыв без
результатов. Значит `EXPECT_COMPILE_ERROR`-маркер в фикстурах ниже —
**документирующий, не проверяемый сегодняшним раннером** (до сравнения
подстроки дело не доходит — процесс падает раньше, чем раннер успевает
сравнить). Прямое следствие: **эти 4 файла нельзя гонять в одном вызове
`nova test` вместе с ЛЮБЫМИ другими файлами** — паника одного молча
обрывает результаты всех остальных в том же вызове (не просто красит СВОЙ
результат, а вообще не даёт напечататься summary). Это даже более острая
причина держать их в `known_red/` (отдельные, всегда соло-вызовы), чем
просто «не красить conformance».

### 2.1 `B11al_panic_method_p67` — raw-pointer метод вне аллоулиста

**Фикстура:** `spec_tests/fixtures/known_red/p196_b11al_terminal_probe.nv`.
```nova
fn main() {
    mut arr = [1, 2, 3]
    ro p = arr.ptr()                       // *int (D216 §21, Vec[T]@ptr())
    ro r = p.zzz_unknown_ptr_method()      // имя НЕ в is_raw_pointer_intrinsic_method
    assert(r == 1)
}
```
**Механизм:** `is_raw_pointer_intrinsic_method` (types/mod.rs ~34431, D216
§21 таблица read/write/offset/dist/copy_*) гейтит ТОЛЬКО материализацию
Channel 2 (types/mod.rs ~8153) — не используется чекером как ОТКАЗ для
незнакомого имени. Чекер, похоже, вообще не валидирует существование
метода на `*T`-ресивере (в отличие от Member-вызова на user-типе, где
`E_UNKNOWN_METHOD` сработал бы раньше). emit_c.rs's
`B11d_typed_pointer_methods` (~52150ish) тоже не знает имя →
`fn_ret_<method>`-фоллбеки (B11ae/B11af) тоже мимо (имя нигде не объявлено
как реальный `@method`) → терминал.

**Наблюдаемо:** `nova: internal error at emit_c.rs:52519: [P67-LEGACY]
method call `.zzz_unknown_ptr_method` return type unknown … obj_ty="nova_int*"
obj=Ident(p) …` (exit=101).

**Нужный фикс:** чекер обязан отклонять метод на `*T` вне
`is_raw_pointer_intrinsic_method`-аллоулиста как `E_UNKNOWN_METHOD`,
симметрично любому другому ресиверу — сейчас `*T` единственный тип,
пропускающий произвольное имя без диагностики на чекер-уровне.

### 2.2 `B12q_panic_path_p67` — Path-форма, неизвестный статик-метод

**Фикстура:** `spec_tests/fixtures/known_red/p196_b12q_terminal_probe.nv`.
```nova
type Foo { x int }
fn main() {
    ro r = Foo.zzz_static_unknown()    // parses to ExprKind::Path(["Foo","zzz_static_unknown"])
    assert(r == 1)
}
```
**Механизм:** тот же класс, что серде `T.deserialize` (закрыт волной-3
ТОЧЕЧНЫМ producer'ом на `parts[1]=="deserialize"`) — ЛЮБОЙ ДРУГОЙ
2-сегментный Path-вызов с неизвестным `method_name` (не Channel.new/
ChanReader/try_from/effect_schemas/var_types fn_ret_*/sum-variant ctor)
падает в тот же терминал. Волна-3 закрыла ОДНО конкретное имя
(`deserialize`), не общий класс.

**Наблюдаемо:** `nova: internal error at emit_c.rs:52662: [P67-LEGACY] Path
call return type unknown for method=zzz_static_unknown …` (exit=101).

**Нужный фикс:** ОБЩИЙ producer/валидатор для ЛЮБОГО Path-form
static-call — чекер обязан проверять `method_name` против реального набора
методов `Type` (method_overloads/all_methods), а не полагаться на
point-fix per имя (тот же паттерн, что закрыл `deserialize`, но без
привязки к конкретной строке метода).

### 2.3 `B12r_panic_path_no_method_seg` — Path длиннее 2 сегментов

**Фикстура:** `spec_tests/fixtures/known_red/p196_b12r_terminal_probe.nv`.
```nova
type Foo { x int }
fn main() {
    ro r = Foo.bar.zzz_unknown()   // Path(["Foo","bar","zzz_unknown"]) — 3 сегмента
    assert(r == 1)
}
```
**Механизм:** parser/mod.rs ~8611-8636 — как только базовый идент
PascalCase, парсер жадно глотает ЛЮБУЮ цепочку `.Ident` в ОДИН `Path`
(`next_upper` вычисляется, но явно НЕ используется — `let _ = next_upper;`
— комментарий в коде признаёт эту жадность как сознательное упрощение).
`infer_call_ret_c`'s Path-арм проверяет ТОЛЬКО `parts.len() == 2` — любая
другая длина падает в `else`, даже не пытаясь резолвить последний сегмент.

**Наблюдаемо:** `nova: internal error at emit_c.rs:52665: [P67-LEGACY] Path
call return type unknown (no method segment) …` (exit=101).

**Нужный фикс:** (а) чекер обязан отклонять `Foo.bar…` где `bar` — не
существующее под-пространство/поле `Foo` (парсер допускает синтаксически
ЛЮБУЮ цепочку, чекер её для этой формы, похоже, не проверяет вовсе); (б)
codegen-терминал для `len()!=2` мог бы хотя бы попробовать резолвить
ПОСЛЕДНИЙ сегмент как метод-имя вместо немедленной паники.

### 2.4 `B12s_panic_path_no_parts` — callee через произвольное выражение

**Фикстура:** `spec_tests/fixtures/known_red/p196_b12s_terminal_probe.nv`.
```nova
fn one() -> int { 1 }
fn two() -> int { 2 }
fn main() {
    mut fns = [one, two]
    ro r = fns[0]()     // func = ExprKind::Index{...}, ни Ident/Member/Path
    assert(r == 1)
}
```
**Механизм:** вызов через ИНДЕКС в массив fn-значений — `func` вызова
`ExprKind::Index`, не Ident/Member/Path вообще. Чекер, судя по всему, не
материализует Channel 2 для вызова через произвольное fn-значение,
полученное индексированием (в отличие от прямого HOF-параметра/переменной)
— co-miss с легаси, который для ЭТОЙ формы `func` вообще ничего не
пытается резолвить и падает в финальный catch-all `else`.

**Наблюдаемо:** `nova: internal error at emit_c.rs:52669: [P67-LEGACY] Path
call return type unknown (no parts) …` (exit=101) — сообщение унаследовано
от Path-ветки по имени функции-обёртки, хотя `func` здесь вообще не Path.

**Нужный фикс:** чекер обязан канализировать Channel 2 для ЛЮБОГО вызова,
где callee — значение fn-типа, полученное произвольным выражением
(Index/Call/Ternary/…), извлекая return type СТРУКТУРНО из
`TypeRef::Func` элемента (тот же путь, что уже работает для HOF-
параметров/переменных напрямую).

### 2.5 Сводка

| Терминал | Фикстура | Строка панике | Нужный чекер-фикс |
|---|---|---|---|
| `B11al_panic_method_p67` | `p196_b11al_terminal_probe.nv` | emit_c.rs:52519 | `E_UNKNOWN_METHOD` на `*T` вне `is_raw_pointer_intrinsic_method` |
| `B12q_panic_path_p67` | `p196_b12q_terminal_probe.nv` | emit_c.rs:52662 | общий Path-static-call validator/producer (не point-fix per имя) |
| `B12r_panic_path_no_method_seg` | `p196_b12r_terminal_probe.nv` | emit_c.rs:52665 | валидация Path-цепочки длиннее 2 сегментов (парсер жадно глотает `.Ident`×N) |
| `B12s_panic_path_no_parts` | `p196_b12s_terminal_probe.nv` | emit_c.rs:52669 | Channel 2 для callee через произвольное выражение (Index и т.п.) |

**Все 4 — вне мандата этой сессии (чекер-правки, types/mod.rs + parser/mod.rs,
frozen-зона не трогалась).** Готовы как красные регрессионные маячки для
будущей волны, которая займётся именно чекер-валидацией этих 3 синтаксических
классов — в этот момент терминалы codegen-стороны (emit_c.rs) действительно
станут structurally unreachable и уйдут вместе с финальным сносом
`infer_call_ret_c`.

---

## 3. Итог для капстоуна

1. **`B10m_ident_empty_fallback`** — НЕ кандидат на снос. Живой
   phase-1c pre-scan путь, подтверждённый CC-FAIL-репро (§1). Реестр
   `infer_call_ret_c` остаётся 48 (не тронут).
2. **4 терминала** — ВСЕ живые (по построению, не по корпусу). Каждый
   получил red-фикстуру + точный чекер-фикс-рецепт (§2). Побочная
   находка: паники в `infer_call_ret_c` НЕ перехватываются `nova test`
   вообще (exit=101, весь батч обрывается) — важно для дисциплины
   будущих корпус-переписей волн (детач+panic методика тоже полагается на
   то, что panic обрывает ОДИН прогон целиком, что уже было известно, но
   это первое прямое подтверждение, что то же самое верно для ЕСТЕСТВЕННО
   достижимых, не-детач панике).
3. **Компилятор НЕ правился** (задание). Все 5 фикстур — в
   `spec_tests/fixtures/known_red/` (не `standalone/` — обоснование в §0).
