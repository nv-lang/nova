# -*- coding: utf-8 -*-
"""scripts/guards/check-spec-amend-places.py -- амендмент ВНУТРИ существующего D-блока
обязан назвать остальные места, где то же правило записано.

ЗАЧЕМ (слово владельца 2026-10-02: «спека должна быть однозначно понятна из любого
места; если D изменяется позже, должна быть ссылка на новый D», и без указания
владельца, для всего класса, а не для одного блока).
Носитель в реестре: реестр 221.1 №1597 (D433 и D405 — одно правило в двух блоках,
правка легла в один, другой молчал).

КЛАСС. Решение меняет правило, записанное в спеке в НЕСКОЛЬКИХ D-блоках, а правку
кладут в одно место -- абзацем-амендментом внутри старого блока. Остальные места
молчат, и агент, прочитавший старое, получает старое правило. Носитель: правило
литералов легло амендментом внутрь D55, а D44, D54, D227, D405, D486 о нём не знали;
исправлено выносом в D489 с разделом `### Что заменено`. Страж
check-dblock-supersede-pointers держит указатели только у блоков, НАЗВАННЫХ в таком
разделе; ничто не заставляло ни завести раздел, ни НАЙТИ все места. Амендмент
проходил молча. Этот страж -- та половина.

ЧТО СУДИТСЯ (один коммит: файл сообщения + staged-дифф, как check-novac-commit-*).
  1. Дифф ДОБАВЛЯЕТ в spec/decisions/*.md строку-амендмент
     (`> **Амендмент`, `> **Уточнение`, `> **Amendment`, `**Амендмент ...`) внутри
     блока, заголовок которого в этом диффе НЕ добавлен. Новый блок целиком --
     не амендмент (его держит страж указателей).
  2. Сообщение обязано нести трейлер `Spec-places:` одной из двух форм:
     (а) `Spec-places: D44, D54, D227 (spec-reader)` -- блоки, где то же правило ещё
         записано. Страж проверяет по ДЕРЕВУ: в первых 8 непустых строках под
         заголовком каждого названного блока есть строка на `>`, называющая
         амендированный блок. Нет -- отказ с перечнем.
     (б) `Spec-places: only here -- <причина, 5+ слов>` -- правило записано только тут.
  3. Нет трейлера -- отказ с рецептом: места ищет агент spec-reader по вопросу «где ещё
     записано это правило»; если мест больше одного -- лучше НОВЫЙ D-блок с
     `### Что заменено`, а не амендмент.

ЧЕСТНАЯ ГРАНИЦА: полноту перечня мест страж не знает -- «only here» и список можно
написать неправду. Он заставляет НАЗВАТЬ места и проверяет, что названные действительно
указывают на амендированный блок; найти места -- работа spec-reader, не страж.

КЛАПАН: NOVA_SPEC_PLACES_NA="<причина>" -- громко пропускает (опечатка в заголовке,
механический перенос текста без смены правила). Слияние (MERGE_HEAD) не судится.

usage: python scripts/guards/check-spec-amend-places.py <файл-сообщения> [КОРЕНЬ]
"""
import io
import os
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import spec_dblocks as sdb  # noqa: E402  (общий разбор заголовков и указателей)

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-spec-amend-places"
RE_AMEND = re.compile(r"^\s*(>\s*)?\*\*(Амендмент|Уточнение|Amendment)\b")
# A DATED mark ANYWHERE in the line counts too: an italic `*Уточнение 2026-10-02 (...)*` in the
# middle of a rule, or `(уточнение 2026-10-02: ...)` -- the integrator's own D84 edit of
# 2026-10-02 used both forms and passed as "no amendment" (the hole this line closes).
# The date is what makes it an amendment: the bare word "уточнение" in prose is not judged.
RE_AMEND_DATED = re.compile(r"(?i)(Амендмент|Уточнение|Поправка|Amendment|AMEND)\b[^\n]{0,40}?\b20\d\d-\d\d-\d\d")
RE_HUNK = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")
RE_TRAILER = re.compile(r"^[A-Z][A-Za-z-]*:")
RE_ONLY = re.compile(r"^only here\s*(--|—|–|-)\s*(.*)$", re.I)


def git(root, *args):
    r = subprocess.run(["git", "-C", root] + list(args), capture_output=True)
    return r.stdout.decode("utf-8", "replace")


def trailer(msg, key):
    lines = msg.split("\n")
    for i, l in enumerate(lines):
        if not l.startswith(key + ":"):
            continue
        out = [l[len(key) + 1:].strip()]
        for nxt in lines[i + 1:]:
            if not nxt.strip() or RE_TRAILER.match(nxt):
                break
            out.append(nxt.strip())
        return " ".join(out).strip()
    return None


def added_lines(diff):
    """-> {путь: {номер строки нового файла: текст}} по добавленным строкам."""
    out = {}
    path = None
    n = 0
    for ln in diff.replace("\r\n", "\n").split("\n"):
        if ln.startswith("+++ "):
            path = ln[4:].strip()
            path = path[2:] if path.startswith("b/") else None if path == "/dev/null" else path
            if path:
                out.setdefault(path, {})
            continue
        m = RE_HUNK.match(ln)
        if m:
            n = int(m.group(1))
            continue
        if path and ln.startswith("+") and not ln.startswith("+++"):
            out[path][n] = ln[1:]
            n += 1
    return out


