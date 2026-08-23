# -*- coding: utf-8 -*-
"""Ядро check-commit-refs.sh: ссылки на коммиты во внутренней доке.

ЧТО СЧИТАЕТ — три правила конвенции `docs/dev/doc-conventions.md`, раздел
«Ссылки наружу» (решение владельца 2026-08-14):

  R1  ссылка на коммит в вебе (`github.com/nv-lang/*/commit/<hash>`) обязана
      нести в той же строке ТЕМУ (в кавычках) и ДАТУ — иначе после
      переписывания истории она даёт вечный 404 и, в отличие от голого хеша,
      выглядит авторитетно и потому не перепроверяется;
  R2  голый хеш в обратных кавычках обязан быть ДОСТИЖИМ ИЗ `main` — не просто
      существовать локально: объект живёт, пока его держит страховочная метка,
      а для свежего клона он мёртв уже сейчас;
  R3  ссылка на наш код идёт на GitHub — зеркала в ссылках не чередуются,
      иначе выбор делается заново на каждой ссылке. Строки, которые говорят О
      САМИХ зеркалах (таблица зеркал в README), из правила исключены: там URL
      зеркала и есть предмет, а не способ сослаться на код.

ПОЧЕМУ ПИТОН, А НЕ ГРЕП. Достижимость — не текстовая проверка: нужен полный
список коммитов `main` и сопоставление ПРЕФИКСОВ (в доке хеши по 7-11 знаков).
Первая редакция звала `git` на каждый хеш и не уложилась в две минуты; эта
берёт список один раз и укладывается в три секунды на 2769 хешах — урок №475:
страж, тормозящий гейт, будет отключён.

ИСПОЛЬЗОВАНИЕ: python commit-refs-scan.py [РЕПА] [КОРЕНЬ-СКАНА]
Второй аргумент отделён от первого ради самотеста: достижимость считается по
настоящей репе, а текст берётся из временной фикстуры.

Вывод: по строке на находку, `правило|путь:номер|подробность`.
"""
import io
import os
import re
import subprocess
import sys

# Хеш в обратных кавычках: 7-40 шестнадцатеричных. Требуем хотя бы одну букву
# a-f — иначе в находки попадают десятичные числа, которых в реестре полно
# (адреса и размеры вроде `2884744404960`).
BARE = re.compile(r"`([0-9a-f]{7,40})`")
HEXY = re.compile(r"[a-f]")
COMMIT_URL = re.compile(r"github\.com/nv-lang/[A-Za-z0-9_.-]+/commit/[0-9a-f]{7,40}")
MIRROR_URL = re.compile(r"(gitverse\.ru|sourcecraft\.dev)", re.I)
# Строка ПРО зеркала, а не ссылка на код через зеркало.
MIRROR_TOPIC = re.compile(u"mirror|зеркал", re.I)
DATE = re.compile(r"20\d\d-\d\d-\d\d")
SUBJ = re.compile(u"[«\"][^»\"]{8,}[»\"]")

ZONES = ("docs/plans", "docs/dev", "spec", "AGENTS.md", "CLAUDE.md", "README.md")
SKIP_NAMES = ("commit-refs-scan.py", "check-commit-refs.sh")


def reachable_shas(repo):
    """Хеши, достижимые ХОТЬ ОТКУДА: `main`, `origin/main`, `HEAD` — ОБЪЕДИНЕНИЕ.

    Два замера 2026-08-23, оба на этом страже:

    1. На PR-чекауте CI локальной `main` нет вовсе. `rev-list main` возвращал
       пусто, и страж печатал FAIL, НИЧЕГО не проверив, — вердикт без проверки,
       худший вид красноты.
    2. Правило ловит МЁРТВЫЙ хеш — тот, которого нет нигде: опечатку или
       переписанную историю. Хеш невлитой ветки не мёртв; он существует, и после
       слияния станет достижим из main. Собственный текст стража это признаёт
       («либо ветка не влита, либо историю переписали»), но считал оба случая
       одинаково — и рабочая ветка, ссылающаяся на свои же коммиты, краснела за
       то, что ещё не влита.

    Отсюда объединение: тревога остаётся ровно на «нет нигде».
    """
    shas = set()
    for ref in ("main", "origin/main", "HEAD"):
        out = subprocess.run(["git", "-C", repo, "rev-list", ref],
                             capture_output=True, text=True).stdout
        shas |= set(l.strip() for l in out.split("\n") if l.strip())
    return shas


def collect(scan_root):
    files = []
    for z in ZONES:
        p = os.path.join(scan_root, z)
        if os.path.isfile(p):
            files.append(p)
        elif os.path.isdir(p):
            for base, dirs, fs in os.walk(p):
                # Исключение `agent-memory` СНЯТО 2026-08-21 вместе с самой
                # выгрузкой: она удалена из репозитория, и периметр вернулся к
                # полному. Держать сужение под удалённый каталог значит копить
                # слепые зоны, о которых через месяц никто не вспомнит.
                dirs[:] = [d for d in dirs if d not in (".git", "target")]
                files += [os.path.join(base, f) for f in fs
                          if f.endswith(".md") and f not in SKIP_NAMES]
    return sorted(files)


def main():
    repo = sys.argv[1] if len(sys.argv) > 1 else "."
    scan_root = sys.argv[2] if len(sys.argv) > 2 else repo

    shas = reachable_shas(repo)
    if not shas:
        # Пустой список — НЕ «нарушений нет», а «git не ответил». Ровно на этом
        # молчании прошёл вхолостую шаг гейта про секреты (реестр №645).
        sys.stderr.write("commit-refs-scan: git не отдал список коммитов main\n")
        return 2
    prefixes = {}
    for s in shas:
        prefixes.setdefault(s[:7], []).append(s)

    def reachable(h):
        return any(full.startswith(h) for full in prefixes.get(h[:7], ()))

    out = io.open(sys.stdout.fileno(), "w", encoding="utf-8", newline="\n",
                  closefd=False)
    for f in collect(scan_root):
        try:
            text = io.open(f, encoding="utf-8").read()
        except Exception:
            continue
        rel = os.path.relpath(f, scan_root).replace("\\", "/")
        for i, line in enumerate(text.split(u"\n"), 1):
            if COMMIT_URL.search(line) and not (DATE.search(line) and SUBJ.search(line)):
                out.write(u"R1|%s:%d|ссылка на коммит без темы и даты рядом\n"
                          % (rel, i))
            if MIRROR_URL.search(line) and not MIRROR_TOPIC.search(line):
                out.write(u"R3|%s:%d|ссылка на зеркало вместо github\n" % (rel, i))
            for m in BARE.finditer(line):
                h = m.group(1)
                if HEXY.search(h) and not reachable(h):
                    out.write(u"R2|%s:%d|хеш %s недостижим из main\n" % (rel, i, h))
    out.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
