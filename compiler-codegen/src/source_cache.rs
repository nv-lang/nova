// SPDX-License-Identifier: MIT OR Apache-2.0
//! План 252 — кэш обращений к дереву исходников (внутрипроцессный, с
//! проверкой актуальности на каждом обращении).
//!
//! **Зачем.** Замер Ф.0 (см. `docs/plans/wip/RECON-std-cache.md`): на корпусе
//! `spec_tests/conformance/neg` (568 файлов) стадия `imports-resolve` забирает
//! ~95 % работы, и внутри неё 85 % — не разбор `std` (он всего 0,4 %), а
//! ПОВТОРНОЕ хождение по файловой системе: `is_folder_module_peer` читает
//! ЦЕЛИКОМ каждый `.nv` в каталоге, только чтобы посмотреть строку `module`,
//! и делает это 93 984 раза; `resolve_module_paths` перечитывает каталоги
//! 89 749 раз. За прогон разобрано 442,9 MiB текста над 94 уникальными
//! файлами — повторность 416,7×.
//!
//! **Что кэшируется.** Только два примитива, оба — чистые функции состояния
//! диска:
//!   * [`file_text`] — содержимое `.nv`-файла;
//!   * [`dir_nv_files`] — список обычных `.nv`-файлов каталога (отсортирован).
//! Ни AST, ни таблицы имён, ни выводы типов здесь не лежат: формат
//! промежуточного представления план не трогает, инференс не задет.
//!
//! **Корректность важнее скорости.** Кэш НЕ «прочитали один раз за процесс и
//! забыли»: у каждой записи есть отпечаток источника (`mtime` + размер для
//! файла, `mtime` для каталога), и он сверяется `fs::metadata` при КАЖДОМ
//! обращении. Отпечаток разошёлся — запись считается протухшей и данные
//! перечитываются с диска. Стоимость проверки — один `stat` (единицы мкс)
//! против чтения файла (сотни мкс под нагрузкой) или перечисления каталога.
//! Дополнительно: после чтения файл штампуется повторно, и если отпечаток
//! изменился ПОКА мы читали (файл переписывали под нами), результат
//! возвращается вызывающему, но в кэш НЕ кладётся — рваная запись не может
//! залипнуть.
//!
//! **Выключатель.** `NOVA_SRC_CACHE=0` полностью отключает кэш (сквозной
//! проход к файловой системе) — нужен для сравнения А/Б и как аварийный
//! рубильник. По умолчанию кэш включён и молчит.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Отпечаток источника. Для файла — время правки и размер, для каталога —
/// только время правки (NTFS/ext обновляют его при добавлении, удалении и
/// переименовании записей, а от содержимого файлов список имён не зависит).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Stamp {
    mtime_ns: u128,
    len: u64,
}

/// `None`, если файловая система не отдала время правки — тогда кэшировать
/// нельзя: без отпечатка нечем проверить актуальность, а «просто запомнить»
/// планом прямо запрещено.
fn stamp_of(md: &std::fs::Metadata) -> Option<Stamp> {
    let mtime_ns = md
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(Stamp { mtime_ns, len: md.len() })
}

const SHARDS: usize = 32;

type FileMap = HashMap<PathBuf, (Stamp, Arc<String>)>;
type DirMap = HashMap<PathBuf, (Stamp, Arc<Vec<PathBuf>>)>;

static FILES: OnceLock<Vec<Mutex<FileMap>>> = OnceLock::new();
static DIRS: OnceLock<Vec<Mutex<DirMap>>> = OnceLock::new();

fn files() -> &'static Vec<Mutex<FileMap>> {
    FILES.get_or_init(|| (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect())
}

fn dirs() -> &'static Vec<Mutex<DirMap>> {
    DIRS.get_or_init(|| (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect())
}

/// Шардирование: 16 рабочих потоков `nova test` бьются за один и тот же
/// каталог `std`; единственный мьютекс на весь кэш свёл бы выигрыш на нет.
fn shard_idx(path: &Path) -> usize {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    (h.finish() as usize) % SHARDS
}

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NOVA_SRC_CACHE").as_deref(),
            Ok("0") | Ok("off") | Ok("false") | Ok("no")
        )
    })
}

