//! Plan 210 Ф.6а (D412-амендмент): NFC-нормализация путей `embed_dir`.
//!
//! Мотивация: macOS хранит имена файлов в NFD, Windows/Linux — обычно в NFC →
//! один и тот же чекаут даёт РАЗНЫЕ байтовые ключи (и разный `.c`) на разных
//! ОС (§2е D412-амендмента). Фикс: нормализовать КАЖДЫЙ путь записи в NFC при
//! обходе `embed_dir` — воспроизводимость между ОС восстановлена; коллизия
//! форм (два разных исходных имени → одна NFC-форма) — `E_EMBED_DIR_NFC_COLLISION`.
//!
//! **Zero новых Cargo-зависимостей.** Разведка (Plan 210 Ф.6а) проверила
//! `cargo tree` в `compiler-codegen` — крейта `unicode-normalization` в дереве
//! нет; добавление добавило бы ~762 КБ исходников (~128 КБ сжатый `.crate`,
//! `tables.rs` — 612 КБ) для NFD+NFKD+CCC+quick-check+stream-safe данных.
//! Вместо этого — переиспользуем УЖЕ СГЕНЕРИРОВАННЫЕ Unicode 16.0 таблицы
//! `std/src/unicode/norm_data.nv` (Plan 152.4.1, `nova-codegen unicode` из
//! UCD) и портируем ТОЧНЫЙ алгоритм canonical-decompose → canonical-order →
//! canonical-compose из `std/src/unicode/normalize.nv` (Plan 152.4.2, D253,
//! `normalize_nfc`/`str @to_nfc()`) на Rust. Нужны только `NFD_DATA`+
//! `CCC_DATA`+`COMP_DATA` (NFC не использует `NFKD_DATA` — тот нужен только
//! NFKD/NFKC) — это ≈45 КБ уже закоммиченных данных, не новый крейт. Одна
//! каноническая версия UCD (16.0) на весь репозиторий — Rust-порт и
//! `std.unicode.normalize_nfc` совпадают побайтово по семантике (общий
//! источник таблиц), только рантайм разный (Rust — компайл-тайм резолвера,
//! Nova — рантайм программы).
//!
//! Компилятор НЕ может вызвать `std.unicode.normalize_nfc` напрямую — эта
//! функция компилируется в C и исполняется В ПРОГРАММЕ, тогда как
//! `embed_resolve` работает ДО type-check, интерпретатора Nova в компиляторе
//! нет (`nova run` отсутствует по архитектуре) — необходим Rust-side порт.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ---- Hangul constants (UAX #15) — идентичны std/src/unicode/normalize.nv ----
const SBASE: u32 = 0xAC00;
const LBASE: u32 = 0x1100;
const VBASE: u32 = 0x1161;
const TBASE: u32 = 0x11A7;
const LCOUNT: u32 = 19;
const VCOUNT: u32 = 21;
const TCOUNT: u32 = 28;
const NCOUNT: u32 = VCOUNT * TCOUNT; // 588
const SCOUNT: u32 = LCOUNT * NCOUNT; // 11172

struct NfcTables {
    nfd: HashMap<u32, Vec<u32>>,
    ccc: HashMap<u32, u32>,
    comp: HashMap<(u32, u32), u32>,
}

fn parse_hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim(), 16).ok()
}

// "cp:d1,d2,..;cp2:d1,..;.." -> { cp -> [d1,d2,..] }. Зеркало
// `cp_utils.nv::parse_cp_keyed_table` (только decode-сторона: у нас значение
// сразу список кодпоинтов, не строка).
fn parse_nfd(data: &str) -> HashMap<u32, Vec<u32>> {
    let mut out = HashMap::new();
    for entry in data.split(';') {
        if entry.is_empty() {
            continue;
        }
        if let Some((cp, seq)) = entry.split_once(':') {
            if let Some(cp) = parse_hex(cp) {
                let ds: Vec<u32> = seq.split(',').filter_map(parse_hex).collect();
                if !ds.is_empty() {
                    out.insert(cp, ds);
                }
            }
        }
    }
    out
}

