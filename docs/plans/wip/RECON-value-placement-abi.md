<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# RECON — размещение значений и ABI (план 172.15, Ф.0 — инвентарь прогоном)

Окно `p253-abi-inventory`, worktree `d:/Sources/nv-lang/nova-p253abi`, ветка
`p253-abi-inventory`. Модель: **sonnet**. Дата: 2026-08-08.

Метод по каждому пункту: минимальный пример `.nv` (синтаксис — только по
образцу существующих `spec_tests/conformance/*.nv`, откуда именно — указано),
`nova build --keep-artifacts`, дословная сигнатура из сгенерированного `.c`.
Изменений компилятора по существу НЕ вносилось — единственная правка
(инструментирование `sret_fn_eligible` для пункта (д)) закоммичена как
«времянка» (`25d5f1706`) и тем же окном отменена (`684a74acb`); в HEAD ветки
её нет.

Примеры лежат вне репозитория, в scratchpad-каталоге сессии:
`C:\Users\<user>\AppData\Local\Temp\claude\d--Sources-nv-lang-nova\...\scratchpad\p253abi\*.nv`
(пути ниже даны относительно этого каталога). Сгенерированные `.c` —
во временных `%TEMP%\nova_tests-<pid>\build-<hash>\*.c` (создаются
`--keep-artifacts`, каталоги названы по PID процесса сборки, приведены
дословно ниже для воспроизводимости в течение этой сессии).

Сборка велась через `nova-cli/target/release/nova.exe build <file> -o <out>
--keep-artifacts` с `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`, указанными на
`vcpkg_installed` основного репо (`d:\Sources\nv-lang\nova\...`), и с
libuv-сабмодулем, скопированным (не git-submodule-checkout) из основного
репо в `compiler-codegen/nova_rt/libuv` этого worktree — по образцу заметки
`project-worktree-nova-test-setup`. **Важное происшествие:** копирование
включило файл `.git` из чекаута основного репо (gitlink на несуществующий в
этом worktree `.git/modules/...`), из-за чего `git status`/`git commit` во
всём worktree стали падать с `fatal: not a git repository`. Файл
`compiler-codegen/nova_rt/libuv/.git` удалён (это не следует коммитить и не
коммитилось — сам каталог `libuv/` не под версионным контролем сабмодуля в
этом дереве, это лишь локальный build-input).

---

## (а) Значимая запись (value record) в параметре — копией или указателем?

**Файл:** `a_param_size.nv`. Синтаксис value-record — по образцу
`spec_tests/conformance/byref_ro_params.nv` (`type BigPair value { ro name
str, ro value str }`, Plan 172.14 Ф.1).

```nova
type Rec2 value { a int, b int }
type Rec8 value { a int, b int, c int, d int, e int, f int, g int, h int }
type Rec30 value { f1 int, ..., f30 int }   // 30 полей int

fn use_rec2(r Rec2) -> int => r.a + r.b
fn use_rec8(r Rec8) -> int => r.a + r.h
fn use_rec30(r Rec30) -> int => r.f1 + r.f30
```

**Команда:**
```
nova.exe build a_param_size.nv -o a_param_size --keep-artifacts
```
(`.c` → `%TEMP%\nova_tests-93916\build-b39d4020f6f4\a_param_size.c`)

**Сигнатуры дословно:**
```c
static nova_int nova_fn_7p253abi12a_param_size8use_rec2(NovaValue_Rec2 r);
static nova_int nova_fn_7p253abi12a_param_size8use_rec8(NovaValue_Rec8* r);
static nova_int nova_fn_7p253abi12a_param_size9use_rec30(NovaValue_Rec30* r);
```
Тела:
```c
static nova_int nova_fn_7p253abi12a_param_size8use_rec2(NovaValue_Rec2 r) {
    return nova_int_checked_add((r.a), (r.b));
}
static nova_int nova_fn_7p253abi12a_param_size8use_rec8(NovaValue_Rec8* r) {
    return nova_int_checked_add(((*r).a), ((*r).h));
}
static nova_int nova_fn_7p253abi12a_param_size9use_rec30(NovaValue_Rec30* r) {
    return nova_int_checked_add(((*r).f1), ((*r).f30));
}
```
Определение структуры (`int` = `nova_int` = intptr_t, 8Б на x64):
```c
struct NovaValue_Rec2 {
    nova_int a;
    nova_int b;
};
```
`Rec2` = 2×8Б = **16Б ровно**.

