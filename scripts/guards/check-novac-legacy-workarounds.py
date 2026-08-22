# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-legacy-workarounds.py — форма обхода багов оракула
(легаси-компилятора) в novac/** нормирована (план 274 §1.5).

Имя маркера — LEGACY, в словаре проекта («легаси» = текущий компилятор, корзина
bug-legacy реестра расхождений).

ПРАВИЛА (план 274 §1.4/§1.5)
  A (фоссилизация): каждый номерной маркер [LEGACY-#NNN] обязан ссылаться на
    баг, чья строка есть в реестре №221.1 и НЕ закрыта. Маркер закрытого бага —
    красный: фикс-волна оракула снимает обходы той же волной.
  B (атрибуция): самоистекающий EXPECT_CC_ERROR — это обход бага оракула;
    файл-носитель обязан нести маркер, иначе счётчик носителей слеп к нему.
  C (номер не бывает вечным «TBD»): носитель [LEGACY-#TBD-<slug>] старше
    TBD_MAX_DAYS суток — красный: это не «пока нет номера», а зависшая заявка.
  D (срок наступил): форма `until:<этап>` самоистекающая — «самоистекающая» без
    машины означало «автор вспомнит».

ЧЕТЫРЕ ЖИВЫЕ ФОРМЫ НОСИТЕЛЯ, а не две (адверсарная проверка 2026-08-17):
  [LEGACY-#677]                              — номерная
  [LEGACY-#701-export-ro-vector]             — номерная со слагом
  [LEGACY-#700-user-error-as-ice until:E2b3] — со слагом и СРОКОМ
  [LEGACY-#TBD-slug]                         — заявка на номер
Прежний регексп требовал `]` сразу за цифрами и судил пять из пятнадцати.
Хвост после номера намеренно не разбирается: `[^]]*` принимает и слаг, и
`until:`, и то, что придумают завтра.

ВОЗРАСТ маркера — от ПЕРВОГО появления слага в истории, а не от даты строки:
`git blame` обнулял часы при любой правке строки (274.3/F7).

ПЕЧАТАЕТ ВСЕГДА строку-счётчик носителей — машинное число «налога оракула» для
вехи недели §1.4, и на зелёном, и на красном прогоне.

НЕ ПРОВЕРЯЕТ: обходы без маркеров (словесный обход машине не виден); честность
самих EXPECT_* (их держит раннер); СМЫСЛ слова ЗАКРЫТ в строке реестра —
детектор грубый и привязан к МАРКЕРУ СТАТУСА, а не к слову где угодно: голое
«ЗАКРЫТ» красило прозу о ЧУЖОМ баге; возраст #TBD в файле, которого git не
знает.

ПОЧЕМУ PYTHON: shell-редакция поднимала четыре рекурсивных grep по дереву с
бинарём на 31 МБ плюс процесс на каждый номер — 2.3с (П14).

$1 — корень репозитория.
env NOVA_LEGACY_TBD_TIME — самотестовая дверь: подменяет определение даты строки
на эту метку unix-времени для ВСЕХ #TBD-носителей. ГРОМКАЯ (274.3/F7).
"""
import os
import pathlib
import re
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-legacy-workarounds"
MARK_RE = re.compile(r"\[LEGACY-#(?:TBD-[A-Za-z0-9._-]+|[0-9]+)[^]]*\]")
NUM_RE = re.compile(r"\[LEGACY-#([0-9]+)[^]]*\]")
UNTIL_RE = re.compile(r"\[LEGACY-#[^]]*until:[A-Za-z0-9]+\]")
TBD_MAX_DAYS = 3
STAGE_ORDER = ["E1", "E2", "E2b1", "E2b2", "E2b3", "E3", "E4", "E5", "E6"]

CLOSED_RE = re.compile(
    r"(Статус:[ \t]*(ЗАКРЫТ|Закрыт|закрыт)"
    r"|✅[^|]{0,40}(ЗАКРЫТ|ЗАКРЫТО|ЗАКРЫТА|Закрыт|закрыт|CLOSED|Closed|closed|DONE|Done|done))")
NOT_CLOSED_RE = re.compile(r"(не закрыт|НЕ ЗАКРЫТ|Не закрыт)")


def walk_text_files(src):
    """Файлы дерева, как их видел `grep -r`: бинарные (с нулевым байтом) не
    дают вывода с -o, и здесь они так же не судятся."""
    for dirpath, _dirs, names in os.walk(src):
        for nm in sorted(names):
            p = pathlib.Path(dirpath) / nm
            try:
                data = p.read_bytes()
            except OSError:
                continue
            if b"\0" in data:
                continue
            yield p, data.decode("utf-8", "replace")


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    src = root / "novac"
    reg = root / "docs" / "plans" / "221.1-bug-sweep.md"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (novac ещё нет)")
        return 0
    if not reg.is_file():
        print(f"{NAME}: FAIL — реестра {reg} нет, маркеры не сверить", file=sys.stderr)
        return 1

    hits = []                      # (путь как у grep -r, номер строки, маркер)
    expect_files = []
    texts = {}
    for p, text in walk_text_files(src):
        shown = str(p).replace("\\", "/")
        texts[shown] = text
        if "EXPECT_CC_ERROR" in text:
            expect_files.append(shown)
        if "LEGACY-#" not in text:
            continue
        for n, line in enumerate(text.replace("\r", "").split("\n"), 1):
            for m in MARK_RE.finditer(line):
                hits.append((shown, n, m.group(0)))

    # `grep -rnoE ... | sort`: порядок находок — байтовый по строке «путь:номер:маркер»
    hits.sort(key=lambda h: f"{h[0]}:{h[1]}:{h[2]}")

    bad = 0
    now = int(time.time())
    seam = os.environ.get("NOVA_LEGACY_TBD_TIME", "")

    marks = len(hits)
    tbd = [h for h in hits if "[LEGACY-#TBD-" in h[2]]
    tbd_n = len(tbd)
    num_n = marks - tbd_n
    files_n = len({h[0] for h in hits})

    # --- Правило C: возраст заявок на номер ---------------------------------
    oldest = -1
    for f, ln, marker in tbd:
        if seam:
            # Самотестовая дверь — ГРОМКАЯ (274.3/F7): подмена времени обязана
            # быть видна в логе гейта, иначе правило C глушится молча.
            print(f"{NAME}: ВНИМАНИЕ — возраст #TBD ПОДМЕНЁН через "
                  f"NOVA_LEGACY_TBD_TIME (самотест)", file=sys.stderr)
            t = seam
        else:
            out = subprocess.run(["git", "-C", str(root), "log", "-S" + marker,
                                  "--format=%at", "--", str(src)],
                                 capture_output=True).stdout.decode("utf-8", "replace").split()
            t = out[-1] if out else ""
            if not t:
                bl = subprocess.run(["git", "-C", str(root), "blame", "-L", f"{ln},{ln}",
                                     "--porcelain", "--", f],
                                    capture_output=True).stdout.decode("utf-8", "replace")
                mm = re.search(r"^author-time ([0-9]+)$", bl, re.M)
                t = mm.group(1) if mm else ""
        if not t:
            continue
        age = (now - int(t)) // 86400
        if age < 0:
            age = 0
        oldest = max(oldest, age)
        if age > TBD_MAX_DAYS:
            print(f"{NAME}: FAIL — {marker} живёт без номера {age} сут "
                  f"(порог {TBD_MAX_DAYS}): {f}:{ln}", file=sys.stderr)
            print("  номер обязан прийти от интегратора: эскалируй.", file=sys.stderr)
            bad = 1

    # --- Правило A: номерные маркеры против реестра -------------------------
    reg_lines = reg.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")
    nums = sorted({m for _f, _n, mk in hits for m in NUM_RE.findall(mk)})
    for n in nums:
        rows = [l for l in reg_lines if l.startswith(f"| {n} |")]
        if not rows:
            print(f"{NAME}: FAIL — [LEGACY-#{n}] есть в novac, а строки | {n} | в реестре нет",
                  file=sys.stderr)
            bad = 1
            continue
        blob = "\n".join(rows)
        if CLOSED_RE.search(blob) and not NOT_CLOSED_RE.search(blob):
            print(f"{NAME}: FAIL — [LEGACY-#{n}] жив в novac, а баг №{n} в реестре ЗАКРЫТ:",
                  file=sys.stderr)
            shown = 0
            for path in sorted(texts):
                if shown >= 5:
                    break
                for i, line in enumerate(texts[path].replace("\r", "").split("\n"), 1):
                    if f"[LEGACY-#{n}]" in line:
                        print(f"    {path}:{i}:{line}", file=sys.stderr)
                        shown += 1
                        if shown >= 5:
                            break
            print("  Фикс-волна оракула снимает обходы той же волной (§1.5):", file=sys.stderr)
            print(f"  греп [LEGACY-#{n}] по novac на ноль — часть её приёмки.", file=sys.stderr)
            bad = 1

    # --- Правило B: самоистекающая форма обязана нести атрибуцию ------------
    for f in sorted(expect_files):
        if not MARK_RE.search(texts[f]):
            print(f"{NAME}: FAIL — {f} несёт EXPECT_CC_ERROR без маркера "
                  f"[LEGACY-#NNN] / [LEGACY-#TBD-<slug>]", file=sys.stderr)
            print("  Самоистекающий обход — тоже обход: без атрибуции счётчик", file=sys.stderr)
            print("  носителей (веха недели, §1.4/§1.5) его не видит.", file=sys.stderr)
            bad = 1

    # --- Правило D: срок `until:<этап>` наступил ----------------------------
    untils = [(f, n, m.group(0)) for f, n, mk in hits for m in [UNTIL_RE.search(mk)] if m]
    untils.sort(key=lambda h: f"{h[0]}:{h[1]}:{h[2]}")
    if untils:
        # Мерка нужна ровно тогда, когда есть кого мерить: обратный порядок
        # красил фикстуры самотеста, у которых нет nova.toml и нет срочных
        # маркеров — страж судил там, где судить нечего.
        stage_now = ""
        toml = src / "nova.toml"
        if toml.is_file():
            for line in toml.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
                m = re.match(r"^#[ \t]*stage:[ \t]*([A-Za-z0-9]*)$", line)
                if m:
                    stage_now = m.group(1)
                    break
        now_idx = STAGE_ORDER.index(stage_now) + 1 if stage_now in STAGE_ORDER else 0
        if now_idx == 0:
            print(f"{NAME}: FAIL - носители со сроком until: есть ({len(untils)}), "
                  f"а этап в novac/nova.toml не прочитан ('{stage_now}'): судить нечем", file=sys.stderr)
            bad = 1
        else:
            for f, n, marker in untils:
                u = marker.rsplit("until:", 1)[1].rstrip("]")
                u_idx = STAGE_ORDER.index(u) + 1 if u in STAGE_ORDER else 0
                hit = f"{f}:{n}:{marker}"
                if u_idx == 0:
                    print(f"{NAME}: FAIL - маркер называет этап '{u}', которого нет в порядке "
                          f"({' '.join(STAGE_ORDER)}): {hit}", file=sys.stderr)
                    bad = 1
                elif now_idx >= u_idx:
                    print(f"{NAME}: FAIL - обход дожил до своего этапа: срок until:{u}, "
                          f"текущий этап {stage_now}", file=sys.stderr)
                    print(f"  {hit}", file=sys.stderr)
                    print("  Самоистекающий обход истёк - снимай его или переназначай срок явно.",
                          file=sys.stderr)
                    bad = 1

    # --- Итог: счётчик печатается всегда ------------------------------------
    if tbd_n == 0:
        age_txt = "нет"
    elif oldest >= 0:
        age_txt = f"{tbd_n}, старейший {oldest} сут при пороге {TBD_MAX_DAYS}"
    else:
        age_txt = f"{tbd_n}, возраст н/д (git даты не дал)"
    counter = (f"налог оракула — носителей {marks} в {files_n} файлах; "
               f"из них #TBD: {age_txt}; номерных {num_n} на {len(nums)} багов реестра")

    if bad == 0:
        print(f"{NAME} ok: {counter}")
        return 0
    print(f"{NAME}: {counter}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
