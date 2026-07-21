<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 221 — Release v0.1 (первая прод-версия Nova)

**Статус:** 🔨 В РАБОТЕ (утверждён 2026-07-21; атомарная декомпозиция 2026-07-21 по запросу
владельца — параллельные атомы запущены, см. маркировку). **Приоритет:** P0.
**Цель:** первый публичный релиз. **Скоуп (решения владельца, все приняты):** компилятор + std +
доки + **LSP в комплекте** + **docker** + **GitHub Releases** + **страница на nv-lang.org/.ru** +
версия **0.1.0** (теги согласованно: nova / nova-tls / nova-http / nova-compress / www).

Легенда атомов: `[зона] (модель)` · **▶СЕЙЧАС** = не зависит от Ф.0, параллелится немедленно ·
**⛓X** = зависит от атома/фазы X.

## Ф.0 — Ноль багов очереди (гейт входа в релизную фазу)

- [ ] **A-B1** Текущие волны влиты: 196-closeout · 217 · Linux-гонка (known-red) ·
      [M-freefn-named-default-arg-shift] (Д3-гейт). `[types/emit_c/callnorm/WSL]` — В ПОЛЁТЕ.
- [x] **A-B2 ✅ 2026-07-21** Ш4-снос conv.h+kill-switch+D422-амендмент+примеры §4-§6 — влит
      (один путь = `*_display_spec`; остатки честные: pad user-типов + ptr-debug).
- [ ] **A-B3** Ш2: перенос примитив-тел `[std/fmt+prelude]` (sonnet) —
      ⛓[M-fmt-write-protocol-collision-cycle-adjacent] (его фикс = отдельный атом A-B3a `[types]`).
- [ ] **A-B4** box-vtable P2 `[emit_c/vtable]` (sonnet) — ⛓A-B2 (зона emit_c).
- [ ] **A-B5** net-утечка-b free-on-close `[nova_rt/net.c]` (sonnet, mn-conventions §9) — независим.
- [ ] **A-B6** Мелочь P3/P4: d55-const `[emit_c]` · oot-дефисы E_D78 `[resolve]` (В ПОЛЁТЕ) ·
      generic-match-scope-gap `[types]` · latent protocol-box `[emit_c]` — по освобождении зон;
      частично haiku (маркер-закрытия).
- [ ] **A-B7** 216-defer-хвосты в v0.1 (решение владельца 2026-07-21 «конечно хочу»):
      Err-пейлоады + nested/tuple-пейлоады consume-enforce (см. 216-план, хвосты Ф-финала)
      `[types/consume]` (sonnet) — запуск при освобождении types-слота.
- [ ] **A-B8** [M-d216-write-at-return-type-unknown-cc-panic] P1 (находка Ш4, блокирует мега-CU
      гейт) `[emit_c]` (sonnet) — В ПОЛЁТЕ (прямой приказ владельца 2026-07-21).
- [ ] **Критерий Ф.0:** backlog без OPEN P1/P2; CI без known_red вовсе.

## Ф.1 — Стабилизация (частично ▶СЕЙЧАС — прогоны находят баги раньше!)

- [ ] **A-S1 ▶СЕЙЧАС** Полный `nova test` Windows (весь std+примеры+nova_tests-baseline),
      батчами <10мин `[прогоны]` — фоновыми Bash-задачами интегратора; красное → в Ф.0-очередь.
- [ ] **A-S2** Полный `nova test` WSL Linux `[WSL]` — ⛓A-S1-рецепт (те же батчи), после Linux-гонки.
- [ ] **A-S3** loadtest.ps1 полный (10× комбо + concurrency 80) `[флагман]` — ⛓A-B1.
- [ ] **A-S4 ▶СЕЙЧАС** Соседние репы: nova-tls/http/compress suites зелёные `[соседние репы]` (sonnet).
- [ ] **A-S5** Slope-регрессы GC в допуске `[WSL]` — ⛓A-B5.

## Ф.2 — Версия и дистрибуция (почти всё ▶СЕЙЧАС)

- [x] **A-V1 ✅ 2026-07-21** `nova --version`/`-V` = «nova 0.1.0», `nova-lsp --version` =
      «nova-lsp 0.1.0» (версии в Cargo.toml всех трёх крейтов уже были 0.1.0; clap
      `#[command(version)]` — без литералов). Теги v0.1.0 — отдельный A-V6.
