# -*- coding: utf-8 -*-
"""scripts/guards/check-diag-paths.py — путь, названный ТЕКСТОМ компилятора,
обязан существовать в дереве.

Адрес: реестр 221.1 №1109 — «компилятор ссылает пользователя на файлы, которых
нет: из девяти путей в текстах диагностик мертвы четыре». Родня по корню, а не
по симптому: №1000 (страж правдивости доки держал рукописную копию списка) —
всякое утверждение о дереве, живущее ОТДЕЛЬНО от дерева, тухнет молча.

ЗАЧЕМ. Диагностика — единственное место, где язык объясняет свой канон тому, кто
на него наткнулся. Условие правила проверяется юнит-тестом, а ССЫЛКА внутри его
же сообщения — ничем: файл переезжает, и совет начинает учить, что канона нет.
Найдено 2026-09-15 интегратором: искал эталон, названный сообщением
`W_FFI_BARE_HANDLE`, и не нашёл его.

ПОЧЕМУ ЧИНИТЬ ЧЕТЫРЕ СТРОКИ БЫЛО БЫ ПОЧИНКОЙ НОСИТЕЛЯ. Замена мёртвых путей
живыми закрывает сегодняшний замер и не трогает класс: следующий переезд файла
убьёт следующий путь так же молча. Предмет стража — не пути, а ОТСУТСТВИЕ
СВЕРКИ.

ЧТО СУДИТСЯ: строковые литералы в `compiler-codegen/src/**/*.rs` и
`nova-cli/src/**/*.rs`. Комментарии (`//`, `/* */`) исключены — их пользователю
не показывают.

ПОЧЕМУ РАЗБОР, А НЕ РЕГУЛЯРКА ПО СТРОКАМ. Регулярка, судящая исходник построчно,
СЛЕПА к литералу, склеенному продолжением строки (`\\` в конце строки: Rust
убирает перевод строки и отступ следующей). Именно так спрятался пятый носитель —
`W_FFI_BARE_HANDLE` в lints.rs, — и первый замер его не увидел: «9» было нижней
границей, а не переписью. Поэтому здесь литералы СОБИРАЮТСЯ так же, как их
собирает компилятор, и только потом в них ищутся пути.

КОРНИ ПУТЕЙ БЕРУТСЯ ИЗ ДЕРЕВА, НЕ ИЗ СПИСКА В КОДЕ. Список верхних каталогов,
записанный здесь руками, был бы второй копией факта о дереве — то есть ровно тем
дефектом, который страж и ловит (№1109 родня №1000). Корень добавили — страж
видит его сам.

ПУТЬ-СООБЩЕНИЕ ПРОТИВ ПУТИ-ЗНАЧЕНИЯ. Судится не всякий литерал с путём, а тот,
где путь делит литерал С ПРОЗОЙ (в литерале есть пробел вне самого пути). Это и
есть граница класса: утверждение «файл существует» делает только ТЕКСТ, который
читает человек. Литерал, целиком равный пути, — значение, а не утверждение, и
существовать не обязан. Замер 2026-09-15 показал, почему различие не косметика:
из 30 путей 13 оказались путями-значениями — в `test_runner.rs` путь подаётся
ВХОДОМ классификатору внутри `#[test]` (функция судит его текстом, файла там
никогда и не было), в `main.rs` это путь, КУДА файл записывается. Страж без
этого различия покраснел бы на законном коде и был бы отключён в первую неделю.

ЧЕГО НЕ СУДИТ (сознательные слепые зоны, названные здесь, а не молчаливые):
  * путь-значение (литерал без прозы) — см. выше; СЧИТАЕТСЯ отдельно и
    печатается числом, чтобы зона не росла незаметно;
  * путь с подстановкой (`{}`, `{name}`) — он собирается во время работы, и
    существование его файла статически не проверить. Такие СЧИТАЮТСЯ отдельно и
    печатаются числом, чтобы слепая зона не росла незаметно;
  * пути в комментариях Rust — пользователь их не видит;
  * смысл сообщения: ведёт ли ссылка к УМЕСТНОМУ файлу, страж не знает, только
    существует ли он.

ХРАПОВИК. База scripts/guards/diag-paths.baseline, ровно две строки `dead=N` и
`judged=N`. `dead` только вниз. `judged` — размер мишени: если путей под судом
стало вдвое меньше базы, это отказ, а не успех. Иначе «мёртвых стало меньше»
означало бы «сканер ослеп», и зелёный вердикт был бы ложью (№911).

Самотест: scripts/guards/selftest/test-check-diag-paths.sh (семь случаев).

$1 — корень репозитория.
"""
import io
import os
import re
import sys
import time

