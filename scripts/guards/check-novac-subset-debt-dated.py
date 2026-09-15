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

ЧТО СУДИТСЯ: строковый литерал, которым novac ОТКАЗЫВАЕТ как вне подмножества, в
`.nv` под `novac/src`. Законная запись несёт этап в скобках — `(E2-b3)`, `(E3)`,
`(E4)` или волну `(274.7 B1b)` — то есть отвечает на вопрос «когда этого отказа не
станет».

ОБЛАСТЬ РАСШИРЕНА 2026-09-13 (окно 274), и это исправление ЛОЖНОЙ ЗЕЛЕНИ. Эта шапка
говорила «not compiled yet (И ЕГО ФОРМЫ)», а образец в коде ловил ровно одно
написание. Вторая формулировка того же предмета — «outside the subset: ...» — ему не
видна, и ею записано БОЛЬШИНСТВО отказов дерева. Обещание «новый бессрочный отказ
красит гейт сразу» для неё не работало: окно, написавшее «outside the subset: ...»
без этапа, проходило зелёным. Улика лежала в шапке БАЗЫ: её собственный образец
законного долга — «outside the subset: ... is not compiled yet (E2-b3)» — несёт ОБА
написания, и ловился он только из-за второго. Узкая область была случайностью, а не
решением. Образцы теперь живут в ОДНОМ доме `novac_subset_debt.py`, который
импортирует и страж коммита `check-novac-commit-no-simplification.py`.

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
# ОБРАЗЦЫ — НЕ ЗДЕСЬ. Их дом `novac_subset_debt.py`, потому что то же правило нужно
# стражу КОММИТА: две копии разошлись бы на первой правке (реестр №988 — ровно этот
# класс, и там вынос в один дом обошёл двух зовущих из трёх).
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import novac_subset_debt as _sd  # noqa: E402
RE_DEBT = _sd.RE_DEBT
# Срок — оттуда же и по той же причине. Форма «(274.7 B1b)» заведена 2026-09-05 по
# решению владельца (274 §1.3а): срок долга теперь волна 274.7, и она называет момент
# точнее этапа; голая ссылка на план сроком НЕ считается.
RE_STAGE = _sd.RE_STAGE


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    base = pathlib.Path(a[3]) if len(a) > 3 else (
        pathlib.Path(__file__).resolve().parent / "subset-debt.baseline")

    # ОТСУТСТВУЮЩАЯ МИШЕНЬ НЕ ЗЕЛЁНАЯ (правка 2026-09-13). Здесь стояло «нет директории —
    # ok, судить нечего»: звучит разумно («это не novac-дерево»), а даёт зелёный ровно
    # там, где страж обязан кричать — `novac/src` может исчезнуть и в настоящем прогоне.
    # Образец семьи (`check-novac-prim-id-compare.py`, названный таковым в
    # `check-guard-empty-root`) отсутствующую директорию НЕ выделяет вовсе: нет файлов —
    # «мишень потеряна», одним правилом на оба случая. Пустой каталог краснел здесь и
    # раньше; соседняя ось — отсутствующий — была открыта, и мета-страж её не судит.

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            # _test.nv исключены: тест ЦИТИРУЕТ сообщения отказов в
            # утверждениях, и цитата — не долг. Всплыло 2026-08-31 при разрезе
            # pipeline_test.nv: тестовая строка стала «13-м бессрочным долгом».
            # Тот же класс, что у процитированного в прозе маркера [M-...], и
            # то же лекарство, что у check-novac-string-build-door.
            if nm.endswith("_test.nv"):
                continue
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

    # СЧЁТ ПО ДВЕРИ (В11-б): признак машинный -- через какую дверь течёт текст, то есть
    # какой код получит пользователь. Область проверена перебором 2026-09-14: все вызовы
    # дверей живут в `check/`, поэтому счёт по папке и по дереву совпадают.
    door_files = {}
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        door_files[rel] = f.read_bytes().decode("utf-8", "replace")
    door_subset, door_lang = _sd.by_door(door_files)
    door_undated = [r for r in door_subset if not r[3]]
    # Сколько из них НЕВИДИМЫ счёту по написанию -- это и есть расхождение двух мерок.
    door_invisible = [r for r in door_undated if not _sd.RE_DEBT.search(r[2])]

    want = None
    want_all = None
    want_door = None
    if base.is_file():
        for line in base.read_text(encoding="utf-8", errors="replace").split("\n"):
            line = line.strip()
            if line.startswith("undated="):
                want = int(line.split("=", 1)[1])
            # ВТОРОЕ ЧИСЛО, предписанное 274.7 §И и не построенное до 2026-09-13:
            # общее число отказов, храповик ВНИЗ с целью НОЛЬ (274 §1.3а п. «г»).
            # Без него датированный долг мог расти бесконечно: этап есть — и ладно,
            # а решение владельца требует не датировать отклонения, а УБИРАТЬ их.
            elif line.startswith("refusals="):
                want_all = int(line.split("=", 1)[1])
            # ТРЕТЬЕ ЧИСЛО (В11-б, 2026-09-14): бессрочные ПО ДВЕРИ. Оно мерит ПРЕДМЕТ --
            # какой код получит читатель отказа, -- тогда как два числа выше мерят
            # НАПИСАНИЕ текста. Держать надо это; два первых остаются как мера того,
            # насколько отказы понятны человеку.
            elif line.startswith("undated_by_door="):
                want_door = int(line.split("=", 1)[1])
    if want is None:
        print(f"{NAME}: FAIL — нет базы {base}: храповик без базы ничего не держит", file=sys.stderr)
        return 1

    if want_door is not None and len(door_undated) > want_door:
        print(f"{NAME}: FAIL — бессрочных ПО ДВЕРИ стало БОЛЬШЕ: "
              f"{len(door_undated)} > базы {want_door}", file=sys.stderr)
        for path, line_no, body, _ in door_undated[:12]:
            print(f"  {path}:{line_no} — {body[:78]}", file=sys.stderr)
        print("  Счёт по ДВЕРИ мерит предмет: через какую дверь течёт текст, то есть", file=sys.stderr)
        print("  какой код получит пользователь. Написание мерит, понятен ли отказ", file=sys.stderr)
        print("  человеку, и слепо к тексту, который написания не несёт.", file=sys.stderr)
        return 1

    total = dated + len(undated)
    if want_all is not None and total > want_all:
        print(f"{NAME}: FAIL — отказов подмножества стало БОЛЬШЕ: "
              f"{total} > базы {want_all}", file=sys.stderr)
        print("  Цель храповика — НОЛЬ (274 §1.3а п. «г»): отказ подмножества есть",
              file=sys.stderr)
        print("  отклонение от спеки и подлежит переделке, а не датированию навсегда.",
              file=sys.stderr)
        print("  Новый отказ законен, только если он ЗАМЕНЯЕТ прежний, а не добавляется",
              file=sys.stderr)
        print("  к ним: реализуй форму по спеке либо сними другой отказ той же волной.",
              file=sys.stderr)
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
          f"бессрочных {len(undated)} (база {want}){extra}; "
          f"всего против базы роста: {total} <= {want_all}; "
          f"ПО ДВЕРИ: отказов {len(door_subset)}, бессрочных {len(door_undated)} "
          f"(база {want_door}), постоянных ошибок на своём коде {len(door_lang)}, "
          f"из бессрочных БЕЗ канонического написания {len(door_invisible)}; "
          f"область: {len(_sd.SPELLINGS)} формулировки отказа, дом образца — novac_subset_debt.py")
    return 0


if __name__ == "__main__":
    sys.exit(main())
