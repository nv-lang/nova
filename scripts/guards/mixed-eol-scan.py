# -*- coding: utf-8 -*-
"""Ядро check-mixed-eol: файлы со СМЕШАННЫМИ окончаниями строк в РАБОЧЕМ ДЕРЕВЕ.

ЗАЧЕМ. Смешанные окончания ломают всё построчное: diff показывает правку там,
где её нет; счётчики строк расходятся между машинами; инструмент, считающий
позицию по строкам, и инструмент, считающий по байтам, дают разные ответы для
одного места. В редакторе файл при этом выглядит нормально.

ЧЕСТНО ПРО ПОВОД. Страж заведён 2026-08-18 после того, как inlay-подсказки LSP
поехали в смешанном `lex.nv`, и корреляция выглядела причиной. Синтетическая
проверка её ОПРОВЕРГЛА: тот же исходник в трёх видах (LF / CRLF / смешанно)
даёт одинаковые подсказки. Причина №709 НЕ найдена; этот страж закрывает свой
класс, а не тот.

ПОЧЕМУ НЕ ЛОВИТСЯ GIT'ОМ. `core.autocrlf=true`: в объекте файл однороден,
смешанным становится ПОСЛЕ выкладки, `git diff` пуст. Лечение — перевыкладка,
а не коммит. Проверяется РАБОЧЕЕ ДЕРЕВО.
"""
import io
import subprocess
import os
import sys

EXTS = (".nv", ".rs", ".md", ".sh", ".py", ".toml", ".c", ".h", ".txt",
        ".json", ".yml", ".yaml", ".baseline", ".list")
SKIP_DIRS = ("target", ".git", "node_modules", "vcpkg_installed", ".claude",
             "nova_tests.old", "out", "dist")
# Файлы, которые ХРАНЯТ образцы смешанных окончаний как данные.
SKIP_FILES = ("mixed-eol-scan.py", "check-mixed-eol.sh",
              "test-check-mixed-eol.sh")


def drop_ignored(root, files):
    """Выбросить то, что git ИГНОРИРУЕТ, — оно СГЕНЕРИРОВАНО, а не написано.

    ЗАЧЕМ (правка 2026-09-08, нашло окно 274). Страж покраснел на `scratch/dump.txt`
    — файле неотслеживаемом и игнорируемом (`.gitignore`, `/scratch*/`). Отбор здесь
    рукописный чёрный список каталогов, `.gitignore` он не читает, поэтому ЛЮБОЙ
    чужой дамп в рабочем дереве красит авторитетный ярус и блокирует пуш.

    И совет стража для такой находки НЕВЫПОЛНИМ: «перевыложить файл» для
    неотслеживаемого — восстанавливать нечего, а причина в шапке (`core.autocrlf`,
    «в объекте однороден») существует только у отслеживаемых.

    Это правило Г15 «проверка судит ПРЕДМЕТ, а не среду», и ВТОРОЙ носитель за
    сутки: тем же утром тем же лекарством починен `check-script-eol-pinned`,
    находивший 213 «нарушений» внутри чужого `.claude/cargo-target/`.

    Спрашиваем ОДНИМ процессом (Г2), вход БАЙТАМИ: текстовый режим допишет `\r` и
    замер испортится сам собой. Код возврата `check-ignore`: 0 — есть игнорируемые,
    1 — нет ни одного, 128 — ошибка; при ошибке НЕ выбрасываем ничего, потому что
    ложный отказ честнее тихой зелени на пустом списке.
    """
    if not files:
        return files
    # ПУТИ ВНУТРИ ПОДМОДУЛЕЙ — ВОН ДО ВОПРОСА (правка 2026-09-08).
    #
    # На полном списке (7796 путей) `git check-ignore --stdin` вернул 128:
    # `fatal: Pathspec '...libuv/.clang-tidy' is in submodule`. ОДИН такой путь
    # убивает ВЕСЬ вызов, и запасная ветка ниже честно не отсекала ничего
    # (замер: cand=7339, kept=7339) — то есть правка была бы мёртвой, а страж
    # выглядел бы починенным.
    #
    # Отсев подмодулей верен и ПО СУЩЕСТВУ, а не только как обход: вендоренный
    # чужой код не наш, его окончания строк — забота upstream, и наш вердикт
    # там ничего не значит.
    subs = []
    try:
        sp = subprocess.run(
            ["git", "-C", root, "config", "--file", ".gitmodules",
             "--get-regexp", "path"], capture_output=True)
        if sp.returncode == 0:
            for ln in sp.stdout.decode("utf-8", "replace").split("\n"):
                parts = ln.strip().split(None, 1)
                if len(parts) == 2 and parts[1]:
                    subs.append(parts[1].replace("\\", "/").rstrip("/") + "/")
    except Exception:
        subs = []
    if subs:
        files = [f for f in files if not any(f.startswith(x) for x in subs)]
        if not files:
            return files
    try:
        proc = subprocess.run(
            ["git", "-C", root, "check-ignore", "--stdin"],
            input=("\n".join(files) + "\n").encode("utf-8"),
            capture_output=True)
    except Exception:
        return files
    if proc.returncode not in (0, 1):
        return files
    ignored = set(proc.stdout.decode("utf-8", "replace").replace("\r", "").split("\n"))
    return [f for f in files if f not in ignored]


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    except Exception:
        pass

    root = sys.argv[1] if len(sys.argv) > 1 else "."
    w = sys.stdout.write
    total = 0

    # СНАЧАЛА СОБРАТЬ КАНДИДАТОВ, потом одним вопросом к git отсечь
    # игнорируемые — и только затем судить (см. drop_ignored выше).
    cand = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fn in sorted(filenames):
            if not fn.endswith(EXTS) or fn in SKIP_FILES:
                continue
            cand.append(os.path.relpath(os.path.join(dirpath, fn), root)
                        .replace(os.sep, "/"))

    for relc in drop_ignored(root, sorted(cand)):
        if True:
            p = os.path.join(root, relc)
            try:
                raw = io.open(p, "rb").read()
            except Exception:
                continue
            crlf = raw.count(b"\r\n")
            lf = raw.count(b"\n") - crlf
            if crlf and lf:
                total += 1
                rel = os.path.relpath(p, root).replace(os.sep, u"/")
                # Первая аномальная строка: с неё и начинается расхождение.
                first = 0
                line = 1
                i = 0
                while i < len(raw):
                    if raw[i:i + 1] == b"\n":
                        if i == 0 or raw[i - 1:i] != b"\r":
                            first = line
                            break
                        line += 1
                    i += 1
                w(u"%s  CRLF=%d  bare-LF=%d  first-anomaly-line=%d\n"
                  % (rel, crlf, lf, first))
    w(u"total=%d\n" % total)
    return 0


if __name__ == "__main__":
    sys.exit(main())
