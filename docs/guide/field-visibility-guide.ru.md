---
source_rev: 21dff1b37
source_date: 2026-08-02
---

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Гайд по видимости полей (модификатор `priv`)

[English](field-visibility-guide.md) | **Русский**

> **Статус:** ACTIVE с 2026-06-02 (Plan 124.1-124.5).
> **Спека:** D220 / D221 / D222 (см. `spec/decisions/02-types.md`).

Этот гайд описывает систему приватности отдельных полей Nova — когда
использовать `priv`, как он сочетается с другими модификаторами,
инструментальную поддержку и сравнение с мейнстримными языками.

---

## 1. TL;DR

```nova
export type Account {
    ro name str               // public, immutable
    priv mut balance f64      // private, mutable (only Account methods can touch it)
}

export fn Account.new(n str) -> Account =>
    { name: n, balance: 0.0 }

export fn Account @deposit(amount f64) {
    @balance = @balance + amount    // OK — inside Account method
}

// External code:
ro acc = Account.new("alice")
ro n   = acc.name           // ✅ public
ro b   = acc.balance        // ❌ E_PRIV_FIELD_READ
acc.balance = 100.0         // ❌ E_PRIV_FIELD_WRITE
```

Видимость по умолчанию — **public** (согласуется с экспортируемыми-по-регистру
полями Go и дефолтами Kotlin/Swift на 92.4% API-поверхности, по замерам на типах
kubernetes API). Опт-ин `priv` на поле — когда нужна защита инварианта.

---

## 2. Когда использовать `priv`

| Кейс | Рекомендация |
|---|---|
| **Несущее инварианты внутреннее состояние** (balance, lock-state, позиция курсора) | ✅ Пометить `priv` |
| **Мутируемый внутренний кэш** (`mut last_modified`) | ✅ Пометить `priv` |
| **Чувствительные данные** (auth-токены, крипто-ключи, сырые указатели) | ✅ Пометить `priv` |
| **Публичная API-поверхность** (поля DTO-record, значения config-struct) | ❌ Оставить public |
| **Data-bag типы** (payload событий, log-записи) | ❌ Оставить public |

Правило большого пальца: **если метод должен валидировать или координировать
перед мутацией — помечай нижележащее поле `priv`**. Иначе public держит
поверхность минимальной.

---

## 3. Синтаксис + сочетание

### 3.1 Модификатор на поле

Сначала модификатор видимости, затем мутабельность, затем имя, затем тип:

```nova
priv mut money f64       // private + mutable
priv ro id u64           // private + read-only
priv consume token Token // private + consume (Plan 100.x)
```

`priv` перед `mut`/`ro`/`consume` соответствует прецеденту порядка модификаторов
Plan 108 D175/D176 + Plan 114 D184.

### 3.2 Взаимное исключение: `priv` vs `pub`

```nova
priv pub x f64    // ❌ E_PRIV_PUB_CONFLICT
pub priv x f64    // ❌ E_PRIV_PUB_CONFLICT (detected at parser)
```

`pub` зарезервирован для явного public-override типового дефолта `priv`
(Plan 124.7 — синтаксис тип-уровневого флипа `type X priv {}`).

### 3.3 Именованные кортежи (Plan 124.4 / D222)

Пофайловый `priv` расширяется на именованно-кортежную форму (Plan 120 D215):

```nova
type Vec3(priv x f64, priv y f64, priv z f64)
type Account(priv balance f64, name str)      // mixed
type Secret(pub key str, priv salt []u8)      // explicit pub
```

### 3.4 Generic-типы (D220 §G1)

Единообразное принуждение:

```nova
type Stack[T] {
    priv mut len int
    ro capacity int
}

export fn Stack[T] @push(x T) {
    @len = @len + 1     // ✅ inside method scope (recv = Stack)
}

// External:
mut s = Stack[int].new(10)
s.len = 0    // ❌ E_PRIV_FIELD_WRITE — uniform for every T
```

---

## 4. Диагностические коды (формат Plan 50 D102)

| Код | Место | Когда |
|---|---|---|
| `E_PRIV_FIELD_READ` | Обращение к члену на priv-поле вне области видимости | `acc.balance` |
| `E_PRIV_FIELD_WRITE` | Мутирующее присваивание вне области видимости | `acc.balance = 0` |
| `E_PRIV_FIELD_INIT` | Record-литерал или named-tuple ctor вне области видимости | `Account { balance: 0 }` или `Vec3(x: 1.0)` |
| `E_PRIV_FIELD_PATTERN` | Паттерн-деструктуризация вне области видимости | `Account { balance } = acc` |
| `E_PRIV_FIELD_INIT_SPREAD` | Spread record-литерала вне области видимости | `Account { ...other }` |
| `E_PRIV_PUB_CONFLICT` | `priv` и `pub` на одном поле | `priv pub x f64` |

Каждая диагностика включает:
- Ссылку на спеку (D220 / D221 / D222)
- Подсказку, предлагающую публичный метод или фабрику
- Span на нарушающем месте

