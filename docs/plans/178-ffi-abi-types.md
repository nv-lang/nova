# Plan 178 — C-FFI ABI: типы `extern "C"` + ABI fn-указателей (спека догоняет реальность)

> **Top-level план.** Создан 2026-06-22 (по аудиту pointer/FFI 2026-06-22). **Статус:** 📋 PROPOSED.
> **Маркер:** `[M-178-ffi-abi]`. **Запуск:** «**выполни план 178**».
> **Поглощает** backlog `[M-D282-ffi-abi-type-list]`. **Координация:** D282 (Plan 91.12), 172 (type-engine),
> 138.5/D216 (pointer/fn-ptr типы). **`spec/decisions/08-runtime.md` (D282) НЕ править в одиночку** — hot; амендмент
> применять согласованно. **Не путать с 177** (pointer-ops→методы; write-cap живёт там).
> **Сквозной критерий:** «без упрощений, как для прода».

## 1. Зачем (найдено 2026-06-22)

D282 rule 2 (`extern "C" fn`, [08-runtime.md:8285](../../spec/decisions/08-runtime.md)) — список C-ABI-совместимых
типов **занижен и местами неверен** относительно реального `std/net/ffi.nv`:

- **Туплы** (анон/именованные) из C-ABI-типов **уже используются**:
  `extern "C" fn socket_addr_parse(s str) -> (int, CSocketAddr)` ([ffi.nv:32](../../std/net/ffi.nv)) — это
  C-struct by-value (C-ABI-совместимо), но в списке D282 их нет.
- **`str`** (`{ptr,len}` value-record, D139) — C-ABI-совместим (POD-struct) и **используется** (`s str`), но D282
  ошибочно числит `str` как «ABI mismatch».
- **value-records / C-структуры** (`CSocketAddr`/`CTcpStream`/`CUdpSocket`/`CTcpListener`) используются, не в списке.
- скаляры минимальны: нет `f64`/`f32`/`i8`-`i64`/`char`.
- `Option[*T]` через NPO — C-совместим (bitwise 0 = NULL).

Плюс: у **fn-указательного ТИПА нет ABI-тега** — неясно, Nova-ABI это (`*fn`, Nova-типы ок) или C-ABI (для
C-callback нужны C-native типы).

## 2. Решение — переписать D282 rule 2 (тип-лист)

**C-ABI-совместимый тип = РЕКУРСИВНО:**
- **скаляры:** `int` / `i8`-`i64` / `u8`-`u64` / `f32` / `f64` / `bool` / `char`;
- **raw-указатели:** `*T` / `*()` / `CStr`;
- **`Option[*T]`** (NPO);
- **value-records и туплы (анон + именованные), ВСЕ поля которых C-ABI-совместимы** — передаются/возвращаются
  by-value как C-struct (`str` подпадает: `{ptr,len}`).

В **параметрах И в возврате**.

**Исключения** (ABI mismatch → `E_FFI_NON_C_ABI_TYPE`): GC-типы (`Vec`, heap-record-ссылки), closures-with-env,
generic tagged unions (`Option[non-ptr T]`, `Result`, прочие sum-типы — теговый layout не C-ABI).

## 3. ABI у fn-указательного типа

- `*fn(...)` / `*unsafe fn(...)` — **Nova-ABI** captureless fn-ptr: Nova-типы в сигнатуре допустимы (Nova ABI их
  передаёт). «Captureless» — про отсутствие env, не про типы.
- Для **C-callback** нужен **C-ABI** fn-ptr: ABI-тег на типе указателя — предлагаемо `*extern "C" fn(...)` (типы
  C-native по §2). Без тега непонятна ABI → нельзя проверить типы.
- **🔲 Под-вопрос:** точный синтаксис ABI-тега на fn-ptr-типе (`*extern "C" fn` / иное) — резолв в этой фазе.

## 4. Spec / D / Q / docs

- **amend D282 rule 2** (тип-лист §2, params+return); ввести `E_FFI_NON_C_ABI_TYPE` (+ error-index 09-tooling).
- fn-ptr ABI-тег — в D216/D282.
- docs/ffi-cookbook.md — раздел «какие типы можно через `extern "C"`» (туплы/str/value-records/Option[*T]).

## 5. Тесты (pos + neg)

- **pos** `nova_tests/ffi178/`: `extern "C" fn` с **туплом** (анон + именованный) в **параметре И возврате**; `str`;
  value-record/C-struct; `f64`/`char`/`i32`; `Option[*T]`. Регресс: `std/net` собирается без обходов.
- **neg:** `Vec[T]` / heap-record / closure / `Result` / `Option[non-ptr]` в `extern "C" fn` → `E_FFI_NON_C_ABI_TYPE`.

## 6. Критерии приёмки

1. D282 rule 2 отражает реальность: туплы (анон/имен) + `str` + value-records + полные скаляры + `Option[*T]` —
   допустимы в params и возврате; не-C-ABI типы → чистый `E_FFI_NON_C_ABI_TYPE`.
2. `std/net` (и прочий `extern "C"`-код) проходит без обходов; `str`-mismatch-ошибка снята.
3. fn-ptr ABI-тег определён (Nova-ABI vs C-ABI fn-ptr различимы).
4. **Без упрощений** (полный тип-лист, рекурсивная проверка, params+return).

## 7. Конвенции + координация

§5 spec-first (D282 amend до кода); §6 коды ошибок + error-index; §1 проверка типов в чекере единого движка (172);
§8 pos+neg + C-codegen. `08-runtime.md` (D282) — не править в одиночку (91.12/172).

## 8. Followup

`[M-178-ffi-abi]`. Поглощает `[M-D282-ffi-abi-type-list]` (backlog → этот план).
