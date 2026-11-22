use super::{Kind, LIMIT, PreviewResult, STANDARD, escape};
use base64::Engine;
use roxmltree::{Document, Node};
use std::{collections::HashMap, fs::File, io::Read, path::Path};

struct Book {
    zip: zip::ZipArchive<File>,
    remaining: usize,
}

impl Book {
    fn read(&mut self, name: &str) -> PreviewResult<Vec<u8>> {
        let entry = self.zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
        let limit = self.remaining.min(LIMIT);
        if entry.size() > limit as u64 {
            return Err("文档内容超过预览上限".into());
        }
        let mut data = Vec::new();
        entry
            .take(limit as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > limit {
            return Err("文档内容超过预览上限".into());
        }
        self.remaining -= data.len();
        Ok(data)
    }

    fn xml(&mut self, name: &str) -> PreviewResult<String> {
        let data = self.read(name)?;
        Ok(super::xml_text(&data).into_owned())
    }

    fn image(&mut self, name: &str) -> String {
        let mime = match name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            _ => return String::new(),
        };
        self.read(name)
            .map(|bytes| {
                format!(
                    "<img alt='' src='data:{mime};base64,{}'>",
                    STANDARD.encode(bytes)
                )
            })
            .unwrap_or_default()
    }

    fn relationships(&mut self, part: &str) -> PreviewResult<HashMap<String, String>> {
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        let name = format!("{dir}/_rels/{file}.rels")
            .trim_start_matches('/')
            .to_string();
        if self.zip.index_for_name(&name).is_none() {
            return Ok(HashMap::new());
        }
        let xml = self.xml(&name)?;
        let doc = parse(&xml)?;
        Ok(doc
            .descendants()
            .filter(|n| tag(*n) == "Relationship" && n.attribute("TargetMode") != Some("External"))
            .filter_map(|n| {
                Some((
                    n.attribute("Id")?.into(),
                    resolve(part, n.attribute("Target")?)?,
                ))
            })
            .collect())
    }
}

pub(super) fn parse(xml: &str) -> PreviewResult<Document<'_>> {
    Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 500_000,
        },
    )
    .map_err(|e| format!("XML 解析失败：{e}"))
}

fn tag<'a>(n: Node<'a, 'a>) -> &'a str {
    n.tag_name().name()
}
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
fn rel_id<'a>(n: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    n.attribute((REL, name)).or_else(|| {
        n.attributes()
            .find(|a| {
                a.name() == name
                    && a.namespace()
                        .is_some_and(|ns| ns.ends_with("/relationships"))
            })
            .map(|a| a.value())
    })
}

