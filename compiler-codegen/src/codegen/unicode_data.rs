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

// ─── Plan 152.4.3: grapheme-break tables (UAX #29) ───

/// Grapheme-cluster-break data: GCB category ranges + Extended_Pictographic ranges.
pub struct GraphemeTables {
    /// (lo, hi, cat) sorted by lo; cat 1..=13; "Other" (0) is omitted (default).
    pub gcb: Vec<(u32, u32, u8)>,
    /// (lo, hi) Extended_Pictographic ranges (emoji-data), sorted by lo.
    pub ext_pict: Vec<(u32, u32)>,
    /// (lo, hi, cat) Indic_Conjunct_Break ranges (DerivedCoreProperties), for
    /// GB9c. cat 1=Consonant 2=Extend 3=Linker. Sorted by lo.
    pub incb: Vec<(u32, u32, u8)>,
}

/// InCB (Indic_Conjunct_Break) value -> small int. MUST match std/unicode/graphemes.nv.
fn incb_cat_code(name: &str) -> u8 {
    match name {
        "Consonant" => 1,
        "Extend" => 2,
        "Linker" => 3,
        _ => 0,
    }
}

/// GCB property name -> small int. MUST match the decoding in std/unicode/graphemes.nv.
fn gcb_cat_code(name: &str) -> u8 {
    match name {
        "CR" => 1,
        "LF" => 2,
        "Control" => 3,
        "Extend" => 4,
        "ZWJ" => 5,
        "Regional_Indicator" => 6,
        "Prepend" => 7,
        "SpacingMark" => 8,
        "L" => 9,
        "V" => 10,
        "T" => 11,
        "LV" => 12,
        "LVT" => 13,
        _ => 0,
    }
}

fn parse_range_pair(s: &str) -> Option<(u32, u32)> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once("..") {
        Some((
            u32::from_str_radix(a.trim(), 16).ok()?,
            u32::from_str_radix(b.trim(), 16).ok()?,
        ))
    } else {
        let v = u32::from_str_radix(s, 16).ok()?;
        Some((v, v))
    }
}

/// Parse `GraphemeBreakProperty.txt` + `emoji-data.txt` into sorted range tables.
pub fn parse_grapheme_tables(ucd_dir: &Path) -> anyhow::Result<GraphemeTables> {
    let read = |name: &str| -> anyhow::Result<String> {
        let p = ucd_dir.join(name);
        std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", p.display(), e))
    };
    // GraphemeBreakProperty.txt: "RANGE ; PROP # comment"
    let mut gcb: Vec<(u32, u32, u8)> = Vec::new();
    for line in read("GraphemeBreakProperty.txt")?.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 2 {
            continue;
        }
        let cat = gcb_cat_code(parts[1].trim());
        if cat == 0 {
            continue;
        }
        if let Some((lo, hi)) = parse_range_pair(parts[0]) {
            gcb.push((lo, hi, cat));
        }
    }
    gcb.sort_by_key(|&(lo, _, _)| lo);
    // emoji-data.txt: "RANGE ; Extended_Pictographic # comment"
    let mut ext_pict: Vec<(u32, u32)> = Vec::new();
    for line in read("emoji-data.txt")?.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 2 || parts[1].trim() != "Extended_Pictographic" {
            continue;
        }
        if let Some((lo, hi)) = parse_range_pair(parts[0]) {
            ext_pict.push((lo, hi));
        }
    }
    ext_pict.sort_by_key(|&(lo, _)| lo);
    // DerivedCoreProperties.txt: "RANGE ; InCB; Value # comment" (GB9c, U15.1+).
    let mut incb: Vec<(u32, u32, u8)> = Vec::new();
    for line in read("DerivedCoreProperties.txt")?.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 3 || parts[1].trim() != "InCB" {
            continue;
        }
        let cat = incb_cat_code(parts[2].trim());
        if cat == 0 {
            continue;
        }
        if let Some((lo, hi)) = parse_range_pair(parts[0]) {
            incb.push((lo, hi, cat));
        }
    }
    incb.sort_by_key(|&(lo, _, _)| lo);
    Ok(GraphemeTables { gcb, ext_pict, incb })
}

