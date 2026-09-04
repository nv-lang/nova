#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-gate-timeout-word.py — «убит пределом» и «красный» обязаны
# быть РАЗНЫМИ словами. Механизм правила Г16 конвенций гейта
# (docs/dev/gate-guard-conventions.md, «Г16. „Убит“ и „красный“ — РАЗНЫЕ СЛОВА в
# сводке»). Аудит стража — план docs/plans/274-novac-self-hosted-compiler.md
# §10.3а; реестр 221.1 №TBD. Самотест:
# selftest/test-check-gate-timeout-word.sh.
"""scripts/guards/check-gate-timeout-word.py — шаг с пределом времени обязан
уметь СКАЗАТЬ, что он снят пределом, а не выдавать это за вердикт о предмете
(правило Г16, docs/dev/gate-guard-conventions.md).

ЗАЧЕМ. 2026-09-04 таких случаев было три за один день, и все три надели одежду
результата:
  * шаг `crate-tests` напечатал «тесты Rust красные» — а 1891 тест зелёный, шаг
    же был снят собственным пределом на 601-й секунде (предел 600 выставлен по
    ТЁПЛОМУ кэшу, холодный идёт вдесятеро дольше);
  * шаг примеров, снятый дедлайном тем же утром, прочитан как «примеры сломаны»;
  * пачечная самопроверка `novac`, оборванная на середине списка, дала счёт «178
    в 20 файлах» вместо честных «1465 в 60».
Два исхода — ПРЕДМЕТ ПРОВЕРЕН И ПЛОХ и ПРО ПРЕДМЕТ НЕ УЗНАЛИ НИЧЕГО — выглядели
в сводке одинаково, и по «красному» шли чинить код, которого никто не судил.
Машине эти два исхода различимы: `timeout` возвращает 124 ИМЕННО в этом случае
(и `scripts/tools/with-deadline.sh` перекладывает 124/137/143 в тот же 124).

ЧТО СЧИТАЕТ. Живые (не закомментированные) строки файлов `scripts/gate.sh`,
`scripts/gate-novac.sh`, `scripts/guards/*.sh`, `scripts/tools/*.sh`, где стоит
вызов с ПРЕДЕЛОМ ВРЕМЕНИ:
  * `timeout [флаги] <предел> …` — предел числом (`600`, `10s`) или переменной
    (`"$DEADLINE"`, `"${LIMIT}s"`); `command -v timeout` вызовом не считается,
    потому что за именем там не предел, а перенаправление;
  * `with-deadline.sh <предел> …` — общая обёртка проекта.
Для каждого вызова смотрится ОКНО: сама строка и WINDOW строк НИЖЕ неё. Ниже —
потому что различение живёт в разборе кода возврата, а он всегда после вызова.

ЧТО КРАСНИТ. Вызов с пределом, в окне которого НЕТ ни одного признака различения
снятия: `124`, `СНЯТ ПРЕДЕЛОМ`, `снят пределом`, `timed out`, `по таймауту`.
Такой вызов не может отличить «упало» от «не успело»: падение по таймауту
выходит наружу неотличимым от настоящего отказа.

ЧЕГО СТРАЖ НЕ УТВЕРЖДАЕТ (Г9 — не утверждать того, чего не мерил). Признак в
окне — это ПРИСУТСТВИЕ РАЗБОРА, а не доказательство, что слово в сводке верное.
Разобрать формулировку машина не может; она может потребовать, чтобы код 124
вообще был замечен. Обратное — отсутствие признака — утверждается твёрдо: где
про 124 не сказано НИЧЕГО, различения нет заведомо.

ХРАПОВИК ВНИЗ: база `scripts/guards/gate-timeout-word.baseline`, ключ
`undistinguished=N` — измеренное число неразличающих вызовов. Рост над базой
красный сразу; цель — 0. Пять примеров остатка названы в базе построчно
(файл:строка), чтобы следующий читатель гасил долг по одному, а не гадал.

ПОТЕРЯ МИШЕНИ — КРАСНОЕ. Ноль подсудных файлов или ноль найденных вызовов
`timeout` — это отказ «образец уехал», а НЕ зелёный ноль. Урок охоты guards × К7
2026-09-04: девять стражей из десяти печатали зелёный ноль ровно тогда, когда их
якорь переименовали.

Аргументы: $1 — корень репозитория (по умолчанию — репозиторий стража);
$2 — override каталога `scripts` (шов самотеста; внутри него ищутся те же
`gate.sh`, `gate-novac.sh`, `guards/*.sh`, `tools/*.sh`).
env GATE_TIMEOUT_WORD_BASELINE — override файла базы (шов самотеста).

Вход для гейта — `main()`: run-guards.py исполняет стражей в одном процессе и
зовёт именно её, а страж с телом на уровне модуля зелен вручную и красен в
гейте.
"""
import io
import os
import re
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-gate-timeout-word"

