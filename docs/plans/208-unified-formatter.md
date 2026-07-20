<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 208 — Unified Formatter (`@display(mut f Fmt)`, байтовый `Write`, zero-alloc)

**Статус:** 🔨 Ф.0-Ф.3 РЕАЛИЗОВАНЫ (2026-07-16, ветка `p208-impl`, owner-go получен). Ф.0 (спека D422
keystone + амендменты) и Ф.1 (буфер-примитивы `.nv`, аддитивно) слиты в main ранее (`edcc4ab73`,
`b6ee6f40a`). Ф.2 (когерентная волна: `Write`/`Fmt`/`FmtCtx`/энумы в std, `emit_c.rs`-диспатч на
`@display(f)`/`@debug(f)`, снос `@display_fmt`-пути, миграция всех известных потребителей —
`json.nv`, `spec_tests/conformance/d374_*`/`d229_*`/бывшие `d419_*`→`d422_*`) — сделана на ветке
`p208-impl`, **с тремя задокументированными V1-упрощениями** (см. `spec/decisions/02-types.md#d422`
§«Статус реализации» и `docs/plans/wip/208-impl-progress.md`). **Ф.3 (дженерики `.nv` `[]T`/`Vec[T]`/
`Option`/`Result` Display/Debug + auto-derive компактной Display-формы) — СДЕЛАНА** (волна 2,
`docs/plans/wip/208-impl-progress.md` §«Ф.3»; попутно найдены и починены 3 тихих пре-существующих
дефекта: `Vec[T]` структурно не удовлетворял Display/Debug, `Option`/`Result` вообще не имели
`@display`, auto-derive Display на примитивном поле звал ретрактированный `str.from`). **Ф.4 → РЕДИЗАЙН Ф.4R (§10R, владелец 2026-07-20: «один путь, по максимуму в .nv») — big-bang распущен на Ш1-Ш4.** Исходная запись: **Ф.4 (снос
`conv.h` → буфер-примитивы Ф.1) — НЕ начата**, заблокирована — волна 2 провела разведку (без кода) и
нашла ДОПОЛНИТЕЛЬНЫЙ блокер сверх V1-упрощения #3: буфер-примитивы Ф.1 не имеют quote/escape-логики
для Debug `str`/`char` (нужна с нуля), плюс их module-privacy (D422 §5) требует переписывать
примитивные `@display`/`@debug`-тела на метод-диспатч, а не просто менять C-вызовы — делает Ф.4 ОДНОЙ
big-bang волной, не безопасными мелкими шагами (см. `wip/208-impl-progress.md` §«Ф.4 — статус:
РАЗВЕДКА»). Заодно найден orthogonal, pre-existing gap: bare `${vec}`-интерполяция для generic-типов
не дозванивается до их собственного `@display`/`@debug` (`[M-208-generic-interp-display-dispatch-gap]`,
**✅ ПОЧИНЕНО 2026-07-17, ветка `p-interp-generic-dispatch`** — см.
`docs/plans/208-impl-progress.md` §НАХОДКА и `docs/simplifications.md`; НЕ регрессия 208).
Авторитетный полный `spec_tests/conformance`-гейт — за
оркестратором/интегратором (не гонялся здесь, только таргетные изолированные фикстуры).
**Приоритет:** ниже Plan 196. **Язык-меняющее** → D-амендменты в том же слиянии, что код.
**Родитель-реестр:** [Plan 200 Пункт 8](200-std-improvements.md).

**⚠️ Координация с 152.7.2 (D419) — обязательна ПЕРЕД go на Фазу 0/1 (заметка владельца 2026-07-16):**
[152.7.2](152.7.2-format-context.md) (`в работе`, ветка `d419-format-fmt`) строит **двух-методную D419-версию
той же поверхности** (`@display(Write)` + `@display_fmt(Fmt)` + interp-direct-to-sink `[M-152.7.2]` /
`[M-d419-interp-direct-primitives]`). 208 её **надстраивает/сменяет, а не расходится**: D419 208-спекой уже
помечен RETRACT→D422 (в main, ветка `plan208-spec` слита) → 208 сворачивает `@display_fmt` в единый
`@display(mut f Fmt)`. НО **interp-direct-to-sink — общий, нужен обоим**: перенести/переиспользовать наработку
152.7.2, не дублировать и не выбрасывать. Перед стартом реализации: сверить, что реально влито из 152.7.2,
и переориентировать её остаток на одно-методный 208-дизайн (НЕ мёржить конфликтующий двух-методный D419-слой
поверх ретракта). Обе задачи — про одну format-поверхность.

## 0. Мотивация / цель

Свести форматирование к ОДНОЙ поверхности, убрать дубли, максимизировать nv-sourcing (§3), zero-alloc:
- **один метод** `@display(mut f Fmt)` вместо пары `@display(Write)` + `@display_fmt(Fmt)` (претензия владельца:
  два метода — некрасиво; в Rust один `Display::fmt(&self, f: &mut Formatter)`);
- **`Write` пишет `[]u8`**, не `str` — фундаментальнее, унифицирует с `io.Write` (Plan 176), открывает zero-alloc;
- **буфер-примитив** `int→bytes` в `.nv` (не C); C остаётся ТОЛЬКО на float-body (shortest round-trip непортируем);
- **почти весь `conv.h`/`nova_fmt_*` переезжает в `.nv`** (int/bool/char/радикс/pad — операции над `*u8`).

## 1. Ключевые решения (converged 2026-07-14/15)