# python на Windows пишет CRLF там, где shell писал LF, и вывод расходится с
# остальным гейтом молча (требование check-guard-honesty.py).
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-diag-paths"
BASE_REL = "scripts/guards/diag-paths.baseline"
SCAN_DIRS = ("compiler-codegen/src", "nova-cli/src")

RE_DEAD = re.compile(r"^dead=(\d+)\s*$", re.M)
RE_JUDGED = re.compile(r"^judged=(\d+)\s*$", re.M)

# Сканируется ПОЗИЦИЕЙ (`.match(text, i)`), а не срезом `text[i:]`: срез копирует
# остаток файла на КАЖДОМ символе, и разбор types/mod.rs уходит в квадрат —
# первый прогон не уложился в две минуты.
RE_RAW_OPEN = re.compile(r'(?:b?r)(#*)"')
RE_CHAR_LIT = re.compile(r"'(?:\\.|[^\\'])'")


def strip_literals(text):
    """Собрать строковые литералы Rust так, как их видит компилятор.

    Возвращает список (line_no, content). Комментарии и char-литералы
    пропускаются. Продолжение строки (`\\` перед переводом строки) склеивается
    с отбрасыванием отступа — иначе путь, разрезанный переносом, не найдётся.
    """
    out = []
    i = 0
    n = len(text)
    line = 1
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        # комментарии
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            while i < n and text[i] != "\n":
                i += 1
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            i += 2
            while i < n and depth:
                if text[i] == "\n":
                    line += 1
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                    continue
                if text.startswith("*/", i):
                    depth -= 1
                    i += 2
                    continue
                i += 1
            continue
        # raw-литерал: r"..." / r#"..."# / br#"..."#
        m = RE_RAW_OPEN.match(text, i)
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = m.group(1)
            start_line = line
            i = m.end()
            close = '"' + hashes
            j = text.find(close, i)
            if j < 0:
                break
            body = text[i:j]
            line += body.count("\n")
            out.append((start_line, body))
            i = j + len(close)
            continue
        # обычный литерал "..."
        if c == '"':
            start_line = line
            i += 1
            buf = []
            while i < n:
                ch = text[i]
                if ch == "\\":
                    if i + 1 < n and text[i + 1] == "\n":
                        # продолжение строки: перевод и отступ съедаются
                        line += 1
                        i += 2
                        while i < n and text[i] in " \t":
                            i += 1
                        continue
                    # прочий escape: сам символ для поиска путей не важен
                    buf.append(text[i:i + 2])
                    i += 2
                    continue
                if ch == '"':
                    i += 1
                    break
                if ch == "\n":
                    line += 1
                buf.append(ch)
                i += 1
            out.append((start_line, "".join(buf)))
            continue
        # char-литерал vs время жизни ('a): разбираем только настоящий char
        if c == "'":
            m = RE_CHAR_LIT.match(text, i)
            if m:
                i = m.end()
                continue
            i += 1
            continue
        i += 1
    return out


def tree_roots(root):
    """Верхние каталоги дерева — мишень для поиска путей, взятая ИЗ дерева."""
    roots = []
    for e in sorted(os.listdir(root)):
        if e.startswith(".") or e == "target":
            continue
        if os.path.isdir(os.path.join(root, e)):
            roots.append(e)
    return roots


def path_re(roots):
    alt = "|".join(re.escape(r) for r in roots)
    return re.compile(
        r"(?<![A-Za-z0-9_./-])(?:%s)(?:/[A-Za-z0-9_.+-]+)+\.[A-Za-z0-9]{1,6}"
        r"(?![A-Za-z0-9_/-])" % alt
    )


def rs_files(root):
    for rel in SCAN_DIRS:
        base = os.path.join(root, rel)
        if not os.path.isdir(base):
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d != "target"]
            for fn in sorted(filenames):
                if fn.endswith(".rs"):
                    yield os.path.join(dirpath, fn)


