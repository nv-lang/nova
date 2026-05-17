// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.A.2 — HTML dashboard generator.
//!
//! `nova bench dashboard --history-branch X --out dashboard/` →
//! static HTML files (index.html + per-bench pages) с echarts (CDN).
//!
//! Дизайн:
//!   - Static — no server, no DB, no build step (просто HTML files).
//!   - Echarts через CDN: `https://cdn.jsdelivr.net/npm/echarts@5.4.3/dist/echarts.min.js`.
//!   - Reads from history orphan branch (via bench::history API).
//!   - Time-series chart: median wall-clock per commit per bench.
//!   - Regression markers: красные точки на коммитах с regress alerts.
//!   - Sortable table: per-bench latest values + trend arrow.
//!   - Offline fallback: можно скачать echarts.min.js и заменить CDN URL.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::history;
use super::schema::RunResultParsed;

#[derive(Debug, Clone)]
pub struct DashboardOpts<'a> {
    pub repo: &'a Path,
    pub history_branch: String,
    pub out_dir: &'a Path,
    /// Cap на сколько entries показывать (newest first). Default 200.
    pub max_entries: usize,
    /// Custom echarts URL (default CDN); полезно для offline.
    pub echarts_url: String,
}

impl Default for DashboardOpts<'_> {
    fn default() -> Self {
        Self {
            repo: Path::new("."),
            history_branch: "bench-history".to_string(),
            out_dir: Path::new("dashboard"),
            max_entries: 200,
            echarts_url: "https://cdn.jsdelivr.net/npm/echarts@5.4.3/dist/echarts.min.js".to_string(),
        }
    }
}