| Вопрос | Решение |
|---|---|
| Один метод или два | ОДИН `@display(mut f Fmt)`; `@debug(mut f Fmt)` — сиблинг (как отдельный `Debug` у Rust) |
| `@display_fmt`/D419 | **RETRACT** — сворачивается в единый `@display(f)` |
| Sink пишет | `[]u8` (байты); `Fmt` embeds `Write` через `use` (D145 protocol-embed) |
| float | **shortest** (существующий C-extern, dtoa/Ryu-класс). round-trippable+красиво; JSON round-trip ок. НЕ голый libc `%g` |
| Радикс `{:x}`/`{:b}` | ось `f.kind()` в едином `@display` (не плодим трейты как Rust `LowerHex`/…) |
| pad (width/align/fill) | **компилятор владеет** (auto-pad), тип НЕ обязан звать pad → нет Rust-footgun'а «забыл `f.pad()`» |
| pad-механизм для width-композита | **pad_in_place** на главном sb (mark + measure + memmove для right/center; left — без сдвига) |
| Scratch-SB | только осмыслен как альтернатива pad_in_place; выбран in-place |
| `pad_consumed` | если тип сам звал `f.pad(...)` — компилятор внешний pad не навешивает (зеркало текущего `precision_consumed`) |
| **fallibility** | fmt-`Write.@write([]u8) -> ()` **инфаллибельно** (пишет в растущий SB, не падает); io-`Write` → `Result[(), IoError]` (Plan 176). **Два байтовых РОДСТВЕННИКА** (единый шейп `@write([]u8)`, разные сигнатуры), НЕ один протокол — как `fmt::Write`/`io::Write` у Rust. Форматировать в файл → мост: SB (инфаллибельно) → `file.write(sb.bytes())?`. Конвенция [177/D325]: std=`Result`, не `Fail`-эффект → инфаллибельной операции Result не нужен |
| `into_str()` | **assume-valid-UTF-8** (писатели гарантируют) + checked-вариант `Result[str,Utf8Error]` для сырого байт-sink |

**Решено 2026-07-15:** буфер-примитив = **внутренний** рендер `int_fmt(v,buf,cap)->len` (не публичный `@to_str`);
`Write`↔`io.Write` — **СЛИТЬ** (коорд. 176), направления `Read`/`Write` НЕ сливать; str-форма БЕЗ overload'а —
str-литерал `w.write("...")` через **коэрсию str-литерал→`[]u8`** (амендмент **D55**, ОБЩЕЕ правило для любой
`[]u8`-позиции, zero-copy), str-переменная → явный `.bytes()` (D176);
`width` = **кодпоинты** (Rust-парити; графемы/дисплей-колонки — future-ось, у нас уже есть unicode/grapheme-таблицы);
`str.from_debug` — **ретракт** вместе с `str.from`. **Новое (investigation):** buffer-consolidation — см. §6.

## 2. Протоколы sink

```nova
export type Write protocol {         // МИНИМАЛЬНЫЙ: только @write; reserve/advance — на StringBuilder (см. §9)
    mut @write(bytes []u8) -> ()     // параметры ro по умолчанию → уже read-only; str-литерал → []u8 коэрсия (D55, §6)
    // reserve/advance — КОНКРЕТНО на StringBuilder (не в протоколе); zero-copy = компилятор через SB
}

export type Fmt protocol {           // Fmt embeds Write (D145 protocol-embed) + оси спека
    use Write                        // компонует @write([]u8) из Write (D145, parse_protocol_body подтверждает)
    @width()     -> Option[int]
    @precision() -> Option[int]
    @align()     -> Option[Align]
    @fill()      -> char
    @sign()      -> Sign
    @alternate() -> bool
    @kind()      -> FmtKind           // Display | Debug | Hex | Oct | Bin | Exp
    mut @pad(bytes []u8) -> ()        // тип-управляемый паддинг → ставит pad_consumed
}

type Align   enum Left | Right | Center      // D406
type Sign    enum Minus | Plus
type FmtKind enum Display | Debug | Hex | Oct | Bin | Exp

export type Display protocol { @display(mut f Fmt) }   // REQUIRED (ревью: без to_str-дефолта, см. §9 инвариант)
export type Debug   protocol { @debug(mut f Fmt) }     // REQUIRED
```
`StringBuilder` реализует `Write`; компилятор на каждом `${x:SPEC}` строит `FmtCtx` (реализатор `Fmt`),
обёрнутый вокруг главного sb + распарсенного спека.

## 3. Алгоритм — БЕЗ спека

Одна `StringBuilder` собирает всю строку; каждый `${x}` зовёт `x.@display`/`@debug`, получая её как `Write`;
значения пишут `[]u8` прямо в неё (никаких промежуточных `str`). Финализация `into_str()` — один раз.

