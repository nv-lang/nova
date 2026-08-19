# -*- coding: utf-8 -*-
"""scripts/guards/run-guards.py — ОДНА ДВЕРЬ запуска питоновских стражей: все
они исполняются в одном процессе, а не по процессу на стража.

ЗАЧЕМ (П14). Интерпретатор стартует 73мс (замер 2026-08-19, Windows, Git Bash),
а работа самого стража — 40..60мс. На сорока стражах это три секунды чистого
старта: дороже всего, что они считают вместе взятые. Гейт для того и меряется,
чтобы такие вещи было видно.

КОНТРАКТ. Раннер пишет РОВНО те же файлы, что писал параллельный цикл гейта:
`$PAR_DIR/<N>.out` (stdout и stderr слитно, как `> out 2>&1`) и `$PAR_DIR/<N>.rc`
(код возврата). Разбор в гейте не знает и не должен знать, кто их написал.

  python run-guards.py <ROOT> <PAR_DIR> <N> [<N> ...]

Номера — индексы очереди par_add; путь стража раннер читает сам из `<N>.cmd`.

ПОЧЕМУ .rc ПИШЕТСЯ ВСЕГДА, даже когда страж упал с исключением: отсутствие
файла гейт трактует как отказ (`|| echo 1`), и это правильно, но тогда в логе
нет ПРИЧИНЫ. Traceback кладётся в `.out` — иначе поломка стража неотличима от
нарушения правила, а это ровно тот класс, что стоил красного CI 2026-08-18.

Стражи изолированы друг от друга: своё имя модуля, свой `sys.argv`, свой
перехват вывода. Единственное общее — процесс, и падение одного не уносит
остальных.
"""
import importlib.util
import io
import os
import pathlib
import re
import sys
import traceback


def winpath(s):
    """MSYS-форма `/d/Sources/...` -> `d:/Sources/...`.

    Гейт живёт в Git Bash и пишет в `.cmd` СВОЮ форму пути. Аргументы командной
    строки MSYS переводит сам, а содержимое файла -- нет: python искал бы
    `D:\\d\\Sources\\...` от текущего диска и падал бы FileNotFoundError
    (поймано первым же прогоном 2026-08-19).

    На Linux перевод ЗАПРЕЩЁН: там `/d/...` -- настоящий абсолютный путь, и
    «починка» сломала бы CI, который как раз Linux.
    """
    if os.name != "nt":
        return s
    m = re.match(r"^/([A-Za-z])/(.*)$", s)
    return f"{m.group(1)}:/{m.group(2)}" if m else s


class Capture(io.StringIO):
    """Перехват вывода стража.

    `reconfigure` — заглушка: каждый страж первым делом просит у своего потока
    LF-перевод строки (иначе python на Windows печатает CRLF там, где shell
    печатал LF). В одном процессе просить это у StringIO нельзя, а падать на
    вызове — тем более: правило потока здесь и так LF.
    """

    def reconfigure(self, **_kw):
        return None


def run_one(path, root, out_file, rc_file):
    cap = Capture()
    saved = sys.stdout, sys.stderr, sys.argv
    sys.stdout = sys.stderr = cap
    sys.argv = [str(path), str(root)]
    try:
        name = "novac_guard_" + path.stem.replace("-", "_")
        spec = importlib.util.spec_from_file_location(name, path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        rc = mod.main()
        rc = 0 if rc is None else int(rc)
    except SystemExit as e:                      # страж вышел через sys.exit
        rc = 0 if e.code is None else int(e.code)
    except BaseException:                        # поломка стража != нарушение правила
        traceback.print_exc(file=cap)
        rc = 1
    finally:
        sys.stdout, sys.stderr, sys.argv = saved
    out_file.write_text(cap.getvalue(), encoding="utf-8", newline="\n")
    rc_file.write_text(f"{rc}\n", encoding="utf-8", newline="\n")
    return rc


def main():
    if len(sys.argv) < 4:
        print("run-guards.py: FAIL — нужны <ROOT> <PAR_DIR> <N>...", file=sys.stderr)
        return 2
    root = pathlib.Path(winpath(sys.argv[1]))
    par = pathlib.Path(winpath(sys.argv[2]))
    worst = 0
    for idx in sys.argv[3:]:
        cmd = par / f"{idx}.cmd"
        out = par / f"{idx}.out"
        rc = par / f"{idx}.rc"
        if not cmd.is_file():
            out.write_text(f"run-guards.py: FAIL — нет {cmd}\n", encoding="utf-8", newline="\n")
            rc.write_text("1\n", encoding="utf-8", newline="\n")
            worst = 1
            continue
        path = pathlib.Path(winpath(cmd.read_text(encoding="utf-8", errors="replace").strip()))
        code = run_one(path, root, out, rc)
        worst = worst or code
    return 0 if worst == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
