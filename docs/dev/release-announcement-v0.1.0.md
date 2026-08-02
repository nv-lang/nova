<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Announcement draft — Nova v0.1.0 (A-V6; финал при тегах)

## EN (GitHub Release / nv-lang.org)

**Nova 0.1.0 — first public release**

Today I'm releasing Nova, a systems-flavored language I've been building solo:
it compiles to C, tracks effects in function signatures, and enforces resource
ownership at compile time.

What makes it interesting:

- **Effects in types.** A function that touches time, network, or spawns
  concurrency says so in its signature — and `--strict-effects` makes the
  compiler enforce it. No hidden I/O.
- **Ownership without a borrow checker tax.** `consume` parameters, `defer`,
  and automatic `@cleanup` give deterministic resource release (files, sockets,
  locks) at compile time, while a GC handles plain memory. You get use-after-
  close and double-close as compile errors, not runtime surprises.
- **M:N concurrency built in.** Fibers on a work-stealing scheduler:
  `spawn`, `parallel for`, `supervised(deadline:)`, channels — structured
  concurrency as language constructs, not a library bolt-on.
- **One way to format.** String interpolation `"${x}"` is the canonical path
  (there is no string `+`), backed by a single zero-copy formatting engine.
- **Batteries.** Collections, JSON, time/tz, unicode, net; TLS, HTTP and
  compression as versioned packages. Plus `nova` CLI (build/check/test/doc),
  an LSP server, a VSCode extension, and a Docker image.

This is an early release: the language surface is not frozen and APIs will
move. But the compiler is real — the whole stdlib and every example builds
under strict effects, the conformance suite (1000+ fixtures in a single
compilation unit) is green on Windows and Linux, and the flagship demo app
(concurrent aggregator with HTTP/TLS) builds and survives load testing.

Get started: download the Windows build, or build from source on Linux —
the [quickstart](docs/guide/quickstart.md) takes you from install to a running
concurrent program in a few minutes. The [language tour](docs/guide/language-tour.md)
covers the surface in 12 short sections, every example verified.

Feedback, bug reports, and hard questions are welcome — this is day one.

## RU (nv-lang.ru)

**Nova 0.1.0 — первый публичный релиз**

Сегодня я выпускаю Nova — язык, который делаю в одиночку: компилируется через
C, эффекты — часть сигнатур функций, владение ресурсами проверяется на этапе
компиляции.

Что в нём интересного:

- **Эффекты в типах.** Функция, трогающая время, сеть или конкурентность,
  объявляет это в сигнатуре, а `--strict-effects` заставляет компилятор это
  проверять. Скрытого I/O нет.
- **Владение без налога borrow checker'а.** `consume`-параметры, `defer` и
  автоматический `@cleanup` дают детерминированное освобождение ресурсов
  (файлы, сокеты, локи) на этапе компиляции; обычной памятью занимается GC.
  Use-after-close и double-close — ошибки компиляции, а не сюрпризы в проде.
- **Встроенная M:N-конкурентность.** Файберы на work-stealing планировщике:
  `spawn`, `parallel for`, `supervised(deadline:)`, каналы — структурная
  конкурентность как конструкции языка.
- **Один путь форматирования.** Интерполяция `"${x}"` — канон (строкового `+`
  в языке нет), под ней единый zero-copy движок.
- **Батарейки.** Коллекции, JSON, время/зоны, unicode, сеть; TLS, HTTP и
  сжатие — версионируемыми пакетами. Плюс CLI (build/check/test/doc),
  LSP-сервер, расширение VSCode и Docker-образ.

Релиз ранний: поверхность языка не заморожена, API будут меняться. Но
компилятор настоящий: вся стандартная библиотека и все примеры собираются под
строгими эффектами, конформанс-сьют (1000+ фикстур одним компилируемым юнитом)
зелёный на Windows и Linux, флагманское демо (конкурентный агрегатор с
HTTP/TLS) собирается и держит нагрузочный тест.

Начать: скачайте Windows-сборку или соберите из исходников на Linux —
quickstart доводит от установки до работающей конкурентной программы за
несколько минут. Язык-тур покрывает поверхность в 12 коротких секциях, каждый
пример проверен.

Обратная связь, баг-репорты и неудобные вопросы приветствуются — это первый
день.
