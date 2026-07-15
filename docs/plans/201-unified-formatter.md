<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 201 — Unified Formatter (`@display(mut f Fmt)`, байтовый `Write`, zero-alloc)

**Статус:** 📋 ДИЗАЙН (в разработке, интерактивная выработка с владельцем 2026-07-14/15). НЕ реализация —
сначала полный дизайн-док + D-амендменты, потом исполнение по карте дешёвыми агентами.
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
`Write`↔`io.Write` — **СЛИТЬ** (коорд. 176), направления `Read`/`Write` НЕ сливать; str-форма = **default-метод**
`@write(s str) => @write(s.as_bytes())` (один required байтовый + бесплатный default, не двойной бойлерплейт);
`width` = **кодпоинты** (Rust-парити; графемы/дисплей-колонки — future-ось, у нас уже есть unicode/grapheme-таблицы);
`str.from_debug` — **ретракт** вместе с `str.from`. **Новое (investigation):** buffer-consolidation — см. §6.

## 2. Протоколы sink

```nova
export type Write protocol {
    mut @write(bytes []u8) -> ()
    // zero-copy для примитивов:
    mut @reserve(n int) -> *u8       // гарантировать n свободных, вернуть голову записи
    mut @advance(n int) -> ()        // зафиксировать n записанных байт
    // (рек. str-overload: mut @write(s str) -> () поверх байтового)
}

export type Fmt protocol {           // Fmt extends Write (= @write/@reserve/@advance +)
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

export type Display protocol { @display(mut f Fmt) { f.write(@to_str()) } }
export type Debug   protocol { @debug(mut f Fmt)   { /* derive / @to_str */ } }
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
int_display(42, sb)          // ${n}   digit-loop → spare-capacity sb (reserve/advance), zero-copy
sb.write(", ")
f64_display(3.14, sb)        // ${f}   nova_f64_into(buf,cap,v,Shortest) [C] → sb.write(buf)
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
buf2[32]; k = nova_f64_into(buf2, 32, f, Fixed, /*prec*/2)   // C: "3.14"  (единственный extern)
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
| пользовательские record/sum | **компилятор** (auto-derive) | нужна рефлексия полей/вариантов; синтезирует как вызовы nv-рантайма + `@display`/`@debug` компонентов |
| tuple `(a,b,c)` | **компилятор** (по арности) | вариадик-арность (Rust тоже макросом) |
| **float-body** (shortest + fixed/sci) | **C-extern** | dtoa непортируем — ЕДИНСТВЕННЫЙ C-кусок |

## 6. D-план

**НОВЫЙ:**
- **D422 (keystone)** — Unified Formatter: единый `@display(mut f Fmt)`; `Fmt extends Write` (оси + `@pad`/`@kind`);
  `pad_consumed` auto-pad; буфер-примитив `(buf,cap)->len`; перенос `conv.h`→`.nv`; float-extern-контракт
  (`nova_f64_into(buf,cap,v,kind,prec)`); энумы `Align`/`Sign`/`FmtKind`.

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
overload); `nova_f64_into` C-контракт. Гейты: полный conformance один-CU зелёный; форматные фикстуры (width/align/fill/
sign/radix/precision/alternate/pretty) pos; byte-parity НЕ требуется (вывод тот же, .c меняется законно).

## 8. Статус развилок

**Все дизайн-развилки закрыты 2026-07-15** (owner-подтверждение):
1. ✅ **Fallibility** — fmt-`Write` инфаллибельно (`-> ()`), io-`Write` — `Result[(), IoError]`; два байтовых
   родственника, не эффект (`Fail` запрещён конвенцией 177 для std). См. таблицу §1.
2. ✅ **`into_str()`** — assume-valid-UTF-8 + checked-вариант.
3. ✅ **buffer-consolidation** (§6) — `StringBuilder` ОСТАЁТСЯ (owner); консолидация = только единый байтовый
   `Write`-шейп, типы не схлопываем.

**Все вопросы закрыты — дизайн-развилок не осталось.**

**Следующий шаг:** финализировать сигнатуры (`Fmt`/`Write`/`Display`/`Debug`/буфер-примитив/`nova_f64_into`) +
карта исполнения (что opus-синтез в компиляторе, что дешёвыми агентами по `.nv`/`conv.h`→`.nv`; порядок; гейты).
**Реализация не начинается без owner-go** (язык-меняющее).
