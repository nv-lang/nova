<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# План 227 — std/validate: библиотека валидаторов (.nv, ноль компилятора)

**Статус:** 📋 ПРЕДЛОЖЕН (владелец 2026-07-24: «заведи известные валидаторы общего назначения +
пример сложного эффектного»). **Тип:** чистая .nv-библиотека — НИ СТРОКИ в компиляторе (в отличие
от отклонённого `#validate`-derive, 222.9). **Приоритет:** P3, после релиза v0.1 (роадмап).
**Мотив:** снять раздувание newtype-валидации в 90% случаев — переиспользуемые готовые типы вместо
своего newtype под каждое поле; закрывает нишу FastAPI/Pydantic-валидации БЕЗ хардкода в компиляторе.

## 0. Три уровня валидации — НЕ смешивать (ключевое)

Владелец 2026-07-24, при разборе `#serde(codec)` и «валидатора с БД»:

| Уровень | Что | Где живёт | Эффекты |
|---|---|---|---|
| **1. Инвариант типа** | форма/диапазон (`Email` формат, `Age` 1..120) | newtype `priv` + валидирующий `.new() -> Result` | НЕТ (чистый) |
| **2. Codec (serde)** | КАК читать/писать поле на проводе (дата RFC3339) — **НЕ валидация** | `#serde(codec = Тип)`, протокол `FieldCodec[T]` | НЕТ |
| **3. Семантическая валидация** | уникальность email в БД, CSRF-токен из запроса, «заказ принадлежит юзеру» | код **хендлера** | ДА (`Db`/доступ к `ServerRequest`) |

Уровень 1 — этот план. Уровень 2 — 180.1 Ф.5 (codec). Уровень 3 — **никогда не в типе/serde**:
требует эффектов и контекста запроса, поэтому по построению живёт в обработчике (см. §3).

## 1. Готовые валидаторы уровня 1 (std/validate/*.nv)

Каждый — newtype с `priv` + валидирующий `.new() -> Result[Self, DeError]` + property-геттер
`@value()` (§4а-канон). Все — обычный .nv, переиспользуемы во ВСЕХ DTO, `#impl(Deserialize)`
вызывает их `.new()` через существующий синтез (D435) → typed-ошибка с `path` автоматически.

| Тип | Инвариант |
|---|---|
| `Email` | RFC5322-lite (есть `@`, домен, без пробелов) |
| `Url` | схема + хост (переиспользует `http.url`-парсер) |
| `NonEmpty[str]` | `.len() > 0` |
| `Bounded[int, MIN, MAX]` | `MIN <= v <= MAX` (const-generic границы — зависит от const-generic поддержки; до неё — конкретные `AgeLike`/`Port` и т.п.) |
| `MinLen[str, N]` / `MaxLen[str, N]` | длина |
| `Pattern` | regex-совпадение (переиспользует `std/regex`) |
| `Uuid` | формат UUID |
| `Slug` | `[a-z0-9-]+` |
| `Positive[T]` / `NonNegative[T]` | знак |

**Пример объявления одного (образец для остальных):**
```nova
export type Email value { priv s str }

export fn Email.new(v str) -> Result[Email, DeError] =>
    if is_email_shape(v) { Ok(Email { s: v }) }
    else { Err(DeError { kind: DeErrorKind.Invalid("email"), path: "" }) }

export fn Email @value() -> str => @s
```

**Использование — переиспользуемость снимает раздувание:**
```nova
#impl(Deserialize) type RegisterReq value { ro email Email, ro name NonEmpty, ro age Age }
#impl(Deserialize) type LoginReq    value { ro email Email, ro pw   NonEmpty }  // Email — 0 новых строк
#impl(Deserialize) type UpdateReq   value { ro email Email, ro age  Age }        // и здесь
```
Объявил `Email` раз — используешь везде; `#validate` наоборот дублировал бы предикат на каждом DTO.
Инвариант держится ВЕЗДЕ в программе (Email нельзя собрать невалидным нигде), не только на границе.

## 2. Свой ПРОСТОЙ валидатор (чистый, уровень 1)

Тот же паттерн — свой предикат обычным кодом (regex/кастом), ноль грамматики атрибутов:
```nova
export type EvenPositive value { priv n int }
export fn EvenPositive.new(v int) -> Result[EvenPositive, DeError] =>
    if v > 0 && v % 2 == 0 { Ok(EvenPositive { n: v }) }
    else { Err(DeError { kind: DeErrorKind.Invalid("even positive"), path: "" }) }
export fn EvenPositive @value() -> int => @n
```

## 3. Свой СЛОЖНЫЙ валидатор (эффектный, уровень 3 — в ХЕНДЛЕРЕ)

Проверка, требующая БД/запроса — НЕ newtype и НЕ serde (у типа нет эффектов, у serde нет БД).
Живёт в обработчике, после десериализации, как обычная бизнес-логика с эффектом `Db`:
```nova
// DTO прошёл СИНТАКСИЧЕСКУЮ валидацию (уровень 1) при десериализации.
// СЕМАНТИЧЕСКАЯ (уникальность в БД) — здесь, эффектно:
fn register(req ServerRequest) Db -> Result[User, HttpError] {
    ro dto = req.json[RegisterReq]()?          // уровень 1 отработал: формат email/age валиден
    // уровень 3 — эффектная проверка против БД:
    if Db.email_exists(dto.email.value()) {
        return Err(HttpError.conflict("email already registered"))   // 409
    }
    ro user = Db.create_user(dto)?
    Ok(user)
}
```
**Почему так, а не в типе:** уникальность зависит от СОСТОЯНИЯ БД (внешний эффект) и МОМЕНТА
(сегодня свободен, завтра занят) — это не инвариант значения, а факт о мире. Инвариант («формат
email») — в типе; факт о мире («этот email свободен») — в эффектном коде. Смешение сделало бы тип
нечистым (десериализация тянула бы `Db`) — против strict-effects и против сути «тип валиден по
построению». CSRF-токен, «заказ принадлежит юзеру», rate-по-пользователю — тот же уровень 3.

## 4. Гейты / модель

Чистая .nv-библиотека: пир-тесты `*_test.nv` на каждый валидатор (pos/neg — валидный/невалидный
вход → typed DeError с path); `nova test std/src/validate`; интеграция с serde — round-trip
`decode` невалидного JSON → DeError. Компилятор не трогается вовсе.

## Связи

222.9 (❌ отклонён #validate — этот план его замещает правильным путём) · 180.1 Ф.5 (codec,
уровень 2) · D435 (serde вызывает field-type `.new`/`.deserialize`) · D325 (DeError typed) ·
strict-effects (уровень 3 несёт `Db` честно).
