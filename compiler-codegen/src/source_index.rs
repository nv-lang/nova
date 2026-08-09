// SPDX-License-Identifier: MIT OR Apache-2.0
//! План 252 Ф.2 — **неизменный индекс дерева исходников**.
//!
//! **Чем это отличается от Ф.1.** Ф.1 завела кэш обращений к ФС, у которого
//! у каждой записи был отпечаток источника (`mtime` + размер), сверяемый при
//! КАЖДОМ обращении. Владелец снял основание этой сверки (2026-08-09):
//! «ты серьёзно ожидаешь, что что-то изменится во время самой компиляции?
//! даже если так, то один файл скомпилируется со старой версией, а другой с
//! новой?». Если исходник правят посреди сборки, согласованности нет в любом
//! случае — сверка отпечатка её не создаёт, а стоит `stat`/`read_dir` на
//! каждое обращение. Отсюда правило: **снимок снимается один раз за прогон и
//! дальше неизменен**; правка исходников во время сборки — ошибка сборки.
//!
//! **Чем это отличается от кэша вообще.** Кэш лечит симптом (повторный
//! перебор), индекс убирает причину: дерево читается по разу на каталог и по
//! разу на файл, а резолв импорта после этого — поиск в карте, без обращений
//! к ФС. Сложность падает с `импорты × кандидаты × ФС` до `файлы + импорты`.
//! Так устроены `go/build` (карта «путь пакета → каталог») и Cargo (граф
//! пакетов строится один раз за сборку).
//!
//! **Что индексируется.**
//!   * [`dir_entries`] — один `read_dir` на каталог: имя → вид (файл/каталог)
//!     плюс готовый отсортированный список `.nv`. Отсюда [`is_file`],
//!     [`is_dir`], [`exists`], [`nv_files`] — **без единого сисвызова**.
//!   * [`file_text`] — содержимое `.nv`-файла, читается по разу.
//!   * [`canonicalize`] — результат `fs::canonicalize`, по разу на путь.
//!   * [`derived`] — произвольный вывод по каталогу (объявления `module`
//!     соседей, вердикт «папка-модуль») — считается по разу на каталог.
//!
//! **Регистр.** `Path::is_file()` спрашивает ОС, а ОС на Windows/macOS
//! отвечает регистронезависимо; поиск по точному имени в карте отвечал бы
//! иначе и менял бы диагностику (`E_CASE_MISMATCH`, план 81 Ф.4). Поэтому
//! индекс каталога хранит и карту «имя в нижнем регистре → имя на диске», а
//! регистро-чувствительность самого каталога определяется ОДНОЙ пробой при
//! его индексации (см. [`probe_case_insensitive`]) — не `cfg!`-догадкой.
//!
//! **Режим.** По умолчанию индекс ВЫКЛЮЧЕН (сквозной проход к ФС) — это в
//! точности сегодняшнее поведение, и всякий забытый вызывающий остаётся
//! корректным. Включает его явно тот, кто знает, что его процесс — один
//! прогон: `nova-cli::main`. Резидентные процессы (`nova-lsp` — отдельный
//! бинарь, `nova doc --watch`, `nova daemon serve`) снимок НЕ включают: они
//! обязаны видеть правки пользователя между проходами.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

// ─── Режим ───────────────────────────────────────────────────────────────

static SNAPSHOT: AtomicBool = AtomicBool::new(false);

/// Включить режим снимка. Зовётся ОДИН раз в начале прогона процессом,
/// который знает, что он — один прогон компиляции.
pub fn enable_snapshot() {
    // Аварийный рубильник и способ сравнения А/Б: `NOVA_SRC_INDEX=0`
    // оставляет сквозной проход к ФС даже там, где снимок запрошен.
    let off = matches!(
        std::env::var("NOVA_SRC_INDEX").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("no")
    );
    SNAPSHOT.store(!off, Ordering::Relaxed);
}

