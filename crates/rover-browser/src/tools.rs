//! Browser Page Tools (`tools`):
//! - Reader Mode (`extract_reader_article`, `render_reader_html` with Dark/Sepia/Light themes)
//! - Zero-dependency Markdown-to-HTML (`markdown_to_html`) & local File Viewer (`render_fileview_html`)
//! - Download collision resolver (`unique_download_path`) & YouTube Downloader (`YtdlJobSpec`)
//! - Google Chrome / Chromium read-only SQLite History importer (`read_chrome_history`)

use crate::homepage::html_escape;
use once_cell::sync::Lazy;
use regex::Regex;
use rover_core::HistoryEntry;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReaderArticle {
    pub title: String,
    pub paragraphs: Vec<String>,
}

static ARTICLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<article\b[^>]*>(.*?)</article>").unwrap());
static MAIN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<main\b[^>]*>(.*?)</main>").unwrap());
static H1_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<h1\b[^>]*>(.*?)</h1>").unwrap());
static TITLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<title\b[^>]*>(.*?)</title>").unwrap());
static PARA_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<p\b[^>]*>(.*?)</p>").unwrap());
static TAG_STRIP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").unwrap());
static NOISE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)<(nav|footer|aside|header|script|style|noscript)\b[^>]*>.*?</(nav|footer|aside|header|script|style|noscript)>")
        .unwrap()
});

