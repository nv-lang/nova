# -*- coding: utf-8 -*-
"""scripts/claude-hooks/remind-session-save.py — записка интегратора обязана
обновляться РАЗ В ЧАС, и следит за этим механизм, а не память окна.

ЗАЧЕМ. 2026-09-16 владелец спросил, где и как часто сохраняется контекст сессии.
Ответ был плохим: записка `docs/dev/prompts/integrator-handoff.md` не
обновлялась СЕМНАДЦАТЬ ЧАСОВ работы — правило существовало, механизма не было.
Правило без механизма есть симптом, а не фикс (план 290 пункт 3).

ЧТО ДЕЛАЕТ. После вызова инструмента смотрит возраст записки. Старше часа —
печатает напоминание с возрастом и командой. Не блокирует НИЧЕГО: напоминание,
ставшее препятствием, выключат первым же днём, и правило исчезнет вместе с ним.

ПОЧЕМУ ВОЗРАСТ ФАЙЛА, А НЕ СВОЙ СЧЁТЧИК. Счётчик живёт в процессе окна и умирает
вместе с ним — то есть врёт ровно в том случае, ради которого всё затевалось
(сессия оборвалась). Возраст файла принадлежит ДЕРЕВУ и переживает любое окно.

ПОЧЕМУ С ОСТУДОЙ. Напоминание на каждый вызов инструмента — это шум, а шум
читают как фон. Остуда хранится в файле, а не в памяти: то же рассуждение.

ТИХИЙ ПО УМОЛЧАНИЮ: если записки нет (чужой проект, свежий клон) — молчит.
"""
import io
import json
import os
import subprocess
import sys
import time

# Вывод — ВСЕГДА UTF-8, а не кодировка консоли. Поймано СОБСТВЕННЫМ самотестом:
# на Windows stdout по умолчанию cp1251, и русский текст доезжал искажённым — то
# есть напоминание было бы нечитаемым ровно там, где оно нужно. Тот же класс,
# что ловит guard-shell-nonascii, и ровно та причина, по которой страж требует
# самотест у каждого хука: руками хуки не гоняют.
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

# РОЛЬ РЕШАЕТ, ПРО КАКУЮ ЗАПИСКУ НАПОМИНАТЬ (правка 2026-09-16 по вопросу
# владельца «куда записывает сессию ты и Карина?»). До неё хук был слеп: он
# срабатывал в ЛЮБОМ окне и звал `/save`, у которой в шапке написано «кому:
# главному интегратору». То есть окно Карины получало напоминание про чужой
# файл и чужую команду — напоминание, исполнить которое нельзя.
#
# Роль определяется ВЕТКОЙ, а не памятью и не именем сессии (оно меняется при
# каждом перезапуске): `main` — интегратор, ветка Карины — окно Карины.
# Незнакомая ветка — молчим, и это не дыра: у пакетных окон своя передача
# (`/stop`), а выдумывать им адресата хук не вправе.
# Ключ таблицы — ВЕТКА, и этого хватает не всем ролям: контролёр пушей
# (`/push-controller`) работает в ГЛАВНОМ дереве, то есть на `main`, как и
# интегратор. По ветке они неразличимы, и до правки 2026-09-18 контролёр
# получал напоминание про ЧУЖУЮ записку — то есть указание, исполнить которое
# он не мог, не испортив чужой файл. Роль такого окна берётся ВИЗИТКОЙ (см.
# role_from_card ниже): визитка несёт `session_id`, и сопоставление идёт по
# нему, а не по имени сессии — имя меняется при каждом перезапуске.
ROLES = (
    ("main", os.path.join("docs", "dev", "prompts", "integrator-handoff.md"),
     "/save", "ЗАПИСКА ИНТЕГРАТОРА"),
    ("p274-novac", os.path.join("docs", "dev", "prompts", "carina-handoff.md"),
     "/save", "ЗАПИСКА ОКНА КАРИНЫ"),
)

