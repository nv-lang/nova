<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 210 — чекпоинт прогресса реализации (embed_dir)

> Рабочий файл исполнителя (worktree `nova-210`, ветка `p210-embed-dir`). Обновляется ПЕРЕД
> каждым коммитом. Карта — [210-embed-dir.md](210-embed-dir.md).

## Статус фаз

- **Ф.1 (std-тип)** — ГОТОВО. `std/src/prelude/embed.nv` (EmbeddedEntry/EmbeddedDir +
  `.new`/`@len`/`@paths`/`@has`/`@entries`/`@get`) + re-export в `std/src/prelude.nv`
  (`PRELUDE_VERSION` 17→18) + `std/src/prelude/embed_test.nv` (7 тестов, включая 2
  `panics "sorted"` на несортированном/дубликате пути + алиас-защита). Гейт:
  `nova check std/src/prelude/embed.nv std/src/prelude/embed_test.nv` — PASS;
  `nova test std/src/prelude/embed_test.nv` — 7/7 PASS.
- **Ф.0 (спека)** — ГОТОВО. D412-амендмент дописан в конец `spec/decisions/03-syntax.md`
  (после существующего D412) — форма `embed_dir`, контракт `EmbeddedDir`, детерминизм,
  dot/symlink-skip, W_EMBED_DIR_LARGE (16 MiB, §9.2 ревью-3 порог), backslash-запрет,
  non-ASCII warning, CRLF/autocrlf док-строка, Option E (C23 `#embed`) как future.
- **Ф.2 (резолвер)** — ГОТОВО. `compiler-codegen/src/embed_resolve.rs`:
  `try_replace_embed_dir` (зеркало `try_replace_embed`) + свободная `walk_embed_dir_rec`
  (рекурсивный обход, dot-skip, symlink-skip+warn, non-ASCII-warn) + синтез
  `Call{EmbeddedDir.new([RecordLit{EmbeddedEntry,...}, …])}`. `resolve_embeds` теперь
  возвращает `(Vec<PathBuf>, Vec<LintWarning>)` вместо голого `Vec<PathBuf>` — warning-канал
  был реально пуст на success-пути (подтверждено эмпирически ДО фикса), пофикшено во ВСЕХ 4
  call-сайтах (nova-cli check/build, compiler-codegen check/compile, test_runner
  codegen_to_c). Добавлены `E_EMBED_IS_A_DIR` (в `try_replace_embed`, симметрия) +
  `E_EMBED_PATH_BACKSLASH` (общий helper `check_path_backslash`, оба интринсика).
  compiler-codegen + nova-cli release собираются чисто (0 новых warning после чистки
  ложного "value never read").
- **Ф.4 (фикстуры)** — ГОТОВО. pos (`d412d_embed_dir.nv` + `d412d_dir/{alpha.txt,beta.txt,
  nested/gamma.txt,.hidden}`) — merged в `spec_tests.conformance` CU. 6 neg в `neg/`
  (not_found/not_a_dir/escape/not_literal/embed_on_dir + бонус backslash) — все PASS
  таргетно. 2 standalone (`W_EMBED_DIR_EMPTY` на `d412d_dir_empty/.gitkeep`;
  `W_EMBED_DIR_NON_ASCII_PATH` на `d412d_dir_unicode/café.txt`) — PASS, warning подтверждён.
- **Верификация Ф.2 (спот-грепы)** — ГОТОВО. `.c` для `d412d_embed_dir_nonascii` (test-build
  --keep-artifacts): `nova_blob_view(nova_blob_0bd47e37eb578d59, 10)` — zero-copy view,
  БЕЗ memcpy; путь `café.txt` интернирован как static `nova_str` (9 байт UTF-8, `é` НЕ
  нормализован). Два прогона того же файла → diff ТОЛЬКО в порядке generic-типа
  forward-decl (`Nova_T/S/U/E`) — известная ПРЕ-EXISTING nondeterminism
  (`[M-codegen-emission-nondeterminism]`, подтверждена сверкой на НЕТРОНУТОМ
  `standalone/n6_opaque_literal_warning.nv` — тот же класс diff). Весь embed_dir-related
  контент (blob-байты, entries-порядок, interned-строки) — БАЙТ-В-БАЙТ идентичен между
  прогонами.