Пример `"hello, ${n}, ${f}, ${rec}, ${tup}, ${b}, ${sum} ${gen}"`:
```
sb = StringBuilder.new(cap: estimate)
sb.write("hello, ")
int_display(42, sb)          // ${n}   компилятор ЗНАЕТ sink=конкретный StringBuilder →
                             //        digit-loop прямо в sb.reserve/advance (КОНКРЕТНЫЕ методы SB, zero-COPY)
sb.write(", ")
f64_display(3.14, sb)        // ${f}   f64_fmt_into(buf,cap,v,Shortest) [C] → sb.write(buf)
sb.write(", ")
Point_display(rec, sb)       // ${rec} компилятор-синтез: "Point(", int_display(@x,sb), ", ", …, ")"
sb.write(", ")
Tup3_display(tup, sb)        // ${tup} компилятор-синтез по арности: "(", elem_display…, ")"
sb.write(", ")
bool_display(true, sb)       // ${b}   sb.write(if @ {"true"} else {"false"})
sb.write(", ")
Option_display(sum, sb)      // ${sum} компилятор-синтез: match вариантов + payload
sb.write(" ")
Vec_display[int](gen, sb)    // ${gen} ЧИСТЫЙ nv-дженерик fn[T Display] []T @display: "[", elem, ", ", "]"
result = sb.into_str()
```
Вложенные типы рекурсят в ТУ ЖЕ sb (глубина любая, буфер один).

**Два пути записи примитива (ревью 2026-07-15, после выноса reserve/advance из протокола):**
- **top-level компилятор-путь** (`int_display(42, sb)` где `sb` — КОНКРЕТНЫЙ `StringBuilder`): компилятор эмитит
  digit-loop прямо в `sb.reserve()`/`sb.advance()` (конкретные методы SB) → **zero-COPY**.
- **generic-путь** (примитив внутри пользовательского `@display(mut f Fmt)` или внутри композита, где sink виден
  как `Fmt`/`Write`): рендер в **стек-буфер** + `f.write(buf[0..k])` → **zero-ALLOC** (без кучи), один memcpy
  стек→sink. Как в Rust: `fmt::Write` минимален, примитивы форматируются в стек и `write`-ятся.
Оба — без heap-аллокаций; top-level ещё и без копии. reserve/advance НЕ в протоколе — только конкретный SB.

## 4. Алгоритм — СО спеком (pad_in_place)

Со спеком простого `advance` не хватает: pad зависит от длины отрендеренного тела, известной только ПОСЛЕ рендера.
Разветвление по «известна ли длина заранее»:
- **известная длина** (int/float/bool/char/str) → рендер в **стек-буфер** (примитивы) или прямой расчёт (str),
  потом `write_padded` (fill+тело по align) в главный sb — **без сдвига**;
- **стриминговый композит** (record/tuple/Vec/sum) + `width` → рендер в главный sb от `mark`, `len−mark` = длина,
  потом **pad_in_place** (right/center — memmove тела + вставка fill; left — только `fill` в конце).

Пример `"| ${name:>8} | ${x:04} | ${f:8.2} | ${rec:>20} | ${p:#?} |"`
(`name="hi"`, `x=42`, `f=3.14159`, `rec=Point{1,2}`, `p=Point{1,2}`):
```
sb = StringBuilder.new(cap: estimate)
sb.write("| ")

// ${name:>8} — str, известная длина → pad ВОКРУГ записи, без сдвига
cols = char_cols("hi")                 // 2
sb.fill(' ', 8 - cols)                 // align Right: сначала 6 пробелов
sb.write("hi")                         // → "      hi"
sb.write(" | ")

// ${x:04} — int-примитив, спек в один проход (рендер знает свою ширину)
buf[24]; k = int_fmt(x, buf, 24, {width:4, zero_pad:true})   // Nova: "0042"
sb.write(buf[0..k])
sb.write(" | ")

// ${f:8.2} — float: тело из C по prec, ширина — через стек, потом write_padded
buf2[32]; k = f64_fmt_into(buf2, 32, f, Fixed, /*prec*/2)   // C: "3.14"  (единственный extern)
write_padded(sb, buf2[0..k], /*w*/8, ' ', Right)             // "    3.14"
sb.write(" | ")

// ${rec:>20} — КОМПОЗИТ + width → ГДЕ «просто advance» НЕ РАБОТАЕТ
mark = sb.len()                        // запомнили старт в ГЛАВНОМ sb
f = FmtCtx{ sink: sb, width: Some(20), align: Right, fill: ' ', alt: false, pad_consumed: false }
Point_display(rec, f)                  // СТРИМИТ "Point(1, 2)" в sb от mark; pad_consumed=false
                                       //   f.write("Point("); int_display(1,f); f.write(", ");
                                       //   int_display(2,f); f.write(")")
if !f.pad_consumed && width_set:
    blen = sb.len() - mark             // 11 — узнали ТОЛЬКО сейчас
    if blen < 20:
        pad_in_place(sb, mark, 20, ' ', Right)   // memmove тела вправо на 9 + 9 пробелов слева
                                                 // → "         Point(1, 2)"
sb.write(" | ")

// ${p:#?} — pretty-debug: alternate, БЕЗ width → стриминг как обычно (pad не нужен)
f2 = FmtCtx{ sink: sb, width: None, alt: true, ... }
Point_debug(p, f2)                     // derive читает f2.alternate()==true → многострочно:
                                       //   "Point {\n    x: 1,\n    y: 2,\n}"
sb.write(" |")
result = sb.into_str()
```
**Вложенные спеки** (`${outer:>20}` где тело содержит `${inner:>5}`): inner-pad происходит во время стрима outer
(от более глубокого mark), outer-pad меряет итог; memmove по офсетам композится — работает.

## 5. Что на nv, что компилятор-синтез, что C

