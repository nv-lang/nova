//! Plan 33.3 Ф.12: Incremental verification cache.
//!
//! Сохраняет результат верификации для каждой функции в
//! `target/contracts-cache/<hash>.json`. При rebuild без изменений
//! контракт повторно не доказывается — используется кэшированный результат.
//!
//! Дизайн:
//! - Cache key: DefaultHasher по "module::fn_name + contract source text".
//!   Быстро, детерминированно в рамках одного Rust build (не между сессиями!
//!   std::hash нестабилен между запусками). Для стабильного хэша между сессиями
//!   используем FNV-1a (ручная реализация, без зависимостей).
//! - Cache value: JSON с result + solver + duration_ms + timestamp.
//! - Инвалидация: изменение текста контракта или тела функции меняет хэш.
//! - Параллельность: каждый файл-кэш пишется атомарно (rename trick).
//!
//! Ограничения MVP:
//! - Хэш строится из Pretty-print (не из AST структуры), поэтому
//!   пробел/комментарий не инвалидирует кэш — это by-design (белый шум).
//! - Нет transitive dependency tracking (план 33.5).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Результат кэша для одной функции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedResult {
    Proven,
    Unknown,
    Disproved,
}

impl CachedResult {
    fn as_str(&self) -> &'static str {
        match self {
            CachedResult::Proven => "proven",
            CachedResult::Unknown => "unknown",
            CachedResult::Disproved => "disproved",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "proven" => Some(CachedResult::Proven),
            "unknown" => Some(CachedResult::Unknown),
            "disproved" => Some(CachedResult::Disproved),
            _ => None,
        }
    }
}

/// Incremental cache. Создаётся один раз на pipeline session.
pub struct ContractCache {
    /// Директория для файлов кэша.
    cache_dir: PathBuf,
    /// Включён ли кэш (выключен если `NOVA_CACHE=0` или dir недоступна).
    enabled: bool,
}

impl ContractCache {
    /// Создать cache в `<target_dir>/contracts-cache/`.
    pub fn new(target_dir: &Path) -> Self {
        let cache_dir = target_dir.join("contracts-cache");
        let enabled = if matches!(std::env::var("NOVA_CACHE").as_deref(),
                                  Ok("0") | Ok("off") | Ok("false")) {
            false
        } else {
            match fs::create_dir_all(&cache_dir) {
                Ok(_) => true,
                Err(_) => false,
            }
        };
        Self { cache_dir, enabled }
    }

    /// Нет-оп кэш для случаев когда кэш не нужен.
    pub fn disabled() -> Self {
        Self {
            cache_dir: PathBuf::new(),
            enabled: false,
        }
    }

    /// Lookup кэша. `None` = cache miss или кэш выключен.
    pub fn lookup(&self, key: u64) -> Option<CachedResult> {
        if !self.enabled { return None; }
        let path = self.entry_path(key);
        let text = fs::read_to_string(&path).ok()?;
        // Минимальный JSON parse — ищем "result":"<value>".
        let result_val = extract_json_str(&text, "result")?;
        CachedResult::from_str(result_val)
    }

    /// Сохранить результат в кэш.
    pub fn store(&self, key: u64, result: &CachedResult, fn_id: &str, duration_ms: u64) {
        if !self.enabled { return; }
        let path = self.entry_path(key);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let json = format!(
            "{{\n  \"fn_id\": {},\n  \"input_hash\": \"fnv:{:016x}\",\n  \"result\": {},\n  \"duration_ms\": {},\n  \"timestamp\": {}\n}}\n",
            json_str(fn_id),
            key,
            json_str(result.as_str()),
            duration_ms,
            ts,
        );
        // Атомарная запись через временный файл + rename.
        let tmp_path = path.with_extension("tmp");
        if let Ok(mut f) = fs::File::create(&tmp_path) {
            if f.write_all(json.as_bytes()).is_ok() {
                let _ = fs::rename(&tmp_path, &path);
            }
        }
    }

    fn entry_path(&self, key: u64) -> PathBuf {
        self.cache_dir.join(format!("{:016x}.json", key))
    }
}

/// FNV-1a 64-bit hash — стабильный между запусками (в отличие от SipHash).
pub fn fnv1a_hash(data: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Построить cache key для функции.
/// Key = FNV1a("module::fn_name\0" + текст всех контрактов + "|" + текст тела).
pub fn cache_key(module_name: &str, fn_name: &str, contracts_text: &str, body_text: &str) -> u64 {
    let combined = format!("{}::{}\0{}\0{}", module_name, fn_name, contracts_text, body_text);
    fnv1a_hash(&combined)
}

/// Минимальный JSON string extractor — без внешних крейтов.
/// Ищет `"key":"<value>"` или `"key": "<value>"`.
fn extract_json_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("\"{}\"", key);
    let pos = json.find(search.as_str())?;
    let after_key = &json[pos + search.len()..];
    // Пропускаем пробелы и ':'
    let after_colon = after_key.trim_start_matches(|c: char| c == ':' || c == ' ');
    if !after_colon.starts_with('"') { return None; }
    let inner = &after_colon[1..];
    let end = inner.find('"')?;
    Some(&inner[..end])
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_deterministic() {
        let h1 = fnv1a_hash("hello world");
        let h2 = fnv1a_hash("hello world");
        assert_eq!(h1, h2);
        let h3 = fnv1a_hash("hello world!");
        assert_ne!(h1, h3);
    }

    #[test]
    fn cache_key_changes_on_contract() {
        let k1 = cache_key("mod", "fn1", "requires x > 0", "x");
        let k2 = cache_key("mod", "fn1", "requires x >= 0", "x");
        assert_ne!(k1, k2);
    }

    #[test]
    fn extract_json_str_basic() {
        let json = r#"{"result": "proven", "fn_id": "foo"}"#;
        assert_eq!(extract_json_str(json, "result"), Some("proven"));
        assert_eq!(extract_json_str(json, "fn_id"), Some("foo"));
    }
}
