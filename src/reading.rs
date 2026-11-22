use crate::core::{Document, Kind};
use base64::Engine;
use calamine::Reader;
use serde::Serialize;
use std::{
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub struct PreviewFile(pub PathBuf, Vec<PathBuf>);
impl PreviewFile {
    pub fn resources(&self) -> &[PathBuf] {
        &self.1
    }
}
impl Drop for PreviewFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        for path in &self.1 {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct Sheet {
    name: String,
    rows: Vec<Vec<String>>,
    total: usize,
}

#[derive(Serialize)]
struct SheetPreview<'a> {
    name: &'a str,
    rows: &'a [Vec<String>],
    total: usize,
    count: usize,
    columns: usize,
    file: String,
}

#[derive(Serialize)]
struct Payload<'a> {
    kind: Kind,
    language: &'a str,
    text: &'a str,
    source: Option<&'a str>,
    rendered: &'a str,
    rendered_pages: &'a [String],
    archive_entries: &'a [crate::archive::Entry],
    sheets: Vec<SheetPreview<'a>>,
    notice: &'a str,
}

fn local_image(document: &Path, source: &str) -> Option<String> {
    let base = std::fs::canonicalize(document.parent()?).ok()?;
    let target = std::fs::canonicalize(base.join(source)).ok()?;
    if !target.starts_with(&base) || target.metadata().ok()?.len() > 8 * 1024 * 1024 {
        return None;
    }
    let mime = match target.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(target).ok()?)
    ))
}

