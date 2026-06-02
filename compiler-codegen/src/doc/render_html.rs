//! Plan 45 Ф.31.1 — HTML output MVP.
//!
//! Single-page HTML render с minimal embedded CSS.
//! Layout:
//! - `<head>` с title + inline `<style>` (light theme, ~50 lines CSS)
//! - `<body>` с sidebar (module index) + main content (per-module sections)
//! - Per-item: signature в `<pre>`, summary, sections, badges
//!
//! **Out-of-scope для MVP:**
//! - Search index (lunr.js) — Ф.31.2
//! - Theme switcher (dark mode) — Ф.31.3
//! - Multi-page output (each module → separate file) — Ф.31.4
//! - Syntax highlighting (highlight.js) — Plan 45.A round 3
//!
//! **Design choices:**
//! - Embedded CSS (no separate file): single-page принцип, no bundle complexity
//! - Pure HTML5 + CSS3 (no JS) — works в browsers и в text-mode browsers
//! - HTML escape всех user content для XSS safety
//! - Stable byte-for-byte output (deterministic order, no timestamps)

use super::doctree::*;
use std::fmt::Write;

/// Plan 45 Ф.31.1 — entry point: DocTree → HTML string (single-page).
pub fn render(tree: &DocTree) -> String {
    let mut out = String::with_capacity(8192);
    write_html_head(&mut out, tree);
    out.push_str("<body>\n");
    write_sidebar(&mut out, tree, None);
    write_main(&mut out, tree);
    // Plan 45 Ф.31.2: inline JS для search filter.
    out.push_str("<script>\n");
    out.push_str(EMBEDDED_JS);
    out.push_str("</script>\n");
    out.push_str("</body>\n</html>\n");
    out
}

/// Plan 45 Ф.31.4 — multi-page HTML output.
///
/// Возвращает map `filename → html_content`:
/// - `index.html` — overview всех modules с links на per-module pages.
/// - `<module.path>.html` — per-module page (один module = один file).
/// - Plan 45 Ф.31.6: `sitemap.xml` — site map для SEO/crawlers (если
///   `NOVA_DOC_SITE_URL` env задан — emit'ит absolute URLs; иначе
///   relative paths — useful для local browsing).
///
/// Каждая page содержит свою sidebar (с links на все modules через
/// `<page>.html#anchor`), embedded CSS+JS (одинаковый везде для simplicity).
///
/// Cross-page links: если link target — item в другом module,
/// rewriting через `<module>.html#anchor`. Within same module — `#anchor`.
///
/// CLI usage: `nova doc <dir> --format html --output-dir <out>`.
pub fn render_multipage(tree: &DocTree) -> std::collections::BTreeMap<String, String> {
    let mut pages = std::collections::BTreeMap::new();
    // Build cross-page link index: item_id → "<module>.html#anchor".
    let mut item_pages: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for m in &tree.modules {
        let mod_file = module_filename(&m.path);
        for it in &m.items {
            item_pages.insert(it.id.clone(),
                format!("{}#{}", mod_file, item_anchor(&it.id)));
        }
    }

    // index.html — overview.
    pages.insert("index.html".to_string(), render_index(tree));

    // Per-module pages.
    for m in &tree.modules {
        let filename = module_filename(&m.path);
        pages.insert(filename, render_module_page(tree, m, &item_pages));
    }

    // Plan 45 Ф.31.6: sitemap.xml.
    pages.insert("sitemap.xml".to_string(), render_sitemap(tree));

    pages
}

