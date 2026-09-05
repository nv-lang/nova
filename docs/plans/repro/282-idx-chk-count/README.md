# Проба к плану 282 (Ф.6) — счёт bounds-check в парсерах `runtime.string`

Не дефект — инструмент приёмки. Заведено исследовательским окном 2026-09-05, чтобы у Ф.6
(указательный обход в `parse.nv` + `parse_float.nv`) был воспроизводимый замер «до/после».

## Как считать

```sh
cp docs/plans/repro/282-idx-chk-count/count_idx_chk.nv.txt <dir>/bc2.nv
nova-cli/target/release/nova.exe build <dir>/bc2.nv -o <dir>/bc2.exe --keep-artifacts
# .c лежит в %LOCALAPPDATA%/Temp/nova_tests-<pid>/build-<hash>/bc2.c
python -X utf8 docs/plans/repro/282-idx-chk-count/count_idx_chk.py <путь к bc2.c>
```

Считается число вызовов `nova_idx_chk` в ТЕЛЕ каждой функции сгенерированного C
(`Nova_str_method_*`), не число `bytes[` в исходнике.

## Замер 2026-09-05 (до Ф.6, оракул)

| функция (C) | `nova_idx_chk` | `bytes[` в исходнике: строк / вхождений |
|---|---|---|
| `to_f64` (`parse_float.nv`) | 11 | 9 / 12 |
| `parse_int_core` (`parse.nv:42-50`) | 3 | 3 / 3 |
| `parse_uint_core` (`parse.nv:74-79`) | не инстанцирована пробой | 2 / 2 |
| `to_int`, `to_i8`, `to_bool` | 0 | — |

Три числа у `to_f64` расходятся законно: `grep -c` считает строки (в одной строке бывает два
чтения — `bytes[0] == '-' || bytes[0] == '+'`), вхождений 12, в C — 11. Хелперов, которые
могли бы инлайниться, в `parse_float.nv` нет.

**Приёмка Ф.6:** в этой же таблице ноль у `to_f64`, `parse_int_core` и `parse_uint_core`
(для последней в пробу надо добавить вызов `to_uint()` / `to_u8()`); обратная проба —
вернуть один `bytes[i]`, число снова ненулевое.
