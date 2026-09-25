# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-oracle-tax-link.py — связь между планом 274.10
(«Четырнадцать дефектов») и маркерами `[LEGACY-#NNN]` в novac/src, ОБЕ СТОРОНЫ.

ПОЧЕМУ ОТДЕЛЬНО ОТ check-novac-legacy-workarounds.py: тот страж сверяет маркер
против РЕЕСТРА №221.1 (закрыт ли баг) — это про фоссилизацию. Этот страж
сверяет маркер против СПИСКА плана 274.10 («Четырнадцать дефектов», раздел
«Что Карине стоит оракул») — это про УЧЁТ: план 274.10:557-560 сам называет
расхождение, найденное при сборке списка 2026-09-16 — счётчик по маркерам дал
ВОСЕМЬ багов, список — ЧЕТЫРНАДЦАТЬ (три описаны словами без маркера, №762/763/
895; и два маркированных числа — №700, №708 — не про налог оракула вовсе).
Без связи список 274.10 и код расходятся тем же способом, что уже разошлись
план и реестр (274.10:265-270): числа устаревают молча.

ДВЕ СТОРОНЫ:
  A (таблица → код). Строка «Четырнадцать дефектов» с непустой колонкой
    «маркер в коде» обязана найти этот маркер живым в novac/src. Маркер в
    таблице без маркера в коде — драфт, который никто не свёл.
  B (код → таблица). Каждый номерной `[LEGACY-#NNN]` в novac/src обязан быть
    ЛИБО одним из 14 номеров плана 274.10, ЛИБО в явном списке исключений
    ALLOWED_EXTRA ниже — с названной причиной. Немаркированное новое число —
    либо забыли вписать в план, либо это не про налог оракула, и это решает
    интегратор, не страж.

ALLOWED_EXTRA — решено интегратором, не выведено грепом:
  700 — дефект САМОЙ Карины (`ice()` на ошибке пользователя, novac/src),
        274.10:553 явно исключает её собственные дефекты из этого плана.
  708 — не баг оракула, а ОТСУТСТВУЮЩАЯ возможность (report()/cause(), D463,
        №680); строка 221.1 №708 сама говорит «БЛОКИРУЕТ ТЕГ: НЕТ», чинится
        планом 173.4 Ф.3, не 274.10. Открытый вопрос 274.10:693-695 («кому:
        интегратор (сам)», срок «до шага 1») решён этим же коммитом: исключён.

НЕ ПРОВЕРЯЕТ: закрыт ли баг в реестре №221.1 (это Rule A соседнего стража);
снят ли обход в коде Карины (274.10:272-275 — обход держит окно 274 отдельной
работой, его наличие ничего не говорит о починке); правдивость колонки
«мест обхода» (число мест не сверяется — только САМ факт маркера).

$1 — корень репозитория.
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-oracle-tax-link"
NUM_RE = re.compile(r"\[LEGACY-#([0-9]+)[^\]]*\]")
DEFECT_NUM_RE = re.compile(r"№([0-9]+)")

# см. docstring выше — решение интегратора, не вывод из грепа.
ALLOWED_EXTRA = {
    "700": "дефект самой Карины (ice() на ошибке пользователя) — 274.10:553",
    "708": "не баг оракула, а недостающая возможность (D463/№680, план 173.4 Ф.3) — 274.10:693-695",
}


def walk_novac_text(src):
    """Тот же обход, что у check-novac-legacy-workarounds.py: бинарные файлы
    (нулевой байт) не судятся — маркер в бинаре grep -o тоже не видит."""
    chunks = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in sorted(names):
            p = pathlib.Path(dirpath) / nm
            try:
                data = p.read_bytes()
            except OSError:
                continue
            if b"\0" in data:
                continue
            chunks.append((str(p).replace("\\", "/"), data.decode("utf-8", "replace")))
    return chunks


