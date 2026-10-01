# -*- coding: utf-8 -*-
"""scripts/guards/check-task-selection-order.py — порядок выбора задач в двух
командах интегратора (`/load-background`, `/cloud-task`) не съехал обратно.

Адрес: требование владельца 2026-10-01 («добавь самотест на эти требования в
команды»), вслед за его вопросом «какого тега? ты отдаёшь задачи, которые
убыстрят релиз Карины?». До правки список выбора начинался со строк реестра
`БЛОКИРУЕТ ТЕГ: 0.2 — ДА` — а это тег ОРАКУЛА, который в релиз не выходит, и
облачным сессиям уходили дефекты оракула без связи с Кариной. Порядок, который
страж держит, — это план 274: мера 0.2 и novac-gate (docs/plans/274-novac-self-hosted-compiler.md),
затем docs/plans/274.10-oracle-tax-on-carina.md и docs/plans/274.11-carina-release.md.

ЧТО СУДИТСЯ (по смысловым якорям и их ПОРЯДКУ, не по полному тексту — правка
формулировок вокруг не краснит):
  R1  load-background.md, «Выбор — по порядку»: ровно четыре пункта, в порядке
      (1) novac-gate + мера 0.2, (2) 274.10, (3) 274.11, (4) дефект оракула со
      словами «ТОЛЬКО с доказанной связью».
  R2  ни один из этих пунктов не выбирает по `БЛОКИРУЕТ ТЕГ`, и под списком
      стоит абзац «Поле `БЛОКИРУЕТ ТЕГ` реестра по выбору задачи НЕ судит».
  R3  раздел «САМОПРОВЕРКА ПЕРЕД ВЫДАЧЕЙ»: шесть пронумерованных вопросов
      (ступень; связь фактом; тег оракула не довод; не занято; раздел «Связь с
      релизом Карины»; «почему БЫСТРЕЕ и НАСКОЛЬКО»: причина и насколько) и образец строки `Самопроверка: …`.
  R4  cloud-task.md: строка порядка ссылается на /load-background и называет
      порядок «novac-gate или мера 0.2 → 274.10 → 274.11 → дефект оракула»;
      есть ссылка на САМОПРОВЕРКУ («шесть вопросов», «почему быстрее и насколько»);
      в обязательных разделах промпта есть пункт
      «1а. **Связь с релизом Карины:**».

ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно): что выданные задачи действительно выбраны
по порядку и что строка `Самопроверка:` в докладе правдива. Это судит человек.

usage: python scripts/guards/check-task-selection-order.py [КОРЕНЬ]
Самотест: scripts/guards/selftest/test-check-task-selection-order.sh
"""
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-task-selection-order"
LB = os.path.join(".claude", "commands", "load-background.md")
CT = os.path.join(".claude", "commands", "cloud-task.md")


def read(root, rel):
    with open(os.path.join(root, rel), encoding="utf-8", newline="") as f:
        return f.read().replace("\r\n", "\n")


def numbered(lines):
    """Пронумерованные строки `N. текст` -> [(N, текст)]."""
    out = []
    for ln in lines:
        m = re.match(r"^(\d+)\. (.*)$", ln)
        if m:
            out.append((int(m.group(1)), m.group(2)))
    return out