- **δ-нейтральность `nova check std`** — подтверждена ОКОНЧАТЕЛЬНО прямым сравнением
  полного прогона main (нетронутый репо, свой бинарь) vs nova-210 (после ВСЕХ фаз):
  FAIL 21 == 21 — СПИСОК ФАЙЛОВ БАЙТ-В-БАЙТ ИДЕНТИЧЕН (encoding/serde_neg×6,
  fs/neg×3, io/neg×2, net/neg×3, time/civil×4 — все `nova check`-артефакты
  `neg/`-фикстур, которые сам `nova check` не умеет трактовать как
  ожидаемо-падающие, плюс пре-existing `E_STR_NO_LEN` в date.nv). PASS 118→120 (+2:
  новые embed.nv/embed_test.nv), WARN 151→153 (+2: тот же системный
  "unused import Vec" на новых файлах). Ноль регрессий.
- **Ф.3 (флагман, опционально)** — ПРОПУЩЕНО. `examples/flagship/aggregator` сейчас
  встраивает ОДИН самодостаточный `frontend/index.html` (58 КБ, инлайн JS/CSS) —
  замена на `embed_dir` была бы недраматичной демонстрацией (папка из 1 файла) и
  требует живого HTTP-раунда через продакшен-пример с деликатной историей
  concurrency-wedge (см. недавние коммиты 187/211) — риск/выгода несоразмерны при
  явно опциональном, не-блокирующем статусе фазы и указании «хост нагружен» в задании.
  Явно отмечено по указанию плана («если пропускаешь — отметь»).

## Коммиты (ветка `p210-embed-dir`)

1. Ф.1 std-тип (`std/src/prelude/embed.nv` + `embed_test.nv` + `prelude.nv`).
2. Ф.0 спека (`spec/decisions/03-syntax.md` D412-амендмент).
3. Ф.2 резолвер (`embed_resolve.rs` + 4 call-сайта warning-канала).
4. Ф.4 фикстуры (pos/neg×6/standalone×2).

## Найденное и исправленное по ходу верификации

- **Баг в собственной фикстуре** (`d412d_embed_dir_empty.nv`): пояснительный
  комментарий начинался ровно с текста `EXPECT_COMPILE_WARNING` (проза, не
  директива) — раннер (first-wins per marker-type) распознал ПРОЗУ как маркер
  раньше настоящей директивы строкой ниже → `NEG-WRONG-WARN`. Пофикшено
  перефразированием прозы (не начинать строку с текста маркера). Урок для
  будущих фикстур: держать пояснения про EXPECT_* маркеры не в виде строки,
  буквально начинающейся с имени маркера.
- **Мега-CU дисциплина**: `d412d_embed_dir.nv` живёт в `spec_tests.conformance`
  (974+ файлов, один CU) — `nova test` на нём триггерит ПОЛНУЮ мега-CU сборку
  (нарушает «мега-CU не гонять»). Верифицировано ОБХОДНЫМ путём: временный
  standalone-дубликат (`module standalone._verify210_tmp`, path
  `"../d412d_dir"` на реальную закоммиченную фикстуру) — PASS, затем удалён
  (не коммитился). Авторитетная проверка мега-CU — у оркестратора при
  вливании.

## Открытые мелочи / отклонения от плана

- `embed_dir(".")`/`""` (self-embed корня пакета, §9.2 ревью-3 п.3) — НЕ реализовано.
  Это была ОТКРЫТАЯ развилка владельца в плане (§9.2.3: «решение владельца; дефолт —
  запретить»), не входящая в закрытую таблицу кодов §4.3 и не перечисленная в
  Ф.2-списке кодов top-level задания. Оставлено как есть (текущее поведение:
  `embed_dir(".")` резолвится и работает как обычный каталог — НЕ запрещено явно).
  Не блокер (не в объёме §4.3).

## Ф.6(б) — ReadFs (VFS-унификация, 2026-07-16, sonnet, отдельная волна)