fn resolve(part: &str, href: &str) -> Option<String> {
    let href = href.split(['?', '#']).next()?;
    if href.is_empty() || href.contains(':') || href.starts_with("//") || href.contains('\\') {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(href)
        .decode_utf8()
        .ok()?;
    let mut parts = if href.starts_with('/') {
        vec![]
    } else {
        part.split('/').collect::<Vec<_>>()
    };
    if !href.starts_with('/') {
        parts.pop();
    }
    for item in decoded.split('/') {
        match item {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            value => parts.push(value),
        }
    }
    Some(parts.join("/"))
}

pub(super) fn render(
    path: &Path,
    kind: Kind,
    cancelled: &impl Fn() -> bool,
) -> PreviewResult<Vec<String>> {
    let zip = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("无法打开文档：{e}"))?;
    let mut book = Book {
        zip,
        remaining: 64 * 1024 * 1024,
    };
    if kind == Kind::Epub {
        return epub(&mut book, cancelled);
    }
    let root = book.xml("_rels/.rels")?;
    let target = parse(&root)?
        .descendants()
        .find(|n| {
            n.attribute("Type")
                .is_some_and(|t| t.ends_with("/officeDocument"))
                && n.attribute("TargetMode") != Some("External")
        })
        .and_then(|n| n.attribute("Target"))
        .and_then(|t| resolve("", t))
        .ok_or("文档缺少主部件")?;
    let xml = book.xml(&target)?;
    let doc = parse(&xml)?;
    let rels = book.relationships(&target)?;
    if kind == Kind::Word {
        let mut out = String::new();
        let body = doc
            .descendants()
            .find(|n| tag(*n) == "body")
            .ok_or("缺少 Word 正文")?;
        word(body, &mut book, &rels, &mut out, 0)?;
        return Ok(vec![out]);
    }
    let size = doc.descendants().find(|n| tag(*n) == "sldSz");
    let width = dimension(size, "cx", 9144000.);
    let height = dimension(size, "cy", 6858000.);
    let mut pages = Vec::new();
    for slide in doc.descendants().filter(|n| tag(*n) == "sldId") {
        if cancelled() {
            return Err("预览已取消".into());
        }
        let part = rel_id(slide, "id")
            .and_then(|id| rels.get(id))
            .ok_or("找不到幻灯片")?;
        let xml = book.xml(part)?;
        let links = book.relationships(part)?;
        let slide = parse(&xml)?;
        let mut out = format!(
            "<section style='position:relative;width:100%;aspect-ratio:{width}/{height};overflow:hidden;background:white;color:#222'>"
        );
        for shape in slide
            .descendants()
            .filter(|n| matches!(tag(*n), "sp" | "pic" | "graphicFrame"))
        {
            let xfrm = shape.children().find(|n| tag(*n) == "xfrm").or_else(|| {
                shape
                    .children()
                    .find(|n| tag(*n) == "spPr")
                    .and_then(|n| n.children().find(|n| tag(*n) == "xfrm"))
            });
            let mut style = String::new();
            if let Some(x) = xfrm {
                let off = x.children().find(|n| tag(*n) == "off");
                let ext = x.children().find(|n| tag(*n) == "ext");
                style = format!(
                    "position:absolute;left:{}%;top:{}%;width:{}%;height:{}%;",
                    100. * dimension(off, "x", 0.) / width,
                    100. * dimension(off, "y", 0.) / height,
                    100. * dimension(ext, "cx", width) / width,
                    100. * dimension(ext, "cy", height) / height
                );
            }
            out.push_str(&format!(
                "<div style='{style}overflow:auto;padding:0 .4em'>"
            ));
            word(shape, &mut book, &links, &mut out, 0)?;
            out.push_str("</div>");
        }
        out.push_str("</section>");
        pages.push(out);
    }
    if pages.is_empty() {
        return Err("演示文稿没有幻灯片".into());
    }
    Ok(pages)
}

fn dimension(node: Option<Node>, key: &str, default: f64) -> f64 {
    node.and_then(|n| n.attribute(key))
        .and_then(|s| s.parse::<i64>().ok())
        .map(|i| i as f64)
        .unwrap_or(default)
}

fn word(
    node: Node,
    book: &mut Book,
    rels: &HashMap<String, String>,
    out: &mut String,
    depth: usize,
) -> PreviewResult<()> {
    if depth > 100 {
        return Err("文档嵌套过深".into());
    }
    let name = tag(node);
    let wrapper = match name {
        "p" => {
            let style = node
                .children()
                .find(|n| tag(*n) == "pPr")
                .and_then(|n| n.children().find(|n| tag(*n) == "pStyle"))
                .and_then(|n| n.attributes().find(|a| a.name() == "val"))
                .map(|a| a.value());
            match style {
                Some("Title" | "Heading1") => "h1",
                Some("Heading2") => "h2",
                Some("Heading3") => "h3",
                _ => "p",
            }
        }
        "r" => {
            let props = node.children().find(|n| tag(*n) == "rPr");
            let on = |key: &str| {
                props.is_some_and(|p| {
                    p.attribute(key).is_some_and(|v| v == "1" || v == "true")
                        || p.children().any(|n| {
                            tag(n) == key
                                && !n.attributes().any(|a| {
                                    a.name() == "val" && matches!(a.value(), "0" | "false" | "none")
                                })
                        })
                })
            };
            let mut style = String::new();
            if on("b") {
                style.push_str("font-weight:bold;");
            }
            if on("i") {
                style.push_str("font-style:italic;");
            }
            if on("u") {
                style.push_str("text-decoration:underline;");
            }
            out.push_str(&format!("<span style='{style}'>"));
            for c in node.children().filter(|n| n.is_element()) {
                word(c, book, rels, out, depth + 1)?;
            }
            out.push_str("</span>");
            return Ok(());
        }
        "tbl" => "table",
        "tr" => "tr",
        "tc" => "td",
        "t" => {
            out.push_str(&escape(node.text().unwrap_or("")));
            return Ok(());
        }
        "br" | "cr" => {
            out.push_str("<br>");
            return Ok(());
        }
        "tab" => {
            out.push_str("&#8195;");
            return Ok(());
        }
        "blip" => {
            if let Some(name) = rel_id(node, "embed").and_then(|id| rels.get(id)) {
                out.push_str(&book.image(name));
            }
            return Ok(());
        }
        "del" | "pPr" | "rPr" | "tblPr" | "txBodyPr" => return Ok(()),
        _ => "",
    };
    if !wrapper.is_empty() {
        out.push_str(&format!("<{wrapper}>"));
    }
    for c in node.children().filter(|n| n.is_element()) {
        word(c, book, rels, out, depth + 1)?;
    }
    if !wrapper.is_empty() {
        out.push_str(&format!("</{wrapper}>"));
    }
    Ok(())
}

