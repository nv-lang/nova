<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova Oracle 0.1.0

Release announcement draft (English/Russian; alpha release).

## EN (GitHub Release / nv-lang.org)

**Nova Oracle 0.1.0**

Today I'm releasing Nova, a systems-flavored language I've been building solo:
it compiles to C, tracks effects in function signatures, and enforces resource
ownership at compile time.

What makes it interesting:

- **Effects in types.** A function that touches time, network, or spawns
  concurrency says so in its signature — and `--strict-effects` makes the
  compiler enforce it. No hidden I/O.
- **Ownership without a borrow checker tax.** `consume` parameters and
  `defer` are checked by the compiler; opted-in `@cleanup` resources are
  cleaned on exit when still live, while a GC handles plain memory.
- **M:N concurrency built in.** Fibers on a work-stealing scheduler:
  `spawn`, `parallel for`, `supervised(deadline:)`, channels — structured
  concurrency as language constructs, not a library bolt-on.
- **One way to format.** String interpolation `"${x}"` is the canonical path
  (there is no string `+`), backed by a single zero-copy formatting engine.
- **Batteries.** Collections, JSON, time/tz, Unicode and networking in `std`;
  the examples workspace pins packages named `tls`, `http`, `compress`,
  `socks`, and `polaris`. Their package boundaries and known limits are in
  the release notes. Plus the `nova` CLI (build/check/test/doc), an LSP
  server, editor support, and a Docker build recipe.

This is an early alpha: the language surface and APIs may change, and
compatibility is not guaranteed. See the release notes for the exact scope,
known limitations, and planned assets. A green compiler build or test suite
does not by itself certify a releasable binary or package ecosystem.

Get started: download the Windows archive when it is published, or build from source on Linux —
the [quickstart](docs/guide/quickstart.md) takes you from install to a running
concurrent program in a few minutes. The [language tour](docs/guide/language-tour.md)
covers the surface in 12 short sections, every example verified.

Feedback, bug reports, and hard questions are welcome — this is day one.

## RU (nv-lang.ru)

**Nova Oracle 0.1.0**

Сегодня я выпускаю Nova — язык, который делаю в одиночку: компилируется через
C, эффекты — часть сигнатур функций, владение ресурсами проверяется на этапе
компиляции.

Что в нём интересного:

- **Эффекты в типах.** Функция, трогающая время, сеть или конкурентность,
  объявляет это в сигнатуре, а `--strict-effects` заставляет компилятор это
  проверять. Скрытого I/O нет.
- **Владение без налога borrow checker'а.** `consume`-параметры и `defer`
  проверяются компилятором; для ресурсов с `@cleanup` очистка запускается
  при выходе, если значение ещё живо, а обычной памятью занимается GC.
- **Встроенная M:N-конкурентность.** Файберы на work-stealing планировщике:
  `spawn`, `parallel for`, `supervised(deadline:)`, каналы — структурная
  конкурентность как конструкции языка.
- **Один путь форматирования.** Интерполяция `"${x}"` — канон (строкового `+`
  в языке нет), под ней единый zero-copy движок.
- **Батарейки.** Коллекции, JSON, время/зоны, Unicode и сеть — в `std`;
  examples workspace закрепляет пакеты `tls`, `http`, `compress`, `socks`
  и `polaris`. Их границы и ограничения — в release notes. Плюс CLI
  (build/check/test/doc), LSP-сервер, поддержка редакторов и рецепт сборки
  Docker-образа.

Это ранняя альфа: поверхность языка и API могут меняться, совместимость не
гарантируется. Точный scope, известные ограничения и планируемые артефакты — в
release notes. Зелёная сборка компилятора или тестовый набор сами по себе не
подтверждают готовность бинарного артефакта или экосистемы пакетов.

Начать: скачайте Windows-архив после его публикации или соберите из исходников на Linux —
quickstart доводит от установки до работающей конкурентной программы за
несколько минут. Язык-тур покрывает поверхность в 12 коротких секциях, каждый
пример проверен.

Обратная связь, баг-репорты и неудобные вопросы приветствуются — это первый
день.
