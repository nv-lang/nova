# 135: диагностика отмены CI, не приёмка кандидата

Разрешение: письмо nova.integrator от 2026-11-01 12:45, делегированное решение дежурного по поручению владельца. Разрешена только минимальная наблюдаемость отменённого пула, один новый диагностический CI. Ни лимит 45m, ни критерии, ни состав/skip стражей не меняются. Main и поставляемые пины не трогаются.

## Исходный CI

Точный SHA: `0fdb2b37b2c71b2bd718de82c12a6eccf2e1cfe6`.
Из девяти existing workflows восемь success; nova-gate [41260350170](https://github.com/nv-lang/nova/actions/runs/41260350170), job `124712137011`, cancelled. Annotation: `The job has exceeded the maximum execution time of 45m0s`. Это не зелёный результат и не доказанный дефект компилятора.

| Фаза | 135 | Зелёный 41251850324 | Зелёный 41237514624 |
|---|---:|---:|---:|
| check-cache, с | 989 | 735 | 953 |
| emitted-unique после emission-size, с | 508.66 | 377.01 | 488.31 |
| self-accepted, с | 55.72 | 43.54 | 53.52 |
| par_run после self-accepted, с | снят после 562.38 | 475.44 | 621.78 |
| следующий heavy, с | не начат | около 215 | около 259 |

Последняя завершённая проверка — self-accepted в 01:51:43Z, отмена в 02:01:05Z. Незавершённый шаг — `par_run` novac-behaviour, не emitted-unique/self-accepted. На зелёных критический путь пула — differential (475/622с); конкретная незавершённая фикстура исходного CI **не установлена**: вывод и времена пула печатаются после общего wait, partial artifacts отсутствуют. Между зелёным main `311397da…` и кандидатом нет изменений workflow/gate-novac/differential/cache/novac/spec_tests.

## Минимальная правка наблюдаемости

- `par_snapshot`: только активные `<N>.cmd/.out/.rc/.sec/.start`; status queued/started/completed, elapsed и существующий rc (unknown для незавершённого), без выдуманного результата. Snapshot в `target/novac-partial/`, дублирование в CI stdout. Произвольный env/дерево не публикуются.
- Существующий EXIT trap сохраняет исходный rc, делает snapshot до release lock. TERM/INT переходят в EXIT с 143/130; незавершённый verdict помечен INTERRUPTED. Штатный cleanup пула не меняется. SIGKILL перехватить невозможно; snapshot при нём не обещан.
- Shell-guard пишет start до запуска; Python runner пишет start/sec и сбрасывает partial `.out` при каждой записи, а не только после выхода.
- Workflow: дешёвый изолированный самотест перед gate и always upload диагностического каталога. Стражи, лимиты и skip не изменены.

## Проверки до push

`test-gate-novac-par-partial.py`: извлечены настоящие функции пула/EXIT-trap; быстрый, медленный и падающий участники. Success rc=0, failure rc=1, TERM rc=143. При отмене: fast completed, slow started с partial без конца, red completed с исходным rc=7 и sec; у всех start/out; lock release выполнен, verdict не зелёный. Отдельный Python-участник: partial виден до завершения, при terminate ложного rc нет; штатный red оставляет rc=7, runner rc=1.

На Linux/WSL: новый mock/Python PASS; существующий par-abort 11/11; interrupted-verdict 5/5 (включая живой TERM, RC143 INTERRUPTED=1). Тяжёлые гейты не запускались. На Git Bash новый mock/Python и par-abort проходят, старый live interrupted-verdict давал отсутствие verdict; диагностическая копия с сохранённым temp прошла. Это платформенная оговорка, не объявленная зелёной проверка Windows live-кейса; тест не ослаблялся.

Следующий diagnostic CI должен дать конкретные незавершённые guard/status/elapsed/partial output при отмене. Это наблюдение, не ускорение гейта и не разрешение landing. SHA и единственный run id сообщаются интегратору после push без изменения этого документа на замороженном SHA.

САБОТАЖ: временный `return 0` в начале par_snapshot делает новый самотест RED (snapshot отсутствует). Исходные байты восстановлены в finally; повторный штатный самотест GREEN (success/failure/cancel/Python partial).