/// Render `std/unicode/grapheme_data.nv` (peer of graphemes.nv). Range tables as
/// `;`-separated `lo,hi[,cat]` (lowercase hex), sorted by lo for binary search.
pub fn render_grapheme_data_nv(t: &GraphemeTables, version: &str) -> String {
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode`. DO NOT EDIT BY HAND.\n");
    out.push_str("// Source: UCD GraphemeBreakProperty.txt + emoji-data.txt (UAX #29).\n");
    out.push_str("// Regenerate: nova-codegen unicode --ucd-dir <UCD-dir> --root <repo>\n");
    out.push_str("//\n");
    out.push_str("// GCB_DATA     : \"lo,hi,cat;..\"  grapheme-cluster-break category ranges\n");
    out.push_str("//   cat: 1=CR 2=LF 3=Control 4=Extend 5=ZWJ 6=Regional_Indicator\n");
    out.push_str("//        7=Prepend 8=SpacingMark 9=L 10=V 11=T 12=LV 13=LVT (Other=absent)\n");
    out.push_str("// EXTPICT_DATA : \"lo,hi;..\"      Extended_Pictographic ranges\n");
    out.push_str("// INCB_DATA    : \"lo,hi,cat;..\"  Indic_Conjunct_Break (GB9c): 1=Consonant 2=Extend 3=Linker\n");
    out.push_str("// Ranges sorted by lo (binary search). Pinned to UNICODE_VERSION.\n");
    out.push('\n');
    out.push_str("module std.unicode\n");
    out.push('\n');
    out.push_str(&format!("export const GRAPHEME_UNICODE_VERSION str = \"{}\"\n\n", version));
    let gcb_s: Vec<String> = t
        .gcb
        .iter()
        .map(|&(lo, hi, cat)| format!("{:x},{:x},{:x}", lo, hi, cat))
        .collect();
    out.push_str(&format!("const GCB_DATA str = \"{}\"\n\n", gcb_s.join(";")));
    let ep_s: Vec<String> = t
        .ext_pict
        .iter()
        .map(|&(lo, hi)| format!("{:x},{:x}", lo, hi))
        .collect();
    out.push_str(&format!("const EXTPICT_DATA str = \"{}\"\n\n", ep_s.join(";")));
    let incb_s: Vec<String> = t
        .incb
        .iter()
        .map(|&(lo, hi, cat)| format!("{:x},{:x},{:x}", lo, hi, cat))
        .collect();
    out.push_str(&format!("const INCB_DATA str = \"{}\"\n", incb_s.join(";")));
    out
}

/// Render `nova_tests/plan152_4/grapheme_conformance.nv` — UAX #29 conformance
/// from GraphemeBreakTest.txt. Each line `÷ cp × cp ÷ cp ÷` encodes the expected
/// grapheme boundaries (÷ = break, × = join). For each (capped, stride-sampled)
/// line we build the full string and assert `as_graphemes()` yields exactly the
/// expected cluster sequence (CONTENT-checked, not just count). Chunked.
pub fn render_grapheme_conformance_nv(ucd_dir: &Path, limit: usize) -> anyhow::Result<String> {
    let data = std::fs::read_to_string(ucd_dir.join("GraphemeBreakTest.txt"))
        .map_err(|e| anyhow::anyhow!("failed to read GraphemeBreakTest.txt: {}", e))?;
    let esc = |cps: &[u32]| -> String {
        let mut s = String::new();
        for &cp in cps {
            s.push_str(&format!("\\u{{{:x}}}", cp));
        }
        s
    };
    // Each case: (full_string_literal, Vec<cluster_literal>).
    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    let mut total = 0usize;
    for line in data.lines() {
        let core = line.split('#').next().unwrap_or("").trim();
        if core.is_empty() {
            continue;
        }
        let mut clusters: Vec<Vec<u32>> = Vec::new();
        let mut cur: Vec<u32> = Vec::new();
        let mut all: Vec<u32> = Vec::new();
        for tok in core.split_whitespace() {
            if tok == "\u{00f7}" {
                // ÷ boundary
                if !cur.is_empty() {
                    clusters.push(std::mem::take(&mut cur));
                }
            } else if tok == "\u{00d7}" {
                // × join — stay in current cluster
            } else if let Ok(cp) = u32::from_str_radix(tok, 16) {
                cur.push(cp);
                all.push(cp);
            }
        }
        if !cur.is_empty() {
            clusters.push(cur);
        }
        if clusters.is_empty() {
            continue;
        }
        total += 1;
        cases.push((esc(&all), clusters.iter().map(|c| esc(c)).collect()));
    }
    // Stride-sample to the budget (spans the whole file / all rules).
    let stride = (cases.len() / limit.max(1)).max(1);
    let mut sel: Vec<(String, Vec<String>)> = Vec::new();
    let mut i = 0usize;
    while i < cases.len() && sel.len() < limit {
        sel.push(cases[i].clone());
        i += stride;
    }
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode --emit-conformance`. DO NOT EDIT.\n");
    out.push_str("// UAX #29 grapheme conformance from GraphemeBreakTest.txt: assert\n");
    out.push_str("// `as_graphemes()` yields the expected cluster sequence (content-checked).\n");
    out.push_str(&format!("// Coverage: stride-{} sample = {} of {} test lines.\n", stride, sel.len(), total));
    out.push_str("module plan152_4.grapheme_conformance\n\n");
    out.push_str("import std.unicode.{GraphemesView}\n\n");
    const CHUNK: usize = 250;
    for (ci, chunk) in sel.chunks(CHUNK).enumerate() {
        out.push_str(&format!(
            "test \"UAX#29 GraphemeBreakTest (chunk {}, {} cases)\" {{\n",
            ci, chunk.len()
        ));
        for (full, clusters) in chunk {
            out.push_str("    {\n");
            out.push_str(&format!("        mut g = \"{}\".as_graphemes()\n", full));
            for c in clusters {
                out.push_str(&format!(
                    "        match g.next() {{ Some(s) => assert(s == \"{}\"), None => assert(false) }}\n",
                    c
                ));
            }
            out.push_str("        assert(g.next() == None)\n");
            out.push_str("    }\n");
        }
        out.push_str("}\n\n");
    }
    Ok(out)
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

// ─── Plan 152.4.5: word-boundary tables (UAX #29) ───

/// Word_Break property name -> small int. MUST match std/unicode/words.nv (WB_*).
fn wb_cat_code(name: &str) -> u8 {
    match name {
        "CR" => 1,
        "LF" => 2,
        "Newline" => 3,
        "Extend" => 4,
        "ZWJ" => 5,
        "Regional_Indicator" => 6,
        "Format" => 7,
        "Katakana" => 8,
        "Hebrew_Letter" => 9,
        "ALetter" => 10,
        "Single_Quote" => 11,
        "Double_Quote" => 12,
        "MidNumLet" => 13,
        "MidLetter" => 14,
        "MidNum" => 15,
        "Numeric" => 16,
        "ExtendNumLet" => 17,
        "WSegSpace" => 18,
        _ => 0,
    }
}

/// Parse `WordBreakProperty.txt` into a sorted (lo, hi, cat) range table.
pub fn parse_word_tables(ucd_dir: &Path) -> anyhow::Result<Vec<(u32, u32, u8)>> {
    let data = std::fs::read_to_string(ucd_dir.join("WordBreakProperty.txt"))
        .map_err(|e| anyhow::anyhow!("failed to read WordBreakProperty.txt: {}", e))?;
    let mut wb: Vec<(u32, u32, u8)> = Vec::new();
    for line in data.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 2 {
            continue;
        }
        let cat = wb_cat_code(parts[1].trim());
        if cat == 0 {
            continue;
        }
        if let Some((lo, hi)) = parse_range_pair(parts[0]) {
            wb.push((lo, hi, cat));
        }
    }
    wb.sort_by_key(|&(lo, _, _)| lo);
    Ok(wb)
}

