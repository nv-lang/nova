# -*- coding: utf-8 -*-
"""scripts/tools/gate-profile-steps.py — куда в ярусе гейта уходит время.

Читает ЛЮБОЙ лог гейта (`scripts/gate.sh`, `scripts/gate-novac.sh` — оба ставят
на каждую строку отметку `[ Ns]` от старта) и печатает десять самых дорогих
шагов. Стоимость шага = отметка следующего шага минус своя.

ЗАЧЕМ. Вопрос «гейт стал медленный, что именно подорожало?» до сих пор
отвечался догадкой, и догадка ошибалась. Замер 2026-09-06, два прогона яруса
push на одной машине:

    шаг                    прогон A       прогон B
    crate-tests               421с (30%)      68с (5%)
    conformance-full          243с (17%)     365с (27%)
    mega-CU                   204с (15%)     285с (21%)
    ИТОГО                    1391с          1377с

Итог почти совпал, а состав — нет: в A кэш крейт-тестов был холодный, в B
тёплый. **Холодный кэш ПЕРЕРАСПРЕДЕЛЯЕТ стоимость между шагами, а итог не
меняет** — и это ровно тот вывод, который без пофайлового профиля сделать
нельзя, а без него интегратор (я) объяснил перебор бюджета «холодным кэшем»
и ошибся. Три шага дают 62–65% яруса в ОБОИХ прогонах: `conformance-full`,
`mega-CU`, `crate-tests`; первые два прогоняют корпус conformance дважды
разными способами.

ИСПОЛЬЗОВАНИЕ:
    python scripts/tools/gate-profile-steps.py /tmp/gate_full.log
    python scripts/tools/gate-profile-steps.py <любой сохранённый лог>

Без аргумента берёт `/tmp/gate_full.log` — путь, куда пишет
`scripts/tools/gate-bg.sh`.

ЧЕГО НЕ ДЕЛАЕТ: не судит, не краснеет, базы не имеет. Это измерительный
инструмент, а не страж: число шагов и их цена меняются законно, и подпирать
их храповиком значило бы запретить гейту расти вместе с корпусом. Решение,
что делать с дорогим шагом, принимает человек.
"""
import io
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

P = sys.argv[1] if len(sys.argv) > 1 else "/tmp/gate_full.log"
ANSI = re.compile(r"\x1b\[[0-9;]*m")
# у части строк отметка стоит ДВАЖДЫ (обёртка awk в gate-bg.sh) — второй
# необязательный кусок в шаблоне именно про это.
STEP = re.compile(r"^\[\s*(\d+)s\]\s+(?:\[\s*\d+s\]\s+)?== gate: (.+?) ==\s*$")

steps = []
last_ts = 0
for raw in io.open(P, encoding="utf-8", errors="replace"):
    line = ANSI.sub("", raw.rstrip("\n"))
    m2 = re.match(r"^\[\s*(\d+)s\]", line)
    if m2:
        last_ts = max(last_ts, int(m2.group(1)))
    m = STEP.match(line)
    if m:
        steps.append([int(m.group(1)), m.group(2)])

if not steps:
    print("нет строк шагов в %s — это лог гейта?" % P)
    raise SystemExit(0)

rows = []
for i, (ts, name) in enumerate(steps):
    end = steps[i + 1][0] if i + 1 < len(steps) else last_ts
    rows.append((end - ts, ts, name))

total = last_ts
if total <= 0:
    print("в логе нет отметок времени — профилировать нечего")
    raise SystemExit(0)

rows.sort(reverse=True)
print("прогон: %s, всего %dс, шагов %d" % (P, total, len(steps)))
print("\nдесять самых дорогих шагов:")
acc = 0
for cost, ts, name in rows[:10]:
    acc += cost
    print("  %5dс  %5.1f%%  [старт %4dс]  %s" % (cost, 100.0 * cost / total, ts, name[:76]))
print("\nсумма первых десяти: %dс (%.0f%% яруса)" % (acc, 100.0 * acc / total))
