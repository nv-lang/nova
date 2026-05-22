//! Plan 81 Ф.9 — content-addressed build cache.
//!
//! `nova build` каждый раз заново прогоняет весь Rust-side пайплайн —
//! type-check, effect-inference, desugaring, call-normalization,
//! codegen — для всех импортированных модулей. Этот кэш устраняет
//! повторную работу: при байт-идентичных входах сгенерированный C
//! (`.c`) переиспользуется напрямую.
//!
//! **Что кэшируется — сгенерированный `.c` целиком.** Nova использует
//! inline-expansion (`resolve_imports_inline` сливает entry + все
//! импортированные модули в один `Module` → один `.c`); раздельной
//! компиляции модулей в архитектуре нет, поэтому естественная
//! гранулярность кэша — сборка целиком, а не отдельный модуль.
//! (Module-granular инкрементальная сборка потребовала бы separate
//! compilation — это отдельная крупная архитектурная работа, не v1;
//! план Ф.9 это и отмечал как «не v1».)
//!
//! **Ключ** = хэш(содержимое всех исходных файлов сборки + отпечаток
//! исполняемого файла компилятора Nova + активные `#cfg`-features +
//! target OS + mono-depth). `.c` НЕ зависит от C-тулчейна — поэтому
//! ключ его не включает; clang запускается всегда, и его обновления
//! применяются немедленно (нет риска устаревшего бинарника после
//! апгрейда clang). Любое изменение входа → другой ключ → промах →
//! полная пересборка. Кэш пишется ТОЛЬКО после успешной сборки —
//! значит попадание гарантирует, что вход байт-в-байт совпадает с
//! ранее успешно собранным (и, следовательно, успешно прошедшим
//! type-check).
//!
//! Хранилище: `<repo>/target/.nova-cache/<key>.c`. v1 без eviction —
//! каталог можно удалить вручную (`target/` — disposable). Отключается
//! через `NOVA_CACHE=0`; пропускается при `--keep-artifacts` (там
//! нужен полный набор промежуточных артефактов реальной сборки).

use nova_codegen::ast::PeerFile;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Каталог кэша: `<repo>/target/.nova-cache/`.
pub fn cache_dir(repo: &Path) -> PathBuf {
    repo.join("target").join(".nova-cache")
}

/// Активен ли кэш сборки? Включён по умолчанию; отключается
/// `NOVA_CACHE=0` (также принимаются `off` / `false`).
pub fn cache_enabled() -> bool {
    !matches!(std::env::var("NOVA_CACHE").as_deref(),
              Ok("0") | Ok("off") | Ok("false"))
}

/// Вычислить content-addressed ключ для сгенерированного `.c`.
///
/// Возвращает `None`, если отпечатать компилятор или прочитать какой-то
/// исходный файл не удалось — тогда caller выполняет обычную полную
/// сборку без кэша (graceful — кэш только оптимизация).
pub fn compute_c_key(
    peer_files: &[PeerFile],
    features: &[String],
    target_os: &str,
    mono_depth: Option<usize>,
) -> Option<String> {
    let mut h = DefaultHasher::new();
    // Версия схемы ключа — смена формата кэша инвалидирует все записи.
    "nova-c-cache-v1".hash(&mut h);

    // Отпечаток компилятора Nova: пересборка компилятора меняет
    // кодогенерацию и ОБЯЗАНА инвалидировать кэш. mtime+size надёжно
    // меняются при любой пересборке исполняемого файла.
    let exe = std::env::current_exe().ok()?;
    let meta = std::fs::metadata(&exe).ok()?;
    meta.len().hash(&mut h);
    if let Ok(d) = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH) {
        d.as_nanos().hash(&mut h);
    }

    // cfg-вход: активные features (отсортированы для детерминизма) +
    // target OS + предел мономорфизации.
    let mut feats: Vec<&String> = features.iter().collect();
    feats.sort();
    feats.len().hash(&mut h);
    for f in feats {
        f.hash(&mut h);
    }
    target_os.hash(&mut h);
    mono_depth.hash(&mut h);

    // Каждый исходный файл сборки: путь + содержимое. Сортировка по
    // пути — детерминированный порядок независимо от обхода резолвера.
    let mut files: Vec<&PeerFile> = peer_files.iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.len().hash(&mut h);
    for pf in files {
        pf.path.to_string_lossy().hash(&mut h);
        let bytes = std::fs::read(&pf.path).ok()?;
        bytes.hash(&mut h);
    }
    Some(format!("{:016x}", h.finish()))
}

/// Прочитать закэшированный `.c` по ключу. `None` — промах.
pub fn load_c(repo: &Path, key: &str) -> Option<String> {
    let path = cache_dir(repo).join(format!("{}.c", key));
    std::fs::read_to_string(&path).ok()
}

/// Сохранить сгенерированный `.c` в кэш. Best-effort: ошибки
/// проглатываются — отсутствие кэша не должно ронять сборку. Запись
/// атомарна (через temp-файл + rename), чтобы параллельные сборки не
/// прочитали наполовину записанный файл.
pub fn store_c(repo: &Path, key: &str, c_code: &str) {
    let dir = cache_dir(repo);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let final_path = dir.join(format!("{}.c", key));
    let tmp_path = dir.join(format!("{}.{}.tmp", key, std::process::id()));
    if std::fs::write(&tmp_path, c_code).is_ok() {
        if std::fs::rename(&tmp_path, &final_path).is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_peer(path: PathBuf) -> PeerFile {
        PeerFile {
            path,
            file_id: 0,
            imports: Vec::new(),
            items_here: Vec::new(),
            imported_item_names: std::collections::HashSet::new(),
            is_entry_module: true,
            module_name: vec!["t".to_string()],
        }
    }

    fn tmp_root(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "nova_p81_f9_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn key_is_stable_and_content_sensitive() {
        let root = tmp_root("key");
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("a.nv");
        std::fs::write(&a, "module t\nfn main() -> int => 0\n").unwrap();
        let peers = vec![mk_peer(a.clone())];

        let k1 = compute_c_key(&peers, &[], "windows", None).expect("key");
        let k2 = compute_c_key(&peers, &[], "windows", None).expect("key");
        assert_eq!(k1, k2, "identical inputs → identical key");

        // Изменение содержимого файла → другой ключ.
        std::fs::write(&a, "module t\nfn main() -> int => 1\n").unwrap();
        let k3 = compute_c_key(&peers, &[], "windows", None).expect("key");
        assert_ne!(k1, k3, "changed source content → different key");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn key_reflects_cfg_inputs() {
        let root = tmp_root("cfg");
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("a.nv");
        std::fs::write(&a, "module t\nfn main() -> int => 0\n").unwrap();
        let peers = vec![mk_peer(a)];

        let base = compute_c_key(&peers, &[], "windows", None).unwrap();
        let other_target = compute_c_key(&peers, &[], "linux", None).unwrap();
        let with_feat =
            compute_c_key(&peers, &["z3".to_string()], "windows", None).unwrap();
        let other_depth = compute_c_key(&peers, &[], "windows", Some(900)).unwrap();
        assert_ne!(base, other_target, "target OS is part of the key");
        assert_ne!(base, with_feat, "active features are part of the key");
        assert_ne!(base, other_depth, "mono-depth is part of the key");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn store_then_load_roundtrip() {
        let root = tmp_root("store");
        let key = "deadbeefcafef00d";
        assert!(load_c(&root, key).is_none(), "miss before store");
        store_c(&root, key, "/* generated C */\nint main(){return 0;}\n");
        let loaded = load_c(&root, key).expect("hit after store");
        assert!(loaded.contains("int main()"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
