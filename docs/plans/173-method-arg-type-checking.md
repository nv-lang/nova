<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 173 — Method-argument type-checking + scalar-narrowing enforcement

**Статус:** 📋 proposed 2026-06-19
**D-блок:** предв. **D311** (финал при impl; D310=172, D309=171, D308=169.2.1)
**Ветка:** TBD (`plan-173-method-arg-typecheck`)
**Приоритет:** P1 (soundness).
**Связан с:** `[M-instance-method-arg-scalar-narrowing]`, `[M-generic-arg-type-mismatch-silent]` (closed), `[M-scalar-nonliteral-narrowing-not-enforced]` (mostly-done — этот план закрывает остаток).

---

## 1. Мотивация

Аргументы вызова **метода** (`obj.method(arg)`) не проходят через type-checker:
`f1_check_call` ([types/mod.rs:6249](../../compiler-codegen/src/types/mod.rs)) для instance-методов
делает `_ => return` — он не резолвит метод (вывод типа получателя + перегрузки исторически
живут в codegen), поэтому типы аргументов методов чекер **не сверяет**.

На практике это **частично** закрыто другими слоями (проверено эмпирически 2026-06-19):

| Слой | Что ловит у method-call | Пример |
|---|---|---|
| Nova type-checker | существование метода (`E7320`), арность («обязательный параметр не передан») | — |
| codegen overload-резолвер (D84, [emit_c.rs:23026](../../compiler-codegen/src/codegen/emit_c.rs)) | перегрузки по статическим типам аргументов; нет кандидата → `CODEGEN-FAIL` | `g(int)/g(str)` ← `g(bool)` → `no matching overload for g(nova_bool)` |
| C-компилятор (codegen-выход) | категориальное несовпадение struct↔scalar | `Vec[int].push(str)` → `CC-FAIL passing nova_str to incompatible type nova_int` |

**Единственная реальная дыра** — `scalar→scalar` неявное **сужение** через аргумент
**одиночной** (не-перегруженной) перегрузки метода: арность совпадает с единственным
`push(u32)`, а `int→u32` — легальная C-конверсия (тихое усечение), поэтому **ни один слой не
ругается**. Канонический случай — `vec_u32.push(int_var)`.

Не-method-формы уже закрыты в Plan «scalar narrowing» (commit `f96016e6`): `[E_IMPLICIT_NARROWING]`
для **binding** (`ro a u8 = int_var`), **свободной/static fn-arg** (`take_u8(int_var)`),
**reassignment** (`a = int_var`). Этот план распространяет ту же гарантию на **аргументы методов**.

## 2. Что есть сейчас (опора)

- `is_int_narrowing(found, expected)` + `int_width_rank` ([types/mod.rs](../../compiler-codegen/src/types/mod.rs)) — готовое правило: value-range-preserving widening неявный; narrowing + signed→unsigned + value-unsafe cross (`u32→i32`, `u64→int`) требуют `as`. Работает на raw-`TypeRef`.
- `Compat::Narrowing` + `[E_IMPLICIT_NARROWING]` — диагностика уже есть.
- `check_call_argbind` ([types/mod.rs:9753](../../compiler-codegen/src/types/mod.rs)) **уже резолвит** instance-методы через `resolve_instance_method(obj, method_name, scope, arity)` → получает `f.params` (для арность/keyword-проверки). Это потенциальная точка для checker-варианта (см. Ф.0).
- codegen overload-резолвер ([emit_c.rs:23026](../../compiler-codegen/src/codegen/emit_c.rs)) сравнивает `param_c_types` с `infer_expr_c_type(arg)` для **перегруженных** свободных fn — клин для codegen-варианта, но одиночные перегрузки param-типы не сверяют.
- Codegen-представление в C-типах: `nova_int`=int64_t (8 байт, signed), `uint32_t` (4 байта) и т.д. — width/sign модель уже есть в `int_width_rank` (Nova-имена) и в width-таблице emit_c (`"uint32_t"=>4`).

## 3. Открытые дизайн-вопросы (решить в Ф.0 — GATE)

- **Q1 — где enforce'ить: checker vs codegen?** Два кандидата:
  - **(A) Checker** — расширить `check_call_argbind`: после `resolve_instance_method` сверить
    каждый аргумент с `param.ty` через существующий `assignable`/`is_int_narrowing`. Плюс: чистая
    диагностика `[E_IMPLICIT_NARROWING]`, единый код-путь с free-fn. Минус: `resolve_instance_method`
    — best-effort; **встроенные** методы (`Vec.push`, `str.*`) живут НЕ в `method_table`, а в
    external/builtin-реестре — их receiver-резолв в чекере может не находить → дыра для именно того
    хотспота (`push`).
  - **(B) Codegen** — добавить narrowing-проверку там, где param-C-типы уже известны (резолвер D84
    + точки method-dispatch). Плюс: ловит ВСЁ (включая builtin `push`). Минус: codegen-многопутёвость
    (`emit_call` :22502 — гигантский диспетчер; `vec_method_call` и спец-эмиттеры), C-уровневая
    диагностика менее чистая, риск задеть hot-файл.
  - **(C) Гибрид** — receiver-type инференс перенести/усилить в чекере (вывести тип `obj`, найти
    сигнатуру метода в едином реестре, включая builtin), а сверку делать через готовый `is_int_narrowing`.
  - **Критерий выбора:** покрывает ли вариант **builtin-методы** (`push`) — иначе хотспот не закрыт;
    минимальный риск для codegen; единая диагностика. Предв. рекомендация — **(C)**: единый
    method-резолвер в чекере (включая builtin-сигнатуры из external-реестра), сверка через
    `is_int_narrowing`. Решить замером покрытия в Ф.0.