| Что | Где | Почему |
|---|---|---|
| примитивы (int/bool/char + радикс + pad) | **nv** (буфер-примитив на `*u8`) | обход выразим |
| `[]T`/`Vec[T]`/slice/Option/Result | **nv** — дженерик `fn[T Display] …` | обход элементов выразим |
| пользовательские record/sum | **компилятор** (auto-derive ОБОИХ) | нужна рефлексия полей/вариантов; синтезирует как вызовы nv-рантайма + `@display`/`@debug` компонентов |
| tuple `(a,b,c)` | **компилятор** (по арности) | вариадик-арность (Rust тоже макросом) |
| **float-body** (shortest + fixed/sci) | **C-extern** | dtoa непортируем — ЕДИНСТВЕННЫЙ C-кусок |

**★ Решение (владелец 2026-07-15): derived-Display и derived-Debug РАЗЛИЧАЮТСЯ формой** (вариант A + различие,
чтобы `${x}` и `${x:?}` не давали тождественный вывод):
- **derived `@debug`** (`{:?}`) — технический дамп С ИМЕНАМИ полей: `Point { x: 1, y: 2 }`; sum:
  `Some(5)` / `Err(IoError { kind: NotFound, ... })`; pretty (`:#?`) — многострочно (через `f.alternate()`).
- **derived `@display`** (`{}`) — компактная «значенческая» форма БЕЗ имён полей: `Point(1, 2)`; sum:
  `Some(5)` (payload как значение). Кастомный `@display` перекрывает.
- Примитивы: у них Display и Debug совпадают (`42`, `true`); различие — только для структурных.
- Rust-контраст: Rust авто-Display НЕ делает (только Debug); мы делаем оба ради эргономики, но различаем формой,
  чтобы Display оставался «вывод-как-значение», а Debug — «вывод-для-разработчика».

## 6. D-план

**НОВЫЙ:**
- **D422 (keystone)** — Unified Formatter: единый `@display(mut f Fmt)`; `Fmt` embeds `Write` через `use` (D145 protocol-embed) + оси/`@pad`/`@kind`;
  `pad_consumed` auto-pad; буфер-примитив `(buf,cap)->len`; перенос `conv.h`→`.nv`; float-extern-контракт
  (`extern "C" fn f64_fmt_into(buf,cap,v,kind,prec)` — литеральное имя, D282); энумы `Align`/`Sign`/`FmtKind`.

**АМЕНДИТЬ:**
- **D55** (literal-coercion) — аменд: str-**литерал** `"..."` коэрсится в `[]u8` в ЛЮБОЙ `[]u8`-позиции (не только
  `@write` — **общее правило**, владелец 2026-07-15). То же семейство, что int-литерал→newtype (D55): «литерал в
  типизированной позиции». Узаконивает эту неявную литеральную коэрсию (Nova без implicit-coercion — это carve-out
  на ЛИТЕРАЛАХ, как для newtype-конструкции; str-значение → всё равно явный `.bytes()`, D176). str = UTF-8-байты →
  коэрсия zero-copy.
- **D419** (`Fmt` для `@display_fmt`) — **RETRACT/SUPERSEDED**.
- **D374** (Write-sink) — аменд ×2: sink Display/Debug = `Fmt`; `Write.@write` → `[]u8`.
- **D237** (Display/Debug) — аменд: сигнатуры `@display`/`@debug` → `(mut f Fmt)`.
- **D229** (Debug + format-spec) — аменд: диспатч спека через `@display(f)`/`@debug(f)`; радикс через `f.kind()`.
- **D179** (`StringBuilder`) — аменд: байтовый append + `@reserve`/`@advance`/`@len` + `pad_in_place`/`write_padded`.
- **ретракт `str.from_debug`** вместе с `str.from` (Plan 174.2) — Debug идёт через derive/`@to_str`, как Display.

**buffer-consolidation — РЕШЕНО 2026-07-15 (owner): `StringBuilder` ОСТАЁТСЯ** отдельным типом (не схлопываем в
`Vec[u8]`). Консолидация = только единый **байтовый шейп** `Write` для fmt и io; сами типы не сливаем.
`ReadBuffer`/`WriteBuffer` остаются адаптерами на едином `Read`/`Write`; направления Read/Write раздельны.

## 7. Миграция / гейты (набросок)

Миграция: ~10 примитивных `@display`/`@debug` тел; немногие `@display_fmt`-юзеры; переписать `emit_interpolated_str`
+ `emit_format_spec_value` (pad_in_place); `conv.h` int/bool/char/радикс/pad → `.nv`; `Write.@write` str→[]u8 (+ str-
overload); `f64_fmt_into` C-контракт. Гейты: полный conformance один-CU зелёный; форматные фикстуры (width/align/fill/
sign/radix/precision/alternate/pretty) pos; byte-parity НЕ требуется (вывод тот же, .c меняется законно).

## 8. Статус развилок

**Все дизайн-развилки закрыты 2026-07-15** (owner-подтверждение):
1. ✅ **Fallibility** — fmt-`Write` инфаллибельно (`-> ()`), io-`Write` — `Result[(), IoError]`; два байтовых
   родственника, не эффект (`Fail` запрещён конвенцией 177 для std). См. таблицу §1.
2. ✅ **`into_str()`** — assume-valid-UTF-8 + checked-вариант.
3. ✅ **buffer-consolidation** (§6) — `StringBuilder` ОСТАЁТСЯ (owner); консолидация = только единый байтовый
   `Write`-шейп, типы не схлопываем.

4. ✅ **reserve/advance в протоколе** (ревью 2026-07-15) — УБРАНЫ из `Write` (Rust-путь: минимальный протокол,
   zero-copy через SB-конкретику, zero-alloc через stack-buf); совместимо с io.Write. См. §9.
