<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 210 — `embed_dir(...)`: вшить папку в бинарь (расширение D412)

> **Статус: ✅ РЕАЛИЗОВАНО 2026-07-16** (owner-go получен; ветка `p210-embed-dir`,
> worktree `nova-210`; модель sonnet). Ф.0 (спека, D412-амендмент)/Ф.1 (std-тип)/
> Ф.2 (резолвер)/Ф.4 (фикстуры) — ГОТОВЫ и провалидированы (`nova check std` δ0
> прямым сравнением с main: FAIL 21==21 байт-в-байт; targeted-фикстуры pos/neg/
> standalone — все PASS; спот-грепом `.c` подтверждён zero-copy + 0 правок
> emit_c.rs). Ф.3 (флагман-демо) — ПРОПУЩЕНА (опционально/не блокер, см.
> `wip/210-impl-progress.md`). **В main НЕ вливалось** — авторитетный гейт
> (мега-CU conformance + флагман-examples под `--strict-effects`) делает
> оркестратор при вливании. Прогресс/детали — [210-impl-progress.md](wip/210-impl-progress.md).
> **Ф.5 (остаток, владелец 2026-07-16): user-facing дока** — `docs/guide/embed.md`
> (гайд по обоим интринсикам: `embed("file")` D412 + `embed_dir("dir")`): как
> использовать, API `EmbeddedDir` (`get/has/paths/len/entries`), детерминизм,
> dot/symlink-skip, коды E_/W_ (§4.3), NFC/NFD-ловушка non-ASCII имён
> (`W_EMBED_DIR_NON_ASCII_PATH` — почему), rodata-мина `[M-d412-blob-view-mut-write]`,
> взаимодействие с 209 (×5.3 hex-рендер, порог `W_EMBED_DIR_LARGE` 16 MiB).
> **Ф.6 (вопросы владельца 2026-07-16, дизайн-развилки на следующую волну):**
> (а) **NFC-нормализация путей** при встраивании (+`E_` на коллизию форм) —
> мотивация: воспроизводимость бинаря между платформами (macOS отдаёт NFD,
> git precomposeunicode хранит NFC → сейчас один и тот же чекаут может дать
> разные байты пути на разных ОС); цена: NFC-таблица Юникода в компиляторе.
> V1-статус-кво (warning) = поведение Go/Rust. (б) **VFS-унификация à la Go
> `embed.FS`+`fs.FS`**: read-only протокол чтения (условно `ReadFs`),
> реализуемый и обычной ФС, и `EmbeddedDir` — главный кейс: статика
> веб-сервера «dev = с диска (live-reload), prod = embedded» одним кодом;
> дизайн-вопросы: форма ошибок (embedded infallible vs io-Result) и
> эффект-полиморфизм (embedded-чтение чистое, fs-чтение эффектное).
> **Ф.6(б) (ReadFs) — ✅ РЕАЛИЗОВАНО 2026-07-16** (owner-го по рекомендациям
> дизайна; sonnet, worktree `nova-210`, ветка `p210-embed-dir`). **R1
> (главный риск §6б.7) — ЗЕЛЁНЫЙ эмпирически**: структурная conformance по
> generic-bound `[F ReadFs]` видит extension-метод `EmbeddedDir @read_file`/
> `@try_exists` (объявлены в `std.fs`, НЕ в родном `prelude.embed`) наравне с
> inherent — extension-путь подтверждён, wrapper-newtype fallback
> (`EmbeddedFs`) НЕ понадобился. `std/src/fs/readfs.nv` (`ReadFs`
> протокол + `EmbeddedDir`-extension + `DirFs`) + `std/src/fs/readfs_test.nv`
> (9 тестов: R1×2, sanity, hit/miss/escape/абсолютный-путь/try_exists,
> dev/prod-унификация §6б.4) — все PASS таргетно (`nova check`/`nova test`,
> включая `--strict-effects`). D323-амендмент в `spec/decisions/04-effects.md`
> + `docs/guide/io-fs.md` абзац + строка в D412-амендменте (`03-syntax.md`).
> **Одно отклонение от дизайн-черновика** (задокументировано в коде и §6б.3-примечании
> здесь): `DirFs @resolve`'ская prefix-проверка — component-граничная строковая
> проверка по ОБОИМ разделителям (`/` и `\`), а не `f.starts_with(r + "/")` —
> потому что `canonicalize` под `mock_fs` всегда отдаёт POSIX-ключи независимо
> от host-style-тега на `Path` (тестовое окружение — Windows, где реальный диск
> отдаёт `\`); черновик сам оставлял выбор реализации открытым («сравнить
> `@components()` или добавить сепаратор-guard», §9.2 ревью-3). Ф.6б.4
> (флагман-потребитель) — НЕ реализована этой волной (карта есть, опционально,
> см. §6б.6/6б.4) — как и предписано дизайном.
> **Ф.7 — Go-паритет+ (glob/embed_str/hidden/merge) и Ф.8 — эффективная эмиссия
> payload — ✅ РЕАЛИЗОВАНО 2026-07-17** (owner-go «впиши в план и реализуй»;
> sonnet, worktree `nova-210g`, ветка `p210-goparity`). Секции —
> [ниже, после §6б](#ф7--go-паритет-globembed_strhiddenmerge) и
> [Ф.8](#ф8--эффективная-эмиссия-payload-embedincbin). Нумерационная коллизия:
> старая секция «Ф.7 — `read_dir`» (только owner-GO, НЕ реализована) переименована
> в **Ф.9** ниже — освобождает номера Ф.7/Ф.8 под эту волну.
> ---
> *(Ниже — оригинальный дизайн-документ, сохранён как есть для истории решений.)*
> **Статус: ✅ ДИЗАЙН ФИНАЛИЗИРОВАН 2026-07-16 (вариант A + материализация Option R, ревиз. R″ — единая метка: R′=ревью-1, R″=ревью-2/9.1/ревью-3).**
> Развилки закрыты (§2), сигнатуры зафиксированы (§4), карта исполнения §6, гейты/риски §7.
> **Ждёт owner-go на Ф.0** (спека D412-амендмент §9). Реализация язык-меняющая — без go не стартует.
> **Q1/Q2/Q3 РЕШЕНЫ владельцем 2026-07-16 по рекомендациям:** Q1=Option R · Q2=dot-skip · Q3=`entries_of()`.
> **Ревью-правки 2026-07-16 (оркестратор, внесены по указанию владельца) — Option R → R′:** синтез =
> `Call` на статик-ктор (НЕ `RecordLit`) + поле `priv entries` → sorted-инвариант
> ЗАЩИЩЁН (нельзя сконструировать несортированным извне; тихий None в `get()` исключён); Q3 пересмотрен —
> `entries_of()` НЕ нужен, публичный аксессор = явный `@entries()` над priv-полем (D117-прецедент `Vec @ptr()`,
> нет «двух дверей» D9); +`W_EMBED_DIR_EMPTY`, +`W_EMBED_DIR_NON_ASCII_PATH` (NFD/NFC-портируемость),
> +`E_EMBED_IS_A_DIR` (симметричная подсказка в `embed`); в спеку пин «порядок сортировки == `str.compare`».
> **Ревью-2 2026-07-16 (второй проход, внесено по «да» владельца) — R′ дотянут:** (1) ктор =
> **`EmbeddedDir.new(entries)`** (конвенция «конструкторы = Type.new», прецедент `Vec.new`; `from_entries`
> нарушал §1а «пятую дверь» — переименован); (2) **алиасинг закрыт с обеих сторон**: ктор хранит защитную
> мелкую копию входа (`clone()` заголовков, payload'ы не копируются), `@entries() -> ro []EmbeddedEntry`
> (L2 read-only, прецедент `str @bytes()`); (3) зафиксирована УНАСЛЕДОВАННАЯ rodata-мина D412
> (`mut`-алиас view из `get()`/переменной → запись в rodata → SEGV) — строка в §9 + floating-маркер
> `[M-d412-blob-view-mut-write]` (backlog, P3, home D412); (4) §1-таблица синхронизирована с Call-синтезом;
> (5) dot-skip не касается ЯВНО названного корня (`embed_dir(".assets")` — встраивается).
> Источник дизайна: [research 2026-07-15 embed-dir](../research/2026-07-15-embed-dir-proposal.md)
> (§3 вариант A + cross-language survey). Маркер: `[M-embed-dir]` (backlog, APPROVED/queued).
> Родитель: [D412](../../spec/decisions/03-syntax.md#d412) (`embed("file")`, single-file, Plan 186).
> **Приоритет P3** (удобство, не блокер: single-embed + самодостаточный html покрывают MVP флагмана 187).

## 0. Цель (одна фраза)

`embed_dir("dir")` — компайл-тайм интринсик: вшивает ВСЮ папку (рекурсивно) в бинарь и
возвращает иммутабельный `EmbeddedDir` (`get(path)->Option[[]u8]` + `paths()->[]str`, нулевая
копия payload'ов), чтобы «фронт целиком в программе» собирался одной строкой (эргономика Go
`//go:embed`+`embed.FS`).

---

## 1. Разведка: как устроен single-embed (D412 / Plan 186) сейчас

Точные точки, которые 210 переиспользует (**210 не строит новую механику — он оркеструет
существующую**). Проверено по коду 2026-07-16.

| Слой | Файл : функция : строки | Что делает | 210 использует |
|---|---|---|---|
| **Резолвер** | `compiler-codegen/src/embed_resolve.rs` : `resolve_embeds` : 38–103 | AST-пасс ПОСЛЕ `resolve_imports_inline`, ДО type-check. Обходит `module.items` + `peer_files[].items_here`. Возвращает `Ok(Vec<PathBuf>)` — canonical-пути встроенных файлов (sort+dedup, стр. 91–93) для fingerprint. | Точка расширения Ф.2 — та же `resolve_embeds` распознаёт и `embed_dir` |
| **Замена узла** | `embed_resolve.rs` : `try_replace_embed` : 164–251 | `embed("lit")` → `e.kind = ExprKind::HexBlobLit(bytes)` (стр. 236). Дальше блоб неотличим от `x"…"`. | Ф.2 добавляет `try_replace_embed_dir` — синтез `Call` на `EmbeddedDir.new(...)` из `HexBlobLit`-элементов (§3, R′) |
| **Границы вызова** | `embed_resolve.rs` : `per_file_embed_root` : 119–129 | Plan 193 gap-2: для peer-файла внутри `project_root` → сам root; для peer из внешней `[dependencies]` → его СОБСТВЕННЫЙ package-root (ближайший `nova.toml`). | Наследуется без изменений (§2и) |
| **Коды ошибок** | `embed_resolve.rs` : 175–248 | `E_EMBED_ARG_NOT_STR_LITERAL` (175/186/195), `E_EMBED_NOT_FOUND` (208/240), `E_EMBED_OUTSIDE_PROJECT` (222). | Reuse + новый `E_EMBED_DIR_NOT_FOUND` / `E_EMBED_NOT_A_DIR` (§4) |
| **Тип узла** | `types/mod.rs` : 7737, 13959; `number_exprs.rs` : 108 | `HexBlobLit` → `Vec[u8]` ≡ `[]u8` (D239 nominal). | Каждый `data`-элемент типизируется автоматически как `[]u8` — новых арм в чекере НЕ нужно |
| **Материализация** | `codegen/emit_c.rs` : `HexBlobLit`-арм : 28579–28588 | Значение-выражение = `nova_blob_view(sym, n)` — **zero-copy вид** (24-байт header в GC-куче, `data`→static rodata, `len==cap==N`; НЕТ memcpy payload'а). Пустой → `nova_blob_view((const uint8_t*)0, 0)`. | **Не трогаем.** `data`-элементы синтезированного `RecordLit` идут через ЭТУ арм → payload'ы zero-copy бесплатно |
| **Интернирование** | `emit_c.rs` : `intern_blob_literal` : 54879–54900 | Символ `nova_blob_<fnv1a-16hex>`, дедуп по содержимому (два одинаковых файла → один static). Коллизия хэша → суффикс `_seq`. | Дедуп покрывает и дубли файлов между папками, и seq-суффикс — бесплатно |
| **mut/consume копия** | `emit_c.rs` : Let-арм : 26599–26605 (→ `nova_blob_copy`) | ТОЛЬКО при `mut`/`consume`-биндинге блоба: копия в GC-кучу. ro-биндинг = вид. | `data`-поля — не отдельные биндинги, всегда идут как view-выражения (нулевая копия) |
| **Runtime-хелперы** | `nova_rt/array.h` : `nova_blob_view` 935–941, `nova_blob_copy` 932 | View-конструктор + copy-конструктор. Boehm игнорит указатели вне своей кучи → static-блоб не собирается/не двигается. | Не трогаем |
| **Fingerprint кэша** | `nova-cli/src/build_cache.rs` : `compute_c_key` : 56–131 (`embed_files` param 71; хэш 123–131) | Хэширует `count` + путь + содержимое каждого embed-файла. | Ф.2 кладёт ВСЕ обойдённые файлы папки в возвращаемый `Vec<PathBuf>` → add/rm/change любого файла инвалидирует кэш (§2ж) |
| **Порядок в pipeline** | `nova-cli/src/main.rs` : 4662–4664 (build) / 2271–2276 (check) | `resolve_embeds` вызывается БЕЗУСЛОВНО и ДО `compute_c_key` (4728). | Гарантирует, что перечень файлов папки всегда свежий → инкрементальность корректна |
| **Спека** | `spec/decisions/03-syntax.md` : D412 : 11513–11562 | Форма, границы, «Материализация: указатель в статические данные». | Ф.0 амендит (§9) |
| **Тесты-образцы** | `spec_tests/conformance/d412_hex_blob_embed.nv` (pos, тест «embed round-trips» 79–90); `neg/d412_embed_not_found_neg.nv`; `neg/d412_embed_not_literal_neg.nv`; фикстура `d412_embed_fixture.bin` | Конвенция §116 folder-module + `EXPECT_COMPILE_ERROR`. | Ф.4 копирует шаблон для `embed_dir` |

**Ключевой вывод разведки:** single-embed = «переписать AST-узел в `HexBlobLit`, дальше всё
существующее». `emit_c` для payload'ов УЖЕ даёт zero-copy static. 210 наследует это целиком, если
`embed_dir` тоже переписать в узлы, состоящие из `HexBlobLit` (→ §3).

---

## 2. Ключевые решения (converged 2026-07-16)

| # | Вопрос (из задания) | Решение | Обоснование / сверка |
|---|---|---|---|
| **а** | Форма/семантика | `embed_dir("relative/dir")` — компайл-тайм интринсик класса C, аргумент **только str-литерал**. **Рекурсивен по умолчанию** (весь поддеревья). **Glob/фильтр — вне объёма** (future). | Go `//go:embed dir` рекурсивен; Rust `include_dir!` рекурсивен. Литерал — паритет single-embed (путь известен на компиляции). |
| **а'** | Детерминизм | Записи **сортируются по POSIX-пути** (UTF-8 байтовый лексикографический порядок) при синтезе → воспроизводимый `.c`. | ФС-обход недетерминирован по ОС; `resolve_embeds` уже сортирует `files` (стр. 91). Два билда → идентичный `.c` (гейт §7). |
| **б** | Тип `EmbeddedDir` | **Вариант A** (owner-принят; ревиз. R′): `get(path str)->Option[[]u8]`, `paths()->[]str`, `len()->int`, `has(path str)->bool`, `entries()->[]EmbeddedEntry` (явный аксессор над `priv`-полем — итерация пар без двойного lookup; `entries_of()` снят, см. §8-Q3′). Иммутабелен (нет `mut`-методов); поле `priv` → sorted-инвариант защищён. | Go `embed.FS`: `ReadFile`/`ReadDir`. Rust `rust-embed`: `.get()`. Минимум для «раздать mux'ом» = get+paths; `entries()`/`has()` — эргономика. |
| **в** | Нулевая копия в C | **payload'ы** (байты файлов) → static rodata через существующую `HexBlobLit`-арм (view, `len==cap==N`, БЕЗ копии). **Таблица** (пути + view-headers) — маленький one-time GC-alloc при вычислении выражения (§3 Option R). | Payload = крупные данные, они zero-copy (главное). Таблица = O(N) мелочь (≈40 байт/файл + interned path). Полностью-static таблица = Option N (§3, future-опт). |
| **г** | Lookup | **Бинарный поиск** по отсортированным записям (O(log N)). Perfect-hash — **не оправдан** (типичный N — десятки–сотни). | Go `embed.FS` хранит файлы отсортированными и бинарит. Инвариант «entries отсортированы по path» в контракте (§9); единственный конструктор (`embed_dir`) его держит. Fallback — линейный скан (проще, для малых N нормально). |
| **д** | Пути | Ключ = путь **относительно embed-корня**, разделитель POSIX `/` (Windows `\` → `/` при обходе), без ведущего `./`. **Кейс-чувствительность = байт-точная** (case-SENSITIVE lookup). | Go `embed.FS` — POSIX-слэши, case-sensitive. Кросс-платформенная воспроизводимость ключа. |
| **е** | Лимиты / шум | (1) **Скип dot-файлов и dot-папок** (скрытые, имя с `.`) по умолчанию — не попадают в `EmbeddedDir`; правило касается записей ВНУТРИ обхода, НЕ самого аргумента: `embed_dir(".assets")` — корень назван явно → встраивается (явное имя побеждает скрытость). (2) **Скип симлинков** (файлы и папки) + build-**WARNING** `W_EMBED_DIR_SYMLINK_SKIPPED`. (3) **Мягкий WARNING** `W_EMBED_DIR_LARGE` при суммарном размере > 64 MiB ИЛИ > 4096 файлов. Жёсткого size-error нет. (4) **`W_EMBED_DIR_EMPTY`** (ревью R′): папка существует, но после dot/symlink-скипа встраивать нечего — ловит «навёлся на пустую соседку» (опечатка в пути даёт `E_EMBED_DIR_NOT_FOUND`, а этот случай раньше молчал; симметрично мотивации LARGE). (5) **`W_EMBED_DIR_NON_ASCII_PATH`** (ревью R′): не-ASCII имя файла — предупредить о непортируемом ключе (macOS хранит имена в NFD, Windows/Linux обычно NFC → один репозиторий даёт РАЗНЫЕ байтовые ключи и разный `.c` на разных ОС; `get("é")` в NFC промахнётся по NFD-ключу). Ключ = байты имени КАК ЕСТЬ, без Unicode-нормализации (пин в §9). | Go по умолчанию исключает `.`/`_`-префиксные (эталон). Берём только `.` (скрытые — универсально; `_` в Nova не спец — НЕ исключаем, чтобы не удивлять). Скип симлинков = safety (циклы + escape). Warning'и ловят «наведён не на ту папку»/непортируемость без падения; NFD/NFC — Go молчит, мы честнее. |
| **ж** | Инкрементальность | Все обойдённые файлы → в `Vec<PathBuf>` из `resolve_embeds` → `compute_c_key` (хэш путь+содержимое). **add/rm/change** любого файла → другой ключ → пересборка. Обход БЕЗУСЛОВЕН и ДО ключа кэша (main.rs 4662<4728). | Проверено: `compute_c_key` хэширует `count` + путь + байты (build_cache.rs 128–131). Новый файл → set растёт → ключ меняется. Удаление → set сжимается. Корректно. |
| **з** | Пакеты / D78 | `embed_dir` внутри folder-module или пакета-зависимости резолвится **относительно .nv-файла вызова**, граница — `per_file_embed_root` этого файла (Plan 193 gap-2). Папка вне package-root вызова → `E_EMBED_OUTSIDE_PROJECT`. | Наследование single-embed без изменений: `embed_dir` в зависимости видит СВОЁ дерево, потребитель зависимости не ловит ложный escape. |
| **и** | Безопасность | (1) Escape `..` выше корня → `E_EMBED_OUTSIDE_PROJECT` — проверяется для аргумента-папки И для КАЖДОГО обойдённого файла (симлинк внутри мог бы указать наружу — но симлинки скипаются, §е). (2) Папка не найдена → `E_EMBED_DIR_NOT_FOUND`; путь есть, но это файл → `E_EMBED_NOT_A_DIR`. (3) Пустая папка → пустой `EmbeddedDir` (легально). | Паритет D412 + skip-симлинков закрывает escape-через-линк и циклы. |
| **к** | LSP / tooling | **Вне объёма.** `embed_dir("dir")` переписывается в литерал ДО type-check → hover видит вызов, не перечень файлов. Future: диагностика/hover со списком встроенных путей. | Минимизация объёма; не блокер. |
| **л** | dev-режим (rust-embed) | **ОТКЛОНЁН.** Не вводим runtime-чтение-с-диска в debug. | (1) Противоречит цели «один бинарь». (2) Runtime-FS-чтение — ЭФФЕКТ (Os/Fs capability) → сломает чистый компайл-тайм тип `EmbeddedDir`. (3) Инкрементальная пересборка уже быстрая (кэш §2ж). Owner-флаг если понадобится (§8). |
| **м** | Content-Type / mime | **Вне объёма** интринсика (он про байты). Опц. mime-хелпер в `std/http` по расширению — отдельно/позже. | Разделение ответственности; Go тоже отдаёт `http.FileServer` поверх FS, mime — слой выше. |

---

## 3. Материализация: Option R (рекомендовано) vs Option N (альтернатива)

**Главное улучшение против исходного плана.** Исходный Ф.2 требовал «`emit_c.rs`: эмитить N
`.rodata`-блобов + статическую таблицу + сконструировать `EmbeddedDir`». Разведка (§1) показала:
это можно получить **с НУЛЁМ изменений в `emit_c`**.

### Option R′ — переписать в `Call` статик-конструктора из `HexBlobLit`-элементов (РЕКОМЕНДОВАНО; ревиз. R)

> **Ревью-правка 2026-07-16 (было: голый `RecordLit{EmbeddedDir}`).** Проблема исходного R:
> `RecordLit` по имени поля требует ПУБЛИЧНОЕ поле `entries` → пользователь может сам написать
> `EmbeddedDir { entries: [несортированные] }`, и `get()` (бинарный поиск) **тихо вернёт None на
> существующий путь** — sorted-инвариант ничем не защищён (Ф.1-тест сам конструировал вручную =
> дверь открыта by design). Фикс: синтезировать **`Call` на публичный статик-ктор** — Call не
> нарушает приватность полей → поле становится `priv`, инвариант защищён конструктивно.
> «0 правок emit_c» сохраняется: Call на Nova-метод — обычный узел.

`resolve_embeds` переписывает `embed_dir("frontend")` в обычный Nova-вызов (все узлы уже
существуют в AST):

```
Call { func: Path(["EmbeddedDir", "new"]), args: [        // ктор = Type.new (конвенция, ревью-2)
  ArrayLit([                                           // отсортировано по path (резолвером)
    RecordLit { type_name: Some(["EmbeddedEntry"]), fields: [
      path: StrLit("app.js"),                          // → interned static nova_str
      data: HexBlobLit(<байты app.js>),                // → nova_blob_view(static, N) — ZERO-COPY
    ]},
    RecordLit { EmbeddedEntry { path: StrLit("index.html"), data: HexBlobLit(<…>) }},
    …
  ])
]}
```

(`EmbeddedEntry` остаётся публично-конструируемым `RecordLit`-ом — у него инвариантов нет;
инвариант сортировки живёт на `EmbeddedDir` и охраняется `EmbeddedDir.new` + `priv entries`.)

Дальше **весь существующий конвейер** обрабатывает это как рукописный код:
- каждый `data: HexBlobLit` → `emit_c` арм 28579 → `nova_blob_view` над static rodata (**нулевая
  копия payload'а, ничего нового**);
- каждый `path: StrLit` → interned `static const nova_str` (Plan 139);
- `EmbeddedDir`/`EmbeddedEntry` — обычные std-record'ы; `new`/`get`/`paths`/`len` —
  обычные Nova-методы (Call → обычный static-dispatch).

**Свойства:**
- **emit_c — НЕ меняется.** Чекер — НЕ меняется (типы выводятся: `data:[]u8`, `path:str`;
  `EmbeddedDir.new` — обычный static). Вся Ф.2 = обход папки + синтез узлов в `embed_resolve.rs`.
- Payload'ы — zero-copy static (главное для «нулевой копии»).
- **Sorted-инвариант защищён герметично (ревью-2):** поле `priv entries` (D281) → извне
  `EmbeddedDir` конструируем ТОЛЬКО через `EmbeddedDir.new`, который (а) verify-sorted за O(N)
  (нарушение → panic с внятным сообщением, не тихий None; synth-путь уже отсортирован — verify
  даром), (б) хранит **защитную мелкую копию** входа (`clone()` — O(N) заголовков/указателей,
  payload'ы НЕ копируются) → пост-конструкционная мутация исходного вектора вызывающим НЕ
  затрагивает `EmbeddedDir`; (в) выход `@entries() -> ro []EmbeddedEntry` — read-only (L2).
- Таблица `entries` (`[]EmbeddedEntry`) строится один раз при вычислении выражения: O(N) мелких
  GC-alloc (Vec-буфер + N view-headers) + O(N) verify + O(N) защитная копия. Для `ro`/module-level
  биндинга — единожды.
- **Риск минимальный** — та же стратегия, что D412 (переписать AST, переиспользовать pipeline).

**Стоимость таблицы и рекомендация по использованию:** `embed_dir(...)` — выражение;
вычисляется на каждом исполнении. Биндить **один раз** (`ro assets = embed_dir("frontend")` на
уровне модуля / в `main`), НЕ звать в горячем пути. Хойстинг константного `embed_dir` в static —
future-опт компилятора (не в этом плане). Тот же нюанс есть у single-embed (`embed()` в fn
пересоздаёт view на вызов — дёшево).

### Option N — полностью-static таблица (АЛЬТЕРНАТИВА, future-опт)

Отдельный `ExprKind::EmbedDirLit(Vec<(String, Vec<u8>)>)` + кастомный `emit_c`, эмитящий
`static const NovaEmbedEntry nova_embeddir_<h>[]` (пути как static `nova_str`, `data` как static
`NovaArrHdr` над блобом) + `EmbeddedDir`-вид над static-массивом. **Zero-heap полностью.**

- **Против:** новый AST-узел → правки в ~30 match-сайтах (как у `HexBlobLit`), новая `emit_c`-арм,
  новая арм в type-inference. Больше кода/риска.
- **За:** таблица тоже в rodata (ноль alloc при инициализации).
- **Вердикт:** не нужен для типичного N. Таблица ничтожна против payload'ов. Если профилирование
  когда-нибудь покажет, что one-time сборка таблицы значима (нереалистично для front-ассетов) —
  это чистая codegen-оптимизация поверх готового API, семантику не меняет. **Не в 210.**

> Решение: **Option R′** (R + Call/priv-ревизия). «Нулевая копия» в контракте = про payload'ы
> файлов; таблица — пренебрежимые метаданные. Owner-развилка §8-Q1 фиксирует это (рекомендация R,
> альтернатива N); Call/priv-ревизия внесена по ревью 2026-07-16 (владелец: «внеси правки»).

---

## 4. Сигнатуры

### 4.1 std-тип (Ф.1) — `std/src/prelude/embed.nv` (новый sub-prelude, re-export через facade)

```nova
module prelude.embed

/// Одна встроенная запись: относительный POSIX-путь + байты файла (zero-copy
/// вид над static rodata). Конструируется ТОЛЬКО компилятором (`embed_dir`).
#stable(since = "0.1")
export type EmbeddedEntry {
    path str      // относительный POSIX-путь от embed-корня, case-sensitive
    data []u8     // содержимое файла, нулевая копия (вид над .rodata)
}

/// Иммутабельная встроенная папка: карта путь→байты. Записи отсортированы по
/// `path` (инвариант охраняет `EmbeddedDir.new` + priv-поле — §9). Нулевая копия payload'ов.
#stable(since = "0.1")
export type EmbeddedDir {
    priv entries []EmbeddedEntry   // ОТСОРТИРОВАНЫ по path (бинарный поиск); priv (D281) —
                                   // извне только через EmbeddedDir.new → инвариант ненарушим
}

/// Единственный публичный конструктор (его же синтезирует `embed_dir` — §3).
/// Конвенция «конструкторы = Type.new» (прецедент Vec.new; §1а: from — пятая дверь).
/// Требует отсортированность по `path` (UTF-8 bytewise == порядок `str.compare`);
/// verify за O(N), нарушение → panic (честная ошибка, не тихий None в get).
/// Хранит ЗАЩИТНУЮ мелкую копию входа (loop-push тех же указателей на записи, НЕ
/// `.clone()` — тот требует `T Clone`-bound, ненужный EmbeddedEntry; ревью-3) —
/// пост-конструкционная мутация исходного вектора вызывающим не ломает инвариант
/// (сами записи иммутабельны: поля EmbeddedEntry объявлены без `mut`).
/// Публичность НАМЕРЕННА: легально строить СВОЙ (не встроенный) каталог в
/// тестах/моках — инвариант всё равно охраняется verify.
export fn EmbeddedDir.new(entries []EmbeddedEntry) -> Self {
    mut i = 1
    while i < entries.len() {
        if entries[i - 1].path.compare(entries[i].path) >= 0 {
            panic("EmbeddedDir.new: entries must be sorted by path, unique")
        }
        i = i + 1
    }
    mut own []EmbeddedEntry = []        // защитная мелкая копия (алиас-защита, ревью-2/3)
    for e in entries { own.push(e) }
    Self { entries: own }
}

/// Число встроенных файлов.
export fn EmbeddedDir @len() -> int => @entries.len()

/// Все встроенные пути (в детерминированном отсортированном порядке).
export fn EmbeddedDir @paths() -> []str {
    mut ps []str = []
    for e in @entries { ps.push(e.path) }
    ps
}

/// Есть ли файл по пути.
export fn EmbeddedDir @has(path str) -> bool => @get(path).is_some()

/// Итерация пар (path, data) без двойного lookup — явный property-read
/// priv-поля (D117-прецедент `Vec @ptr()`). Возврат `ro` — read-only view
/// (L2, прецедент `str @bytes() -> ro []u8`): мутация результата = compile
/// error → инвариант не разъедается через выходной алиас.
export fn EmbeddedDir @entries() -> ro []EmbeddedEntry => @entries

/// Байты файла по пути, `None` если нет. Бинарный поиск по отсортированным
/// entries (O(log N)); опирается на sorted-инвариант (§9). Ключ — точная
/// байтовая форма (§2д): без ведущего `./`, `..` не нормализуется —
/// `get("./app.js")` честно даёт None. Данные — вид над .rodata: НЕ
/// мутировать (см. [M-d412-blob-view-mut-write], унаследовано от D412);
/// для мутации — `.clone()`.
export fn EmbeddedDir @get(path str) -> Option[[]u8] {
    mut lo = 0
    mut hi = @entries.len()
    while lo < hi {
        ro mid = lo + (hi - lo) / 2
        ro e = @entries[mid]                // одно чтение на итерацию
        ro c = e.path.compare(path)         // str.compare (D178), <0/0/>0
        if c == 0 { return Some(e.data) }
        if c < 0 { lo = mid + 1 } else { hi = mid }
    }
    None
}
```

> **Точная форма методов** сверена по `std/src/collections/deque.nv` (record + `@field`-property +
> `fn Type @method() -> …`), `str.compare` — по prelude re-export (`std.runtime.string.{…, compare}`).
> **Q3 пересмотрен ревью 2026-07-16:** `entries_of()` снят — с `priv`-полем публичный аксессор =
> явный `fn EmbeddedDir @entries()` (по D117-амендменту/D84/D409 явный аксессор И ЕСТЬ property-read
> поля — прецедент `Vec @ptr()` над `data`); «двух дверей» (property + `_of`-дубль) не возникает (D9).
> Ф.1-тест конструирует через `EmbeddedDir.new` (позитив) + проверяет panic на несортированном
> (негатив) + алиас-защиту (мутация исходного вектора после `new` не меняет `paths()`); co-equal
> `embed_test.nv` в том же модуле при необходимости может читать priv-поле напрямую (D281).

**Прелюд-facade (`std/src/prelude.nv`):** добавить строку
`export import std.prelude.embed.{EmbeddedDir, EmbeddedEntry}` + bump `PRELUDE_VERSION` (→ 18) с
записью в chronology-блоке. Оба типа обязаны быть prelude-видимы: синтезированный `RecordLit`
(§3) вставляется в ПОЛЬЗОВАТЕЛЬСКИЙ код и ссылается на них по имени.

### 4.2 Резолвер (Ф.2) — `compiler-codegen/src/embed_resolve.rs`

Добавить сиблинг `try_replace_embed_dir`, вызываемый из `walk_expr` перед общей рекурсией
(зеркало вызова `try_replace_embed`, стр. 320):

```
fn try_replace_embed_dir(&mut self, e) -> bool:
    # 1. распознать Call Ident("embed_dir") с одним str-литералом
    #    (иначе E_EMBED_ARG_NOT_STR_LITERAL — reuse)
    #    ТОЛЬКО free-Ident-форма: METHOD-позиция `x.embed_dir(...)` НЕ перехватывается
    #    (ревью-3; паритет try_replace_embed). Имя зарезервировано — см. §9
    # 2. base = base_dir(file_id); candidate = base.join(rel); canon = canonicalize()
    #    Err → E_EMBED_DIR_NOT_FOUND ; !is_dir() → E_EMBED_NOT_A_DIR
    # 3. root = root_for(file_id); !canon.starts_with(root) → E_EMBED_OUTSIDE_PROJECT
    # 4. walk рекурсивно (собственный обход, НЕ follow symlinks):
    #      - пропустить записи с именем, начинающимся на '.'  (§2е dot-skip)
    #      - симлинк (файл|папка) → skip + push W_EMBED_DIR_SYMLINK_SKIPPED
    #      - не-ASCII байты в имени → push W_EMBED_DIR_NON_ASCII_PATH (§2е NFD/NFC)
    #      - файл → (rel_posix_path, bytes) ; each file: escape-recheck + push в self.files
    # 5. отсортировать пары по rel_posix_path (UTF-8 bytewise; == порядок str.compare — §9)
    # 6. size/count warning (§2е); пусто после скипов → W_EMBED_DIR_EMPTY (§2е)
    # 7. синтез (R′): e.kind = Call{ Path(EmbeddedDir::new),
    #      args: [ ArrayLit[ RecordLit{EmbeddedEntry, path:StrLit, data:HexBlobLit} … ] ] }
    #      (spans = span вызова embed_dir; НЕ голый RecordLit{EmbeddedDir} — поле priv, §3)
    # 8. return true
```

**Никаких** правок в `emit_c.rs`, `types/mod.rs`, `number_exprs.rs` — синтезированные узлы уже
покрыты (§1, §3). AST-узлы: `RecordLit { type_name: Some(vec!["EmbeddedDir"]), fields, .. }` +
`ArrayLit(Vec<ArrayElem::Item>)` + `RecordLit{EmbeddedEntry}` + `StrLit`/`HexBlobLit` (все —
`ast/mod.rs` 2276/2307).

### 4.3 Коды диагностик

| Код | Класс | Когда |
|---|---|---|
| `E_EMBED_ARG_NOT_STR_LITERAL` | error (reuse) | аргумент не str-литерал / spread / named / арность≠1 |
| `E_EMBED_DIR_NOT_FOUND` | error (**новый**) | путь не резолвится / не существует |
| `E_EMBED_NOT_A_DIR` | error (**новый**) | путь существует, но это файл (подсказать `embed(...)`) |
| `E_EMBED_OUTSIDE_PROJECT` | error (reuse) | папка (или обойдённый файл) вне package-root вызова |
| `E_EMBED_IS_A_DIR` | error (**новый**, ревью R′) | симметрия: `embed("папка")` — путь ведёт на директорию (подсказать `embed_dir(...)`; сейчас падает невнятным read-fail; правка в `try_replace_embed`, та же зона) |
| `W_EMBED_DIR_SYMLINK_SKIPPED` | warning (**новый**) | симлинк пропущен при обходе (перечислить пути) |
| `W_EMBED_DIR_LARGE` | warning (**новый**) | суммарно > 64 MiB или > 4096 файлов |
| `W_EMBED_DIR_EMPTY` | warning (**новый**, ревью R′) | папка существует, но после dot/symlink-скипа встраивать нечего («навёлся не туда») |
| `W_EMBED_DIR_NON_ASCII_PATH` | warning (**новый**, ревью R′) | не-ASCII имя файла — непортируемый байтовый ключ (macOS NFD vs NFC; §2е) |

---

## 5. Улучшения против исходного плана + что подсказал survey

| Улучшение | Было в исходном 210 | Стало | Причина |
|---|---|---|---|
| **Zero-emit_c материализация** | «Ф.2 (б) emit_c: эмитить N блобов + таблицу + сконструировать EmbeddedDir» | Option R: переписать в `RecordLit`/`HexBlobLit` → `emit_c` НЕ трогается | Разведка §1: `HexBlobLit`-арм уже даёт zero-copy static; синтез литерала переиспользует её. Сильно режет риск/объём Ф.2. |
| **Скип dot-файлов** | не оговорено | по умолчанию скрытые (`.`-префикс) не встраиваются | Go-эталон исключает `.`/`_`-префиксные; берём `.` (не `_` — в Nova не спец). Ловит `.git`/`.DS_Store`/dotfiles. |
| **Скип симлинков + warning** | «символ-линки — НЕ следовать (safety)» — только скип | скип + `W_EMBED_DIR_SYMLINK_SKIPPED` (прозрачность) | Тихий скип удивляет; warning честнее. Закрывает escape-через-линк и циклы. |
| **Size/count guard** | «лимиты (размер суммарный/на файл; warning/error)» — размыто | мягкий `W_EMBED_DIR_LARGE` (>64 MiB / >4096); БЕЗ hard-error | Крупный embed легален (Go не лимитирует); warning ловит «не та папка». |
| **Бинарный поиск** | «линейный/бинарный поиск/perfect-hash?» — открыто | бинарный (Go-parity) + sorted-инвариант в спеке | O(log N); resolver уже сортирует. |
| **Код `E_EMBED_NOT_A_DIR`** | только `E_EMBED_DIR_NOT_FOUND` | +`E_EMBED_NOT_A_DIR` с подсказкой `embed()` | «указал файл вместо папки» — частая ошибка, дружелюбная диагностика. |
| **dev-режим — явный REJECT** | не обсуждалось | отклонён с обоснованием (§2л) | rust-embed debug=disk противоречит «один бинарь» + вводит эффект. Зафиксировано, чтобы не всплывало. |
| **`entries()`/`has()` аксессоры** (ревиз. R′: явный `@entries()`, не `entries_of`) | get/paths/len | +has, +итерация пар | `for p in paths()` затем `get(p)` = двойной поиск; пары дают дешёвую итерацию (Go `ReadDir`-эргономика). |
| **Sorted-инвариант защищён** (ревью R′) | публичное поле `entries` + голый `RecordLit`-синтез | `priv entries` + ктор `EmbeddedDir.new` (O(N) verify, panic) + синтез `Call` | Несортированная ручная конструкция давала бы ТИХИЙ None в `get()`; Call не нарушает priv → 0 правок emit_c сохранены. |

**Survey-сверка (что взяли / что нет):**
- **Go `embed.FS`** (эталон): взяли рекурсию, сортировку, бинарный поиск, dot-skip, POSIX-пути,
  case-sensitive. НЕ взяли: виртуальный `fs.FS`-интерфейс (у Nova нет `io/fs` абстракции — избыточно
  для «раздать байты»); `all:`-префикс (glob — future).
- **rust-embed** (`#[derive(RustEmbed)]` + debug=disk/release=embed): взяли `.get(path)->Option`.
  НЕ взяли dev-режим (§2л).
- **Zig/C23/.NET/Java**: single-file + сторонний макрос / «ресурсы в архиве» — модель Nova (компайл-
  тайм в бинарь) ближе к Go; ничего дополнительного не заимствуем.

---

## 6. Карта исполнения (фазы · модели · гейты · файлы)

> Модели по [feedback-cheap-models]: **sonnet** — спека + резолвер + std-тип (исполнение по этой
> карте); **haiku** — механика фикстур по образцу D412. Каждая фаза = свой worktree; суб-агентов НЕ
> спавнить; ФОНОВЫХ прогонов нет (синхронно); checkpoint прогресса в файл (rate-limit); греп
> конфликт-маркеров ОДНОЙ командой с коммитом; `git add` по именам; без Co-Authored-By.

**Ф.0 — Спека (sonnet).** D412-амендмент (§9) в `spec/decisions/03-syntax.md`: форма `embed_dir`,
контракт `EmbeddedDir` (get/paths/len/has + sorted-инвариант), детерминизм, dot-skip, symlink-skip,
size-warning, коды ошибок. **Гейт:** owner sign-off (язык-меняющее); едет В ТОМ ЖЕ слиянии, что Ф.2.

**Ф.1 — std-тип (sonnet).** `std/src/prelude/embed.nv` (§4.1): `EmbeddedDir` + `EmbeddedEntry` +
методы (Nova-body, `str.compare`-бинарный поиск). Re-export в `std/src/prelude.nv` + bump
`PRELUDE_VERSION`→18. Тест рядом: `std/src/prelude/embed_test.nv` (сконструировать `EmbeddedDir`
вручную из 3 отсортированных `EmbeddedEntry`, проверить get/paths/len/has/бинарный-поиск на
границах). **Гейт:** `nova check std` δ-нейтрально (кроме нового файла); юнит-тест зелёный.

> Ф.1 аддитивна и **не зависит** от компилятора — можно делать параллельно Ф.0. Ручная
> конструкция `EmbeddedDir{entries:[…]}` в тесте доказывает тип ДО того, как резолвер умеет его
> синтезировать.

**Ф.2 — резолвер (sonnet, компилятор-ядро).** `compiler-codegen/src/embed_resolve.rs` (§4.2):
`try_replace_embed_dir` — обход папки (рекурсия, dot-skip, symlink-skip+warn, size-warn, границы,
сорт) + синтез `RecordLit`. Проверить, что `walk_expr` (стр. 319–555) зовёт новый распознаватель.
Возвращаемый `Vec<PathBuf>` пополняется всеми файлами папки (fingerprint). **NO emit_c changes.**
**Гейт:** фикстуры Ф.4 точечно зелёные; `embed_dir` на 3-файловой папке даёт байт-верные `get`.

**Ф.3 — флагман-потребитель (sonnet, опционально — НЕ блокер).**
`examples/flagship/aggregator`: заменить `embed("../frontend/index.html")`
(`src/main.nv:128`) на `embed_dir("../frontend")` + раздать через mux по `get(path)` (даже если
файл пока один — как демонстрация; либо разнести ассеты `index.html`+`app.js`+`style.css`).
**Гейт:** пример собирается `--strict-effects` + обслуживает; авторитетный флагман-гейт делает
оркестратор при вливании (test-conventions: conformance app-регрессии не ловит).

**Ф.4 — тесты (haiku).** Фикстуры по образцу D412 (folder-module §116):
- **pos** (`d412d_embed_dir.nv` + папка `d412d_dir/` из 3 файлов + `.hidden` для dot-skip):
  `embed_dir` → `get` каждого → байты совпадают; `paths()` == отсортированные 3 (без `.hidden`);
  `len()==3`; `has("x")` true/false; `get("нет")==None`; `get("./a.txt")==None` (ключ без `./`, §4.1).
- **neg**: `neg/d412d_dir_not_found_neg.nv` (`E_EMBED_DIR_NOT_FOUND`); `neg/d412d_not_a_dir_neg.nv`
  (указан файл → `E_EMBED_NOT_A_DIR`); `neg/d412d_dir_escape_neg.nv` (`../..` → `E_EMBED_OUTSIDE_PROJECT`);
  `neg/d412d_dir_not_literal_neg.nv` (`E_EMBED_ARG_NOT_STR_LITERAL`); `neg/d412d_embed_on_dir_neg.nv`
  (`embed("папка")` → `E_EMBED_IS_A_DIR`, симметрия R′).
- **runtime-neg (Ф.1-тест, не фикстура):** `EmbeddedDir.new` на несортированном → panic (verify-инвариант R′).
- **edge**: пустая папка → `len()==0` + `W_EMBED_DIR_EMPTY` в выводе сборки.
**Гейт:** точечно зелёные; порядок в `paths()` детерминирован (два прогона — один вывод).

**Порядок:** Ф.0 ∥ Ф.1 → Ф.2 → Ф.4 → (Ф.3 опц.). Ф.0 и Ф.2 сливаются ВМЕСТЕ (язык-меняющее).

**Оценка объёма:** Ф.1 ≈ 90 строк .nv + тест. Ф.2 ≈ 120–160 строк Rust (обход + синтез, без
emit_c). Ф.4 ≈ 6 фикстур + бинарные файлы. Спека ≈ +40 строк. Итого малый-средний; риск низкий
(0 правок в emit_c/чекере).

---

## 7. Гейты приёмки (весь план) + риски

**Гейты:**
1. D412-амендмент (§9) в спеке В ТОМ ЖЕ слиянии, что код (Ф.0 ↔ Ф.2).
2. Точечные фикстуры Ф.4 зелёные; `nova check std` δ-нейтрально; юнит-тест Ф.1 зелёный.
3. **Нулевая копия payload'ов** проверена: спот-грепом `.c` — байты файлов в `static const uint8_t
   nova_blob_*[]`, `data`-поля = `nova_blob_view(...)` (не memcpy). Детерминизм: два билда → идентичный
   порядок записей в `.c`.
4. conformance один CU δ0 — **авторитетный полный гейт делает ОРКЕСТРАТОР** при вливании
   (исполнителю — CPU-дисциплина, мега-CU не гонять). Для флагман-затрагивающего Ф.3 —
   `--strict-effects` на `examples/flagship/aggregator` (test-conventions, прецедент 206).
5. Маркер `[M-embed-dir]` снят из backlog + лог в `simplifications.md`.

**Риски / митигации:**

| Риск | Митигация |
|---|---|
| Синтезированный `Call`/`RecordLit` не типизируется (EmbeddedDir/EmbeddedDir.new не видны на type-check) | Ф.1 делает типы+ктор prelude-видимыми ДО Ф.2; Ф.1-тест конструирует через `EmbeddedDir.new` → доказывает видимость независимо от резолвера |
| ~~Пользовательский несортированный `EmbeddedDir` ломает `get()` тихим None~~ | **СНЯТ (R′):** поле `priv` + единственный ктор `EmbeddedDir.new` с O(N)-verify → нарушение = panic, не тихий None |
| Synth-узлы с неверными spans → плохие диагностики | Все узлы берут span вызова `embed_dir` (зеркало `HexBlobLit`-замены, стр. 236) |
| NFD/NFC-расхождение ключей между ОС (не-ASCII имена) | `W_EMBED_DIR_NON_ASCII_PATH` + пин в спеке «байты как есть, без нормализации» (§2е); детерминизм-гейт §7.3 ловит расхождение `.c` |
| Недетерминированный обход ФС между ОС | Явная сортировка пар по POSIX-байтам (§2а'); гейт §7.3 сверяет два билда |
| Большая папка раздувает `.c`/heap | payload'ы в rodata (не heap); `W_EMBED_DIR_LARGE` предупреждает; Option N — future-опт если таблица когда-то станет узкой |
| Симлинк-цикл в папке | Скип всех симлинков (§2е) — обход конечен |
| Большой N → синтезированный `ArrayLit` = одна длинная C-init-функция (компилятор C жуёт, но медленно) | Не блокер: порог `W_EMBED_DIR_LARGE` (>4096 файлов) тот же; исполнителю Ф.2 — ожидаемо, не удивляться (ревью-3); чанкование init — future вместе с Option N |
| Пользовательская `fn embed_dir(...)` молча хайджачится резолвером | Имя зарезервировано (§9, паритет `embed`); METHOD-позиция `x.embed_dir(...)` НЕ перехватывается (§4.2, ревью-3) |
| `embed_dir` в горячем пути пересоздаёт таблицу | Док-рекомендация «биндить один раз» (§3); хойстинг — future |
| Инкрементальность пропустит add-файла | Проверено: обход безусловен и до ключа кэша; set-изменение меняет ключ (§2ж) |
| Флагман-регрессия не ловится conformance | Ф.3-гейт под `--strict-effects` у оркестратора (test-conventions) |

---

## 8. Вопросы владельцу (минимум — рекомендация + альтернатива)

- **Q1 — Материализация таблицы.** Рекомендую **Option R** (переписать в `RecordLit`, 0 правок
  emit_c; payload'ы zero-copy static, таблица = O(N) one-time heap). Альтернатива — **Option N**
  (полностью-static таблица, но новый AST-узел + emit_c-арм, больше кода). *Рекомендация R;
  «нулевая копия» в контракте трактуется как «payload'ы файлов не дублируются».* → §3.
- **Q2 — dot-skip по умолчанию.** Рекомендую **скипать скрытые (`.`-префикс) файлы/папки** по
  умолчанию (Go-эталон, ловит `.git`/`.DS_Store`). Это тихо исключает часть файлов — потому выношу.
  Альтернатива — встраивать ВСЁ (тогда `.git` в ассет-папке попадёт в бинарь). *Рекомендация: dot-skip;
  opt-in «включить скрытые» — future-флаг.* → §2е.
- **Q3 — имя аксессора пар. ПЕРЕСМОТРЕН ревью 2026-07-16 (снят):** с `priv entries` (R′) публичный
  аксессор = явный `fn EmbeddedDir @entries()` — по D117-амендменту явный аксессор и есть property-read
  поля (прецедент `Vec @ptr()`); `entries_of()` не нужен, «двух дверей» нет (D9). → §4.1.

*(Если owner молчит по §8 — вести по рекомендациям R′ / dot-skip / явный `@entries()`; это дефолт
задания «работай/твоё решение».)*

---

## 9. Черновик D412-амендмента (готов к вставке в `spec/decisions/03-syntax.md` после D412)

```markdown
### D412-амендмент (2026-07-16, Plan 210): `embed_dir("dir")` — встраивание папки

**Статус: ПРИНЯТО** (владелец 2026-07-15, вариант A; материализация Option R). Расширяет п.2
(`embed`) на целую директорию. Маркер [M-embed-dir].

3. **`embed_dir("relative/dir")`** — компайл-тайм интринсик: содержимое ВСЕЙ папки (рекурсивно)
   как иммутабельный `EmbeddedDir`.
   - Аргумент — ТОЛЬКО строковый литерал (E_EMBED_ARG_NOT_STR_LITERAL). Путь резолвится
     относительно .nv-файла вызова (модель `embed`); граница — package-root вызова (Plan 193
     gap-2). Выход наружу (папка ИЛИ любой обойдённый файл) — E_EMBED_OUTSIDE_PROJECT.
   - Папка не найдена — E_EMBED_DIR_NOT_FOUND; путь ведёт на файл — E_EMBED_NOT_A_DIR (используй
     `embed`). Пустая папка → пустой `EmbeddedDir` (легально).
   - **Обход:** рекурсивный. **Скрытые** записи (имя с `.`) — пропускаются. **Символические
     ссылки** (файлы и папки) — НЕ следуются, пропускаются с предупреждением
     W_EMBED_DIR_SYMLINK_SKIPPED (защита от escape и циклов). Предупреждение W_EMBED_DIR_LARGE при
     суммарном размере > 64 MiB или > 4096 файлов (совет, не ошибка). Папка существует, но после
     скипов пуста → W_EMBED_DIR_EMPTY. Не-ASCII имя файла → W_EMBED_DIR_NON_ASCII_PATH
     (непортируемый ключ: macOS хранит имена в NFD, Windows/Linux — обычно NFC).
   - Симметрия: `embed("папка")` (одиночный embed на директорию) — E_EMBED_IS_A_DIR (используй
     `embed_dir`).
   - **Имя `embed_dir` зарезервировано** (паритет `embed`): распознаётся резолвером до type-check
     в free-Ident-форме вызова; пользовательская функция с этим именем не поддерживается.
     METHOD-позиция (`x.embed_dir(...)`) интринсиком НЕ является и не перехватывается.
   - **Ключ** записи = путь относительно embed-корня, разделитель POSIX `/`, case-sensitive, без
     ведущего `./`; **байты имени как есть, БЕЗ Unicode-нормализации**. `get` не нормализует
     аргумент (`get("./x")` → None). **Записи отсортированы** по ключу (UTF-8 байтовый порядок ==
     порядок `str.compare` — предпосылка корректности бинарного поиска) — воспроизводимый `.c`.
   - **Fingerprint:** все обойдённые файлы — зависимости сборки (add/rm/change → пересборка).
   - Glob-фильтр, mime/Content-Type, сжатие — вне объёма (future).

**Тип `EmbeddedDir` (prelude):**
- `EmbeddedEntry { path str, data []u8 }` — путь + байты (нулевая копия, вид над .rodata);
  публично конструируем (инвариантов не несёт).
- `EmbeddedDir { priv entries []EmbeddedEntry }` — записи ОТСОРТИРОВАНЫ по `path`; поле priv —
  инвариант охраняет ЕДИНСТВЕННЫЙ публичный конструктор `EmbeddedDir.new(entries)` (защитная копия входа)
  (O(N) verify-sorted; нарушение → panic, не тихий промах поиска).
- API: `@get(path str) -> Option[[]u8]` (бинарный поиск, O(log N)), `@paths() -> []str`
  (отсортированы), `@len() -> int`, `@has(path str) -> bool`, `@entries() -> []EmbeddedEntry`
  (явный property-read priv-поля, прецедент `Vec @ptr()`). Иммутабелен (нет мутирующих методов).

**Материализация (Option R′):** `embed_dir("dir")` переписывается компилятором (пасс
`embed_resolve`, ДО type-check) в вызов `EmbeddedDir.new([EmbeddedEntry{path: "…",
data: x"…"}, …])` (отсортировано резолвером; verify конструктора пробегает даром). Каждый `data` —
hex-блоб (п.1/D412) → zero-copy вид над `static const uint8_t nova_blob_<h>[]` (.rodata); пути —
interned static-строки. Таблица `entries` строится один раз при вычислении выражения (совет:
биндить `embed_dir` единожды, не в горячем пути). Никаких новых codegen-примитивов: payload'ы
наследуют модель материализации D412.

**Прецеденты:** Go `//go:embed dir` + `embed.FS` (эталон: рекурсия, сортировка, бинарный поиск,
исключение скрытых, POSIX-пути), Rust `rust-embed`/`include_dir!`.
```

---

## 9.1 Ревью-2 (Fable, 2026-07-16) — правки к исполнению (R″)

1. **Мутабельность наружу (КРИТИЧНО):** (а) `@entries()` НЕ должен отдавать живую мутабельную
   ссылку на priv-Vec (`dir.entries().push(...)` ломал бы sorted-инвариант мимо `EmbeddedDir.new`).
   **Согласовано (два ревью сошлись):** primary = `-> ro []EmbeddedEntry` (L2 read-only view,
   прецедент `str @bytes() -> ro []u8` — zero-copy + compile-error на мутацию; уже в §4.1);
   fallback (если на Ф.1 выяснится, что ro-return НЕ энфорсит L2 для Vec-поля записи) = вернуть
   **КОПИЮ** (`mut out = []; for e in @entries { out.push(e) } out`). Исполнителю Ф.1: проверить
   энфорс ro-return тестом (mut-биндинг результата + push → ждём compile-error);
   (б) `get()`-view над .rodata: mut-биндинг результата + in-place запись (`d[0]=5`) = SEGV
   (D412-копия на 26599 ловит только биндинг блоб-ЛИТЕРАЛА, не значения из функции). Спек-пин:
   «`data` — иммутабельный вид; in-place запись = краш; модификация — только явной копией».
   Общий D412-хазард (есть и у одиночного embed) — отдельный маркер
   `[M-d412-blob-view-mut-write]` (проверить/закрыть чекером или рантаймом; вне 210).
2. **Не-ASCII фикстура (Ф.4+):** pos-фикстура с не-ASCII именем файла (ожидаемый
   W_EMBED_DIR_NON_ASCII_PATH; `get()` НАХОДИТ) — тестирует предпосылку «байтовая сортировка ==
   str.compare» (бинарный поиск).
3. **Backslash в аргументе:** `E_EMBED_PATH_BACKSLASH` для `embed`/`embed_dir` (аргумент с `\` —
   непортируемый исходник; Go-прецедент запрета) — одно условие в резолвере.
4. **Совет по биндингу:** до закрытия `[M-codegen-emission-nondeterminism]`(c) (static-init
   topological order) рекомендация в доке/спеке = «биндить в `main()`», НЕ на уровне модуля.
5. **Проверка исполнителю:** warning-канал из `embed_resolve` (пасс до type-check) — убедиться,
   что W_-коды реально доносятся (сейчас пасс эмитит только E_).
6. **Спека §9:** явно дописать «пути уникальны» в инвариант (verify `>= 0` уже отвергает дубли).

## 9.2 Ревью-3 (Fable, 2026-07-16, третий проход) — физика эмиссии и стыки планов

1. **🔴 Взрыв `.c` ×5.3 + стык с Планом 209 (проверено по коду):** блоб рендерится `0x%02X,`
   (render_interned_blob_literals) = ~5.3 байта текста/байт данных.
   (а) Порог `W_EMBED_DIR_LARGE` **64 MiB → 16 MiB** (64 MiB = ~340 МБ `.c`, clang умрёт раньше);
   текст варнинга объясняет множитель.
   (б) **209-стык:** блоб-статики идут в ПРОЛОГ → в multi-TU это `_common.h` → каждый part
   перекомпилирует массив N раз (анти-цель 209); плюс блоб — неделимый юнит для split_tu.
   Совместное требование 209×210: определения блобов — в ОДИН part, в common — только extern;
   тест «большой блоб под NOVA_MULTI_TU=1». → отразить и в 209 Ф.5.
   (в) **Option E (future, настоящий фикс):** C23 `#embed` (clang ≥19; WSL-clang 21 есть,
   windows-clang проверить; msvc — hex-fallback) — `.c` крошечный, компиляция мгновенная.
2. **CRLF/autocrlf:** текстовые ассеты на Windows-чекауте (autocrlf) байтово отличаются от
   Linux → разный `.c`/fingerprint между ОС; гейт §7.3 (same-machine) не ловит. Спек/док-строка:
   «байты как в рабочей копии; для кросс-ОС воспроизводимости — `-text` в .gitattributes на
   asset-папки».
3. **`embed_dir(".")` / `""`:** самовстраивание корня пакета (вкл. src/*.nv, nova.toml) — почти
   наверняка ошибка. Рекомендация: `E_EMBED_DIR_IS_PACKAGE_ROOT`; альтернатива — узаконить явной
   спек-строкой (решение владельца; дефолт — запретить).
4. **Верифицировано (сомнение снято):** `-> ro []u8` существует (`str @bytes()`, core.nv:79) —
   R″-механизм `@entries() -> ro []EmbeddedEntry` валиден.

## Ф.6(б) — дизайн: ReadFs (VFS-унификация à la Go embed.FS/fs.FS)

> **Статус: ДИЗАЙН (opus, 2026-07-16) → ✅ РЕАЛИЗОВАНО (sonnet, 2026-07-16, см. шапку
> плана).** Развилка владельца из шапки Ф.6(б):
> read-only протокол чтения (`ReadFs`), реализуемый и обычной ФС, и `EmbeddedDir`;
> главный кейс — статика веб-сервера «dev = с диска (live-reload), prod = embedded»
> одним кодом. **Всё сверено по коду/спеке (file:line ниже); синтаксис не выдуман.**
> Реализация **аддитивная и НЕ язык-меняющая** (см. 6б.5) — ниже ceremony, чем Ф.0-Ф.2.
> Ниже — дизайн-документ КАК СПРОЕКТИРОВАН (сохранён для истории); фактическая
> реализация — `std/src/fs/readfs.nv`/`readfs_test.nv`, R1 подтверждён extension-путём
> (§6б.7 R1 — ЗЕЛЁНЫЙ, wrapper `EmbeddedFs` не понадобился).

### 6б.0. Итог разведки (что уже есть — цитаты)

| Факт | Источник | Значение для дизайна |
|---|---|---|
| **io-протоколы эффект-агностичны**: conformer несёт СВОЙ plumbing-эффект (`File`→`Fs`, `TcpStream`→`Net`, console→`Io`), который **всплывает транзитивно при mono** (Q15, D122 amended). Generic через io-bound — **mono-dispatch only, vtable для effectful bounds НЕТ**. | `std/src/io/core.nv:4-9`; декл. `type Read protocol { mut @read(buf mut []u8) -> Result[int, IoError] }` — **без эффекта** — `core.nv:43-45` | `ReadFs` копирует ЭТУ модель: методы протокола БЕЗ эффекта; эффект даёт конкретный impl. Развилка «эффект embedded-чтение чистое / fs-чтение эффектное» — **уже решена прецедентом** (6б.1). |
| **Чистый conformer** `BytesReader` (курсор над `[]u8`, in-memory, `mut @read(...)->Result[int,IoError]` без эффекта) конформит `io.Read` рядом с эффектным `File`. | `std/src/io/mem.nv:5,42`; `File @read ... Fs -> Result[int,IoError]` — `std/src/fs/fs.nv:207` | Прямой прецедент: `EmbeddedDir` (чистый) и `DirFs` (Fs) под ОДНИМ `ReadFs`. Чистый impl эффект-агностичного протокола — доказанно легален. |
| **Effectful vtable dispatch НЕ поддержан**: «true-vtable dispatch (Plan 03) не пробрасывает effect-handlers через vtable-ABI — в truly-erased контексте effectful-protocol bounds ОБЯЗАНЫ mono-dispatch'иться». | `spec/decisions/02-types.md:4047-4056` (D122-амендмент) | **dyn-значение `ReadFs` для эффектной (DirFs) ветки невозможно в V1** → dev/prod-выбор = ветка на СТАРТЕ над двумя mono-инстансами, не одна runtime-переменная (6б.4). |
| **Nova structural, orphan rule НЕТ**; «методы едут с типом» (D286); extension-метод на ЧУЖОМ типе легален, требует explicit `import` (D287); «Nova не ограничивает добавление методов на тип из любого модуля». | `spec/decisions/02-types.md:3865, 9957`; `spec/decisions/07-modules.md:723-740` | `@read_file`/`@try_exists` на `EmbeddedDir` можно объявить **в `std.fs` рядом с протоколом** (extension-методы) → prelude НЕ зависит от std.fs (6б.3). Прецедент размещения conformance-surface у типа-конформера — `TcpStream @read` в `std/src/net/tcp.nv:138`. |
| **Одна ошибка** `IoError { ro kind ErrorKind, ro raw_os int, ro op str }` для io/fs/os; `ErrorKind` enum с `NotFound`/`PermissionDenied`/… ; ктор `IoError.new(kind, op)` / `IoError.from_os(raw_os, op)`. **Типа `FsError` НЕ существует** — рабочее имя из задания = `IoError`. | `std/src/io/error.nv:57,68,74`; `docs/guide/io-fs.md:44-52` | Протокол возвращает `Result[[]u8, IoError]` (НЕ выдуманный `FsError`). |
| **fs-API чтения**: `fn read(path Path) Fs -> Result[[]u8, IoError]` (открыть→до EOF→всегда close); `canonicalize(path) Fs -> Result[Path, IoError]` (realpath, резолвит симлинки); `try_exists(path) Fs -> Result[bool, IoError]`. Имя `exists` — **зарезервированный квантор**, поэтому free-fn зовётся `try_exists`. | `std/src/fs/fs.nv:380,574,468` (+ коммент про reserved `exists` `fs.nv:463-467`) | Протокол-метод НЕ может зваться `@exists` (reserved) → `@try_exists` (паритет free-fn). DirFs переиспользует `read`/`canonicalize`/`try_exists` как есть. |
| **Path-API (чистый, без I/O)**: `Path.from_str`/`.posix`/`.from_bytes`; `@join_path`; `@normalize()` (лексически схлопывает `.`, резолвит `..`, никогда выше корня; относительный путь МОЖЕТ сохранить ведущий `..`); `@is_absolute`; `@components() -> []str`; `@os_bytes`; `@to_str`. Симлинк-резолв — только `Fs.realpath`/`canonicalize`. `str.starts_with` есть. | `std/src/fs/path.nv:85,364,404,235,346,107,116`; `str.starts_with` — `std/src/prelude/errors.nv:242` | DirFs escape-защита = лексический `normalize` + symlink-hard `canonicalize`+prefix (6б.3). |

### 6б.1. Эффект-развилка — РЕШЕНА прецедентом (главный вывод)

Задание ставит два вопроса: (i) subsumption — может ли impl иметь МЕНЬШЕ эффектов, чем
декларация метода протокола; (ii) как сочетать «чистое embedded-чтение» и «эффектное
fs-чтение» под одним протоколом.

**Ответ снимает вопрос (i) целиком:** `ReadFs` объявляется **эффект-агностично** — методы
протокола **без аннотации эффекта**, ровно как `io.Read`/`io.Write` (`core.nv:43-54`). Тогда:
- **чистый `EmbeddedDir`-impl легален** — pure ⊆ {любой набор}; прецедент = `BytesReader`
  (`mem.nv:42`) чистый под тем же протоколом, что эффектный `File`;
- **`DirFs`-impl добавляет `Fs`** — этот эффект НЕ объявлен в протоколе, а **всплывает при
  mono** конкретной инстанциации (модель Q15/D122; `core.nv:6-9`).

Мы **никогда не объявляем `Fs` в сигнатуре протокола**, поэтому ситуации «impl имеет МЕНЬШЕ
эффектов, чем декларация» просто не возникает — subsumption не нужен. Нового D-блока про
эффект-полиморфизм протоколов **НЕ требуется**: механика (structural conformance +
mono-dispatch с транзитивным всплытием эффекта конкретного impl) уже работает и покрыта
тестами `io.Read`/`io.Write` (`std/src/io/d322_*_test.nv`).

**Единственная цена** — следствие того же прецедента: generic через effectful-bound —
mono-dispatch only, **vtable для effectful bounds нет** (`core.nv:8-9`; `02-types.md:4052-4056`).
Значит нельзя хранить dev/prod-выбор в ОДНОЙ runtime-переменной типа `ReadFs` (existential/dyn)
— для эффектной `DirFs`-ветки это потребовало бы effectful-vtable-dispatch (future Plan 03).
Выбор делается **веткой на старте** над двумя mono-инстансами (6б.4) — эргономически дёшево.

**Сравнение путей эффект-развилки:**

| Путь | Поддержан сейчас | Новый D-блок | dev/prod в одной переменной | Вердикт |
|---|---|---|---|---|
| **A. Эффект-агностичный протокол + mono (io.Read-модель)** | ✅ да (Q15/D122, тесты io) | нет | нет (ветка на старте) | **РЕКОМЕНДОВАНО** |
| B. Протокол объявляет `Fs`, EmbeddedDir «омывает» его до pure (subsumption вниз) | ⚠️ не специфицировано; чистый vtable-эффект — future | да (правило subsumption эффектов) | нет (та же vtable-стена) | Лишний D-блок ради худшего |
| C. Effect-polymorphic протокол (`[E] ReadFs<E>`) | ❌ rank-2 effect polymorphism — future (`06-concurrency.md:2098,2139`) | да, крупный | теоретически | Вне горизонта V1 |
| D. Две ветви кода без протокола (dev-fn + prod-fn) | ✅ да | нет | — | Дублирование логики сервера; протокол убирает дубль |

Путь A даёт единый код сервера (генерик по `[F ReadFs]`) при нулевом языковом расширении.

### 6б.2. Протокол `ReadFs` — сигнатуры дословно

**Размещение:** новый файл `std/src/fs/readfs.nv`, `module std.fs` (co-equal с `fs.nv`/
`effect.nv`; `import std.io.{IoError, ErrorKind}` — тот же паттерн, что `fs.nv:24`). НЕ в
prelude — иначе prelude потянул бы std.io/std.fs.

```nova
// std/fs/readfs.nv — read-only VFS-протокол над реальной ФС и встроенной папкой.
module std.fs

import std.io.{IoError, ErrorKind}

/// Read-only виртуальная ФС: карта относительный-POSIX-путь → байты. Эффект-
/// АГНОСТИЧЕН (модель io.Read, core.nv:4-9): конформер несёт свой эффект
/// (`DirFs`→`Fs`, `EmbeddedDir`→чистый), всплывающий при mono. Диспатч — только
/// mono (vtable для effectful-bound нет). Ключ — POSIX `/`, case-sensitive, без
/// ведущего `./` (общая конвенция с `embed_dir`, §2д).
#stable(since = "0.1")
export type ReadFs protocol {
    /// Байты файла по пути. `Err(IoError{NotFound})` — файла нет; прочие `Err` —
    /// реальный сбой I/O (только у эффектных impl). Чистый impl инфаллибелен,
    /// кроме NotFound.
    @read_file(path str) -> Result[[]u8, IoError]

    /// Существует ли путь. `Ok(true/false)`; `Err` — не-NotFound сбой (эффектные
    /// impl). Имя `@try_exists` (не `@exists` — reserved-квантор; паритет free-fn
    /// `try_exists`, fs.nv:468).
    @try_exists(path str) -> Result[bool, IoError]
}
```

**Минимум = `read_file` + `try_exists`; обоснование.** Для статик-сервера обработчик делает
`match fs.read_file(key) { Ok(b)=>200; Err(NotFound)=>404; Err(_)=>500 }` — `read_file` УЖЕ
различает 404 через `NotFound`. `try_exists` вторичен (HEAD-запрос / pre-check без чтения тела);
включён по заданию, но 90% кейса закрывает один `read_file`.

**Почему `list`/`paths` — НЕ в протоколе (сверх минимума):**
- у `EmbeddedDir` перечисление дёшево и детерминировано (`@paths()`, всё известно на компиляции);
- у **реальной ФС** — наоборот: (1) рекурсивный обход дерева на КАЖДЫЙ запрос = `Fs` + O(дерева),
  дорого; (2) **недетерминированно** и меняется между вызовами (dev live-reload — файлы
  добавляются/удаляются); (3) обход выводит наружу симлинки/dot-файлы (те же ловушки, что
  закрывает `embed_dir` §2е) — семантика «что считать записью» разъезжается между impl;
- **directory-index (autoindex)** — нишевая фича, не нужна для «раздать статику по точному пути».
- **Вывод:** `list` не в `ReadFs`. Если понадобится — **отдельный протокол** `ListFs {
  @list(prefix str) -> Result[[]str, IoError] }` (эффектный у реальной ФС, чистый у embedded),
  future, вне V1. Дробление на два протокола = не платить обходом там, где нужно только чтение
  (принцип «минимальный протокол» — как раздельные `io.Read`/`io.Write`/`io.Seek`, `core.nv`).

### 6б.3. Impl'ы дословно

**(а) `EmbeddedDir` — extension-методы в `std/src/fs/readfs.nv` (родной Option-API не трогается).**
`EmbeddedDir`/`@get`/`@has` приезжают из prelude (D286, method-table). Extension-методы (D287)
живут рядом с протоколом → prelude чист:

```nova
// EmbeddedDir конформит ReadFs — extension-методы (D287), НЕ правка prelude/embed.nv.
// Родной Option-API (@get/@has/@paths) остаётся; это ДОПОЛНЕНИЕ.
export fn EmbeddedDir @read_file(path str) -> Result[[]u8, IoError] =>
    match @get(path) {                       // @get — inherent (prelude), едет с типом (D286)
        Some(b) => Ok(b)                     // zero-copy view над .rodata (не мутировать!)
        None    => Err(IoError.new(ErrorKind.NotFound, "read_file"))
    }

export fn EmbeddedDir @try_exists(path str) -> Result[bool, IoError] => Ok(@has(path))
```
Чистые (без эффекта) — `EmbeddedDir` конформит `ReadFs` как чистый conformer.

**(б) `DirFs` — обёртка над реальной ФС с корнем-префиксом + защита от escape (новый тип там же).**

```nova
/// Read-only вид на поддерево реальной ФС с корнем `root`. Все чтения ограничены
/// поддеревом `root` (защита от escape — зеркалит решение embed_dir §2и):
/// лексический `..`-escape отвергается, симлинк-escape ловится `canonicalize`.
/// `value`-тип (иммутабелен, дёшев в передаче). Главный кейс — dev-режим
/// (live-reload с диска), тогда как prod = `EmbeddedDir` тем же кодом (6б.4).
#stable(since = "0.1")
export type DirFs value {
    priv root Path
}

/// Корень поддерева (обычно `Path.from_str("./frontend")`). Канонизацию НЕ делаем
/// в кторе — это `Fs`-эффект, а ктор держим чистым; проверка escape — на каждом
/// чтении (root мог не существовать в момент конструирования — dev).
#stable(since = "0.1")
export fn DirFs.new(root Path) -> DirFs => { root }

/// Разрешить ключ запроса в ОГРАНИЧЕННЫЙ корнем абсолютный путь, или Err.
/// Инвариант «результат всегда внутри realpath(root)»:
///   1) лексически: normalize (схлопнуть `.`/`..`); отвергнуть абсолютный и
///      сохранившийся ведущий `..` (path.nv:404,414 — relative может унести `..` вверх);
///   2) симлинк-hard: canonicalize(root) и canonicalize(join) → требуем префикс
///      (симлинк внутри дерева, указывающий наружу, отсекается — realpath его резолвит).
/// canonicalize NotFound на несуществующем → пробрасывается как NotFound (→ 404).
fn DirFs @resolve(path str) Fs -> Result[Path, IoError] {
    ro rel = Path.posix(path).normalize()
    if rel.is_absolute() {
        return Err(IoError.new(ErrorKind.PermissionDenied, "read_file"))
    }
    ro comps = rel.components()
    if comps.len() > 0 && comps[0] == ".." {          // escape над корнем
        return Err(IoError.new(ErrorKind.PermissionDenied, "read_file"))
    }
    ro joined = @root.join_path(rel)
    ro croot = match canonicalize(@root)  { Ok(p) => p, Err(e) => return Err(e) }
    ro cfull = match canonicalize(joined) { Ok(p) => p, Err(e) => return Err(e) }
    // prefix-проверка на границе компонента (не голый str-prefix: "/a/b" vs "/a/bc").
    // Реализация Ф.: сравнить @components() или добавить сепаратор-guard.
    match (croot.to_str(), cfull.to_str()) {
        (Some(r), Some(f)) =>
            if f == r || f.starts_with(r + "/") { Ok(cfull) }
            else { Err(IoError.new(ErrorKind.PermissionDenied, "read_file")) }
        _ => Err(IoError.new(ErrorKind.InvalidData, "read_file"))
    }
}

/// ReadFs: байты файла (эффект `Fs` — всплывает при mono; протокол его НЕ объявляет).
#stable(since = "0.1")
export fn DirFs @read_file(path str) Fs -> Result[[]u8, IoError] {
    match @resolve(path) { Ok(p) => read(p), Err(e) => Err(e) }   // read — fs.nv:380
}

/// ReadFs: существование (NotFound-escape → Ok(false), прочее — Err).
#stable(since = "0.1")
export fn DirFs @try_exists(path str) Fs -> Result[bool, IoError] {
    match @resolve(path) {
        Ok(p)  => try_exists(p)                                    // fs.nv:468
        Err(e) => match e.kind { ErrorKind.NotFound => Ok(false), _ => Err(e) }
    }
}
```

**Инварианты `DirFs` (зеркало escape-решения `embed_dir`, §2и):**
1. **Никакой выход за `realpath(root)`** — лексический `..`-фильтр + symlink-hard prefix.
   `embed_dir` при обходе СКИПАЕТ симлинки на компиляции (§2е); `DirFs` не может «скипнуть» —
   он ОТВЕРГАЕТ escape в рантайме (`PermissionDenied`).
2. **case-sensitive, POSIX-ключ** — общая конвенция с `EmbeddedDir` (§2д): один и тот же `key`
   даёт один результат из обоих impl (dev==prod по путям).
3. **Инфаллибелен по конструкции, эффектен по чтению** — `Fs` только в методах чтения, не в кторе.
4. **read-only** — `DirFs` не пишет; никаких mut-методов.

**Эффекты:** `EmbeddedDir @read_file/@try_exists` — чистые; `DirFs @read_file/@try_exists` — `Fs`.
Протокол `ReadFs` — эффект-агностичен. Всё согласовано с 6б.1.

### 6б.4. Кейс dev/prod — код (генерик по `[F ReadFs]`, НЕ dyn-значение)

Идиоматичная форма (mono-dispatch, поддержано сегодня): сервер-регистратор **генерик**, выбор
dev/prod — ветка на СТАРТЕ, каждая ветвь mono'ит свой инстанс:

```nova
import std.fs.{ReadFs, DirFs}                 // extension-conformance EmbeddedDir виден отсюда (D287)

// Один код раздачи статики поверх любого ReadFs. Мономорфизуется дважды:
// [DirFs] (несёт Fs) и [EmbeddedDir] (чистый) — эффект всплывает per-instance.
fn serve_assets[F ReadFs](mux mut ServeMux, assets F) -> () {
    mux.get("/{path...}", handler_fn(|req| {
        ro key = req.param("path").unwrap_or("index.html")
        match assets.read_file(key) {                       // mono: DirFs→Fs / Embedded→pure
            Ok(bytes) => ServerResponse.bytes(200, mime_of(key), bytes)
            Err(e)    => match e.kind {
                ErrorKind.NotFound => ServerResponse.empty(404)
                _                  => ServerResponse.empty(500)
            }
        }
    }))
}

fn embedded_assets() -> EmbeddedDir => embed_dir("../frontend")   // prod: вшито в бинарь

fn main() {
    with Net = real_net(), Fs = real_fs() {          // Fs присутствует и в prod (безвредно)
        mut mux = ServeMux.new()
        if dev_mode() {
            serve_assets(mut mux, DirFs.new(Path.from_str("./frontend")))   // serve_assets[DirFs]
        } else {
            serve_assets(mut mux, embedded_assets())                        // serve_assets[EmbeddedDir]
        }
        serve(mux, ":8080")
    }
}
```

**Почему НЕ одна переменная `mut assets: ReadFs = if dev {...} else {...}`:** existential-значение
`ReadFs` с эффектным методом (`DirFs.read_file` несёт `Fs`) требует effectful-vtable-dispatch —
**не поддержано** (`02-types.md:4052-4056`). Ветка стоит на ТОЧКЕ ИНСТАНЦИАЦИИ (`if` выбирает,
какой mono-инстанс `serve_assets` запустить), а не в переменной. Цена — один `if` на старте;
тело сервера единое. В prod `with Fs` всё равно связан (для остального сервера) — присутствие
`Fs` в prod-инстансе безвредно: embedded-ветка просто его не использует (диск не трогается,
бинарь самодостаточен).

**Замена во флагмане:** `examples/flagship/aggregator/src/main.nv:128` (`fn frontend_html() ->
[]u8 => embed("../frontend/index.html")`) и `:340` (`mux.get("/", ...)`) переводятся на
`serve_assets(mut mux, embedded_assets())` (карта, не реализация — Ф. ниже).

### 6б.5. Черновик спек-правки (аддитивно, НЕ язык-меняюще)

**Важно: `ReadFs` НЕ вводит нового синтаксиса/семантики** — это ещё один std-протокол поверх
готовой structural-protocol + mono-dispatch машины (та же, что несёт `io.Read`). Поэтому спек-
след **лёгкий**, а слияние **не требует owner-gated D-sign-off** уровня `embed_dir` Ф.0 (то было
язык-меняющим — новый интринсик). Достаточно:

1. **`spec/decisions/04-effects.md`** — короткий амендмент к D322/D323 (io-core/fs), рядом с
   описанием `io.Read`. Черновик:

```markdown
### D323-амендмент (Plan 210 Ф.6б): ReadFs — read-only VFS-протокол

`ReadFs` (`std.fs`) — read-only виртуальная ФС, объединяющая чтение из реальной
ФС (`DirFs`) и из вшитой папки (`EmbeddedDir`) под одним bound. Эффект-АГНОСТИЧЕН
(модель io.Read/D322): методы без аннотации эффекта; конформер несёт свой
(`DirFs`→`Fs`, `EmbeddedDir`→чистый), всплывающий при mono. Диспатч — mono-only
(effectful-vtable-dispatch — future Plan 03), поэтому dev/prod-выбор = ветка над
двумя mono-инстансами, не dyn-значение.
- API: `@read_file(path str) -> Result[[]u8, IoError]`,
        `@try_exists(path str) -> Result[bool, IoError]`. Ключ — POSIX `/`,
  case-sensitive, без ведущего `./` (конвенция embed_dir). `NotFound` = «нет
  файла»; прочие `Err` — реальный I/O-сбой (только эффектные impl).
- `DirFs { priv root Path }` — чтения ограничены `realpath(root)` (лексический
  `..`-фильтр + symlink-hard prefix; зеркалит escape-решение embed_dir).
- `EmbeddedDir` конформит через extension-методы (D287) в std.fs — родной
  Option-API (`@get`/`@has`/`@paths`) не тронут.
- `list`/directory-index — вне ReadFs (реальная ФС: дорого/недетерминированно);
  future отдельный `ListFs`.
```

2. **`docs/guide/io-fs.md`** — в разделе «Protocols vs the text sink» дописать абзац про `ReadFs`
   (read-only VFS, dev/prod-кейс, кросс-ссылка на D412-амендмент этого плана).
3. **D412-амендмент (§9 этого плана)** — одна строка «см. D323-амендмент ReadFs: `EmbeddedDir`
   конформит read-only `ReadFs` (VFS-унификация dev/prod)».

### 6б.6. Карта исполнения (фазы · модели · гейты · файлы · объём)

> Модели по [feedback-cheap-models]: **sonnet** — протокол+DirFs+extension (исполнение по этой
> карте); **haiku** — фикстуры по образцу. Каждая фаза свой worktree; суб-агентов не спавнить;
> синхронно; checkpoint прогресса; греп конфликт-маркеров ОДНОЙ командой с commit; `git add` по
> именам; без Co-Authored-By.

**Ф.6б.1 — протокол + impl'ы (sonnet, std).** `std/src/fs/readfs.nv`: `ReadFs` (6б.2), extension
`EmbeddedDir @read_file/@try_exists` (6б.3а), `DirFs` + `@resolve`/`@read_file`/`@try_exists`
(6б.3б). `import std.io.{IoError, ErrorKind}`. **Гейт:** `nova check std` δ-нейтрально (кроме
нового файла). Объём ≈ 90-120 строк .nv.

**Ф.6б.2 — тест рядом (sonnet/haiku).** `std/src/fs/readfs_test.nv` (module-beside-module,
прецедент `net/d302_neterror_iokind_test.nv`): (1) `EmbeddedDir` (ручной `EmbeddedDir.new` из 2-3
отсортированных `EmbeddedEntry`) конформит `ReadFs` — `read_file` hit/`NotFound`-miss,
`try_exists`; (2) **generic `[F ReadFs]`-функция** зовётся и над `EmbeddedDir`, и над `DirFs`
(mono дважды) — **доказать, что structural conformance ловит extension-методы на generic-bound**
(ключевой риск R1); (3) `DirFs` escape: `read_file("../secret")`→`PermissionDenied`,
`read_file("a.txt")` hit (через `mock_fs()`/`mem_fs()` — детерминизм, без диска). **Гейт:** тест
зелёный; при провале (2) — fallback-wrapper (R1). Объём ≈ 60-90 строк.

**Ф.6б.3 — спек+дока (sonnet).** D323-амендмент (6б.5.1) + `docs/guide/io-fs.md` абзац + строка в
D412-амендменте. **Гейт:** owner-review не требуется (аддитивно); едет вместе с Ф.6б.1.

**Ф.6б.4 — флагман-потребитель (sonnet, опц., НЕ блокер).** Карта в 6б.4: `aggregator` main.nv
`embed(...)`→`serve_assets(embedded_assets())`. **НЕ реализуется этой волной** (только карта).
**Гейт (когда делается):** `--strict-effects` на `examples/flagship/aggregator` у оркестратора
(test-conventions; conformance app-регрессии не ловит).

**Порядок:** Ф.6б.1 → Ф.6б.2 → Ф.6б.3 (вместе). Ф.6б.4 — позже/опц. Всё аддитивно; в main —
после зелёного гейта Ф.6б.2 + conformance-CU у оркестратора.

**Оценка:** малый. std ≈ 150-210 строк + тест; спек/дока ≈ +30. Риск низкий (0 нового синтаксиса,
переиспользование io.Read-машины).

### 6б.7. Риски и открытые вопросы владельцу (рекомендация каждому)

**Риски / митигации:**

| Риск | Митигация |
|---|---|
| **R1 (главный):** structural conformance generic-bound `[F ReadFs]` НЕ видит extension-методы `EmbeddedDir` (только inherent) → `EmbeddedDir` не конформит | Ф.6б.2 тест (2) доказывает ДО остального. **Fallback (пред-специфицирован, как §9.1-паттерн):** тонкая обёртка-newtype `EmbeddedFs value { priv dir EmbeddedDir }` в std.fs с INHERENT `@read_file`/`@try_exists` → гарантированная conformance-видимость; `embedded_assets()` возвращает `EmbeddedFs`, а не голый `EmbeddedDir`. Родной API `EmbeddedDir` всё равно не тронут. |
| Symlink-escape в `DirFs` через realpath не пойман (TOCTOU: файл-симлинк подменён между canonicalize и read) | Читать РЕЗУЛЬТАТ `canonicalize` (уже резолвлен), не исходный `joined`; остаточный TOCTOU — общий для всех статик-серверов (тот же класс, что `[M-176-dir-scoped-ops]` openat-hardening; вне V1). |
| str-prefix `f.starts_with(r)` ложно проходит `/a/bc` при корне `/a/b` | В 6б.3 уже граница компонента (`f == r \|\| starts_with(r + "/")`); реализация Ф.: либо этот guard, либо сравнение `@components()`. |
| Эффектный `DirFs` под `handler_fn(fn(ServerRequest)->ServerResponse)`: closure-тип не объявляет `Fs` | Проверить в Ф.6б.4 (карта): closure зовётся под ambient `with Fs` (связан в main); всплытие эффекта в теле замыкания — по closure-capture-правилам D62. НЕ блокер дизайна протокола; риск ИНТЕГРАЦИИ, вскрывается на потребителе. |
| dev live-reload: `DirFs` читает диск на каждый запрос (нет кэша) | By design (dev = свежесть важнее скорости); prod (`EmbeddedDir`) — zero-I/O. Кэш-слой — future, вне ReadFs. |

**Открытые вопросы владельцу (рекомендация + альтернатива):**

- **Q6б-1 — имя протокола.** Рекомендую **`ReadFs`** (коротко, читается «read filesystem»;
  паритет `io.Read`). Альтернативы: `Fs` (занято эффектом), `Vfs`, `FileSource`, `Assets`.
  *Рекомендация: `ReadFs`.* → 6б.2.
- **Q6б-2 — `try_exists` в протоколе (сверх `read_file`).** Рекомендую **оставить** (HEAD/pre-check;
  задание просит минимум read_file+exists). Альтернатива — выкинуть (read_file's NotFound
  закрывает 404). *Рекомендация: оставить `@try_exists`; `list` — вне (отдельный `ListFs`,
  future).* → 6б.2.
- **Q6б-3 — размещение EmbeddedDir-conformance.** Рекомендую **extension-методы в std.fs**
  (prelude чист; родной API не тронут; идиоматично D286/D287). Альтернатива при провале R1 —
  **wrapper-newtype `EmbeddedFs`**. *Рекомендация: extension; fallback wrapper пред-специфицирован.*
  → 6б.3а / R1.
- **Q6б-4 — DirFs.new чистый vs fallible-канонизирующий.** Рекомендую **чистый ктор** (root не
  обязан существовать при конструировании — dev; escape-проверка на чтении). Альтернатива —
  `DirFs.open(root) Fs -> Result[DirFs, IoError]` (канонизирует root заранее, ловит «нет папки» на
  старте). *Рекомендация: чистый `new`; при желании «падать на старте, если папки нет» — добавить
  опц. `DirFs.open` позже.* → 6б.3б.

---

## Ф.7 — Go-паритет+: glob/embed_str/hidden/merge

> **Статус: ✅ РЕАЛИЗОВАНО 2026-07-17** (owner-go «впиши в план и реализуй»; sonnet,
> worktree `nova-210g`, ветка `p210-goparity`). D412-амендмент дополнен в том же
> слиянии (§9-продолжение ниже) — язык-меняющие пункты (glob/embed_str — новый
> интринсик; hidden — новый именованный аргумент) едут С амендментом, как того
> требует dev-workflow. `merge` — чистая Nova-body функция, аддитивна, языка не меняет.

Закрывает оставшийся «сегодня нужный» кусок Go-паритета сверх V1 `embed_dir`
(`//go:embed` поддерживает glob-паттерны и множественные директивы; `embed.FS`
не даёт merge, но композиция нескольких `embed_dir` в один каталог — частый
реальный кейс раздачи статики из нескольких источников).

### Ф.7.1 — `embed_dir("dir", glob: "pattern")`

**Реализация:** `compiler-codegen/src/embed_resolve.rs` — `try_replace_embed_dir`
теперь парсит `args[1..]` как именованные аргументы (`CallArg::Named`), а не
только позиционный путь. Новый `glob_match_posix` (+ `glob_tokenize`/`GlobAtom`)
— собственный DP-матчер над `char`-массивами (`O(P·T)`, БЕЗ backtracking-взрыва,
БЕЗ новых Cargo-зависимостей):
- `*` — ноль+ символов, **не пересекает** `/` (как bash без `globstar` /
  `path.Match`);
- `**` — ноль+ символов, **пересекает** `/` (bash `globstar`-стиль — это
  РАСХОЖДЕНИЕ с Go `//go:embed`, который `**` не поддерживает вообще; выбрано
  по прямому заданию владельца 2026-07-17, а не как paritet с Go);
- `?` — ровно один символ, не `/` (добавлено сверх задания — почти бесплатно
  в той же DP, обычное ожидание от «простого матчера»);
- любой другой символ — буквальный (включая явный `/`, что даёт возможность
  писать `"nested/*"` для явной сегментации).

**Известное упрощение** (не bash-паритет): `**` не даёт «нулевой-каталог»
сокращения bash — `"**/*.png"` требует буквальный `/` в тексте, НЕ матчит
файл в корне. Для матча с любой глубиной ИЗ КОРНЯ используйте `"**.png"` (без
слэша перед маской) — задокументировано в doc-comment `glob_match_posix` и
подтверждено фикстурой `d412e_embed_dir_glob.nv`.

**Точка применения фильтра:** ПОСЛЕ dot/symlink-skip и ПОСЛЕ NFC-нормализации
(Ф.6а) — маска сверяется с ФИНАЛЬНЫМ POSIX-ключом, тем же, что уходит в
`EmbeddedEntry.path`. `glob` и `hidden` (Ф.7.3) — независимы: `hidden` решает,
что ПОПАДАЕТ в обход; `glob` фильтрует уже собранный результат (dot-skipped
записи `glob` вернуть не может, если `hidden` не включён рядом).

**Диагностики:**
- не-литеральный `glob:` (переменная/выражение) → **reuse**
  `E_EMBED_ARG_NOT_STR_LITERAL` (та же семья, что не-литеральный путь —
  задание явно просило «как not_literal»);
- пустой результат ПОСЛЕ фильтра → **reuse** `W_EMBED_DIR_EMPTY` (уже
  существовал для dot/symlink-опустошения; текст warning'а расширен, чтобы
  упомянуть `glob`, когда он присутствует);
- неизвестное имя именованного аргумента / дублирующий `glob:`/`hidden:` /
  лишний позиционный аргумент / spread — **новый** `E_EMBED_DIR_BAD_ARG`
  (не было в исходном задании явно, но необходимо: без него опечатка вроде
  `filter:` вместо `glob:` тихо игнорировалась бы — заведён один код на всю
  эту малую семью form-ошибок, симметрично тому, как `E_EMBED_ARG_NOT_STR_LITERAL`
  уже покрывает несколько сценариев одним кодом).

**Фикстуры:** pos `spec_tests/conformance/d412e_embed_dir_glob.nv` (+ папка
`d412e_glob_dir/{a.png,b.txt,nested/{c.png,d.txt}}`) — 4 теста (`*.png`
non-crossing, `**.png` crossing, `nested/*` явный сепаратор, без `glob` —
unfiltered backward-compat). neg: `neg/d412e_glob_not_literal_neg.nv`
(`E_EMBED_ARG_NOT_STR_LITERAL`), `neg/d412e_dir_bad_named_arg_neg.nv`
(`E_EMBED_DIR_BAD_ARG`). standalone (warning — целый TU):
`standalone/d412e_embed_dir_glob_empty.nv` (+ `standalone/d412e_glob_dir_for_empty/`)
— `W_EMBED_DIR_EMPTY`. **Вердикт: все PASS** (проверено обходным путём —
временные `standalone._verify210g_*_tmp` дубликаты на `../`-путях к РЕАЛЬНЫМ
committed-фикстурам, прецедент `210-impl-progress.md`, — прогнаны, удалены
перед коммитом; committed pos-фикстуры не гоняются напрямую, чтобы не
триггерить мега-CU `spec_tests.conformance`, см. Ф.4 note там же).

### Ф.7.2 — `embed_str("file")`

**Реализация:** новая `try_replace_embed_str` в `embed_resolve.rs` — зеркало
`try_replace_embed` (not_found/is_dir/escape/backslash — **ТЕ ЖЕ коды**,
задание явно требовало симметрии). Отличие: после чтения байт —
`String::from_utf8`; успех → `ExprKind::StrLit(text)` (та же интернирующая
инфраструктура, что и любой рукописный `"…"` — `intern_str_literal`,
`emit_c.rs` строка ~55020, **0 правок emit_c**, тот же принцип, что и весь
план 210); провал → **новый** `E_EMBED_NOT_UTF8` с offset'ом первого
невалидного байта (`Utf8Error::valid_up_to()` — Rust std УЖЕ даёт точный
byte-offset, не пришлось реализовывать вручную).

**Фикстуры:** pos `d412e_embed_str.nv` (+ `d412e_str_fixture.txt` = "Hello,
Nova! Ω", валидный UTF-8 с не-ASCII символом) — round-trip через `==` на
`str`. neg: `neg/d412e_str_not_utf8_neg.nv` (`E_EMBED_NOT_UTF8`, фикстура
`d412e_str_invalid_utf8.bin` = `"Hi "` + `0xFF 0xFE` + `"end"`, первый битый
байт на offset 3 — сообщение содержит offset), `neg/d412e_str_not_found_neg.nv`
(`E_EMBED_NOT_FOUND`), `neg/d412e_str_on_dir_neg.nv` (`E_EMBED_IS_A_DIR`,
реюз `../d412e_glob_dir`). Escape/backslash для `embed_str` НЕ дублированы
отдельными фикстурами — тот же код (`root_for`/`check_path_backslash`), что
уже покрыт `embed`/`embed_dir`-фикстурами; предельно низкий маргинальный
риск нового бага именно в этой ветке. **Вердикт: все PASS.**

### Ф.7.3 — `embed_dir("dir", hidden: true)`

**Реализация:** `walk_embed_dir_rec` получил параметр `hidden: bool`;
dot-skip (`name.starts_with('.')`) теперь под условием `!hidden`. Симлинки
скипаются **безусловно** — `hidden` на это НЕ влияет (прямое требование
задания). Дефолт `false` — байт-в-байт то же поведение, что до Ф.7 (проверено
неизменным `d412d_*`-фикстурами Ф.4, регрессии нет).

**Фикстуры:** pos `d412e_embed_dir_hidden.nv` (+ `d412e_hidden_dir/{visible.txt,.secret}`)
— 3 теста (default/explicit-false/true). Явных neg не требовалось (нет нового
кода ошибок для `hidden`, кроме `E_EMBED_DIR_BAD_ARG` при не-bool значении —
покрыт тем же тестом, что unknown-arg neg, той же веткой кода). **Вердикт: PASS.**

### Ф.7.4 — `EmbeddedDir @merge(other EmbeddedDir) -> EmbeddedDir`

**Реализация:** ЧИСТАЯ Nova-body функция в `std/src/prelude/embed.nv` — O(N+M)
отсортированная склейка (шаг mergesort, НЕ конкатенация+пересорт). Совпадающий
путь в обоих входах → `panic("EmbeddedDir.merge: duplicate path ... present in
both directories")` — тот же контракт, что `EmbeddedDir.new` на несортированном
входе (конфликт двух встроенных деревьев с одинаковым путём — программная
ошибка автора, не runtime-развилка «взять любую версию»); реализовано через
`EmbeddedDir.new(out)` в конце (переиспользует verify + защитную копию, а не
голый `Self { entries: out }`, ради консистентности со всем остальным API).
**0 правок компилятора** — как и предсказано заданием.

**Тесты** (в `std/src/prelude/embed_test.nv`, рядом с остальными
`EmbeddedDir`-тестами — module-beside-module конвенция): happy-path
непересекающейся склейки (порядок/байты проверены), оба edge-case пустой
стороны (left-empty/right-empty), `panics "duplicate"` на пересекающемся
пути. **Вердикт: 10/10 PASS** (`nova test std/src/prelude/embed_test.nv`,
включая 7 существующих Ф.1-тестов — регрессии нет).

### Общий гейт волны Ф.7

Изолированный прогон (`nova check`/`nova test` на конкретных файлах —
таргетно, БЕЗ полного мега-CU и БЕЗ `nova test std`) — все новые pos/neg/
standalone-фикстуры и std-тесты зелёные (см. вердикты по каждому пункту
выше). Полный `spec_tests/conformance` (мега-CU) и флагман-`--strict-effects`
— у оркестратора при вливании (test-conventions, прецедент 206/D423).

---

## Ф.8 — эффективная эмиссия payload (`#embed`/incbin)

> **Статус: ✅ РЕАЛИЗОВАНО (разведка + implementation) 2026-07-17** (owner-go
> «впиши в план и реализуй»; sonnet, worktree `nova-210g`, ветка
> `p210-goparity`). Стык с Планом 209 — см. §9.2 ревью-3 п.1(б) выше (уже
> зафиксирован) + `docs/plans/209-multi-tu-codegen.md` §Ф.5 (тоже уже
> ссылается сюда, 2026-07-16) — взаимная ссылка подтверждена в обоих планах.

### Ф.8.1 — Разведка (замер, не мнение)

**Локальный clang:** `C:\Program Files\LLVM\bin\clang.exe`, **22.1.5**
(`x86_64-pc-windows-msvc`) — C23 `#embed` **работает** с `-std=c23`
(проверено минимальным пробником `#embed "payload.bin"` внутри
`static const unsigned char[]` — компилируется и линкуется, `sizeof`/первый
байт верны).

**CI (`ubuntu-latest`, `nova-gate.yml:105` — `apt-get install -y clang`):**
проверено НАПРЯМУЮ в `docker run ubuntu:24.04` (та же база, что GH-хостед
`ubuntu-latest`) — `apt-get install clang` даёт **clang 18.1.3** (пакет
`1:18.0-59~exp2`). `#embed` (C23 в Clang ≥19) **НЕ поддержан**: и `-std=c23`,
и `-std=c2x` дают `error: invalid preprocessing directive` — препроцессор
clang 18 не знает директиву вообще (это НЕ ошибка типов/семантики, это
незнакомый токен). **Вывод разведки:** локальная машина и CI **расходятся**
по возможностям тулчейна ПРЯМО СЕЙЧАС — эмиссия ОБЯЗАНА быть
**рантайм-проверяемой** (behavior-probe), а не флагом/версией-парсингом
(разные vendor'ы/платформы нумеруют clang по-разному — Apple clang тому
пример, хотя здесь не проверялся; сам факт CI≠локально уже доказывает, что
одной статической проверки версии недостаточно).

**Альтернативы, оценены честно:**
- **Строковый литерал с escape** (`"\x41\x42..."` вместо `0x41,0x42,...`
  массива) — НЕ измерялся отдельно: тот же класс «текстовый рендеринг
  каждого байта», не решает корневую причину (объём текста в `.c` и время
  парсинга/лексирования clang'ом), выигрыш кардинально МЕНЬШЕ, чем `#embed`
  (см. замер ниже) — не оправдывает отдельную реализацию.
- **`objcopy`/`incbin`** — НЕпортируемо на MSVC-линк (объектный формат/линкер
  не тот же, что GNU `ld`; на Windows потребовал бы либо MASM `INCBIN`-style
  ассемблерной обёртки, либо отдельный COFF-объект — новый тулчейн-путь на
  каждой платформе). Честный вердикт: НЕ реализовывать — портируемость хуже,
  чем у `#embed` (который просто НЕ работает при недоступности, а не ломает
  сборку по-другому на каждой ОС).
- **C23 `#embed` (реализовано, см. Ф.8.2)** — единственный вариант, который
  портируем В ОБА КОНЦА: работает, где доступен; НЕ трогает тулчейн там, где
  недоступен (просто не используется, hex остаётся как было).

### Ф.8.2 — Замер (синтетика 1 МБ, `1048576` случайных байт)

| Метрика | Hex-рендер (текущий, `0x%02X,`) | C23 `#embed` (sidecar-файл) | Выигрыш |
|---|---|---|---|
| Размер `.c` | 6 422 659 байт (~6.4 МБ, ×6.1 — `xxd -i`-формат чуть многословнее собственного `0x%02X,` компилятора, но того же порядка, что задокументированный ×5.3) | **157 байт** | **~40 900×** меньше текста |
| `clang -c` время (4 прогона, `x86_64-pc-windows-msvc`, clang 22.1.5) | 3647 / 3673 / 2092 / 3375 мс | 124 / 111 / 115 / 114 мс | **~28-30×** быстрее компиляция |
| Итоговый `.o` | 746 байт | 746 байт (**байт-в-байт идентичен** hex-версии) | корректность подтверждена |

**Порог гейта задания** («если выигрыш <×2 по любому из двух — оставить за
флагом») **разгромно превышен по ОБОИМ измерениям** (~40 900× и ~28-30×,
против требуемых ×2) — реализация оправдана без всяких оговорок, ГДЕ `#embed`
доступен. Там, где недоступен (CI, clang 18) — автоматический fallback на
существующий hex-рендер, **байт-в-байт то же поведение**, что было (нулевое
изменение для CI, снятие пользы только там, где технически невозможно).

### Ф.8.3 — Реализация (feature-probe + sidecar + fallback) — ✅ ГОТОВО

**Рантайм-проб** (`compiler-codegen/src/test_runner.rs`, `embed_c23_supported()`):
кэшированный (`OnceLock<bool>`, один раз за процесс) — компилирует крошечный
`#embed`-пробник через `clang -std=c23 -fsyntax-only` (без объектного файла,
дёшево); `false` для не-clang тулчейнов и для clang, отвергающего пробник.
`find_clang_path` повышена до `pub(crate)` для переиспользования.

**Sidecar-эмиссия** (`compiler-codegen/src/codegen/emit_c.rs`): новое поле
`CEmitter.blob_sidecar_dir: Option<PathBuf>` (+ сеттер `set_blob_sidecar_dir`,
по образцу существующих `set_mono_depth_limit`/`set_source_for_annotations`).
`render_interned_blob_literals` — когда `blob_sidecar_dir` задан И
`embed_c23_supported()` — каждый блоб пишется в `<sym>.bin` РЯДОМ с `.c`
(через новый `try_write_blob_sidecar`) и рендерится `#embed "<sym>.bin"`
вместо `0x%02X,`-массива. **Per-blob fallback:** сбой sidecar-записи одного
блоба не теряет данные — этот ОДИН блоб рендерится hex'ом, остальные (если
проб/директория ок) — по-прежнему `#embed`. Дефолт (`blob_sidecar_dir ==
None`) — ЗАДОКУМЕНТИРОВАННОЕ и ПРОВЕРЕННОЕ (см. верификация ниже)
byte-identical поведение с pre-Ф.8 кодом.

**Wiring — OPT-IN, один call-site** (`nova-cli/src/main.rs`, `build`-команда):
`NOVA_C23_EMBED=1` — единственный переключатель. Env var, не CLI-флаг
(`clap`) — потому что `emit_module`/`CEmitter` вызывается из ТРЁХ разных
бинарей/файлов (`nova-cli/main.rs`, `compiler-codegen/main.rs`,
`test_runner.rs`) и проброс через сигнатуры задел бы все три; env
var — ортогонально, нулевой сигнатурный след. Директория sidecar вычисляется
РАНЬШЕ обычного места (`path_hash`/`default_tmp_dir` чисты от `path`+pid
процесса → байт-в-байт совпадают с уже существующим `tmp_path`, вычисляемым
позже в той же функции для записи `.c`) — **дефолт (env unset) не меняет ни
одной строки в горячем пути** `nova build` (ранний блок — чистый no-op,
условие на `env::var` даже не читает `path_hash` если var не установлена).

**НЕ wired этой волной:** `test_runner.rs`'s `nova test`-путь (карта
симметрична, несложный follow-up — не сделано из-за бюджета времени после
сетевого обрыва в процессе исполнения). `compiler-codegen/main.rs`
(standalone `nova-codegen` CLI) — тоже не wired (секондари тулза).

**Верификация E2E** (300 КБ `embed("asset.bin")`, `--keep-artifacts`,
локальный clang 22.1.5):

| | `NOVA_C23_EMBED` unset (дефолт) | `NOVA_C23_EMBED=1` |
|---|---|---|
| `.c` размер | 1 774 947 байт (18752× `0x`, 0× `#embed`) | 181 237 байт (0× `0x` для этого блоба, 1× `#embed`) |
| sidecar-файл | нет | `nova_blob_<hash>.bin`, 300 000 байт — точное совпадение с `asset.bin` |
| `.exe` вывод | `300000` | `300000` (байт-в-байт идентичный рантайм-результат) |

Регрессия исключена (дефолт-путь не тронут ни байтом); прирост подтверждён
на реальном (не только синтетическом) файле сквозным прогоном компилятора.

**Таргетный regression-check после Ф.8** (не мега-CU): `nova test
std/src/prelude/embed_test.nv` (10/10) + 3 разноплановых `d412d_*`/`d412e_*`
фикстуры (neg not_found, standalone warning, neg not_utf8) — все PASS,
эмиттер общий код (`render_interned_blob_literals`) не сломал существующий
hex-путь ни для одного из них (все три собирались с дефолтным `blob_sidecar_dir
== None`).

### Ф.8.4 — Стык с Планом 209 (взаимная ссылка)

Уже зафиксировано в обоих планах (проверено 2026-07-17, НЕ новая запись):
`docs/plans/210-embed-dir.md` §9.2 ревью-3 п.1(б) (выше в этом документе) и
`docs/plans/209-multi-tu-codegen.md` §Ф.5 (`> **210-стык (ревью-3
2026-07-16):** блоб-статики (embed/embed_dir) сейчас рендерятся в ПРОЛОГ …`)
— оба указывают друг на друга. **Дополнение этой волны:** sidecar-файлы
`#embed`-пути (Ф.8.3) наследуют ТО ЖЕ требование 209×210 («определения
блобов — в ОДИН part, в common — только extern») — sidecar физически должен
лежать там, где искомый `.c`-part СОДЕРЖИТ определение (не там, где он
только `extern`-декларирован), иначе относительный путь `#embed "…"` в
common-заголовке не найдёт файл ни у одного part'а или найдёт не тот. Если
209 Ф.6 (multi-TU default-on) включается ПОСЛЕ Ф.8 — ревизировать sidecar
path-relativity на реальном multi-TU прогоне (не блокер, т.к. multi-TU
сегодня opt-in `NOVA_MULTI_TU=1`, не дефолт).

---

## Ф.9 — `read_dir` (по-каталожный листинг; Go `ReadDir`-паритет) — ✅ GO владельца 2026-07-16

> **Renamed 2026-07-17** (sonnet, wave Ф.7/Ф.8-goparity): эта секция была
> «Ф.7» в исходном документе — номер освобождён под новую волну
> (glob/embed_str/hidden/merge, вставлена выше по документу, после §6б).
> Статус/объём этой секции не менялись — `read_dir` НЕ реализован этой
> волной (владелец не просил его в задании 2026-07-17).

Замыкает последний «сегодня нужный» Go-паритет-гэп (плоский `paths()` → уровень-листинг). **Чистый `.nv`,
0 правок компилятора:**
- `EmbeddedDir @read_dir(prefix str) -> []str` — пути отсортированы → бинарный поиск диапазона префикса +
  срез ОДНОГО уровня + dedup под-папок (~30 строк; вернуть имена уровня: файлы как есть, под-папки без
  дублей — форма записи (str vs типизированная DirEntry-лайт) — по месту, минимализм);
- `DirFs @read_dir(prefix str) Fs -> Result[[]str, IoError]` — делегат в существующий `std/fs.read_dir`;
- вопрос протокола: добавить `read_dir` в `ReadFs` (оба импла тривиальны) ИЛИ отдельный `ReadDirFs`
  (Go-модель `ReadDirFS`) — рекомендация: В `ReadFs` (у нас нет interface-extension паттерна, оба импла есть);
  внимание на асимметрию fallible (Embedded infallible / DirFs Result — та же развязка, что read_file Ф.6б).
- Тесты рядом (readfs_test.nv): уровень/вложенность/пустой префикс/несуществующий префикс/dedup под-папок.
**Модель:** sonnet по этой карте (haiku не потянет протокольную развязку); гейт — оркестратор.

**HTTP-статик + mime (Go `http.FileServer`-паритет) — ✅ GO владельца 2026-07-16, но НЕ здесь:** после
Plan 203 http живёт во внешнем пакете **nv-lang/nova-http** — пункт зарегистрирован в его README-roadmap
(`serve_static(mux, fs ReadFs)` + mime-по-расширению); потребитель-витрина — флагман 187 (`embed_dir` +
`serve_static` = «фронт целиком в бинаре» одной строкой). Glob-фильтр — остаётся future (§10) до реального кейса.

## 10. Вне объёма

- **Glob-фильтр** (`embed_dir("dir", "*.html")`, `all:`-префикс) — future, отдельный заход.
- **Content-Type/mime-роутинг** — хелпер `std/http`, не интринсик.
- **Сжатие вшитых ассетов** (gzip-in-binary) — future (гейт nova-compress).
- **dev-режим** (АВТОМАТИЧЕСКОЕ чтение с диска в debug, rust-embed-стиль) — отклонён (§2л).
  NB: это НЕ противоречит `ReadFs`/`DirFs` (Ф.6б) — там dev-с-диска **явный opt-in** через
  генерик-ветку на старте (`DirFs` — отдельный тип, чистота `EmbeddedDir` не тронута), а не
  скрытая подмена типа `embed_dir` в debug.
- **Полностью-static таблица** (Option N) — future codegen-опт (§3).
- **LSP/hover со списком встроенных путей** — future tooling (§2к).
