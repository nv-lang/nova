<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# 1590 — текст от ОС на Windows приходил в cp1251

Пакет `hcap`: скопируй каталог, убери суффиксы `.txt`, затем из каталога

    nova build u1_args_env.nv -o u1.exe && python u1run.py u1.exe
    nova build u3_os_points.nv -o u3.exe && python u3run.py u3.exe

Запускать из python: оболочка Windows портит не-ASCII в командной строке сама.

Замер 2026-10-02 (оракул main + ветка `p1590-windows-utf8-os-text`, рантайм из дерева):

| точка | до (os_env.h с main) | после |
|---|---|---|
| argv (u1) | `c5e2e3` | `d095d0b2d0b3` |
| env get (u1) | `c5e2e3` | `d095d0b2d0b3` |
| env set→get (u3) | верно (круг в одной кодировке) | верно |
| vars() (u3) | `c5e2e3` | `d095d0b2d0b3` |
| cwd из кириллического каталога (u3) | `efbfbd…` (U+FFFD) | UTF-8 пути |
| temp_dir / home_dir (u3) | `efbfbd…` | UTF-8 пути |
| hostname (u3, `COMPUTERNAME=ХОСТ`) | `d5ced1d2` (cp1251) | `d0a5d09ed0a1d0a2` |

Имена файлов (std.fs) идут через libuv и были верны и до (u2 пробы nova-63).
