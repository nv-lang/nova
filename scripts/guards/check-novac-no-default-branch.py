# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-default-branch.py — на закрытом множестве ветки
«всё остальное» нет (конвенция П21; требование владельца 2026-08-16).

КЛАСС. Живой случай в `emit_type_decls`: `if kind == TkRecord { … } else {
emit_sum_decl(…) }`. `else` здесь значит «всё, что не запись, — сумма»: появится
четвёртый вид типа — он молча уедет в эмиссию суммы. Это класс №652 («тихий
дефолт»), только на диспетчере, и он не даст ни ошибки, ни диагностики.

ПРАВИЛА (перенесены из shell-редакции слово в слово, 2026-08-19):

  1. Арм `_ =>` в match по ЗАКРЫТОМУ множеству законен ТОЛЬКО объяснённым
     (уточнение владельца 2026-08-18: «`_` должно быть объяснено»). Законных
     форм три: `_ => None` — частичность, объявленная типом; отказ
     (`ice`/`@refuse`); и `_` с КОММЕНТАРИЕМ хвостом той же строки или строкой
     выше, не короче ПЯТИ слов. Множество ОТКРЫТО, если сосед-арм разбирает
     литерал (символьный, строковый, числовой) — тогда `_` обязателен.

  1а. Пустое значение ХВОСТОМ функции (`""` сразу после закрывающей скобки) —
      тот же тихий дефолт; законно только с маркером [LEGACY-#NNN].

  2-3. `else`, закрывающий разбор ПО ВИДУ (`... kind ... == Тип.Вариант`),
       обязан быть честным отказом: `ice(`, `@refuse(`, `report`, `NodeKind.Err`
       в ближайших 12 значащих строках. Заглядывание `peek()` — вопрос да/нет и
       не судится. Условие запоминается по ОТСТУПУ: `else` принадлежит цепочке
       своего отступа (иначе он приписывался далёкому `if`).

ИЗВЕСТНАЯ ДЫРА (2026-08-16, честно): диспетчер по ID типа (`at == prims.str_id`)
не судится — попытка расширить не доказала ловлю на подложке, а страж без
доказательства запрещён (П16). Записано в 274.3.

ПОЧЕМУ PYTHON: shell-редакция поднимала `tr` и `awk` на КАЖДЫЙ файл — 2.3с там,
где работы на доли секунды (П14).

$1 — корень репозитория; $2 — override сканируемой директории (самотест).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-default-branch"


def wordcount(s):
    s = s.strip()
    return len(s.split()) if s else 0


def scan(rel, text, bad):
    lit_seen = False
    prev_comment_words = 0
    prev_bare_close = False
    marked = False
    cond_at, line_at, isvar_at = {}, {}, {}
    pending, look, cond = 0, 0, ""
    else_ind = -1

    for i, raw in enumerate(text.split("\n"), 1):
        line = re.sub(r"//.*$", "", raw)

        # открытость множества: сосед-арм разбирает литерал
        if "=>" in line and re.match(r"^[ \t]*['\"0-9-]", line):
            lit_seen = True
        if re.match(r"^[ \t]*match ", line):
            lit_seen = False

        # объяснение: хвостовой комментарий или комментарий строкой выше
        expl = False
        m = re.search(r"//[^/]", raw)
        if m:
            tail = raw[raw.index("//") + 2:]
            if wordcount(tail) >= 5:
                expl = True
        if prev_comment_words >= 5:
            expl = True

        if re.match(r"^[ \t]*_[ \t]*=>[ \t]*None[ \t]*$", line):
            ok_default = True
        elif re.match(r"^[ \t]*_[ \t]*=>", line) and expl:
            ok_default = True
        elif re.match(r"^[ \t]*_[ \t]*=>", line) and ("ice(" in line or "@refuse(" in line):
            ok_default = True
        else:
            ok_default = False

        if not ok_default and re.match(r"^[ \t]*_[ \t]*=>", line) and not lit_seen:
            bad.append(f"  {rel}:{i}: арм `_ =>` без объяснения проглотит будущий вариант")

        # пустое значение хвостом
        if re.match(r"^[ \t]*(return[ \t]+)?\"\"[ \t]*$", line) and prev_bare_close:
            if not marked:
                bad.append(f"  {rel}:{i}: пустое значение хвостом без маркера [LEGACY-#NNN]")

        # память о комментарии строкой выше
        if re.match(r"^[ \t]*//", raw):
            pc = re.sub(r"^[ \t]*//+[ \t]*", "", raw)
            prev_comment_words = wordcount(pc)
        elif raw.strip():
            prev_comment_words = 0

        if "[LEGACY-#" in raw:
            marked = True
        elif raw.strip() and not re.match(r"^[ \t]*//", raw) and not re.match(r"^[ \t]*(return[ \t]+)?\"\"[ \t]*$", line):
            marked = False

        if raw.strip() and not re.match(r"^[ \t]*//", raw):
            prev_bare_close = bool(re.match(r"^[ \t]*\}[ \t]*$", line))

        # условие цепочки по отступу
        if re.match(r"^[ \t]*(\} else )?if[ \t]", line):
            ind = re.match(r"^[ \t]*", line).group(0)
            L = len(ind)
            c = line.lstrip()
            cond_at[L] = c
            line_at[L] = i
            isvar_at[L] = bool(re.search(r"==[ \t]*[A-Z][A-Za-z0-9_]*\.[A-Z]", line)
                               and "kind" in line and "peek" not in line)

        # else, закрывающий цепочку своего отступа
        if re.search(r"\}[ \t]*else[ \t]*\{", line):
            eind = re.match(r"^[ \t]*", line).group(0)
            L = len(eind)
            # ОДНОСТРОЧНАЯ ФОРМА `x = if C { a } else { b }`: условие стоит на
            # ЭТОЙ строке, и судить надо его, а не последний `if` того же отступа
            # выше (type_of.nv:457 был спарен с проверкой варианта за 20 строк,
            # 2026-09-05). Условие на строке -- вариант ли оно, решается тут же.
            # A same-line `x = if C { a } else { b }` is a PREDICATE choosing a
            # value (tree.nv:353 `if t.kind == TokenKind.Ident { Some(t.text) }
            # else { None }`), not a dispatcher over a closed set: its else IS
            # the honest "no". P21 aims at the multi-line chain that hides the
            # next variant; the inline form is left alone, and -- the actual
            # fix -- no longer paired with an unrelated `if` above it.
            if re.search(r"\bif[ \t]+.+?\{.*\}[ \t]*else[ \t]*\{", line):
                continue
            if isvar_at.get(L) and i - line_at.get(L, 0) <= 60:
                pending, look, cond, else_ind = i, 12, cond_at.get(L, ""), L
            continue

        if pending > 0 and look > 0:
            # ОКНО КОНЧАЕТСЯ ВМЕСТЕ С БЛОКОМ else: строка `}` с отступом самого
            # `} else {` закрывает его, и что стоит дальше -- уже следующий арм.
            # Без этого пустой else с одним комментарием (законная форма по П31)
            # читался как «делает работу» строками соседнего арма -- ложняк,
            # пойманный 2026-09-05 на exprs.nv (П21 велел убрать else, П31 --
            # вернуть; спорили не правила, а окно этого стража).
            if re.match(r"^[ \t]*\}[ \t]*$", line) and len(re.match(r"^[ \t]*", line).group(0)) == else_ind:
                # Блок else кончился. Ни одной строки работы внутри (look не
                # тронут) -- пустой else с комментарием, законен по П31. Были --
                # вердикт ЗДЕСЬ: работа без отказа в ветке «всё остальное».
                if look < 12:
                    bad.append(f"  {rel}:{pending}: `else` за проверкой варианта (`{cond}`) делает работу, а не отказ")
                pending = 0
                continue
            if re.search(r"ice\(|@refuse\(|report|NodeKind\.Err", line):
                pending = 0
            if not re.match(r"^[ \t]*//", raw):
                look -= 1
            if look == 0 and pending > 0:
                bad.append(f"  {rel}:{pending}: `else` за проверкой варианта (`{cond}`) делает работу, а не отказ")
                pending = 0

    if pending > 0:
        bad.append(f"  {rel}:{pending}: `else` за разбором по виду (`{cond}`) делает работу, а не отказ")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    judged = [p for p in sorted(src.rglob("*.nv")) if not p.name.endswith("_test.nv")]
    if not judged:
        print(f"{NAME} ok: судить нечего (в {src} файлов .nv: 0)")
        return 0

    bad = []
    for p in judged:
        scan(p.relative_to(src).as_posix(),
             p.read_text(encoding="utf-8", errors="replace").replace("\r", ""), bad)

    if bad:
        print(f"{NAME}: FAIL — ветка «всё остальное» на закрытом множестве (конвенция П21):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Диспетчер по сумме — исчерпывающий 'match' с именованной веткой на вариант.", file=sys.stderr)
        print("  Тогда новый вариант даёт ошибку компиляции ровно здесь, а не уезжает", file=sys.stderr)
        print("  в чужую ветку. Если ветка «остальное» нужна — она обязана быть отказом:", file=sys.stderr)
        print("  'ice(…)' для сломанного инварианта, '@refuse(…)' для формы вне подмножества.", file=sys.stderr)
        return 1

    matches = 0
    for p in src.rglob("*.nv"):
        text = p.read_text(encoding="utf-8", errors="replace")
        matches += sum(1 for l in text.split("\n") if "match " in l)

    print(f"{NAME} ok: файлов .nv: {len(judged)}, match-выражений: {matches}, веток «всё остальное»: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