// "cp:ccc;cp2:ccc2;.." -> { cp -> ccc }. Зеркало `parse_cp_int_table`.
fn parse_ccc(data: &str) -> HashMap<u32, u32> {
    let mut out = HashMap::new();
    for entry in data.split(';') {
        if entry.is_empty() {
            continue;
        }
        if let Some((cp, ccc)) = entry.split_once(':') {
            if let (Some(cp), Some(ccc)) = (parse_hex(cp), parse_hex(ccc)) {
                out.insert(cp, ccc);
            }
        }
    }
    out
}

// "a,b:cp;.." -> { (a,b) -> cp }. Зеркало `parse_comp_table` (там пакуется в
// один `int`-ключ `(a<<21)|b` ради Nova HashMap без tuple-key; в Rust
// tuple-ключ доступен напрямую — паковать незачем).
fn parse_comp(data: &str) -> HashMap<(u32, u32), u32> {
    let mut out = HashMap::new();
    for entry in data.split(';') {
        if entry.is_empty() {
            continue;
        }
        if let Some((pair, cp)) = entry.split_once(':') {
            if let Some((a, b)) = pair.split_once(',') {
                if let (Some(a), Some(b), Some(cp)) = (parse_hex(a), parse_hex(b), parse_hex(cp))
                {
                    out.insert((a, b), cp);
                }
            }
        }
    }
    out
}

/// Извлечь значение `const NAME = "..."` (канон W_REDUNDANT_CONST_TYPE_ANNOTATION —
/// без аннотации; старая форма `const NAME str = "..."` принимается для
/// совместимости со старыми снимками) из текста `norm_data.nv`
/// (генератор не эмитит экранирование внутри — формат см. заголовок файла).
fn extract_const(src: &str, name: &str) -> Option<String> {
    let start = [
        format!("const {} = \"", name),
        format!("const {} str = \"", name),
    ]
    .iter()
    .find_map(|needle| src.find(needle.as_str()).map(|i| i + needle.len()))?;
    let end = start + src[start..].find('"')?;
    Some(src[start..end].to_string())
}

fn load_tables(norm_data_path: &Path) -> Option<NfcTables> {
    let text = std::fs::read_to_string(norm_data_path).ok()?;
    let nfd_raw = extract_const(&text, "NFD_DATA")?;
    let ccc_raw = extract_const(&text, "CCC_DATA")?;
    let comp_raw = extract_const(&text, "COMP_DATA")?;
    Some(NfcTables {
        nfd: parse_nfd(&nfd_raw),
        ccc: parse_ccc(&ccc_raw),
        comp: parse_comp(&comp_raw),
    })
}

/// Кэш по `std_src` (обычно один процесс = один резолвленный std-корень, но
/// `test_runner` может гонять фикстуры с разными `project_root`/`NOVA_STD_PATH`
/// в одном процессе — ключуем по пути, а не голым `OnceLock<Tables>`).
static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<std::sync::Arc<NfcTables>>>>> =
    OnceLock::new();

fn tables_for(std_src: &Path) -> Option<std::sync::Arc<NfcTables>> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = guard.get(std_src) {
        return entry.clone();
    }
    let loaded = load_tables(&std_src.join("unicode/norm_data.nv")).map(std::sync::Arc::new);
    guard.insert(std_src.to_path_buf(), loaded.clone());
    loaded
}

fn ccc_of(t: &NfcTables, cp: u32) -> u32 {
    t.ccc.get(&cp).copied().unwrap_or(0)
}

fn is_hangul_syllable(cp: u32) -> bool {
    cp >= SBASE && cp < SBASE + SCOUNT
}

// ---- canonical decompose (порт `normalize.nv::decompose(.., compat: false)`) ----
fn decompose(t: &NfcTables, cps: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(cps.len() * 2);
    for &cp in cps {
        if is_hangul_syllable(cp) {
            let si = cp - SBASE;
            out.push(LBASE + si / NCOUNT);
            out.push(VBASE + (si % NCOUNT) / TCOUNT);
            let tt = si % TCOUNT;
            if tt != 0 {
                out.push(TBASE + tt);
            }
        } else if let Some(seq) = t.nfd.get(&cp) {
            out.extend_from_slice(seq);
        } else {
            out.push(cp);
        }
    }
    out
}

