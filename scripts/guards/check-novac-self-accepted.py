# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-self-accepted.py — храповик меры 0.2 Карины:
МНОЖЕСТВО файлов собственного исходника (`novac/src`), которое самосборка
ОТВЕРГАЕТ, сверено с базой `novac-self-accepted.baseline` в обе стороны
(реестр 221.1 №1665; решение интегратора 2026-10-02, пересмотр решения
2026-09-25 «мера, а не храповик» — см. шапку scripts/tools/novac-self-residual.sh).

ПОВОД. 2026-10-02 пять файлов (`sem/collect.nv`, `sem/mangle.nv`,
`sem/mangle_test.nv`, `sem/binding_test.nv`, `pipeline/handed_free_test.nv`),
принимавшихся в 00:23, к утру стали отвергаться — и этого не увидел никто до
ручного замера соседнего окна вечером, потому что мера 0.2 была числом, которое
никто не сравнивал. Храповик по МНОЖЕСТВУ файлов краснеет в первом же прогоне
яруса novac. Сама мера ДИАГНОСТИК (novac-self-residual.sh) остаётся мерой —
этот страж держит множество отвергнутых, не число диагностик.

СПОСОБ — ровно тот, что у меры 0.2 (scripts/tools/double-build.sh и
novac-self-residual.sh, консенсус 2026-09-23 «one rung, one measure»): ОДИН
процесс `NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src novac check` по
`novac/src/*/*.nv novac/src/*.nv`; отвергнутый файл — тот, что назван полем
`"file"` хотя бы одной диагностики. Бинарь Карины — ТОЛЬКО через дверь
`novac_bin` (реестр №1607, страж check-novac-bin-door).

КРАСНЫЙ (exit 1):
  (а) отвергнут файл, которого нет в базе — печатается имя файла и ПЕРВАЯ
      диагностика по нему. Регресс самосборки — чинить; законное вскрытие
      (проверка стала глубже и показала скрытое) — поднять базу ТЕМ ЖЕ
      слиянием, строкой-летописью с причиной;
  (б) файл из базы теперь принят, а база не сужена — иначе следующий рост до
      прежнего множества прошёл бы молча (та же двусторонность, что у
      check-novac-surface);
  (в) ICE (E_NOVAC_ICE) больше нуля — ICE обрывает проход и прячет всё за
      собой, счёт с ICE не мера и не база;
  (г) база не читается, пуста или бита (строка не путь `novac/src/**/*.nv`).

«ВЕРДИКТА НЕТ» (exit 3, слова `ok` нет): бинаря Карины нет, процесс снят
пределом времени, код возврата не 0/1/2 (паника — не вердикт компилятора,
lib/novac.sh F3), вывод не разобран. На пустом корне (нет novac/src) — тоже
«вердикта нет», а не зелёный ноль (реестр №911).

ШВЫ САМОТЕСТА: $2 — `--from-file <файл>`: вывод Карины читается из файла,
бинарь не зовётся; `--baseline <файл>` — override базы. Самотест:
selftest/test-check-novac-self-accepted.sh.

ЦЕНА: один процесс novac над ~110 файлами — секунды (замер в REPORT-p1665.md);
внутренний предел 240с, чтобы поспеть сказать «вердикта нет» раньше внешнего
предела шага гейта.

