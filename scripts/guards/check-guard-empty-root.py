#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-guard-empty-root.py — МЕТА-СТРАЖ: страж, чья мишень
# уехала, обязан краснеть или честно оговариваться, а не печатать зелёный ноль.
# Реестр 221.1 №911 (класс «потерянная мишень читается как замер», сестра №519);
# аудит стражей — план 274 §10.3а. Самотест: selftest/test-check-guard-empty-root.sh.
"""scripts/guards/check-guard-empty-root.py — каждый `check-novac-*`, запущенный
на ПУСТОМ корне, обязан либо отказать, либо честно сказать «судить нечего»;
зелёный `ok` с правдоподобным числом на пустоте — ложь (реестр 221.1 №911).

ЗАЧЕМ. Замер охоты 2026-09-04 (трек guards, отчёт
`docs/dev/hunts/guards/2026-09-04-check-novac-k7.md`, реестр №911): девять
стражей из десяти проверенных печатают ЗЕЛЁНЫЙ НОЛЬ с правдоподобным счётом,
когда их якорь уезжает — модуль переименован, поле переименовано, эмиссия
разрослась на четвёртый файл. Образец из отчёта:
`check-novac-tyid-door ok: файлов .nv: 1, сравнений с нулём вне двери: 0` — на
дереве, где судить нечего. Такой вердикт НЕОТЛИЧИМ от настоящей проверки: и там
и там ноль нарушений и число рядом. Инвентарь того же дня: 44 из 82 стражей
семьи не несут проверки мишени вовсе.

ПОЧЕМУ МЕТА-СТРАЖ, А НЕ ФИКСТУРА У КАЖДОГО. Замечание интегратора при
регистрации №911: строка закрывается только механизмом, судящим ВСЮ семью, —
зелёная фикстура одного стража её не закрывает. Починка носителя приёмкой не
считается: завтрашний страж напишется без проверки мишени и пройдёт мимо любой
чужой фикстуры молча.

ЧТО СЧИТАЕТ. Берёт КАЖДЫЙ `scripts/guards/check-novac-*.py` и
`check-novac-*.sh` (85 файлов на 2026-09-04) и запускает его ДВАЖДЫ, отдав
первым аргументом временный корень — тем самым швом, которым страж принимает
корень. Два уровня, потому что они ловят РАЗНОЕ:
  1. ПУСТОЙ КОРЕНЬ — только пустые `docs/` и `scripts/`. Ловит стража, у
     которого проверки существования нет вовсе.
  2. ПУСТОЙ КАРКАС — те же пустые `docs/{dev,plans,guide}`, `scripts/{guards,
     guards/selftest,tools}`, `novac/{src,fixtures,target}` и пустые модули
     `novac/src/{lex,parse,tree,syntax,sem,check,emit_c,util,driver,diag}`,
     `nova-cli`, `std`, `spec`, `target`. Каталоги ЕСТЬ и все пусты — это и
     есть «якорь уехал»: модуль переименован, файл переехал, эмиссия ушла в
     другой каталог. Замер 2026-09-04: на уровне 1 лгущих НОЛЬ (все 85 упираются
     во внешнюю проверку «нет novac/src» и честно говорят «судить нечего»), на
     уровне 2 — ДЕВЯТЬ. То есть уровень 1 в одиночку доказывал бы пустоту: он
     красив, зелен и ничего не проверяет. Именно этот разрыв — предупреждение о
     том, что «страж есть» и «страж судит» — разные вещи.
Лгущим страж считается, если солгал ХОТЯ БЫ НА ОДНОМ уровне.

РАЗБОР ИСХОДА (на каждом уровне):
  * код возврата НЕ ноль — ЧЕСТНО: страж отказал, мишени нет и он это сказал;
  * код 0 и в `ok`-строке есть оговорка о пустоте — ЧЕСТНО. Честными считаются
    вхождения: «судить нечего», «нечего судить», «мишень», «класс №519»;
  * код 0 и `ok`-строка без оговорки — ЛОЖЬ, страж идёт в список лгущих;
  * код 0 и слова `ok` нет вовсе (например «пропущен») — корзина «тихих»:
    молчание не выдаёт себя за замер и в число лгущих не входит;
  * не уложился в таймаут — корзина «зависших»: дефект, но другого класса.

ЧТО КРАСНИТ.
  * ЛГУЩИХ БОЛЬШЕ БАЗЫ `scripts/guards/guard-empty-root.baseline` (`lying=N`).
    Храповик ВНИЗ: цель — ноль, новый страж семьи обязан рождаться с проверкой
    мишени. Падение ниже базы не красное — печатается напоминание опустить базу.
  * ПОД СУДОМ НОЛЬ СТРАЖЕЙ — красное как потерянная мишень самого мета-стража:
    иначе он стал бы ровно тем, что судит (переименуй семью — и он зелен).
  * ПОД СУДОМ МЕНЬШЕ ПОЛОВИНЫ от `judged=N` базы — та же потеря мишени, только
    частичная: ноль ловится первым правилом, а «было 85, стало 3» — нет, и
    выглядело бы улучшением (лгущих меньше, потому что судить некого).
  * Нет базы, нет ключа `lying=N`/`judged=N`, нет оболочки для `.sh` — судить
    нечем, и это отказ, а не зелёный ноль.

ЧЕГО НЕ СУДИТ. Правильность самих стражей на НАСТОЯЩЕМ дереве (это их
собственное дело), тексты их сообщений сверх наличия оговорки, семьи стражей
кроме `check-novac-*` (мера №911 названа по этой семье; расширение — отдельным
замером и отдельной базой). Не судит и себя: своё имя из списка исключается.
Не судит и ПОЛНОТУ каркаса: страж, чей якорь лежит вне списка каталогов уровня 2
(скажем, `novac/src/backend`), честно скажет «нет каталога» и в лгущие не
попадёт — каркас расширяется тем же коммитом, что вводит новый модуль.

ЦЕНА (замеры 2026-09-04, Windows, машина под чужим гейтом): 85 стражей × 2
уровня = 170 процессов, СУММАРНО 17.7с / 24.4с / 43.2с в трёх прогонах — то
есть 0.10..0.25с на процесс, и разброс задаёт не страж, а загрузка машины.
Порог, за которым понадобилась бы выборка, назван в 60 секунд: худший из
замеров ниже него, но не втрое, и на совсем занятой машине запас невелик —
если бюджет шага начнёт трещать, первым делом смотри сюда. По умолчанию
судятся ВСЕ и последовательно: параллелизм добавил бы нагрузку на и без того
занятую машину ради секунд. Для локальной итерации есть выборка —
`GUARD_EMPTY_ROOT_ONLY=<кусок имени>` оставляет под судом только совпавших, но
тогда храповик по базе НЕ судится (часть не сравнивают с целым) и печатается
честная строка выборки.

ПОЧЕМУ PYTHON: страж запускает процессы и разбирает их вывод; на shell это был
бы тот же алгоритм втрое длиннее (конвенция П14).

Аргументы: $1 — корень репозитория (по умолчанию — репозиторий стража);
$2 — override каталога со стражами (шов самотеста: подставляется каталог
фикстурных «стражей»); env `GUARD_EMPTY_ROOT_BASELINE` — override базы (шов
самотеста); env `GUARD_EMPTY_ROOT_ONLY` — выборка по куску имени;
env `GUARD_EMPTY_ROOT_TIMEOUT` — таймаут на одного стража, секунд (по умолчанию 60).
Самотест: selftest/test-check-guard-empty-root.sh (шесть случаев).
Вход для гейта — `main()`: run-guards.py исполняет питоновских стражей в одном
процессе и зовёт именно её; страж с телом на уровне модуля зелен вручную и
красен в гейте.
"""
import io
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")