/// Render `std/unicode/word_data.nv` (peer of words.nv). WB category ranges as
/// `;`-separated `lo,hi,cat` (lowercase hex), sorted by lo for binary search.
/// (Extended_Pictographic for WB3c is reused from grapheme_data.nv's EXTPICT_DATA.)
pub fn render_word_data_nv(wb: &[(u32, u32, u8)], version: &str) -> String {
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode`. DO NOT EDIT BY HAND.\n");
    out.push_str("// Source: UCD WordBreakProperty.txt (UAX #29 word boundaries).\n");
    out.push_str("// Regenerate: nova-codegen unicode --ucd-dir <UCD-dir> --root <repo>\n");
    out.push_str("//\n");
    out.push_str("// WB_DATA : \"lo,hi,cat;..\"  Word_Break category ranges\n");
    out.push_str("//   cat: 1=CR 2=LF 3=Newline 4=Extend 5=ZWJ 6=Regional_Indicator 7=Format\n");
    out.push_str("//        8=Katakana 9=Hebrew_Letter 10=ALetter 11=Single_Quote 12=Double_Quote\n");
    out.push_str("//        13=MidNumLet 14=MidLetter 15=MidNum 16=Numeric 17=ExtendNumLet\n");
    out.push_str("//        18=WSegSpace (Other=absent). Ranges sorted by lo (binary search).\n");
    out.push('\n');
    out.push_str("module std.unicode\n");
    out.push('\n');
    out.push_str(&format!("export const WORD_UNICODE_VERSION str = \"{}\"\n\n", version));
    let wb_s: Vec<String> = wb
        .iter()
        .map(|&(lo, hi, cat)| format!("{:x},{:x},{:x}", lo, hi, cat))
        .collect();
    out.push_str(&format!("const WB_DATA str = \"{}\"\n", wb_s.join(";")));
    out
}

/// Render `nova_tests/plan152_4/word_conformance.nv` — UAX #29 word conformance
/// from WordBreakTest.txt (same `÷`/`×` format as GraphemeBreakTest). For each
/// (capped, stride-sampled) line, assert `as_words()` yields exactly the expected
/// segment sequence (CONTENT-checked, not just count). Chunked.
pub fn render_word_conformance_nv(ucd_dir: &Path, limit: usize) -> anyhow::Result<String> {
    let data = std::fs::read_to_string(ucd_dir.join("WordBreakTest.txt"))
        .map_err(|e| anyhow::anyhow!("failed to read WordBreakTest.txt: {}", e))?;
    let esc = |cps: &[u32]| -> String {
        let mut s = String::new();
        for &cp in cps {
            s.push_str(&format!("\\u{{{:x}}}", cp));
        }
        s
    };
    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    let mut total = 0usize;
    for line in data.lines() {
        let core = line.split('#').next().unwrap_or("").trim();
        if core.is_empty() {
            continue;
        }
        let mut segs: Vec<Vec<u32>> = Vec::new();
        let mut cur: Vec<u32> = Vec::new();
        let mut all: Vec<u32> = Vec::new();
        for tok in core.split_whitespace() {
            if tok == "\u{00f7}" {
                if !cur.is_empty() {
                    segs.push(std::mem::take(&mut cur));
                }
            } else if tok == "\u{00d7}" {
                // × join — stay in current segment
            } else if let Ok(cp) = u32::from_str_radix(tok, 16) {
                cur.push(cp);
                all.push(cp);
            }
        }
        if !cur.is_empty() {
            segs.push(cur);
        }
        if segs.is_empty() {
            continue;
        }
        total += 1;
        cases.push((esc(&all), segs.iter().map(|c| esc(c)).collect()));
    }
    // Uniform spread across the whole file (NOT a contiguous head).
    let take = limit.min(cases.len());
    let mut sel: Vec<(String, Vec<String>)> = Vec::with_capacity(take);
    if take == cases.len() {
        sel.extend(cases.iter().cloned());
    } else {
        for i in 0..take {
            sel.push(cases[i * cases.len() / take].clone());
        }
    }
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode --emit-conformance`. DO NOT EDIT.\n");
    out.push_str("// UAX #29 word conformance from WordBreakTest.txt: assert `as_words()`\n");
    out.push_str("// yields the expected segment sequence (content-checked).\n");
    out.push_str(&format!(
        "// Coverage: uniform spread = {} of {} test lines.\n",
        sel.len(), total
    ));
    out.push_str("module plan152_4.word_conformance\n\n");
    out.push_str("import std.unicode.{WordsView}\n\n");
    const CHUNK: usize = 250;
    for (ci, chunk) in sel.chunks(CHUNK).enumerate() {
        out.push_str(&format!(
            "test \"UAX#29 WordBreakTest (chunk {}, {} cases)\" {{\n",
            ci, chunk.len()
        ));
        for (full, segs) in chunk {
            out.push_str("    {\n");
            out.push_str(&format!("        mut w = \"{}\".as_words()\n", full));
            for c in segs {
                out.push_str(&format!(
                    "        match w.next() {{ Some(s) => assert(s == \"{}\"), None => assert(false) }}\n",
                    c
                ));
            }
            out.push_str("        assert(w.next() == None)\n");
            out.push_str("    }\n");
        }
        out.push_str("}\n\n");
    }
    Ok(out)
}

