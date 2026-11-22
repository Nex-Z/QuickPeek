use quickpeek::core::{self, Kind};
use std::path::Path;

#[test]
fn new_formats() {
    for (file, kind) in [
        ("sound.WAV", Kind::Audio),
        ("index.htm", Kind::Html),
        ("data.csv", Kind::Csv),
        ("data.XLSX", Kind::Spreadsheet),
    ] {
        assert_eq!(core::detect(Path::new(file)), kind);
    }
}

#[test]
fn csv_quoted_fields_and_embedded_newlines() {
    let path = std::env::temp_dir().join(format!("quickpeek-csv-{}.csv", std::process::id()));
    let source = "name,note\r\nAlice,\"hello, world\"\r\nBob,\"line1\nline2\"\r\n";
    std::fs::write(&path, source).unwrap();
    let doc = core::load(&path).unwrap();
    let preview = doc.preview.as_ref().unwrap().0.clone();
    let html = std::fs::read_to_string(preview.with_extension("data.js")).unwrap();
    assert!(html.contains("hello, world"));
    assert!(html.contains("line1\\nline2"));
    assert!(html.contains("\"total\":3"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    drop(doc);
    assert!(!preview.exists());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn html_cannot_escape_reader_payload() {
    let path = std::env::temp_dir().join(format!("quickpeek-html-{}.html", std::process::id()));
    std::fs::write(
        &path,
        "</script><script>window.injection=true</script><h1>Heading</h1>",
    )
    .unwrap();
    let doc = core::load(&path).unwrap();
    let html = std::fs::read_to_string(&doc.preview.as_ref().unwrap().0).unwrap();
    assert!(!html.contains("<script>window.injection"));
    let data =
        std::fs::read_to_string(doc.preview.as_ref().unwrap().0.with_extension("data.js")).unwrap();
    assert!(data.contains("<h1>Heading</h1>"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn markdown_retains_heading_and_list_structure() {
    let doc = core::load(Path::new("tests/fixtures/readme.md")).unwrap();
    let html =
        std::fs::read_to_string(doc.preview.as_ref().unwrap().0.with_extension("data.js")).unwrap();
    assert!(html.contains("<h1>"));
    assert!(html.contains("<li>"));
}

#[test]
fn image_dimensions_and_path_independent_decoding() {
    let doc = core::load(Path::new("tests/fixtures/prototype.png")).unwrap();
    let (bytes, w, h) = doc.image.unwrap();
    assert!(bytes.starts_with(b"\x89PNG"));
    assert!(w > 0 && h > 0);
}
