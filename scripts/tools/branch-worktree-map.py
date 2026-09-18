#!/usr/bin/env python
# -*- coding: utf-8 -*-
u"""Карта несли́тых веток и рабочих деревьев (задание интегратора 2026-09-19).

Зачем скриптом, а не таблицей в доке. Рукописная карта протухает в тот день, когда
её написали, и становится вторым домом правды — тем самым, ради борьбы с которым в
проекте убрали таблицу очереди. Здесь карта ВЫВОДИТСЯ; каждое число берётся
командой, и команда названа в шапке выдачи.

**Это КАРТА, а не приговор.** Скрипт ничего не удаляет и удалять не умеет. Снятие
дерева согласует интегратор с окном-хозяином: чужая рабочая копия — не мусор, а
чьё-то незакоммиченное.

Три различения, без которых карта врёт (названы интегратором заранее):

* **«влита» и «имеет уникальный диф» — РАЗНЫЕ вопросы.** Ветка может не быть
  предком `main` и при этом не нести ни единого отличия: работа попала в `main`
  другим путём. Поэтому в строке стоят ОБА ответа.
* **Счёт `rev-list --count` включает коммиты СЛИЯНИЯ.** Число без дифа
  прочитывается как «столько работы», и это неверно; рядом всегда диф.
* **Точка отсчёта названа.** Сравнение идёт с ЛОКАЛЬНЫМ `main` (его же берёт
  `git branch --no-merged main`), а расхождение локального `main` с `origin/main`
  печатается отдельно: «впереди на N» без точки отсчёта не факт — у ветки два
  `main`, и оба ответа бывают верны.

**Замер, ради которого различения появились** (2026-09-19): подозрение было, что
половина из 12 несли́тых веток мертва. Оказалось — НИ ОДНОЙ: все 12 не влиты и все
несут непустой диф. Четыре ветки, названные как «влитые», в списке `--no-merged` и
не могли стоять: их там нет ПОТОМУ ЧТО они влиты. Мёртвым оказался другой предмет —
ДЕРЕВО при уже влитой ветке; их восемь, и это ровно то, что сторожит храповик
деревьев (стоит на 22, база обязана вернуться к 18).

Запуск:  python scripts/tools/branch-worktree-map.py [путь-к-дереву]
"""

import os
import subprocess
import sys

TREE = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else u".").replace(u"\\", u"/")


def git(*args, **kw):
    tree = kw.get("tree", TREE)
    r = subprocess.run([u"git", u"-C", tree] + list(args), capture_output=True)
    return r.returncode, r.stdout.decode("utf-8", "replace").replace(u"\r\n", u"\n").strip()


def out(line=u""):
    sys.stdout.buffer.write(line.encode("utf-8") + b"\n")


def worktrees():
    _, porcelain = git(u"worktree", u"list", u"--porcelain")
    items, cur = [], {}
    for line in porcelain.split(u"\n"):
        if line.startswith(u"worktree "):
            if cur:
                items.append(cur)
            cur = {u"path": line[9:].strip(), u"branch": u"(detached)"}
        elif line.startswith(u"branch "):
            cur[u"branch"] = line[len(u"branch refs/heads/"):].strip()
    if cur:
        items.append(cur)
    return items


def branch_facts(b):
    rc_anc, _ = git(u"merge-base", u"--is-ancestor", b, u"main")
    _, cnt = git(u"rev-list", u"--count", u"main..%s" % b)
    _, cnt_nm = git(u"rev-list", u"--count", u"--no-merges", u"main..%s" % b)
    _, stat = git(u"diff", u"--stat", u"main...%s" % b)
    _, when = git(u"log", u"-1", u"--format=%ad", u"--date=short", b)
    return {
        u"merged": rc_anc == 0,
        u"count": cnt, u"count_nomerge": cnt_nm,
        u"diff": stat.split(u"\n")[-1].strip() if stat.strip() else u"",
        u"when": when,
    }


def dirt(path):
    u"""Незакоммиченное РАЗДЕЛЬНО: отслеживаемое и брошенный мусор. Слитые в одно
    число, они врут в обе стороны — скретч-каталог читается как работа, а живая
    правка исходника теряется среди мусора."""
    r = subprocess.run([u"git", u"-C", path, u"status", u"--porcelain"], capture_output=True)
    lines = [l for l in r.stdout.decode("utf-8", "replace").split(u"\n") if l.strip()]
    tracked = [l for l in lines if not l.startswith(u"??")]
    untracked = [l for l in lines if l.startswith(u"??")]
    return tracked, untracked


