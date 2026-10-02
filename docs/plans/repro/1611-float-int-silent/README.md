<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# 1611 — значение `f64` молча усекалось в целую позицию

Убери суффиксы `.txt` и прогони каждый файл одиночным `nova test` (маркер `EXPECT_STDOUT`
записан тем, что печатал оракул main — то есть ДО фикса тест зелёный, и это и есть дефект).

| проба | что делает | оракул main | после фикса |
|---|---|---|---|
| `w3_float_into_int.nv` | `f64` в `int`: объявление, возврат, поле литерала, `push` | check `ok`, печатает `2 2 2 2` | `E_IMPLICIT_NARROWING` ×4 |
| `w7_assign_and_f32.nv` | `n = d`, `g = i` (обратно), `h(d)` при `h(x f32)` | check `ok`, `2 3 2.5 …` | `E_IMPLICIT_NARROWING` ×3 |
| `g_field_assign.nv` | `p.n = d` | check `ok`, печатает `2` | `E_IMPLICIT_NARROWING` |

Замер 2026-10-02 (оракул main и ветка `p1611-float-int-kind`). Фикстуры класса —
`spec_tests/conformance/neg/p1611_*_neg.nv` (13) и `standalone/p1611_numeric_as_ok.nv`.
