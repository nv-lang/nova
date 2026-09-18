# -*- coding: utf-8 -*-
u"""Stop-хук плана 292: остановка требует ДОКАЗАТЕЛЬСТВА, а продолжение не
требует ничего.

ПОДКЛЮЧЁН 2026-09-19: `.claude/settings.json`, ключ `Stop`. Прежний
`guard-stop.py` снят тем же коммитом вместе со своим самотестом — двое судей
с разными правилами дают отказы, о которых нельзя сказать, чей верен.

ЭТА ШАПКА ЛГАЛА ПОЛСУТОК, и урок дороже строки. Она говорила «НЕ ПОДКЛЮЧЁН»,
пока хук работал и блокировал окна; нашло это окно Карины, уткнувшись в
отказ и прочитав в файле, что его тут быть не может. Файл, неверно
описывающий СВОЮ ЖИВОСТЬ, хуже файла без комментария: читатель перестаёт
искать причину там, где она есть. Тот же класс, что «немой отказ в костюме
успеха», только наоборот — работает, а объявляет себя мёртвым.

ЗАЧЕМ. Владелец 2026-09-18: «работа по плану постоянно останавливается и в
интеграторе, и в Карине; /flow не помогает, /push-controller сам иногда
останавливается, /goal не помогает; как решить раз и навсегда». Замер по
одиннадцати стенограммам проекта: 1104 конца хода, из них с объявленным
следующим шагом 7, заблокировано v1-хуком 5. Один процент. Остальные
остановки МОЛЧАЛИВЫЕ — в них нет слов, по которым v1 судит.

ЧТО МЕНЯЕТСЯ. v1 ищет обещание в тексте и блокирует нарушенное. v2 не читает
намерений: он требует, чтобы конец хода НЁС ПРИЧИНУ КОДОМ, и проверяет код
against мира. Нет строки — блокируем; код есть — проверяем то, что можно
проверить машиной. Умолчание переворачивается: раньше продолжение держалось
на памяти окна, теперь на памяти держится остановка.

ФОРМА. Последний абзац доклада несёт строку:

    СТОП: очередь-пуста
    СТОП: вопрос
    СТОП: неавторизовано <действие>

Три кода — ровно три законные причины из `/flow`, других нет.

ЧТО ПРОВЕРЯЕТСЯ ПО КАЖДОМУ КОДУ.

* `очередь-пуста` — сверяется со снимком `target/queue.json`, который пишет
  `scripts/tools/queue-snapshot.py` (план 292 Ш.2). Снимка нет или он старше
  получаса — блокируем с требованием обновить; в снимке есть открытые пункты —
  блокируем и называем их. Снимок ВЫВОДИТСЯ командами (три зеркала,
  `--no-merged`, `registry-routes-scan.py`), то есть решение 2026-08-10
  «очередь спрашивается, а не ведётся» в силе: второго дома правды не
  появляется, появляется КЭШ ответа, который машина умеет прочитать в момент
  остановки.
* `вопрос` — в последнем абзаце обязан быть вопросительный знак, и предыдущий
  ход не должен был кончиться тем же кодом без единого вызова инструмента
  между ними. Два вопроса подряд без работы — это не вопрос, это пауза.
* `неавторизовано` — названное действие обязано быть из закрытого списка
  (пуш, тег, удаление, публикация, наружу). Пустое «неавторизовано» не
  доказательство.

ПРЕДОХРАНИТЕЛЬ, И ОН ОБЯЗАТЕЛЕН. Хук, который может запереть окно, однажды
запрёт. Поэтому: `stop_hook_active` пропускается (это делает и v1), а сверх
того считается число блокировок ПОДРЯД в файле состояния. Третья подряд
пропускается, а случай пишется в `target/.stop-guard/escapes.log` — чтобы
побег был виден владельцу, а не растворился в тишине. Счётчик сбрасывается
любым пропуском.

СО ВСЕМИ КОМАНДАМИ — АВТОМАТИЧЕСКИ, И ЭТО ТРЕБУЕТ ВОРОТ ПО РОЛИ. Stop-хук
настраивается в `settings.json` один раз и срабатывает на КОНЦЕ ЛЮБОГО хода,
какой бы командой тот ни начался; править команды ради него не нужно. Но из
этого же следует, что без ворот он спросил бы код у окна, которое отвечает на
вопрос владельца и никакой очереди не имеет. Поэтому первым делом определяется
РОЛЬ по ветке (`main` — интегратор, `p274-novac` — Карина, ветка со словом
`controller` — контролёр; переопределяется `NOVA_WINDOW_ROLE`). Роли нет —
хук молчит ВООБЩЕ: доказывать ему нечем, а шуметь он не вправе.

В ролевом окне `/explain`, `/status` и прочие «ответные» команды кода не
отменяют, и это НЕ недосмотр: ответ владельцу не опустошает очередь, а
`/flow` прямо говорит, что доклад — запятая. Ответил и взял следующий пункт.

ДВЕ ЗАКОННЫЕ КОНЦОВКИ СВЕРХ ТРЁХ КОДОВ, обе машинно проверяемые:
* прерывание владельцем (`[Request interrupted by user]`) — не остановка окна,
  а вмешательство; хук пропускает молча;
* `СТОП: смена` — окно сдаёт смену командой `/stop`; проверяется тем, что
  ролевая записка ДЕЙСТВИТЕЛЬНО записана в последние десять минут.

ЦЕНА. Ноль запусков процессов: хук читает стенограмму и два маленьких файла.
Git не зовётся ни разу — ветка берётся чтением `.git/HEAD`, очередь снимком.
"""
from __future__ import annotations