**Вывод:** граница ЕСТЬ, и это ровно `> 16Б`. `Rec2` (16Б) передаётся
**копией** (`NovaValue_Rec2 r`), `Rec8` (64Б) и `Rec30` (240Б) — **указателем**
(`NovaValue_Rec8* r`, `NovaValue_Rec30* r`), с авто-разыменованием в теле
(`(*r).a`). Это буквально механизм R3 из `spec/decisions/02-types.md` (раздел
про `ref`-режим, ревизия Р-184): «компилятор передаёт value-параметр скрытым
ro-указателем вместо копии, когда `sizeof > ~2*sizeof(ptr)` (≈16B)». 16Б
ровно ещё **не** превышает порог → копия; 64Б/240Б превышают → указатель.
Совпадает с наблюдаемым дословно.

---

## (б) Нерекурсивная сумма — где живёт (стек или куча)?

**Файл:** `b_sum_placement.nv`. Синтаксис — D406 (`enum`-маркер, только эта
форма), рекурсивная форма — по образцу
`spec_tests/conformance/d66_self_universal.nv` (`type D66Tree enum | D66Leaf
| D66Node(int, D66Tree, D66Tree)`).

```nova
type NonRecSum enum
    | NrA
    | NrB(int)
    | NrC(int, int)

type RecTree enum
    | RtLeaf
    | RtNode(int, RecTree, RecTree)

fn make_nonrec(x int) -> NonRecSum => NrB(x)
fn take_nonrec(s NonRecSum) -> int => match s { NrA => 0, NrB(v) => v, NrC(a,b) => a+b }
fn make_rec(x int) -> RecTree => RtNode(x, RtLeaf, RtLeaf)
fn take_rec(t RecTree) -> int => match t { RtLeaf => 0, RtNode(v,l,r) => v + l.count_dummy() + r.count_dummy() }
```

**Команда:**
```
nova.exe build b_sum_placement.nv -o b_sum_placement --keep-artifacts
```
(`.c` → `%TEMP%\nova_tests-78208\build-3bd1148d8232\b_sum_placement.c`)

**Сигнатуры дословно:**
```c
static Nova_NonRecSum* nova_fn_7p253abi15b_sum_placement11make_nonrec(nova_int x);
static nova_int nova_fn_7p253abi15b_sum_placement11take_nonrec(Nova_NonRecSum* s);
static Nova_RecTree* nova_fn_7p253abi15b_sum_placement8make_rec(nova_int x);
static nova_int nova_fn_7p253abi15b_sum_placement8take_rec(Nova_RecTree* t);
```
Конструктор (в т.ч. пустого unit-варианта `NrA`, БЕЗ payload):
```c
static Nova_NonRecSum* nova_make_NonRecSum_NrA(void) {
    Nova_NonRecSum* _r = (Nova_NonRecSum*)nova_alloc(sizeof(Nova_NonRecSum));
    _r->tag = NOVA_TAG_NonRecSum_NrA;
    return _r;
}
static Nova_NonRecSum* nova_make_NonRecSum_NrB(nova_int _0) {
    Nova_NonRecSum* _r = (Nova_NonRecSum*)nova_alloc(sizeof(Nova_NonRecSum));
    _r->tag = NOVA_TAG_NonRecSum_NrB;
    _r->payload.NrB._0 = _0;
    return _r;
}
```

**Вывод:** и нерекурсивная (`NonRecSum`), и рекурсивная (`RecTree`) сумма
живут **на куче** (`Nova_<X>*`, аллокация через `nova_alloc`) — механизм
ИДЕНТИЧЕН для обоих случаев, включая тривиальный unit-вариант `NrA` без
единого поля payload. **Механизма размещения нерекурсивной суммы на стеке
не существует** — ни для какого случая (не только для «сложных», для самого
минимального тоже). Прежний ответ интегратора «не реализовано» по существу
верен (стек-механизма нет), но по методу был неверен (обосновывался статусом
плана 172.4 Ф.5, а не пробой).

