# -*- coding: utf-8 -*-
"""scripts/tools/novac-fuzz-gen.py — генератор мутаций фаззера novac ОДНИМ
процессом вместо двух процессов на каждый файл.

ЗАЧЕМ. Прежняя генерация жила шелл-циклами, и каждая мутация стоила пары
процессов (`head`/`tail`, а у swap — четырёх). Замер 2026-09-16: на шаге 160
корпус даёт ~4570 файлов, то есть около девяти тысяч запусков процессов ДО
первого вызова Карины; на Windows (Git Bash) запуск процесса — миллисекунды, и
страж уходил за пятнадцать минут локально и съедал 37 из 45 минут job'а на CI
(реестр 221.1 №1138). Проба: те же 187 файлов одного вида одним процессом —
0.25 с.

ПОЧЕМУ ЭТО НЕ «УМЕНЬШЕНИЕ ПРОВЕРКИ». Виды мутаций, их смещения и имена файлов
повторены ОДИН В ОДИН: покрытие то же до файла. Дешевеет ФОРМА, а не предмет —
ровно тот приём, который гейт требует искать первым («сначала ищи ФОРМУ, а не
проверку: процессы на учёт, старт интерпретатора на каждого стража»).

ЧЕГО НЕ ДЕЛАЕТ: не судит. Он только пишет случаи; вердикт выносит
`novac-fuzz-mutations.sh`, прогоняя по ним Карину пачками.

usage: python novac-fuzz-gen.py <корпус> <каталог-случаев> <шаг> [зерно] [файл-списка]
печатает: число созданных файлов.

ЗЕРНО И СПИСОК. С двумя последними аргументами генератор дополнительно пишет
СПИСОК случаев, ПЕРЕМЕШАННЫЙ детерминированно по зерну. Зачем: сам набор
детерминирован (два прогона побайтово одинаковы), и при бюджете времени
проверялись бы вечно одни и те же первые случаи. Зерно от хэша коммита даёт и
то и другое: на ОДНОМ коммите порядок воспроизводим (находку можно повторить),
на РАЗНЫХ — слепая зона каждый раз иная, и покрытие копится по пушам.
"""
import io
import os
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")


def main():
    if len(sys.argv) < 4:
        print("usage: novac-fuzz-gen.py <corpus> <cases-dir> <step>",
              file=sys.stderr)
        return 2
    corpus, cases, step = sys.argv[1], sys.argv[2], int(sys.argv[3])

    srcs = []
    for root, _dirs, files in os.walk(corpus):
        for f in files:
            if f.endswith(".nv"):
                srcs.append(os.path.join(root, f))
    srcs.sort()
    if not srcs:
        print("novac-fuzz-gen: v %s net ni odnogo .nv" % corpus, file=sys.stderr)
        return 2

    os.makedirs(cases, exist_ok=True)
    n = 0

    def put(name, blob):
        nonlocal n
        with io.open(os.path.join(cases, name), "wb") as fh:
            fh.write(blob)
        n += 1

    for srcn, src in enumerate(srcs, 1):
        data = io.open(src, "rb").read()
        size = len(data)
        if size <= 2:
            continue
        base = "%s-%d" % (os.path.basename(src)[:-3], srcn)

        # (a) обрыв: файл кончается там, где его не ждали
        off = 1
        while off < size:
            put("%s-trunc-%d.nv" % (base, off), data[:off])
            off += step

        # (b) порча одного байта управляющим \001
        off = 0
        while off < size:
            put("%s-corrupt-%d.nv" % (base, off),
                data[:off] + b"\001" + data[off + 1:])
            off += step

        # (c) дыра: текст верен с обеих сторон — так выглядит недопечатанная правка
        off = 1
        while off + step < size:
            put("%s-hole-%d.nv" % (base, off), data[:off] + data[off + step:])
            off += step

        # (d) перестановка двух соседних байт: длина и алфавит те же, удивляется ПАРСЕР
        off = 1
        while off + 1 < size:
            put("%s-swap-%d.nv" % (base, off),
                data[:off - 1] + data[off:off + 1] + data[off - 1:off]
                + data[off + 1:])
            off += step

        # (e) лишняя закрывающая скобка: пути восстановления — самый молодой код
        for cl in (b")", b"}", b"]"):
            tag = "%02x" % cl[0]
            off = 1
            while off < size:
                put("%s-closer%s-%d.nv" % (base, tag, off),
                    data[:off] + cl + data[off:])
                off += step * 3

        put("%s-doubled.nv" % base, data + data)
        half = size // 2
        put("%s-halves.nv" % base, data[:half] + data[:half])

    if len(sys.argv) >= 6:
        seed, listpath = sys.argv[4], sys.argv[5]
        names = sorted(os.listdir(cases))
        # Перемешивание ДЕТЕРМИНИРОВАННОЕ по зерну: тот же коммит — тот же
        # порядок, и находка повторяется. `random` с явным seed, а не
        # системное время: время сделало бы прогон невоспроизводимым.
        import random
        random.Random(seed).shuffle(names)
        with io.open(listpath, "w", encoding="utf-8", newline="\n") as fh:
            for x in names:
                fh.write(x + "\n")
    print(n)
    return 0


if __name__ == "__main__":
    sys.exit(main())
