// SPDX-License-Identifier: MIT OR Apache-2.0
//! План 252 Ф.0 — счётчики стадии `imports-resolve`.
//!
//! Замер плана 255 показал, что на корпусе 568 файлов 95,2 % работы уходит в
//! `imports-resolve`. Числа отвечают на «сколько», но не на «почему»: одно
//! дело — стадия вызывается по разу на файл и каждый раз заново читает и
//! разбирает весь `std`, другое — она вызывается тысячу раз для одного и
//! того же compile-unit. Лечится это по-разному (кэш разбора против
//! объединения единиц компиляции), поэтому счётчики заведены ДО кода.
//!
//! Молчат по умолчанию: включаются `NOVA_IMPORTS_STATS=1`, при выключенном
//! переключателе каждая точка учёта — одна проверка `OnceLock<bool>` и
//! ранний возврат (тот же приём, что у `PerfTimer`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Число вызовов `resolve_imports_inline_ex` (верхнеуровневый вход стадии).
static RESOLVE_CALLS: AtomicU64 = AtomicU64::new(0);
/// Число вызовов `collect_all_signatures` (вторая половина той же стадии).
static SIG_CALLS: AtomicU64 = AtomicU64::new(0);
/// Число разборов peer-файла в полном резолве (`resolve_one`, PASS 1).
static RESOLVE_PARSES: AtomicU64 = AtomicU64::new(0);
/// Число разборов peer-файла в сигнатурном пред-проходе (`collect_sigs_one`).
static SIG_PARSES: AtomicU64 = AtomicU64::new(0);
/// Суммарный объём разобранного текста, байт (обе половины).
static PARSED_BYTES: AtomicU64 = AtomicU64::new(0);

/// Сколько раз разобран каждый физический файл. Ключ — путь как он пришёл в
/// разбор (не канонизируем: канонизация сама стоит сисвызова, а для ответа
/// «сколько уникальных файлов» достаточно строкового пути — он у всех
/// вызовов формируется одним и тем же кодом резолва).
static PER_FILE: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("NOVA_IMPORTS_STATS")
            .map(|v| v == "1" || v == "true" || v == "yes")
            .unwrap_or(false)
    })
}

/// Вход в полный резолв импортов.
pub fn note_resolve_call() {
    if !enabled() {
        return;
    }
    RESOLVE_CALLS.fetch_add(1, Ordering::Relaxed);
}

/// Вход в сигнатурный пред-проход.
pub fn note_sig_call() {
    if !enabled() {
        return;
    }
    SIG_CALLS.fetch_add(1, Ordering::Relaxed);
}

/// Разбор одного peer-файла. `from_sig_pass` разделяет две половины стадии.
pub fn note_parse(path: &Path, bytes: usize, from_sig_pass: bool) {
    if !enabled() {
        return;
    }
    if from_sig_pass {
        SIG_PARSES.fetch_add(1, Ordering::Relaxed);
    } else {
        RESOLVE_PARSES.fetch_add(1, Ordering::Relaxed);
    }
    PARSED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    let key = path.to_string_lossy().to_string();
    if let Ok(mut g) = PER_FILE.lock() {
        let map = g.get_or_insert_with(HashMap::new);
        *map.entry(key).or_insert(0) += 1;
    }
}

/// Сводка. Пустая строка, если переключатель выключен или ничего не собрано.
pub fn dump_stats() -> String {
    if !enabled() {
        return String::new();
    }
    let resolve_calls = RESOLVE_CALLS.load(Ordering::Relaxed);
    let sig_calls = SIG_CALLS.load(Ordering::Relaxed);
    let resolve_parses = RESOLVE_PARSES.load(Ordering::Relaxed);
    let sig_parses = SIG_PARSES.load(Ordering::Relaxed);
    let bytes = PARSED_BYTES.load(Ordering::Relaxed);
    if resolve_calls == 0 && sig_calls == 0 && resolve_parses == 0 && sig_parses == 0 {
        return String::new();
    }
    let (unique_files, top): (usize, Vec<(String, u64)>) = {
        let g = PER_FILE.lock().ok();
        match g.as_ref().and_then(|g| g.as_ref()) {
            Some(m) => {
                let mut rows: Vec<(String, u64)> =
                    m.iter().map(|(k, v)| (k.clone(), *v)).collect();
                rows.sort_by(|a, b| b.1.cmp(&a.1));
                rows.truncate(10);
                (m.len(), rows)
            }
            None => (0, Vec::new()),
        }
    };
    let total_parses = resolve_parses + sig_parses;
    let mut out = String::new();
    use std::fmt::Write;
    let _ = writeln!(out, "\n===== imports stats (план 252 Ф.0) =====");
    let _ = writeln!(out, "resolve_imports_inline_ex вызовов : {}", resolve_calls);
    let _ = writeln!(out, "collect_all_signatures вызовов    : {}", sig_calls);
    let _ = writeln!(out, "разборов peer-файлов (полный)     : {}", resolve_parses);
    let _ = writeln!(out, "разборов peer-файлов (сигнатуры)  : {}", sig_parses);
    let _ = writeln!(out, "разборов всего                    : {}", total_parses);
    let _ = writeln!(out, "уникальных файлов                 : {}", unique_files);
    if unique_files > 0 {
        let _ = writeln!(
            out,
            "повторность (разборов/уникальных) : {:.1}x",
            total_parses as f64 / unique_files as f64
        );
    }
    let _ = writeln!(out, "разобрано байт                    : {} ({:.1} MiB)",
        bytes, bytes as f64 / (1024.0 * 1024.0));
    if !top.is_empty() {
        let _ = writeln!(out, "-- чаще всего разбираемые файлы --");
        for (p, n) in &top {
            let _ = writeln!(out, "{:>8}x {}", n, p);
        }
    }
    out
}