/// Выключить режим снимка (резидентный процесс, следующая итерация watch).
pub fn disable_snapshot() {
    SNAPSHOT.store(false, Ordering::Relaxed);
    reset();
}

#[inline]
fn on() -> bool {
    SNAPSHOT.load(Ordering::Relaxed)
}

/// Режим снимка включён: вызывающий вправе считать дерево неизменным.
pub fn snapshot_enabled() -> bool {
    on()
}

// ─── Счётчики обращений к ФС ─────────────────────────────────────────────

static FS_READ_DIR: AtomicU64 = AtomicU64::new(0);
static FS_READ_FILE: AtomicU64 = AtomicU64::new(0);
static FS_CANON: AtomicU64 = AtomicU64::new(0);
static FS_STAT: AtomicU64 = AtomicU64::new(0);
/// Число разрешений импорта (знаменатель мерила «обращений к ФС на импорт»).
static IMPORT_RESOLVES: AtomicU64 = AtomicU64::new(0);

/// Отметить одно разрешение импорта (вызов `resolve_module_paths`).
pub fn note_import_resolve() {
    IMPORT_RESOLVES.fetch_add(1, Ordering::Relaxed);
}

/// Суммарное число обращений к ФС, сделанных резолвом импортов.
pub fn fs_calls() -> u64 {
    FS_READ_DIR.load(Ordering::Relaxed)
        + FS_READ_FILE.load(Ordering::Relaxed)
        + FS_CANON.load(Ordering::Relaxed)
        + FS_STAT.load(Ordering::Relaxed)
}

/// Сводка для отчёта. Печатается вместе со счётчиками Ф.0.
pub fn stats_line() -> String {
    let rd = FS_READ_DIR.load(Ordering::Relaxed);
    let rf = FS_READ_FILE.load(Ordering::Relaxed);
    let cn = FS_CANON.load(Ordering::Relaxed);
    let st = FS_STAT.load(Ordering::Relaxed);
    let imports = IMPORT_RESOLVES.load(Ordering::Relaxed);
    let total = rd + rf + cn + st;
    let per_import = if imports > 0 {
        format!("{:.4}", total as f64 / imports as f64)
    } else {
        "n/a".to_string()
    };
    let n_dirs = map_len(&DIRS);
    let n_head = map_len(&HEADERS);
    let n_full = map_len(&FILES);
    let n_canon = map_len(&CANON);
    format!(
        "\n===== source_index (план 252 Ф.2) =====\n\
         режим снимка                      : {}\n\
         read_dir                          : {}\n\
         чтений файлов                     : {}\n\
         canonicalize                      : {}\n\
         stat (симлинки + проба регистра)  : {}\n\
         ВСЕГО обращений к ФС              : {}\n\
         разрешений импорта                : {}\n\
         обращений к ФС НА ИМПОРТ          : {}\n\
         в индексе: каталогов              : {}\n\
         в индексе: заголовков             : {}\n\
         в индексе: файлов целиком         : {}\n\
         в индексе: канонических путей     : {}\n",
        if on() { "включён" } else { "ВЫКЛЮЧЕН (сквозной проход)" },
        rd, rf, cn, st, total, imports, per_import,
        n_dirs, n_head, n_full, n_canon
    )
}

fn map_len<T: 'static + Send>(cell: &'static OnceLock<Vec<Mutex<HashMap<PathBuf, T>>>>) -> usize {
    match cell.get() {
        Some(sh) => sh.iter().map(|s| s.lock().map(|g| g.len()).unwrap_or(0)).sum(),
        None => 0,
    }
}

// ─── Хранилище ───────────────────────────────────────────────────────────

const SHARDS: usize = 32;

fn shard_of(path: &Path) -> usize {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    (h.finish() as usize) % SHARDS
}