def fail(msg):
    sys.stderr.write("%s: FAIL - %s\n" % (NAME, msg))
    return 1


def main():
    t0 = time.time()
    argv = sys.argv
    if len(argv) < 2:
        return fail("нужен корень репозитория первым аргументом")
    root = os.path.abspath(argv[1])
    if not os.path.isdir(root):
        return fail("нет каталога %s" % root)

    roots = tree_roots(root)
    if not roots:
        return fail("в корне %s нет ни одного каталога - мишень потеряна, "
                    "искать пути не в чем" % root)
    rx = path_re(roots)

    found = {}      # путь -> (файл, строка)
    dynamic = 0
    as_value = 0
    for fp in rs_files(root):
        try:
            text = io.open(fp, encoding="utf-8", errors="replace").read()
        except IOError:
            continue
        for line_no, body in strip_literals(text):
            for m in rx.finditer(body):
                p = m.group(0)
                # путь с подстановкой не проверить статически: либо она внутри
                # самого пути, либо путь приклеен к закрывшейся подстановке
                # слева (`{root}/std/foo.nv`).
                if "{" in p or body[:m.start()].endswith("}"):
                    dynamic += 1
                    continue
                # путь-значение: литерал не несёт прозы вокруг пути, значит
                # ничего человеку не утверждает
                if " " not in (body[:m.start()] + body[m.end():]):
                    as_value += 1
                    continue
                if p not in found:
                    found[p] = (os.path.relpath(fp, root).replace("\\", "/"),
                                line_no)

    if not found:
        return fail("в текстах компилятора не найдено НИ ОДНОГО пути - мишень "
                    "потеряна (сканер литералов сломан либо каталоги %s пусты). "
                    "Зелёный ноль здесь был бы ложью" % ", ".join(SCAN_DIRS))

    dead = []
    for p in sorted(found):
        if not os.path.exists(os.path.join(root, p)):
            dead.append((p, found[p][0], found[p][1]))

    base_file = os.path.join(root, BASE_REL)
    try:
        base_t = io.open(base_file, encoding="utf-8", errors="replace").read()
    except IOError:
        return fail("нет базы %s (ключи dead=N и judged=N) - храповик судить нечем"
                    % BASE_REL)
    m_d = RE_DEAD.search(base_t)
    m_j = RE_JUDGED.search(base_t)
    if not m_d or not m_j:
        return fail("в базе %s нет строки dead=N и/или judged=N - храповик "
                    "судить нечем" % BASE_REL)
    base_dead = int(m_d.group(1))
    base_judged = int(m_j.group(1))

    n = len(found)
    if n * 2 < base_judged:
        return fail("под судом %d путей, а база помнит %d - мишень потеряна "
                    "больше чем наполовину. «Мёртвых стало меньше» здесь "
                    "значило бы «сканер ослеп», и зелёный вердикт был бы "
                    "ложью (№911)" % (n, base_judged))

    if len(dead) > base_dead:
        sys.stderr.write(
            "%s: FAIL - текст компилятора ссылает на несуществующие файлы: %d, "
            "база %d (реестр 221.1 №1109). Путь в диагностике - утверждение о "
            "дереве, и оно обязано быть истинным:\n" % (NAME, len(dead), base_dead))
        for p, f, ln in dead[:20]:
            sys.stderr.write("    %s  <- %s:%d\n" % (p, f, ln))
        if len(dead) > 20:
            sys.stderr.write("    ... и ещё %d\n" % (len(dead) - 20))
        sys.stderr.write(
            "  Чинить НЕ удалением ссылки: совет без образца хуже совета с "
            "переехавшим образцом. Найди живой файл и назови его.\n")
        return 1

    tail = ""
    if len(dead) < base_dead:
        tail = " - база устарела, опусти dead до %d тем же коммитом" % len(dead)
    sys.stdout.write(
        "%s ok: путей в текстах %d, мёртвых %d (база %d)%s; пропущено: "
        "путей-значений %d, с подстановкой %d; корней дерева %d; за %.1fс\n"
        % (NAME, n, len(dead), base_dead, tail, as_value, dynamic, len(roots),
           time.time() - t0))
    return 0


if __name__ == "__main__":
    sys.exit(main())
