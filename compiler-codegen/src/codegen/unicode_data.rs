//! Plan 152.4.1 (Q-unicode-data): build-time generator of Unicode normalization
//! tables from the Unicode Character Database (UCD).
//!
//! Reads the canonical UCD text files and emits `std/unicode/norm_data.nv` —
//! compact `;`-separated string tables that `std/unicode/normalize.nv` parses
//! lazily (once, on first touch) into `HashMap` lookups. No ICU / OS dependency;
//! the tables are pinned to a Unicode version. Precedent: Rust `unicode-*`
//! crates (codegen), Go `maketables`.
//!
//! This is a faithful Rust port of the reference prototype generator: it builds
//! the canonical (NFD) and compatibility (NFKD) full decompositions, the
//! canonical combining class (CCC) table, and the canonical composition table
//! (the inverse of length-2 canonical decompositions, minus singletons and
//! full-composition-exclusions). Hangul is handled algorithmically by the
//! normalizer (UAX #15 §‑Hangul), not via these tables.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Parsed + derived UCD normalization data. `BTreeMap`/`BTreeSet` keep the
/// emitted tables deterministically key-sorted (stable output for `--check`).
pub struct NormTables {
    /// cp -> full canonical decomposition (length >= 1, != [cp]).
    pub nfd: BTreeMap<u32, Vec<u32>>,
    /// cp -> full compatibility decomposition (length >= 1, != [cp]).
    pub nfkd: BTreeMap<u32, Vec<u32>>,
    /// cp -> canonical combining class (non-zero only; absent => 0).
    pub ccc: BTreeMap<u32, u32>,
    /// (a, b) -> primary composite cp (canonical, non-excluded, non-singleton).
    pub comp: BTreeMap<(u32, u32), u32>,
}

fn parse_cps(s: &str) -> Vec<u32> {
    s.split_whitespace()
        .filter_map(|x| u32::from_str_radix(x, 16).ok())
        .collect()
}

/// Add a UCD range (`AAAA..BBBB`) or a single codepoint (`AAAA`) to `out`.
fn add_range_or_single(s: &str, out: &mut BTreeSet<u32>) {
    if let Some((a, b)) = s.split_once("..") {
        if let (Ok(a), Ok(b)) = (
            u32::from_str_radix(a.trim(), 16),
            u32::from_str_radix(b.trim(), 16),
        ) {
            for x in a..=b {
                out.insert(x);
            }
        }
    } else if let Ok(x) = u32::from_str_radix(s.trim(), 16) {
        out.insert(x);
    }
}

/// Recursive full decomposition. Canonical (`compat == false`) stops at
/// compatibility-decomposition boundaries; compatibility (`compat == true`)
/// expands through both.
fn full_decomp(cp: u32, compat: bool, decomp_raw: &BTreeMap<u32, (bool, Vec<u32>)>) -> Vec<u32> {
    match decomp_raw.get(&cp) {
        None => vec![cp],
        Some((is_compat, seq)) => {
            if *is_compat && !compat {
                return vec![cp];
            }
            let mut out = Vec::new();
            for &c in seq {
                out.extend(full_decomp(c, compat, decomp_raw));
            }
            out
        }
    }
}