_, local_main = git(u"rev-parse", u"--short", u"main")
_, origin_main = git(u"rev-parse", u"--short", u"origin/main")
_, lr = git(u"rev-list", u"--left-right", u"--count", u"origin/main...main")

out(u"# Карта веток и деревьев")
out()
out(u"ТОЧКА ОТСЧЁТА: локальный `main` = %s; `origin/main` = %s; "
    u"`git rev-list --left-right --count origin/main...main` = %s (позади / впереди)."
    % (local_main, origin_main, lr.replace(u"\t", u" / ")))
out()

_, branches = git(u"branch", u"--no-merged", u"main", u"--format=%(refname:short)")
names = [b.strip() for b in branches.split(u"\n") if b.strip()]
wt_by_branch = dict((w[u"branch"], w[u"path"]) for w in worktrees())

out(u"## Несли́тые ветки — `git branch --no-merged main` (%d)" % len(names))
out()
out(u"| ветка | влита (merge-base) | сверх main: всего / без слияний | diff --stat main...ветка | дерево | последний коммит |")
out(u"|---|---|---|---|---|---|")
alive = dead = 0
for b in names:
    f = branch_facts(b)
    if f[u"merged"] or not f[u"diff"]:
        dead += 1
    else:
        alive += 1
    out(u"| `%s` | %s | %s / %s | %s | %s | %s |"
        % (b, u"ДА" if f[u"merged"] else u"нет", f[u"count"], f[u"count_nomerge"],
           f[u"diff"] or u"**ПУСТО**", wt_by_branch.get(b, u"нет"), f[u"when"]))
out()
out(u"Несут работу: **%d**; влиты либо без отличий (кандидаты в снятие ВЕТКИ): **%d**."
    % (alive, dead))
out()

items = worktrees()
out(u"## Рабочие деревья — `git worktree list` (%d)" % len(items))
out()
out(u"| дерево | ветка | влита | сверх main | изменено отслеживаемых | брошено неотслеживаемых | последний коммит |")
out(u"|---|---|---|---|---|---|---|")
cand, held, junk = [], [], []
for w in items:
    b = w[u"branch"]
    tracked, untracked = dirt(w[u"path"])
    short = w[u"path"].replace(u"D:/Sources/nv-lang/", u"")
    if b == u"(detached)":
        out(u"| `%s` | (открепл.) | — | — | %d | %d | — |" % (short, len(tracked), len(untracked)))
        continue
    f = branch_facts(b)
    out(u"| `%s` | `%s` | %s | %s | %d | %d | %s |"
        % (short, b, u"ДА" if f[u"merged"] else u"нет", f[u"count"],
           len(tracked), len(untracked), f[u"when"]))
    if f[u"merged"] and not tracked and not untracked:
        cand.append((short, b))
    elif f[u"merged"] and tracked:
        held.append((short, b, len(tracked)))
    elif f[u"merged"] and untracked:
        junk.append((short, b, len(untracked)))
out()
out(u"**Ветка влита И дерево чисто — %d.** Снятие согласует интегратор с окном-хозяином: "
    u"«чисто» говорит о файлах, а не о том, что окно не работает прямо сейчас." % len(cand))
for c in cand:
    out(u"* `%s` (ветка `%s`)" % c)
out()
out(u"**Влита, но есть ИЗМЕНЁННЫЕ отслеживаемые файлы — %d. НЕ ТРОГАТЬ:** это живая "
    u"незакоммиченная работа, и снос уничтожает её без следа." % len(held))
for h in held:
    out(u"* `%s` (ветка `%s`, изменённых файлов %d)" % h)
out()
out(u"**Влита, а в дереве брошен НЕОТСЛЕЖИВАЕМЫЙ мусор — %d.** Он сам по себе нарушает "
    u"правило о временных файлах (их место — скретчпад сессии), но и он бывает чьим-то "
    u"черновиком: спросить хозяина дешевле, чем восстанавливать." % len(junk))
for j in junk:
    out(u"* `%s` (ветка `%s`, неотслеживаемых %d)" % j)