NAME = "check-guard-empty-root"

PREFIX = "check-novac-"
SUFFIXES = (".py", ".sh")

# Уровень 1: голый корень. Только два каталога верхнего уровня — без них часть
# стражей падает по ДРУГОЙ причине (нет каталога вообще), а это не ответ на наш
# вопрос.
BARE = ("docs", "scripts")

# Уровень 2: каркас есть, всё пусто. Список — каталоги, на которые семья
# `check-novac-*` реально ставит якоря; он растёт тем же коммитом, что вводит
# новый модуль novac.
SKELETON = BARE + (
    "docs/dev", "docs/plans", "docs/guide",
    "scripts/guards", "scripts/guards/selftest", "scripts/tools",
    "novac", "novac/src", "novac/fixtures", "novac/target",
    "novac/src/lex", "novac/src/parse", "novac/src/tree", "novac/src/syntax",
    "novac/src/sem", "novac/src/check", "novac/src/emit_c", "novac/src/util",
    "novac/src/driver", "novac/src/diag",
    "nova-cli", "std", "spec", "target",
)

LEVELS = (("пустой корень", BARE), ("пустой каркас", SKELETON))

# Оговорка о пустоте: страж САМ сказал, что судить нечего или что мишень
# потеряна. Список закрытый и короткий намеренно — свободное «ничего не
# найдено» ничем не отличается от зелёного нуля.
HONEST_MARKS = ("судить нечего", "нечего судить", "мишень", "класс №519")

