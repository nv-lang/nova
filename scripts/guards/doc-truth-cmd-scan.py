# -*- coding: utf-8 -*-
"""Ядро check-doc-truth (ОСЬ 2): тело цикла по кандидатам `nova <sub> ...`.

ЗАЧЕМ ПЕРЕПИСАНО (план 275-Ф.1, гейт-стоимость). Профилировщик показал: обход
дерева стоит 0.37с, а тело цикла по 234 кандидатам — сотни запусков `sed`,
`printf|grep`, `grep -qxF` (по 5-10 форков на кандидата) — это ~30-40с из
общего времени стража. Кандидаты и их порядок ПРЕЖНИЕ (даёт их всё та же
find+awk-связка в check-doc-truth.sh — не тронута, чтобы не рисковать
порядком обхода файловой системы); в питон вынесено ровно тело цикла: разбор
строки, классификация SKIP, вызов `nova <sub> --help` (кэш на диске тот же,
что был у bash-версии — ключ `$key.help`/`$key.exit`/`$key.allflags`/
`$key.valflags`/$key.required`, тот же CACHE_DIR) и сверка флагов/позиционника.

ПОЧЕМУ РАЗБОР СТРОКИ ВЫГЛЯДИТ СТРАННО (file/lineno/line через split(':',1)
дважды). Это НЕ упрощение, а точное повторение bash-семантики оригинала
(`file="${cand%%:*}"`, `rest="${cand#*:}"`, `lineno="${rest%%:*}"`,
`line="${rest#*:}"`), которая на абсолютных путях Windows (двоеточие сразу
после буквы диска, `X:/...`) даёт file="X" (одна буква), lineno=остаток пути
до следующего двоеточия, line="123:nova ...". Это не ловилось раньше, потому
что все дальнейшие
сообщения СНОВА склеивают `f"{file}:{lineno}: {line}"` — и склейка
восстанавливает исходный вид байт-в-байт. Задание — не изменить ни одного
вердикта, поэтому квирк воспроизведён дословно, а не «исправлен».

ВЫВОД. Идущие мимо SKIP-строки — сразу в stderr (тот же live-порядок, что
раньше выдавал bash при разборе кандидатов). Итог — в stdout, парсится
обёрткой: `skipped_commands=N`, `unrunnable_commands=M`, затем по одной
строке `BADCMD:<file>:<lineno>: <проблема> -- <line>` на каждую находку (уже
готовый текст для встраивания под индентацию — оболочка не трогает
экранирование, как раньше через `printf '%b'`).

Реестр 221.1 №455 (сам страж), план docs/plans/275-gate-cost.md (эта правка).
"""
import io
import os
import re
import subprocess
import sys

PLACEHOLDER_RE = re.compile(u"<[^>]+>|path/to/")
SHELL_CONSTRUCT_RE = re.compile(r"[|;]|>|\\$")
TRAILING_COMMENT_RE = re.compile(r"[ \t]+#.*$")
VALUE_FLAG_RE = re.compile(r"^ {2,}(--[a-zA-Z][a-zA-Z0-9-]*) <[A-Z_]+>")
BOOL_FLAG_RE = re.compile(r"^ {2,}(--[a-zA-Z][a-zA-Z0-9-]*)$")
ARGS_POSITIONAL_RE = re.compile(r"^ *<[A-Za-z_]+>")
ARGS_REQUIRED_RE = re.compile(r"required|at least one", re.IGNORECASE)


def cache_key(sub):
    return re.sub(r"[^a-zA-Z0-9]", "_", sub)


def help_and_exit(bin_path, cache_dir, sub):
    """Читает/строит дисковый кэш `<key>.help`/`<key>.exit` — тот же формат,
    что писала bash-версия (`help_for`/`exit_for`), чтобы кэш переживал смену
    реализации и оставался общим между прогонами (ключ — mtime бинаря,
    вычисляется обёрткой)."""
    key = cache_key(sub)
    hf = os.path.join(cache_dir, key + ".help")
    xf = os.path.join(cache_dir, key + ".exit")
    if not os.path.isfile(hf):
        try:
            proc = subprocess.run([bin_path, sub, "--help"],
                                   stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT)
            out, rc = proc.stdout, proc.returncode
        except Exception:
            out, rc = b"", 1
        with open(hf, "wb") as f:
            f.write(out)
        with open(xf, "w") as f:
            f.write(u"%d\n" % rc)
    with io.open(hf, encoding="utf-8", errors="replace") as f:
        help_text = f.read()
    with io.open(xf, encoding="utf-8", errors="replace") as f:
        rc_text = f.read().strip()
    return help_text, rc_text