---

## 5. Инструментарий

### 5.1 `nova doc`

По умолчанию priv-поля **скрыты** из отрендеренной документации:

```bash
$ nova doc src/account.nv
type Account { name str }    # balance hidden
```

Используй `--include-private`, чтобы показать все поля (с сохранением ключевого
слова `priv` в отрендеренной сигнатуре):

```bash
$ nova doc src/account.nv --include-private
type Account { name str; priv mut balance f64 }
```

JSON-вывод (`--format json`) эмитит `"priv_field": true` для каждого priv-поля,
независимо от `--include-private` — потребляется LSP и другим инструментарием.

### 5.2 LSP (forward-ref)

Когда выйдут Plan 104.2 (hover) и Plan 104.3 (completion), они будут:
- Скрывать priv-поля из автокомплита вне области видимости методов типа.
- Показывать бейдж `🔒 priv` в hover-попапах.
- Показывать code-lens декорации priv-полей.

Флаги AST `RecordField.priv_field` и `NamedTupleField.priv_field` (уже
экспонированы) — источник данных. Plan 124.5 V1 подключает doc-слой;
LSP-интеграция следует после релиза Plan 104.2/104.3.

### 5.3 Никакого reflection-бэкдора

У Nova нет API рефлексии (D6 managed GC + AOT codegen). Принуждение `priv` —
**компиляйт-тайм, жёсткая гарантия**.

Сравни с Java/Kotlin/C#/Swift, у которых есть API рефлексии, обходящие private
(не считая привилегий) — гарантия Nova строже любой из них.

---

## 6. Сравнение с другими языками

| Возможность | Go | Rust | TS | Java | Swift | C# | **Nova** |
|---|---|---|---|---|---|---|---|
| Пофайловая приватность | ❌ (по регистру) | ✅ `pub` | ✅ `private` | ✅ `private` | ✅ `private` | ✅ `private` | ✅ **`priv`** |
| Видимость по умолчанию | pkg-priv если lowercase | mod-priv | public | package | internal | private | **public, opt-in priv** |
| Строгая область видимости только для типа | ❌ (pkg-wide) | ❌ (mod-wide) | ✅ (класс) | ✅ (класс) | ❌ (файл/мод) | ✅ (класс) | ✅ **type-method-only** |
| Reflection-бэкдор | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ **compile-time принуждается** |
| Принудительная фабрика для priv-init | ❌ | ✅ `pub(...)` | ✅ | ✅ | ✅ | ✅ | ✅ **доступ вне области видимости заблокирован** |
| Приватность полей кортежа | ❌ | ✅ `struct(pub T)` | ❌ | ❌ | ❌ | ❌ | ✅ **priv именованного кортежа** |

Nova совпадает или превосходит на 6/6 возможностей + 3 Nova-only превосходящих
гарантии (строжайшая область видимости, без рефлексии, интегрирован с эффектной системой D2).

---

## 7. Миграция

V1-V5 — **чисто аддитивные** — существующий код (без модификатора `priv`) не
меняется, компилируется бит-в-бит как до Plan 124.

Plan 124.6 (тестовый escape доступа) и 124.7 (тип-уровневый флип) добавляют
опт-ин фичи без нарушения семантик V1-V5. Флип эдишена рассматривался, но
отклонён в пользу пер-типового флипа `type X priv {}` (более мягкая история
миграции).

---

## 8. Частые паттерны

### 8.1 Сеттер, сохраняющий инвариант

```nova
export type Account {
    ro id str
    priv mut balance f64
}

export fn Account mut @deposit(amount f64) -> () {
    assert(amount >= 0.0, "deposit must be non-negative")
    @balance = @balance + amount
}

export fn Account @balance_of() -> f64 => @balance
```

### 8.2 Кэш через priv mut

```nova
export type ParseCache {
    ro source str
    priv mut last_parse Option[Ast]
}

export fn ParseCache mut @parse() -> Ast {
    if Some(a) = @last_parse {
        return a
    }
    ro a = do_parse(@source)
    @last_parse = Some(a)
    a
}
```

### 8.3 Чувствительные данные с priv ro

```nova
export type Session {
    ro user_id str
    priv ro token str          // immutable + private
}

export fn Session.from_login(uid str, t str) -> Session =>
    { user_id: uid, token: t }

// Token only used internally:
export fn Session @authorize(target_op str) -> bool =>
    verify_signature(@token, target_op)
```

---

## 9. См. также

- `spec/decisions/02-types.md` — D220 / D221 / D222 (семантика)
- `spec/decisions/07-modules.md` — D47 (module-level pub vs per-field priv)
- `docs/plans/124-priv-field-visibility.md` — зонтичный план
- `docs/dev/research/06-field-visibility-go-kubernetes.md` — эмпирическое
  исследование видимости по умолчанию (kubernetes 11099 structs / 35239 fields).