# Строка вердикта: `<имя> ok: ...`. Ищем слово целиком, чтобы `ok` внутри слова
# (`token`, `look`) не считалось вердиктом.
RE_OK = re.compile(r"\bok\b", re.IGNORECASE)

RE_LYING = re.compile(r"^lying=(\d+)\s*$", re.M)
RE_JUDGED = re.compile(r"^judged=(\d+)\s*$", re.M)


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def guard_files(gdir, only, myself):
    """Стражи семьи, отсортированные по имени. Себя страж не судит: на пустом
    корне он не нашёл бы там ни одного стража и покраснел бы потерей мишени —
    правдой о пустоте, но не о семье."""
    out = []
    try:
        names = sorted(os.listdir(gdir))
    except OSError:
        return out
    for nm in names:
        if nm == myself or not nm.startswith(PREFIX) or not nm.endswith(SUFFIXES):
            continue
        if only and only not in nm:
            continue
        p = os.path.join(gdir, nm)
        if os.path.isfile(p):
            out.append((nm, p))
    return out


def shell_for(path, sh, bash):
    """Оболочка по шебангу: `#!/usr/bin/env bash` под dash сломается на
    башизмах, и страж отказал бы НЕ по нашей причине — а отказ мы читаем как
    честность, то есть замер оказался бы завышен в пользу семьи."""
    try:
        with io.open(path, "rb") as f:
            first = f.readline()
    except IOError:
        first = b""
    return bash if b"bash" in first else sh


def classify(rc, out):
    if rc != 0:
        return "refused"
    ok_lines = [ln for ln in out.splitlines() if RE_OK.search(ln)]
    if not ok_lines:
        return "silent"
    joined = "\n".join(ok_lines)
    for mark in HONEST_MARKS:
        if mark in joined:
            return "caveat"
    return "lying"


def ok_line_of(out):
    for ln in out.splitlines():
        if RE_OK.search(ln):
            return ln.strip()[:120]
    return ""


def make_root(dirs):
    root = tempfile.mkdtemp(prefix="guard-empty-root.")
    for d in dirs:
        os.makedirs(os.path.join(root, d.replace("/", os.sep)), exist_ok=True)
    return root