import io
import json
import os
import re
import sys
import time

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

# Сырой stdin держим глобально: обработчику падения он нужен, чтобы узнать cwd и
# `stop_hook_active` уже ПОСЛЕ того, как разбор упал.
RAW = u""


# УСЛОВИЕ, ПЕРЕЕХАВШЕЕ ИЗ СНЯТОГО v1 (2026-09-19). Оно жило в `guard-stop.py`
# по плану 276 и чуть не пропало вместе с ним: `docs/dev/recheck-common.md`
# ссылается на него как на МАШИННОЕ требование, и без переноса получилось бы
# правило, объявляющее принуждение, которого нет, — ровно то, за что
# `AGENTS.md` отчитывает строку про DCO.
#
# Судит УЗКО и намеренно: только доказуемый случай «работа агентов названа, а
# строки с моделями нет». Случай «агентов не пускал» остаётся на норме — по
# тексту его от «пускал и умолчал» не отличить.
AGENT_WORK = [
    u"запустил агент",
    u"пустил агент",
    u"пущен агент",
    u"делегировал",
    u"делегирую",
    u"отдаю агент",
    u"отдал агент",
    u"агент вернул",
    u"агент отчитал",
]

MODELS_LINE = u"модели агентов"


def emit(obj):
    u"""Печать решения БАЙТАМИ, а не через `print`.

    Кириллица в `print` на консоли cp1251 роняет хук UnicodeEncodeError, а
    упавший хук неотличим от снятого: пустой stdout — это пропуск. Ровно этот
    класс поломки интегратор наблюдал 2026-09-18 на кириллическом пути.
    """
    try:
        sys.stdout.buffer.write(json.dumps(obj, ensure_ascii=False).encode("utf-8"))
        sys.stdout.buffer.write(bytes([10]))
        sys.stdout.buffer.flush()
    except Exception:
        sys.stdout.write(json.dumps(obj, ensure_ascii=True) + chr(10))


STOP_RE = re.compile(
    u"^[\\s>*_-]*СТОП:\\s*(очередь-пуста|вопрос|неавторизовано|смена)\\b[ \\t]*(.*)$",
    re.IGNORECASE | re.MULTILINE,
)

INTERRUPT = u"[request interrupted"