pub fn generate(opts: DashboardOpts) -> Result<i32> {
    // 1. Read history entries.
    let entries = history::list(opts.repo, &opts.history_branch)?;
    if entries.is_empty() {
        return Err(anyhow!(
            "no entries в branch `{}` — run `nova bench history-add` сначала",
            opts.history_branch));
    }
    eprintln!("dashboard: found {} entries в branch `{}`",
        entries.len(), opts.history_branch);

    let take_n = entries.len().min(opts.max_entries);
    // Newest-first → reverse для chronological order.
    let mut chronological: Vec<history::HistoryEntry> = entries[..take_n].to_vec();
    chronological.reverse();

    // 2. Parse каждую entry в RunResultParsed.
    let mut runs: Vec<(history::HistoryEntry, RunResultParsed)> = Vec::new();
    for e in chronological {
        let content = history::read_entry(opts.repo, &opts.history_branch, &e.filename)
            .map_err(|err| anyhow!("read entry {}: {}", e.filename, err))?;
        let v: Value = serde_json::from_str(&content)
            .map_err(|err| anyhow!("parse {}: {}", e.filename, err))?;
        match RunResultParsed::from_json(&v) {
            Ok(r) => runs.push((e, r)),
            Err(err) => {
                eprintln!("dashboard: skip {} (schema mismatch: {})",
                    e.filename, err);
            }
        }
    }

    // 3. Collect bench names (union across runs).
    let mut bench_names: Vec<String> = Vec::new();
    for (_, run) in &runs {
        for b in &run.benches {
            if !bench_names.contains(&b.raw.name) {
                bench_names.push(b.raw.name.clone());
            }
        }
    }
    bench_names.sort();

    // 4. Build time-series data: для каждого bench — массив (timestamp,
    //    median_ns). Missing = null.
    let mut series_data: Vec<(String, Vec<(u64, Option<f64>)>)> = Vec::new();
    for name in &bench_names {
        let mut points = Vec::with_capacity(runs.len());
        for (entry, run) in &runs {
            let med = run.benches.iter()
                .find(|b| &b.raw.name == name)
                .map(|b| b.stats_ns.median);
            points.push((entry.timestamp_unix, med));
        }
        series_data.push((name.clone(), points));
    }

    // 5. Write output files.
    std::fs::create_dir_all(opts.out_dir)
        .map_err(|e| anyhow!("create dashboard dir: {}", e))?;

    // Index page — overview + time-series chart.
    let index_html = render_index(&runs, &series_data, &opts.echarts_url);
    std::fs::write(opts.out_dir.join("index.html"), index_html)
        .map_err(|e| anyhow!("write index.html: {}", e))?;

    // Per-bench detail pages.
    for (name, points) in &series_data {
        let safe = filename_for_bench(name);
        let html = render_bench_detail(name, points, &runs, &opts.echarts_url);
        std::fs::write(opts.out_dir.join(format!("bench-{}.html", safe)), html)
            .map_err(|e| anyhow!("write bench-{}.html: {}", safe, e))?;
    }

    // Raw data JSON (для consumers).
    let raw_json = serde_json::json!({
        "format_version": super::SCHEMA_VERSION,
        "branch": opts.history_branch,
        "entries_count": runs.len(),
        "bench_names": bench_names,
        "series": series_data.iter().map(|(name, pts)| {
            serde_json::json!({
                "name": name,
                "points": pts.iter().map(|(ts, m)| {
                    serde_json::json!({"ts": ts, "median_ns": m})
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    });
    std::fs::write(opts.out_dir.join("data.json"),
        serde_json::to_string_pretty(&raw_json)?)
        .map_err(|e| anyhow!("write data.json: {}", e))?;

    eprintln!("dashboard: wrote {} files в {}", series_data.len() + 2,
        opts.out_dir.display());
    eprintln!("dashboard: open {}/index.html", opts.out_dir.display());
    Ok(0)
}

fn filename_for_bench(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#39;")
}

fn fmt_ns_short(ns: f64) -> String {
    if ns < 1_000.0 { format!("{:.0} ns", ns) }
    else if ns < 1_000_000.0 { format!("{:.1} µs", ns / 1e3) }
    else if ns < 1_000_000_000.0 { format!("{:.1} ms", ns / 1e6) }
    else { format!("{:.2} s", ns / 1e9) }
}

fn render_index(
    runs: &[(history::HistoryEntry, RunResultParsed)],
    series_data: &[(String, Vec<(u64, Option<f64>)>)],
    echarts_url: &str,
) -> String {
    let timestamps: Vec<u64> = runs.iter().map(|(e, _)| e.timestamp_unix).collect();

    let timestamps_json = serde_json::to_string(&timestamps).unwrap_or("[]".to_string());

    let series_json = serde_json::to_string(&series_data.iter().map(|(name, pts)| {
        let data: Vec<serde_json::Value> = pts.iter().map(|(_, m)| {
            match m {
                Some(v) => serde_json::json!(*v),
                None => serde_json::Value::Null,
            }
        }).collect();
        serde_json::json!({
            "name": name,
            "type": "line",
            "data": data,
            "connectNulls": true,
            "smooth": false,
            "symbolSize": 6,
        })
    }).collect::<Vec<_>>()).unwrap_or("[]".to_string());

    let bench_links: String = series_data.iter().map(|(name, _)| {
        format!("<li><a href=\"bench-{}.html\">{}</a></li>",
            filename_for_bench(name), html_escape(name))
    }).collect::<Vec<_>>().join("\n");

    let runs_table: String = runs.iter().rev().take(20).map(|(e, run)| {
        let bench_count = run.benches.len();
        let host = run.metadata.hostname.as_deref().unwrap_or("?");
        let cpu = run.metadata.cpu_model.as_deref().unwrap_or("?");
        let dt = unix_to_iso(e.timestamp_unix);
        format!("<tr><td>{}</td><td><code>{}</code></td><td>{}</td>\
                 <td>{}</td><td>{}</td></tr>",
            dt, html_escape(&e.git_sha), bench_count,
            html_escape(host), html_escape(cpu))
    }).collect::<Vec<_>>().join("\n");

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Nova bench dashboard</title>
<script src="{echarts_url}"></script>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
        Roboto, sans-serif; margin: 0; padding: 1em 2em; color: #222; }}
header {{ border-bottom: 1px solid #ccc; padding-bottom: 1em;
         margin-bottom: 1em; }}
h1 {{ margin: 0; font-size: 1.5em; }}
.meta {{ color: #666; font-size: 0.9em; }}
#chart {{ width: 100%; height: 500px; margin-bottom: 2em;
         border: 1px solid #ddd; border-radius: 4px; }}
.bench-list {{ columns: 3; column-gap: 2em; }}
.bench-list li {{ padding: 0.2em 0; break-inside: avoid; }}
table {{ width: 100%; border-collapse: collapse; }}
th, td {{ text-align: left; padding: 0.4em 0.8em;
          border-bottom: 1px solid #eee; }}
th {{ background: #f4f4f4; font-weight: 600; }}
code {{ font-family: SFMono-Regular, Consolas, monospace; font-size: 0.9em; }}
section {{ margin-bottom: 2em; }}
</style>
</head>
<body>
<header>
<h1>Nova bench dashboard</h1>
<div class="meta">{n_runs} runs · {n_benches} benches · history orphan branch</div>
</header>

<section>
<h2>Time-series (median wall-clock per bench)</h2>
<div id="chart"></div>
</section>

<section>
<h2>Benchmarks</h2>
<ul class="bench-list">{bench_links}</ul>
</section>

<section>
<h2>Recent runs (last 20)</h2>
<table>
<thead>
<tr><th>Timestamp</th><th>Git SHA</th><th>Benches</th>
    <th>Host</th><th>CPU</th></tr>
</thead>
<tbody>{runs_table}</tbody>
</table>
</section>

<footer style="color:#999; font-size:0.85em; margin-top:3em;">
Generated by <code>nova bench dashboard</code> · Plan 57.A.2
</footer>

<script>
const ts = {timestamps_json};
const labels = ts.map(t => new Date(t*1000).toISOString().slice(5,16).replace('T',' '));
const seriesData = {series_json};
const chart = echarts.init(document.getElementById('chart'));
chart.setOption({{
    tooltip: {{ trigger: 'axis' }},
    legend: {{ type: 'scroll', orient: 'horizontal', top: 8 }},
    grid: {{ left: '5%', right: '4%', bottom: '14%', top: 70 }},
    xAxis: {{ type: 'category', data: labels,
              axisLabel: {{ rotate: -35 }} }},
    yAxis: {{ type: 'value', name: 'median ns (log)', nameLocation: 'middle',
              nameGap: 50, type: 'log' }},
    dataZoom: [
        {{ type: 'inside', start: 0, end: 100 }},
        {{ type: 'slider', start: 0, end: 100, bottom: 30 }}
    ],
    series: seriesData
}});
window.addEventListener('resize', () => chart.resize());
</script>
</body>
</html>"#,
        echarts_url = html_escape(echarts_url),
        n_runs = runs.len(),
        n_benches = series_data.len(),
        bench_links = bench_links,
        runs_table = runs_table,
        timestamps_json = timestamps_json,
        series_json = series_json,
    )
}

fn render_bench_detail(
    name: &str,
    points: &[(u64, Option<f64>)],
    runs: &[(history::HistoryEntry, RunResultParsed)],
    echarts_url: &str,
) -> String {
    let timestamps: Vec<u64> = points.iter().map(|(t, _)| *t).collect();
    let medians: Vec<serde_json::Value> = points.iter().map(|(_, m)| {
        match m {
            Some(v) => serde_json::json!(*v),
            None => serde_json::Value::Null,
        }
    }).collect();
    let timestamps_json = serde_json::to_string(&timestamps).unwrap_or("[]".to_string());
    let medians_json = serde_json::to_string(&medians).unwrap_or("[]".to_string());

    // Plan 57.E.1: latest bench для histogram + sidebar stats.
    let latest_bench = runs.iter().rev()
        .find_map(|(_, run)| run.benches.iter().find(|b| b.raw.name == name));

    // Histogram: 30 bins по raw_ns latest run.
    let histogram_json = if let Some(b) = latest_bench {
        let raw = &b.raw.raw_ns;
        if raw.is_empty() {
            "{\"bins\":[],\"counts\":[]}".to_string()
        } else {
            let min_v = *raw.iter().min().unwrap() as f64;
            let max_v = *raw.iter().max().unwrap() as f64;
            let n_bins = 30usize;
            let bin_width = if max_v > min_v { (max_v - min_v) / n_bins as f64 } else { 1.0 };
            let mut counts = vec![0usize; n_bins];
            let mut bin_centers = Vec::with_capacity(n_bins);
            for i in 0..n_bins {
                bin_centers.push(min_v + bin_width * (i as f64 + 0.5));
            }
            for &v in raw {
                let v_f = v as f64;
                let idx = if bin_width > 0.0 {
                    (((v_f - min_v) / bin_width) as usize).min(n_bins - 1)
                } else { 0 };
                counts[idx] += 1;
            }
            // Tukey outlier fences для visualization.
            let lo_fence = b.stats_ns.p25 - 1.5 * b.stats_ns.iqr;
            let hi_fence = b.stats_ns.p75 + 1.5 * b.stats_ns.iqr;
            serde_json::json!({
                "bins": bin_centers,
                "counts": counts,
                "lo_fence": lo_fence,
                "hi_fence": hi_fence,
                "median": b.stats_ns.median,
                "mean": b.stats_ns.mean,
            }).to_string()
        }
    } else {
        "{\"bins\":[],\"counts\":[]}".to_string()
    };

    // Plan 57.E.1: stats sidebar (latest values).
    let stats_sidebar_html = if let Some(b) = latest_bench {
        let st = &b.stats_ns;
        format!(r#"<aside class="stats-sidebar">
<h3>Latest stats</h3>
<dl>
<dt>median</dt><dd>{}</dd>
<dt>MAD</dt><dd>{}</dd>
<dt>mean</dt><dd>{}</dd>
<dt>stddev</dt><dd>{}</dd>
<dt>CI 95%</dt><dd>{} … {}</dd>
<dt>range</dt><dd>{} … {}</dd>
<dt>n</dt><dd>{}</dd>
<dt>outliers</dt><dd>{} low / {} high</dd>
</dl>
</aside>"#,
            fmt_ns_short(st.median),
            fmt_ns_short(st.mad),
            fmt_ns_short(st.mean),
            fmt_ns_short(st.stddev),
            fmt_ns_short(st.ci95_lo),
            fmt_ns_short(st.ci95_hi),
            fmt_ns_short(st.min),
            fmt_ns_short(st.max),
            st.n,
            st.outliers_low,
            st.outliers_high,
        )
    } else {
        String::new()
    };

    // Plan 57.E.1: comparison view — latest vs oldest (если >1 runs).
    let comparison_html = if runs.len() >= 2 {
        let latest = runs.iter().rev()
            .find_map(|(_, r)| r.benches.iter().find(|b| b.raw.name == name));
        let oldest = runs.iter()
            .find_map(|(_, r)| r.benches.iter().find(|b| b.raw.name == name));
        if let (Some(l), Some(o)) = (latest, oldest) {
            if !std::ptr::eq(l, o) {
                let delta_pct = if o.stats_ns.median > 0.0 {
                    (l.stats_ns.median - o.stats_ns.median) / o.stats_ns.median * 100.0
                } else { 0.0 };
                let color = if delta_pct.abs() < 5.0 { "#888" }
                            else if delta_pct > 0.0 { "#d33" }
                            else { "#393" };
                format!(r#"<aside class="comparison">
<h3>Latest vs oldest</h3>
<p>oldest median: <code>{}</code></p>
<p>latest median: <code>{}</code></p>
<p style="color: {}; font-weight: bold;">delta: {:+.1}%</p>
<p style="color: #999; font-size: 0.85em;">{} runs span</p>
</aside>"#,
                    fmt_ns_short(o.stats_ns.median),
                    fmt_ns_short(l.stats_ns.median),
                    color,
                    delta_pct,
                    runs.len(),
                )
            } else { String::new() }
        } else { String::new() }
    } else { String::new() };

    // Detail table: each run — median + mad + outliers + n.
    let mut rows = Vec::new();
    for (entry, run) in runs.iter().rev() {
        if let Some(b) = run.benches.iter().find(|b| b.raw.name == name) {
            let st = &b.stats_ns;
            rows.push(format!(
                "<tr><td>{}</td><td><code>{}</code></td>\
                 <td>{}</td><td>{}</td><td>{}</td>\
                 <td>{}</td><td>{} / {}</td></tr>",
                unix_to_iso(entry.timestamp_unix),
                html_escape(&entry.git_sha),
                fmt_ns_short(st.median),
                fmt_ns_short(st.mad),
                fmt_ns_short(st.mean),
                st.n,
                st.outliers_low,
                st.outliers_high,
            ));
        }
    }
    let runs_table = rows.join("\n");

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>{name_escaped} — Nova bench</title>
<script src="{echarts_url}"></script>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, sans-serif;
        margin: 0; padding: 1em 2em; color: #222; }}
header {{ border-bottom: 1px solid #ccc; padding-bottom: 1em;
         margin-bottom: 1em; }}
h1 {{ margin: 0; font-size: 1.5em; font-family: SFMono-Regular, monospace; }}
.back {{ display: inline-block; margin-bottom: 1em; color: #06c; }}
.layout {{ display: grid; grid-template-columns: 1fr 280px; gap: 1.5em;
           align-items: start; }}
.charts {{ display: flex; flex-direction: column; gap: 1.5em; }}
#trend-chart, #histogram-chart {{ width: 100%; height: 380px;
                                    border: 1px solid #ddd; border-radius: 4px; }}
.stats-sidebar, .comparison {{ background: #f8f8f8; padding: 1em;
                                border-radius: 4px; font-size: 0.9em; }}
.stats-sidebar dl, .comparison p {{ margin: 0; }}
.stats-sidebar dt {{ font-weight: 600; color: #666;
                       margin-top: 0.4em; font-size: 0.85em; }}
.stats-sidebar dd {{ margin: 0; font-family: SFMono-Regular, monospace; }}
.comparison {{ margin-top: 1em; }}
table {{ width: 100%; border-collapse: collapse; margin-top: 2em; }}
th, td {{ text-align: left; padding: 0.4em 0.8em;
          border-bottom: 1px solid #eee; }}
th {{ background: #f4f4f4; }}
code {{ font-family: SFMono-Regular, Consolas, monospace; }}
section {{ margin-bottom: 2em; }}
</style>
</head>
<body>
<a href="index.html" class="back">← back to overview</a>
<header>
<h1>{name_escaped}</h1>
<div style="color: #666;">{n_points} data points</div>
</header>

<div class="layout">
<div class="charts">
<section>
<h2 style="margin-top:0">Median trend over time</h2>
<div id="trend-chart"></div>
</section>
<section>
<h2>Latest run — sample distribution (Tukey fences marked)</h2>
<div id="histogram-chart"></div>
</section>
</div>
<div>
{stats_sidebar_html}
{comparison_html}
</div>
</div>

<section>
<h2>Run history</h2>
<table>
<thead><tr><th>Timestamp</th><th>SHA</th><th>median</th><th>MAD</th>
<th>mean</th><th>n</th><th>outliers (lo/hi)</th></tr></thead>
<tbody>{runs_table}</tbody>
</table>
</section>

<script>
const ts = {timestamps_json};
const data = {medians_json};
const hist = {histogram_json};
const labels = ts.map(t => new Date(t*1000).toISOString().slice(5,16).replace('T',' '));

// Plan 57.E.1: trend chart (existing line chart).
const trend = echarts.init(document.getElementById('trend-chart'));
trend.setOption({{
    tooltip: {{ trigger: 'axis',
                formatter: function(p) {{
                  return labels[p[0].dataIndex] + '<br>median: ' +
                         (p[0].value ? p[0].value.toFixed(1) + ' ns' : '—');
                }} }},
    grid: {{ left: '10%', right: '4%', bottom: '20%', top: 20 }},
    xAxis: {{ type: 'category', data: labels,
              axisLabel: {{ rotate: -35 }} }},
    yAxis: {{ type: 'value', name: 'median ns' }},
    dataZoom: [
        {{ type: 'inside', start: 0, end: 100 }},
        {{ type: 'slider', start: 0, end: 100, bottom: 30 }}
    ],
    series: [{{
        name: 'median',
        type: 'line',
        data: data,
        connectNulls: true,
        smooth: false,
        symbolSize: 8,
        markLine: {{ data: [{{ type: 'average', name: 'avg' }}] }}
    }}]
}});

// Plan 57.E.1: histogram of latest run's raw samples + Tukey fences.
if (hist.bins && hist.bins.length > 0) {{
    const hchart = echarts.init(document.getElementById('histogram-chart'));
    const markLines = [];
    if (hist.median !== undefined)
        markLines.push({{ xAxis: hist.median, lineStyle: {{ color: '#06c', width: 2 }},
                          label: {{ formatter: 'median', position: 'middle' }} }});
    if (hist.mean !== undefined)
        markLines.push({{ xAxis: hist.mean, lineStyle: {{ color: '#90a' }},
                          label: {{ formatter: 'mean', position: 'middle' }} }});
    if (hist.lo_fence !== undefined && hist.lo_fence > 0)
        markLines.push({{ xAxis: hist.lo_fence, lineStyle: {{ color: '#c66', type: 'dashed' }},
                          label: {{ formatter: 'lo fence' }} }});
    if (hist.hi_fence !== undefined)
        markLines.push({{ xAxis: hist.hi_fence, lineStyle: {{ color: '#c66', type: 'dashed' }},
                          label: {{ formatter: 'hi fence' }} }});
    hchart.setOption({{
        tooltip: {{ trigger: 'axis',
                    formatter: p => 'ns: ' + p[0].axisValue.toFixed(1) +
                                    '<br>count: ' + p[0].value }},
        grid: {{ left: '10%', right: '4%', bottom: '15%', top: 20 }},
        xAxis: {{ type: 'value', name: 'ns', nameLocation: 'middle', nameGap: 30 }},
        yAxis: {{ type: 'value', name: 'count' }},
        series: [{{
            type: 'bar',
            data: hist.bins.map((b, i) => [b, hist.counts[i]]),
            itemStyle: {{ color: '#5b8def' }},
            markLine: {{ silent: false, data: markLines, symbol: 'none' }}
        }}]
    }});
    window.addEventListener('resize', () => hchart.resize());
}}

window.addEventListener('resize', () => trend.resize());
</script>
</body>
</html>"#,
        name_escaped = html_escape(name),
        echarts_url = html_escape(echarts_url),
        n_points = points.len(),
        runs_table = runs_table,
        timestamps_json = timestamps_json,
        medians_json = medians_json,
        histogram_json = histogram_json,
        stats_sidebar_html = stats_sidebar_html,
        comparison_html = comparison_html,
    )
}

/// YYYY-MM-DD HH:MM via unix_to_ymdhms helper в repro.rs (inline copy).
fn unix_to_iso(secs: u64) -> String {
    let (y, mo, d, h, mi, _s) = unix_to_ymdhms(secs);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, mi)
}

fn unix_to_ymdhms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let z = (secs / 86400) as i64;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let z_adj = z + 719468;
    let era = if z_adj >= 0 { z_adj } else { z_adj - 146096 } / 146097;
    let doe = (z_adj - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m_calendar = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m_calendar <= 2 { y + 1 } else { y } as i32;
    (year, m_calendar as u32, d as u32, h as u32, m as u32, s as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_sanitization() {
        assert_eq!(filename_for_bench("hashmap_insert"), "hashmap_insert");
        assert_eq!(filename_for_bench("hashmap insert n=1000"),
            "hashmap_insert_n_1000");
        assert_eq!(filename_for_bench("a/b/c"), "a_b_c");
    }

    #[test]
    fn html_escape_basic() {
        assert_eq!(html_escape("a<b>c"), "a&lt;b&gt;c");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("plain"), "plain");
    }

    #[test]
    fn fmt_ns_short_scales() {
        assert_eq!(fmt_ns_short(500.0), "500 ns");
        assert_eq!(fmt_ns_short(5000.0), "5.0 µs");
        assert_eq!(fmt_ns_short(5_000_000.0), "5.0 ms");
    }

    #[test]
    fn unix_iso_format() {
        assert_eq!(unix_to_iso(0), "1970-01-01 00:00");
        assert_eq!(unix_to_iso(3723), "1970-01-01 01:02");
    }
}
