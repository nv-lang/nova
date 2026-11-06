# Задача 141 — Замер корпуса 0.3: все примеры и пакеты Кариной

Дата замера: 2026-11-05. База: ветка задачи от main `0af45b45a`, HEAD дерева
замера `0b01c838b57f4056000eeb46987252e91dcdad71` (diff novac/ std/
compiler-codegen/ nova-driver/ nova-cli/ examples/ против main пуст).
Платформа: Windows, clang; Boehm/libuv из vendored подмодулей.

## Provenance компилятора (критерий 2)

**Карина из HEAD, собранная оракулом** (умолчание задания, вариант (а) +
«делай по рекомендации» владельца 2026-11-05): oracle nova-cli release
(sha256 `6876c9d1…6388`) собрал `novac/src/main.nv` HEAD →
novac141.exe (sha256 `d0ab736a…6460`), упакован в локальный сид
`nova-carina-0.3.0-141head-windows-x86_64.zip` (sha256 `f2647039…3832`,
**не опубликован**), дескриптор `stage: oracle-a`,
`compiler_provenance: oracle:0b01c838b…`. Полные хэши: `evidence/manifests.md`.

**Разрыв с нормой поколений сид — фиксируется честно (требование задания).**
Норма: сид поколения N собирается сидом поколения N-1 (опубликованным).
Здесь сид собран оракулом (Rust-компилятором), то есть замер измеряет HEAD
ценой отступления от цепочки самосборки. Каждый прогон это объявлял:
`verify-seed` проходил с `--allow-oracle-stage-a --oracle-reason "task 141:
oracle-built Carina from HEAD 0b01c838b for the 0.3 distance measurement;
generation-norm deviation recorded in the task report"` — NOTICE есть в
каждом логе сборки (`evidence/logs-118/`, `evidence/logs-packages/`).
Официальный `docs/dev/carina-seeds/` не тронут; дескриптор жил только в
scratch-инсталляции. Почему не (б): seed-поколение v0.2.1 на pinned-дереве
измеряет не HEAD (реестр №1960), а задание — расстояние до 0.3 на HEAD.

## Сводная таблица расстояния до 0.3

| Корпус | Входов | PASS | FAIL | Классы FAIL | Δ к 135 |
|---|---|---|---|---|---|
| examples/ (118) | 118 | **63** (62 + 1 новый) | 55 | SUBSET 45 · LANG 6 · LINK-NO-MAIN 4 · ICE 0 | **+1 PASS** (string_literals), −1 SUBSET, остальное без изменений |
| пакеты (runnable) | 19 | 0 | 19 | SUBSET 18 · DEP 1 (nova-sdl, remote-not-found) | 0 — 19/19 класс-в-класс |
| пакеты (библиотеки) | 5 | — | — | NOT-MEASURED: bignum, compress, http, socks, tls — нет runnable entry (область 0.4) | без изменений |

**Расстояние до 0.3 = 55 отказов примеров + 19 отказов пакетных входов**,
все — адресные отказы подмножества/языка/линковки и один недоступный
репозиторий; **ICE — 0**. Доля PASS примеров: 63/118 = 53.4% (135: 62/118 =
52.5%).

## Сверка со снимком 135 (критерий 4)

- Те же входы: 118 .nv == table-118.tsv 135 (diff 0 до прогона); HEAD'ы 9
  спутников == 135 (проверены до прогона, `packages.tsv` колонка head).
- **PASS-множество 135 ⊂ PASS-множество 141 целиком; потерянных PASS нет.**
  Разница поимённо: `+examples/basics/string_literals.nv` (в 135 был
  E_NOVAC_SUBSET — parser fallback).
- Классы отказов: 135 финал 46/6/4 → 141 45/6/4; разница ровно
  string_literals, перешедший в PASS.
- **Первые диагностики всех 55 FAIL байт-в-байт совпали с логами 135**
  (после нормализации путей): `evidence/fail-diagnostics-118.tsv`,
  same_as_135=SAME ×55. Пакеты: 19/19 первая диагностика совпала.
- Оба намеренных rc=101 (`neg_overflow_panics`, `requires_violation_names_line`)
  соответствуют принятым кодам этих же входов в 135/128; остальные 61 PASS —
  run rc=0, первая строка stdout совпала с ожиданием 135 (где записано).

## PASS-NEW: сверка с оракулом → новая строка реестра

`string_literals.nv`: Карина build rc=0, run rc=0, вывод разумный; оракул
того же HEAD (build rc=0, run rc=0) даёт **отличающийся stdout**: в
многострочном backtick-литерале не-блочной формы (открывающий backtick НЕ
сразу за переводом строки) Карина нормализует `\r\n`→`\n`, оракул хранит
байты исходника. D467 §7 п.4 норму нормализации записывает только для
блочной формы — край спеки не назван. Улика:
`evidence/oracle-check-string-literals.md`; дефект заведён в
`docs/plans/221.1-bug-sweep.md` строкой **№TBD** (🟡 К2, P2) этим же мержем.
Вердикт входа остаётся PASS-NEW (критерий — build rc=0 + run с ожидаемым rc;
ожидаемого stdout у входа не было), расхождение честно названо.

