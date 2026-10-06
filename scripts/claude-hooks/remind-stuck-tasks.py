# -*- coding: utf-8 -*-
"""scripts/claude-hooks/remind-stuck-tasks.py — задача, принятая и НЕ убранная,
видна интегратору механизмом, а не его вниманием.

ЗАЧЕМ (вопрос владельца 2026-10-06: «если бы я не сказал, ты бы не знал, что
что-то не так»). Задача #9 принята 02:49, а `peer_task cleaned` ей плагин не дал:
он судит хвосты по КОММИТУ, и на только что влитом коммите уже стояли ветки
новых задач #13/#14 (ложное срабатывание плагина). Интегратор ответил приёмщику
«с твоей стороны больше ничего не нужно» — и задача 11,5 часа висела в статусе
«принята», занимая место в `inflight_limit` проекта. Строку «#9 принята» в
`peer_task list` интегратор видел минимум дважды и не заметил; вскрылось, только
когда лимит не дал поставить задачу первого приоритета.

ЧТО ДЕЛАЕТ. После вызова инструмента в сессии ИНТЕГРАТОРА (роль берётся из
файла ролей плагина по `OPENCODE_SESSION_ID`) читает задачи проекта из данных
плагина и называет каждую, что стоит в статусе `accepted` дольше STUCK_SEC.
Не блокирует ничего; с остудой — шум перестают читать.

ЧТО ДЕЛАТЬ ПО НАПОМИНАНИЮ: проверить хвосты задачи (дерево, ветка, integrate/t<N>)
и написать её приёмщику «повтори `peer_task cleaned {n}`» — если мешает чужая
ветка на влитом коммите, повтор проходит после первого коммита той задачи.

ТИХИЙ: нет `OPENCODE_SESSION_ID` (окно не во вкладке OpenCode), сессия не держит
роль интегратора, нет данных плагина — молчит.
"""
import io
import json
import os
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

STUCK_SEC = 30 * 60      # приёмщик убирает за минуты; полчаса — уже застряло
COOLDOWN_SEC = 900       # не чаще раза в 15 минут


def peers_root():
    u"""Корень данных плагина: `$XDG_DATA_HOME/opencode/nova-peers` (та же дверь,
    что в guard-stop-v2.py; машинного пути в коде нет)."""
    base = (os.environ.get("XDG_DATA_HOME") or u"").strip()
    if not base:
        base = os.path.join(os.path.expanduser(u"~"), u".local", u"share")
    return os.path.join(base, u"opencode", u"nova-peers")


def load_json(p):
    try:
        with io.open(p, encoding="utf-8", errors="replace") as fh:
            return json.load(fh)
    except Exception:
        return None


def integrator_projects(root, sid):
    u"""Проекты, где эта сессия держит роль интегратора (`roles/<проект>_integrator.json`)."""
    out = []
    d = os.path.join(root, u"roles")
    try:
        names = os.listdir(d)
    except OSError:
        return out
    for nm in names:
        if not nm.endswith(u"_integrator.json"):
            continue
        r = load_json(os.path.join(d, nm)) or {}
        if r.get(u"session") == sid:
            out.append(nm[: -len(u"_integrator.json")])
    return out


def stuck_tasks(root, project, now_ms):
    d = os.path.join(root, u"tasks", project)
    out = []
    try:
        names = os.listdir(d)
    except OSError:
        return out
    for nm in names:
        if not nm.endswith(u".json"):
            continue
        t = load_json(os.path.join(d, nm)) or {}
        if t.get(u"status") != u"accepted":
            continue
        upd = t.get(u"updated") or 0
        if not isinstance(upd, (int, float)) or now_ms - upd < STUCK_SEC * 1000:
            continue
        out.append((t.get(u"n"), int((now_ms - upd) / 60000), t.get(u"reviewer") or u"?"))
    out.sort(key=lambda x: (x[0] is None, x[0]))
    return out


def stamp_path():
    u"""Отметка остуды — в сборочном каталоге дерева сессии, не в %TEMP% (№1153)."""
    try:
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        from hook_tree import session_root
        tree = session_root()
    except Exception:
        tree = os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    return os.path.join(tree, u"target", u".remind-stuck-tasks")


def main():
    try:
        sys.stdin.read()
    except Exception:
        pass
    sid = (os.environ.get("OPENCODE_SESSION_ID") or u"").strip()
    if not sid:
        return 0
    root = peers_root()
    projects = integrator_projects(root, sid)
    if not projects:
        return 0
    now_ms = time.time() * 1000
    found = []
    for pr in projects:
        for n, mins, rev in stuck_tasks(root, pr, now_ms):
            found.append(u"#%s (%dч %02dм, приёмщик %s)" % (n, mins // 60, mins % 60, rev))
    if not found:
        return 0
    p = stamp_path()
    try:
        if os.path.isfile(p) and time.time() - os.path.getmtime(p) < COOLDOWN_SEC:
            return 0
    except OSError:
        pass
    try:
        os.makedirs(os.path.dirname(p), exist_ok=True)
        io.open(p, "w", encoding="utf-8").write(u"%d\n" % int(time.time()))
    except OSError:
        pass
    msg = (u"ЗАДАЧИ ПРИНЯТЫ, НО НЕ УБРАНЫ (`cleaned` не сделан): %s. Каждая занимает "
           u"место в inflight_limit проекта. Проверь хвосты (дерево, ветка, integrate/t<N>) "
           u"и напиши приёмщику: «повтори `peer_task cleaned {n}`»; если плагин отказывает "
           u"из-за чужой ветки на влитом коммите — повтор после первого коммита той задачи. "
           u"«Больше ничего не нужно» в этом случае не говорить: так #9 провисела 11,5 ч "
           u"(2026-10-06)." % u", ".join(found))
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": msg,
        }
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
