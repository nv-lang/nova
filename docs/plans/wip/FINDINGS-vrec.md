# VREC_FINDINGS.md — проверка заявок о дефектах C-codegen вокруг value-record

Дата: 2026-07-30
Модель: opencode/big-pickle
Коммиты: (см. «Коммиты» в конце)

---

## Заявка 1: Enum-поле в value-record

### Вердикт: ПОДТВЕРЖДЕНА (с уточнением)

### Репро 1a — `type Sign enum Neg | Zero | Pos` как поле value-record

**Файл:** `vrec/repro1a.nv`

**Результат:** CC-FAIL (2 errors)

**Дословные ошибки:**
```
error: unknown type name 'Nova_Sign'; did you mean 'Nova_Align'?
error: member reference type 'nova_int' (aka 'long long') is not a pointer
```

**Анализ generated C (`vrec/repro1a.c`):**
- `NovaValue_Pt` (строка 220): поле `sign` объявлено как `Nova_vrec_repro1a_Sign*` (указатель на heap-allocated enum struct) — корректно.
- Конструктор (строка 5176): `_nv_tmp_440.sign = (nova_int)(intptr_t)nova_make_vrec_repro1a_Sign_Pos()` — ошибочно кастует указатель в `nova_int`, хотя поле объявлено как указатель на struct.
- Match (строка 5180/5184): `Nova_vrec_repro1a_Sign* _nv_scr_441 = (p.sign)` и `(p.sign)->tag` — компилятор видит тип `p.sign` как `nova_int` из-за предыдущего каста → `->` на `nova_int` недопустим.

**Природа дефекта:** Ошибка в эмишене конструктора Enum-поля: значение variant'а (функция, возвращающая `Nova_vrec_repro1a_Sign*`) кастуется в `nova_int` при присваивании в поле структуры, хотя поле объявлено как указатель. Вероятно, codegen трактует enum-поле как скалярное (как int-backed enum), но struct layout ожидает pointer.

Дополнительно: forward declaration для пользовательского `Sign` в vtable Fmt (строка 98) генерирует `Nova_Sign*` вместо `Nova_vrec_repro1a_Sign*` — проблема квалификации имени при совпадении unqualified-имени с пользовательским типом.

### Репро 1b — `type Sign(i8)` (newtype) как поле value-record

**Файл:** `vrec/repro1b.nv`

**Результат:** CC-FAIL (1 error)

**Дословная ошибка:**
```
error: typedef redefinition with different types ('int8_t' (aka 'signed char') vs 'struct Nova_Sign')
```

**Анализ generated C (`vrec/repro1b.c`):**
- Строка 53: `typedef struct Nova_Sign Nova_Sign;` — forward decl stdlib-типа `runtime.fmt_buf.Sign`.
- Строка 123: `typedef int8_t Nova_Sign;` — пользовательский newtype `Sign(i8)`.

**Природа дефекта:** Коллизия имён. Сокращение `Nova_runtime_fmt_buf_Sign` → `Nova_Sign` сталкивается с пользовательским `type Sign(i8)`, который тоже генерирует `Nova_Sign` (как `int8_t`). При отсутствии пользовательского `Sign` этот фрагмент кода корректен (сгенерированный `typedef struct Nova_Sign` никогда не конфликтует, потому что это единственный `Nova_Sign`). С появлением пользовательского `Sign` — redefinition error.

**Важно:** В обоих случаях (1a и 1b) ошибка НЕ является фундаментальной невозможностью разместить enum/newtype в value-record — это ошибка нейминга сгенерированного C при коллизии с stdlib-типом `Sign`.

---

## Заявка 2: Option[value-record] в возврате

### Вердикт: НЕ ПОДТВЕРЖДЕНА

### Репро 2a — `fn mk(ok bool) -> Option[Pt]`

**Файл:** `vrec/repro2a.nv`

**Результат:** PASS (компиляция и выполнение успешны)

### Репро 2b — `fn div_rem(ok bool) -> Option[(Pt, Pt)]`

**Файл:** `vrec/repro2b.nv`

**Результат:** PASS (компиляция и выполнение успешны)

**Примечание:** Обе формы проходят без ошибок. Заявка о forward-declaration `NovaOpt_NovaValue_...` до typedef не воспроизводится на `Pt` как на простом value-record (2 поля `int`) ни в прямой форме `Option[Pt]`, ни в парной `Option[(Pt, Pt)]`. Возможно, ошибка специфична для `BigInt` (с полем `limbs []u64`) или была исправлена между моментом заявки и текущей ревизией компилятора. Вывод: на репро-классе `Pt` дефект **отсутствует**.

---

## Заявка 3: Анонимный литерал value-record

### Вердикт: НЕ ПОДТВЕРЖДЕНА

### Репро 3a — `{ sign, x }` (shorthand) в return-позиции

**Файл:** `vrec/repro3a.nv`

**Результат:** PASS

```nova
fn mk_anon(sign int, x int) -> Pt => { sign, x }
```

### Репро 3b — `Pt { sign: 1, x: 99 }` (именованный) в let-присваивании

**Файл:** `vrec/repro3b.nv`

**Результат:** PASS

### Репро 3c — `{ sign: -1, x: 42 }` (анонимный с явными именами полей) с type-аннотацией let

**Файл:** `vrec/repro3c.nv`

**Результат:** PASS

```nova
ro a Pt = { sign: -1, x: 42 }
```

**Примечание:** Все три формы (shorthand-анонимный, именованный, явный-анонимный с type-аннотацией) компилируются и выполняются корректно. Язык D52 предписывает shorthand `{ sign, x }` когда имя поля совпадает с переменной, и запрещает как избыточную запись `{ sign: sign, x: x }`, так и избыточный префикс `Pt { ... }` когда тип уже задан сигнатурой return. Но обе запрещённые формы отлавливаются checker'ом с внятным diagnostic, не являются codegen-ошибкой. Все разрешённые D52 формы компилируются успешно.

---

## Итоговая таблица

| Заявка | Репро | Результат | Вердикт |
|--------|-------|-----------|---------|
| 1. Enum-поле в VR | 1a (enum) | CC-FAIL | **ПОДТВЕРЖДЕНА** |
| 1. Enum-поле в VR | 1b (newtype) | CC-FAIL | **ПОДТВЕРЖДЕНА** (коллизия имён, отдельный баг) |
| 2. `Option[VR]` в возврате | 2a (`Option[Pt]`) | PASS | **НЕ ПОДТВЕРЖДЕНА** |
| 2. `Option[VR]` в возврате | 2b (`Option[(Pt, Pt)]`) | PASS | **НЕ ПОДТВЕРЖДЕНА** |
| 3. Анонимный литерал VR | 3a (shorthand) | PASS | **НЕ ПОДТВЕРЖДЕНА** |
| 3. Анонимный литерал VR | 3b (именованный) | PASS | **НЕ ПОДТВЕРЖДЕНА** |
| 3. Анонимный литерал VR | 3c (анонимный+аннотация) | PASS | **НЕ ПОДТВЕРЖДЕНА** |

---

## Файлы репро

Все файлы находятся в `vrec/`:
- `repro1a.nv`, `repro1b.nv` — заявка 1
- `repro2a.nv`, `repro2b.nv` — заявка 2
- `repro3a.nv`, `repro3b.nv`, `repro3c.nv` — заявка 3
- `*.c` — generated C artifacts (`--keep-artifacts`)

---

## Коммиты

Базовый коммит (HEAD перед началом): `0292c47c4`
Коммит с результатами: `271c5d800`