- **R1 (главный риск §6б.7) — ЗЕЛЁНЫЙ эмпирически, ПЕРВОЙ ФАЗОЙ по указанию задания.**
  Написан `std/src/fs/readfs.nv` (`ReadFs` протокол + `EmbeddedDir`-extension-методы +
  `DirFs`) целиком (нужен независимо от исхода R1 — только conformance-путь EmbeddedDir
  мог бы измениться), затем `std/src/fs/readfs_test.nv` с двумя R1-тестами
  (`[F ReadFs]`-generic над `EmbeddedDir` и над `DirFs`). `nova check`/`nova test`
  (обычный и `--strict-effects`) — PASS: структурная conformance по generic-bound
  видит EXTENSION-метод `EmbeddedDir @read_file`/`@try_exists` (объявлен в `std.fs`,
  НЕ в родном `prelude.embed`) наравне с inherent. **Вывод: extension-путь, БЕЗ
  wrapper-newtype `EmbeddedFs`** (fallback из дизайна не понадобился).
- **Файлы:** `std/src/fs/readfs.nv` (протокол `ReadFs` + `EmbeddedDir @read_file`/
  `@try_exists` extension + `DirFs`/`DirFs.new`/`@resolve`(priv)/`@read_file`/
  `@try_exists`), `std/src/fs/readfs_test.nv` (9 тестов).
- **Гейт:** `nova check std/src/fs/readfs.nv std/src/fs/readfs_test.nv` — 2/2 PASS
  (тот же систематический "unused import"-шум, что и на любом файле fs-модуля —
  подтверждено сверкой на нетронутом `fs.nv`, не регрессия). `nova test
  std/src/fs/readfs_test.nv` — PASS (9/9 test-блоков, включая `--strict-effects`
  на обеих командах).
- **Отклонение от дизайн-черновика (§6б.3):** `DirFs @resolve`'ская prefix-проверка
  реализована как component-граничная строковая проверка по ОБОИМ разделителям
  (`rest.starts_with("/") || rest.starts_with("\\")`), а НЕ буквальный
  `f.starts_with(r + "/")` из черновика. Причина: `canonicalize` под `mock_fs`
  (`std/src/fs/mock.nv:97-100`, `mem_key`) всегда возвращает POSIX-ключи независимо
  от host-style-тега на возвращаемом `Path` (`Path.from_bytes` тегирует host-стилем
  байты КАК ЕСТЬ — на этом (Windows) хосте тег будет `Windows`, но фактические байты
  от mock — с `/`), тогда как реальный диск на Windows отдаёт `\`. Черновик сам
  оставлял выбор реализации открытым (§9.2 ревью-3 п.2: «либо этот guard, либо
  сравнение `@components()`») — выбрана строковая граница по обоим байтам как более
  простая и одинаково переносимая на mock/реальный диск, без привязки к
  (потенциально не совпадающему с фактическими байтами) style-тегу.
- **Спека:** D323-амендмент дописан в конец секции D323 (`spec/decisions/04-effects.md`,
  перед D324) — протокол `ReadFs`, `DirFs`, extension-conformance `EmbeddedDir`, dev/prod-
  выбор веткой на инстанциации. `docs/guide/io-fs.md` — абзац «`ReadFs` — one VFS protocol over
  the disk and an embedded directory» в разделе Protocols vs the text sink. Одна строка
  «См. также» в D412-амендменте (`spec/decisions/03-syntax.md`), ссылающаяся на D323-
  амендмент.
- **НЕ реализовано этой волной (по плану, опционально):** Ф.6б.4 — флагман-потребитель
  (`examples/flagship/aggregator` на `serve_assets(embedded_assets())`) — карта есть
  в дизайне (§6б.4/6б.6), реализация явно отложена («НЕ реализуется этой волной»
  в самом дизайне).
- **Symlink-hard escape у `DirFs`** покрыт СТРУКТУРНО (переиспользует `canonicalize`/
  `realpath` из `fs.nv`, уже протестированный там своими юнит-тестами) — отдельного
  теста «реальный symlink наружу корня» в `readfs_test.nv` НЕТ: `mock_fs`
  (`mem_realpath`) не дереференсит symlink-цели вообще (просто проверяет узел по
  ключу), а создание РЕАЛЬНОГО symlink на Windows требует привилегий, которых нет
  прецедента запрашивать в этом test suite (ни один существующий `_test.nv` в
  `std/src/fs` не создаёт symlink реальным `Fs.symlink`). Задокументировано как
  осознанный, а не пропущенный, пробел.