def check_lb(text, bad):
    lines = text.split("\n")
    # --- R1 / R2: список выбора ---
    start = next((i for i, l in enumerate(lines) if "Выбор — по порядку" in l), None)
    items = []
    if start is None:
        bad.append("R1: в load-background.md нет строки «Выбор — по порядку»")
    else:
        for l in lines[start + 1:]:
            if re.match(r"^\d+\. ", l):
                items.append(l)
            elif items:
                break
            elif l.strip() and not l.startswith("**"):
                break
        if len(items) != 4:
            bad.append("R1: в списке выбора %d пунктов, нужно ровно 4" % len(items))
        else:
            want = [
                ("novac-gate", "мер"),   # (1) novac-gate + мера 0.2 Карины
                ("274.10",),
                ("274.11",),
                ("дефект оракула", "ТОЛЬКО с доказанной связью"),
            ]
            for n, (it, anchors) in enumerate(zip(items, want), 1):
                miss = [a for a in anchors if a not in it]
                if n == 1 and "0.2" not in it:
                    miss.append("0.2")
                if miss:
                    bad.append("R1: пункт %d не несёт якорь(я) %s — порядок novac-gate/0.2 "
                               "-> 274.10 -> 274.11 -> дефект оракула ТОЛЬКО с связью нарушен: «%s»"
                               % (n, ", ".join(miss), it[:70]))
    # R2: выбор по БЛОКИРУЕТ ТЕГ
    for n, it in enumerate(items, 1):
        if "БЛОКИРУЕТ ТЕГ" in it:
            bad.append("R2: пункт %d списка выбирает по `БЛОКИРУЕТ ТЕГ`: «%s»" % (n, it[:70]))
    if not re.search(r"Поле `БЛОКИРУЕТ ТЕГ` реестра по выбору задачи НЕ судит", text):
        bad.append("R2: нет абзаца «Поле `БЛОКИРУЕТ ТЕГ` реестра по выбору задачи НЕ судит»")
    # --- R3: САМОПРОВЕРКА ---
    m = re.search(r"САМОПРОВЕРКА ПЕРЕД ВЫДАЧЕЙ", text)
    if not m:
        bad.append("R3: нет раздела «САМОПРОВЕРКА ПЕРЕД ВЫДАЧЕЙ»")
        return
    sec = text[m.start():]
    nxt = re.search(r"\n## ", sec)
    if nxt:
        sec = sec[:nxt.start()]
    qs = numbered(sec.split("\n"))
    if [n for n, _ in qs] != [1, 2, 3, 4, 5, 6]:
        bad.append("R3: в САМОПРОВЕРКЕ вопросы пронумерованы %s, нужно 1..6"
                   % [n for n, _ in qs])
    else:
        keys = [("Ступень",), ("Связь с Кариной",), ("Тег оракула", "не довод"),
                ("Не занято",), ("Связь с релизом Карины",),
                ("БЫСТРЕЕ", "НАСКОЛЬКО")]
        for (n, q), ks in zip(qs, keys):
            miss = [k for k in ks if k not in q]
            if miss:
                bad.append("R3: вопрос %d не несёт якорь(я) %s" % (n, ", ".join(miss)))
    q6 = re.search(r"(?m)^6\. .*$", sec)
    if q6:
        tail = sec[q6.end():]
        for part in ("**причина**", "**насколько**"):
            if part not in tail:
                bad.append("R3: в вопросе 6 нет обязательной части %s" % part)
    if not re.search(r"`Самопроверка: ", sec):
        bad.append("R3: нет образца строки `Самопроверка: …`")
    elif not re.search(r"`Самопроверка: [^`]*быстрее — ", sec):
        bad.append("R3: образец строки `Самопроверка: …` не содержит «быстрее — »")


def check_ct(text, bad):
    line = next((l for l in text.split("\n") if "load-background" in l and "novac-gate" in l), None)
    if line is None:
        bad.append("R4: нет строки порядка со ссылкой на /load-background и novac-gate")
    else:
        if not re.search(r"novac-gate.{0,40}0\.2.*274\.10.*274\.11.*дефект оракула.*связью", line):
            bad.append("R4: строка порядка не называет «novac-gate или мера 0.2 -> 274.10 "
                       "-> 274.11 -> дефект оракула только с доказанной связью»")
        if "САМОПРОВЕРК" not in line:
            bad.append("R4: в строке порядка нет ссылки на САМОПРОВЕРКУ")
        elif "шесть вопросов" not in line:
            bad.append("R4: ссылка на САМОПРОВЕРКУ не говорит «(шесть вопросов)»")
        if "почему быстрее и насколько" not in line:
            bad.append("R4: строка порядка не требует «почему быстрее и насколько»")
    m = re.search(r"(?m)^1а\. \*\*Связь с релизом Карины:\*\*(.*)$", text)
    if not m:
        bad.append("R4: нет пункта «1а. **Связь с релизом Карины:**» в обязательных разделах промпта")
    elif "ожидаемый выигрыш" not in m.group(1):
        bad.append("R4: пункт 1а не требует «ожидаемый выигрыш»")


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    bad = []
    for rel, fn in ((LB, check_lb), (CT, check_ct)):
        try:
            fn(read(root, rel), bad)
        except OSError as e:
            bad.append("нет файла %s: %s" % (rel, e))
    if bad:
        print("%s: FAIL — нарушено: %s" % (NAME, "; ".join(bad)))
        print("%s: FAIL" % NAME, file=sys.stderr)
        for b in bad:
            print("    %s" % b, file=sys.stderr)
        return 1
    print("%s ok: порядок novac-gate/0.2 -> 274.10 -> 274.11 -> оракул-со-связью, "
          "БЛОКИРУЕТ ТЕГ не судит, САМОПРОВЕРКА из 6 вопросов, cloud-task ссылается" % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