# ВИЗИТКА НАЗЫВАЕТ РОЛЬ ПРЯМО, и потому решает для ВСЕХ ролей, а не только для
# тех, кого не различает ветка. Вторая причина слепоты ветки найдена окном
# Карины 2026-09-18 и она отдельная: у окна, работающего в WORKTREE,
# `CLAUDE_PROJECT_DIR` указывает на ГЛАВНОЕ дерево, так что ветка читается
# оттуда — `main`, — и окно Карины получало напоминание про записку
# ИНТЕГРАТОРА. Таблица веток при этом верна; неверен был корень, а корень хук
# не выбирает.
#
# Поэтому порядок такой: визитка (роль сказана) → ветка (роль угадана по
# дереву). Ветка остаётся запасным путём: окно могло не успеть написать
# визитку, и тогда прежнее поведение лучше молчания.
CARD_ROLES = {
    "controller": (os.path.join("docs", "dev", "prompts", "controller-handoff.md"),
                   "/save", "ЗАПИСКА КОНТРОЛЁРА"),
    "carina": (os.path.join("docs", "dev", "prompts", "carina-handoff.md"),
               "/save", "ЗАПИСКА ОКНА КАРИНЫ"),
    "integrator": (os.path.join("docs", "dev", "prompts", "integrator-handoff.md"),
                   "/save", "ЗАПИСКА ИНТЕГРАТОРА"),
}
MAX_AGE_SEC = 3600            # час — число владельца (план 290 п.3)
COOLDOWN_SEC = 900            # не чаще раза в 15 минут


def current_branch(root):
    """Ветка берётся у git, а не из окружения: окружение врёт в worktree."""
    # `symbolic-ref` ПЕРВЫМ, а не `rev-parse --abbrev-ref`: второй отвечает
    # пустотой в репозитории БЕЗ КОММИТОВ (HEAD ещё не разрешается), а именно
    # такой делает самотест — и молчание хука читалось бы как «роль не найдена».
    # Замер 2026-09-16: два случая самотеста провалились ровно на этом.
    try:
        import subprocess
        for args in (("symbolic-ref", "--short", "HEAD"),
                     ("rev-parse", "--abbrev-ref", "HEAD")):
            out = subprocess.run(["git", "-C", root] + list(args),
                                 capture_output=True, text=True, timeout=10)
            name = (out.stdout or "").strip()
            if name and name != "HEAD":
                return name
        return ""
    except Exception:
        return ""

def role_from_card(root):
    """Роль ЭТОЙ сессии по визитке в общем `.git`, либо None.

    Сопоставление по `session_id`, а не по имени: имя вида `nova-NN` меняется
    при каждом перезапуске, и визитка соседа с тем же именем увела бы
    напоминание в чужую записку. Визитка — ПОДСКАЗКА, а не власть (решение
    владельца 2026-09-16), и здесь этого достаточно: цена ошибки — напоминание
    не тому окну, а не правка файла.
    """
    sid = os.environ.get("CLAUDE_CODE_SESSION_ID", "").strip()
    if not sid:
        return None
    try:
        import subprocess
        out = subprocess.run(["git", "-C", root, "rev-parse", "--git-common-dir"],
                             capture_output=True, text=True, timeout=10)
        gitdir = (out.stdout or "").strip()
        if not gitdir:
            return None
        if not os.path.isabs(gitdir):
            gitdir = os.path.join(root, gitdir)
        for name in os.listdir(gitdir):
            if not (name.startswith("nova-session-") and name.endswith(".card")):
                continue
            fields = {}
            for line in io.open(os.path.join(gitdir, name),
                                encoding="utf-8", errors="replace"):
                if "=" in line:
                    k, v = line.split("=", 1)
                    fields[k.strip()] = v.strip()
            if fields.get("session_id") == sid:
                return fields.get("role")
    except Exception:
        return None
    return None


def session_root():
    """Дерево ЭТОЙ сессии, а не главное дерево проекта.

    Найдено окном Карины 2026-09-18 (реестр 221.1 №1156), и замер сошёлся до
    минуты: хук требовал `/save` через двадцать минут после того, как записка
    была написана и закоммичена, и печатал «не обновлялась 43ч 46м» — возраст
    КОПИИ в главном дереве, тогда как рабочая копия в `nova-p274` была свежей.

    Причина названа чтением: `CLAUDE_PROJECT_DIR` указывает на ГЛАВНОЕ дерево,
    а по `AGENTS.md` всякое окно, кроме интегратора, работает в СВОЁМ worktree.
    Значит для целого КЛАССА адресатов напоминание было невыполнимым: сколько
    ни сохраняйся, оно вернётся через час и замолчит только после чужого
    слияния. Требование, которое нельзя удовлетворить, учит себя игнорировать —
    а хук заведён ровно потому, что правило без механизма не сработало
    (план 290 п.3).

    САМ ОТВЕТ ЖИВЁТ В `hook_tree.py`: тот же вопрос задают ещё два хука, и
    копия, написанная по памяти, не унаследует тонкость с чтением пути
    БАЙТАМИ (см. шапку того файла).
    """
    try:
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        from hook_tree import session_root as _sr
        return _sr()
    except Exception:
        return os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()