---

## (в) Возврат составного типа — `__sret`, по значению или через кучу?

**Файл:** `c_return_composite.nv`. Синтаксис named tuple — по образцу
`spec_tests/conformance/d215_named_tuple_value.nv` (`type D215Vec3(x f64, y
f64, z f64)`).

```nova
type Pt3(x int, y int, z int)
type RecPair { a int, b int }

fn make_tuple(v int) -> Pt3 => Pt3(v, v + 1, v + 2)
fn make_record(v int) -> RecPair => { a: v, b: v + 1 }
fn make_vec(v int) -> []int => [v, v + 1, v + 2]
```

**Команда:**
```
nova.exe build c_return_composite.nv -o c_return_composite --keep-artifacts
```
(`.c` → `%TEMP%\nova_tests-59132\build-b460692baa45\c_return_composite.c`)

**Сигнатуры дословно:**
```c
static NovaTuple_Pt3 nova_fn_7p253abi18c_return_composite10make_tuple(nova_int v);
static Nova_RecPair* nova_fn_7p253abi18c_return_composite11make_record(nova_int v);
static Nova_Vec____nova_int* nova_fn_7p253abi18c_return_composite8make_vec(nova_int v);
```
Тела:
```c
static NovaTuple_Pt3 nova_fn_...make_tuple(nova_int v) {
    return ((NovaTuple_Pt3){v, nova_int_checked_add(v, 1LL), nova_int_checked_add(v, 2LL)});
}
static Nova_RecPair* nova_fn_...make_record(nova_int v) {
    Nova_RecPair* _nv_tmp_432 = (Nova_RecPair*)nova_alloc(sizeof(Nova_RecPair));
    _nv_tmp_432->a = v;
    _nv_tmp_432->b = nova_int_checked_add(v, 1LL);
    return _nv_tmp_432;
}
static Nova_Vec____nova_int* nova_fn_...make_vec(nova_int v) {
    Nova_Vec____nova_int* _nv_tmp_433 = Nova_Vec____nova_int_static_new(0);
    (void)Vec____nova_int_method_push(_nv_tmp_433, v);
    (void)Vec____nova_int_method_push(_nv_tmp_433, nova_int_checked_add(v, 1LL));
    (void)Vec____nova_int_method_push(_nv_tmp_433, nova_int_checked_add(v, 2LL));
    return _nv_tmp_433;
}
```

**Вывод:** три РАЗНЫХ механизма для трёх случаев, и **ни один из трёх не
использует `__sret`**:
1. **Tuple (`Pt3`)** — возврат **по значению**, C-structура копируется через
   compound-literal (`return (NovaTuple_Pt3){...}`) — стек/регистры, ноль
   аллокаций.
2. **Record (`RecPair`)** — **через кучу**: `nova_alloc` внутри функции,
   возврат указателя (`Nova_RecPair*`).
3. **`Vec[T]`/`[]int` (`make_vec`)** — тоже **через кучу**: `_static_new(0)`
   + `push()`×N, возврат указателя. Показательно: C-тип возврата
   `Nova_Vec____nova_int*` **проходит фильтр ①** предиката `sret_fn_eligible`
   дословно (`Nova_Vec____` + одна `*`), но `__sret` НЕ применён — тело
   (array-литерал `[v, v+1, v+2]`, лежащий в основе `.new()+.push()`-
   последовательности) не матчит ни `Leaf` (`Self{...}`), ни `ChainTo`
   (`Vec[..].<m>`-вызов) → отсеян фильтром ②. Это независимое, отдельное от
   пункта (д) подтверждение того, что фильтр ① один периметр не задаёт.

`__sret` в этих трёх минимальных случаях просто не сработал ни разу — узость
периметра, установленная планом ранее (57 вхождений в другом, более крупном
файле conformance — факт, зафиксированный ДО этого прогона и не
переустанавливаемый здесь), этим прогоном подтверждена с другой стороны:
даже прямое попадание в фильтр ① не гарантирует применения.

---

## (г) `mut @` на примитиве