# Ветка → роль. Дом соответствия — `/save`; здесь копия ТОЛЬКО потому, что хук
# не имеет права звать git, а знать роль обязан. Расхождение ловится Ш.3-самотестом.
ROLE_BY_BRANCH = [
    (u"p274-novac", u"carina"),
    (u"controller", u"controller"),
    (u"main", u"integrator"),
]

# Ролевые записки — для кода «смена» (их свежесть и есть доказательство).
ROLE_NOTE = {
    u"integrator": u"docs/dev/prompts/integrator-handoff.md",
    u"carina": u"docs/dev/prompts/carina-handoff.md",
    u"controller": u"docs/dev/prompts/controller-handoff.md",
}
NOTE_FRESH_SEC = 10 * 60

# Закрытый список необратимых действий (AGENTS.md «Git», /flow пункт 2).
IRREVERSIBLE = [u"пуш", u"push", u"тег", u"tag", u"удал", u"публик",
                u"наружу", u"force", u"релиз", u"слия", u"merge"]

QUEUE_MAX_AGE_SEC = 30 * 60
MAX_BLOCKS_IN_ROW = 2


def block(reason):
    emit({"decision": "block", "reason": reason})
    return True


def read_turns(path):
    u"""Возвращает список ходов: [(текст последнего сообщения, были ли вызовы
    инструментов после него)]. Нужен ровно для правила «два вопроса подряд»."""
    turns = []
    if not path or not os.path.exists(path):
        return turns
    cur_text = None
    tools_since = 0
    with io.open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except Exception:
                continue
            t = rec.get("type")
            if t == "assistant":
                msg = rec.get("message") or {}
                parts = msg.get("content") or []
                if isinstance(parts, str):
                    parts = [{"type": "text", "text": parts}]
                buf = []
                for p in parts:
                    if not isinstance(p, dict):
                        continue
                    if p.get("type") == "text":
                        buf.append(p.get("text") or "")
                    elif p.get("type") == "tool_use":
                        tools_since += 1
                if any(b.strip() for b in buf):
                    cur_text = "\n".join(buf)
            elif t == "user":
                if cur_text is not None:
                    turns.append((cur_text, tools_since))
                cur_text = None
                tools_since = 0
    if cur_text is not None:
        turns.append((cur_text, tools_since))
    return turns


def state_path(cwd, session):
    d = os.path.join(cwd or ".", "target", ".stop-guard")
    try:
        os.makedirs(d, exist_ok=True)
    except Exception:
        return None
    return os.path.join(d, "%s.json" % (session or "unknown"))


def load_state(p):
    if not p or not os.path.exists(p):
        return {"blocks": 0}
    try:
        with io.open(p, encoding="utf-8") as fh:
            return json.load(fh)
    except Exception:
        return {"blocks": 0}


def save_state(p, st):
    if not p:
        return
    try:
        with io.open(p, "w", encoding="utf-8") as fh:
            fh.write(json.dumps(st, ensure_ascii=False))
    except Exception:
        pass


def log_escape(cwd, text):
    d = os.path.join(cwd or ".", "target", ".stop-guard")
    try:
        os.makedirs(d, exist_ok=True)
        with io.open(os.path.join(d, "escapes.log"), "a", encoding="utf-8") as fh:
            fh.write(u"%s %s\n" % (time.strftime("%Y-%m-%d %H:%M"), text))
    except Exception:
        pass