5. ✅ **@display/@debug required + цикл** (ревью 2026-07-15) — required-примитив, `@to_str` bounded Display,
   auto-derive структур, опак=compile-error. Цикл структурно невозможен. См. §9 инвариант.
6. ✅ **derived Display vs Debug** (владелец 2026-07-15, ПОДТВЕРЖДЕНО) — auto-derive **ОБОИХ**, но РАЗЛИЧАЮТСЯ формой:
   Debug=`Point { x: 1, y: 2 }` (с именами), Display=`Point(1, 2)` (компактно). См. §5. **Намеренный отход от Rust:**
   Rust деривит только `Debug` (Display — ручной, намеренно); мы деривим оба ради эргономики (`${любая_структура}`
   работает без ручного impl). Инвариант №4 (§9) тогда срабатывает только на опак-типах без полей и без `@display`.

**Все вопросы закрыты — дизайн-развилок не осталось.**

---

## 9. Финальные сигнатуры (финализирует наброски §2)

> **NB — ⚠syntax-развилки СВЕРЕНЫ ПО СПЕКЕ 2026-07-15:** (1) `@write(str)`-overload **убран** — str-значение через
> `s.bytes()` (D176), str-**литерал**→`[]u8` через **коэрсию (амендмент D55**, то же семейство «литерал в
> типизированной позиции», где уже int-литерал→newtype; **общее правило** для ЛЮБОЙ `[]u8`-позиции, не только
> `@write`). (2) **`use Write` — ВЕРНО** (protocol-embed, **D145**): `use TypeName` внутри `protocol {}` = композиция
> протокола (парсер `parse_protocol_body` подтверждает: leading `use`, comma-list `use Reader, Writer`). ОТДЕЛЬНЫЙ
> механизм от record-`use` (field-delegation) — одно ключевое слово, контекст (protocol-body vs record-body) различает
> семантику. Существующий `Fmt` (protocols.nv:241) пока ПОВТОРЯЕТ `@write` (до-D145 стиль); Фаза 2 → `use Write`.
> (3) extern-C = `extern "C" fn` + **литеральное имя без
> `nova_`** — ВЕРНО (`08-runtime.md#d282`, эталон nova-tls). **NB:** `Write`/`Fmt` УЖЕ существуют (protocols.nv:206/241),
> сейчас `@write(s str)` — Фаза 2 амендит на `[]u8` (D374-аменд + миграция юзеров на `.bytes()`/литерал-коэрсию).

**Энумы (D406):**
```nova
type Align   enum Left | Right | Center
type Sign    enum Minus | Plus
type FmtKind enum Display | Debug | Hex | Oct | Bin | Exp
type FloatKind enum Shortest | Fixed | Sci        // для f64_fmt_into
```

**`Write` — байтовый sink форматирования (ИНФАЛЛИБЕЛЬНЫЙ). МИНИМАЛЬНЫЙ (ревью 2026-07-15):**
```nova
export type Write protocol {
    mut @write(bytes []u8) -> ()      // параметры ro по умолчанию → уже read-only (explicit `ro` только на возвратах)
}
```
**Ревью 2026-07-15: `@write(str)`-overload УБРАН.** str пишется: переменная → `w.write(s.bytes())` (D176, zero-copy
view, даром); литерал `w.write("Point(")` → **коэрсия str-литерала → `[]u8`** (амендмент **D55**, общее правило)
компилятором (str и есть UTF-8-байты; чище protocol-overload'а). Протокол остаётся чисто `@write([]u8)`. (Убирает прежний ⚠syntax
«protocol-default для перегрузки».)

**Ревью-решение (владелец 2026-07-15): `@reserve`/`@advance` УБРАНЫ из протокола `Write`** (Rust-путь). Причины:
(1) `@reserve -> *mut u8` реализуем только буфером — стриминговый/io-sink не даёт указатель на будущие байты, т.е.
это разъезжается с io-`Write`-родственником; (2) сырой `*mut u8` не должен течь в общий протокол. **Zero-copy
сохранён иначе:** `@reserve`/`@advance` остаются КОНКРЕТНЫМИ методами `StringBuilder` (§«StringBuilder аменд»), и
**компилятор**, зная что sink — конкретно `StringBuilder`, использует их напрямую для примитивов (digit-loop в
spare-capacity). Пользовательские generic-`@display` (над `Fmt`/`Write`) пишут через `@write(slice)`: примитив
рендерится в **стек-буфер** (zero-**alloc**, без кучи) + один memcpy стек→sink. Итог: zero-COPY на горячем
компилятор-пути (sink=SB), zero-ALLOC для generic-методов — как в Rust (`fmt::Write` минимален).

(io-`Write` из Plan 176 — ОТДЕЛЬНЫЙ протокол с `@write([]u8) -> Result[(), IoError]`; тот же байтовый шейп, другая
сигнатура — «два родственника»; теперь обе минимальны = честно общий шейп.)

**`Fmt` — sink (`use Write`, protocol-embed D145) + оси спека** (сверено 2026-07-15: `use TypeName` в `protocol {}`
= композиция протокола, parse_protocol_body подтверждает):
```nova
export type Fmt protocol {
    use Write                        // protocol-embed (D145) — компонует @write([]u8) из Write
    @width()     -> Option[int]
    @precision() -> Option[int]
    @align()     -> Option[Align]
    @fill()      -> char
    @sign()      -> Sign
    @alternate() -> bool
    @kind()      -> FmtKind
    mut @pad(bytes []u8) -> ()          // тип-управляемый паддинг → ставит pad_consumed
}
```