def amended_blocks(root, diff):
    """Амендменты внутри СТАРЫХ блоков: [(номер блока, путь, строка, текст)]."""
    found = []
    for path, adds in added_lines(diff).items():
        if not re.match(r"^spec/decisions/[^/]+\.md$", path):
            continue
        staged = git(root, "show", ":" + path).replace("\r\n", "\n").split("\n")
        for n, text in sorted(adds.items()):
            if not RE_AMEND.match(text) and not RE_AMEND_DATED.search(text):
                continue
            head = None
            for k in range(min(n, len(staged)) - 1, -1, -1):
                m = sdb.HEAD.match(staged[k])
                if m:
                    head = (k + 1, m.group(2))
                    break
            if head is None or head[0] in adds:
                continue   # вне блока или заголовок добавлен тем же коммитом -- новый блок
            found.append((head[1], path, n, text.strip()[:90]))
    return found


def fail(*lines):
    for l in lines:
        print(l, file=sys.stderr)
    return 1


def main():
    a = sys.argv
    msg_path = a[1] if len(a) > 1 else ""
    root = a[2] if len(a) > 2 else str(pathlib.Path(__file__).resolve().parents[2])

    text = None
    if msg_path and os.path.exists(msg_path):
        text = io.open(msg_path, encoding="utf-8", errors="replace").read()
    if not text or not text.strip():
        print("%s ok: судить нечего (сообщение пусто или его нет -- так зовут проверку "
              "запускаемости; настоящий коммит приходит через хук commit-msg)" % NAME)
        return 0

    gd = git(root, "rev-parse", "--absolute-git-dir").strip()
    if gd and os.path.exists(os.path.join(gd, "MERGE_HEAD")):
        print("%s ok: слияние не судится (MERGE_HEAD)" % NAME)
        return 0

    diff = git(root, "diff", "--cached", "-U0", "--no-color", "--", "spec/decisions")
    amended = amended_blocks(root, diff)
    if not amended:
        print("%s ok: амендментов внутри существующих D-блоков в индексе нет" % NAME)
        return 0

    na = os.environ.get("NOVA_SPEC_PLACES_NA", "").strip()
    if na:
        print("%s ok: КЛАПАН NOVA_SPEC_PLACES_NA=%r -- места правила не судятся "
              "(амендментов: %d)" % (NAME, na, len(amended)))
        return 0

    nums = sorted({b for b, _p, _n, _t in amended}, key=int)
    listing = ["    D%s %s:%d  %s" % (b, p, n, t) for b, p, n, t in amended]
    recipe = [
        "  Правило, записанное в нескольких D-блоках, нельзя менять амендментом в одном:",
        "  остальные места продолжат говорить старое. Как найти места: агент",
        "  .claude/agents/spec-reader.md по вопросу «где ещё записано это правило».",
        "  Мест больше одного -- заведи НОВЫЙ D-блок с `### Что заменено` (номер даёт",
        "  интегратор), старые получат указатели; страж указателей это держит.",
        "  Место одно -- оставь амендмент и впиши трейлер:",
        "    Spec-places: only here -- <причина, не меньше пяти слов>",
        "  Места найдены и несут указатель `> ...D%s...` под заголовком:" % nums[0],
        "    Spec-places: D44, D54, D227 (spec-reader)",
    ]
    val = trailer(text.replace("\r", ""), "Spec-places")
    if val is None:
        return fail("%s: FAIL -- амендмент внутри существующего D-блока без трейлера "
                    "`Spec-places:`:" % NAME, *listing, *recipe)

    m = RE_ONLY.match(val)
    if m:
        words = [w for w in re.split(r"\s+", m.group(2)) if re.search(r"\w", w)]
        if len(words) < 5:
            return fail("%s: FAIL -- `Spec-places: only here` без причины (нужно 5+ слов, "
                        "есть %d): %r" % (NAME, len(words), m.group(2)), *recipe)
        print("%s ok: амендментов %d в блоках %s, заявлено «only here» с причиной"
              % (NAME, len(amended), ", ".join("D" + n for n in nums)))
        return 0

    places = []
    for n in re.findall(r"\bD(\d+)\b", val):
        if n not in places:
            places.append(n)
    if not places:
        return fail("%s: FAIL -- в `Spec-places:` не назван ни один D-блок и нет формы "
                    "`only here -- причина`: %r" % (NAME, val), *recipe)

    _files, headings, _supers = sdb.scan(root)
    bad = []
    for p in places:
        if p in nums:
            bad.append("D%s -- это сам амендированный блок, а не другое место" % p)
            continue
        hs = headings.get(p)
        if not hs:
            bad.append("D%s -- заголовок не найден в spec/decisions (опечатка?)" % p)
            continue
        if not any(sdb.has_pointer(lines, ln - 1, new) for (_f, ln, lines) in hs for new in nums):
            f, ln, _ = hs[0]
            bad.append("D%s (%s:%d) не несёт под заголовком строки `> ...D%s...` "
                       "(первые %d непустых строк)" % (p, f, ln, "/D".join(nums), sdb.LOOKAHEAD))
    if bad:
        return fail("%s: FAIL -- названные места не указывают на амендированный блок:" % NAME,
                    *["    " + b for b in bad],
                    "  Поставь под заголовком места `> **Уточнено [D%s](...)**: ...` и "
                    "включи в тот же коммит." % nums[0])
    print("%s ok: амендментов %d в блоках %s, мест названо и проверено %d (%s)"
          % (NAME, len(amended), ", ".join("D" + n for n in nums), len(places),
             ", ".join("D" + p for p in places)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