WINDOW = 12

# Предел: число (`600`, `10s`) или переменная (`"$DEADLINE"`, `"${LIMIT}s"`).
_LIMIT = r'"?(?:\$\{?[A-Za-z_][A-Za-z0-9_]*\}?[smhd]?|[0-9]+[smhd]?)"?'
# Флаги timeout перед пределом: `--kill-after=10s`, `-k 10`, `-s KILL`.
_FLAGS = r"(?:-{1,2}[A-Za-z][-A-Za-z0-9]*(?:=\S+)?\s+|-[a-zA-Z]\s+\S+\s+)*"

RE_TIMEOUT = re.compile(r"(?:^|[\s;&|(`=\"'])timeout\s+" + _FLAGS + _LIMIT + r"(?=\s|$)")
RE_DEADLINE = re.compile(r"with-deadline\.sh\"?\s+" + _LIMIT + r"(?=\s|$)")

# Признаки того, что снятие пределом РАЗБИРАЕТСЯ, а не сливается с отказом.
RE_MARKS = re.compile(
    r"(?<![0-9])124(?![0-9])"
    r"|снят\w*\s+предел\w*"
    r"|timed\s+out"
    r"|по\s+таймауту",
    re.IGNORECASE | re.UNICODE,
)


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def judged_files(scripts_dir):
    """Ровно та выборка, что названа в Г16: два гейта по имени плюс два каталога.

    Список НЕ рекурсивный: `scripts/guards/selftest/` — фикстуры самотестов, у
    них предел времени не шаг гейта, и судить их значило бы красить чужой долг.
    """
    out = []
    for nm in ("gate.sh", "gate-novac.sh"):
        p = os.path.join(scripts_dir, nm)
        if os.path.isfile(p):
            out.append(p)
    for sub in ("guards", "tools"):
        d = os.path.join(scripts_dir, sub)
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            p = os.path.join(d, fn)
            if fn.endswith(".sh") and os.path.isfile(p):
                out.append(p)
    return out


def shown(p, root):
    """Путь так, как читатель будет его искать: относительно корня, когда файл
    под ним; как дан — когда самотест указывает на другой диск (relpath на
    Windows отказывается пересекать точки монтирования)."""
    try:
        return os.path.relpath(p, root).replace("\\", "/")
    except ValueError:
        return p.replace("\\", "/")


def read_lines(path):
    with io.open(path, "rb") as f:
        data = f.read()
    lines = data.decode("utf-8", "replace").split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    return [ln[:-1] if ln.endswith("\r") else ln for ln in lines]