def run_level(guards, dirs, env_base, sh, bash, timeout):
    """Прогон всей семьи на одном временном корне. Возвращает {имя: (вид, строка)}."""
    root = make_root(dirs)
    arg_root = root.replace(os.sep, "/")
    env = dict(env_base)
    # Пустой корень не должен «найтись» внутри чужого репозитория: git обходит
    # каталоги ВВЕРХ, и страж судил бы историю временного каталога.
    env["GIT_CEILING_DIRECTORIES"] = os.path.dirname(root).replace(os.sep, "/")
    res = {}
    try:
        for nm, path in guards:
            if nm.endswith(".py"):
                cmd = [sys.executable, path, arg_root]
            else:
                cmd = [shell_for(path, sh, bash), path.replace(os.sep, "/"), arg_root]
            try:
                r = subprocess.run(cmd, cwd=root, env=env, timeout=timeout,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                out = r.stdout.decode("utf-8", "replace")
                kind = classify(r.returncode, out)
            except subprocess.TimeoutExpired:
                out, kind = "", "timeout"
            except OSError as e:
                out, kind = str(e), "refused"
            res[nm] = (kind, ok_line_of(out))
    finally:
        shutil.rmtree(root, ignore_errors=True)
    return res


def main() -> int:
    argv = sys.argv
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.abspath(argv[1] if len(argv) > 1 else os.path.join(here, "..", ".."))
    gdir = os.path.abspath(argv[2]) if len(argv) > 2 else os.path.join(root, "scripts", "guards")
    base_file = os.environ.get("GUARD_EMPTY_ROOT_BASELINE",
                               os.path.join(here, "guard-empty-root.baseline"))
    only = os.environ.get("GUARD_EMPTY_ROOT_ONLY", "").strip()
    try:
        timeout = float(os.environ.get("GUARD_EMPTY_ROOT_TIMEOUT", "60"))
    except ValueError:
        return fail("GUARD_EMPTY_ROOT_TIMEOUT — не число")

    guards = guard_files(gdir, only, os.path.basename(os.path.abspath(__file__)))
    if not guards:
        if only:
            return fail("выборка GUARD_EMPTY_ROOT_ONLY=%s не совпала ни с одним стражем в %s"
                        % (only, gdir))
        return fail("под судом ни одного стража %s{%s} в %s — мишень потеряна, а не «лгущих 0»: "
                    "переименуй семью, и мета-страж стал бы ровно тем, что судит (№911)"
                    % (PREFIX, ",".join(SUFFIXES), gdir))

    sh = shutil.which("sh") or shutil.which("bash")
    bash = shutil.which("bash") or sh
    if any(nm.endswith(".sh") for nm, _ in guards) and not sh:
        return fail("не найдена оболочка (sh/bash) — стражей .sh судить нечем, а «нечем» != зелено")

    env_base = dict(os.environ)
    env_base["LC_ALL"] = "C"
    env_base["PYTHONIOENCODING"] = "utf-8"
    env_base.pop("GIT_DIR", None)

    started = time.time()
    per_level = []
    for title, dirs in LEVELS:
        per_level.append((title, run_level(guards, dirs, env_base, sh, bash, timeout)))
    spent = time.time() - started

    # Лгущий — тот, кто солгал хотя бы на одном уровне; в отчёт идёт уровень,
    # на котором ложь видна (для читателя это подсказка, чем чинить).
    lying = []
    for nm, _ in guards:
        for title, res in per_level:
            kind, line = res[nm]
            if kind == "lying":
                lying.append((nm, title, line))
                break

    counts = {}
    for title, res in per_level:
        c = {}
        for kind, _ in res.values():
            c[kind] = c.get(kind, 0) + 1
        counts[title] = c
    n = len(guards)

    def brief():
        parts = []
        for title, _ in LEVELS:
            c = counts[title]
            parts.append("%s: лгущих %d, отказов %d, оговорок %d, тихих %d, зависших %d"
                         % (title, c.get("lying", 0), c.get("refused", 0), c.get("caveat", 0),
                            c.get("silent", 0), c.get("timeout", 0)))
        return "; ".join(parts)

    if only:
        print("%s: ВЫБОРКА GUARD_EMPTY_ROOT_ONLY=%s — под судом %d из семьи, храповик НЕ судится "
              "(часть не сравнивают с целым)" % (NAME, only, n))
        for g, title, line in lying:
            print("    ЛЖЁТ %s (%s) — %s" % (g, title, line))
        print("%s ok (выборка): лгущих %d из %d; %s; за %.1fс"
              % (NAME, len(lying), n, brief(), spent))
        return 0

    try:
        base_t = io.open(base_file, encoding="utf-8", errors="replace").read()
    except IOError:
        return fail("нет базы %s (ключи lying=N и judged=N) — храповик судить нечем" % base_file)
    m_l = RE_LYING.search(base_t)
    m_j = RE_JUDGED.search(base_t)
    if not m_l or not m_j:
        return fail("в базе %s нет строки lying=N и/или judged=N — храповик судить нечем" % base_file)
    base_lying = int(m_l.group(1))
    base_judged = int(m_j.group(1))

    if n * 2 < base_judged:
        return fail("под судом %d стражей, а база помнит %d — мишень потеряна больше чем наполовину. "
                    "Ноль ловится отдельным правилом, а «было %d, стало %d» выглядело бы улучшением: "
                    "лгущих меньше, потому что судить некого (№911)"
                    % (n, base_judged, base_judged, n))

    if len(lying) > base_lying:
        sys.stderr.write("%s: FAIL — стражей, печатающих зелёный `ok` с числом на ПУСТОМ корне: %d, "
                         "база %d (№911). Честны ровно два ответа: ненулевой код либо `ok` с "
                         "оговоркой («судить нечего», «мишень потеряна»):\n"
                         % (NAME, len(lying), base_lying))
        for g, title, line in lying[:20]:
            sys.stderr.write("    %s (%s) — %s\n" % (g, title, line))
        if len(lying) > 20:
            sys.stderr.write("    ... и ещё %d\n" % (len(lying) - 20))
        sys.stderr.write("  Новый страж семьи обязан рождаться с проверкой мишени: ноль носителей —\n")
        sys.stderr.write("  это отказ с текстом про потерянную мишень, а НЕ зелёный ноль. Образец —\n")
        sys.stderr.write("  scripts/guards/check-novac-prim-id-compare.py.\n")
        return 1

    tail = ""
    if len(lying) < base_lying:
        tail = " — база устарела, опусти lying до %d тем же коммитом" % len(lying)
    print("%s ok: стражей под судом %d, лгущих на пустом корне %d (база %d)%s; %s; за %.1fс"
          % (NAME, n, len(lying), base_lying, tail, brief(), spent))
    return 0


if __name__ == "__main__":
    sys.exit(main())
