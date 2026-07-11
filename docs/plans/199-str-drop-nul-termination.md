<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 199 — снять NUL-termination инвариант `str` (модель Rust/Go)

**Статус:** 📋 APPROVED 2026-07-11 (решение владельца после research). **Приоритет:** P2
(язык-чистка, не блокер). **Исполнять ПОСЛЕ мержа 196** (кодогенная часть трогает
`emit_c.rs` — не конфликтовать с идущим 196-агентом). **Требует D-амендмент** (retract
D26 §Nul-termination) — ложится В ТОЙ ЖЕ волне что и код (правило lang-change=spec,
[[feedback-lang-change-needs-spec]]).

## Решение (research 2026-07-11)

`str` = **чистый `ptr[len]` UTF-8, БЕЗ NUL-терминатора**. Отменяем аллокаторный
инвариант `ptr[len]=='\0'` (D26 §Nul-termination, реализован Plan 139 Ф.4). C-FFI —
через **явный** `CStr`/`as_cstr`, где `as_cstr()` **копирует** в NUL-терминированный
буфер (как Go `C.CString` / Rust `CString`), отвязано от per-alloc-накладных.

**Обоснование:** инвариант окупался только для целых строк (не для срезов `s[a..b]` —
у них NUL нет, `as_cstr` и так копировал → выгода частичная), стоил +1 байт/аллокацию
+ сложность аллокатора + концептуальную течь-на-слайсах; реляйнс низкий (cstr.nv +
3 консьюмера: os/ffi, encoding/toml, fs/fs). Rust/Go/Swift держат строку без NUL и дают
явные C-string-типы — современный консенсус. (Полный research — discussion-log.)

## Фазы (все в ОДНОЙ волне — код+спека вместе)

- **Ф.1 — D-амендмент (спека):** retract D26 §Nul-termination; зафиксировать `str` =
  `ptr[len]` UTF-8 без терминатора; C-FFI = явный `CStr` (copy-based). Обновить §строк
  спеки + cstr-доки.
- **Ф.2 — `as_cstr` → `@to_cstr` (РЕШЕНО владельцем 2026-07-11):** заменить
  `as_cstr`/`as_cstr_unchecked` на ДВЕ перегрузки `str @to_cstr` (receiver-form, матчит
  `@to_char` D54/§22; `to_`=копирующая конверсия правильнее `as_`=borrow, раз zero-copy нет):
  (a) `@to_cstr() -> CStr` — GC-копия (`[]u8.new().cap(byte_len+1).append(bytes).append(0)`);
  (b) `@to_cstr(buf *mut u8, buf_size int) -> CStr requires buf_size>0` — zero-alloc,
  caller-buffer (`RawMem.copy` + NUL на `byte_len().min(buf_size-1)` — .min ОБЯЗАТЕЛЕН, иначе
  OOB при обрезке). Убрать примитив `nova_fn_nova_str_terminated_ptr`. Обновить всех
  вызывающих `as_cstr` → `@to_cstr()`. Записать API в D418.
- **Ф.3 — codegen (ПОСЛЕ 196):** снять резерв `+1` NUL в эмиссии строковых литералов +
  динамической аллокации строк (`emit_c.rs` + `nova_rt` строковый аллокатор). `str` C-repr
  = `ptr[len]` без хвостового байта. ⚠️ emit_c.rs — ждать мержа 196.
- **Ф.4 — FFI-тесты + гейты:** обновить `plan115_ffi_test.h` (as_cstr теперь copy, не
  in-place) + str→CStr→C strlen через копию. Грепнуть 0 опор на `ptr[len]=='\0'`.

## Гейты

conformance δ0 (поведение строк не меняется, кроме снятого инварианта); FFI-тесты
зелёные с copy-based `as_cstr`; `nova test std` без регрессий; grep-инвариант «нет опоры
на хвостовой NUL вне явного CStr»; D-амендмент retract D26 §Nul-termination в ТОМ ЖЕ
слиянии; discussion-log + simplifications.md обновлены.

## Границы

Не меняет UTF-8-семантику str, lens-модель (D249 byte_len/chars), Buffer/[]u8. Только
снимает хвостовой NUL и переводит C-FFI на явную копию.
