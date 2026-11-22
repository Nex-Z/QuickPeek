use crate::core::Kind;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{fs::File, io::Read, path::Path};

mod package;

const LIMIT: usize = 16 * 1024 * 1024;
type PreviewResult<T> = Result<T, String>;

fn xml_text(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    let (encoding, offset) =
        encoding_rs::Encoding::for_bom(bytes).unwrap_or((encoding_rs::UTF_8, 0));
    encoding.decode(&bytes[offset..]).0
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn bytes(path: &Path) -> PreviewResult<Vec<u8>> {
    let mut out = Vec::new();
    File::open(path)
        .and_then(|f| f.take((LIMIT + 1) as u64).read_to_end(&mut out))
        .map_err(|e| e.to_string())?;
    if out.len() > LIMIT {
        return Err("文件超过 16 MB 的文档预览上限".into());
    }
    Ok(out)
}

pub fn pages(path: &Path, kind: Kind, cancelled: &impl Fn() -> bool) -> PreviewResult<Vec<String>> {
    let html = match kind {
        Kind::Word | Kind::PowerPoint | Kind::Epub => {
            return package::render(path, kind, cancelled);
        }

        Kind::Email => email(&bytes(path)?)?,
        Kind::Rtf => rtf_to_html::rtf_to_html(&bytes(path)?).map_err(|e| e.to_string())?,
        Kind::Svg => {
            let data = bytes(path)?;
            let svg = xml_text(&data);
            if !package::parse(&svg).is_ok_and(|doc| doc.root_element().tag_name().name() == "svg")
            {
                return Err("SVG 文档无法解析".into());
            }
            format!(
                "<img alt='SVG' style='display:block;width:100%;height:80vh;object-fit:contain' src='data:image/svg+xml;base64,{}'>",
                STANDARD.encode(data)
            )
        }
        _ => return Err("不支持的文档格式".into()),
    };
    Ok(vec![html])
}

fn email(bytes: &[u8]) -> PreviewResult<String> {
    let mail = mail_parser::MessageParser::new()
        .parse(bytes)
        .ok_or("邮件无法解析")?;
    let mut out = format!("<h1>{}</h1>", escape(mail.subject().unwrap_or("无主题")));
    for (label, group) in [("发件人", mail.from()), ("收件人", mail.to())] {
        if let Some(group) = group {
            let addresses = group
                .mailboxes()
                .map(|m| {
                    format!(
                        "{} &lt;{}&gt;",
                        escape(m.name().unwrap_or("")),
                        escape(m.address().unwrap_or(""))
                    )
                })
                .collect::<Vec<_>>();
            out.push_str(&format!("<p>{label}：{}</p>", addresses.join(", ")));
        }
    }
    if let Some(date) = mail.date() {
        out.push_str(&format!("<p>{}</p>", escape(&date.to_rfc3339())));
    }
    for part in mail.text_bodies() {
        out.push_str(&format!(
            "<div style='white-space:pre-wrap'>{}</div>",
            escape(&part)
        ));
    }
    for part in mail.attachments() {
        out.push_str(&format!(
            "<p>附件：{}（{}）</p>",
            escape(part.attachment_name().unwrap_or("未命名")),
            escape(&crate::core::size(part.decoded_len() as u64))
        ));
    }
    Ok(out)
}