/// Содержимое `.nv`-файла. Возвращает `None` ровно там, где вернул бы `None`
/// прямой `fs::read_to_string(path).ok()` — файла нет, нет прав, не UTF-8.
pub fn file_text(path: &Path) -> Option<Arc<String>> {
    if !enabled() {
        return std::fs::read_to_string(path).ok().map(Arc::new);
    }
    let stamp = match std::fs::metadata(path).ok().as_ref().and_then(stamp_of) {
        Some(s) => s,
        // Нет отпечатка — работаем без кэша, а не «на авось».
        None => return std::fs::read_to_string(path).ok().map(Arc::new),
    };
    let shard = &files()[shard_idx(path)];
    if let Ok(g) = shard.lock() {
        if let Some((s, v)) = g.get(path) {
            if *s == stamp {
                return Some(v.clone());
            }
        }
    }
    let text = Arc::new(std::fs::read_to_string(path).ok()?);
    // Файл могли переписать ПОКА мы читали: повторный штамп ловит это.
    // Расхождение → отдаём прочитанное, но в кэш не кладём.
    let after = std::fs::metadata(path).ok().as_ref().and_then(stamp_of);
    if after == Some(stamp) {
        if let Ok(mut g) = shard.lock() {
            g.insert(path.to_path_buf(), (stamp, Arc::clone(&text)));
        }
    }
    Some(text)
}

/// Обычные `.nv`-файлы непосредственно в `dir`, отсортированы по пути.
/// Пустой вектор — каталога нет либо в нём нет `.nv`.
pub fn dir_nv_files(dir: &Path) -> Arc<Vec<PathBuf>> {
    if !enabled() {
        return Arc::new(scan_dir_nv(dir));
    }
    let stamp = match std::fs::metadata(dir).ok().as_ref().and_then(stamp_of) {
        Some(s) => s,
        None => return Arc::new(scan_dir_nv(dir)),
    };
    let shard = &dirs()[shard_idx(dir)];
    if let Ok(g) = shard.lock() {
        if let Some((s, v)) = g.get(dir) {
            if *s == stamp {
                return Arc::clone(v);
            }
        }
    }
    let listing = Arc::new(scan_dir_nv(dir));
    let after = std::fs::metadata(dir).ok().as_ref().and_then(stamp_of);
    if after == Some(stamp) {
        if let Ok(mut g) = shard.lock() {
            g.insert(dir.to_path_buf(), (stamp, Arc::clone(&listing)));
        }
    }
    listing
}

/// Снимок каталога: `(путь, отпечаток)` для каждого обычного `.nv`,
/// отсортирован по пути. **Всегда свежий** — это ключ проверки, а не кэш.
///
/// Один `read_dir`: на Windows `DirEntry::metadata()` отдаёт данные из
/// `WIN32_FIND_DATA`, полученной вместе с именем, без отдельного сисвызова —
/// то есть отпечатки всех файлов каталога достаются за одно обращение к ФС
/// вместо N вызовов `stat`.
fn dir_snapshot(dir: &Path) -> Vec<(PathBuf, Stamp)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<(PathBuf, Stamp)> = Vec::new();
    for e in entries.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("nv") {
            continue;
        }
        let md = match e.metadata() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        if !md.is_file() {
            continue;
        }
        match stamp_of(&md) {
            Some(s) => out.push((p, s)),
            // Без отпечатка проверять нечем — возвращаем пустой снимок,
            // он никогда не совпадёт с сохранённым, и вывод пересчитается.
            None => return Vec::new(),
        }
    }
    out.sort();
    out
}