fn epub(book: &mut Book, cancelled: &impl Fn() -> bool) -> PreviewResult<Vec<String>> {
    let container = book.xml("META-INF/container.xml")?;
    let container_doc = parse(&container)?;
    let root = container_doc
        .descendants()
        .find_map(|n| n.attribute("full-path"))
        .ok_or("缺少 EPUB 书目")?;
    let xml = book.xml(root)?;
    let doc = parse(&xml)?;
    let items = doc
        .descendants()
        .filter(|n| tag(*n) == "item")
        .filter_map(|n| Some((n.attribute("id")?, resolve(root, n.attribute("href")?)?)))
        .collect::<HashMap<_, _>>();
    let mut pages = Vec::new();
    for item in doc.descendants().filter(|n| tag(*n) == "itemref") {
        if cancelled() {
            return Err("预览已取消".into());
        }
        let path = item
            .attribute("idref")
            .and_then(|id| items.get(id))
            .ok_or("缺少 EPUB 章节")?;
        let xml = book.xml(path)?;
        let chapter = parse(&xml)?;
        let body = chapter
            .descendants()
            .find(|n| tag(*n) == "body")
            .ok_or("章节缺少正文")?;
        let mut out = String::new();
        chapter_html(body, book, path, &mut out, 0)?;
        pages.push(out);
    }
    if pages.is_empty() {
        return Err("电子书没有可读章节".into());
    }
    Ok(pages)
}

fn chapter_html(
    node: Node,
    book: &mut Book,
    part: &str,
    out: &mut String,
    depth: usize,
) -> PreviewResult<()> {
    if depth > 100 {
        return Err("章节嵌套过深".into());
    }
    if node.is_text() {
        out.push_str(&escape(node.text().unwrap_or("")));
        return Ok(());
    }
    let name = tag(node);
    match name {
        "script" | "style" | "iframe" | "object" => return Ok(()),
        "br" | "hr" => {
            out.push_str(&format!("<{name}>"));
            return Ok(());
        }
        "img" | "image" => {
            if let Some(path) = node
                .attributes()
                .find(|a| matches!(a.name(), "href" | "src"))
                .and_then(|a| resolve(part, a.value()))
            {
                out.push_str(&book.image(&path));
            }
            return Ok(());
        }
        _ => (),
    }
    let allowed = matches!(
        name,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "table"
            | "tr"
            | "th"
            | "td"
            | "ul"
            | "ol"
            | "li"
            | "strong"
            | "b"
            | "i"
            | "em"
            | "pre"
            | "code"
            | "blockquote"
            | "sup"
            | "sub"
    );
    if allowed {
        out.push_str(&format!("<{name}>"));
    }
    for child in node.children() {
        chapter_html(child, book, part, out, depth + 1)?;
    }
    if allowed {
        out.push_str(&format!("</{name}>"));
    }
    Ok(())
}
