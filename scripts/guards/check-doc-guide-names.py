# -*- coding: utf-8 -*-
"""scripts/guards/check-doc-guide-names.py — форма, НАЗВАННАЯ в публикуемой
доке, обязана существовать в языке.

Адрес: план 241 (канон публикации `docs/guide`), реестр 221.1 — строка заведена
слиянием, которое принесло стража. Дом правила: `docs/dev/doc-conventions.md`.

ЗАЧЕМ — случай 2026-09-17, дословно. Страница `docs/guide/caller-location.md`
была опубликована на сайт и учила форме `debug_assert`, ретрактированной планом
194 (A4) и удалённой из прелюдии. Нашёл её ВЛАДЕЛЕЦ, прочитав живую страницу.
Пять моих проверок публикации молчали, и молчали справедливо: сборка, ссылки,
якоря, коды 200 и вердикты деплоя отвечают на вопрос «ДОЕХАЛО ЛИ», а страница с
выдуманным синтаксисом доезжает ровно так же хорошо, как верная.

ЧТО СУДИТСЯ. Только `docs/guide/**` — это ЕДИНСТВЕННОЕ, что попадает на сайт
(`docs/dev/` не публикуется никогда). Предмет — НАЗВАННАЯ ВЫЗЫВАЕМАЯ ФОРМА:
голый snake_case-идентификатор в обратных кавычках, за которым стоит `(`,
то есть `` `debug_assert(` ``. Такое сужение выбрасывает прозу целиком — пути,
флаги, типы, английские слова в кавычках, — и оставляет ровно то, что читатель
прочтёт как «так пишут на Nova».

ИСТИНА БЕРЁТСЯ ИЗ ДЕРЕВА, А НЕ ИЗ СПИСКА ЗАПРЕТНЫХ СЛОВ. Это главное отличие от
провалившейся первой попытки того же дня: список «плохих имён», составленный по
памяти, дал 179 попаданий, и ВСЕ проверенные оказались ложными — `null` в доке
про указатели есть проза про C NULL, `let f = File::open(path)?` есть RUST в
блоке сравнения, `external fn` — действующий синтаксис (D82). Здесь наоборот:
собирается множество СУЩЕСТВУЮЩИХ имён — ключевые слова из лексера плюс все
объявления `fn` в `std/src`, — и красным становится то, чего в нём нет.

БАЗА — СПИСОК, А НЕ ЧИСЛО. В доке законно называются формы, которых нет в
`std/src`: функции C в FFI-поваренной книге (`_alloc`, `_free`), внутренности
CLI (`detect_toolchain`), интринсики компилятора (`size_of`, `align_of`,
`embed`) — они живут строковыми литералами в кодогене, а не объявлением `fn`.
Каждое такое имя вносится в базу ОДНОЙ строкой с причиной. Список, а не число,
потому что именно имя и есть предмет: счётчик разрешил бы заменить одно чужое
имя другим молча.

ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно, чтобы вердикт не читали шире, чем он есть):
  * формы БЕЗ скобок — `ro`, `consume`, `#debug`: их в прозе не отличить от
    английских слов, и попытка судить их даёт ложняки;
  * верность ОПИСАНИЯ формы: что `assert` существует, страж знает; что абзац
    про него не врёт — нет;
  * код внутри ```nova-блоков: его судит компилятор, а не этот страж;
  * доку вне `docs/guide/` — она не публикуется.

usage: python scripts/guards/check-doc-guide-names.py [КОРЕНЬ]
Самотест: scripts/guards/selftest/test-check-doc-guide-names.sh
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-doc-guide-names"

# Предмет: `имя(` в обратных кавычках. Три символа минимум — короче идут
# английские слова вроде `if(`.
CALL = re.compile(r"`([a-z_][a-z0-9_]{2,})\(")
# Объявление функции в .nv: свободной или метода (`fn T @name(`).
DECL = re.compile(r"\bfn\s+(?:[A-Za-z_][\w\[\], ]*\s+)?@?([a-z_][a-z0-9_]*)\s*[\(\[]")
KW = re.compile(r'"([a-z_]+)"\s*=>\s*TokenKind::')


def known_names(root):
    """Множество СУЩЕСТВУЮЩИХ имён: лексер + все `fn` в std/src."""
    names = set()
    lex = os.path.join(root, "compiler-codegen", "src", "lexer", "mod.rs")
    if os.path.isfile(lex):
        names |= set(KW.findall(io.open(lex, encoding="utf-8",
                                        errors="replace").read()))
    std = os.path.join(root, "std", "src")
    for r, _d, fs in os.walk(std):
        for f in fs:
            if f.endswith(".nv"):
                t = io.open(os.path.join(r, f), encoding="utf-8",
                            errors="replace").read()
                names |= set(DECL.findall(t))
    return names


def baseline_names(root):
    p = os.path.join(root, "scripts", "guards", "doc-guide-names.baseline")
    if not os.path.isfile(p):
        return set(), p
    out = set()
    for line in io.open(p, encoding="utf-8", errors="replace"):
        line = line.split("#", 1)[0].strip()
        if line:
            out.add(line)
    return out, p


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    guide = os.path.join(root, "docs", "guide")
    if not os.path.isdir(guide):
        print("%s ok: судить нечего (нет docs/guide)" % NAME)
        return 0

    known = known_names(root)
    if len(known) < 20:
        print("%s: FAIL — множество известных имён подозрительно мало (%d): "
              "источник истины не прочитан, и тогда КАЖДОЕ имя выглядело бы "
              "неизвестным" % (NAME, len(known)), file=sys.stderr)
        return 1

    allowed, bpath = baseline_names(root)

    seen = {}
    for f in sorted(os.listdir(guide)):
        if not f.endswith(".md"):
            continue
        t = io.open(os.path.join(guide, f), encoding="utf-8",
                    errors="replace").read()
        for m in CALL.finditer(t):
            seen.setdefault(m.group(1), set()).add(f)

    unknown = sorted(n for n in seen if n not in known and n not in allowed)
    stale = sorted(n for n in allowed if n not in seen)

    print("%s: имён известно %d, названо в гайдах %d, в базе %d, "
          "неизвестных сверх базы %d"
          % (NAME, len(known), len(seen), len(allowed), len(unknown)))
    if stale:
        print("%s: в базе есть имена, больше не названные в доке (%d) — "
              "их можно снять: %s"
              % (NAME, len(stale), ", ".join(stale[:8])))

    if unknown:
        print("%s: НАЗВАНА ФОРМА, КОТОРОЙ НЕТ В ЯЗЫКЕ:" % NAME, file=sys.stderr)
        for n in unknown:
            print("    %-28s %s" % (n, ", ".join(sorted(seen[n])[:3])),
                  file=sys.stderr)
        print("", file=sys.stderr)
        print("    Два законных исхода, и выбор между ними — суждение человека:",
              file=sys.stderr)
        print("    (1) это НАСТОЯЩАЯ форма Nova — тогда дока врёт, и чинится ДОКА",
              file=sys.stderr)
        print("        (так было с `debug_assert` 2026-09-17: форма снята планом",
              file=sys.stderr)
        print("        194, а страница учила ей на публичном сайте);",
              file=sys.stderr)
        print("    (2) это ЧУЖОЕ имя — функция C, внутренность CLI, интринсик —",
              file=sys.stderr)
        print("        тогда оно вносится в %s строкой С ПРИЧИНОЙ." % bpath,
              file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    print("%s ok: каждая названная в публикуемой доке форма существует в языке"
          % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