**`Display` / `Debug` — ОДИН метод каждый, инфаллибельный, `@display`/`@debug` = REQUIRED-примитив
(ревью 2026-07-15 — убран to_str-зовущий дефолт во избежание цикла):**
```nova
export type Display protocol { @display(mut f Fmt) -> () }   // REQUIRED — без дефолта (не зовёт to_str!)
export type Debug   protocol { @debug(mut f Fmt) -> () }     // REQUIRED
```
**★ Инвариант «нет циклической ловушки» (легализация 2026-07-15):**
1. `@display`/`@debug` — **required-примитив**, дефолта, зовущего `@to_str`, НЕТ.
2. `@to_str` — бланкет **bounded `Display`** (`fn[T Display] T @to_str()`): вызываем только на типе, который
   *уже* Display (уже имеет реальный `@display`) → зовёт настоящий примитив, не себя. Цикл невозможен.
3. **Auto-derive** структурных типов (record/sum/tuple, §5/§159): компилятор синтезирует реальный
   `@display`/`@debug` по требованию → структурный тип Display-способен без ручного impl.
4. Тип без `@display` и не-деривируемый (опак без полей) → **НЕ Display** → `${x}` = **compile-error
   «type X не реализует Display»**, а не рекурсия. Худший случай — понятная ошибка сборки, не бесконечный цикл.

**`FmtCtx` — конкретный реализатор `Fmt`, строит компилятор на каждом `${x:SPEC}`:**
```nova
export type FmtCtx {
    sink Write            // главный SB (или под-регион при pad_in_place)
    mark int              // старт тела в SB (для pad_in_place)
    spec FormatSpec       // width/align/fill/sign/alternate/precision/kind
    mut pad_consumed  bool
    mut prec_consumed bool
}
// @write → sink.@write; @width → spec.width; …; @pad(bytes) → write_padded в sink + pad_consumed=true
```

**Буфер-примитивы — ВНУТРЕННИЕ (.nv, не публичные), zero-alloc:**
```nova
fn int_fmt(v int, buf *mut u8, cap int, spec FmtSpec) -> int      // digit-loop + радикс + zero_pad; вернуть len
fn bool_fmt(v bool, buf *mut u8, cap int) -> int
fn char_fmt(v char, buf *mut u8, cap int) -> int                  // UTF-8 encode
// float — ЕДИНСТВЕННЫЙ C-extern (dtoa непортируем). D282: extern "C" fn + ЛИТЕРАЛЬНОЕ имя, БЕЗ nova_-префикса:
extern "C" fn f64_fmt_into(buf *mut u8, cap int, v f64, kind int, prec int) -> int
// FloatKind пересекает C-ABI как int (0=Shortest/1=Fixed/2=Sci); .nv-wrapper конвертит enum→int. C-имя = литеральное.
```

**`StringBuilder` аменд (D179):**
```nova
fn StringBuilder mut @reserve(n int) -> *mut u8
fn StringBuilder mut @advance(n int) -> ()
fn StringBuilder @len() -> int
fn StringBuilder mut @pad_in_place(mark int, width int, fill char, align Align) -> ()   // memmove + fill (right/center)
fn StringBuilder mut @write_padded(bytes []u8, width int, fill char, align Align) -> ()
fn StringBuilder consume @into_str() -> str                         // assume-valid UTF-8
fn StringBuilder consume @into_str_checked() -> Result[str, Utf8Error]
```

**Бланкет convenience:** `export fn[T Display] T @to_str() -> str` = собрать в свежий SB через `@display`, `into_str()`.

## 10. Карта исполнения (фазы · модели · гейты)

> Модели по [feedback-cheap-models]: **opus** — спека + компилятор-синтез/переписка emit_c (архитектура); **sonnet** —
> исполнение по карте (.nv-протоколы/дженерики); **haiku** — механическая зачистка. Каждая фаза = свой worktree,
> суб-агентов не спавнить, checkpoint+resumeFromRunId, греп маркеров с коммитом.

**Фаза 0 — Спека (opus).** D422 (keystone) + амендменты D419(retract)/D374/D237/D229/D179 + ретракт `str.from_debug`
в `spec/decisions/`. **Гейт:** owner sign-off (язык-меняющее).

**Фаза 1 — Фундамент, АДДИТИВНО без смены поведения (sonnet .nv + 1 C-файл).** Буфер-примитивы в .nv
(`int_fmt`/`bool_fmt`/`char_fmt` + радикс + `pad_in_place`/`write_padded`) РЯДОМ с conv.h; `f64_fmt_into` (C-extern
буфер-форма) рядом с текущим float; `StringBuilder` аменд (`@reserve`/`@advance`/`@len`/`into_str`). Старый путь ещё
работает. **Гейт:** unit-тесты примитивов + полный conformance БЕЗ регресса.

**Фаза 2 — КОГЕРЕНТНАЯ ВОЛНА: протоколы + переписка компилятора (opus компилятор + sonnet .nv).** Самая рискованная
(big-bang, всё вместе, т.к. смена сигнатур взаимозависима):
- std: `Write`([]u8, инфаллибельно) + `Fmt`(оси) + `FmtCtx` + энумы; `Display`/`Debug` → `(mut f Fmt) -> ()`;
- компилятор: переписать `emit_interpolated_str` (~39556) + `emit_format_spec_value` (~39944) на единый `@display(f)`/
  `@debug(f)` + `pad_in_place`; **удалить `@display_fmt`-путь** (~40125); `Write.@write` str→[]u8; примитивы →
  буфер-примитивы Фазы 1;