fn shards<T: 'static + Send>(
    cell: &'static OnceLock<Vec<Mutex<HashMap<PathBuf, T>>>>,
) -> &'static Vec<Mutex<HashMap<PathBuf, T>>> {
    cell.get_or_init(|| (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    File,
    Dir,
    Other,
}

/// Запись каталога: вид + признак «это симлинк». Признак нужен ровно одному
/// месту — сверке регистра: `fs::canonicalize` разыменовывает ссылки и
/// показывает имена ЦЕЛИ, а индекс хранит имена ссылки. Там, где ссылок нет
/// (обычный случай), ответы совпадают; там, где есть, сверка регистра
/// откатывается на `canonicalize`, чтобы семантика не разошлась.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Ent {
    kind: Kind,
    symlink: bool,
}

/// Снимок одного каталога: снят один `read_dir`, дальше неизменен.
struct DirIndex {
    /// Каталог существует и читается.
    exists: bool,
    /// В каталоге есть хотя бы одна символическая ссылка.
    has_symlink: bool,
    /// Имя записи → запись. Симлинки разрешены до цели (как `Path::is_file`).
    by_name: HashMap<String, Ent>,
    /// Имя в нижнем регистре → имя на диске. Нужна только на
    /// регистронезависимой ФС; на регистро-чувствительной не смотрится.
    lower: HashMap<String, String>,
    /// ФС этого каталога регистронезависима (определено пробой, не догадкой).
    case_insensitive: bool,
    /// Обычные `.nv`-файлы каталога, отсортированы по пути.
    nv: Arc<Vec<PathBuf>>,
}

static DIRS: OnceLock<Vec<Mutex<HashMap<PathBuf, Arc<DirIndex>>>>> = OnceLock::new();
static FILES: OnceLock<Vec<Mutex<HashMap<PathBuf, Option<Arc<String>>>>>> = OnceLock::new();
type HeaderVal = Option<(Arc<String>, bool)>;
static HEADERS: OnceLock<Vec<Mutex<HashMap<PathBuf, HeaderVal>>>> = OnceLock::new();
static CANON: OnceLock<Vec<Mutex<HashMap<PathBuf, Option<PathBuf>>>>> = OnceLock::new();

type DerivedMap = HashMap<PathBuf, HashMap<&'static str, Arc<dyn std::any::Any + Send + Sync>>>;
static DERIVED: OnceLock<Vec<Mutex<DerivedMap>>> = OnceLock::new();

fn derived_shards() -> &'static Vec<Mutex<DerivedMap>> {
    DERIVED.get_or_init(|| (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect())
}

/// Сбросить индекс целиком — вместе со всем, что из него выведено (карта
/// модулей, разобранные манифесты). Для внутренних тестов и для итерации
/// `--watch`: одна точка сброса вместо трёх — иначе где-нибудь останется
/// вывод, посчитанный по прежнему снимку.
pub fn reset() {
    crate::imports::reset_module_map();
    crate::manifest::reset_manifest_cache();
    reset_index_only();
}

fn reset_index_only() {
    if let Some(sh) = DIRS.get() {
        for s in sh {
            if let Ok(mut g) = s.lock() {
                g.clear();
            }
        }
    }
    if let Some(sh) = FILES.get() {
        for s in sh {
            if let Ok(mut g) = s.lock() {
                g.clear();
            }
        }
    }
    if let Some(sh) = HEADERS.get() {
        for s in sh {
            if let Ok(mut g) = s.lock() {
                g.clear();
            }
        }
    }
    if let Some(sh) = CANON.get() {
        for s in sh {
            if let Ok(mut g) = s.lock() {
                g.clear();
            }
        }
    }
    if let Some(sh) = DERIVED.get() {
        for s in sh {
            if let Ok(mut g) = s.lock() {
                g.clear();
            }
        }
    }
}

// ─── Индексация каталога ─────────────────────────────────────────────────

