<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 208 — Unified Formatter (`@display(mut f Fmt)`, байтовый `Write`, zero-alloc)

**Статус:** ✅ ДИЗАЙН ФИНАЛИЗИРОВАН 2026-07-15 (все развилки закрыты; финальные сигнатуры §9 + карта исполнения
§10 + гейты/риски §11). **Ждёт owner-go на Фазу 0** (спека D422 + амендменты). Реализация язык-меняющая —
без go не начинается.
**Приоритет:** ниже Plan 196. **Язык-меняющее** → D-амендменты в том же слиянии, что код.
**Родитель-реестр:** [Plan 200 Пункт 8](200-std-improvements.md).

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
| Sink пишет | `[]u8` (байты), `Fmt extends Write` |
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
литерал `w.write("...")` через коэрсию str→`[]u8` (лоуэрится в `@bytes()`, D176 zero-copy), переменная `.bytes()`;
`width` = **кодпоинты** (Rust-парити; графемы/дисплей-колонки — future-ось, у нас уже есть unicode/grapheme-таблицы);
`str.from_debug` — **ретракт** вместе с `str.from`. **Новое (investigation):** buffer-consolidation — см. §6.

## 2. Протоколы sink

```nova
export type Write protocol {         // МИНИМАЛЬНЫЙ: только @write; reserve/advance — на StringBuilder (см. §9)
    mut @write(bytes []u8) -> ()     // параметры ro по умолчанию → уже read-only; str-литерал коэрсится в @bytes() (D176)
    // reserve/advance — КОНКРЕТНО на StringBuilder (не в протоколе); zero-copy = компилятор через SB
}

export type Fmt protocol {           // Fmt = use Write (@write) + оси спека (reserve/advance НЕ в протоколе)
    use Write
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
f64_display(3.14, sb)        // ${f}   fmt_f64_into(buf,cap,v,Shortest) [C] → sb.write(buf)
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
buf2[32]; k = fmt_f64_into(buf2, 32, f, Fixed, /*prec*/2)   // C: "3.14"  (единственный extern)
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
- **D422 (keystone)** — Unified Formatter: единый `@display(mut f Fmt)`; `Fmt` = write-методы + оси/`@pad`/`@kind` (без наследования протокола — см. §9);
  `pad_consumed` auto-pad; буфер-примитив `(buf,cap)->len`; перенос `conv.h`→`.nv`; float-extern-контракт
  (`extern "C" fn fmt_f64_into(buf,cap,v,kind,prec)` — литеральное имя, D282); энумы `Align`/`Sign`/`FmtKind`.

**АМЕНДИТЬ:**
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
overload); `fmt_f64_into` C-контракт. Гейты: полный conformance один-CU зелёный; форматные фикстуры (width/align/fill/
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
6. ✅ **derived Display vs Debug** (владелец 2026-07-15) — auto-derive ОБОИХ, но РАЗЛИЧАЮТСЯ формой:
   Debug=`Point { x: 1, y: 2 }` (с именами), Display=`Point(1, 2)` (компактно). См. §5.

**Все вопросы закрыты — дизайн-развилок не осталось.**

---

## 9. Финальные сигнатуры (финализирует наброски §2)

> **NB — ⚠syntax-развилки РАЗРЕШЕНЫ 2026-07-15 (владелец):** (1) `@write(str)`-overload **убран** — str через
> `s.bytes()` (D176) + коэрсия str-литерала→`[]u8`; (2) `Fmt` компонует `Write` через **`use`** (форму embed сверить
> по `02-types.md`); (3) extern-C = **литеральное имя без `nova_`** (D282, эталон nova-tls). Осталось сверить точные
> формы `use`-embed и `extern "C"` по спеке ПЕРЕД кодом (Фаза 0/1).

**Энумы (D406):**
```nova
type Align   enum Left | Right | Center
type Sign    enum Minus | Plus
type FmtKind enum Display | Debug | Hex | Oct | Bin | Exp
type FloatKind enum Shortest | Fixed | Sci        // для fmt_f64_into
```

**`Write` — байтовый sink форматирования (ИНФАЛЛИБЕЛЬНЫЙ). МИНИМАЛЬНЫЙ (ревью 2026-07-15):**
```nova
export type Write protocol {
    mut @write(bytes []u8) -> ()      // параметры ro по умолчанию → уже read-only (explicit `ro` только на возвратах)
}
```
**Ревью 2026-07-15: `@write(str)`-overload УБРАН.** str пишется: переменная → `w.write(s.bytes())` (D176, zero-copy
view, даром); литерал `w.write("Point(")` → **коэрсия str-литерала → `[]u8`** компилятором (str и есть UTF-8-байты;
мелкая фича, чище protocol-overload'а). Протокол остаётся чисто `@write([]u8)`. (Убирает прежний ⚠syntax
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

**`Fmt` — sink (`use Write`) + оси спека** (композиция протокола через `use` — владелец 2026-07-15; точную форму
embed сверить по `02-types.md`):
```nova
export type Fmt protocol {
    use Write                        // компонует @write([]u8) из Write (⚠syntax: форма use/embed — сверить)
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
extern "C" fn fmt_f64_into(buf *mut u8, cap int, v f64, kind int, prec int) -> int
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
(`int_fmt`/`bool_fmt`/`char_fmt` + радикс + `pad_in_place`/`write_padded`) РЯДОМ с conv.h; `fmt_f64_into` (C-extern
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

**Следующий шаг:** финализировать сигнатуры (`Fmt`/`Write`/`Display`/`Debug`/буфер-примитив/`fmt_f64_into`) +
карта исполнения (что opus-синтез в компиляторе, что дешёвыми агентами по `.nv`/`conv.h`→`.nv`; порядок; гейты).
**Реализация не начинается без owner-go** (язык-меняющее).