def check_queue(cwd):
    u"""(ok, причина-отказа, побег). Снимок отсутствует/просрочен/непуст — не ok.

    ТРЕТЬЕ УСЛОВИЕ ИНТЕГРАТОРА (2026-09-18, приёмка Ш.2): отказ снимка НЕ ИМЕЕТ
    ПРАВА выглядеть пустой очередью. Различаются два «плохо», и это не одно и то же:

    * снимка нет или он просрочен — окно способно починить это САМО, одной
      командой, поэтому блокируем и говорим какой;
    * снимок есть, но сам себя объявил неполным (`ok: false` — не достучались до
      зеркала, не ответил `gh`) — окно починить это НЕ может. Блокировать значит
      запереть его за чужую сеть. Пропускаем, но пишем ПОБЕГ в лог: «не смог
      посчитать» обязан быть громким, а не выглядеть разрешением стоять.
    """
    p = os.path.join(cwd or ".", "target", "queue.json")
    if not os.path.exists(p):
        return False, u"", (
            u"Код «очередь-пуста» ничем не подтверждён: снимка `target/queue.json` нет. "
            u"Сними его — `python scripts/tools/queue-snapshot.py .` — и либо останови "
            u"ход с доказанным пустым снимком, либо бери из него следующий пункт."
        )
    age = time.time() - os.path.getmtime(p)
    if age > QUEUE_MAX_AGE_SEC:
        return False, (
            u"Снимку очереди %d мин, порог %d. Устаревший снимок доказательством "
            u"не считается: обнови `python scripts/tools/queue-snapshot.py .`."
            % (int(age // 60), QUEUE_MAX_AGE_SEC // 60)
        ), u""
    try:
        with io.open(p, encoding="utf-8") as fh:
            q = json.load(fh)
    except Exception as e:
        return False, u"Снимок очереди не читается (%s) — обнови его." % e, u""
    if q.get("ok") is False:
        bad = q.get("failed_sources") or [u"источник не назван"]
        return True, u"", (
            u"Снимок очереди НЕПОЛЕН (ok=false): %s. Остановка пропущена, "
            u"но пустой очередью это НЕ считается." % u"; ".join(unicode_str(b) for b in bad)
        )
    if q.get("role") == "none":
        return True, u"", u""
    items = q.get("open") or []
    if items:
        head = u"; ".join(unicode_str(i) for i in items[:5])
        more = u" (и ещё %d)" % (len(items) - 5) if len(items) > 5 else u""
        return False, (
            u"Очередь НЕ пуста: %d пункт(ов) — %s%s. Бери следующий сейчас, в этом "
            u"же ходе. Если пункт больше не нужен — сними его в снимке и объясни "
            u"строкой, а не молчанием." % (len(items), head, more)
        ), u""
    return True, u"", u""


def current_branch(cwd):
    u"""Ветка чтением файлов, без запуска git: в worktree `.git` — файл со
    строкой `gitdir: <путь>`, HEAD лежит там."""
    g = os.path.join(cwd or ".", ".git")
    try:
        if os.path.isfile(g):
            with io.open(g, encoding="utf-8", errors="replace") as fh:
                line = fh.read().strip()
            if line.startswith("gitdir:"):
                g = line.split(":", 1)[1].strip()
        head = os.path.join(g, "HEAD")
        with io.open(head, encoding="utf-8", errors="replace") as fh:
            h = fh.read().strip()
        if h.startswith("ref:"):
            return h.split("/", 2)[-1]
        return u""
    except Exception:
        return u""


def common_git_dir(cwd):
    u"""Общий `.git` — ЧТЕНИЕМ, без запуска git (правило этого хука).

    В главном дереве `.git` — каталог, он и есть общий. В worktree `.git` — файл
    со строкой `gitdir: <общий>/worktrees/<имя>`, и общий лежит на два уровня выше.
    """
    g = os.path.join(cwd or ".", u".git")
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


def role_belongs_to_me(cwd, role):
    u"""Ветка называет РОЛЬ, но не говорит, ЧЬЁ это окно.

    ЗАМЕР 2026-09-19, 02:18, первым же живым применением хука: окно `nova-90`,
    работавшее в ГЛАВНОМ дереве по просьбе владельца, было заблокировано как
    ИНТЕГРАТОР — хотя интегратор это другая сессия, а у окна роли нет вовсе.
    Ветка `main` действительно принадлежит интегратору, и хук посчитал верно —
    и всё равно потребовал доказательство по чужой очереди.

    Лечится тем же, чем уже вылечено в `remind-session-save.py`
    (`_branch_role_belongs_to_me`, находка окна nova-1a 2026-09-18): ВИЗИТКА роли
    знает `session_id` того, кто роль ведёт. Визитка есть и её id НЕ мой — роль
    не моя, и судить меня по ней нельзя. Второй дом правды здесь не заводится:
    визитка уже существует и уже читается другим хуком.

    ВИЗИТКИ НЕТ — прежнее поведение (судим по ветке). Окно роли могло не успеть
    её написать, и пропускать всех при её отсутствии значило бы выключить стража
    ровно тогда, когда роль поднимается заново.
    """
    sid = (os.environ.get("CLAUDE_CODE_SESSION_ID") or u"").strip()
    if not sid:
        return True
    g = common_git_dir(cwd)
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


def card_role(cwd):
    u"""Роль ИЗ ВИЗИТКИ — до всякой ветки.

    До 2026-09-19 визитка только ПОДТВЕРЖДАЛА роль, угаданную по ветке, и
    из-за этого целый класс окон был невидим стражу: ПОМОЩНИК работает в
    произвольной ветке (`research/...`), под правило ветки не попадает и
    получает `none`, то есть право молча закончить ход чем угодно. Заметил
    владелец. Лечится тем же приёмом, которым лечилась роль вообще: ветка
    УГАДЫВАЕТ, визитка РЕШАЕТ — значит визитка и должна уметь роль назвать.

    Читается только визитка, НАЗЫВАЮЩАЯ ЭТУ сессию: чужая карточка роли не
    даёт, иначе окно наследовало бы роль соседа по общему `.git`.
    """
    sid = (os.environ.get("CLAUDE_CODE_SESSION_ID") or u"").strip()
    if not sid:
        return u""
    g = common_git_dir(cwd)
    if not g:
        return u""
    try:
        names = os.listdir(g)
    except Exception:
        return u""
    for n in sorted(names):
        if not (n.startswith(u"nova-session-") and n.endswith(u".card")):
            continue
        role = n[len(u"nova-session-"):-len(u".card")].strip().lower()
        if not role:
            continue
        try:
            with io.open(os.path.join(g, n), encoding="utf-8",
                         errors="replace") as fh:
                for line in fh:
                    if line.startswith(u"session_id="):
                        if line.split(u"=", 1)[1].strip() == sid:
                            return role
                        break
        except Exception:
            continue
    return u""


def detect_role(cwd):
    env = (os.environ.get("NOVA_WINDOW_ROLE") or u"").strip().lower()
    if env:
        return env
    card = card_role(cwd)
    if card:
        return card
    br = current_branch(cwd).lower()
    if not br:
        return u"none"
    for needle, role in ROLE_BY_BRANCH:
        if needle in br:
            # Роль ветки — ещё не моя роль: см. role_belongs_to_me.
            return role if role_belongs_to_me(cwd, role) else u"none"
    return u"none"


def note_is_fresh(cwd, role):
    rel = ROLE_NOTE.get(role)
    if not rel:
        return False
    p = os.path.join(cwd or ".", rel)
    try:
        return (time.time() - os.path.getmtime(p)) <= NOTE_FRESH_SEC
    except Exception:
        return False


def unicode_str(x):
    if isinstance(x, dict):
        return x.get("title") or x.get("id") or json.dumps(x, ensure_ascii=False)
    return u"%s" % x


def main():
    try:
        data = json.loads(RAW or u"{}")
    except Exception as e:
        # Единственный немой пропуск, который остаётся, и он вынужденный: без
        # полезной нагрузки неизвестны ни cwd, ни роль, а блокировать вслепую
        # значит запереть любое окно проекта. Но след оставляем.
        log_escape(os.environ.get("CLAUDE_PROJECT_DIR") or u".",
                   u"вход хука не разобран (%s) — ход пропущен вслепую" % e)
        return 0

    if data.get("stop_hook_active"):
        return 0

    cwd = data.get("cwd") or os.environ.get("CLAUDE_PROJECT_DIR") or "."

    # ВОРОТА ПО РОЛИ — до всего остального. Окно без роли (исследование, пакет,
    # разовая волна) хук не судит: очереди у него нет, доказывать нечем.
    role = detect_role(cwd)
    if role == u"none":
        return 0

    sp = state_path(cwd, data.get("session_id"))
    st = load_state(sp)

    turns = read_turns(data.get("transcript_path") or "")
    if not turns:
        return 0
    text = turns[-1][0]
    tail = text[-900:]

    # Прерывание владельца — не остановка окна: он вмешался сам.
    if INTERRUPT in text.lower():
        st["blocks"] = 0
        save_state(sp, st)
        return 0

    # Предохранитель: третья блокировка подряд не ставится.
    if st.get("blocks", 0) >= MAX_BLOCKS_IN_ROW:
        st["blocks"] = 0
        save_state(sp, st)
        log_escape(cwd, u"пропуск после %d блокировок подряд" % MAX_BLOCKS_IN_ROW)
        return 0

    # Строка «Модели агентов:» ищется по ВСЕМУ ходу, а не по хвосту: работа
    # агентов называется в середине доклада, а строка — в конце.
    whole = text.lower()
    if any(w in whole for w in AGENT_WORK) and MODELS_LINE not in whole:
        st["blocks"] = st.get("blocks", 0) + 1
        save_state(sp, st)
        block(
            u"В ходе есть работа агентов, но нет строки «Модели агентов: …». "
            u"Требование живёт в `/delegate` и в `docs/dev/recheck-common.md`: "
            u"без неё экономию лимитов нельзя ни проверить, ни повторить. "
            u"Назови агентов, их модели и работу — и одной фразой почему дороже "
            u"haiku, если дороже."
        )
        return 0

    m = STOP_RE.search(tail)
    if not m:
        st["blocks"] = st.get("blocks", 0) + 1
        save_state(sp, st)
        block(
            u"Ход окончен без причины. Остановка требует доказательства: последняя "
            u"строка доклада обязана нести код — «СТОП: очередь-пуста» (сверяется со "
            u"снимком `target/queue.json`), «СТОП: вопрос» (в абзаце есть вопрос), "
            u"«СТОП: неавторизовано <действие>» (пуш, тег, удаление, публикация) или "
            u"«СТОП: смена» (сдача смены через `/stop`, записка обновлена). "
            u"Если причины нет — значит очередь не пуста: бери следующий пункт сейчас, "
            u"в этом же ходе."
        )
        return 0

    code = m.group(1).lower()
    arg = (m.group(2) or u"").strip()

    ok = True
    reason = u""

    if code == u"очередь-пуста":
        ok, reason, escape = check_queue(cwd)
        if escape:
            log_escape(cwd, escape)

    elif code == u"вопрос":
        if u"?" not in tail:
            ok, reason = False, (
                u"Код «вопрос» без вопроса: в последнем абзаце нет ни одного знака "
                u"вопроса. Либо задай вопрос, либо это не причина остановки."
            )
        elif len(turns) >= 2:
            prev_text, tools_between = turns[-2][0], turns[-1][1]
            prev = STOP_RE.search(prev_text[-900:])
            if prev and prev.group(1).lower() == u"вопрос" and tools_between == 0:
                ok, reason = False, (
                    u"Второй вопрос подряд без единого действия между ними. Вопрос — "
                    u"причина остановки, когда без ответа работа НЕ идёт; если идёт — "
                    u"делай её, а вопрос задай в конце сделанного."
                )

    elif code == u"смена":
        if role not in ROLE_NOTE:
            # Роль без записки (помощник) смену не сдаёт: его передача — это
            # ДОКЛАД ведущему окну плюс закоммиченная работа. Пускать сюда
            # значило бы принять код, доказательства которому не существует.
            ok, reason = False, (
                u"Код «смена» у роли «%s» не доказуем: ролевой записки у неё нет. "
                u"Помощник сдаёт не смену, а ЗАДАНИЕ: закоммить работу и доложи "
                u"ведущему окну, тогда законный код — «СТОП: очередь-пуста»: "
                u"снимок проверит, что незакоммиченного не осталось."
                % role
            )
        elif not note_is_fresh(cwd, role):
            ok, reason = False, (
                u"Код «смена» без сданной смены: ролевая записка (%s) не обновлялась "
                u"последние %d минут. Сдача смены — это ЗАПИСЬ, а не слово: выполни "
                u"`/stop`, потом заканчивай ход."
                % (ROLE_NOTE.get(role, u"?"), NOTE_FRESH_SEC // 60)
            )

    elif code == u"неавторизовано":
        low = arg.lower()
        if not any(w in low for w in IRREVERSIBLE):
            ok, reason = False, (
                u"Код «неавторизовано» без названного действия. Назови его словом из "
                u"закрытого списка: пуш, тег, удаление, публикация, слияние. Всё "
                u"остальное авторизовано самой задачей и остановкой не является."
            )

    if ok:
        st["blocks"] = 0
        save_state(sp, st)
        return 0

    st["blocks"] = st.get("blocks", 0) + 1
    save_state(sp, st)
    block(reason)
    return 0


def crashed(exc):
    u"""ХУК УПАЛ — и это обязано быть громким.

    ПРЕДУПРЕЖДЕНИЕ ИНТЕГРАТОРА (2026-09-18, к приёмке Ш.3): проверять надо не
    только поломку ИСТОЧНИКА данных, но и поломку САМОГО скрипта — исключение,
    битый JSON, отсутствующий python. У него в тот день фикс прошёл самотест и
    всё равно молча ушёл в запасную ветку на кириллическом пути.

    Почему это опаснее отказа источника: отказ источника снимок хотя бы
    называет (`ok: false`), а упавший скрипт не говорит НИЧЕГО — Claude Code
    видит ненулевой код возврата и пустой stdout и пропускает ход. Хук,
    молчащий при поломке, неотличим от снятого, и заметить это нельзя ничем.
    Поэтому падение превращается в блокировку с именем ошибки и местом.

    Запереть окно это не может: на следующем заходе Claude Code ставит
    `stop_hook_active`, и он читается ЗДЕСЬ, из сырого входа, — то есть до
    любого кода, который мог упасть.
    """
    import traceback
    where = u"?"
    try:
        tb = traceback.extract_tb(sys.exc_info()[2])
        if tb:
            where = u"%s:%s" % (os.path.basename(tb[-1][0]), tb[-1][1])
    except Exception:
        pass
    cwd = os.environ.get("CLAUDE_PROJECT_DIR") or u"."
    active = False
    try:
        payload = json.loads(RAW or u"{}")
        cwd = payload.get("cwd") or cwd
        active = bool(payload.get("stop_hook_active"))
    except Exception:
        pass
    text = u"ХУК УПАЛ: %s в %s (%s)" % (type(exc).__name__, where, exc)
    log_escape(cwd, text + (u" — повтор, пропускаем" if active else u""))
    if active:
        return 0
    block(
        u"%s. Это не разрешение остановиться: хук не смог проверить причину, а "
        u"молчащий страж неотличим от снятого. Либо закончи ход законным кодом и "
        u"он пройдёт следующим заходом, либо почини хук — след в "
        u"`target/.stop-guard/escapes.log`." % text
    )
    return 0


if __name__ == "__main__":
    # Вход читается ДО всего, что может упасть: обработчику падения он нужен,
    # чтобы увидеть `stop_hook_active` и не запереть окно повтором блокировки.
    try:
        RAW = sys.stdin.read() or u"{}"
    except Exception:
        RAW = u"{}"
    try:
        rc = main()
    except BaseException as exc:  # намеренно ВСЁ, включая SystemExit из main
        rc = crashed(exc)
    sys.exit(rc)