/// Plan 45 Ф.31.6 — sitemap.xml generation.
///
/// Standard sitemaps.org/0.9 format. Emits entries для:
/// - index.html (priority 1.0)
/// - per-module pages (priority 0.8)
///
/// Base URL берётся из `NOVA_DOC_SITE_URL` env var (e.g.
/// `https://docs.nova-lang.org/api`). Если не задан — emit relative paths
/// (useful для local file:// browsing или CI artifact preview).
fn render_sitemap(tree: &DocTree) -> String {
    let base = std::env::var("NOVA_DOC_SITE_URL").ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_default();
    let url = |relative: &str| -> String {
        if base.is_empty() {
            relative.to_string()
        } else {
            format!("{}/{}", base, relative)
        }
    };
    let mut out = String::with_capacity(1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    // index.html — priority 1.0.
    let _ = writeln!(out,
        "  <url><loc>{}</loc><priority>1.0</priority></url>",
        html_escape(&url("index.html")));
    // Per-module — priority 0.8.
    for m in &tree.modules {
        let filename = module_filename(&m.path);
        let _ = writeln!(out,
            "  <url><loc>{}</loc><priority>0.8</priority></url>",
            html_escape(&url(&filename)));
    }
    out.push_str("</urlset>\n");
    out
}

/// Plan 45 Ф.31.4 — index.html (workspace overview).
fn render_index(tree: &DocTree) -> String {
    let mut out = String::with_capacity(4096);
    write_html_head(&mut out, tree);
    out.push_str("<body>\n");
    // Sidebar: links на все module pages.
    out.push_str("<nav class=\"sidebar\">\n");
    out.push_str("<strong>nova doc</strong>\n");
    out.push_str("<input type=\"text\" class=\"search-box\" id=\"nova-search\" placeholder=\"Search…\" autocomplete=\"off\">\n");
    out.push_str("<h2>Modules</h2>\n<ul>\n");
    for m in &tree.modules {
        let path = m.path.join(".");
        let file = module_filename(&m.path);
        let _ = writeln!(out, "  <li><a href=\"{}\">{}</a></li>",
            html_escape(&file), html_escape(&path));
    }
    out.push_str("</ul>\n</nav>\n");
    // Main: brief listing.
    out.push_str("<main>\n");
    out.push_str("<h1>API documentation</h1>\n");
    let _ = writeln!(out, "<p>{} module(s) documented.</p>", tree.modules.len());
    out.push_str("<h2>Modules</h2>\n<ul>\n");
    for m in &tree.modules {
        let path = m.path.join(".");
        let file = module_filename(&m.path);
        let summary = m.summary.as_deref().unwrap_or("");
        let _ = writeln!(out,
            "<li><a href=\"{}\"><code>{}</code></a> — {}</li>",
            html_escape(&file), html_escape(&path), html_escape(summary));
    }
    out.push_str("</ul>\n</main>\n");
    out.push_str("<script>\n");
    out.push_str(EMBEDDED_JS);
    out.push_str("</script>\n");
    out.push_str("</body>\n</html>\n");
    out
}

/// Plan 45 Ф.31.4 — per-module page rendering.
fn render_module_page(
    tree: &DocTree,
    m: &DocModule,
    item_pages: &std::collections::HashMap<String, String>,
) -> String {
    let mut out = String::with_capacity(8192);
    // Mini DocTree-like header.
    let title = format!("nova doc — {}", m.path.join("."));
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(out, "<title>{}</title>", html_escape(&title));
    out.push_str("<style>\n");
    out.push_str(EMBEDDED_CSS);
    out.push_str("</style>\n");
    out.push_str("</head>\n");
    out.push_str("<body>\n");
    // Sidebar — навигация по всем modules + items текущего module.
    out.push_str("<nav class=\"sidebar\">\n");
    out.push_str("<strong><a href=\"index.html\">nova doc</a></strong>\n");
    out.push_str("<input type=\"text\" class=\"search-box\" id=\"nova-search\" placeholder=\"Search…\" autocomplete=\"off\">\n");
    out.push_str("<h2>Modules</h2>\n<ul>\n");
    for other in &tree.modules {
        let path = other.path.join(".");
        let file = module_filename(&other.path);
        let current_marker = if std::ptr::eq(other, m) { " <strong>(here)</strong>" } else { "" };
        let _ = writeln!(out, "  <li><a href=\"{}\">{}</a>{}</li>",
            html_escape(&file), html_escape(&path), current_marker);
    }
    out.push_str("</ul>\n");
    if !m.items.is_empty() {
        out.push_str("<h2>Items</h2>\n<ul>\n");
        for it in &m.items {
            let anchor = item_anchor(&it.id);
            let _ = writeln!(out, "  <li><a href=\"#{}\">{}</a></li>",
                anchor, html_escape(&it.name));
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</nav>\n");
    // Main — текущий module rendered (с cross-page links).
    out.push_str("<main>\n");
    write_module_with_xpage_links(&mut out, m, &tree.links, item_pages);
    out.push_str("</main>\n");
    out.push_str("<script>\n");
    out.push_str(EMBEDDED_JS);
    out.push_str("</script>\n");
    out.push_str("</body>\n</html>\n");
    out
}

/// Same as `write_module` но links используют cross-page URLs из `item_pages`.
fn write_module_with_xpage_links(
    out: &mut String,
    m: &DocModule,
    links: &[DocLink],
    item_pages: &std::collections::HashMap<String, String>,
) {
    // Build per-page link map: text → URL (single-page anchor OR cross-page).
    let effective_links: Vec<DocLink> = links.iter().map(|l| {
        let mut effective = l.clone();
        // Если target_url есть (external) — оставить.
        // Если target_id есть — substitute cross-page URL.
        if effective.target_url.is_none() {
            if let Some(tid) = &effective.target_id {
                if let Some(url) = item_pages.get(tid) {
                    effective.target_url = Some(url.clone());
                }
            }
        }
        effective
    }).collect();
    // Filter to keep relevant only (perf). Не важно для correctness.
    let _ = effective_links.len();
    write_module(out, m, &effective_links);
}

fn module_filename(path: &[String]) -> String {
    if path.is_empty() {
        "_root.html".to_string()
    } else {
        format!("{}.html", path.join("."))
    }
}

fn write_html_head(out: &mut String, tree: &DocTree) {
    let title = if tree.modules.len() == 1 {
        format!("nova doc — {}", tree.modules[0].path.join("."))
    } else {
        format!("nova doc — {} modules", tree.modules.len())
    };
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(out, "<title>{}</title>", html_escape(&title));
    out.push_str("<style>\n");
    out.push_str(EMBEDDED_CSS);
    out.push_str("</style>\n");
    out.push_str("</head>\n");
}

/// Plan 45 Ф.31.1 (light) + Ф.31.3 (dark mode via CSS variables + media query).
const EMBEDDED_CSS: &str = r#"
  :root {
    --bg: #fafafa;
    --fg: #1f1f1f;
    --sidebar-bg: #f0f0f0;
    --border: #ddd;
    --muted: #666;
    --code-bg: #f0f0f0;
    --pre-bg: #f5f5f5;
    --pre-border: #e0e0e0;
    --link: #0066cc;
    --section-color: #333;
    --item-color: #444;
    --summary-color: #444;
    --quote-border: #d0d0d0;
    --quote-color: #555;
    --target-bg: #fff8e0;
    --search-bg: #fff;
    --search-border: #ccc;
    --dim-opacity: 0.25;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #1a1a1a;
      --fg: #e0e0e0;
      --sidebar-bg: #222;
      --border: #333;
      --muted: #888;
      --code-bg: #2a2a2a;
      --pre-bg: #1f1f1f;
      --pre-border: #3a3a3a;
      --link: #4d9fff;
      --section-color: #ddd;
      --item-color: #ccc;
      --summary-color: #bbb;
      --quote-border: #444;
      --quote-color: #aaa;
      --target-bg: #2a2a1a;
      --search-bg: #222;
      --search-border: #444;
    }
  }
  body { font: 14px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
         color: var(--fg); background: var(--bg); margin: 0; display: grid;
         grid-template-columns: 260px 1fr; min-height: 100vh; }
  nav.sidebar { background: var(--sidebar-bg); padding: 1rem;
                border-right: 1px solid var(--border);
                overflow-y: auto; position: sticky; top: 0; height: 100vh; }
  nav.sidebar h2 { font-size: 0.85rem; text-transform: uppercase; color: var(--muted);
                   margin: 1rem 0 0.3rem; letter-spacing: 0.05em; }
  nav.sidebar ul { list-style: none; padding-left: 0; margin: 0; }
  nav.sidebar li { margin: 0.15rem 0; }
  nav.sidebar a { color: var(--link); text-decoration: none; font-size: 0.9rem; }
  nav.sidebar a:hover { text-decoration: underline; }
  main { padding: 2rem; max-width: 980px; }
  h1 { font-size: 1.6rem; border-bottom: 1px solid var(--border); padding-bottom: 0.3rem; }
  h2 { font-size: 1.3rem; margin-top: 2rem; color: var(--section-color); }
  h3 { font-size: 1.05rem; margin-top: 1.5rem; color: var(--item-color);
       font-family: ui-monospace, "Cascadia Code", "Consolas", monospace; }
  h4 { font-size: 0.95rem; color: var(--quote-color); margin-top: 1rem; }
  pre { background: var(--pre-bg); border: 1px solid var(--pre-border); border-radius: 4px;
        padding: 0.8rem; overflow-x: auto;
        font: 13px/1.45 ui-monospace, "Cascadia Code", "Consolas", monospace; }
  code { background: var(--code-bg); padding: 0.1rem 0.3rem; border-radius: 3px;
         font: 0.9em ui-monospace, "Cascadia Code", "Consolas", monospace; }
  pre code { background: none; padding: 0; }
  a { color: var(--link); text-decoration: none; }
  a:hover { text-decoration: underline; }
  .badge { display: inline-block; padding: 0.1rem 0.4rem; border-radius: 3px;
           font-size: 0.75rem; margin-right: 0.3rem; }
  .badge-stable { background: #e0f2e0; color: #2a5f2a; }
  .badge-unstable { background: #fff4cc; color: #8a6800; }
  .badge-experimental { background: #ffe0e0; color: #8a2a2a; }
  .badge-deprecated { background: #444; color: #fff; }
  .badge-realtime { background: #e0e8ff; color: #2a4f8a; }
  .badge-pure { background: #d0e8ff; color: #1a3a6a; }
  .badge-forbid { background: #ffd0d0; color: #6a1a1a; }
  .item { border-left: 3px solid var(--border); padding-left: 1rem; margin: 1.5rem 0;
          transition: opacity 0.15s ease; }
  .item:target { border-left-color: var(--link); background: var(--target-bg);
                 padding: 0.5rem 1rem; }
  .item.dim { opacity: var(--dim-opacity); }
  .src-link { font-size: 0.75rem; color: var(--muted); margin-left: 0.5rem; }
  .summary { color: var(--summary-color); font-style: italic; margin: 0.3rem 0; }
  blockquote { border-left: 4px solid var(--quote-border); padding-left: 1rem;
               color: var(--quote-color); margin: 0.5rem 0; }
  /* Plan 45 Ф.31.2 — search bar. */
  .search-box { width: 100%; padding: 0.4rem 0.6rem; margin-bottom: 0.8rem;
                background: var(--search-bg); color: var(--fg);
                border: 1px solid var(--search-border); border-radius: 4px;
                font-size: 0.9rem; box-sizing: border-box; }
  .search-box:focus { outline: 2px solid var(--link); outline-offset: -1px; }
  /* Plan 45 Ф.31.5 — syntax highlighting classes. */
  .tok-kw { color: #c084fc; font-weight: 600; }
  .tok-type { color: #38bdf8; }
  .tok-str { color: #a3e635; }
  .tok-num { color: #fbbf24; }
  .tok-comment { color: var(--muted); font-style: italic; }
  @media (prefers-color-scheme: light) {
    .tok-kw { color: #8b5cf6; }
    .tok-type { color: #0284c7; }
    .tok-str { color: #65a30d; }
    .tok-num { color: #b45309; }
  }
  @media (max-width: 720px) {
    body { grid-template-columns: 1fr; }
    nav.sidebar { position: static; height: auto; }
  }
"#;

/// Plan 45 Ф.31.2 — inline JS substring search filter + Ф.31.5 syntax highlighter.
const EMBEDDED_JS: &str = r##"
  (function() {
    // Plan 45 Ф.31.5 — syntax highlighting для Nova code blocks.
    // Regex-based tokenizer (без deps); applied к <pre><code> blocks.
    var KEYWORDS = ['fn','let','const','mut','if','else','match','while','for','in',
      'return','break','continue','export','import','module','type','protocol',
      'effect','handler','with','spawn','await','async','defer','errdefer',
      'requires','ensures','decreases','invariant','axiom','ghost','assume',
      'forall','exists','newtype','alias','as','true','false','self','forbid',
      'realtime','nogc','pure','stable','unstable','experimental','deprecated',
      'public','private','interrupt','resume','throw','yield','where','impl'];
    var TYPES = ['int','str','bool','float','char','unit','void','any',
      'i8','i16','i32','i64','u8','u16','u32','u64','f32','f64'];
    function tokenize(text) {
      var out = '';
      var i = 0;
      while (i < text.length) {
        var c = text[i];
        // Comment // ... \n
        if (c === '/' && text[i+1] === '/') {
          var end = text.indexOf('\n', i);
          if (end < 0) end = text.length;
          out += '<span class="tok-comment">' + escapeHtml(text.slice(i, end)) + '</span>';
          i = end;
          continue;
        }
        // String "..."
        if (c === '"') {
          var j = i + 1;
          while (j < text.length && text[j] !== '"') {
            if (text[j] === '\\' && j + 1 < text.length) j++;
            j++;
          }
          j = Math.min(j + 1, text.length);
          out += '<span class="tok-str">' + escapeHtml(text.slice(i, j)) + '</span>';
          i = j;
          continue;
        }
        // Number
        if (c >= '0' && c <= '9') {
          var j = i;
          while (j < text.length && /[0-9._]/.test(text[j])) j++;
          out += '<span class="tok-num">' + escapeHtml(text.slice(i, j)) + '</span>';
          i = j;
          continue;
        }
        // Identifier / keyword
        if (/[a-zA-Z_]/.test(c)) {
          var j = i;
          while (j < text.length && /[a-zA-Z0-9_]/.test(text[j])) j++;
          var word = text.slice(i, j);
          if (KEYWORDS.indexOf(word) >= 0) {
            out += '<span class="tok-kw">' + word + '</span>';
          } else if (TYPES.indexOf(word) >= 0) {
            out += '<span class="tok-type">' + word + '</span>';
          } else {
            out += escapeHtml(word);
          }
          i = j;
          continue;
        }
        out += escapeHtml(c);
        i++;
      }
      return out;
    }
    function escapeHtml(s) {
      return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }
    // Apply на все <pre><code> blocks (skip уже highlighted).
    document.querySelectorAll('pre > code').forEach(function(el) {
      if (el.dataset.hl === '1') return;
      // textContent декодирует existing HTML entities; берём raw text.
      var raw = el.textContent;
      el.innerHTML = tokenize(raw);
      el.dataset.hl = '1';
    });

    // Plan 45 Ф.31.2 — search filter.
    var box = document.getElementById('nova-search');
    if (!box) return;
    var items = document.querySelectorAll('article.item');
    var sidebarLinks = document.querySelectorAll('nav.sidebar a[href^="#"]');
    box.addEventListener('input', function() {
      var q = box.value.trim().toLowerCase();
      items.forEach(function(it) {
        if (!q) { it.classList.remove('dim'); return; }
        var hay = (it.textContent || '').toLowerCase();
        if (hay.indexOf(q) >= 0) { it.classList.remove('dim'); }
        else { it.classList.add('dim'); }
      });
      sidebarLinks.forEach(function(a) {
        if (!q) { a.style.display = ''; return; }
        var t = (a.textContent || '').toLowerCase();
        a.style.display = (t.indexOf(q) >= 0) ? '' : 'none';
      });
    });
  })();
"##;

fn write_sidebar(out: &mut String, tree: &DocTree, _multipage_marker: Option<&DocModule>) {
    out.push_str("<nav class=\"sidebar\">\n");
    out.push_str("<strong>nova doc</strong>\n");
    // Plan 45 Ф.31.2: search box.
    out.push_str("<input type=\"text\" class=\"search-box\" id=\"nova-search\" placeholder=\"Search…\" autocomplete=\"off\">\n");
    for m in &tree.modules {
        let _ = writeln!(out, "<h2>{}</h2>", html_escape(&m.path.join(".")));
        if m.items.is_empty() { continue; }
        out.push_str("<ul>\n");
        for it in &m.items {
            let anchor = item_anchor(&it.id);
            let _ = writeln!(out, "  <li><a href=\"#{}\">{}</a></li>",
                anchor, html_escape(&it.name));
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</nav>\n");
}

fn write_main(out: &mut String, tree: &DocTree) {
    out.push_str("<main>\n");
    for m in &tree.modules {
        write_module(out, m, &tree.links);
    }
    out.push_str("</main>\n");
}

fn write_module(out: &mut String, m: &DocModule, links: &[DocLink]) {
    let module_anchor = m.path.join("-");
    let _ = writeln!(out, "<section id=\"mod-{}\">", html_escape(&module_anchor));
    let _ = writeln!(out, "<h1>{}</h1>", html_escape(&m.path.join(".")));
    if let Some(s) = &m.summary {
        let _ = writeln!(out, "<p class=\"summary\">{}</p>", rewrite_and_escape(s, links));
    }
    if let Some(d) = &m.description {
        let _ = writeln!(out, "<p>{}</p>", rewrite_and_escape(d, links));
    }
    for it in &m.items {
        write_item(out, it, links);
    }
    out.push_str("</section>\n");
}

fn write_item(out: &mut String, it: &DocItem, links: &[DocLink]) {
    let anchor = item_anchor(&it.id);
    let _ = writeln!(out, "<article class=\"item\" id=\"{}\">", html_escape(&anchor));
    let _ = writeln!(out, "<h3>{}</h3>", html_escape(&it.name));
    // Badges row.
    write_badges(out, it);
    // Signature / definition.
    write_kind(out, it);
    // Summary + description.
    if let Some(s) = &it.summary {
        let _ = writeln!(out, "<p class=\"summary\">{}</p>", rewrite_and_escape(s, links));
    }
    if let Some(d) = &it.description {
        let _ = writeln!(out, "<p>{}</p>", rewrite_and_escape(d, links));
    }
    // Sections (canonical order).
    const CANONICAL_ORDER: &[(&str, &str)] = &[
        ("examples", "Examples"),
        ("errors", "Errors"),
        ("panics", "Panics"),
        ("safety", "Safety"),
        ("effects", "Effects"),
        ("contracts", "Contracts"),
        ("since", "Since"),
        ("see also", "See also"),
        ("deprecated", "Deprecated"),
    ];
    for (key, title) in CANONICAL_ORDER {
        if let Some(body) = it.sections.get(*key) {
            let _ = writeln!(out, "<h4>{}</h4>", html_escape(title));
            let _ = writeln!(out, "<div>{}</div>", rewrite_and_escape(body, links));
        }
    }
    out.push_str("</article>\n");
}

fn write_badges(out: &mut String, it: &DocItem) {
    // Collect badges first, then emit if non-empty.
    let mut badges: Vec<(&'static str, String)> = Vec::new();
    if let Some(s) = &it.stability {
        let class = match s.tier {
            StabilityTier::Stable => "badge-stable",
            StabilityTier::Unstable => "badge-unstable",
            StabilityTier::Experimental => "badge-experimental",
        };
        badges.push((class, s.tier.as_str().to_string()));
    }
    if it.deprecation.is_some() {
        badges.push(("badge-deprecated", "deprecated".to_string()));
    }
    let cap = &it.capabilities;
    if cap.realtime_nogc {
        badges.push(("badge-realtime", "realtime nogc".to_string()));
    } else if cap.realtime {
        badges.push(("badge-realtime", "realtime".to_string()));
    }
    if cap.pure_fn {
        badges.push(("badge-pure", "pure".to_string()));
    }
    for f in &cap.forbid {
        badges.push(("badge-forbid", format!("forbid {}", f)));
    }
    if badges.is_empty() { return; }
    out.push_str("<p>");
    for (class, text) in &badges {
        let _ = write!(out, "<span class=\"badge {}\">{}</span>",
            class, html_escape(text));
    }
    out.push_str("</p>\n");
}

fn write_kind(out: &mut String, it: &DocItem) {
    out.push_str("<pre><code>");
    match &it.kind {
        ItemKind::Fn(sig) => {
            // Compact fn signature: fn name(params) -> ret
            out.push_str("fn ");
            out.push_str(&html_escape(&it.name));
            out.push('(');
            for (i, p) in sig.params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                let _ = write!(out, "{} {}", html_escape(&p.name), html_escape(&p.ty));
            }
            let _ = write!(out, ") -> {}", html_escape(&sig.return_type));
        }
        ItemKind::Const { ty, value } => {
            let _ = write!(out, "const {} {} = {}",
                html_escape(&it.name), html_escape(ty), html_escape(value));
        }
        ItemKind::Type(def) => {
            write_type_def(out, &it.name, def);
        }
        ItemKind::Effect { methods, axioms: _, handlers: _ } => {
            let _ = writeln!(out, "type {} effect {{", html_escape(&it.name));
            for m in methods {
                let _ = write!(out, "    fn {}(", html_escape(&m.name));
                for (i, p) in m.params.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    let _ = write!(out, "{} {}", html_escape(&p.name), html_escape(&p.ty));
                }
                let _ = writeln!(out, ") -> {}", html_escape(&m.return_type));
            }
            out.push('}');
        }
        ItemKind::Protocol { methods, implementors: _ } => {
            let _ = writeln!(out, "type {} protocol {{", html_escape(&it.name));
            for m in methods {
                let _ = write!(out, "    fn {}(", html_escape(&m.name));
                for (i, p) in m.params.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    let _ = write!(out, "{} {}", html_escape(&p.name), html_escape(&p.ty));
                }
                let _ = writeln!(out, ") -> {}", html_escape(&m.return_type));
            }
            out.push('}');
        }
        ItemKind::ReExport { source } => {
            let _ = write!(out, "export import {} (as {})", html_escape(source), html_escape(&it.name));
        }
    }
    out.push_str("</code></pre>\n");
}

fn write_type_def(out: &mut String, name: &str, def: &TypeDefinition) {
    match def {
        TypeDefinition::Alias(ty) => {
            let _ = write!(out, "type {} alias {}", html_escape(name), html_escape(ty));
        }
        TypeDefinition::Newtype { inner } => {
            let _ = write!(out, "type {} {}", html_escape(name), html_escape(inner));
        }
        TypeDefinition::Record(fields) => {
            let _ = writeln!(out, "type {} {{", html_escape(name));
            for f in fields {
                // Plan 124.5 (D220/D222): priv badge.
                let priv_kw = if f.priv_field { "priv " } else { "" };
                let _ = writeln!(out, "    {}{} {}", priv_kw, html_escape(&f.name), html_escape(&f.ty));
            }
            out.push('}');
        }
        TypeDefinition::Sum(variants) => {
            let _ = write!(out, "type {}", html_escape(name));
            for v in variants {
                let _ = write!(out, " | {}", html_escape(&v.name));
            }
        }
    }
}

/// Stable anchor для item ID: lowercase, `.`→`-`, `::`→`-`.
fn item_anchor(item_id: &str) -> String {
    item_id.to_lowercase().replace("::", "-").replace('.', "-")
}

/// HTML escape: `<`, `>`, `&`, `"`, `'`.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Rewrite intra-doc links `[Name]` к `<a href="#anchor">Name</a>` или external URL.
/// Then HTML-escape rest (preserving our `<a>` tags).
fn rewrite_and_escape(text: &str, links: &[DocLink]) -> String {
    // Build link map: text → (anchor или external URL).
    let mut map: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for l in links {
        if let Some(url) = &l.target_url {
            map.insert(l.text.as_str(), url.clone());
        } else if let Some(tid) = &l.target_id {
            map.insert(l.text.as_str(), format!("#{}", item_anchor(tid)));
        }
    }
    if map.is_empty() {
        return html_escape(text);
    }
    // Naive replacement: для каждого link, replace `[text]` на `<a href="...">text</a>`.
    // First HTML-escape full text, then substitute.
    let mut escaped = html_escape(text);
    for (link_text, target) in &map {
        let pat = format!("[{}]", link_text);
        let pat_escaped = html_escape(&pat);
        let repl = format!(
            "<a href=\"{}\">{}</a>",
            html_escape(target), html_escape(link_text)
        );
        escaped = escaped.replace(&pat_escaped, &repl);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_basic() {
        assert_eq!(html_escape("a < b > c"), "a &lt; b &gt; c");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn item_anchor_format() {
        assert_eq!(item_anchor("std.io::println"), "std-io-println");
        assert_eq!(item_anchor("mod::Type.method"), "mod-type-method");
    }
}
