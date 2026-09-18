#!/usr/bin/env python
# -*- coding: utf-8 -*-
u"""План 292 Ш.2 — очередь как ДАННЫЕ: снимок `target/queue.json`.

Зачем это существует. Очередь работ в проекте ВЫВОДИТСЯ командами, а не ведётся
руками (решение 2026-08-10, и оно верное: рукописная таблица расходилась молча и
была вторым домом правды). Следствие, которого никто не сделал: у выводимой
очереди нет существования в момент конца хода — значит ни одна машина не может
ответить «работа осталась?», и Stop-страж обречён судить ТЕКСТ. Замер по
одиннадцати стенограммам: 1104 конца хода, поймано пять. Один процент.

Этот скрипт запускает ровно те команды, что записаны в записке интегратора
(раздел «Очередь — СПРАШИВАЕТСЯ, а не ведётся»), и кладёт ответ в
`target/queue.json`. Он НЕ заводит второй дом правды: он КЭШИРУЕТ ответ
источников и называет каждый источник его дословной командой.

Три условия интегратора (план 292, §3б), они же приёмка:

* И.1 — снимок живёт вне git (`target/` под `.gitignore`), несёт `produced_by`
  и `at`, и у КАЖДОГО источника стоит `cmd` — дословная команда. Читатель не
  верит автору, а повторяет команду и получает то же.
* И.2 — на снимок не опирается ни одно решение: он открывает ворота Stop-хука и
  ничего больше.
* И.3 — ОТКАЗ НЕ ИМЕЕТ ПРАВА ВЫГЛЯДЕТЬ ПУСТОЙ ОЧЕРЕДЬЮ. Недостижимое зеркало,
  не ответивший git, упавший сам скрипт — всё это `ok: false` плюс
  `failed_sources`, и НИКОГДА не пустой `open`.

Три беды различаются ФОРМУЛИРОВКОЙ, и это не косметика (урок куплен 2026-09-18:
первая попытка измерить три зеркала показала отказ всех трёх, а на деле в Git
Bash нет `/usr/bin/time -f` — падала ИЗМЕРЯЛКА, а не зеркала):

* `ИСТОЧНИК ОТКАЗАЛ`      — команда запустилась и вернула ошибку/таймаут;
* `ЗАПУСК НЕ СОСТОЯЛСЯ`   — команду не удалось запустить вовсе (дефект среды);
* `СНИМОК УПАЛ`           — сломался сам скрипт.

Гейта среди источников НЕТ намеренно: он идёт сорок минут, снимком не снимается,
а записанный вчерашний вердикт — тот самый протухший кэш, ради борьбы с которым
очередь и перестали вести руками.

Запуск:  python scripts/tools/queue-snapshot.py [путь-к-дереву]

Сиденья для самотеста (только они, больше скрипт из среды ничего не читает):
  NOVA_WINDOW_ROLE      — роль в обход определения по ветке (как у хука);
  NOVA_QUEUE_MIRRORS    — список зеркал через запятую (умолчание три);
  NOVA_QUEUE_PYTHON     — чем звать сканер реестра (умолчание — текущий python);
  NOVA_QUEUE_SCAN_PY    — путь к сканеру реестра внутри дерева.
"""

import io
import json
import os
import re
import subprocess
import sys
import threading
import time

SOURCE_TIMEOUT_SEC = 15
MIRRORS_DEFAULT = (u"origin", u"gitverse", u"sourcecraft")
SCAN_REL_DEFAULT = u"scripts/guards/registry-routes-scan.py"

# Ветка -> роль. Дом соответствия — `/save`; здесь копия ТОЛЬКО потому, что она
# обязана совпасть с `detect_role` хука `guard-stop-v2.py`: снимок и хук должны
# одинаково понимать, чьё это окно. Расхождение ловится клеткой самотеста.
ROLE_BY_BRANCH = [
    (u"p274-novac", u"carina"),
    (u"controller", u"controller"),
    (u"main", u"integrator"),
]

FAIL_RAN = u"ИСТОЧНИК ОТКАЗАЛ"
FAIL_LAUNCH = u"ЗАПУСК НЕ СОСТОЯЛСЯ"
FAIL_SELF = u"СНИМОК УПАЛ"


# --------------------------------------------------------------------------
# запуск команд
# --------------------------------------------------------------------------