pub fn prepare(
    path: &Path,
    doc: &Document,
    cancelled: &impl Fn() -> bool,
) -> Result<PreviewFile, String> {
    let mut sheets = Vec::new();
    let mut notice = doc.notice.clone();
    if doc.kind == Kind::Csv {
        let delimiter = if path.extension().is_some_and(|e| e == "tsv") {
            b'\t'
        } else {
            b','
        };
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .delimiter(delimiter)
            .from_reader(doc.source.as_bytes());
        let mut rows = Vec::new();
        for record in reader.records().take(250_001) {
            if cancelled() {
                return Err("预览已取消".into());
            }
            rows.push(
                record
                    .map_err(|e| format!("CSV 解析失败：{e}"))?
                    .iter()
                    .take(256)
                    .map(str::to_owned)
                    .collect(),
            );
        }
        let total = rows.len();
        rows.truncate(250_000);
        if total > rows.len() {
            notice.push_str(" 表格仅预览前 250,000 行。");
        }
        sheets.push(Sheet {
            name: "CSV".into(),
            rows,
            total,
        });
    }
    if doc.kind == Kind::Spreadsheet {
        let mut workbook =
            calamine::open_workbook_auto(path).map_err(|e| format!("表格无法打开：{e}"))?;
        for name in workbook.sheet_names().to_vec() {
            if cancelled() {
                return Err("预览已取消".into());
            }
            let range = workbook.worksheet_range(&name).map_err(|e| e.to_string())?;
            let total = range.height();
            let row_limit = (2_000_000 / range.width().clamp(1, 256)).min(250_000);
            if total > row_limit || range.width() > 256 {
                notice.push_str(&format!(" 大工作表仅预览前 {row_limit} 行 / 256 列。"));
            }
            let rows = range
                .rows()
                .take(row_limit)
                .map(|row| row.iter().take(256).map(ToString::to_string).collect())
                .collect();
            sheets.push(Sheet { name, rows, total });
        }
    }
    let mut archive_entries = Vec::new();
    if doc.kind == Kind::Archive {
        let (entries, truncated) = crate::archive::entries(path, cancelled)?;
        archive_entries = entries;
        if truncated {
            notice.push_str(" 目录过大，仅显示部分条目。");
        }
    }
    let mut rendered_pages = if doc.kind.rich_file() && doc.kind != Kind::Archive {
        crate::formats::pages(path, doc.kind, cancelled)?
    } else {
        Vec::new()
    };
    let rendered = match doc.kind {
        Kind::Markdown => {
            let parser = pulldown_cmark::Parser::new_ext(
                &doc.text,
                pulldown_cmark::Options::ENABLE_TABLES
                    | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
                    | pulldown_cmark::Options::ENABLE_TASKLISTS,
            );
            let mut html = String::new();
            let parser = parser.map(|event| match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                }) => {
                    let dest_url = local_image(path, &dest_url)
                        .map(Into::into)
                        .unwrap_or(dest_url);
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        id,
                    })
                }
                event => event,
            });
            if doc.text.len() > 512 * 1024 {
                let mut events = Vec::new();
                let mut depth = 0usize;
                let mut blocks = 0;
                for event in parser {
                    if cancelled() {
                        return Err("预览已取消".into());
                    }
                    match &event {
                        pulldown_cmark::Event::Start(_) => depth += 1,
                        pulldown_cmark::Event::End(_) => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                blocks += 1;
                            }
                        }
                        _ => {}
                    }
                    events.push(event);
                    if depth == 0 && blocks >= 160 {
                        let mut page = String::new();
                        pulldown_cmark::html::push_html(&mut page, events.drain(..));
                        rendered_pages.push(page);
                        blocks = 0;
                    }
                }
                if !events.is_empty() {
                    let mut page = String::new();
                    pulldown_cmark::html::push_html(&mut page, events.into_iter());
                    rendered_pages.push(page);
                }
            } else {
                pulldown_cmark::html::push_html(&mut html, parser);
            }
            html
        }
        // HTML reuses the original text, rather than embedding a second copy.
        _ => String::new(),
    };
    if cancelled() {
        return Err("预览已取消".into());
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join("QuickPeek-previews");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let stem = format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut preview = PreviewFile(directory.join(format!("{stem}.html")), Vec::new());
    let mut metadata = Vec::new();
    for (index, sheet) in sheets.iter().enumerate() {
        if cancelled() {
            return Err("预览已取消".into());
        }
        let filename = format!("{stem}.sheet{index}.js");
        let target = directory.join(&filename);
        preview.1.push(target.clone());
        let mut writer = BufWriter::new(std::fs::File::create(&target).map_err(|e| e.to_string())?);
        write!(writer, "window.loadSheet({index},").map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut writer, &sheet.rows).map_err(|e| e.to_string())?;
        writer.write_all(b");").map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        metadata.push(SheetPreview {
            name: &sheet.name,
            rows: &sheet.rows[..sheet.rows.len().min(101)],
            total: sheet.total,
            count: sheet.rows.len(),
            columns: sheet.rows.iter().map(Vec::len).max().unwrap_or(0),
            file: filename,
        });
    }
    let language = match doc.kind {
        Kind::Json => "json",
        Kind::Html => "xml",
        Kind::Markdown => "markdown",
        Kind::Code => path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("plaintext"),
        _ => "plaintext",
    };
    let payload = Payload {
        kind: doc.kind,
        language,
        text: &doc.text,
        source: (doc.source != doc.text).then_some(doc.source.as_str()),
        rendered: &rendered,
        rendered_pages: &rendered_pages,
        archive_entries: &archive_entries,
        sheets: metadata,
        notice: &notice,
    };
    let target = directory.join(format!("{stem}.data.js"));
    preview.1.push(target.clone());
    let mut writer = BufWriter::new(std::fs::File::create(target).map_err(|e| e.to_string())?);
    writer
        .write_all(b"const data = ")
        .map_err(|e| e.to_string())?;
    // External JS is not parsed as HTML: literal </script> in document strings
    // cannot escape the payload. Serialize borrowed fields without Value clones.
    serde_json::to_writer(&mut writer, &payload).map_err(|e| e.to_string())?;
    writer.write_all(b";").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    let mut highlight_scripts = String::new();
    for (suffix, bytes) in [
        ("js", include_bytes!("../assets/reader.js").as_slice()),
        ("css", include_bytes!("../assets/reader.css").as_slice()),
        (
            "highlight.js",
            include_bytes!("../assets/vendor/highlight.min.js").as_slice(),
        ),
        (
            "powershell.js",
            include_bytes!("../assets/vendor/powershell.min.js").as_slice(),
        ),
    ] {
        if suffix == "highlight.js" || suffix == "powershell.js" {
            if language == "plaintext"
                || doc.kind == Kind::Markdown
                || (suffix == "powershell.js"
                    && !language.eq_ignore_ascii_case("ps1")
                    && !language.eq_ignore_ascii_case("psm1")
                    && !language.eq_ignore_ascii_case("psd1"))
            {
                continue;
            }
            highlight_scripts
                .push_str(&format!(r#"<script src="{stem}.{suffix}" defer></script>"#));
        }
        let target = directory.join(format!("{stem}.{suffix}"));
        preview.1.push(target.clone());
        std::fs::write(target, bytes).map_err(|e| e.to_string())?;
    }
    let html = include_str!("../assets/reader.html")
        .replace("__STEM__", &stem)
        .replace("__HIGHLIGHT_SCRIPTS__", &highlight_scripts);
    std::fs::write(&preview.0, html).map_err(|e| e.to_string())?;
    Ok(preview)
}