/// Parse the UCD files in `ucd_dir` and derive the normalization tables.
/// Required files: `UnicodeData.txt`, `CompositionExclusions.txt`,
/// `DerivedNormalizationProps.txt`.
pub fn parse_ucd(ucd_dir: &Path) -> anyhow::Result<NormTables> {
    let read = |name: &str| -> anyhow::Result<String> {
        let p = ucd_dir.join(name);
        std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", p.display(), e))
    };

    // --- UnicodeData.txt: raw decompositions + canonical combining class ---
    // Fields (';'-separated): [0]=codepoint, [3]=ccc (decimal), [5]=decomp.
    // A decomp beginning with `<tag>` is a *compatibility* mapping; otherwise
    // it is canonical. CJK/Hangul range rows ("First>"/"Last>") carry no decomp
    // and ccc=0, so they are skipped naturally.
    let mut decomp_raw: BTreeMap<u32, (bool, Vec<u32>)> = BTreeMap::new();
    let mut ccc: BTreeMap<u32, u32> = BTreeMap::new();
    for line in read("UnicodeData.txt")?.lines() {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 6 {
            continue;
        }
        let cp = match u32::from_str_radix(f[0], 16) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Ok(c) = f[3].parse::<u32>() {
            if c != 0 {
                ccc.insert(cp, c);
            }
        }
        let d = f[5].trim();
        if !d.is_empty() {
            if d.starts_with('<') {
                // "<compat-tag> cp cp ..." — strip the tag, keep the sequence.
                if let Some((_, rest)) = d.split_once('>') {
                    decomp_raw.insert(cp, (true, parse_cps(rest)));
                }
            } else {
                decomp_raw.insert(cp, (false, parse_cps(d)));
            }
        }
    }

    // --- composition exclusions ---
    // The full set is `Full_Composition_Exclusion` (DerivedNormalizationProps),
    // a superset of CompositionExclusions.txt; we union both for robustness.
    let mut excl: BTreeSet<u32> = BTreeSet::new();
    for line in read("CompositionExclusions.txt")?.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        add_range_or_single(line, &mut excl);
    }
    for line in read("DerivedNormalizationProps.txt")?.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || !line.contains(';') {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() >= 2 && parts[1].trim() == "Full_Composition_Exclusion" {
            add_range_or_single(parts[0].trim(), &mut excl);
        }
    }

    // --- full recursive decompositions (NFD / NFKD) ---
    let mut nfd: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut nfkd: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &cp in decomp_raw.keys() {
        let fc = full_decomp(cp, false, &decomp_raw);
        if fc != [cp] {
            nfd.insert(cp, fc);
        }
        let fk = full_decomp(cp, true, &decomp_raw);
        if fk != [cp] {
            nfkd.insert(cp, fk);
        }
    }

    // --- canonical composition: invert length-2 canonical decompositions ---
    // Singletons (len != 2) and full-composition-exclusions are not composable.
    let mut comp: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (&cp, (is_compat, seq)) in &decomp_raw {
        if *is_compat || seq.len() != 2 || excl.contains(&cp) {
            continue;
        }
        comp.insert((seq[0], seq[1]), cp);
    }

    Ok(NormTables {
        nfd,
        nfkd,
        ccc,
        comp,
    })
}

fn emit_map_seq(m: &BTreeMap<u32, Vec<u32>>) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(m.len());
    for (k, v) in m {
        let vs: Vec<String> = v.iter().map(|x| format!("{:x}", x)).collect();
        parts.push(format!("{:x}:{}", k, vs.join(",")));
    }
    parts.join(";")
}

fn emit_ccc(m: &BTreeMap<u32, u32>) -> String {
    m.iter()
        .map(|(k, v)| format!("{:x}:{:x}", k, v))
        .collect::<Vec<_>>()
        .join(";")
}

fn emit_comp(m: &BTreeMap<(u32, u32), u32>) -> String {
    m.iter()
        .map(|((a, b), cp)| format!("{:x},{:x}:{:x}", a, b, cp))
        .collect::<Vec<_>>()
        .join(";")
}

/// Render `std/unicode/norm_data.nv` — a peer file of the `unicode` folder
/// module holding the table strings as `const`s. `normalize.nv` (same module)
/// parses them lazily.
pub fn render_norm_data_nv(tables: &NormTables, version: &str) -> String {
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode`. DO NOT EDIT BY HAND.\n");
    out.push_str("// Source of truth: Unicode Character Database (UCD).\n");
    out.push_str("// Regenerate: nova-codegen unicode --ucd-dir <UCD-dir> --root <repo>\n");
    out.push_str("// See docs/plans/152.4-std-unicode.md (Q-unicode-data).\n");
    out.push_str("//\n");
    out.push_str("// Table formats (all integers lowercase hex, ';'-separated entries):\n");
    out.push_str("//   NFD_DATA / NFKD_DATA : \"cp:d1,d2,..;..\"  codepoint -> decomposition seq\n");
    out.push_str("//   CCC_DATA             : \"cp:ccc;..\"       codepoint -> combining class\n");
    out.push_str("//   COMP_DATA            : \"a,b:cp;..\"       canonical (a,b) -> composite cp\n");
    out.push('\n');
    // Folder-module `std/unicode/` (peer of normalize.nv etc.): a folder-module
    // directly under the package root declares `<package>.<folder>` = std.unicode
    // (D29 rev-3); peers share declarations, so the tables below are visible to
    // normalize.nv without export/import. Default prelude (NOT `#no_prelude`) so
    // `[]T` lowers to `Vec[T]` consistently — `std.unicode` is opt-in (not in the
    // prelude itself), so there is no prelude import cycle (cf. std/encoding/utf16.nv).
    out.push_str("module std.unicode\n");
    out.push('\n');
    out.push_str(&format!(
        "/// Pinned Unicode version these tables were generated from.\nexport const UNICODE_VERSION str = \"{}\"\n\n",
        version
    ));
    out.push_str(&format!("const NFD_DATA str = \"{}\"\n\n", emit_map_seq(&tables.nfd)));
    out.push_str(&format!("const NFKD_DATA str = \"{}\"\n\n", emit_map_seq(&tables.nfkd)));
    out.push_str(&format!("const CCC_DATA str = \"{}\"\n\n", emit_ccc(&tables.ccc)));
    out.push_str(&format!("const COMP_DATA str = \"{}\"\n", emit_comp(&tables.comp)));
    out
}