**Часть 1 — реальный `mut @` на непримитивном (но МАЛЕНЬКОМ, 8Б) value-типе.**
Файл `d_mut_at_value.nv`. Синтаксис mut-value-record с `mut @`-методом — по
образцу `spec_tests/conformance/d186_impl_annotation.nv` (`type
D186Countdown value { mut n int }`, `fn D186Countdown mut @next() ->
Option[int]`).

```nova
type Ctr value { mut n int }
fn Ctr mut @bump() -> int { @n = @n + 1; @n }
```

**Команда:**
```
nova.exe build d_mut_at_value.nv -o d_mut_at_value --keep-artifacts
```
(`.c` → `%TEMP%\nova_tests-44000\build-0424d1b7ee6e\d_mut_at_value.c`)

**Сигнатура дословно:**
```c
static nova_int Nova_Ctr_method_bump(NovaValue_Ctr* nova_self);
```
Точка вызова: `Nova_Ctr_method_bump(&(c))`.

**Вывод:** `Ctr` — 8 байт (один `int`-field), это МЕНЬШЕ порога `>16Б`,
установленного в пункте (а) для обычных ro-параметров — и тем не менее
receiver передан **указателем** (`NovaValue_Ctr*`). Подтверждает R5
(`spec/decisions/02-types.md`) дословно: «`mut @` ≡ `mut ref @` — всегда
by-pointer (любой размер)». Размерный порог (а) и правило (г) — РАЗНЫЕ
механизмы, что и требовалось разграничить.

**Часть 2 — попытка `mut @` на примитиве.** Файл `d_mut_at_primitive.nv`:
```nova
fn int mut @weird() -> int { @ }
```
**Команда:**
```
nova.exe build d_mut_at_primitive.nv -o d_mut_at_primitive
```
**Вывод компилятора дословно:**
```
error: [E_PRIMITIVE_MUT_METHOD] primitive type `int` cannot have mut-methods
(`fn int mut @weird(...)`): primitives are immutable by design — receiver is
passed by value, so in-place mutation through `@` has no observable effect on
the caller. Use a pure function returning a new value instead, e.g. `fn
weird(x int) -> int` (or a method returning a new int). See Plan 91
§«Nova-first», Plan 128 Ф.3.
```
Компиляция дальше не идёт (нет `.c`, дальше в цепочке ничего не проверялось).

**Вывод:** диагностика `E_PRIMITIVE_MUT_METHOD` существует, срабатывает
именно на объявлении `fn <примитив> mut @m()`, текст соответствует R5.
Дефект №468 (реестр 221.1) НЕ трогался и не чинился — только подтверждён
факт его существования как ЗАПРЕТА на объявление (а не как «примитив
мутируется»).

---

## (д) Почему `str.bytes()` не получает `__sret`

**Метод:** временное инструментирование `sret_fn_eligible`
(`compiler-codegen/src/codegen/emit_c.rs:49356`) — `eprintln!` с именем
функции, `ret_c`, формой тела `f.body` (усечённый `{:?}`) и причиной отказа.
Коммит `25d5f1706` («времянка»), собран `nova-cli` (release), прогнан на
сборке `a_param_size.nv` (через который транзитивно тянется `str.bytes()` —
`println` использует `Display`, а `std/src/prelude/protocols.nv:567`: `fn str
@display(mut f Fmt) -> () { f.write(@bytes()) }`), вывод захвачен, коммит
`684a74acb` откатывает инструментирование. **В HEAD ветки инструментирования
нет** (проверено `git diff HEAD -- compiler-codegen/src/codegen/emit_c.rs`
— пусто, свежий release-бинарник собран и проверен на отсутствие
`SRET_DEBUG` в выводе).

**Отладочный вывод дословно:**
```
SRET_DEBUG: fn=bytes ret_c="Nova_Vec____nova_byte*" body_shape=Block(is_unsafe=false, stmts=0, trailing_kind=Block(Block { stmts: [], trailing: Some(Expr { kind: Call { func: Expr { kind: Member { obj: Expr { kind: Path(["__array", "u8"]), span: Span { start: 4181, end: 4185, file_id: 10 }, id: ExprId(3904),)
SRET_DEBUG: fn=bytes passed filter (1); body_form=None
```