- ретракт `str.from_debug`.
**Гейт:** полный conformance один-CU зелёный; формат-фикстуры (см. §11); D-амендменты в ТОМ ЖЕ слиянии; byte-parity
НЕ требуется. **Дробить осторожно, checkpoint.**

**Фаза 3 — Дженерики .nv + auto-derive (sonnet .nv + opus компилятор-синтез).** `[]T`/`Vec[T]`/`Option`/`Result`
Display/Debug — дженерик-импл в .nv; компилятор auto-derive record/sum/tuple → через `@display(f)`/`@debug(f)`, pretty
через `f.alternate()`. **Гейт:** derive-фикстуры pos, pretty pos.

**Фаза 4 — Зачистка (haiku/sonnet).** Оставшийся conv.h int/bool/char/радикс/pad → .nv; удалить мёртвый `nova_fmt_*`.
**Гейт:** conformance; C-поверхность = ТОЛЬКО float-body.

## 10R. Ф.4R — редизайн сноса conv.h: «семантика живёт в fmt_buf, все пути — вызовы» (владелец 2026-07-20)

### §10R-Д — три нормы-дополнения (владелец 2026-07-21; переданы исполнителю брифом, здесь — source of truth)

**Д1. Порядок аргументов: value-first ВЕЗДЕ**, включая extern-границу (`(v, buf, cap, оси…)`);
buf-first (`f64_fmt_into(buf, cap, v, …)`) — просочившаяся snprintf-идиома C, оба конца границы
наши. Перестановка: nova_rt.h тела + extern-деклы + все call-сайты.

**Д2. Имена: type-first** (канон семьи `int_fmt`/`bool_fmt`/`char_fmt`, решение владельца
`29974ab36`): `fmt_f64` → `f64_fmt`. Греп на прочих глагол-first отставших.