// ---- canonical ordering (порт `normalize.nv::canonical_order`, stable insertion sort) ----
fn canonical_order(t: &NfcTables, arr: &mut [u32]) {
    let n = arr.len();
    let mut i = 1;
    while i < n {
        let cc = ccc_of(t, arr[i]);
        if cc != 0 {
            let mut j = i;
            while j > 0 && ccc_of(t, arr[j - 1]) > cc {
                arr.swap(j, j - 1);
                j -= 1;
            }
        }
        i += 1;
    }
}

// ---- canonical composition (порт `normalize.nv::compose_pair`) ----
fn compose_pair(t: &NfcTables, a: u32, b: u32) -> Option<u32> {
    if a >= LBASE && a < LBASE + LCOUNT && b >= VBASE && b < VBASE + VCOUNT {
        let li = a - LBASE;
        let vi = b - VBASE;
        Some(SBASE + (li * VCOUNT + vi) * TCOUNT)
    } else if a >= SBASE && a < SBASE + SCOUNT && ((a - SBASE) % TCOUNT) == 0
        && b > TBASE
        && b < TBASE + TCOUNT
    {
        Some(a + (b - TBASE))
    } else {
        t.comp.get(&(a, b)).copied()
    }
}

// ---- canonical composition walk (порт `normalize.nv::compose`) ----
fn compose(t: &NfcTables, cps: &[u32]) -> Vec<u32> {
    let n = cps.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n);
    out.push(cps[0]);
    let mut starter_pos = 0usize;
    let mut starter_ch = cps[0];
    let mut last_ccc = ccc_of(t, cps[0]);
    if last_ccc != 0 {
        last_ccc = 256; // non-starter first char can never compose
    }
    let mut i = 1;
    while i < n {
        let ch = cps[i];
        let cc = ccc_of(t, ch);
        let mut composed = false;
        if let Some(comp) = compose_pair(t, starter_ch, ch) {
            if last_ccc < cc || last_ccc == 0 {
                out[starter_pos] = comp;
                starter_ch = comp;
                composed = true;
            }
        }
        if !composed {
            if cc == 0 {
                starter_pos = out.len();
                starter_ch = ch;
            }
            last_ccc = cc;
            out.push(ch);
        }
        i += 1;
    }
    out
}

/// NFC-нормализация `s`, используя таблицы Unicode 16.0 из
/// `<std_src>/unicode/norm_data.nv` (`std_src` = `manifest::resolve_std_path`
/// вызывающего проекта). Мягкая деградация: если файл таблиц недоступен
/// (виртуальный/минимальный тестовый корень без реального std) — возвращает
/// `s` БЕЗ ИЗМЕНЕНИЙ (тот же soft-skip дух, что `per_file_embed_root` — не
/// падать компиляцией из-за отсутствующей опциональной инфраструктуры).
pub fn normalize_nfc(std_src: &Path, s: &str) -> String {
    let Some(t) = tables_for(std_src) else {
        return s.to_string();
    };
    let cps: Vec<u32> = s.chars().map(|c| c as u32).collect();
    let mut decomposed = decompose(&t, &cps);
    canonical_order(&t, &mut decomposed);
    let composed = compose(&t, &decomposed);
    composed.into_iter().filter_map(char::from_u32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn std_src() -> PathBuf {
        // compiler-codegen/src/nfc.rs -> repo root -> std/src
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("std")
            .join("src")
    }

    #[test]
    fn nfd_precomposes_to_nfc() {
        // "cafe" + COMBINING ACUTE ACCENT (U+0301) -> "café" (U+00E9).
        let nfd = "cafe\u{0301}.txt";
        let got = normalize_nfc(&std_src(), nfd);
        assert_eq!(got, "caf\u{00e9}.txt");
    }

    #[test]
    fn already_nfc_is_unchanged() {
        let nfc = "caf\u{00e9}.txt";
        assert_eq!(normalize_nfc(&std_src(), nfc), nfc);
    }

    #[test]
    fn ascii_is_unchanged() {
        assert_eq!(normalize_nfc(&std_src(), "alpha.txt"), "alpha.txt");
    }

    #[test]
    fn nfc_and_nfd_forms_collide() {
        let a = normalize_nfc(&std_src(), "caf\u{00e9}.txt");
        let b = normalize_nfc(&std_src(), "cafe\u{0301}.txt");
        assert_eq!(a, b);
    }
}