/// Одна проба на каталог: регистронезависима ли его файловая система.
///
/// Берём первое имя, у которого смена регистра даёт ДРУГУЮ строку, и
/// спрашиваем ОС про изменённое написание. `cfg!`-догадка («Windows и macOS
/// всегда регистронезависимы») здесь не годится: на macOS бывают
/// регистро-чувствительные тома, а на Linux — смонтированные NTFS/exFAT.
///
/// Раньше проверки: если два РАЗНЫХ имени совпадают в нижнем регистре,
/// файловая система заведомо регистро-чувствительна — пробы не нужно.
fn probe_case_insensitive(dir: &Path, by_name: &HashMap<String, Ent>, lower_collided: bool) -> bool {
    if lower_collided {
        return false;
    }
    for name in by_name.keys() {
        let flipped = if name.chars().any(|c| c.is_ascii_lowercase()) {
            name.to_ascii_uppercase()
        } else {
            name.to_ascii_lowercase()
        };
        if flipped == *name {
            continue;
        }
        FS_STAT.fetch_add(1, Ordering::Relaxed);
        return std::fs::symlink_metadata(dir.join(&flipped)).is_ok();
    }
    // Каталог пуст либо все имена не содержат букв — вердикт не наблюдаем и
    // ни на что не влияет (регистро-поиск всё равно ничего не найдёт).
    false
}

fn build_dir_index(dir: &Path) -> DirIndex {
    FS_READ_DIR.fetch_add(1, Ordering::Relaxed);
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => {
            return DirIndex {
                exists: false,
                has_symlink: false,
                by_name: HashMap::new(),
                lower: HashMap::new(),
                case_insensitive: false,
                nv: Arc::new(Vec::new()),
            }
        }
    };
    let mut by_name: HashMap<String, Ent> = HashMap::new();
    let mut lower: HashMap<String, String> = HashMap::new();
    let mut lower_collided = false;
    let mut has_symlink = false;
    let mut nv: Vec<PathBuf> = Vec::new();
    for e in rd.filter_map(|e| e.ok()) {
        // Имя не в UTF-8 — в индекс не попадает: запрашиваемые пути
        // собираются из сегментов модуля (ASCII-идентификаторов), совпасть
        // с таким именем они не могут.
        let name = match e.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut symlink = false;
        let kind = match e.file_type() {
            Ok(t) if t.is_file() => Kind::File,
            Ok(t) if t.is_dir() => Kind::Dir,
            // Симлинк (и всё прочее): `file_type()` симлинк НЕ разыменовывает,
            // а `Path::is_file()`/`is_dir()` — разыменовывают. Спрашиваем ОС,
            // чтобы вид совпал с сегодняшним ответом.
            t => {
                symlink = t.map(|t| t.is_symlink()).unwrap_or(false);
                has_symlink |= symlink;
                FS_STAT.fetch_add(1, Ordering::Relaxed);
                match std::fs::metadata(e.path()) {
                    Ok(m) if m.is_file() => Kind::File,
                    Ok(m) if m.is_dir() => Kind::Dir,
                    _ => Kind::Other,
                }
            }
        };
        if kind == Kind::File
            && Path::new(&name).extension().and_then(|s| s.to_str()) == Some("nv")
        {
            nv.push(dir.join(&name));
        }
        let lc = name.to_lowercase();
        if lower.insert(lc, name.clone()).is_some() {
            lower_collided = true;
        }
        by_name.insert(name, Ent { kind, symlink });
    }
    nv.sort();
    let case_insensitive = probe_case_insensitive(dir, &by_name, lower_collided);
    DirIndex {
        exists: true,
        has_symlink,
        by_name,
        lower,
        case_insensitive,
        nv: Arc::new(nv),
    }
}