// ─── Plan 152.4.4: case folding + Unicode case mapping (UAX, SpecialCasing) ───

/// Case-mapping data: full case folding + full lower/upper mappings, plus the
/// `Cased` / `Case_Ignorable` ranges needed for the Final_Sigma context rule.
pub struct CaseTables {
    /// cp -> full case folding (CaseFolding.txt status C+F), != [cp].
    pub fold: BTreeMap<u32, Vec<u32>>,
    /// cp -> full lowercase (SpecialCasing unconditional, else UnicodeData simple), != [cp].
    pub lower: BTreeMap<u32, Vec<u32>>,
    /// cp -> full uppercase (SpecialCasing unconditional, else UnicodeData simple), != [cp].
    pub upper: BTreeMap<u32, Vec<u32>>,
    /// cp -> full titlecase (SpecialCasing unconditional, else UnicodeData simple), != [cp].
    pub title: BTreeMap<u32, Vec<u32>>,
    /// (lo, hi) `Cased` ranges (DerivedCoreProperties), sorted by lo.
    pub cased: Vec<(u32, u32)>,
    /// (lo, hi) `Case_Ignorable` ranges (DerivedCoreProperties), sorted by lo.
    pub case_ignorable: Vec<(u32, u32)>,
}

/// Parse `CaseFolding.txt`, `SpecialCasing.txt`, `UnicodeData.txt` and
/// `DerivedCoreProperties.txt` into the locale-independent case-mapping tables.
///
/// Locale rules are deliberately excluded (D253: no-locale): language-tagged
/// SpecialCasing entries (tr/az/lt) and Turkic folding (status T) are dropped.
/// The single context rule (Final_Sigma) is NOT baked into the table — the
/// default σ mapping is kept and `case.nv` applies the ς form contextually using
/// the `Cased`/`Case_Ignorable` ranges below.
pub fn parse_case_tables(ucd_dir: &Path) -> anyhow::Result<CaseTables> {
    let read = |name: &str| -> anyhow::Result<String> {
        let p = ucd_dir.join(name);
        std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", p.display(), e))
    };

    // --- UnicodeData.txt simple mappings: [12]=upper [13]=lower [14]=title ---
    let mut upper: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut lower: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut title: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for line in read("UnicodeData.txt")?.lines() {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 15 {
            continue;
        }
        let cp = match u32::from_str_radix(f[0], 16) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Ok(u) = u32::from_str_radix(f[12].trim(), 16) {
            if u != cp {
                upper.insert(cp, vec![u]);
            }
        }
        if let Ok(l) = u32::from_str_radix(f[13].trim(), 16) {
            if l != cp {
                lower.insert(cp, vec![l]);
            }
        }
        // [14]=simple titlecase; UCD convention: empty field defaults to the
        // simple uppercase mapping, so fall back to f[12] when [14] is blank.
        let tfield = if f[14].trim().is_empty() { f[12].trim() } else { f[14].trim() };
        if let Ok(t) = u32::from_str_radix(tfield, 16) {
            if t != cp {
                title.insert(cp, vec![t]);
            }
        }
    }

    // --- SpecialCasing.txt: full mappings override the simple ones ---
    // "code; lower; title; upper; (condition;)? # comment". A non-empty condition
    // field (Final_Sigma / tr / az / lt / ...) marks a conditional/locale entry —
    // skipped here (Final_Sigma is handled in case.nv).
    for line in read("SpecialCasing.txt")?.lines() {
        let core = line.split('#').next().unwrap_or("").trim();
        if core.is_empty() {
            continue;
        }
        let parts: Vec<&str> = core.split(';').map(|s| s.trim()).collect();
        if parts.len() < 4 {
            continue;
        }
        let conditional = parts.get(4).map_or(false, |s| !s.is_empty());
        if conditional {
            continue;
        }
        let cp = match u32::from_str_radix(parts[0], 16) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let lo = parse_cps(parts[1]);
        let ti = parse_cps(parts[2]);
        let up = parse_cps(parts[3]);
        if lo.as_slice() != [cp] {
            lower.insert(cp, lo);
        }
        if ti.as_slice() != [cp] {
            title.insert(cp, ti);
        }
        if up.as_slice() != [cp] {
            upper.insert(cp, up);
        }
    }

    // --- CaseFolding.txt: full folding = status C (common) + F (full) ---
    // S (simple) and T (Turkic, locale) are excluded.
    let mut fold: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for line in read("CaseFolding.txt")?.lines() {
        let core = line.split('#').next().unwrap_or("").trim();
        if core.is_empty() {
            continue;
        }
        let parts: Vec<&str> = core.split(';').map(|s| s.trim()).collect();
        if parts.len() < 3 {
            continue;
        }
        if parts[1] != "C" && parts[1] != "F" {
            continue;
        }
        let cp = match u32::from_str_radix(parts[0], 16) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let m = parse_cps(parts[2]);
        if m.as_slice() != [cp] {
            fold.insert(cp, m);
        }
    }

    // --- DerivedCoreProperties.txt: Cased + Case_Ignorable (Final_Sigma ctx) ---
    let mut cased: Vec<(u32, u32)> = Vec::new();
    let mut case_ignorable: Vec<(u32, u32)> = Vec::new();
    for line in read("DerivedCoreProperties.txt")?.lines() {
        let core = line.split('#').next().unwrap_or("").trim();
        if core.is_empty() {
            continue;
        }
        let parts: Vec<&str> = core.split(';').collect();
        if parts.len() < 2 {
            continue;
        }
        match parts[1].trim() {
            "Cased" => {
                if let Some(r) = parse_range_pair(parts[0]) {
                    cased.push(r);
                }
            }
            "Case_Ignorable" => {
                if let Some(r) = parse_range_pair(parts[0]) {
                    case_ignorable.push(r);
                }
            }
            _ => {}
        }
    }
    cased.sort_by_key(|&(lo, _)| lo);
    case_ignorable.sort_by_key(|&(lo, _)| lo);

    Ok(CaseTables { fold, lower, upper, title, cased, case_ignorable })
}

