<!-- SPDX-License-Identifier: CC-BY-4.0 -->
КЛЕТКА | check | К2

# Охота: scoped identity локального callable при посадке двери A

Исследован кандидат 27437b24662ba352012b90d9e291fb75e0756160. Охота выполнена до оформления отчёта; дата отчёта 2026-11-22. Использован принятый da-fix2 с SHA256 2a168493b53c851fcb9ae788f9e99efe688173e7e019e35d559fa08c614cdbf1: novac/src кандидата совпадает с принятой веткой. Пробы лежат в `probes/2026-11-22-check-k2-local-call/`, каждая в отдельном каталоге.

## Находки

НИЧЕГО НЕ НАШЁЛ — нового подтверждённого нарушения scoped identity на этих пробах не воспроизведено. Это результат поиска, не утверждение отсутствия дефектов и не приёмка полного контура.

| Проба | Что проверено | Carina check/emit/clang/run | stdout |
|---|---|---|---|
| nested | восстановление callable после вложенного затенения | 0/0/0/0 | 532 |
| forward | передача локального fn-значения | 0/0/0/0 | 312 |
| rebind | повторное связывание | 0/0/0/0 | 312 |
| capture | двухуровневый захват | 0/0/0/0 | 312 |
| receiver | callable scopes внутри метода | 0/0/0/0 | 200, затем 24 |
| intrinsic | коллизия локальных имён println/assert | 0/0/0/0 | 21 |
| variant | коллизия конструктора и nested rebind | 0/0/0/0 | 326 |
| stored | сохранение литерала и вызов после rebind | 1/1/не запускался/не запускался | именованный E_NOVAC_SUBSET |

Нормы: D22, D347 R3/R4/R6/R7; местный приоритет callable закреплён в check/calls.nv. Oracle check первых пяти проб — rc0; intrinsic — E_NO_MATCHING_OVERLOAD, variant — E_REFUTABLE_BINDING. Эти различия требуют нормативного триажа, сами по себе не признаны дефектами Carina. Oracle check stored — rc0; именованная граница Carina не названа багом без отдельного решения.

Команды повторения из корня репозитория, с выбранным проверенным бинарём в NOVAC_BIN:

```sh
unset NOVAC_SELF_PATH
export NOVA_CACHE=0
"$NOVAC_BIN" check docs/dev/hunts/novac/probes/2026-11-22-check-k2-local-call/nested/main.nv
"$NOVAC_BIN" emit --target-os windows docs/dev/hunts/novac/probes/2026-11-22-check-k2-local-call/nested/main.nv
```

Тот же вызов применим к каждому из восьми каталогов; нативная проверка использовала сохранённые ABI-аргументы clang и runtime библиотек из v9b-smoke-cache. Это не независимый oracle-parity прогон и не полный combined self-build. Точные argv/rc/stdout первоначальных запусков сохранены в scratch `hunt-door-a-check-k2-88589/evidence.jsonl`; пути внутри самих проб переносимы, абсолютных путей они не содержат.

## Что обошёл и почему

Не покрыты mut/by-reference captures, generic signatures, named arguments, trailing/full формы D43 и межфайловые коллизии. Проба capture не доказывает сохранение старого захвата после subsequent rebind: stored встретила границу подмножества. Pipeline ICE 984 и полный корпус не перезапускались. Проверки scoped undo trail и binding/capture identity напрягались runnable nested/rebind/receiver пробами; ключевой страж проверяет форму ключей, не полноту lexical restoration.

## Противоречия

Документационный дрейф: check/local_call.nv:20 говорит `A local can hold a fn type only as a parameter today`, тогда как check/local_bindings.nv:19 выполняет `@scope.bind(name, ty, mode)` и новая rebind-проба допускает `ro old = f`. Это неточность описания, не доказательство нарушения архитектуры; она не исправляется под видом новой языковой нормы этим отчётом.