/// **Одна работа на ключ.** Замок шарда держится НА ВРЕМЯ чтения, а не
/// только на время вставки. Иначе 16 воркеров `nova test`, стартуя
/// одновременно, промахиваются мимо пустой карты каждый сам и читают один и
/// тот же каталог по 3-4 раза — замерено счётчиком: 238 `read_dir` на 87
/// каталогов. Работы это не портило, но обращения к ФС множило, а мерило
/// Ф.2 — именно они. Вложенных обращений под этим замком нет (только вызовы
/// `std::fs`), поэтому взаимоблокировка невозможна; шардов 32.
fn dir_index(dir: &Path) -> Arc<DirIndex> {
    let sh = &shards(&DIRS)[shard_of(dir)];
    match sh.lock() {
        Ok(mut g) => {
            if let Some(v) = g.get(dir) {
                return Arc::clone(v);
            }
            let built = Arc::new(build_dir_index(dir));
            g.insert(dir.to_path_buf(), Arc::clone(&built));
            built
        }
        Err(_) => Arc::new(build_dir_index(dir)),
    }
}

/// Родитель для поиска по имени. Пустой родитель (`"a.nv"`) — текущий каталог.
fn parent_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Запись каталога для `path` — вместе с ИМЕНЕМ, как оно лежит на диске
/// (оно и есть «объявленный регистр имени» из шага 1 алгоритма Ф.2).
fn lookup_entry(path: &Path) -> Option<(String, Ent)> {
    // Пути, оканчивающиеся на `..`/`.`/корень, именем не адресуются —
    // спрашиваем ОС напрямую (такие пути приходят только из `path`-записей
    // `nova.toml` и встречаются считаные разы за прогон).
    let name = path.file_name()?.to_str()?;
    let idx = dir_index(&parent_of(path));
    if !idx.exists {
        return None;
    }
    if let Some(e) = idx.by_name.get(name) {
        return Some((name.to_string(), *e));
    }
    if idx.case_insensitive {
        if let Some(actual) = idx.lower.get(&name.to_lowercase()) {
            return idx.by_name.get(actual).map(|e| (actual.clone(), *e));
        }
    }
    None
}

fn lookup_kind(path: &Path) -> Option<Kind> {
    lookup_entry(path).map(|(_, e)| e.kind)
}

/// Имя `path`, как оно записано на диске. Шаг 3 алгоритма Ф.2: сверка
/// регистра сравнивает запрошенный сегмент с этим полем, а не канонизирует
/// путь заново на каждый импорт.
///
/// `None` — записи нет, снимок выключен, либо путь именем не адресуется.
pub fn on_disk_name(path: &Path) -> Option<String> {
    if !on() {
        return None;
    }
    lookup_entry(path).map(|(n, _)| n)
}

/// В каталоге `dir` есть символическая ссылка. Единственный потребитель —
/// сверка регистра: при ссылке она откатывается на `fs::canonicalize`, чтобы
/// не разойтись с сегодняшним ответом (ссылка показывает имя ЦЕЛИ).
pub fn dir_has_symlink(dir: &Path) -> bool {
    if !on() {
        return true;
    }
    dir_index(dir).has_symlink
}

/// Прямой ответ ОС — для путей, которые именем не адресуются.
fn direct_kind(path: &Path) -> Option<Kind> {
    FS_STAT.fetch_add(1, Ordering::Relaxed);
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => Some(Kind::File),
        Ok(m) if m.is_dir() => Some(Kind::Dir),
        Ok(_) => Some(Kind::Other),
        Err(_) => None,
    }
}

fn kind_of(path: &Path) -> Option<Kind> {
    if !on() {
        return direct_kind(path);
    }
    if path.file_name().and_then(|s| s.to_str()).is_none() {
        return direct_kind(path);
    }
    lookup_kind(path)
}

// ─── Публичный интерфейс ─────────────────────────────────────────────────

/// Тот же ответ, что `Path::is_file()`, без обращения к ФС.
pub fn is_file(path: &Path) -> bool {
    kind_of(path) == Some(Kind::File)
}

/// Тот же ответ, что `Path::is_dir()`, без обращения к ФС.
pub fn is_dir(path: &Path) -> bool {
    kind_of(path) == Some(Kind::Dir)
}

