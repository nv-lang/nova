<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 210 — `embed_dir(...)`: вшить папку в бинарь (расширение D412)

> **Статус: ✅ ДИЗАЙН ФИНАЛИЗИРОВАН 2026-07-16 (вариант A + материализация Option R, ревиз. R′).**
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
/// Хранит ЗАЩИТНУЮ мелкую копию входа (заголовки/указатели; payload'ы не копируются) —
/// пост-конструкционная мутация исходного вектора вызывающим не ломает инвариант.
export fn EmbeddedDir.new(entries []EmbeddedEntry) -> Self {
    mut i = 1
    while i < entries.len() {
        if entries[i - 1].path.compare(entries[i].path) >= 0 {
            panic("EmbeddedDir.new: entries must be sorted by path, unique")
        }
        i = i + 1
    }
    Self { entries: entries.clone() }   // защитная мелкая копия (алиас-защита, ревью-2)
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

## 10. Вне объёма

- **Glob-фильтр** (`embed_dir("dir", "*.html")`, `all:`-префикс) — future, отдельный заход.
- **Content-Type/mime-роутинг** — хелпер `std/http`, не интринсик.
- **Сжатие вшитых ассетов** (gzip-in-binary) — future (гейт nova-compress).
- **dev-режим** (чтение с диска в debug, rust-embed-стиль) — отклонён (§2л).
- **Полностью-static таблица** (Option N) — future codegen-опт (§3).
- **LSP/hover со списком встроенных путей** — future tooling (§2к).