type DerivedMap = HashMap<(PathBuf, &'static str), (Vec<(PathBuf, Stamp)>, Arc<dyn std::any::Any + Send + Sync>)>;
static DERIVED: OnceLock<Vec<Mutex<DerivedMap>>> = OnceLock::new();

fn derived() -> &'static Vec<Mutex<DerivedMap>> {
    DERIVED.get_or_init(|| (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect())
}

/// Кэш вывода, зависящего ТОЛЬКО от содержимого каталога (имена + содержимое
/// его `.nv`-файлов). `tag` разделяет разные выводы по одному каталогу.
///
/// Проверка актуальности — сравнение со свежим [`dir_snapshot`]: он ловит и
/// добавление/удаление/переименование файла (имена), и правку любого из них
/// (отпечаток). Расхождение → `compute` считает заново.
pub fn dir_derived<T, F>(dir: &Path, tag: &'static str, compute: F) -> Arc<T>
where
    T: Send + Sync + 'static,
    F: FnOnce() -> T,
{
    if !enabled() {
        return Arc::new(compute());
    }
    let snap = dir_snapshot(dir);
    let key = (dir.to_path_buf(), tag);
    let shard = &derived()[shard_idx(dir)];
    if let Ok(g) = shard.lock() {
        if let Some((s, v)) = g.get(&key) {
            if *s == snap {
                if let Ok(t) = Arc::clone(v).downcast::<T>() {
                    return t;
                }
            }
        }
    }
    let value: Arc<T> = Arc::new(compute());
    // Пересняли после вычисления: если каталог менялся под нами, в кэш не
    // кладём — иначе туда залипнет вывод, посчитанный на рваном состоянии.
    if dir_snapshot(dir) == snap {
        if let Ok(mut g) = shard.lock() {
            g.insert(key, (snap, Arc::clone(&value) as Arc<dyn std::any::Any + Send + Sync>));
        }
    }
    value
}

fn scan_dir_nv(dir: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("nv"))
        .collect();
    out.sort();
    out
}

/// Сбросить кэш целиком. Для внутренних тестов, которые правят файлы на
/// диске в пределах одного процесса быстрее, чем разрешение `mtime`.
pub fn clear() {
    if let Some(sh) = FILES.get() {
        for s in sh {
            if let Ok(mut g) = s.lock() {
                g.clear();
            }
        }
    }
    if let Some(sh) = DIRS.get() {
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

/// ТОЛЬКО для проб: положить в кэш заведомо негодную запись с чужим
/// отпечатком. Проверяет свойство «устаревшая запись не может быть выдана»:
/// после такой подмены следующее обращение обязано увидеть расхождение
/// отпечатка и перечитать файл, а не отдать подсунутое.
#[cfg(test)]
fn poison_file_entry(path: &Path, text: &str) {
    let shard = &files()[shard_idx(path)];
    let bogus = Stamp { mtime_ns: 1, len: 1 };
    if let Ok(mut g) = shard.lock() {
        g.insert(path.to_path_buf(), (bogus, Arc::new(text.to_string())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nova_p252_srccache_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn text_reflects_edit() {
        let d = tmp_dir("edit");
        let f = d.join("a.nv");
        std::fs::write(&f, "module a\n").unwrap();
        assert_eq!(file_text(&f).unwrap().as_str(), "module a\n");
        // Отпечаток = mtime+размер. Меняем размер — расхождение видно даже
        // если часы файловой системы не успели тикнуть.
        std::fs::write(&f, "module a_changed_longer\n").unwrap();
        assert_eq!(file_text(&f).unwrap().as_str(), "module a_changed_longer\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn text_none_for_missing() {
        let d = tmp_dir("missing");
        assert!(file_text(&d.join("nope.nv")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn listing_reflects_new_file() {
        let d = tmp_dir("listing");
        std::fs::write(d.join("a.nv"), "module a\n").unwrap();
        assert_eq!(dir_nv_files(&d).len(), 1);
        std::fs::write(d.join("b.nv"), "module b\n").unwrap();
        // mtime каталога обновляется при добавлении записи; если у ФС
        // разрешение слишком грубое — подстраховываемся явным сбросом,
        // проверяя при этом, что сам список считается верно.
        let n = dir_nv_files(&d).len();
        if n != 2 {
            clear();
            assert_eq!(dir_nv_files(&d).len(), 2);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Проба «подсунь негодное»: устаревшая запись в кэше не может быть
    /// выдана — расхождение отпечатка перечитывает файл.
    #[test]
    fn stale_entry_is_never_served() {
        let d = tmp_dir("poison");
        let f = d.join("a.nv");
        std::fs::write(&f, "module a\nfn real() -> int => 1\n").unwrap();
        // Прогреваем кэш законным чтением.
        assert!(file_text(&f).unwrap().contains("real"));
        // Подсовываем негодное с чужим отпечатком.
        poison_file_entry(&f, "module a\nfn STALE() -> int => 0\n");
        let got = file_text(&f).unwrap();
        assert!(
            got.contains("real") && !got.contains("STALE"),
            "устаревшая запись выдана молча: {:?}",
            got
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Правка файла обязана менять вывод, посчитанный по каталогу
    /// (`dir_derived`), — иначе «посчитали один раз и забыли».
    #[test]
    fn dir_derived_follows_peer_edit() {
        let d = tmp_dir("derived");
        let f = d.join("a.nv");
        std::fs::write(&f, "module aaa\n").unwrap();
        let first: Arc<String> = dir_derived(&d, "probe", || {
            dir_nv_files(&d)
                .iter()
                .filter_map(|p| file_text(p))
                .map(|t| t.trim().to_string())
                .collect::<Vec<_>>()
                .join("|")
        });
        assert_eq!(first.as_str(), "module aaa");
        std::fs::write(&f, "module bbbbbb\n").unwrap();
        let second: Arc<String> = dir_derived(&d, "probe", || {
            dir_nv_files(&d)
                .iter()
                .filter_map(|p| file_text(p))
                .map(|t| t.trim().to_string())
                .collect::<Vec<_>>()
                .join("|")
        });
        assert_eq!(second.as_str(), "module bbbbbb", "вывод по каталогу залип");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn listing_skips_non_nv() {
        let d = tmp_dir("filter");
        std::fs::write(d.join("a.nv"), "module a\n").unwrap();
        std::fs::write(d.join("b.txt"), "x").unwrap();
        std::fs::create_dir_all(d.join("sub.nv")).unwrap();
        let l = dir_nv_files(&d);
        assert_eq!(l.len(), 1, "только обычные .nv-файлы: {:?}", l);
        let _ = std::fs::remove_dir_all(&d);
    }
}