CARD_STALE_SEC = 12 * 3600   # тот же порог, что у `session-card.sh read`


def _card_is_stale(card):
    """Визитка старше 12 часов — след, а не адрес: судим как при её отсутствии.

    Своя копия, а не импорт из `guard-stop-v2.py`: хук обязан быть
    самодостаточным. Чтобы копии не разошлись, самотест ЗАГРУЖАЕТ оба модуля и
    требует одинакового ответа на одном файле — расхождение ловится механизмом.

    Причина и замер — в `guard-stop-v2.py`, `card_is_stale` (единственный дом
    объяснения; здесь ссылка, чтобы две копии текста не разошлись).
    """
    try:
        stamp = 0.0
        for line in io.open(card, encoding="utf-8", errors="replace"):
            if line.startswith("epoch="):
                try:
                    stamp = float(line.split("=", 1)[1].strip())
                except ValueError:
                    stamp = 0.0
                break
        if stamp <= 0:
            stamp = os.path.getmtime(card)
        return (time.time() - stamp) > CARD_STALE_SEC
    except Exception:
        return False


def _branch_role_belongs_to_me(root):
    """Ветка называет РОЛЬ, но не говорит, ЧЬЁ это окно.

    Находка окна nova-1a 2026-09-18: разовое окно, работавшее в ГЛАВНОМ дереве по
    просьбе владельца, получило напоминание «записка ИНТЕГРАТОРА не обновлялась».
    Хук посчитал верно — ветка `main` действительно принадлежит интегратору, — и
    всё равно адресовал не тому: у окна нет этой роли, и исполнить указание оно
    может только испортив чужой файл.

    Это ЗЕРКАЛО случая окна Карины (там роль не совпадала с веткой, здесь окно без
    роли сидит в ветке роли), и лечится тем же: ВИЗИТКА знает `session_id` того,
    кто роль ведёт. Если визитка роли существует и её id НЕ совпадает с моим —
    напоминание адресовано не мне, и хук молчит.

    ЕСЛИ ВИЗИТКИ НЕТ ВОВСЕ — прежнее поведение (адресация по ветке). Окно роли
    могло не успеть её написать, и молчание тогда хуже ложного адреса: записка
    перестала бы напоминать о себе совсем.
    """
    sid = os.environ.get("CLAUDE_CODE_SESSION_ID", "").strip()
    if not sid:
        return True          # id неизвестен — судить не по чему, ведём как раньше
    try:
        import subprocess
        out = subprocess.run(["git", "-C", root, "rev-parse", "--git-common-dir"],
                             capture_output=True, text=True, timeout=10)
        gitdir = (out.stdout or "").strip()
        if not gitdir:
            return True
        if not os.path.isabs(gitdir):
            gitdir = os.path.join(root, gitdir)
        card = os.path.join(gitdir, "nova-session-integrator.card")
        if not os.path.isfile(card):
            return True      # визитки роли нет — прежнее поведение
        if _card_is_stale(card):
            return True      # визитка протухла — тоже прежнее поведение
        owner = ""
        for line in io.open(card, encoding="utf-8", errors="replace"):
            if line.startswith("session_id="):
                owner = line.split("=", 1)[1].strip()
                break
        if not owner:
            return True
        return owner == sid
    except Exception:
        return True


ESCAPES_COOLDOWN_SEC = 3600   # раз в час — «почасовое напоминание» плана 292 Ш.5


def emit_context(text):
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": text,
        }
    }, ensure_ascii=False))