- [x] **A-V2 ✅ 2026-07-21** `scripts/package-release.ps1` (`-SkipBuild`/`-SmokeTest`/`-VcpkgBase`).
      Состав `nova-v0.1.0-windows-x64.zip` (12.4MB, 506 файлов): nova.exe + nova-lsp.exe + std/ +
      nova_rt/ (урезанный libuv-подсет) + gc/ (Boehm подсет) + setup-env.ps1 (5 env vars от
      $PSScriptRoot) + README-INSTALL.md + LICENSE*+THIRD_PARTY. std-discovery = env-vars
      (штатная поверхность). **SmokeTest PASSED: hello.exe собран+выполнен из изолированной
      папки вне монорепы** (sha256 b76550ac…065f; 4 реальных бага упаковки найдены и починены —
      wip/221-version-notes.md).
- [ ] **A-V3 ▶СЕЙЧАС** docs/linux-build.md актуализация (gcc15/clang21 — фиксы влиты; nova-lsp
      сборка) `[docs]` (sonnet/haiku).
- [ ] **A-V4 ▶СЕЙЧАС** THIRD_PARTY сверка (libuv/bdwgc/dtoa лицензии) `[docs]` (haiku).
- [ ] **A-D1 ▶СЕЙЧАС** Dockerfile: образ с компилятором, hello-smoke, инструкция `[docker/]`
      (sonnet) — CPU-тяжёлый билд, очередь за слотом.
- [ ] **A-V5** vsix-сборка VSCode-расширения в артефакт `[editors/vscode]` — ⛓A-V2 ✅ (разблокирован).
- [ ] **A-V6** Теги v0.1.0 на 4 репы + артефакты на GitHub Releases — ⛓Ф.0+Ф.1+Ф.3 (финал).

## Ф.3 — Документация внешнего пользователя (всё ▶СЕЙЧАС)

- [x] **A-Q1 ✅ 2026-07-21** `docs/quickstart.md`: установка (Windows zip + setup-env.ps1;
      Linux — ссылка на docs/linux-build.md) → hello world (реально собран+прогнан standalone-
      проектом) → mini_aggregator.nv (эффекты Time + spawn/parallel for/supervised(deadline:),
      реально собран+прогнан) → ссылки (spec/overview.md, flagship/aggregator, spec/decisions/).
      Все команды прогнаны живым nova.exe из main-репы (read-only).
- [x] **A-Q2 ✅ 2026-07-21** README.md — добавлен абзац-суть (compiles-to-C/эффекты-в-типах/
      consume+Boehm GC/M:N fiber-scheduler/батарейки std+net+tls+http+compress) сразу после
      тэглайна; секция Status переписана под v0.1.0 (компилятор+CLI+LSP+VSCode, что готово/что
      на roadmap); новая секция Installation (ссылка на docs/quickstart.md) перед Building from
      source; quickstart-ссылка в шапке. Существующий контент (Show me the code/Memory/What's
      removed/License) сохранён без изменений.
- [ ] **A-Q3 ▶СЕЙЧАС** Язык-тур (выжимка из spec/decisions, не вся спека) `[docs]` (sonnet, крупнее).
- [ ] **A-W1 ▶СЕЙЧАС** Страница релиза на сайте: версия/скачать/quickstart-ссылка `[репа www/site]`
      (sonnet — ПОЛНОСТЬЮ независимая репа). — ЗАПУЩЕН
- [ ] **A-Q4** Release notes v0.1.0 (draft ▶СЕЙЧАС из истории сессий; финал ⛓Ф.0).

## Ф.4 — Релиз (финальная последовательность, всё ⛓)

- [ ] **A-R1** Финальный CI-прогон на релизном коммите (все гейты, оба ОС) — ⛓всё.
- [ ] **A-R2** A-V6 теги+артефакты → страница A-W1 live → анонс (текст — владелец).

## Матрица параллельности (2026-07-21)

СЕЙЧАС ЗАПУЩЕНО: A-B1 (4 волны) + A-V1/A-V2 (version+zip) + A-W1 (www-страница).
СЛЕДУЮЩИЕ СЛОТЫ: A-S1 (полный тест, фоновые Bash) → A-Q1/Q2 → A-D1 (docker) → A-V3/V4 → A-Q3.
Зонная развязка: доки/скрипты/www не пересекаются с баг-волнами вообще; A-B2..B6 —
последовательность по зонам emit_c/types.

## Оценка (после декомпозиции)

Ф.0 ~2 дня · Ф.1 хвост ~1 день (A-S1 гонится уже сейчас) · Ф.2+Ф.3 — параллельно Ф.0
(готовы к моменту Ф.0-выхода) · Ф.4 — полдня. **Реалистично: ~4-6 рабочих дней до v0.1.0**
(было ~1.5 недели — параллелизация доков/дистрибуции срезает хвост).
