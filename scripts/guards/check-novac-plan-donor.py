# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-plan-donor.py — в ТЕКСТЕ плана и конвенций novac
оракул не назван донором (П25 / П27 п.2а; заведён 2026-08-23).

ЗАЧЕМ. Запрет «оракул донором быть не может» существовал с 2026-08-16 и был
подкреплён двумя стражами — `check-novac-module-donor` (шапки модулей) и
`check-novac-commit-donor` (сообщения коммитов). Ни один не читает ПЛАН. Из-за
этого в `docs/plans/274-novac-self-hosted-compiler.md` строка «донор: оракул
держит ABI тегом на том же объявлении» прожила от 2026-08-20 до 2026-08-23 при
зелёном гейте: нашёл её владелец глазами, спросив «а не написано ли у нас
где-то, что оракулом пользоваться можно?». Класс — нормативное правило без
механизма над одним из своих носителей.

ЧТО ПРОВЕРЯЕТ: в отслеживаемых `docs/plans/274*.md` и `docs/dev/novac-*.md` нет
строки, ПРИПИСЫВАЮЩЕЙ форму оракулу: `донор` рядом с `оракул` / `нынешний
компилятор` / `nova-cli` / `compiler-codegen` / `emit_c`, в любом регистре, и то
же для английского `Donor:`.

ПОЧЕМУ СПИСКОМ РАЗРЕШЁННЫХ, А НЕ УМНЫМ РЕГЕКСПОМ. Сам запрет записан теми же
словами («Оракул донором быть НЕ МОЖЕТ»), и датированная поправка тоже цитирует
снятую форму. Отличать утверждение от цитаты регекспом — гадание; поэтому
законные вхождения перечислены в `novac-plan-donor.allow` (путь `|` подстрока
строки + причина комментарием). Список тоже под храповиком: запись, которая
больше ни к чему не подходит, — красная, иначе протухший список молча пропустит
следующее нарушение.

НЕ ПРОВЕРЯЕТ: сообщения коммитов и шапки модулей (у них свои стражи); прочие
планы — правило про семью novac.

$1 — корень; $2 — override файла разрешений (шов самотеста).
"""
import pathlib
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-plan-donor"

ORACLE = r"(?:оракул\w*|oracle|нынешн\w+\s+компилятор\w*|nova-cli|compiler-codegen|emit_c)"
DONOR = r"(?:донор\w*|donor)"
HIT = re.compile(r"%s.{0,80}?%s|%s.{0,80}?%s" % (DONOR, ORACLE, ORACLE, DONOR),
                 re.IGNORECASE)

PATTERNS = ("docs/plans/274*.md", "docs/dev/novac-*.md")


def tracked(root):
    out = subprocess.run(["git", "-C", str(root), "ls-files", "--"] + list(PATTERNS),
                         capture_output=True, text=True, encoding="utf-8")
    return [p for p in out.stdout.replace("\r", "").split("\n") if p.strip()]


def load_allow(path):
    """Список разрешённых: `путь|подстрока`. Комментарии и пустые — мимо."""
    rows = []
    if not path.is_file():
        return rows
    for raw in path.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "|" not in line:
            rows.append((None, line))
            continue
        f, sub = line.split("|", 1)
        rows.append((f.strip(), sub.strip()))
    return rows


def main(argv):
    root = pathlib.Path(argv[1] if len(argv) > 1 else ".").resolve()
    allow_path = pathlib.Path(argv[2]) if len(argv) > 2 else root / "scripts" / "guards" / "novac-plan-donor.allow"
    allow = load_allow(allow_path)

    files = tracked(root)
    if not files:
        print("%s ok: судить нечего (git не отдал файлов плана/конвенций novac)" % NAME)
        return 0

    used = set()
    bad = []
    for rel in files:
        p = root / rel
        if not p.is_file():
            continue
        lines = p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")
        for i, line in enumerate(lines, 1):
            if not HIT.search(line):
                continue
            hit_allowed = False
            for idx, (f, sub) in enumerate(allow):
                if (f is None or f == rel) and sub and sub in line:
                    used.add(idx)
                    hit_allowed = True
                    break
            if not hit_allowed:
                bad.append((rel, i, line.strip()[:120]))

    stale = [allow[i][1][:60] for i in range(len(allow)) if i not in used]

    if bad or stale:
        print("%s FAIL: оракул назван донором в тексте (П25/П27 2а)" % NAME)
        for rel, i, text in bad:
            print("  %s:%d: %s" % (rel, i, text))
        for s in stale:
            print("  разрешение ни к чему не подходит (протухло): %s" % s)
        print("  Законные доноры формы: rustc, Go, Swift, Zig, Roslyn, clang/LLVM, "
              "Koka, статьи. Нет донора — честное `Donor: none — причина`.")
        print("  Законную цитату запрета вносить строкой в %s с причиной."
              % allow_path.name)
        return 1

    print("%s ok: файлов проверено %d, приписок оракулу 0, разрешений живых %d (П25)"
          % (NAME, len(files), len(allow)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
