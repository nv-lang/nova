# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-second-door.py — вторая дверь ловится ФОРМОЙ,
а не именем.

ПОЧЕМУ ЭТОТ СТРАЖ ПОЯВИЛСЯ (владелец, 2026-08-18: «почему у нас опять
прорываются вторые двери?»). Разбор в тот же день дал ответ, который стоит
записать целиком, потому что он объясняет и все прежние случаи:

  ДВЕРЬ ИЩУТ ПО ИМЕНИ. Не знаешь имени — пишешь своё. Единственный машинный
  страж этого класса (`check-novac-one-door-export.sh`) в своей же шапке
  признаёт, что умеет ровно половину: «одно ИМЯ — из одного модуля». Две двери
  с РАЗНЫМИ именами — хоть в одном модуле, хоть в разных — ему невидимы.

Замер того дня: `emit_c/fn_row_of_decl` и `mono/fn_row_of` совпадали ТЕЛО В
ТЕЛО (1.00), и вторую написал автор этих строк шестью часами ранее, снабдив её
комментарием «тот же вопрос, что задаёт mono». Вежливость к читателю не заменяет
двери: вопрос «строка вызываемого по имени» не имел двери в `sem` вообще, и
поэтому его переписывали все, кому он был нужен — десять мест в трёх модулях.

ЧТО ЭТО ЗНАЧИТ ДЛЯ ПРАВИЛА. Вторую дверь надёжно выдаёт не имя, а ФОРМА:
  (1) два тела, совпадающие почти дословно, — копипаст двери;
  (2) вопрос ЧУЖОГО реестра, заданный мимо его двери (двухшаговый
      `defs.find(...)` + `defs.rows[...]` вне `sem/`), — дверь, которой нет.

ПРОВЕРЯЕТ novac/src/**/*.nv (тесты исключены):
  * (1) — жёстко: любая пара функций с >= MIN_LINES значащих строк и
    совпадением >= SIM_RATIO — красный. Сегодня таких ноль, поэтому порог
    держится нулём;
  * (2) — ХРАПОВИКОМ: число таких функций не должно расти. Жёстко нельзя:
    семь оставшихся мест спрашивают реестр СЛОЖНЕЕ («какой это вид, и что
    делать с вариантом»), и запрет без двери на этот вопрос был бы запретом
    работать. Храповик двигают вниз по мере появления дверей.

НЕ ПРОВЕРЯЕТ: смысловые дубли, у которых разошлась форма (это ревью приёмки —
и страж честно говорит, что их не видит); двери в shell-скриптах.

Аргументы: [корень репозитория] [--update-baseline]
"""
import pathlib
import re
import sys

# Вывод строго UTF-8: на Windows труба берёт cp1251, и тогда собственные
# сообщения стража приходят мозаикой — самотест не находит в них своих строк,
# а человек не читает причину (поймано первым же прогоном самотеста).
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-second-door"
MIN_LINES = 6
SIM_RATIO = 0.9
BASELINE = "scripts/guards/novac-second-door.baseline"


def funcs(path):
    lines = path.read_text(encoding="utf-8", errors="replace").split("\n")
    out, cur, body, start = [], None, [], 0
    for i, l in enumerate(lines, 1):
        if re.match(r"^(export )?fn ", l):
            if cur:
                out.append((cur, start, body))
            cur, body, start = l.split("(")[0].strip(), [], i
        elif cur:
            body.append(l)
    if cur:
        out.append((cur, start, body))
    return out


def significant(body):
    sig = [re.sub(r"\s+", " ", b).strip() for b in body]
    return [s for s in sig if s and not s.startswith("//")]


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    src = root / "novac" / "src"
    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = [p for p in sorted(src.rglob("*.nv")) if not p.name.endswith("_test.nv")]
    if not files:
        print(f"{NAME} ok: судить нечего (в novac/src файлов .nv: 0)")
        return 0

    all_funcs, two_step = [], []
    for p in files:
        rel = p.relative_to(src).as_posix()
        for name, line, body in funcs(p):
            sig = significant(body)
            txt = "\n".join(body)
            if len(sig) >= MIN_LINES:
                all_funcs.append((rel, line, name, set(sig)))
            if p.parent.name != "sem" and "defs.find(" in txt and "defs.rows[" in txt:
                two_step.append(f"{rel}:{line} {name}")

    dupes = []
    for i in range(len(all_funcs)):
        for j in range(i + 1, len(all_funcs)):
            a, b = all_funcs[i], all_funcs[j]
            ratio = len(a[3] & b[3]) / max(len(a[3]), len(b[3]))
            if ratio >= SIM_RATIO:
                dupes.append((ratio, a, b))

    base_path = root / BASELINE
    if "--update-baseline" in sys.argv:
        base_path.write_text(f"{len(two_step)}\n", encoding="utf-8")
        print(f"{NAME}: база храповика записана: {len(two_step)}")
        return 0
    base = int(base_path.read_text(encoding="utf-8").strip()) if base_path.is_file() else len(two_step)

    bad = []
    for ratio, a, b in dupes:
        bad.append(f"  {a[0]}:{a[1]} {a[2]}")
        bad.append(f"      <-> {b[0]}:{b[1]} {b[2]}  (совпадение {ratio:.2f})")
    if len(two_step) > base:
        bad.append(f"  вопросов реестра мимо его двери: {len(two_step)} (база {base}) — РОСТ")
        for t in two_step:
            bad.append(f"      {t}")

    if bad:
        print(f"{NAME}: FAIL — вторая дверь: одна операция написана дважды", file=sys.stderr)
        for line in bad:
            print(line, file=sys.stderr)
        print("  Дверь ищут по ИМЕНИ: не нашёл — напишешь свою. Заведи дверь в том", file=sys.stderr)
        print("  модуле, которому принадлежит ОТВЕТ, и позови её из обоих мест.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: функций сверено {len(all_funcs)}, дословных копий 0, "
          f"вопросов реестра мимо двери {len(two_step)} (база {base}, не растёт)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