def parse_defect_table(text):
    """Возвращает (table_nums: set[str], marker_of: dict[str, str|None]) из
    раздела «Четырнадцать дефектов» — между его заголовком и следующим `---`."""
    lines = text.replace("\r", "").split("\n")
    start = None
    for i, l in enumerate(lines):
        if l.startswith("## Четырнадцать дефектов"):
            start = i
            break
    if start is None:
        return None, None

    rows = []
    for l in lines[start + 1:]:
        if l.strip() == "---":
            break
        if l.startswith("## "):
            break
        if l.startswith("|") and "---" not in l:
            rows.append(l)

    table_nums = set()
    marker_of = {}
    for row in rows:
        cells = [c.strip() for c in row.strip("|").split("|")]
        if len(cells) < 4:
            continue
        header_like = cells[0] in ("порядок", "---")
        if header_like:
            continue
        defect_cell = cells[1]
        marker_cell = cells[3]
        nums = DEFECT_NUM_RE.findall(defect_cell)
        if not nums:
            continue
        marker_text = None
        mc = marker_cell.strip("`").strip()
        if mc and mc != "нет":
            marker_text = mc
        for n in nums:
            table_nums.add(n)
            marker_of[n] = marker_text
    return table_nums, marker_of


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    src = root / "novac"
    plan = root / "docs" / "plans" / "274.10-oracle-tax-on-carina.md"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (novac ещё нет)")
        return 0
    if not plan.is_file():
        print(f"{NAME}: FAIL — плана {plan} нет, список 274.10 не сверить", file=sys.stderr)
        return 1

    plan_text = plan.read_text(encoding="utf-8", errors="replace")
    table_nums, marker_of = parse_defect_table(plan_text)
    if table_nums is None:
        print(f"{NAME}: FAIL — раздел '## Четырнадцать дефектов' в {plan} не найден", file=sys.stderr)
        print("  Заголовок переименован или удалён — список 274.10 судить нечем.", file=sys.stderr)
        return 1
    if not table_nums:
        print(f"{NAME}: FAIL — таблица под '## Четырнадцать дефектов' пуста или не разобрана", file=sys.stderr)
        return 1

    chunks = walk_novac_text(src)
    whole = "\n".join(t for _p, t in chunks)

    bad = 0

    # --- Сторона A: таблица -> код -------------------------------------------
    for n in sorted(table_nums, key=int):
        marker_text = marker_of.get(n)
        if not marker_text:
            continue
        needle = "[" + marker_text
        if needle not in whole:
            print(f"{NAME}: FAIL — план 274.10 называет маркер `{marker_text}` "
                  f"для №{n}, а в novac/src его нет", file=sys.stderr)
            print("  Обход сняли (и забыли обновить таблицу) или таблица", file=sys.stderr)
            print("  называет маркер, который никогда не был написан.", file=sys.stderr)
            bad = 1

    # --- Сторона B: код -> таблица --------------------------------------------
    code_nums = set()
    for path, text in chunks:
        if "LEGACY-#" not in text:
            continue
        for ln, line in enumerate(text.replace("\r", "").split("\n"), 1):
            for m in NUM_RE.finditer(line):
                n = m.group(1)
                code_nums.add(n)
                if n not in table_nums and n not in ALLOWED_EXTRA:
                    print(f"{NAME}: FAIL — [LEGACY-#{n}] есть в novac ({path}:{ln}), "
                          f"а в списке «Четырнадцать дефектов» плана 274.10 и в "
                          f"ALLOWED_EXTRA его нет", file=sys.stderr)
                    print("  Впиши строку в план 274.10, либо, если это не налог", file=sys.stderr)
                    print("  оракула (дефект самой Карины / недостающая", file=sys.stderr)
                    print("  возможность), добавь номер и причину в ALLOWED_EXTRA", file=sys.stderr)
                    print("  этого стража.", file=sys.stderr)
                    bad = 1

    extra_seen = sorted(n for n in code_nums if n in ALLOWED_EXTRA)
    marked_in_table = sorted((n for n in table_nums if marker_of.get(n)), key=int)
    counter = (f"дефектов в плане {len(table_nums)}, с маркером в коде {len(marked_in_table)}, "
               f"маркеров в novac {len(code_nums)}, исключений {len(extra_seen)}")

    if bad == 0:
        print(f"{NAME} ok: {counter}")
        return 0
    print(f"{NAME}: {counter}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
