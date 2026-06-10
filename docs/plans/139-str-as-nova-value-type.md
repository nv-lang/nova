<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 139 — `str` as a Nova value type (`{ ptr *ro u8, len int }`)

> **Создан:** 2026-06-10.  **Статус:** 📋 PLANNED (design / decomposition).
> **Sequencing (решено 2026-06-10):** **139-first** — этот план идёт ПЕРЕД 138.2 Ф.2-Ф.4
> и subsumes 138.2 Ф.1 (string-слой). Порядок: 138.2 Ф.0 (universal Vec) → **139 Ф.0-Ф.2** → 138.2 Ф.2-Ф.4.
> **Эстимат:** ~5-8 dev-day (крупнейший single-type refactor; high-risk).
> **Model:** Opus + Thinking ON.
> **Зависит от:** Plan 118 (D216 typed pointers `*ro T`), Plan 131 (Vec on raw ptr),
> Plan 124.8/127 (value records). Пересекается с Plan 138.2 (string-layer).
> **Предложено пользователем:** `type str value priv { ptr *u8; len int }`.

---

## Идея

Сделать `str` **Nova value-типом** вместо C-примитива `nova_str`:

```nova
// lang-item: компилятор знает layout для лоуэринга литералов,
// но методы — Nova-body.
type str value priv {
    ptr *ro u8     // указатель на иммутабельный UTF-8 буфер (read-only)
    len int        // длина в БАЙТАХ (D26: str.len = bytes)
}
```

`value` → stack, 16 байт, copy-семантика (совпадает с текущим nova_str).
`priv` → поля видны только методам str (инкапсуляция).
`*ro u8` (не `*u8`) → данные строки иммутабельны.

**Цель:** и `str`, и `Vec`/`[]T` — полностью на Nova; последний C-примитив-
коллекция (`nova_str`) ретайрится до тонкого ABI-typedef. Униформность с
Plan 138.x (Vec на `*mut T`).

---

## Почему это lang-item, а не обычный тип

`str` используется компилятором с самого начала: литералы `"abc"`, интерполяция
`${...}`, `panic(msg str)`, сообщения ошибок, Display. Компилятор обязан знать
layout `str`, чтобы эмитить литералы → `str` не может быть чисто
пользовательским. Модель — **lang-item** (как Rust `str`/`String`): Nova-объявленный
тип, спец-распознаваемый codegen'ом для лоуэринга литералов, с Nova-body методами.

Прецедент: `Vec`/`Range` уже получают полу-спец-обработку; `never`/`int` —
строчные примитивы. `str` встаёт между ними: объявлен на Nova, известен компилятору.

---

## ABI-стратегия (ключ к ограничению риска)

`nova_str` встречается **~431 раз в compiler-codegen/src** (369 в emit_c.rs) +
**~354 в 22 рантайм-C-файлах** (net.c/effects/channels/sync/vtables/string_builder/
conv/fibers). Переписать всё — неподъёмно. **Решение:** value-record `str`
лоуэрится в C-структуру **layout-идентичную** текущему `nova_str`:

```c
// сейчас:  typedef struct { const char*    ptr; size_t  len; } nova_str;
// станет:  typedef struct { const uint8_t* ptr; int64_t len; } nova_str;  // = str value-record
```

`const char*` ≡ `const uint8_t*` (тот же 8-байт указатель), `size_t` ≡ `int64_t`
на x64. → **354 рантайм-вхождения продолжают работать через `nova_str`-typedef-
алиас** без правок. Работа концентрируется в:
- **emit_c.rs** — лоуэринг литералов, type-mapping `str`→`nova_str`, роутинг методов
  на Nova-body вместо external (часть из 369, но не все — большинство это просто
  `nova_str` C-имя, которое остаётся).
- **std/runtime/string.nv** — `str` становится value-record + Nova-body методы.

---

## Фазы

### Ф.0 — `str` lang-item value-record + литералы (GATE) (~1-2d)

- Объявить `type str value priv { ptr *ro u8, len int }` (в core-модуле, доступном
  очень рано — bootstrap-safe, как prelude.core).
- Компилятор распознаёт `str` как lang-item: type-mapping `str` → C `nova_str`
  (layout-идентичный typedef, см. ABI-стратегию).
- Лоуэринг литералов: `"abc"` → `str { ptr: <static const u8[] buffer>, len: 3 }`
  (C: `(nova_str){.ptr=(const uint8_t*)"abc", .len=3}` — как сейчас).
- Интерполяция `${...}` — через StringBuilder (без изменений семантики).
- **GATE:** тривиальная программа (`let s = "abc"; println(s); let t = s + "d"`)
  компилируется релизным компилятором и выполняется.

**Commit:** `feat(plan139 Ф.0): str lang-item value-record {ptr *ro u8, len int}`

### Ф.1 — Методы `str` → Nova-body (~1-2d)

Перевести на Nova-body, читая `@ptr[i]` (unsafe, typed-ptr deref):
`@byte_at`, `@len`, `@char_len`, `@char_at`, `@starts_with`, `@ends_with`,
`@contains`, `@find`, `@rfind`, `@trim`, `@to_lower`, `@to_upper`, `@concat`,
`@compare`, `@hash` (FNV-1a по байтам).

