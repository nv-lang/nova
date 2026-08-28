# -*- coding: utf-8 -*-
"""scripts/guards/check-context-layer-budget.py — ВЕСЬ постоянный слой контекста
не растёт молча и не приезжает дырявым.

Адрес: реестр 221.1 №774, план 276 шаг 1.

ПРЕЖНЕЕ ИМЯ — `check-after-compact-budget.py` (переименован 2026-08-29, история
сохранена через `git mv`). Судил он ровно половину предмета: впрыск ПОСЛЕ СЖАТИЯ.
Вторая половина — цепочка `@`-импортов корневого `CLAUDE.md`, приезжающая в КАЖДОЕ
окно при СТАРТЕ сессии, — не мерилась никем, хотя стоит дороже: `AGENTS.md` один
весит больше, чем весь список после сжатия.

ПОЧЕМУ ОДИН СТРАЖ, А НЕ ДВА. Черновик плана предлагал завести второго стража с
собственной базой. Это дало бы два дома на пересекающийся предмет — те же три
файла команд входят в ОБЕ половины, — и на первой же правке дома разошлись бы.
Предмет один: «сколько текста приезжает в окно, не будучи спрошенным».

ПОЧЕМУ ДВА ПОТОЛКА, А НЕ ОДИН ОБЩИЙ. Половины живут своей жизнью и считаются
ПО-РАЗНОМУ, и это не придирка:
  * после сжатия инжектор СНИМАЕТ YAML-шапку — значит и мы снимаем;
  * при старте сессии шапка приезжает КАК ТЕКСТ (это записано в самом
    `CLAUDE.md`: «шапка команды (`---`) и слово `$ARGUMENTS` при импорте видны
    как текст») — значит считаем файл целиком.
Один общий потолок скрыл бы, какая половина выросла, а именно это и нужно знать.

ПОТОЛОК, А НЕ ХРАПОВИК — форма взята у `gate-budget.baseline`. Храповик краснел бы
на каждой прозаической правке `flow.md`: сократил абзац — изволь опустить базу.
Такой шум кончается тем, что базу правят не глядя.

ПРОВЕРЯЕТ ШЕСТЬ ВЕЩЕЙ:
  1. файл потолков есть и разбирается: ДВА именованных числа, байты;
  2. хук ЧИТАЕТ список — в тексте инжектора есть имя `after-compact.list`.
     Механизм, который можно вырезать одной строкой и не заметить, — не механизм;
  3. КАЖДЫЙ файл списка существует. Пропавший файл инжектор пропускает со строкой
     в stderr и кодом 0 — правило перестаёт приезжать в окна, а stderr хука в
     контекст не попадает. Молчание читается как успех (класс реестра №770);
  4. сумма ТЕЛ списка (шапка снята) не выше потолка `after-compact`;
  5. КАЖДАЯ цель `@`-импорта существует. Битый импорт — это молча непривезённое
     правило, ровно та же болезнь, что и пункт 3, только на другом конце;
  6. сумма ЦЕЛИКОМ взятых файлов цепочки импортов (вместе с самим `CLAUDE.md`,
     он тоже приезжает) не выше потолка `session-start`.

ЧЕГО НЕ ПРОВЕРЯЕТ: разумность самих потолков и содержимое файлов. Первое задаёт
замер, второе — их собственные стражи.

$1 — корень; $2 — override пути к списку; $3 — override пути к потолкам;
$4 — override пути к корневому файлу импортов (швы самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-context-layer-budget"

# Первый символ пути ОБЯЗАН допускать точку: три из четырёх импортов — это
# `@.claude/commands/...`. На этом же и попался первый вариант стража: он нашёл
# два файла вместо пяти и был бы зелён, пропустив большую половину слоя.
# `@path.md` — и в начале строки, и внутри фразы: в `CLAUDE.md` импорт AGENTS.md
# стоит ВНУТРИ заголовка («## Правила приезжают сами: @AGENTS.md»), и греп по
# `^@` его не видит. Это не гипотетический случай — на нём и попались 2026-08-29.
IMPORT_RE = re.compile(r"@([A-Za-z0-9_.][A-Za-z0-9_./-]*\.md)")


def body_without_frontmatter(text):
    """Ровно то, что делает инжектор: между первой парой строк `---` — шапка."""
    lines = text.split("\n")
    if lines and lines[0].strip() == "---":
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                return "\n".join(lines[i + 1:]).strip("\n")
    return text.strip("\n")


def entries(path):
    rows = []
    for line in path.read_text(encoding="utf-8").split("\n"):
        s = line.strip()
        if s and not s.startswith("#"):
            rows.append(s)
    return rows


def read_caps(cap_file):
    """Две именованные строки `ключ = число`. Возвращает (caps, ошибка)."""
    caps = {}
    for line in cap_file.read_text(encoding="utf-8").split("\n"):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        if "=" not in s:
            return None, f"строка {s!r} не вида `ключ = число`"
        k, _, v = s.partition("=")
        k, v = k.strip(), v.strip()
        if not v.isdigit():
            return None, f"у ключа {k!r} значение {v!r} — не число"
        caps[k] = int(v)
    missing = {"after-compact", "session-start"} - set(caps)
    if missing:
        return None, f"нет ключей: {', '.join(sorted(missing))}"
    return caps, None


def import_closure(entry, root):
    """Транзитивное замыкание `@`-импортов от entry. (файлы, битые ссылки)."""
    seen, order, broken = set(), [], []
    queue = [entry]
    while queue:
        cur = queue.pop(0)
        key = str(cur.resolve()) if cur.exists() else str(cur)
        if key in seen:
            continue
        seen.add(key)
        if not cur.is_file():
            broken.append(cur)
            continue
        order.append(cur)
        text = cur.read_text(encoding="utf-8", errors="replace")
        for rel in IMPORT_RE.findall(text):
            # Путь разрешается от папки импортирующего файла, с откатом на
            # корень: так же ведёт себя загрузчик, и так же — наш `CLAUDE.md`.
            cand = cur.parent.joinpath(*rel.split("/"))
            if not cand.is_file():
                alt = root.joinpath(*rel.split("/"))
                cand = alt if alt.is_file() else cand
            queue.append(cand)
    return order, broken


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    lst = pathlib.Path(a[2]) if len(a) > 2 else root / ".claude" / "after-compact.list"
    cap_file = (pathlib.Path(a[3]) if len(a) > 3
                else root / "scripts" / "guards" / "context-layer-budget.baseline")
    entry = pathlib.Path(a[4]) if len(a) > 4 else root / "CLAUDE.md"
    injector = root / "scripts" / "claude-hooks" / "inject-after-compact.py"

    if not lst.is_file() and not entry.is_file():
        print(f"{NAME} ok: судить нечего — ни списка {lst}, ни корня импортов {entry}")
        return 0

    if not cap_file.is_file():
        print(f"{NAME}: FAIL — нет файла потолков {cap_file}: постоянный слой "
              f"контекста не ограничен ничем", file=sys.stderr)
        return 1
    caps, err = read_caps(cap_file)
    if err:
        print(f"{NAME}: FAIL — потолки {cap_file} не разобраны: жду ДВЕ строки "
              f"`after-compact = <байты>` и `session-start = <байты>`; {err}",
              file=sys.stderr)
        return 1

    # ── половина 1: впрыск после сжатия ──────────────────────────────────────
    ac_total, ac_files = 0, 0
    if lst.is_file():
        if not injector.is_file():
            print(f"{NAME}: FAIL — нет инжектора {injector}, а список есть: "
                  f"список без читателя ничего не гарантирует", file=sys.stderr)
            return 1
        if "after-compact.list" not in injector.read_text(encoding="utf-8"):
            print(f"{NAME}: FAIL — инжектор {injector.name} НЕ читает список: "
                  f"механизм выхолощен, впрыск больше не управляется списком",
                  file=sys.stderr)
            return 1
        rows = entries(lst)
        ac_files = len(rows)
        missing = []
        for rel in rows:
            p = root.joinpath(*rel.split("/"))
            if not p.is_file():
                missing.append(rel)
                continue
            ac_total += len(body_without_frontmatter(
                p.read_text(encoding="utf-8", errors="replace")).encode("utf-8"))
        if missing:
            print(f"{NAME}: FAIL — файлов из списка нет на диске: {len(missing)}",
                  file=sys.stderr)
            for rel in missing:
                print(f"    {rel}", file=sys.stderr)
            print("    Хук пропустит их со строкой в stderr и кодом 0 — то есть правило",
                  file=sys.stderr)
            print("    перестанет приезжать в окна, а stderr хука в контекст не попадает.",
                  file=sys.stderr)
            return 1

    # ── половина 2: цепочка импортов при старте сессии ───────────────────────
    ss_total, ss_files = 0, 0
    if entry.is_file():
        chain, broken = import_closure(entry, root)
        if broken:
            print(f"{NAME}: FAIL — целей `@`-импорта нет на диске: {len(broken)}",
                  file=sys.stderr)
            for p in broken:
                print(f"    {p}", file=sys.stderr)
            print("    Битый импорт — молча непривезённое правило: окно стартует без него,",
                  file=sys.stderr)
            print("    и заметить это может только тот, кто помнит, что оно должно быть.",
                  file=sys.stderr)
            return 1
        ss_files = len(chain)
        # Целиком, БЕЗ снятия шапки: при импорте она видна как текст.
        ss_total = sum(len(p.read_text(encoding="utf-8", errors="replace").encode("utf-8"))
                       for p in chain)

    over = []
    if ac_total > caps["after-compact"]:
        over.append(("after-compact", ac_total, caps["after-compact"]))
    if ss_total > caps["session-start"]:
        over.append(("session-start", ss_total, caps["session-start"]))

    line = (f"после сжатия {ac_total}/{caps['after-compact']} байт ({ac_files} файлов), "
            f"старт сессии {ss_total}/{caps['session-start']} байт ({ss_files} файлов), "
            f"весь слой {ac_total + ss_total}")

    if over:
        print(f"{NAME}: FAIL — {line}", file=sys.stderr)
        for half, got, cap in over:
            print(f"    половина `{half}` вышла за потолок: {got} > {cap}", file=sys.stderr)
        print("    Это едет в КАЖДОЕ окно, поэтому число растёт молча и стоит дороже,",
              file=sys.stderr)
        print("    чем кажется на диффе одного файла. Либо сократи, либо подними тот",
              file=sys.stderr)
        print(f"    потолок в {cap_file.name} ТЕМ ЖЕ диффом и напиши там, что понадобилось.",
              file=sys.stderr)
        return 1

    print(f"{NAME} ok: {line}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