## Отказы без PASS: классификация (критерий 5)

Корзины первых диагностик (наблюдаемые сообщения, не доказанные общие корни;
полная таблица носителей — `fail-diagnostics-118.tsv`):

**Примеры (55):**
- E_NOVAC_SUBSET 45, из них крупные корзины: parser fallback 13 ·
  «fn declares a return type but its body ends without a value» 6 ·
  static-method ambiguity (E2-b3) 4 · effect runtime pieces в shell 4
  (Counter×3, Audit×1) · type form (slices/tuples/fn types, E2-b) 3 ·
  effect attribute 3 · прочие по одному-два (spread, generics E2-b2, `[]T.new`
  с capacity, handler literal, `#impl` кроме Display/Equal/Serialize, формат
  `${expr:?}`, extern nova fn, bare `None` без типа, interop Result в shell,
  unknown name).
- E_NOVAC_LANG 6: `() as int` (D54) ×2 — это носители №1929 (норма требует
  приёма, разобрано в отчёте 135); `break`/`continue` вне цикла ×2
  (agent_runner, pty_session); `defer` не собирается (D90, долг E2) ×1
  (consume_tour); коллизия имён вариантов (D406/№1929) ×1.
- LINK-NO-MAIN 4: nested_spawn_scope_var, tour/greeter core+loud,
  unsafe_fn_keyword — Carina C не содержит `main` (класс из 135, без
  изменений).

**Пакеты (19):** 18 E_NOVAC_SUBSET (первая диагностика — parser fallback или
tail-without-value на ранней форме dep'ов; класс в класс с 135) + nova-sdl
DEP: `remote-not-found` (репозиторий не опубликован — как в 135).

**Новых классов отказов, ICE и регрессий против 135 нет.**

## Прогоны (критерий 6)

Оба прогона — через машинную очередь (`crew_watch machine:true`), один файл
за раз, без повторных прогонов: 118 примеров — 25 мин (SUMMARY
`{"FAIL": 55, "PASS": 62, "PASS-NEW": 1}`); 19 пакетных входов — 2 мин
(`{"FAIL": 19}`). Дымовая проверка контура до прогонов: hello.nv build rc=0 →
run rc=0 «Hello, Nova!».

## Улики (критерий 3)

`evidence/`: table-118.tsv, fail-diagnostics-118.tsv, logs-118/ (118
нормализованных логов), packages.tsv, logs-packages/ (19), manifests.md
(sha256), oracle-check-string-literals.md, scratch-driver-141.patch,
satellite-lock-status.md, README.md (указатель). Машинные пути обезличены
($WT/$S/$INSTALL/$REPOS/$TMP/$NOVA_HOME); сырые байты — в scratch сессии.
Состояние спутников после замера совпало с записью 135 (polaris — прежний
дрейф 12 lock; duckdb — 3 untracked lock тем же содержимым; остальные чисты).
Ничего не откатывалось.

## Строка результата (§6 window-brief-template)

```text
РЕПРО: не применимо (замер, не фикс) — снимок 135 подтверждён полностью:
  PASS-множество 135 ⊂ 141, все 55+19 первых диагностик совпали дословно
ТОЧКА: замер HEAD 0b01c838b Кариной oracle-built (stage oracle-a,
  разрыв нормы поколений зафиксирован выше)
ФИКС: нет (измерение); новая строка реестра: docs/plans/221.1-bug-sweep.md №TBD
  (CRLF в не-блочном backtick: Карина ≠ оракул, D467 §7 молчит)
ФИКСТУРЫ: не добавлялись (запрет задания: не чинить); негативные входы
  языка уже в spec_tests
САБОТАЖ: контроль классификатора — 55/55 FAIL имеют build rc≠0 и не пустой
  класс; PASS-NEW перепроверен оракулом (обнаружено value-расхождение,
  заведено); повторных прогонов без изменения не было
std/src: ДО == ПОСЛЕ — std не тронут (install141: junction на std HEAD дерева)
ВЕТКА/КОММИТ: task/141-zamer-korpusa-0-3-vse-primery-i-pakety-k <SHA при сдаче>
ЧТО НЕ СДЕЛАНО: 5 библиотечных пакетов (bignum, compress, http, socks, tls)
  не измерены — нет runnable entry (область 0.4, как в 128/135); nova-sdl
  не собран — репозиторий не опубликован (DEP, как в 135)
```