fn strip_tags(html: &str) -> String {
    TAG_STRIP_RE
        .replace_all(html, "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extracts the primary article body and title from raw HTML, dropping nav, sidebar, and footer noise.
pub fn extract_reader_article(raw_html: &str) -> Option<ReaderArticle> {
    let cleaned = NOISE_RE.replace_all(raw_html, "");
    let title = H1_RE
        .captures(&cleaned)
        .or_else(|| TITLE_RE.captures(raw_html))
        .map(|c| strip_tags(&c[1]))
        .unwrap_or_else(|| "Article".into());

    let container = ARTICLE_RE
        .captures(&cleaned)
        .or_else(|| MAIN_RE.captures(&cleaned))
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| cleaned.into_owned());

    let mut paragraphs = Vec::new();
    let mut total_chars = 0usize;
    for caps in PARA_RE.captures_iter(&container) {
        let p = strip_tags(&caps[1]);
        if p.chars().count() >= 35 {
            total_chars += p.chars().count();
            paragraphs.push(p);
        }
    }

    if paragraphs.len() < 2 || total_chars < 220 {
        return None;
    }
    Some(ReaderArticle { title, paragraphs })
}

pub fn render_reader_html(
    article: &ReaderArticle,
    source_url: &str,
    theme: &str,
    font_size_px: u8,
) -> String {
    let (bg, fg, border) = match theme {
        "sepia" => ("rgb(244, 236, 216)", "#47090C", "#d8c8a8"),
        "light" => ("#FFF8F2", "#47090C", "#FDB78E"),
        _ => ("#160708", "#FEF0E4", "#531E21"),
    };
    let paras_html = article
        .paragraphs
        .iter()
        .map(|p| format!("<p>{}</p>", html_escape(p)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="rover-source" content="{src}">
<title>{title} — Rover Reader</title>
<style>
  body {{ background:{bg}; color:{fg}; font-family:Georgia,serif; font-size:{size}px; line-height:1.72; margin:0; padding:0; }}
  .toolbar {{ position:sticky; top:0; background:{bg}; border-bottom:1px solid {border}; padding:10px 24px; display:flex; gap:12px; align-items:center; font-family:system-ui,sans-serif; font-size:13px; }}
  .toolbar a {{ color:{fg}; text-decoration:none; padding:5px 10px; border:1px solid {border}; border-radius:6px; }}
  main {{ max-width:680px; margin:36px auto; padding:0 24px 64px; }}
  h1 {{ font-family:system-ui,sans-serif; font-size:1.7em; line-height:1.25; margin-bottom:24px; }}
  p {{ margin:0 0 1.25em; }}
</style>
</head>
<body>
  <div class="toolbar">
    <a href="?rover=reader-exit">✕ Exit Reader</a>
    <a href="?rover=reader-smaller">A−</a>
    <a href="?rover=reader-bigger">A+</a>
    <a href="?rover=reader-theme-dark">Dark</a>
    <a href="?rover=reader-theme-sepia">Sepia</a>
    <a href="?rover=reader-theme-light">Light</a>
    <a href="?rover=reader-summarize">✦ Summarize with Rover AI</a>
  </div>
  <main>
    <h1>{title}</h1>
    {paras_html}
  </main>
</body>
</html>"#,
        src = html_escape(source_url),
        title = html_escape(&article.title),
        bg = bg,
        fg = fg,
        border = border,
        size = font_size_px.clamp(13, 32),
        paras_html = paras_html
    )
}

/// Zero-dependency Markdown-to-HTML converter supporting headings, fenced code blocks,
/// lists, blockquotes, bold, italics, inline code, and links.
pub fn markdown_to_html(md: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    let mut in_list = false;

    for raw_line in md.replace('\r', "").lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            if in_code {
                out.push_str("</code></pre>\n");
                in_code = false;
            } else {
                out.push_str("<pre><code>");
                in_code = true;
            }
            continue;
        }
        if in_code {
            out.push_str(&html_escape(raw_line));
            out.push('\n');
            continue;
        }
        if trimmed.is_empty() {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            out.push_str(&format!("<h3>{}</h3>\n", format_inline_md(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            out.push_str(&format!("<h2>{}</h2>\n", format_inline_md(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            out.push_str(&format!("<h1>{}</h1>\n", format_inline_md(rest)));
        } else if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            if !in_list {
                out.push_str("<ul>\n");
                in_list = true;
            }
            out.push_str(&format!("<li>{}</li>\n", format_inline_md(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            out.push_str(&format!(
                "<blockquote>{}</blockquote>\n",
                format_inline_md(rest)
            ));
        } else {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            out.push_str(&format!("<p>{}</p>\n", format_inline_md(trimmed)));
        }
    }
    if in_list {
        out.push_str("</ul>\n");
    }
    if in_code {
        out.push_str("</code></pre>\n");
    }
    out
}

fn format_inline_md(text: &str) -> String {
    static CODE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`]+)`").unwrap());
    static BOLD_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*([^*]+)\*\*").unwrap());
    static ITALIC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*([^*]+)\*").unwrap());
    static LINK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\[([^\]]+)\]\((https?://[^)\s]+)\)").unwrap());

    let escaped = html_escape(text);
    let s1 = CODE_RE.replace_all(&escaped, "<code>$1</code>");
    let s2 = BOLD_RE.replace_all(&s1, "<strong>$1</strong>");
    let s3 = ITALIC_RE.replace_all(&s2, "<em>$1</em>");
    LINK_RE
        .replace_all(&s3, r#"<a href="$2">$1</a>"#)
        .into_owned()
}

pub fn render_fileview_html(
    path: &Path,
    content: &str,
    show_source: bool,
    theme: &str,
) -> String {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let is_md = matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "md" | "markdown" | "mdown"
    );
    let (bg, fg, bar_bg) = match theme {
        "light" => ("#FFF8F2", "#47090C", "#FED8B1"),
        "sepia" => ("#f4ecd8", "#47090C", "#eadcc0"),
        _ => ("#160708", "#FEF0E4", "#220B0D"),
    };
    let body_html = if is_md && !show_source {
        markdown_to_html(content)
    } else {
        format!("<pre><code>{}</code></pre>", html_escape(content))
    };
    let toggle_link = if is_md {
        if show_source {
            r#"<a href="?rover=file-rendered">Show formatted</a>"#
        } else {
            r#"<a href="?rover=file-source">Show source</a>"#
        }
    } else {
        ""
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{name} — Rover File View</title>
<style>
  body {{ background:{bg}; color:{fg}; font-family:system-ui,-apple-system,sans-serif; margin:0; line-height:1.65; }}
  .bar {{ position:sticky; top:0; background:{bar_bg}; padding:10px 20px; display:flex; gap:12px; align-items:center; font-size:13px; border-bottom:1px solid rgba(148,163,184,.2); }}
  .bar a {{ color:{fg}; text-decoration:none; padding:4px 10px; border-radius:6px; border:1px solid rgba(148,163,184,.3); }}
  main {{ max-width:860px; margin:28px auto; padding:0 24px 64px; }}
  pre {{ background:rgba(0,0,0,.18); padding:16px; border-radius:10px; overflow-x:auto; }}
</style>
</head>
<body>
  <div class="bar">
    <strong>{name}</strong>
    {toggle_link}
    <a href="?rover=file-reload">Reload</a>
    <a href="?rover=file-theme-dark">Dark</a>
    <a href="?rover=file-theme-light">Light</a>
    <a href="?rover=file-theme-sepia">Sepia</a>
  </div>
  <main>{body_html}</main>
</body>
</html>"#,
        name = html_escape(file_name),
        bg = bg,
        fg = fg,
        bar_bg = bar_bg,
        toggle_link = toggle_link,
        body_html = body_html
    )
}

/// Resolves a collision-free download target path (`file (1).ext`, `file (2).ext`, ...).
pub fn unique_download_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = candidate
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for i in 1..10_000 {
        let p = dir.join(format!("{stem} ({i}){ext}"));
        if !p.exists() {
            return p;
        }
    }
    candidate
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtdlJobSpec {
    pub url: String,
    pub audio_only: bool,
    pub max_height: Option<u32>,
    pub allow_playlist: bool,
    pub output_dir: PathBuf,
}

impl YtdlJobSpec {
    pub fn build_cli_args(&self, has_ffmpeg: bool) -> Vec<String> {
        let mut args = Vec::new();
        if !self.allow_playlist {
            args.push("--no-playlist".into());
        }
        if self.audio_only {
            args.push("-f".into());
            args.push("bestaudio/best".into());
            if has_ffmpeg {
                args.push("-x".into());
                args.push("--audio-format".into());
                args.push("mp3".into());
                args.push("--add-metadata".into());
            }
        } else {
            let fmt = match (self.max_height, has_ffmpeg) {
                (Some(h), true) => {
                    format!("bestvideo[height<={h}][ext=mp4]+bestaudio[ext=m4a]/best[height<={h}][ext=mp4]/best")
                }
                (Some(h), false) => format!("best[height<={h}][ext=mp4]/best"),
                (None, true) => "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best".into(),
                (None, false) => "best[ext=mp4]/best".into(),
            };
            args.push("-f".into());
            args.push(fmt);
        }
        args.push("-o".into());
        args.push(
            self.output_dir
                .join("%(title)s.%(ext)s")
                .to_string_lossy()
                .to_string(),
        );
        args.push(self.url.clone());
        args
    }
}

/// Reads Google Chrome / Chromium SQLite `History` database read-only via a temporary copy.
pub fn read_chrome_history(chrome_history_db: &Path, limit: usize) -> Vec<HistoryEntry> {
    if !chrome_history_db.exists() {
        return Vec::new();
    }
    let tmp_path = std::env::temp_dir().join(format!(
        "rover_chrome_history_{}.db",
        std::process::id()
    ));
    if fs::copy(chrome_history_db, &tmp_path).is_err() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Ok(conn) = Connection::open(&tmp_path) {
        // Chrome stores timestamps in microseconds since 1601-01-01 UTC
        if let Ok(mut stmt) = conn.prepare(
            "SELECT url, title, visit_count, (last_visit_time / 1000000.0 - 11644473600.0) as unix_ts
             FROM urls ORDER BY last_visit_time DESC LIMIT ?1",
        ) {
            if let Ok(rows) = stmt.query_map([limit as i64], |r| {
                Ok(HistoryEntry {
                    url: r.get(0)?,
                    title: r.get(1)?,
                    visits: r.get(2)?,
                    last: r.get(3)?,
                })
            }) {
                for r in rows.flatten() {
                    out.push(r);
                }
            };
        };
    }
    let _ = fs::remove_file(&tmp_path);
    out
}