- **Q2 — какой объём type-check у методов включать?** Только narrowing, или ВСЕ типы аргументов
  (как у free-fn — `E7301` на `str→int`)? Полная типизация чище, но вскроет больше (часть уже
  ловит C-компилятор как CC-FAIL → станет чистой compile-error раньше). Предв.: включить полную
  типизацию аргументов методов (narrowing — частный случай), но за флагом/поэтапно, чтобы
  контролировать blast-radius.
- **Q3 — overload-резолв в чекере.** Если методы перегружены, сверка должна выбрать правильную
  перегрузку ДО проверки типов (иначе ложные ошибки). Переиспользовать логику codegen D84-резолва
  (filter по arity+param-types) на уровне чекера, либо консультироваться с тем же реестром.
- **Q4 — alias/newtype/generic-receiver.** Permissive там же, где `is_int_narrowing` (alias/newtype
  не резолвятся → skip), и где receiver — generic-параметр (тип неизвестен → skip). Без ложных
  срабатываний на generic-коде.

## 4. Фазы

- **Ф.0 — Аудит + decision point (GATE).** Замерить: (a) сколько method-call сайтов в std/tests —
  scalar-narrowing аргументы (детект-режим, не-фатально); (b) резолвит ли `resolve_instance_method`
  builtin `Vec.push`/`str.*` (определяет жизнеспособность варианта A); (c) выбрать слой Q1.
  **Выход:** число сайтов миграции + выбранный вариант. Без этого Ф.1 не начинать.
- **Ф.1 — Method-резолв + сверка аргументов.** По выбору Ф.0: единый method-резолвер (receiver-тип
  + сигнатура, включая builtin) и сверка каждого аргумента через `is_int_narrowing` (+ опц. полная
  `assignable`). Эмит `[E_IMPLICIT_NARROWING]` (+ опц. `E7301`). Permissive на alias/generic/unknown.
- **Ф.2 — Измерение blast-radius.** Полный прогон std+nova_tests, точный список сайтов
  `[E_IMPLICIT_NARROWING]` по method-args (ожидаемо ~375 `push(int)` — buffers/codecs/crypto/unicode).
- **Ф.3 — Миграция std + тестов.** Механически обернуть сужающие аргументы в `as <T>`
  (`out.push(cp)` → `out.push(cp as u32)`). Воркфлоу-фан-аут по файлам (каждый агент: батч файлов
  → добавить `as` где новый чекер требует → проверить). Флагнуть случаи, где `as`-усечение
  семантически неверно (нужен range-check / надо менять тип контейнера) — отдельно.
- **Ф.4 — Тесты + спека.** Позитив (widening/as/литерал у method-args — OK) + негатив (`push(int)`,
  `obj.m(narrowing)` — `E_IMPLICIT_NARROWING`). Спека: D54/conversions.md amend (value-preserving int
  widening — неявный; narrowing/sign-unsafe — `as`); зафиксировать «аргументы методов типизируются».
  Новый/переиспользованный код ошибки в `09-tooling.md` error-index.

## 5. Критерии приёмки

1. `vec_u32.push(int_var)` (и любой `obj.method(narrowing_int)`) → `[E_IMPLICIT_NARROWING]`,
   `obj.method(int_var as u32)` → OK, widening (`push(u8_var)` в `Vec[int]`) → OK.
2. Полнота: покрыты **встроенные** методы (`Vec.push`, `str.*`) — не только user-методы.
3. **Без упрощений как для прода:** реальный резолв метода + сверка, никаких заглушек; permissive
   только на принципиально-неразрешимом (alias/generic/unknown), зафиксированном маркером.
4. Ноль ложных срабатываний на позитивном корпусе после миграции; std+nova_tests зелёные.
5. Миграция std завершена (все сужающие method-args получили `as`), либо обоснованно вынесена
   точечными followup'ами там, где `as` семантически неверен.
6. Спека/доки/error-index обновлены; D311 написан.

## 6. Риски

- **Codegen-вариант (B)** рискует задеть `emit_c.rs` (hot, общий с другими сессиями) — координировать.
- **Полная типизация (Q2)** может вскрыть много existing-ошибок за пределами narrowing — поэтапно/за флагом.
- **Миграция 375 сайтов** — механическая, но проверять, что `as`-усечение не маскирует реальный
  range-баг (значение может не влезать); где сомнительно — range-check вместо `as`.

## 7. Источник

Обсуждение 2026-06-19 (сессия generic-arg + scalar-narrowing). Эмпирическая карта method-call
проверок (существование/арность — чекер; перегрузки — codegen; категория — C; scalar-narrowing — дыра)
получена прямыми пробами. Готовая инфраструктура (`is_int_narrowing`, `Compat::Narrowing`,
`E_IMPLICIT_NARROWING`) из commit `f96016e6`.