**Неустранимые C-примитивы (минимизировать):** UTF-8 decode-cursor (1 helper),
alloc нового буфера (RawMem), literal-from-static. Всё прочее — Nova.

**Acceptance:** string.nv компилируется; все str-тесты + lexer/find/trim (Plan 90
byte-алгоритмы) зелёные.

**Commit:** `feat(plan139 Ф.1): str methods to Nova-body via @ptr byte access`

### Ф.2 — `[]T`-producers → Vec (subsumes Plan 138.2 Ф.1) (~0.5d)

`@to_bytes -> Vec[u8]`, `@to_chars -> Vec[char]`, `@split -> Vec[str]`,
`@as_bytes -> ro Vec[u8]` (zero-copy view: `Vec{ data: @ptr as *mut u8, len, cap: len }`
— `@ptr` уже под рукой как поле, примитив `as_ptr` НЕ нужен — выигрыш value-record'а!).
`from_bytes_* (bytes Vec[u8])` — конструируют `str { ptr: bytes.data, len }`.

**Acceptance:** to_bytes/split/as_bytes round-trip; encoding(base64/hex)/text зелёные.

**Commit:** `feat(plan139 Ф.2): str []T-producers in Nova -> Vec`

### Ф.3 — Реконсиляция рантайм-C-слоя (~1d)

Подтвердить, что `nova_str` typedef-алиас держит 354 рантайм-вхождения
(net/effects/channels/sync/vtables). Починить прямые field-poke (если где-то
читают `.ptr`/`.len` с предположением `char*`/`size_t` — выровнять под `uint8_t*`/`int64`).
Особое внимание: net.c (адреса/данные), effects (error-msg), vtables (Display).

**Acceptance:** net/effects/sync/channels тесты зелёные; полная C-сборка без warning'ов типов.

**Commit:** `refactor(plan139 Ф.3): reconcile runtime C layer with str value-record ABI`

### Ф.4 — Полная регрессия (~1-2d)

str везде → широкая/полная регрессия. Per-subsystem fix order.

**Acceptance:** 0 новых FAIL vs baseline.

### Ф.5 — Docs + close (~0.5d)

D26 **major amend** (str — Nova value-record, не примитив; layout; lang-item статус).
D216 (str.ptr — `*ro u8` use-case). simplifications/project-creation/discussion-log/
README/memory.

**Commit:** `docs(plan139 Ф.5 D26): str as Nova value type — complete`

---

## Risk register

| # | Риск | Sev | Mitigation |
|---|---|---|---|
| R1 | str — самый пронизывающий тип; любая поломка лоуэринга литералов = всё ломается | 🔴 HIGH | Ф.0 GATE на тривиальной программе ДО методов; ABI-typedef сохраняет рантайм |
| R2 | bootstrap: panic/error-msg/interpolation используют str во время его же конструкции | 🔴 HIGH | lang-item доступен рано (core); literal-lowering не зависит от методов |
| R3 | `*ro u8` field + value-record: GC должен видеть ptr в str-значениях на стеке | 🟡 MED | conservative stack-scan; str-буферы статические или RawMem-tracked |
| R4 | const char* vs const uint8_t* / size_t vs int64 — type-pun в 354 C-сайтах | 🟡 MED | typedef-алиас + точечный аудит field-poke (Ф.3) |
| R5 | масштаб (~785 nova_str вхождений) | 🔴 HIGH | ABI-typedef резко сужает до emit_c.rs + string.nv |

---

## Acceptance criteria

- **E1** — `type str value priv { ptr *ro u8, len int }` объявлен, распознан как lang-item.
- **E2** — литералы `"..."` + интерполяция + `+` работают (релизный компилятор).
- **E3** — все str-методы — Nova-body (кроме ≤2 неустранимых C-примитивов: UTF-8 decode, alloc).
- **E4** — `to_bytes/to_chars/split/as_bytes` → Vec; `as_bytes` zero-copy без нового примитива (поле @ptr).
- **E5** — рантайм (net/effects/sync/channels/vtables) зелёный через ABI-typedef.
- **E6** — 0 новых FAIL; D26 major amend задокументирован.

---

## Связь / sequencing

| План | Отношение |
|---|---|
| Plan 138.1 | partial-landing []T→Vec (где Vec есть) + typed-storage gap |
| Plan 138.2 | universal Vec + NovaArray retire. **Ф.1 (string-layer) SUBSUMED этим планом** если 139 идёт первым |
| **Sequencing** | ✅ Решено 2026-06-10: **139-first**. 138.2 Ф.0 → 139 Ф.0-Ф.2 → 138.2 Ф.2-Ф.4. 138.2 Ф.1 subsumed здесь, без двойной переписки методов. |

> Если делаем Plan 139, рекомендуется: **139 Ф.0-Ф.2** (str value + методы + Vec-producers)
> → затем **138.2 Ф.0/Ф.2/Ф.3/Ф.4** (universal Vec + parfor/closure + remove NovaArray +
> regression). String-layer закрывается в 139, не дублируется.