fn emit_range_pairs(ranges: &[(u32, u32)]) -> String {
    ranges
        .iter()
        .map(|&(lo, hi)| format!("{:x},{:x}", lo, hi))
        .collect::<Vec<_>>()
        .join(";")
}

/// Render `std/unicode/case_data.nv` (peer of case.nv). Mapping tables reuse the
/// `cp:d1,d2;..` format (parsed by `parse_decomp_table`); Cased/Case_Ignorable use
/// `lo,hi;..` range pairs (binary search). Sorted/keyed deterministically.
pub fn render_case_data_nv(t: &CaseTables, version: &str) -> String {
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode`. DO NOT EDIT BY HAND.\n");
    out.push_str("// Source: UCD CaseFolding.txt + SpecialCasing.txt + UnicodeData.txt\n");
    out.push_str("//         + DerivedCoreProperties.txt (Cased / Case_Ignorable).\n");
    out.push_str("// Regenerate: nova-codegen unicode --ucd-dir <UCD-dir> --root <repo>\n");
    out.push_str("//\n");
    out.push_str("// FOLD/LOWER/UPPER/TITLE_DATA : \"cp:m1,m2,..;..\" full mapping seq\n");
    out.push_str("//   (locale-independent: SpecialCasing conditional/language entries excluded;\n");
    out.push_str("//    Turkic fold status T excluded; Final_Sigma handled in case.nv).\n");
    out.push_str("// CASED_DATA / CASE_IGNORABLE_DATA : \"lo,hi;..\" ranges (Final_Sigma context).\n");
    out.push('\n');
    out.push_str("module std.unicode\n");
    out.push('\n');
    out.push_str(&format!(
        "export const CASE_UNICODE_VERSION str = \"{}\"\n\n",
        version
    ));
    out.push_str(&format!("const FOLD_DATA str = \"{}\"\n\n", emit_map_seq(&t.fold)));
    out.push_str(&format!("const LOWER_DATA str = \"{}\"\n\n", emit_map_seq(&t.lower)));
    out.push_str(&format!("const UPPER_DATA str = \"{}\"\n\n", emit_map_seq(&t.upper)));
    out.push_str(&format!("const TITLE_DATA str = \"{}\"\n\n", emit_map_seq(&t.title)));
    out.push_str(&format!("const CASED_DATA str = \"{}\"\n\n", emit_range_pairs(&t.cased)));
    out.push_str(&format!(
        "const CASE_IGNORABLE_DATA str = \"{}\"\n",
        emit_range_pairs(&t.case_ignorable)
    ));
    out
}

/// Render `nova_tests/plan152_4/case_conformance.nv` — breadth check that the
/// runtime parses+applies every mapping. For a uniform spread-sample of all mapped
/// codepoints, assert `fold_case`/`to_uppercase`/`to_lowercase` of the isolated
/// codepoint match the UCD-derived expected sequence. (Contextual Final_Sigma is
/// exercised by the hand-authored `case.nv` word cases — an isolated Σ has no
/// preceding cased char, so its lowercase is the default σ here, matching.)
///
/// SCOPE (important — this check is self-referential for SELECTION): the expected
/// values are derived from the SAME `parse_case_tables` that emits `case_data.nv`,
/// so this validates the runtime PARSER + lookup + multi-codepoint emission
/// (round-trip), NOT the table SELECTION. A selection regression (wrong UnicodeData
/// column, dropping a CaseFolding C/F row, or wrongly keeping a Turkic/locale
/// SpecialCasing entry) would shift both sides together and still pass. Selection
/// correctness is pinned independently by the hand-typed oracle asserts in
/// `case.nv` (Turkic-exclusion I→i, İ→i̇, field-index sentinels, 3-cp expansions,
/// Final_Sigma with case-ignorable interleaving).
pub fn render_case_conformance_nv(ucd_dir: &Path, limit: usize) -> anyhow::Result<String> {
    let t = parse_case_tables(ucd_dir)?;
    let esc = |cps: &[u32]| -> String {
        let mut s = String::new();
        for &cp in cps {
            s.push_str(&format!("\\u{{{:x}}}", cp));
        }
        s
    };
    let mut keys: BTreeSet<u32> = BTreeSet::new();
    keys.extend(t.fold.keys().copied());
    keys.extend(t.lower.keys().copied());
    keys.extend(t.upper.keys().copied());
    keys.extend(t.title.keys().copied());
    let keys: Vec<u32> = keys.into_iter().collect();
    let total = keys.len();
    // Uniform spread across the WHOLE key range (NOT a contiguous head): pick
    // `take` indices evenly via i*total/take, so the committed sample always spans
    // low ASCII through the ligatures (U+FB00..), Greek iota-subscript titlecase
    // (U+1F88..) and the supplementary-plane cased scripts (Deseret/Adlam/…).
    let take = limit.min(total);
    let mut sel: Vec<u32> = Vec::with_capacity(take);
    if take == total {
        sel.extend_from_slice(&keys);
    } else {
        for i in 0..take {
            sel.push(keys[i * total / take]);
        }
    }
    let expect = |m: &BTreeMap<u32, Vec<u32>>, cp: u32| -> String {
        match m.get(&cp) {
            Some(v) => esc(v),
            None => esc(&[cp]),
        }
    };
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen unicode --emit-conformance`. DO NOT EDIT.\n");
    out.push_str("// Case-mapping breadth conformance (UCD-derived): for a uniform spread sample\n");
    out.push_str("// of all mapped codepoints, assert fold_case/to_uppercase/to_lowercase of the\n");
    out.push_str("// isolated codepoint equal the expected full mapping sequence.\n");
    out.push_str("// NOTE: self-referential for SELECTION (expected derived from the same generator)\n");
    out.push_str("//   -> validates the runtime PARSER+lookup+emission, not table selection; the\n");
    out.push_str("//   no-locale SELECTION is pinned by the hand-typed oracle in case.nv.\n");
    out.push_str(&format!(
        "// Coverage: uniform spread = {} of {} mapped codepoints (low ASCII .. supplementary).\n",
        sel.len(), total
    ));
    out.push_str("module plan152_4.case_conformance\n\n");
    out.push_str("import std.unicode.{fold_case, to_uppercase, to_lowercase, to_titlecase}\n\n");
    const CHUNK: usize = 250;
    for (ci, chunk) in sel.chunks(CHUNK).enumerate() {
        out.push_str(&format!(
            "test \"case-mapping conformance (chunk {}, {} cps)\" {{\n",
            ci, chunk.len()
        ));
        for &cp in chunk {
            let src = esc(&[cp]);
            out.push_str(&format!(
                "    assert(fold_case(\"{}\") == \"{}\")\n",
                src, expect(&t.fold, cp)
            ));
            out.push_str(&format!(
                "    assert(to_uppercase(\"{}\") == \"{}\")\n",
                src, expect(&t.upper, cp)
            ));
            out.push_str(&format!(
                "    assert(to_lowercase(\"{}\") == \"{}\")\n",
                src, expect(&t.lower, cp)
            ));
            // Isolated cp is its own single-char word: to_titlecase(cp) titlecases
            // the lone (first) cased char → the per-cp title mapping.
            out.push_str(&format!(
                "    assert(to_titlecase(\"{}\") == \"{}\")\n",
                src, expect(&t.title, cp)
            ));
        }
        out.push_str("}\n\n");
    }
    Ok(out)
}