def escapes_note(root):
    u"""Строка о побегах Stop-стража — или пусто (план 292 Ш.5).

    Разбор лога НЕ повторяется здесь: он живёт в `scripts/tools/stop-escapes.py`,
    и вторая копия разошлась бы с первой на первой же правке формата строки.
    Хук зовёт тот же дом чтения, что и `/status`.

    Цена под контролем: печать не чаще раза в час, и ДО запуска процесса
    проверяется отметка времени — иначе `PostToolUse` платил бы за python на
    каждом вызове инструмента.

    Своя поломка молчит НАМЕРЕННО и только здесь: это напоминание, а не судья;
    сломавшись, оно не имеет права мешать работе окна. Про молчание отказа у
    СУДЕЙ действует обратное правило — см. `guard-stop-v2.py`.
    """
    try:
        stamp = os.path.join(root, "target", ".stop-escapes-reminder")
        if os.path.isfile(stamp) and time.time() - os.path.getmtime(stamp) < ESCAPES_COOLDOWN_SEC:
            return u""
        tool = os.path.join(root, "scripts", "tools", "stop-escapes.py")
        if not os.path.isfile(tool):
            return u""
        r = subprocess.run([sys.executable, tool, root, "--quiet"],
                           capture_output=True, timeout=20)
        text = r.stdout.decode("utf-8", "replace").strip()
        if not text:
            return u""
        os.makedirs(os.path.dirname(stamp), exist_ok=True)
        io.open(stamp, "w", encoding="utf-8").write(str(time.time()))
        return (u"%s\n  Побег — это случай, когда страж остановки НЕ СМОГ судить и "
                u"отпустил окно.\n  Смотреть: `python scripts/tools/stop-escapes.py .`; "
                u"если страж не прав — сказать интегратору\n  с цитатой отказа, а не "
                u"обходить его молча." % text)
    except Exception:
        return u""


def main():
    try:
        sys.stdin.read()
    except Exception:
        pass
    # Корень — дерево ЭТОЙ сессии: и ветка, и путь к записке обязаны браться
    # из ОДНОГО места. Прежде ветка читалась у git, а файл — по
    # `CLAUDE_PROJECT_DIR`, и в worktree это были разные деревья.
    root = session_root()
    branch = current_branch(root)
    HANDOFF = CMD = TITLE = None
    # ВИЗИТКА СТАРШЕ ВЕТКИ, и только в эту сторону: она называет роль прямо,
    # тогда как ветка её лишь угадывает по дереву. Обратный порядок вернул бы
    # контролёру записку интегратора — ровно ту ошибку, ради которой правка.
    card = role_from_card(root)
    if card in CARD_ROLES:
        HANDOFF, CMD, TITLE = CARD_ROLES[card]
    elif _branch_role_belongs_to_me(root):
        for _b, _h, _c, _t in ROLES:
            if branch == _b:
                HANDOFF, CMD, TITLE = _h, _c, _t
                break
    if HANDOFF is None:
        return 0          # роль незнакома — молчим, см. ROLES
    path = os.path.join(root, HANDOFF)
    if not os.path.isfile(path):
        return 0

    escapes = escapes_note(root)

    age = time.time() - os.path.getmtime(path)
    if age < MAX_AGE_SEC:
        # Записка свежа — но побеги Stop-стража от её свежести не зависят и
        # обязаны быть видны (план 292 Ш.5): лог, которого никто не читает,
        # равен отсутствию лога.
        if escapes:
            emit_context(escapes)
        return 0

    stamp = os.path.join(root, "target", ".session-save-reminder")
    try:
        if os.path.isfile(stamp) and time.time() - os.path.getmtime(stamp) < COOLDOWN_SEC:
            return 0
    except OSError:
        pass

    hours = int(age // 3600)
    mins = int((age % 3600) // 60)
    msg = (
        u"{title} НЕ ОБНОВЛЯЛАСЬ {h}ч {m:02d}м (предел — час).\n"
        u"  Сохрани контекст: команда {cmd}. Она дописывает раздел состояния\n"
        u"  СВЕРХУ в {path}, беря факты из дерева.\n"
        u"  Почему это не пожелание: 2026-09-16 семнадцать часов работы не были\n"
        u"  записаны нигде, кроме истории git (план 290 п.3)."
    ).format(title=TITLE, cmd=CMD, path=HANDOFF, h=hours, m=mins)

    try:
        os.makedirs(os.path.dirname(stamp), exist_ok=True)
        io.open(stamp, "w", encoding="utf-8").write(str(time.time()))
    except OSError:
        pass

    # Побеги приписываются к тому же сообщению, а не шлются вторым: два
    # напоминания подряд читаются как шум, и первым перестают читать оба.
    emit_context(msg + (u"\n\n" + escapes if escapes else u""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