**Д3. Суффикс `_into` УПРАЗДНЯЕТСЯ** — он значил две разные вещи (C-extern'ы И экспорт-мосты),
при том что ВСЯ семья пишет в `(buf, cap)`. Норма: «тип_fmt + оси с default-аргами»:
```nova
export unsafe fn int_fmt(v int, buf *mut u8, cap int, spec FmtSpec = FmtSpec.new()) -> int  requires cap >= 0
export unsafe fn f64_fmt(v f64, buf *mut u8, cap int, kind FloatKind = FloatKind.Shortest, prec int = -1) -> int  requires cap >= 0
export unsafe fn f32_fmt(v f32, buf *mut u8, cap int) -> int  requires cap >= 0
```
Мосты `int_fmt_into`/`*_fmt_shortest_into` упраздняются как сущности (простой рендер = та же
функция с дефолтами; call-сайты SB/display_spec/тесты — прямые вызовы); C-extern'ы →
`nova_f64_fmt`/`nova_f32_fmt` (стиль заголовка, D282 литеральные имена); шапка fmt_buf фиксирует
норму семьи. **Проба до Д3-п.1:** default-арги у FREE fn через callnorm-backfill (Пункт-1/200
доказал статики, free-форма — нет); дыра → НЕ перегрузки-по-арности и НЕ попутный callnorm-фикс:
fallback = обёртки остаются с именами без `_into` (`f64_fmt_shortest`) + маркер
`[M-freefn-default-arg-backfill-gap]` с probe-репро.

**Гейты Д1-Д3:** эталоны Ш0 байт-в-байт; string_builder_test 1/0; checksums 3/0; греп `_into`
по std/src + nova_rt.h = 0 (или честный fallback-остаток).

> NB: примеры в §4-§6 (`f64_fmt_into(buf,cap,…)` и т.п.) — доредизайновые; канон сигнатур —
> ЭТА секция. Примеры переписываются волной Ш4 вместе с D422-амендментом.


**Мотив (диагноз владельца: «два источника семантики — неверный путь; нужен один, по максимуму в .nv»).**
Аудит 2026-07-20 показал хуже, чем «два пути»: примитивные `@display`-тела — ЦИРКУЛЯРНЫЕ заглушки
(`fn int @display(f) { f.write("${@}".bytes()) }` — зовут интерполяцию, т.е. компиляторный
fast-path + аллокация на `.bytes()`), живая семантика — на 100% в emit_c/conv.h
(`nova_fmt_f64_prefix/_body`, `nova_fmt_int_body`, `nova_fmt_pad`; emit_c.rs ~41563-41600),
а .nv-движок `int_fmt` (fmt_buf, спек-полный) МЁРТВ — подключены только SB-мосты.
Нарушения: compiler-conventions §0/§10 (параллельные пути = источник багов; родня уже
стрелявшего `[M-208-generic-interp-display-dispatch-gap]`), §3 (рендер-семантика захардкожена
в Rust-эмиттере), rustc-эталон (у Rust ОДИН путь Display::fmt, скорость — девиртуализацией).

**Целевая норма.** ЕДИНСТВЕННЫЙ носитель рендер-семантики примитива — .nv-функции в `fmt_buf`:
`int_fmt` (есть) · `fmt_f64` (обвязка единственного легального extern-исключения) · НОВОЕ:
Debug-escape str/char (портируемый цикл, .nv) · поверх — семейство
**`*_display_spec(mut sb StringBuilder, v, width int, prec int, align, fill, флаги...)`**
с ПЛОСКИМИ аргументами (прецедент плоскости — `FmtCtx.rich`). Три потребителя, ВСЕ — вызовы:
1. **компиляторный fast-path** (литеральный спек): эмитит вызов `*_display_spec` ПО РЕЗОЛВУ
   декларации (не C-строкой имени — §3), минуя FmtCtx-объект = легальная девиртуализация,
   собственной семантики НОЛЬ;
2. **протокольный путь**: примитивные `@display`/`@debug`-тела ПЕРЕЕЗЖАЮТ в fmt_buf
   (extension-методы, D267) и зовут те же `*_display_spec`, распаковывая оси из `f` —
   закрывает Находку Б (privacy цела, экспорт не расширяется), убивает циркулярную заглушку
   и её аллокацию;
3. **SB-аппенды** — уже на unsafe-мостах (частный случай без спека).
`conv.h` `nova_fmt_*` (16 сайтов) сносится; `nova_fmt_pad` → существующие .nv
`pad_in_place`/`write_padded`.

**Шаги (big-bang распадается: сосуществование + атомарный флип с kill-switch,
приём feedback-codegen-dce-verification):**
- **Ш1 (std, чисто аддитивно; sonnet):** Debug-escape движок (закрывает Находку А) +
  `*_display_spec`-семейство поверх int_fmt/fmt_f64 + **эталон-фикстуры**, байт-тексты
  сняты с ТЕКУЩЕГО вывода (фиксация поведения ДО флипа): int радиксы/знак/zero-pad границы
  (MIN/MAX/-1 hex), f64 `.N`-precision + width (`"${-12.345:010.2}"`-семья), str/char
  Debug-escape, bool/char.
- **Ш2 (std; sonnet):** переезд примитивных `@display`/`@debug`-тел из prelude/protocols.nv
  в fmt_buf-extension → протокольный путь становится .nv-истинным и zero-alloc.
- **Ш3 (компилятор, АТОМ; sonnet по карте):** fast-path эмитит вызовы `*_display_spec`
  вместо conv.h-цепочки; kill-switch `NOVA_FMT_LEGACY=1` (старая эмиссия) для
  байт-дифф-верификации на корпусе НА ОДНОМ бинаре. Гейт: эталоны Ш1 + байт-паритет
  выборки + полный CU + флагман strict.
- **Ш4:** снос conv.h `nova_fmt_*` + kill-switch после зелёного CI; D422 «Статус
  реализации» обновляется, V1-упрощения #1/#3 закрываются.

**Инвариант-приёмка (НОРМАТИВНО, в план навсегда):** interp-fast-path НЕ имеет собственной
рендер-семантики — только прямые вызовы тех же .nv-примитивов, что зовёт `@display`;
отдельная реализация = §0/§10-нарушение. Равенство путей пинуется фикстурами
(`"${x:спек}" == ручной FmtCtx.rich + display` на граничных значениях).

**Риски/связи:** байт-паритет %g-хвостов и радикс-регистров — главный (гейт: эталоны Ш1 +
kill-switch-дифф); `[M-imports-entry-folder-module-self-cycle-empty-exports]` мешает только
fmt_buf-as-entry прогонам (тесты гоняются peer-CU; фикс — отдельная волна, не блокер);
перф — display_spec пишет в тот же SB без FmtCtx-объекта, сопоставимо. Приёмка/флип-гейт —
интегратор.

## 11. Гейты и риски

**Гейты (каждая волна):** полный `spec_tests/conformance` один-CU зелёный; НОВЫЕ формат-фикстуры pos —
width/align/fill/sign/`#`alt/`0`zero-pad/radix(hex/oct/bin)/precision(float+str)/`:?`debug/`:#?`pretty + вложенные
спеки; byte-parity НЕ требуется (вывод тот же, `.c` меняется законно), но тесты зелёные.

**Риски / координация:**
- **Plan 176** — io-`Write` байтовый: координация (общий байтовый шейп, направления Read/Write раздельны, `StringBuilder`
  остаётся);
- **Plan 196** — НЕ трогать замороженную зону `infer_call_ret_c` (46293–48883); interp/format-codegen ВНЕ её
  (~2428 / 39xxx / 40xxx) — безопасно;
- **язык-меняющее** → D-амендмент в том же слиянии (Фаза 0 спека → Фаза 2 код ссылается);
- **Фаза 2 = big-bang** (сигнатуры взаимозависимы) — главный риск; митигигация: Фаза 1 аддитивна (примитивы готовы
  заранее), Фаза 2 дробить по под-шагам с checkpoint, полный conformance после каждого под-шага.
- **Стоимость гейтов Фазы 2 (заложить в сроки, ревью 2026-07-15):** смена сигнатур рипплит по корпусу, а опыт
  показал — merged-CU-регрессии ловятся ТОЛЬКО полным conformance-гейтом (не таргетным). Значит КАЖДЫЙ под-шаг Фазы 2
  оркестратор гейтит полным conformance САМ (агенты — только таргетно). Это много ~7-мин серийных гейтов оркестратора —
  Фаза 2 по календарю дольше прочих; это норма, не задержка.

**Следующий шаг:** финализировать сигнатуры (`Fmt`/`Write`/`Display`/`Debug`/буфер-примитив/`f64_fmt_into`) +
карта исполнения (что opus-синтез в компиляторе, что дешёвыми агентами по `.nv`/`conv.h`→`.nv`; порядок; гейты).
**Реализация не начинается без owner-go** (язык-меняющее).