def shell_form(argv):
    u"""Команда одной строкой — ровно в том виде, в каком её повторяет человек."""
    out = []
    for a in argv:
        out.append(u'"%s"' % a if u" " in a else u"%s" % a)
    return u" ".join(out)


def run(argv, cwd, timeout=SOURCE_TIMEOUT_SEC):
    u"""(статус, stdout, пояснение). Статус: ok | failed | not_launched.

    Два разных «плохо» РАЗЛИЧАЮТСЯ здесь, а не у вызывающего: «команда вернула
    ошибку» — беда источника, «команду не удалось запустить» — беда среды или
    скрипта. Одинаковая запись для двух разных бед делает лог бесполезным ровно
    тогда, когда он нужен.
    """
    try:
        p = subprocess.Popen(
            argv, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
    except Exception as e:
        return u"not_launched", u"", u"%s: %s" % (type(e).__name__, e)
    try:
        out, err = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            p.kill()
            p.communicate()
        except Exception:
            pass
        return u"failed", u"", u"таймаут %d с" % timeout
    # CRLF снимается ЗДЕСЬ, у всех источников разом: на Windows вывод приходит
    # с `\r`, и разбор по `^...$` молча не находит строку, которая в глазах
    # человека есть. Замерено на первом же прогоне: `blockers=73\r` дал
    # «в выводе нет строки blockers=N».
    txt = (out or b"").decode("utf-8", "replace").replace(u"\r\n", u"\n")
    if p.returncode != 0:
        tail = (err or b"").decode("utf-8", "replace").strip().splitlines()
        return u"failed", txt, u"код возврата %d%s" % (
            p.returncode, (u"; " + tail[-1]) if tail else u"")
    return u"ok", txt, u""


# --------------------------------------------------------------------------
# роль
# --------------------------------------------------------------------------

def current_branch(tree):
    u"""Ветка ЧТЕНИЕМ файлов, без запуска git — тем же способом, что у хука.
    В worktree `.git` не каталог, а файл со строкой `gitdir: <путь>`."""
    g = os.path.join(tree, u".git")
    try:
        if os.path.isfile(g):
            with io.open(g, encoding="utf-8", errors="replace") as fh:
                line = fh.read().strip()
            if line.startswith(u"gitdir:"):
                g = line.split(u":", 1)[1].strip()
        with io.open(os.path.join(g, u"HEAD"), encoding="utf-8", errors="replace") as fh:
            h = fh.read().strip()
        return h.split(u"/", 2)[-1] if h.startswith(u"ref:") else u""
    except Exception:
        return u""


def common_git_dir(tree):
    u"""Общий `.git` чтением: каталог в главном дереве, `gitdir:` двумя уровнями
    выше в worktree."""
    g = os.path.join(tree, u".git")
    try:
        if os.path.isdir(g):
            return g
        if os.path.isfile(g):
            with io.open(g, encoding="utf-8", errors="replace") as fh:
                line = fh.read().strip()
            if line.startswith(u"gitdir:"):
                p = line.split(u":", 1)[1].strip()
                return os.path.dirname(os.path.dirname(p))
    except Exception:
        pass
    return u""


def role_belongs_to_me(tree, role):
    u"""Ветка УГАДЫВАЕТ роль, визитка РЕШАЕТ, чья она.

    Дом правила — `scripts/claude-hooks/guard-stop-v2.py` (`role_belongs_to_me`),
    туда же оно пришло из `remind-session-save.py`. Здесь копия, и она обязана
    остаться согласной: снимок и хук отвечают на ОДИН вопрос «чьё это окно», и
    разойдясь, дадут окну снимок чужой очереди при молчащем страже. Клетка
    самотеста сверяет оба ответа.

    Замерено 2026-09-19 02:06, на себе: хук уже получил визитную проверку и
    отвечал `none`, а снимок в том же дереве отвечал `integrator` — то есть
    печатал окну чужую очередь из 12 пунктов.
    """
    sid = (os.environ.get("CLAUDE_CODE_SESSION_ID") or u"").strip()
    if not sid:
        return True
    g = common_git_dir(tree)
    if not g:
        return True
    card = os.path.join(g, u"nova-session-%s.card" % role)
    if not os.path.isfile(card):
        return True
    try:
        with io.open(card, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                if line.startswith(u"session_id="):
                    owner = line.split(u"=", 1)[1].strip()
                    return (not owner) or owner == sid
    except Exception:
        return True
    return True


def detect_role(tree):
    env = (os.environ.get("NOVA_WINDOW_ROLE") or u"").strip().lower()
    if env:
        return env
    br = current_branch(tree).lower()
    if not br:
        return u"none"
    for needle, role in ROLE_BY_BRANCH:
        if needle in br:
            return role if role_belongs_to_me(tree, role) else u"none"
    return u"none"


# --------------------------------------------------------------------------
# источники
# --------------------------------------------------------------------------

def source_mirrors(tree, state):
    u"""Три зеркала, и они сравниваются МЕЖДУ СОБОЙ, а не с одним origin.

    Куплено ошибкой 2026-08-21: `git rev-list --count origin/main..main` ответил
    0 и был прав ровно про origin, тогда как gitverse и sourcecraft отставали на
    70 коммитов. Команда, знающая одно зеркало из трёх, на вопрос «всё ли
    запушено» отвечать не может.
    """
    names = [s.strip() for s in (
        os.environ.get("NOVA_QUEUE_MIRRORS") or u",".join(MIRRORS_DEFAULT)
    ).split(u",") if s.strip()]

    per = {}
    for name in names:
        argv = [u"git", u"-C", tree, u"ls-remote", u"--heads", name, u"main"]
        st, out, why = run(argv, tree)
        sha = u""
        if st == u"ok":
            line = (out or u"").strip().splitlines()
            sha = line[0].split()[0] if line and line[0].split() else u""
            if not sha:
                st, why = u"failed", u"зеркало не отдало ветку main"
        per[name] = {u"sha": sha, u"cmd": shell_form(argv), u"status": st, u"why": why}
        if st != u"ok":
            state.fail(st, u"зеркало %s" % name, shell_form(argv), why)

    live = dict((n, d[u"sha"]) for n, d in per.items() if d[u"status"] == u"ok")
    behind, note = [], u""
    if len(live) >= 2:
        shas = set(live.values())
        if len(shas) == 1:
            note = u"все зеркала несут один объект"
        else:
            tip = _tip_of(tree, list(shas))
            if tip:
                behind = sorted(n for n, s in live.items() if s != tip)
                note = u"самый свежий объект %s; отстают: %s" % (
                    tip[:9], u", ".join(behind))
            else:
                # Порядок локально не устанавливается (объектов нет в дереве).
                # Молчать об этом нельзя: расхождение есть, а какое — неизвестно.
                behind = sorted(live.keys())
                note = u"зеркала РАСХОДЯТСЯ, порядок локально не установлен"
    elif len(live) == 1:
        note = u"живо одно зеркало — сравнивать не с чем"

    return {
        u"value": len(behind),
        u"cmd": u" ; ".join(per[n][u"cmd"] for n in names),
        u"note": note,
        u"behind": behind,
        u"mirrors": dict(
            (n, {u"sha": d[u"sha"], u"cmd": d[u"cmd"], u"status": d[u"status"]})
            for n, d in per.items()),
    }


def _tip_of(tree, shas):
    u"""Тот объект из списка, потомком которого являются все прочие. Пусто —
    если объектов нет локально: тогда порядок неизвестен, и врать нельзя."""
    for cand in shas:
        ok = True
        for other in shas:
            if other == cand:
                continue
            st, _, _ = run([u"git", u"-C", tree, u"merge-base",
                            u"--is-ancestor", other, cand], tree, timeout=10)
            if st != u"ok":
                ok = False
                break
        if ok:
            return cand
    return u""


def source_unmerged(tree, state):
    argv = [u"git", u"-C", tree, u"branch", u"--no-merged", u"main",
            u"--format=%(refname:short)"]
    st, out, why = run(argv, tree)
    if st != u"ok":
        state.fail(st, u"несли́тые ветки", shell_form(argv), why)
        return {u"value": None, u"cmd": shell_form(argv), u"branches": []}
    names = [l.strip() for l in (out or u"").splitlines() if l.strip()]
    return {u"value": len(names), u"cmd": shell_form(argv), u"branches": names}


def source_blockers(tree, state):
    u"""Счёт блокеров — ТОЛЬКО ядром стража `registry-routes-scan.py`.

    Свой счётчик уже дважды дал неверное число (греп — 75, самодельное правило —
    47), и третий даст третье: спор о готовности пойдёт про арифметику вместо
    кода. Поэтому здесь зовётся скрипт и разбирается его вывод.
    """
    py = os.environ.get("NOVA_QUEUE_PYTHON") or sys.executable or u"python"
    scan = os.environ.get("NOVA_QUEUE_SCAN_PY") or SCAN_REL_DEFAULT
    argv = [py, scan, u"."]
    st, out, why = run(argv, tree)
    cmd = u"python %s ." % scan
    if st != u"ok":
        state.fail(st, u"блокеры реестра", cmd, why)
        return {u"value": None, u"cmd": cmd, u"numbers": []}

    m = re.search(u"^blockers=(\\d+)$", out, re.MULTILINE)
    nums = []
    tail = out.split(u"blockers_list:", 1)
    if len(tail) == 2:
        for line in tail[1].splitlines():
            found = re.findall(u"\\d+", line)
            if found:
                nums = [int(x) for x in found]
                break
    if m is None:
        state.fail(u"failed", u"блокеры реестра", cmd,
                   u"в выводе нет строки blockers=N")
        return {u"value": None, u"cmd": cmd, u"numbers": []}
    value = int(m.group(1))
    if value != len(nums):
        # Число и список обязаны сходиться: разойдясь, они делают снимок
        # правдоподобным и неверным одновременно.
        state.fail(u"failed", u"блокеры реестра", cmd,
                   u"blockers=%d, а в списке %d номеров" % (value, len(nums)))
    return {u"value": value, u"cmd": cmd, u"numbers": nums}


# Признак ЗАКРЫТОСТИ подплана. Список взят ИЗ ДЕРЕВА (перебор всех строк
# `**Статус:**` в `docs/plans/` 2026-09-19), а не придуман: вокабуляр там
# свободный, семьдесят с лишним форм первого слова. Поэтому правило
# ОДНОСТОРОННЕЕ — закрытым считается только явно названное закрытым, всё
# прочее ОТКРЫТО. Незнакомая формулировка обязана давать НЕПУСТУЮ очередь:
# ошибка в сторону «работа есть» стоит окну одного лишнего хода, ошибка в
# другую сторону выдаёт окну право остановиться, которого у него нет.
SUBPLAN_CLOSED_WORDS = (
    u"ЗАКРЫТ", u"ЗАВЕРШЁН", u"ИСПОЛНЕН", u"ПОГЛОЩЁН",
    u"CLOSED", u"SUPERSEDED", u"СУПЕРСЕДЕД",
)

# Подпланы, которые ведёт ОКНО КАРИНЫ (`/integrator`: 274.1-274.9 её, 274.10 и
# 274.11 мои). Номера перечислены явно: шаблон `274.*` затянул бы и мои, и
# очередь Карины поехала бы от моей работы.
CARINA_SUBPLAN_NUMBERS = tuple(u"274.%d" % n for n in range(1, 10))


def source_carina_subplans(tree, state):
    u"""Очередь роли carina = её подпланы, не объявленные закрытыми.

    До 2026-09-19 источника не было вовсе, и снимок отвечал `ok: false` —
    то есть код `СТОП: очередь-пуста` был для Карины НЕДОКАЗУЕМ ПО
    ПОСТРОЕНИЮ: законно остановиться она не могла никогда, только пробиться
    предохранителем на третьей блокировке подряд. Нашло это её собственное
    окно, уткнувшись в отказ. Побег, случающийся по расписанию, перестаёт
    быть сигналом, а давление на нас, которым эта дыра оправдывалась,
    платилось НЕ нами.

    Источник читается ФАЙЛАМИ, а не командой: `**Статус:**` и есть
    единственный дом правды о состоянии плана (`/status`, `/integrator`).
    `cmd` всё равно ставится — дословный греп, которым читатель повторит
    тот же ответ руками (И.1).
    """
    cmd = u"grep -m1 '^[*][*]Статус:[*][*]' docs/plans/274.{1..9}-*.md"
    plans_dir = os.path.join(tree, u"docs", u"plans")
    if not os.path.isdir(plans_dir):
        state.fail(u"not_launched", u"подпланы Карины", cmd,
                   u"каталога docs/plans нет в дереве %s" % tree)
        return {u"value": None, u"cmd": cmd, u"plans": []}

    try:
        names = sorted(os.listdir(plans_dir))
    except Exception as e:
        state.fail(u"failed", u"подпланы Карины", cmd,
                   u"каталог не читается: %s" % e)
        return {u"value": None, u"cmd": cmd, u"plans": []}

    open_plans = []
    seen = []
    for num in CARINA_SUBPLAN_NUMBERS:
        hit = [n for n in names
               if n.startswith(num + u"-") and n.endswith(u".md")]
        if not hit:
            # Пропавший подплан — беда, а не пустая очередь: его могли
            # переименовать, и тогда снимок молча перестал бы его считать.
            state.fail(u"failed", u"подпланы Карины", cmd,
                       u"подплана %s нет в docs/plans" % num)
            continue
        seen.append(num)
        path = os.path.join(plans_dir, hit[0])
        try:
            text = io.open(path, encoding="utf-8", errors="replace").read()
        except Exception as e:
            state.fail(u"failed", u"подпланы Карины", cmd,
                       u"%s не читается: %s" % (hit[0], e))
            continue
        m = re.search(u"^\\*\\*Статус:\\*\\*(.*)$", text, re.MULTILINE)
        if m is None:
            # Плана без статус-строки быть не должно; молчать нельзя.
            state.fail(u"failed", u"подпланы Карины", cmd,
                       u"%s: нет строки **Статус:**" % hit[0])
            continue
        # ВЕРДИКТ НЕСЁТ ПЕРВОЕ СЛОВО, а не вхождение куда-нибудь в строку.
        # Куплено пробой 2026-09-19: 274.8 со статусом «В РАБОТЕ — … (3а)
        # ЗАКРЫТА обеими половинами критерия» был посчитан ЗАКРЫТЫМ, то есть
        # страж прочёл прозу как данные и выдал Карине право остановиться при
        # живом подплане. Ведущие значки и `**` снимаются, дальше берётся
        # ровно один токен.
        head = m.group(1).strip().upper()
        head = re.sub(u"^[^0-9A-ZА-ЯЁ]+", u"", head)
        first = re.split(u"[^0-9A-ZА-ЯЁ]", head, maxsplit=1)[0]
        if first not in SUBPLAN_CLOSED_WORDS:
            open_plans.append(num)

    return {u"value": len(open_plans), u"cmd": cmd,
            u"plans": open_plans, u"checked": seen}


# --------------------------------------------------------------------------
# сборка
# --------------------------------------------------------------------------

class State(object):
    u"""Копилка отказов. Каждый отказ НАЗЫВАЕТ СВОЙ РОД — см. докстринг модуля."""

    def __init__(self):
        self.failed = []
        self.lock = threading.Lock()

    def fail(self, status, name, cmd, why):
        kind = FAIL_LAUNCH if status == u"not_launched" else FAIL_RAN
        with self.lock:
            self.failed.append(u"%s: %s — `%s` — %s" % (kind, name, cmd, why))


# Источники, из которых собирается `open`. `blockers` СЮДА НЕ ВХОДИТ — см.
# build_open.
QUEUE_SOURCES = (u"mirrors_behind", u"unmerged_branches",
                 u"carina_subplans")


def build_open(sources):
    u"""`open` СОБИРАЕТСЯ из источников, руками не пишется. Правило вывода —
    план 292 Ш.2; менять его здесь значит менять договор, а не код.

    `blockers` В ОЧЕРЕДЬ НЕ ИДЁТ (вердикт интегратора 2026-09-18, сверено по
    плану 274 обеими цитатами). Поле `БЛОКИРУЕТ ТЕГ` осталось от мёртвого
    события — тега ОРАКУЛА, которого не будет; владелец снял предмет
    2026-09-15, а смысл поля «перечитывается как „блокирует релиз Карины“ при
    работе со строкой, а не сплошной заменой» (274). Как ХРАПОВИК счёт законен
    и остаётся в `sources` со своей командой; как ОЧЕРЕДЬ он лжив: «73 открытых
    пункта» прочтётся как работа, которой нет. Разница между храповиком и
    очередью — ровно та, ради которой писан весь этот снимок.
    """
    items = []
    mir = sources.get(u"mirrors_behind") or {}
    if mir.get(u"behind"):
        items.append(u"запушить на зеркала: %s" % u", ".join(mir[u"behind"]))
    for br in (sources.get(u"unmerged_branches") or {}).get(u"branches") or []:
        items.append(u"слить ветку %s" % br)
    for num in (sources.get(u"carina_subplans") or {}).get(u"plans") or []:
        items.append(u"подплан %s не закрыт" % num)
    return items


def snapshot(tree, produced_by):
    started = time.time()
    state = State()

    # Дерева нет или это не рабочая копия — беда САМОГО снимка, и молчать о ней
    # нельзя. Без этой проверки путь с опечаткой даёт `role: none`, то есть
    # «ворота открыты» — отказ, надевший костюм законной пустой очереди. Ровно
    # то, что запрещает И.3, и ровно тот класс, на который указал интегратор:
    # ломается не источник, а скрипт.
    if not os.path.isdir(tree) or not os.path.exists(os.path.join(tree, u".git")):
        return {
            u"role": u"unknown", u"at": iso_now(), u"produced_by": produced_by,
            u"ok": False,
            u"failed_sources": [u"%s: дерева нет или это не рабочая копия git: %s"
                                % (FAIL_SELF, tree)],
            u"open": [u"указать снимку существующее дерево репозитория"],
            u"sources": {}, u"elapsed_sec": round(time.time() - started, 2),
        }

    role = detect_role(tree)

    if role == u"controller":
        # Очередь КОНТРОЛЁРА не выводится: в его записке раздела с командами
        # нет, а сама роль под вопросом — владелец 2026-09-19 оставил её
        # «пока работает, если не мешает», и критерий её сворачивания —
        # замер (план 292 Ш.6). Заводить ему источник значит закреплять роль,
        # которую, возможно, снимут. По И.3 это `ok: false` с названной
        # причиной, а НЕ пустая очередь.
        #
        # РОЛЬ carina ОТСЮДА УШЛА 2026-09-19 — см. `source_carina_subplans`.
        # Держать её здесь значило сделать код `СТОП: очередь-пуста`
        # недоказуемым по построению для целого окна.
        why = u"очередь роли %s не выводится: источник не назван" % role
        return {
            u"role": role, u"at": iso_now(), u"produced_by": produced_by,
            u"ok": False, u"failed_sources": [why],
            u"open": [u"назвать машинный источник очереди для роли %s "
                      u"(план 292 Ш.2)" % role],
            u"sources": {}, u"elapsed_sec": round(time.time() - started, 2),
        }

    if role == u"none":
        # Окно без роли (исследовательское, разовая волна): очередь выводить не
        # из чего И НЕ НАДО — хук по такой роли пропускает по построению.
        return {
            u"role": role, u"at": iso_now(), u"produced_by": produced_by,
            u"ok": True, u"failed_sources": [], u"open": [], u"sources": {},
            u"note": u"роль none: очередь не выводится, ворота открыты по роли",
            u"elapsed_sec": round(time.time() - started, 2),
        }

    # Источники опрашиваются ПАРАЛЛЕЛЬНО: последовательно это ~12 с (замер
    # 2026-09-18: sourcecraft один даёт 9,8 с), а бюджет снимка — 15 с.
    sources = {}
    # Набор источников — ПО РОЛИ: очередь интегратора (зеркала, несли́тые
    # ветки) не есть очередь Карины, и подавать ей мою работу значит то же
    # самое, за что 2026-09-19 чинился сам снимок.
    if role == u"carina":
        jobs = [
            (u"carina_subplans", lambda: source_carina_subplans(tree, state)),
        ]
    else:
        jobs = [
            (u"mirrors_behind", lambda: source_mirrors(tree, state)),
            (u"unmerged_branches", lambda: source_unmerged(tree, state)),
            (u"blockers", lambda: source_blockers(tree, state)),
        ]
    threads = []
    for key, fn in jobs:
        def worker(key=key, fn=fn):
            try:
                sources[key] = fn()
            except Exception as e:
                sources[key] = {u"value": None, u"cmd": u"", u"error": u"%s" % e}
                state.fail(u"failed", key, u"(внутри снимка)",
                           u"%s: %s" % (type(e).__name__, e))
        t = threading.Thread(target=worker)
        t.daemon = True
        t.start()
        threads.append(t)
    for t in threads:
        t.join(SOURCE_TIMEOUT_SEC + 5)

    items = build_open(sources)

    ok = not state.failed
    # Пустой `open` при непустых источниках — ДЕФЕКТ СКРИПТА, а не пустая
    # очередь. Это тот самый случай, ради которого написано И.3.
    # Считаются ТОЛЬКО очередные источники: `blockers` в `open` не идёт по
    # построению, и учитывать его здесь значило бы объявлять дефектом верную
    # работу — снимок падал бы в `ok: false` ровно потому, что храповик не ноль.
    nonzero = [k for k in QUEUE_SOURCES if (sources.get(k) or {}).get(u"value")]
    if ok and nonzero and not items:
        ok = False
        state.failed.append(
            u"%s: пустой `open` при непустых источниках (%s) — правило сборки "
            u"не сработало" % (FAIL_SELF, u", ".join(sorted(nonzero))))
    if not ok and not items:
        items = [u"очередь не посчитана: %s" % state.failed[0]]

    return {
        u"role": role, u"at": iso_now(), u"produced_by": produced_by,
        u"ok": ok, u"failed_sources": list(state.failed), u"open": items,
        u"sources": sources, u"elapsed_sec": round(time.time() - started, 2),
    }


def iso_now():
    off = -(time.altzone if time.daylight and time.localtime().tm_isdst else time.timezone)
    sign = u"+" if off >= 0 else u"-"
    off = abs(off)
    return u"%s%s%02d:%02d" % (time.strftime("%Y-%m-%dT%H:%M:%S"), sign,
                               off // 3600, (off % 3600) // 60)


def write_snapshot(tree, data):
    # NOVA_QUEUE_OUT — сиденье для самотеста: он гоняет десяток клеток и не
    # имеет права затирать настоящий снимок окна.
    out = os.environ.get("NOVA_QUEUE_OUT")
    if out:
        d = os.path.dirname(os.path.abspath(out)) or u"."
        os.makedirs(d, exist_ok=True)
        with io.open(out, "w", encoding="utf-8") as fh:
            fh.write(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True))
            fh.write(u"\n")
        return out
    d = os.path.join(tree, u"target")
    os.makedirs(d, exist_ok=True)
    p = os.path.join(d, u"queue.json")
    with io.open(p, "w", encoding="utf-8") as fh:
        fh.write(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True))
        fh.write(u"\n")
    return p


def emit_line(data):
    u"""Итог в stdout. Печатается ВСЕГДА: снимок, о котором нельзя узнать из
    консоли, заставляет читать файл ради одной строки."""
    line = u"queue-snapshot: role=%s ok=%s open=%d failed=%d %ss" % (
        data.get(u"role"), u"true" if data.get(u"ok") else u"false",
        len(data.get(u"open") or []), len(data.get(u"failed_sources") or []),
        data.get(u"elapsed_sec"))
    try:
        sys.stdout.buffer.write(line.encode("utf-8") + b"\n")
        sys.stdout.buffer.flush()
    except Exception:
        sys.stdout.write(line.encode("ascii", "replace").decode("ascii") + "\n")


def main(argv):
    # Прямые слэши — НЕ косметика: `cmd` в снимке обязан ВСТАВЛЯТЬСЯ в ту
    # оболочку, которой здесь пользуются (Git Bash). С обратными слэшами строка
    # при вставке молча теряет их, и проверяющий получает ДРУГОЕ значение,
    # решив, что разошёлся снимок. Замерено на приёмке И.1: так «разошлись»
    # четыре источника из пяти, хотя расходилась проверялка.
    tree = os.path.abspath(argv[1] if len(argv) > 1 else u".").replace(u"\\", u"/")
    produced_by = u"python scripts/tools/queue-snapshot.py %s" % (
        argv[1] if len(argv) > 1 else u".")
    try:
        data = snapshot(tree, produced_by)
        write_snapshot(tree, data)
        emit_line(data)
        return 0 if data.get(u"ok") else 1
    except Exception as e:
        # Падение САМОГО снимка обязано быть громким и записанным: немой снимок
        # неотличим от пустой очереди, а это ровно то, что запрещает И.3.
        data = {
            u"role": u"unknown", u"at": iso_now(), u"produced_by": produced_by,
            u"ok": False,
            u"failed_sources": [u"%s: %s: %s" % (FAIL_SELF, type(e).__name__, e)],
            u"open": [u"починить `scripts/tools/queue-snapshot.py`: снимок упал"],
            u"sources": {}, u"elapsed_sec": 0,
        }
        try:
            write_snapshot(tree, data)
        except Exception:
            pass
        emit_line(data)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
