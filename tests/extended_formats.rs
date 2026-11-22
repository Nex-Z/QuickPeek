use quickpeek::core;
use std::path::Path;

fn pages(extension: &str) -> Vec<String> {
    let path = Path::new("tests/fixtures/formats").join(format!("sample.{extension}"));
    let before = std::fs::read(&path).unwrap();
    let doc = core::load(&path).unwrap_or_else(|error| panic!("{extension}: {error}"));
    let preview = doc.preview.unwrap();
    let js = std::fs::read_to_string(preview.0.with_extension("data.js")).unwrap();
    let payload: serde_json::Value = serde_json::from_str(
        js.strip_prefix("const data = ")
            .unwrap()
            .trim_end()
            .trim_end_matches(';'),
    )
    .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), before);
    serde_json::from_value(payload["rendered_pages"].clone()).unwrap()
}

#[test]
fn office_content_and_presentation_order() {
    let word = &pages("docx")[0];
    for part in ["项目摘要", "font-weight:bold", "<table>", "Table cell"] {
        assert!(word.contains(part), "{part}");
    }
    let slides = pages("pptx");
    assert_eq!(slides.len(), 2);
    assert!(slides[0].contains("First slide"));
    assert!(slides[1].contains("Second slide"));
}

#[test]
fn epub_uses_spine_order() {
    let chapters = pages("epub");
    assert_eq!(chapters.len(), 2);
    assert!(chapters[0].contains("<h1>Chapter b</h1>"));
    assert!(chapters[1].contains("<h1>Chapter a</h1>"));
}

#[test]
fn archive_listings_include_members_and_sizes() {
    for kind in ["zip", "7z", "rar"] {
        let path = Path::new("tests/fixtures/formats").join(format!("sample.{kind}"));
        let (entries, truncated) = quickpeek::archive::entries(&path, &|| false).unwrap();
        assert!(!truncated);
        let member = entries
            .iter()
            .find(|e| e.path == "folder/sub/note.txt")
            .expect(kind);
        assert_eq!(member.size, Some(9));
        let (dir, file) = quickpeek::archive::member(&path, &member.path).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"QuickPeek");
        drop(dir);
        assert!(!file.exists());
    }
}

#[test]
fn svg_email_and_rtf() {
    assert!(pages("svg")[0].contains("data:image/svg+xml;base64,"));
    let mail = &pages("eml")[0];
    for part in ["邮件样例", "邮件正文", "附件.txt", "alice@example.com"] {
        assert!(mail.contains(part), "{part}");
    }
    let rtf = &pages("rtf")[0];
    assert!(
        rtf.contains("<strong>") && rtf.contains("Bold") && rtf.contains("<em>"),
        "{rtf}"
    );
}

#[test]
fn svg_accepts_utf16_and_a_doctype() {
    let path = std::env::temp_dir().join(format!("quickpeek-svg-{}.svg", std::process::id()));
    let source = r#"<?xml version="1.0" encoding="UTF-16"?><!DOCTYPE svg><svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#;
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(source.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&path, bytes).unwrap();
    let result = core::load(&path);
    std::fs::remove_file(path).unwrap();
    assert!(result.unwrap().preview.is_some());
}
