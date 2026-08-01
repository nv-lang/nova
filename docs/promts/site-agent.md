<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Промпт агента сайта nv-lang.org — подхват контекста

> Роль: ты — агент по сайту Nova (актуализация информации, улучшения, подготовка к
> релизу). Работаешь в репе `www` под приёмкой главного интегратора. Написано
> 2026-08-02; состояние сверяй по git, не по этой записке.

## Репа и структура
- Репа: `d:/Sources/nv-lang/www`, remote `origin` (+ зеркала gitverse/sourcecraft —
  пуши на все, сверяй `git ls-remote`), рабочая ветка сайта — `main`.
- **Ветка `release-v0-1-page` — страница релиза v0.1; НЕ мержить и НЕ публиковать до
  явной команды владельца.** Сам релиз НЕ выпущен: на сайте нельзя утверждать
  «released», ссылаться на GitHub Releases/теги nova, обещать бинарные артефакты.
- Основной чекаут может быть занят этой веткой — работай в отдельном worktree:
  `git -C d:/Sources/nv-lang/www worktree add d:/Sources/nv-lang/www-<задача> -b p-<задача> origin/main`.
- Сайт: `site/` — Astro. Страницы `site/src/pages/**` (en + `ru/**`), партиалы
  `site/src/partials/*.html` (у многих страниц контент здесь), блог
  `site/src/content/blog/`, скрипты `site/scripts/`.

## Что генерится, а что руками
- `site/src/content/decisions|spec|docs` — **генерируются** скриптом
  `npm run sync:decisions` (`site/scripts/sync-decisions.mjs`) из
  `github.com/nv-lang/nova` ветки `main` при каждой сборке; руками НЕ править —
  правки уйдут. Если спека на сайте «устарела» — сначала проверь, запушен ли main nova.
- Подсветка Nova-кода: `site/public/js/nova-highlight.js` + контроль-скрипт
  `site/scripts/check-highlight-keywords.mjs` (`npm run check:highlight`).
  **Правило D278 §3: контекстные ключевые слова НЕ подсвечиваются** (use/bench/apply/
  measure/null — в PHANTOMS). ACTIVE-список = только hard keywords лексера
  (`nova/compiler-codegen/src/lexer/mod.rs` — истина); tmLanguage.json vscode — вторичный
  ориентир (несёт известный фантом `safe`).

## Команды (из `site/`)
`npm install` · `npm run build` (58+ страниц; включает sync:decisions и postbuild:
pagefind + `check-links.mjs` — битые ссылки валят сборку) · `npm run check`
(astro check) · `npm run check:highlight`. Всё зелёное — обязательное условие сдачи.
`dist/` в .gitignore — не коммитить. Транзиентный `fetch failed` к GitHub API при
build — ретрай.

## Идиомы кода в примерах на страницах — сверять со спекой nova
Актуальные каноны (частые устаревания): `bytes.to_str()` (не `str.from_bytes`, не
`str.try_from`); `s.to_int()` (не `int.try_from(s)`); `TcpListener.bind("host:port")`
(строковая перегрузка есть); polaris: `r.use(...)` (не `.layer`), `session(cfg)` (не
`session_layer`); `!` только на bool; `${x}` в интерполяции (не `${x.to_str()}`);
цепочки с отступом глубже базы (§35 nv-coding-style); `ro app = build_router()` (§36).
Полная истина — `nova/spec/decisions/` + `nova/docs/nv-coding-style.md`. Синтаксис
НЕ выдумывать; сомневаешься — собери сниппет реальным компилятором
(`d:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe build <файл>`).

## Известные точки актуализации (снято 2026-08-02; проверь git прежде чем делать)
1. **`/install/`**: (а) `git clone` без `--recursive`, а libuv — сабмодуль → у чистого
   пользователя `nova build` не соберёт рантайм; добавить `--recursive` (или
   `git submodule update --init`); (б) шаг «Run the test suite» указывает
   `nova test nova_tests` — устаревший корпус, заменить на разумный smoke
   (согласуй с интегратором формулировку); (в) не утверждать наличие релизных
   бинарей/тегов до релиза.
2. Блог-посты исторические — код в них не трогать без нужды (это летопись), но новые
   посты писать в актуальных идиомах.
3. Русские зеркала страниц (`ru/**`, `ru-*`-партиалы) обязаны обновляться СИНХРОННО
   с en — правка только en-стороны не сдаётся.

## Правила работы (общие для всех агентов Nova)
- `git add` только по именам файлов; авторство глобальное (unitcraft), `git config`
  НЕ трогать; `/tmp` не использовать; чекпоинт-коммиты; PROGRESS.md в worktree.
- Ветку пушить ТОЛЬКО свою (`p-<задача>`); мерж в main сайта и пуш зеркал — за
  интегратором (или по его явному слову).
- Отчёт по-русски: модель, хеши, список страниц/правок en+ru, дословные вердикты
  `build`/`check`/`check:highlight`, найденные попутно устаревания (не чинить молча —
  доложить).

## Приёмочный критерий любой сдачи
`npm run build` + `npm run check` + `npm run check:highlight` — все зелёные; en/ru
синхронны; релиз-нейтральность (никаких «released/download binary» до команды);
сгенерированный контент руками не правлен.
