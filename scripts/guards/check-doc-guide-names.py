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

ВТОРОЙ ПРЕДМЕТ — СНЯТЫЕ ФОРМЫ БЕЗ СКОБОК (правка 2026-09-17 по слову владельца
«закроешь?»). Первая редакция объявляла их непроверяемыми: `external fn`,
`readonly`, `let x =` — это слова, а не вызовы, и в прозе их 42 штуки, из них
подавляющее большинство ЗАКОННЫ (дока объясняет саму ретракцию: «планы 118.6/
118.7 сняли `addr_of()` в пользу `&x`»). Судить их по смыслу нельзя — отличить
«учит форме» от «объясняет, что форма снята» машина не может.

Поэтому судится НЕ смысл, а РОСТ: каждая клетка «файл × форма» внесена в базу с
её нынешним числом, и красным становится ПРИБАВЛЕНИЕ. Новая страница, назвавшая
снятую форму, краснеет; новое упоминание в старой странице краснеет; удаление
упоминания требует опустить базу — храповик ходит вниз.

СПИСОК ФОРМ ПРОВЕРЯЕТ СЕБЯ САМ. У каждой записан КОД ДИАГНОСТИКИ, которым
компилятор её отвергает, и страж требует, чтобы код существовал в
`compiler-codegen/src`. Пропал код — значит форма разснята или переименована, и
список протух: это отказ, а не тихое «ничего не нашли». Ровно этого не хватало
первой попытке того же дня — там список жил в голове и разошёлся с деревом.

ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно, чтобы вердикт не читали шире, чем он есть):
  * верность ОПИСАНИЯ формы: что `assert` существует, страж знает; что абзац
    про него не врёт — нет;
  * код внутри ```nova-блоков: его судит `check-doc-examples.sh` по тому же
    списку диагностик — это его дом, и здесь он не дублируется;
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


# Снятые формы БЕЗ скобок. У каждой — код, которым компилятор её отвергает;
# существование кода проверяется, иначе список протухает молча.
RETRACTED = [
    ("external-fn", r"`external\s+fn\b", "E_EXTERNAL_FN_RETRACTED"),
    ("readonly", r"`readonly\b", "E_KW_REMOVED_READONLY"),
    ("let-binding", r"`let\s+[a-z_]", "E_KW_REMOVED_LET"),
    ("addr-of", r"`addr_of(?:_mut)?\(", "E_ADDR_OF_REMOVED"),
    ("ptr-ro", r"`\*ro\s", "E_REDUNDANT_POINTER_RO"),
    ("ptr-unsafe", r"`\*unsafe\s", "E_UNSAFE_TYPE_MODIFIER_RENAMED"),
]
FENCE = re.compile(r"```.*?```", re.S)


def compiler_blob(root):
    parts = []
    for r, _d, fs in os.walk(os.path.join(root, "compiler-codegen", "src")):
        for f in fs:
            if f.endswith(".rs"):
                parts.append(io.open(os.path.join(r, f), encoding="utf-8",
                                     errors="replace").read())
    return "\n".join(parts)


def retracted_counts(guide):
    """Клетки «файл × форма» -> число упоминаний В ПРОЗЕ (вне ```-блоков)."""
    out = {}
    for f in sorted(os.listdir(guide)):
        if not f.endswith(".md"):
            continue
        t = FENCE.sub("", io.open(os.path.join(guide, f), encoding="utf-8",
                                  errors="replace").read())
        for tag, rx, _code in RETRACTED:
            n = len(re.findall(rx, t))
            if n:
                out["%s|%s" % (f, tag)] = n
    return out


def retracted_baseline(root):
    """Строки вида `файл|форма=N` из той же базы; комментарии игнорируются."""
    p = os.path.join(root, "scripts", "guards", "doc-guide-names.baseline")
    out = {}
    if not os.path.isfile(p):
        return out
    for line in io.open(p, encoding="utf-8", errors="replace"):
        line = line.split("#", 1)[0].strip()
        if "|" in line and "=" in line:
            k, v = line.rsplit("=", 1)
            try:
                out[k.strip()] = int(v.strip())
            except ValueError:
                pass
    return out


def baseline_names(root):
    p = os.path.join(root, "scripts", "guards", "doc-guide-names.baseline")
    if not os.path.isfile(p):
        return set(), p
    out = set()
    for line in io.open(p, encoding="utf-8", errors="replace"):
        line = line.split("#", 1)[0].strip()
        # Строки второго предмета (`файл|форма=N`) сюда не относятся: без этого
        # отсечения они читались бы как ИМЕНА и молча разрешали бы что попало.
        if line and "|" not in line:
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

    # --- ВТОРОЙ ПРЕДМЕТ: снятые формы без скобок, судятся РОСТОМ ---
    blob = compiler_blob(root)
    stale_codes = [c for _t, _r, c in RETRACTED if c and c not in blob]
    now = retracted_counts(guide)
    base = retracted_baseline(root)
    grown = sorted(k for k, n in now.items() if n > base.get(k, 0))
    shrunk = sorted(k for k, n in now.items() if n < base.get(k, 0))
    gone = sorted(k for k in base if k not in now)

    print("%s: снятых форм в прозе — клеток %d, упоминаний %d (база %d клеток)"
          % (NAME, len(now), sum(now.values()), len(base)))
    if shrunk or gone:
        print("%s: упоминаний стало МЕНЬШЕ — опусти базу: %s"
              % (NAME, ", ".join((shrunk + gone)[:6])))

    if stale_codes:
        print("%s: FAIL — список снятых форм ПРОТУХ: кода нет в компиляторе — %s"
              % (NAME, ", ".join(stale_codes)), file=sys.stderr)
        print("    Форма разснята или диагностика переименована. Проверь и "
              "поправь список в самом страже, а не базу.", file=sys.stderr)
        return 1

    if grown:
        print("%s: В ПУБЛИКУЕМОЙ ДОКЕ ПРИБАВИЛОСЬ СНЯТЫХ ФОРМ:" % NAME,
              file=sys.stderr)
        for k in grown:
            print("    %-46s было %d, стало %d"
                  % (k, base.get(k, 0), now[k]), file=sys.stderr)
        print("", file=sys.stderr)
        print("    Снятую форму компилятор ОТВЕРГАЕТ — читатель, скопировавший",
              file=sys.stderr)
        print("    пример, получит отказ. Если абзац ОБЪЯСНЯЕТ ретракцию, это",
              file=sys.stderr)
        print("    законно: подними базу строкой `файл|форма=N` с причиной.",
              file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

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