/// Тот же ответ, что `Path::exists()`, без обращения к ФС.
pub fn exists(path: &Path) -> bool {
    kind_of(path).is_some()
}

/// Обычные `.nv`-файлы непосредственно в `dir`, отсортированы по пути.
/// Пустой вектор — каталога нет либо в нём нет `.nv`.
pub fn nv_files(dir: &Path) -> Arc<Vec<PathBuf>> {
    if !on() {
        let mut out: Vec<PathBuf> = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("nv"))
                .collect(),
            Err(_) => Vec::new(),
        };
        out.sort();
        return Arc::new(out);
    }
    Arc::clone(&dir_index(dir).nv)
}

/// Содержимое `.nv`-файла. `None` ровно там, где `fs::read_to_string` дал бы
/// ошибку (нет файла, нет прав, не UTF-8).
pub fn file_text(path: &Path) -> Option<Arc<String>> {
    if !on() {
        FS_READ_FILE.fetch_add(1, Ordering::Relaxed);
        return std::fs::read_to_string(path).ok().map(Arc::new);
    }
    // Одна работа на ключ — см. `dir_index`.
    let sh = &shards(&FILES)[shard_of(path)];
    match sh.lock() {
        Ok(mut g) => {
            if let Some(v) = g.get(path) {
                return v.clone();
            }
            FS_READ_FILE.fetch_add(1, Ordering::Relaxed);
            let text = std::fs::read_to_string(path).ok().map(Arc::new);
            g.insert(path.to_path_buf(), text.clone());
            text
        }
        Err(_) => {
            FS_READ_FILE.fetch_add(1, Ordering::Relaxed);
            std::fs::read_to_string(path).ok().map(Arc::new)
        }
    }
}

/// Сколько байт файла достаточно, чтобы увидеть объявление `module`.
///
/// Шаг 1 алгоритма Ф.2 требует читать ТОЛЬКО заголовок: чтение файлов
/// целиком ради одной строки `module` и стоило 2459 с в замере Ф.1.
/// Объявление обязано стоять первой значащей строкой (`scan_module_decl`)
/// либо в первых 200 строках после комментариев и атрибутов
/// (`extract_declared_module`); 64 KiB покрывает и то и другое с запасом.
const HEADER_BYTES: usize = 64 * 1024;

/// Заголовок `.nv`-файла — префикс, которого гарантированно хватает для
/// сканера объявления `module`.
///
/// **Тот же ответ, что у полного чтения.** Если объявление в префикс не
/// уложилось (файл длиннее `HEADER_BYTES` и сканер вернул `None`), вызывающий
/// обязан переспросить [`file_text`]; для этого возвращается флаг
/// `truncated`. Так свойство «результат совпадает с полным чтением»
/// доказуемо, а не «на практике хватает».
pub fn header_text(path: &Path) -> Option<(Arc<String>, bool)> {
    use std::io::Read;
    // Одна работа на ключ — см. `dir_index`.
    let guard = if on() {
        match shards(&HEADERS)[shard_of(path)].lock() {
            Ok(mut g) => {
                if let Some(v) = g.get(path) {
                    return v.clone();
                }
                Some(g)
            }
            Err(_) => None,
        }
    } else {
        None
    };
    FS_READ_FILE.fetch_add(1, Ordering::Relaxed);
    let res = (|| {
        let mut f = std::fs::File::open(path).ok()?;
        let mut buf = vec![0u8; HEADER_BYTES];
        let mut filled = 0usize;
        loop {
            match f.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => {
                    filled += n;
                    if filled == HEADER_BYTES {
                        break;
                    }
                }
                Err(_) => return None,
            }
        }
        let truncated = filled == HEADER_BYTES;
        // Обрезка могла разрубить многобайтовый символ — берём только
        // корректный префикс. Для сканера `module` этого достаточно, а
        // «не-UTF-8 → None» сохраняется: если корректен ноль байт при
        // непустом файле, ответ пустой строкой сканер отвергнет так же.
        let s = match std::str::from_utf8(&buf[..filled]) {
            Ok(s) => s.to_string(),
            Err(e) if truncated => String::from_utf8_lossy(&buf[..e.valid_up_to()]).into_owned(),
            Err(_) => return None,
        };
        Some((Arc::new(s), truncated))
    })();
    if let Some(mut g) = guard {
        g.insert(path.to_path_buf(), res.clone());
    }
    res
}

