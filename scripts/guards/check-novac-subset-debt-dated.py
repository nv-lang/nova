# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-subset-debt-dated.py — отказ «пока не компилируется»
несёт ЭТАП, к которому исчезнет
(план docs/plans/274.5-read-own-source.md, раздел о долге подмножества).

ПОЧЕМУ. Отказ подмножества — это ДОЛГ, а долг без срока становится нормой.
Проект уже держит два вида временного механизмом: временное ребро карты
(`check-novac-temp-edges`, у каждого этап, до которого оно законно) и обход бага
оракула (`check-novac-legacy-workarounds`, у каждого номер реестра). Отказы
подмножества выпадали из обоих — и выпадали ЗАКОНОМЕРНО: ребро и обход выглядят
временными, а отказ выглядит ПРАВИЛОМ. «Эта форма не компилируется» читается как
норма языка, а не как запись о невыплаченном.

ЗАМЕР, ЗАВОДЯЩИЙ СТРАЖА (2026-08-30, вопрос владельца): таких отказов в дереве
22, и срок не несёт НИ ОДИН. Меток с этапом во всём novac всего восемь, и все
восемь принадлежат рёбрам и ice-маркерам.

ЧТО СУДИТСЯ: строковый литерал, содержащий «not compiled yet» (и его формы), в
`.nv` под `novac/src`. Законная запись несёт этап в скобках — `(E2-b3)`, `(E3)`,
`(E4)` — то есть отвечает на вопрос «когда этого отказа не станет».

ХРАПОВИК, А НЕ ЗАПРЕТ: 22 существующих записи не переписываются одним слиянием —
это работа не одной волны. База держит число БЕССРОЧНЫХ, и оно ходит только вниз;
новый бессрочный отказ красит гейт сразу.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории; $3 — override базы.
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-subset-debt-dated"
# Долговая формулировка: «... not compiled yet», «... is read but not compiled yet».
RE_DEBT = re.compile(r'"[^"]*not compiled yet[^"]*"')
# Этап, к которому долг закрывается: (E1) (E2) (E2-b3) (E3) (E4) (E5) (E6)
RE_STAGE = re.compile(r"\(E[0-9][0-9a-zA-Z-]*\)")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    base = pathlib.Path(a[3]) if len(a) > 3 else (
        pathlib.Path(__file__).resolve().parent / "subset-debt.baseline")

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень", file=sys.stderr)
        return 1

    undated = []
    dated = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            s = line.lstrip(" \t\v\f")
            if s.startswith("//"):
                continue
            m = RE_DEBT.search(line)
            if not m:
                continue
            if RE_STAGE.search(m.group(0)):
                dated += 1
            else:
                undated.append(f"  {rel}:{n} — долг без этапа: {m.group(0)[:78]}")

    want = None
    if base.is_file():
        for line in base.read_text(encoding="utf-8", errors="replace").split("\n"):
            line = line.strip()
            if line.startswith("undated="):
                want = int(line.split("=", 1)[1])
    if want is None:
        print(f"{NAME}: FAIL — нет базы {base}: храповик без базы ничего не держит", file=sys.stderr)
        return 1

    if len(undated) > want:
        print(f"{NAME}: FAIL — бессрочных отказов подмножества стало БОЛЬШЕ: "
              f"{len(undated)} > базы {want}", file=sys.stderr)
        for u in undated[want:] if want < len(undated) else undated:
            print(u, file=sys.stderr)
        print("  Отказ подмножества — это ДОЛГ, а долг без срока становится нормой.", file=sys.stderr)
        print("  Назови этап, к которому его не станет, прямо в тексте: «... is not", file=sys.stderr)
        print("  compiled yet (E2-b3)». Так уже устроены временные рёбра карты и", file=sys.stderr)
        print("  обходы багов оракула; отказы выпадали из обоих только потому, что", file=sys.stderr)
        print("  выглядят правилом, а не записью о невыплаченном.", file=sys.stderr)
        return 1

    extra = ""
    if len(undated) < want:
        extra = f" — храповик можно опустить до {len(undated)}"
    print(f"{NAME} ok: долгов подмножества {dated + len(undated)}, из них с этапом {dated}, "
          f"бессрочных {len(undated)} (база {want}){extra}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