def scan(path, root):
    """-> (список вызовов timeout, список вызовов with-deadline, список плохих).

    Плохой вызов = кортеж (адрес, текст строки)."""
    lines = read_lines(path)
    n_timeout = 0
    n_deadline = 0
    bad = []
    for i, line in enumerate(lines):
        stripped = line.lstrip(" \t\v\f")
        if stripped.startswith("#"):
            continue  # проза, цитирующая форму, — не вызов
        hit_t = bool(RE_TIMEOUT.search(line))
        hit_d = bool(RE_DEADLINE.search(line))
        if not (hit_t or hit_d):
            continue
        if hit_t:
            n_timeout += 1
        if hit_d:
            n_deadline += 1
        window = "\n".join(lines[i:i + 1 + WINDOW])
        if not RE_MARKS.search(window):
            bad.append(("%s:%d" % (shown(path, root), i + 1), stripped[:100]))
    return n_timeout, n_deadline, bad


def main() -> int:
    argv = sys.argv
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.abspath(argv[1] if len(argv) > 1 else os.path.join(here, "..", ".."))
    scripts_dir = os.path.abspath(argv[2]) if len(argv) > 2 else os.path.join(root, "scripts")
    base_file = os.environ.get("GATE_TIMEOUT_WORD_BASELINE",
                               os.path.join(here, "gate-timeout-word.baseline"))

    files = judged_files(scripts_dir)
    if not files:
        return fail("под судом ни одного .sh в %s (gate.sh, gate-novac.sh, guards/, tools/) — "
                    "мишень потеряна, а не «вызовов 0»" % scripts_dir)

    total_timeout = 0
    total_deadline = 0
    bad = []
    for p in files:
        t, d, b = scan(p, root)
        total_timeout += t
        total_deadline += d
        bad.extend(b)

    if total_timeout == 0:
        sys.stderr.write("%s: FAIL — ни одного вызова `timeout <предел>` в %d подсудных файлах: "
                         "мишень потеряна (образец уехал — обёртку переименовали, шаги переехали "
                         "или выборка файлов больше не та)\n" % (NAME, len(files)))
        sys.stderr.write("  Ноль — не «чисто»: страж, считающий несуществующую форму, печатает\n")
        sys.stderr.write("  ноль и выглядит замером (урок охоты guards x К7 2026-09-04).\n")
        return 1

    try:
        base_t = io.open(base_file, encoding="utf-8", errors="replace").read()
    except IOError:
        return fail("нет базы %s (ключ undistinguished=N) — храповик судить нечем" % base_file)
    m = re.search(r"^undistinguished=(\d+)\s*$", base_t, re.M)
    if not m:
        return fail("в базе %s нет строки undistinguished=N — храповик судить нечем" % base_file)
    base = int(m.group(1))

    if len(bad) > base:
        sys.stderr.write("%s: FAIL — вызовов с пределом, не различающих СНЯТИЕ: %d, база %d (Г16).\n"
                         % (NAME, len(bad), base))
        sys.stderr.write("  У такого вызова «предмет проверен и плох» и «про предмет не узнали\n")
        sys.stderr.write("  ничего» выходят наружу ОДНИМ словом — по «красному» идут чинить код,\n")
        sys.stderr.write("  которого никто не судил (crate-tests, 2026-09-04: 1891 тест зелёный).\n")
        for addr, text in bad:
            sys.stderr.write("    %s — %s\n" % (addr, text))
        sys.stderr.write("  Чинить: разобрать код возврата и напечатать ТРЕТЬЕ слово —\n")
        sys.stderr.write("  `rc=$?; [ \"$rc\" -eq 124 ] && echo \"СНЯТ ПРЕДЕЛОМ <N>с: вердикта нет\"`,\n")
        sys.stderr.write("  и этот исход обязан быть красным (иначе он тихий пропуск, Г15).\n")
        return 1

    print("%s ok: файлов %d, вызовов с пределом %d (timeout %d, with-deadline %d), "
          "без различения снятия %d (база %d)"
          % (NAME, len(files), total_timeout + total_deadline, total_timeout, total_deadline,
             len(bad), base))
    return 0


if __name__ == "__main__":
    sys.exit(main())
