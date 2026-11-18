# Задача 141-rerun — Перезамер корпуса 0.3 (все примеры Кариной)

Дата: 2026-11-18. База: `origin/main` = `155bc51f5b19cfdbec4048223ea7e68601a3b6ee`
(post-0.1.5 merge + Carina shell re-pin). Платформа: Windows, clang,
Boehm/libuv vendored. Исполнитель: окно «Исполнитель, Карина»
(sess_0860f005), по слову владельца «(а)» — старт сейчас, задача заводится
задним числом.

## Provenance компилятора

**Карина из main HEAD, собранная оракулом 0.1.5 из того же HEAD** (stage
oracle-a, отклонение от нормы поколений сид — как в 141, честно
фиксируется): oracle nova-cli release (sha256 `7b2b5b08…`, `nova 0.1.5`,
4м03с) собрал `novac/src/main.nv` → novac-030.exe (sha256 `2517fce9…`,
55с). Сид `nova-carina-0.3.0-030head-windows-x86_64.zip` (sha256
`47748d6b…`, НЕ опубликован), дескриптор oracle-a, provenance
`oracle:155bc51f5…`. Контур: **patched-scratch-driver-030head** — nova-driver
@ 155bc51f5 с пинами SEED_* на локальный сид (+`--allow-oracle-stage-a`
с reason на каждом запуске; верификатор PASS). Правки драйвера и
дескриптора — scratch, в git не попали, дерево восстановлено (clean).

## Прогон 118 примеров (14:51–16:50, ~2ч)

| Метрика | 141 (05.11, base 0af45b45a) | rerun (18.11, base 155bc51f5) |
|---|---|---|
| PASS | 63 | **63** |
| FAIL | 55 | **55** |
| Классы FAIL | SUBSET 45 / LANG 6 / LINK-NO-MAIN 4 | SUBSET 45 / LANG 6 / LINK 4 |
| Регрессии | — | **0** |
| Новых PASS | — | **0** (L048 string_literals остаётся PASS) |
| ICE | 0 | 0 |

Счёт дистанции НЕ ИЗМЕНИЛСЯ: волны между 05.11 и 18.11 (supervised-значение,
coalesce-отказы, K3/K6-охоты, 0.1.5-слияние и перепин шелла) не открыли
корпусу новых входов и не закрыли ни один FAIL.

## Фронт отказов СДВИНУЛСЯ ВГЛУБЬ (главная находка)

Из 55 FAIL первая диагностика совпала с 141 только у 12; у 43 она ДРУГАЯ —
не потому что сломалось, а потому что вход проходит дальше (0 регрессий,
то же множество FAIL). Примеры: L078–L085 (aggregator) и L101/L102 (tls)
в 141 умирали на «novac did not parse this» / static-methods, теперь —
«outside the subset: an effect attribute is read but not compiled yet»;
L086–L091 (http_proxy_chain) — «fn declares a return type but its body
ends without a value» → теперь E_NOVAC_SUBSET глубже по файлу; L104
(concurrency) — парсер прошёл, теперь «a range is a `for` head here, not
a value (E2-b)».

### Кластеры новых первых диагностик (кандидаты следующей волны)

1. **«an effect attribute is read but not compiled yet» — 10 входов**
   (aggregator ×8, tls ×2). Крупнейший кластер фронта.
2. «no such method in the declarations handed to novac» — 4+ (эффектные
   рантайм-куски в шелле: Counter/Audit).
3. E_NOVAC_SUBSET с diagnostic-JSON дальше по файлу — ~20 входов,
   разбить по сообщениям при планировании (полный список —
   fail-diagnostics-118.tsv).
4. LINK (4) — linker command failed (в 141 тот же класс, обёртка
   сообщения изменилась).

## Прогон 19 пакетных входов — NOT-MEASURED (контур, не компилятор)

Все 19 завершились «entry is not associated with a Nova workspace or
installation»: драйверу нужен вход внутри workspace; 141 решала это
scratch-инсталляцией со стейджингом пакетных программ (заметка «staged
under $S\install141\target/carina-build»), чей рецепт жил в scratch той
сессии и не сохранился. Данных о компиляторе эта нога не дала; standing
reference — 141: 19/19 FAIL (18 E_NOVAC_SUBSET + 1 DEP nova-sdl). Для
воспроизведения нужен рецепт стейджинга пакетов (у интегратора/владельца
или восстановить по логам logs-packages/ здесь).

## Улики (scratch-opencode/measure-0.3-1118/)

table-118.tsv (118 строк, тот же порядок L001–L118), fail-diagnostics-118.tsv
(55 строк: сообщение новое + 141 + SAME/DIFF), logs-118/ ×118,
packages.tsv (19, all NOT-MEASURED с причиной), logs-packages/ ×19,
run-118.py / run-pkgs.py / compare*.py (контур), seed zip + дескриптор.
Машинных путей в нормализуемых артефактах нет (логи сырые, в отчёт не идут).

## Вывод

Расстояние до 0.3 по корпусу НЕ сократилось (55/118 + 19 пакетных),
но фронт отказов уехал вглубь: подмножество переварило верхние двери.
Следующая волна по рангу: **эффектные атрибуты (#realtime/#cfg-семейство
«an effect attribute is read but not compiled yet», 10 входов)** —
крупнейший кластер; затем рантайм-куски эффектов в шелле (Counter/Audit),
затем разбор оставшихся ~20 SUBSET по сообщениям. Формы v0.3
(cancel:/parallel for/detach/Outcome) в топе отказов корпуса НЕ значатся —
их время придёт после срезания кластеров 1–2.

## Оговорки NOT-MEASURED (по вердикту интегратора, задача 151)

- Пакетная нога (19 входов) NOT-MEASURED: рецепт стейджинга 141 частично выжил (patch + install внутри workspace); нога измеряема и будет внесена в evidence 151 отдельным дополнением при исполнении.
- Doc-комментарий над as_value-дверью (5 строк поверх влитого дефолта 5c797a50d) не перенесён отдельным CI-циклом — доедет попутным конвоем.

## Provenance (манифест)

- oracle nova-cli 0.1.5 из HEAD: sha256 7b2b5b0810c254f229385ef9dd8b82ecda47b04bd03e2ce287b14061c206c4f0
- Carina novac-030.exe: sha256 2517fce966e0ced1842f83b55c2401b54ee2ac0543ad23c6dd2a8f643552565c
- seed zip: sha256 47748d6b7669a545dd5335c5a1c6af305444c892e6ed547b1807ce01d05c7a58 (локальный, не опубликован)
- patched driver: sha256 00ce2a862cfa0340d56d8e23f37e8b641fba02b8b18dc5aedbb1965b523e9b8c (scratch)