def flag_map(cache_dir, sub, help_text):
    """Флаг-карта и требуемость позиционника — считаются один раз на
    подкоманду и кэшируются на диске (`.allflags`/`.valflags`/`.required`,
    та же схема имён, что у bash-версии)."""
    key = cache_key(sub)
    af_f = os.path.join(cache_dir, key + ".allflags")
    vf_f = os.path.join(cache_dir, key + ".valflags")
    req_f = os.path.join(cache_dir, key + ".required")
    if not os.path.isfile(af_f):
        lines = help_text.split(u"\n")
        value_flags = sorted(set(
            m.group(1) for m in (VALUE_FLAG_RE.match(l) for l in lines) if m))
        bool_flags = sorted(set(
            m.group(1) for m in (BOOL_FLAG_RE.match(l) for l in lines) if m))
        all_flags = sorted(set(value_flags) | set(bool_flags))

        args_block = []
        in_args = False
        for l in lines:
            if l.startswith(u"Arguments:"):
                in_args = True
                continue
            if l.startswith(u"Options:"):
                in_args = False
                continue
            if in_args:
                args_block.append(l)
        required = 0
        if any(ARGS_POSITIONAL_RE.match(l) for l in args_block):
            required = 1
        if any(ARGS_REQUIRED_RE.search(l) for l in args_block):
            required = 1

        with io.open(vf_f, "w", encoding="utf-8") as f:
            f.write(u"\n".join(value_flags) + (u"\n" if value_flags else u""))
        with io.open(af_f, "w", encoding="utf-8") as f:
            f.write(u"\n".join(all_flags) + (u"\n" if all_flags else u""))
        with io.open(req_f, "w", encoding="utf-8") as f:
            f.write(u"%d\n" % required)

    with io.open(vf_f, encoding="utf-8", errors="replace") as f:
        value_flags = set(x for x in f.read().split(u"\n") if x)
    with io.open(af_f, encoding="utf-8", errors="replace") as f:
        all_flags = set(x for x in f.read().split(u"\n") if x)
    with io.open(req_f, encoding="utf-8", errors="replace") as f:
        required = f.read().strip() == u"1"
    return all_flags, value_flags, required


def main():
    try:
        # newline="\n" — на Windows TextIOWrapper иначе переводит '\n' в
        # '\r\n' при записи, и вывод байт-в-байт расходится с bash-версией
        # (та печатает чистый LF).
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace", newline="\n")
        sys.stderr.reconfigure(encoding="utf-8", errors="backslashreplace", newline="\n")
    except Exception:
        pass

    if len(sys.argv) < 3:
        sys.stderr.write("doc-truth-cmd-scan: usage: <bin> <cache_dir>\n")
        return 1
    bin_path, cache_dir = sys.argv[1], sys.argv[2]

    skipped_commands = 0
    unrunnable_commands = 0
    bad_commands = []
    exit_cache = {}
    flagmap_cache = {}

    for raw in io.open(sys.stdin.fileno(), encoding="utf-8", errors="replace"):
        cand = raw.rstrip(u"\n")
        if not cand:
            continue
        # Разбор file/lineno/line — намеренно дословная копия bash-квирка,
        # см. докстринг модуля.
        file_part, sep, rest = cand.partition(u":")
        if not sep:
            continue
        lineno, sep2, line = rest.partition(u":")
        file = file_part

        stripped = TRAILING_COMMENT_RE.sub(u"", line)

        if PLACEHOLDER_RE.search(stripped):
            skipped_commands += 1
            sys.stderr.write(u"SKIP(placeholder) %s:%s: %s\n" % (file, lineno, line))
            continue
        if SHELL_CONSTRUCT_RE.search(stripped):
            skipped_commands += 1
            sys.stderr.write(u"SKIP(shell-construct) %s:%s: %s\n" % (file, lineno, line))
            continue

        tok = stripped.split()
        sub = tok[1] if len(tok) > 1 else u""
        if not sub:
            skipped_commands += 1
            sys.stderr.write(u"SKIP(no-subcommand) %s:%s: %s\n" % (file, lineno, line))
            continue

        if sub not in exit_cache:
            help_text, rc_text = help_and_exit(bin_path, cache_dir, sub)
            exit_cache[sub] = (help_text, rc_text)
        help_text, rc_text = exit_cache[sub]

        if rc_text != u"0":
            unrunnable_commands += 1
            bad_commands.append(
                u"%s:%s: unknown-subcommand '%s' -- %s" % (file, lineno, sub, line))
            continue

        if sub not in flagmap_cache:
            flagmap_cache[sub] = flag_map(cache_dir, sub, help_text)
        all_flags, value_flags, required = flagmap_cache[sub]

        problem = u""
        has_positional = False
        skip_next = False
        for t in tok[2:]:
            if skip_next:
                skip_next = False
                continue
            if t.startswith(u"--"):
                fname = t.split(u"=", 1)[0]
                if fname not in all_flags:
                    problem += u"unknown-flag(%s) " % fname
                elif fname in value_flags and u"=" not in t:
                    skip_next = True
            else:
                has_positional = True

        if required and not has_positional:
            problem += u"missing-required-positional "

        if problem:
            unrunnable_commands += 1
            bad_commands.append(u"%s:%s: %s-- %s" % (file, lineno, problem, line))

    w = sys.stdout.write
    w(u"skipped_commands=%d\n" % skipped_commands)
    w(u"unrunnable_commands=%d\n" % unrunnable_commands)
    for bc in bad_commands:
        w(u"BADCMD:%s\n" % bc)
    return 0


if __name__ == "__main__":
    sys.exit(main())