**Вывод — отсев ТОЧНО по фильтру ② (форма тела), НЕ по фильтру ①:**

1. `ret_c = "Nova_Vec____nova_byte*"` **проходит фильтр ①** (начинается с
   `Nova_Vec____`, оканчивается ровно одной `*`) — строка `passed filter (1)`
   в выводе это подтверждает буквально.
2. `sret_body_form(f)` возвращает **`None`** — это и есть отказ фильтром ②.
3. Причина видна в `body_shape`: тело функции — `FnBody::Block` (0 stmt),
   `trailing` = `ExprKind::Block(...)` (это `unsafe { … }` — исходник
   `std/src/runtime/string/core.nv:75`: `export fn str @bytes() -> ro []u8 =>
   unsafe { []u8.new(@ptr, @byte_len()) }`). Функция `sret_body_form`
   матчит `tail.kind` **только** на `ExprKind::RecordLit{…}` (Leaf) и
   `ExprKind::Call{…}` (ChainTo) — `ExprKind::Block` не покрыт НИ ОДНОЙ
   веткой match'а, падает в `_ => None`. Предикат **не разворачивает**
   `unsafe { }`-обёртку перед классификацией тела.
4. Двойная проверка (даже если бы `unsafe{}` разворачивался): усечённый
   `{:?}`-вывод показывает внутренний `Call` — его `func.obj.kind =
   Path(["__array", "u8"])` (внутреннее представление типа `[]u8`), а НЕ
   `Ident("Vec")`/`Path(["Vec", …])`. Ветка `ChainTo` в `sret_body_form`
   опознаёт базу вызова ТОЛЬКО как буквальный идентификатор `"Vec"`
   (`ExprKind::Ident(n) if n == "Vec"` либо `Path(parts) if parts[0] ==
   "Vec"`) — `[]u8.new(...)` под это не подходит НИ ПРИ каком разворачивании
   `unsafe{}`.
5. Комментарий над предикатом (`emit_c.rs:49355`) называет целевым примером
   «bytes→from_raw_parts» — но текущий исходник (грепом подтверждено)
   вызывает `[]u8.new(@ptr, @byte_len())`, НЕ `Vec[u8].from_raw_parts(...)`.
   Сам файл `emit_c.rs` (строки ~53850, ~53876) содержит комментарии,
   описывающие БОЛЕЕ РАННЮЮ форму реализации `bytes()`/`as_bytes` именно
   через `Vec[u8].from_raw_parts(ptr,len,len)` (Plan 139.2 Ф.0) — то есть
   предикат и его комментарий писались под форму исходника, которая с тех
   пор ИЗМЕНИЛАСЬ (переименование `as_bytes`→`bytes`, смена вызова на
   `[]u8.new`, обёртка в `unsafe{}`), а сам предикат синхронизирован не был.

**Итог:** отказ — по фильтру ②, с ДВУМЯ независимыми причинами (unsafe-
обёртка не разворачивается; даже если бы разворачивалась, база вызова не
`Vec`), поверх которых виден факт дрейфа реализации (`from_raw_parts` →
`[]u8.new`) относительно того, под что предикат писался. Ни один из трёх
возможных исходов, перечисленных в задании, не оказался «bytes вообще не
`FnDecl`» — `bytes()` ЕСТЬ обычная `.nv`-функция
(`std/src/runtime/string/core.nv:75`, `export fn str @bytes() -> ro []u8 =>
…`), проходит весь обычный codegen-конвейер как FnDecl.

---

## Сводная таблица

| Пункт | Механизм | Установлено? |
|---|---|---|
| (а) | value-record >16Б параметр → hidden pointer, ≤16Б → копия | ДА, сигнатурой |
| (б) | сумма (рекурсивная и нерекурсивная) — heap, `nova_alloc`, без различия | ДА, сигнатурой |
| (в) | tuple → by-value; record/`Vec[T]` → heap+pointer; `__sret` не сработал ни разу | ДА, сигнатурой |
| (г) | `mut @` всегда by-pointer (даже 8Б); примитив — `E_PRIMITIVE_MUT_METHOD` | ДА, сигнатурой + дословной диагностикой |
| (д) | `bytes()` отсеян фильтром ② (unsafe-обёртка не разворачивается + база вызова не `Vec`) | ДА, инструментированной трассой |