/// `fs::canonicalize`, посчитанный по разу на путь.
pub fn canonicalize(path: &Path) -> Option<PathBuf> {
    if !on() {
        FS_CANON.fetch_add(1, Ordering::Relaxed);
        return std::fs::canonicalize(path).ok();
    }
    // Одна работа на ключ — см. `dir_index`.
    let sh = &shards(&CANON)[shard_of(path)];
    match sh.lock() {
        Ok(mut g) => {
            if let Some(v) = g.get(path) {
                return v.clone();
            }
            FS_CANON.fetch_add(1, Ordering::Relaxed);
            let canon = std::fs::canonicalize(path).ok();
            g.insert(path.to_path_buf(), canon.clone());
            canon
        }
        Err(_) => {
            FS_CANON.fetch_add(1, Ordering::Relaxed);
            std::fs::canonicalize(path).ok()
        }
    }
}

/// Вывод, зависящий только от содержимого каталога (объявления `module`
/// соседей, вердикт «папка-модуль»). Считается по разу на пару
/// `(каталог, tag)`; `tag` разделяет разные выводы по одному каталогу.
pub fn derived<T, F>(dir: &Path, tag: &'static str, compute: F) -> Arc<T>
where
    T: Send + Sync + 'static,
    F: FnOnce() -> T,
{
    if !on() {
        return Arc::new(compute());
    }
    let sh = &derived_shards()[shard_of(dir)];
    if let Ok(g) = sh.lock() {
        if let Some(v) = g.get(dir).and_then(|m| m.get(tag)) {
            if let Ok(t) = Arc::clone(v).downcast::<T>() {
                return t;
            }
        }
    }
    let value: Arc<T> = Arc::new(compute());
    if let Ok(mut g) = sh.lock() {
        let slot = g.entry(dir.to_path_buf()).or_default();
        let stored = slot
            .entry(tag)
            .or_insert_with(|| Arc::clone(&value) as Arc<dyn std::any::Any + Send + Sync>);
        if let Ok(t) = Arc::clone(stored).downcast::<T>() {
            return t;
        }
    }
    value
}

/// То же, что [`derived`], но ключ — произвольный путь (файл), а не каталог.
/// Нужен для выводов, зависящих от одного файла и его каталога: например
/// ключ реестра модулей (`canonical_module_key`).
pub fn derived_for_path<T, F>(path: &Path, tag: &'static str, compute: F) -> Arc<T>
where
    T: Send + Sync + 'static,
    F: FnOnce() -> T,
{
    derived(path, tag, compute)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тесты правят ГЛОБАЛЬНЫЙ режим снимка и общий индекс, поэтому идут по
    /// одному: без этого `cargo test` (многопоточный по умолчанию) сбрасывал
    /// бы индекс из-под соседнего теста.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "nova_p252_index_{}_{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Снимок неизменен по построению: при ВКЛЮЧЁННОМ снимке правка файла на
    /// диске не видна — и это заявленное свойство, а не дефект.
    #[test]
    fn snapshot_is_immutable_within_a_run() {
        let _s = serial();
        let d = tmp_dir("immutable");
        let f = d.join("a.nv");
        std::fs::write(&f, "module a\n").unwrap();
        reset();
        enable_snapshot();
        assert_eq!(file_text(&f).unwrap().as_str(), "module a\n");
        std::fs::write(&f, "module a_changed\n").unwrap();
        assert_eq!(
            file_text(&f).unwrap().as_str(),
            "module a\n",
            "снимок обязан быть неизменен в пределах прогона"
        );
        disable_snapshot();
        // Новый прогон (снимок пересобран) обязан видеть текущий диск.
        assert_eq!(file_text(&f).unwrap().as_str(), "module a_changed\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Ключевое мерило Ф.2: после того как каталог проиндексирован, ответы
    /// `is_file`/`is_dir`/`nv_files` не стоят НИ ОДНОГО обращения к ФС.
    #[test]
    fn zero_fs_calls_after_index_is_built() {
        let _s = serial();
        let d = tmp_dir("zerofs");
        std::fs::write(d.join("a.nv"), "module a\n").unwrap();
        std::fs::create_dir_all(d.join("sub")).unwrap();
        reset();
        enable_snapshot();
        // Прогрев: один `read_dir` (+ возможная проба регистра).
        assert!(is_file(&d.join("a.nv")));
        let before = fs_calls();
        for _ in 0..1000 {
            assert!(is_file(&d.join("a.nv")));
            assert!(is_dir(&d.join("sub")));
            assert!(!is_file(&d.join("nope.nv")));
            assert!(!is_dir(&d.join("nope")));
            assert_eq!(nv_files(&d).len(), 1);
        }
        let after = fs_calls();
        assert_eq!(
            after, before,
            "5000 ответов индекса стоили {} обращений к ФС вместо нуля",
            after - before
        );
        disable_snapshot();
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Отсутствующий файл — «нет», а не «нашли в карте». Проба «подсунь
    /// негодное» в её машинной части.
    #[test]
    fn missing_file_is_missing() {
        let _s = serial();
        let d = tmp_dir("missing");
        std::fs::write(d.join("a.nv"), "module a\n").unwrap();
        reset();
        enable_snapshot();
        assert!(is_file(&d.join("a.nv")));
        assert!(!is_file(&d.join("gone.nv")));
        assert!(file_text(&d.join("gone.nv")).is_none());
        disable_snapshot();
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Индекс обязан отвечать про регистр так же, как ОС: на
    /// регистронезависимой ФС `A.NV` находит `a.nv`, на
    /// регистро-чувствительной — не находит. Сверяется с живым `Path::is_file`,
    /// а не с `cfg!`.
    #[test]
    fn case_lookup_matches_the_os() {
        let _s = serial();
        let d = tmp_dir("case");
        let f = d.join("abc.nv");
        std::fs::write(&f, "module abc\n").unwrap();
        let upper = d.join("ABC.NV");
        let os_says = upper.is_file();
        reset();
        enable_snapshot();
        assert_eq!(
            is_file(&upper),
            os_says,
            "индекс разошёлся с ОС по регистру для {:?}",
            upper
        );
        disable_snapshot();
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Выключенный снимок — сквозной проход: правки видны сразу.
    #[test]
    fn pass_through_sees_edits() {
        let _s = serial();
        let d = tmp_dir("passthru");
        let f = d.join("a.nv");
        std::fs::write(&f, "one\n").unwrap();
        reset();
        disable_snapshot();
        assert_eq!(file_text(&f).unwrap().as_str(), "one\n");
        std::fs::write(&f, "two\n").unwrap();
        assert_eq!(file_text(&f).unwrap().as_str(), "two\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Вывод по каталогу считается один раз.
    #[test]
    fn derived_computed_once() {
        let _s = serial();
        let d = tmp_dir("derived");
        std::fs::write(d.join("a.nv"), "module a\n").unwrap();
        reset();
        enable_snapshot();
        let calls = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..10 {
            let v: Arc<usize> = derived(&d, "probe", || {
                calls.fetch_add(1, Ordering::Relaxed);
                nv_files(&d).len()
            });
            assert_eq!(*v, 1);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1, "вывод по каталогу пересчитан");
        disable_snapshot();
        let _ = std::fs::remove_dir_all(&d);
    }
}