$1 — корень репозитория. Вход для гейта — `main()` (run-guards.py зовёт её
в общем процессе).
"""
import os
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "lib"))
from novac_bin import novac_bin  # noqa: E402 -- #1607: the one door choosing Carina's binary

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-self-accepted"
TIMEOUT = 240

FILE_RE = re.compile(r'"file":"([^"]*)"')
MSG_RE = re.compile(r'"message":"([^"]*)')


def fail(msg, lines=()):
    print(f"{NAME}: FAIL — {msg}", file=sys.stderr)
    for l in lines:
        print(l, file=sys.stderr)
    return 1


def no_verdict(msg):
    print(f"{NAME}: ВЕРДИКТА НЕТ — {msg}", file=sys.stderr)
    return 3


def norm_path(raw):
    """Путь из поля "file" к виду базы: novac/src/<...>.nv, прямые слэши."""
    p = raw.replace("\\", "/")
    i = p.rfind("novac/src/")
    return p[i:] if i >= 0 else p


def parse_output(text):
    """Множество отвергнутых файлов, счёт ICE, первые сообщения по файлу."""
    refused, first_msg = {}, {}
    ice = text.count("E_NOVAC_ICE")
    for line in text.split("\n"):
        m = FILE_RE.search(line)
        if not m:
            continue
        p = norm_path(m.group(1))
        if p not in first_msg:
            mm = MSG_RE.search(line)
            first_msg[p] = mm.group(1) if mm else "(без текста)"
        refused[p] = True
    return set(refused), ice, first_msg


def read_baseline(path):
    """Множество путей базы; None — не читается; ('broken', строка) — бита."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    out = set()
    for raw in text.replace("\r", "").split("\n"):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if not (line.startswith("novac/src/") and line.endswith(".nv")) or " " in line:
            return ("broken", line)
        out.add(line)
    return out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    from_file, base_path = None, root / "scripts" / "guards" / "novac-self-accepted.baseline"
    i = 2
    while i < len(a):
        if a[i] == "--from-file" and i + 1 < len(a):
            from_file = pathlib.Path(a[i + 1]); i += 2
        elif a[i] == "--baseline" and i + 1 < len(a):
            base_path = pathlib.Path(a[i + 1]); i += 2
        else:
            i += 1

    src = root / "novac" / "src"

    base = read_baseline(base_path)
    if base is None:
        return fail(f"нет базы {base_path}: храповику меры 0.2 не с чем сверять множество отвергнутых")
    if isinstance(base, tuple):
        return fail(f"база {base_path} бита: строка не путь novac/src/**/*.nv: {base[1]!r}")
    if not base:
        return fail(f"база {base_path} пуста: храповик без множества не судит ничего")

    if from_file is not None:
        try:
            text = from_file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            return no_verdict(f"шов --from-file не читается: {from_file}")
        nfiles = -1
    else:
        if not src.is_dir():
            return no_verdict(f"нет {src}: меры 0.2 на этом дереве не существует, и это не «зелёно»")
        novac = novac_bin(root)
        if not pathlib.Path(novac).is_file():
            return no_verdict(f"нет бинаря Карины ({novac}): гейт обязан собрать её шагом novac-build (274.3/F1)")
        files = sorted(str(p.relative_to(root).as_posix())
                       for p in list(src.glob("*.nv")) + list(src.glob("*/*.nv")))
        if not files:
            return no_verdict(f"под {src} нет ни одного .nv: замерять нечего")
        nfiles = len(files)
        env = dict(os.environ, NOVAC_UNIT="1", NOVAC_SELF_PATH="novac/src")
        try:
            proc = subprocess.run([str(novac), "check"] + files, cwd=str(root),
                                  env=env, stdin=subprocess.DEVNULL,
                                  stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                  timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            return no_verdict(f"процесс novac check снят пределом {TIMEOUT}с: множества отвергнутых нет")
        text = proc.stdout.decode("utf-8", errors="replace")
        if proc.returncode not in (0, 1, 2):
            return no_verdict(f"novac check вышел кодом {proc.returncode} (не вердикт 0/1 и не дверь 2, lib/novac.sh F3)")
        if proc.returncode != 0 and '"code":' not in text:
            return no_verdict("novac check отказал, но вывод не разобран: ни одной диагностики по схеме §7")

    refused, ice, first_msg = parse_output(text)

    if ice > 0:
        return fail(f"ICE в самосборке: E_NOVAC_ICE x{ice} — счёт с ICE не мера и не база "
                    "(ICE обрывает проход и прячет всё за собой)")

    grew = sorted(refused - base)
    shrank = sorted(base - refused)
    if grew or shrank:
        lines = []
        for p in grew:
            lines.append(f"  РЕГРЕСС: отвергнут {p}, которого нет в базе — {first_msg.get(p, '')}")
        for p in shrank:
            lines.append(f"  БАЗА ПРОТУХЛА: {p} теперь принят, а в базе числится отвергнутым — сузи базу")
        lines.append("  Регресс самосборки — чинить (его чинят сессии Карины). Законное вскрытие")
        lines.append("  (проверка стала глубже и показала скрытое) — поднять базу ТЕМ ЖЕ слиянием,")
        lines.append("  строкой-летописью с датой, коммитом и причиной, как у novac-surface.baseline.")
        return fail("множество отвергнутых файлов самосборки разошлось с базой (реестр №1665):", lines)

    count = f" из {nfiles}" if nfiles > 0 else ""
    print(f"{NAME} ok: отвергнуто {len(refused)}{count} файлов самосборки, ровно множество базы, ICE 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