/// Render `nova_tests/plan152_4/normalization_conformance.nv` — the official
/// UAX #15 conformance check (NormalizationTest.txt). For each selected data line
/// `source;NFC;NFD;NFKC;NFKD` we assert all four `normalize_*(source)` outputs.
///
/// Full coverage (~20k lines × 4 = ~80k asserts) is impractical as embedded
/// asserts (Nova has no runtime file read). Instead: **Part 0 (hand-curated
/// specific cases) IN FULL** + a **uniform stride-sample of the rest** (Parts
/// 1–5) up to `limit` — so the sample spans the whole codepoint range and all
/// scripts (Latin/Greek/Cyrillic/CJK-compat/Hangul/Arabic/math/…), not just the
/// low-codepoint head. Cases are split across several `test` blocks (≤500 each)
/// to keep generated C functions small. The selection is recorded in the header
/// (not a silent truncation).
pub fn render_conformance_nv(ucd_dir: &Path, limit: usize) -> anyhow::Result<String> {
    let data = std::fs::read_to_string(ucd_dir.join("NormalizationTest.txt"))
        .map_err(|e| anyhow::anyhow!("failed to read NormalizationTest.txt: {}", e))?;
    // Hex codepoint sequence -> a Nova string literal of \u{..} escapes.
    let lit = |s: &str| -> String {
        let mut out = String::new();
        for h in s.split_whitespace() {
            if let Ok(cp) = u32::from_str_radix(h, 16) {
                out.push_str(&format!("\\u{{{:x}}}", cp));
            }
        }
        out
    };
    let mut part0: Vec<[String; 5]> = Vec::new();
    let mut rest: Vec<[String; 5]> = Vec::new();
    let mut in_part0 = false;
    let mut total = 0usize;
    for line in data.lines() {
        let line = line.trim();
        if let Some(p) = line.strip_prefix('@') {
            in_part0 = p.starts_with("Part0");
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let core = line.split('#').next().unwrap_or("");
        let cols: Vec<&str> = core.split(';').collect();
        if cols.len() < 5 {
            continue;
        }
        total += 1;
        let case = [lit(cols[0]), lit(cols[1]), lit(cols[2]), lit(cols[3]), lit(cols[4])];
        if in_part0 {
            part0.push(case);
        } else {
            rest.push(case);
        }
    }
    // Part 0 in full, then uniform stride-sample of the rest to fill the budget.
    let budget = limit.saturating_sub(part0.len()).max(1);
    let stride = (rest.len() / budget).max(1);
    let mut cases: Vec<[String; 5]> = Vec::with_capacity(limit);
    cases.extend(part0.iter().cloned());
    let mut i = 0usize;
    while i < rest.len() && cases.len() < limit {
        cases.push(rest[i].clone());
        i += stride;
    }

    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode --emit-conformance`. DO NOT EDIT.\n");
    out.push_str("// UAX #15 conformance: for each NormalizationTest.txt case\n");
    out.push_str("//   c1=source c2=NFC c3=NFD c4=NFKC c5=NFKD\n");
    out.push_str("// assert NFC(c1)==c2, NFD(c1)==c3, NFKC(c1)==c4, NFKD(c1)==c5.\n");
    out.push_str(&format!(
        "// Coverage: Part 0 in full ({}) + stride-{} sample of Parts 1-5 = {} of {} data lines.\n",
        part0.len(), stride, cases.len(), total
    ));
    out.push_str("module plan152_4.normalization_conformance\n\n");
    out.push_str("import std.unicode.{normalize_nfc, normalize_nfd, normalize_nfkc, normalize_nfkd}\n\n");
    // Split into chunks (≤500 cases / 2000 asserts per test fn) to keep the
    // generated C functions a reasonable size.
    const CHUNK: usize = 500;
    for (ci, chunk) in cases.chunks(CHUNK).enumerate() {
        out.push_str(&format!(
            "test \"UAX#15 NormalizationTest.txt conformance (chunk {}, {} cases)\" {{\n",
            ci, chunk.len()
        ));
        for c in chunk {
            out.push_str(&format!("    assert(normalize_nfc(\"{}\") == \"{}\")\n", c[0], c[1]));
            out.push_str(&format!("    assert(normalize_nfd(\"{}\") == \"{}\")\n", c[0], c[2]));
            out.push_str(&format!("    assert(normalize_nfkc(\"{}\") == \"{}\")\n", c[0], c[3]));
            out.push_str(&format!("    assert(normalize_nfkd(\"{}\") == \"{}\")\n", c[0], c[4]));
        }
        out.push_str("}\n\n");
    }
    Ok(out)
}