Все пять пунктов Ф.0 закрыты сигнатурами/трассами. Пунктов со статусом «не
установлено» нет.

---

## Что расходится со спекой

* **(а) — совпадает.** `spec/decisions/02-types.md`, раздел про `ref`-режим,
  правило R3: «...передаёт value-параметр скрытым ro-указателем вместо
  копии, когда `sizeof > ~2*sizeof(ptr)` (≈16B)». Наблюдаемая граница (16Б —
  копия, 64Б/240Б — указатель) совпадает буквально.
* **(б) — совпадает, но по умолчанию отсутствия механизма.** Таксономия
  (`02-types.md`, строка ~3050, раздел «Явная таксономия value vs reference
  типов») безусловно относит `Sum types` к «managed heap / by reference» —
  без исключения для нерекурсивных. Наблюдаемое поведение (heap для ОБОИХ
  случаев) спеке НЕ противоречит. Отдельного нормативного текста про
  «нерекурсивная сумма может жить на стеке» в спеке НЕТ — то есть спрашивать
  о расхождении здесь не с чем: спека обещает heap безусловно, реализация
  делает heap безусловно.
* **(в) — расхождения констатировать не с чем, т.к. спеки НЕТ вовсе.**
  Плановый факт (раздел «Установленные факты» плана 172.15): «`sret`/`_out`
  не описаны в `spec/`». Прогон это подтверждает с обратной стороны: даже
  РАЗНЫЕ, несовместимые друг с другом механизмы возврата (by-value/heap+ptr/
  heap+ptr-без-sret) сосуществуют без единого нормативного описания, какой
  когда применяется. Пользователю негде прочитать, что `Vec[T]`-возврат из
  literal-массива всегда аллоцирует, а не использует sret.
* **(г) — совпадает.** R5 дословно: «`mut @` ≡ `mut ref @` — всегда
  by-pointer (любой размер...)», и «Primitives никогда mut-method... Plan
  128 Ф.3 `E_PRIMITIVE_MUT_METHOD` diagnostic enforces». Оба утверждения
  подтверждены буквально (8Б `Ctr` → pointer; примитив → диагностика с этим
  же кодом).
* **(д) — расхождение есть, но не со спекой (спеки нет), а ВНУТРИ
  реализации.** Комментарий-обоснование предиката `sret_fn_eligible`
  (`emit_c.rs:49355`, «fixpoint глубиной 1: bytes→from_raw_parts») описывает
  форму исходника `bytes()`, которая с тех пор изменилась (миграция на
  `[]u8.new`, обёртка `unsafe{}` — Plan 139.2 и последующие), а сам предикат
  синхронизирован не был. Это ровно тот класс дефекта, который план 172.15
  называет мотивацией: «границы заданы формой... а расхождение реализации
  со спекой не ловится ни одной проверкой» — здесь то же самое происходит
  ДАЖЕ БЕЗ участия спеки, между комментарием-документацией и кодом внутри
  одного файла.

## Файлы

* Отчёт: `d:\Sources\nv-lang\nova-p253abi\docs\plans\wip\RECON-value-placement-abi.md`
* План: `d:\Sources\nv-lang\nova-p253abi\docs\plans\172.15-value-placement-abi-audit.md`
* Инструментирование (снято): коммиты `25d5f1706` → `684a74acb` в ветке
  `p253-abi-inventory`
* Предикат-объект исследования: `compiler-codegen/src/codegen/emit_c.rs:49356`
  (`sret_fn_eligible`), `:682` (`SretBodyForm`), `:49381` (`sret_body_form`)
* Источник `bytes()`: `std/src/runtime/string/core.nv:75`
* Примеры `.nv` (вне репозитория, scratchpad сессии): `a_param_size.nv`,
  `b_sum_placement.nv`, `c_return_composite.nv`, `d_mut_at_value.nv`,
  `d_mut_at_primitive.nv`
