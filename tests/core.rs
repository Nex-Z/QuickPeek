use quickpeek::core::{self, Kind};
use std::path::Path;

#[test]
fn json_preview_preserves_file_and_precision() {
    let path = Path::new("tests/fixtures/example.json");
    let before = std::fs::read(path).unwrap();
    let d = core::load(path).unwrap();
    assert_eq!(d.kind, Kind::Json);
    assert!(d.text.contains("123456789012345678901234567890"));
    assert!(d.text.lines().count() > 5);
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert!(d.preview.as_ref().unwrap().0.is_file());
    assert_eq!(d.source.as_bytes(), before.as_slice());
}
#[test]
fn malformed_json_stays_readable() {
    let d = core::load(Path::new("tests/fixtures/invalid.json")).unwrap();
    assert!(d.notice.contains("JSON 格式错误"));
    assert!(d.text.contains(",}"));
}
#[test]
fn code_and_navigation() {
    let p = Path::new("tests/fixtures/example.rs");
    let d = core::load(p).unwrap();
    assert_eq!(d.kind, Kind::Code);
    assert!(
        d.lines.is_empty(),
        "Highlighting must not block file loading"
    );
    assert!(d.preview.is_some());
    assert!(core::siblings(p).contains(&p.into()));
}
#[test]
fn bounded_large_text_and_binary_rejection() {
    let folder = std::env::temp_dir().join(format!("quickpeek-test-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let large = folder.join("large.txt");
    std::fs::write(&large, vec![b'a'; core::TEXT_LIMIT as usize + 100]).unwrap();
    let d = core::load(&large).unwrap();
    assert_eq!(d.text.len(), core::TEXT_LIMIT as usize);
    assert!(d.notice.contains("16 MB"));
    let binary = folder.join("binary.txt");
    std::fs::write(&binary, [0, 1, 2, 3]).unwrap();
    assert!(core::load(&binary).is_err());
    std::fs::write(&large, "中".repeat(core::TEXT_LIMIT as usize / 3 + 20)).unwrap();
    let unicode = core::load(&large).unwrap();
    assert!(unicode.text.chars().all(|c| c == '中'));
    assert!(core::load_cancellable(&large, || true).is_err());
    std::fs::remove_file(large).unwrap();
    std::fs::remove_file(binary).unwrap();
    std::fs::remove_dir(folder).unwrap();
}
#[test]
fn directory_preview_lists_only_immediate_children_and_sorts_folders_first() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("album.png");
    std::fs::create_dir(&dir).unwrap();
    std::fs::create_dir(dir.join("nested")).unwrap();
    std::fs::write(dir.join("nested/hidden.txt"), "nested").unwrap();
    std::fs::write(dir.join("a.txt"), "text").unwrap();
    let doc = quickpeek::core::load(&dir).unwrap();
    assert_eq!(doc.kind, quickpeek::core::Kind::Folder);
    assert_eq!(doc.children.len(), 2);
    assert!(doc.children[0].directory);
    assert_eq!(doc.children[0].path, dir.join("nested"));
    assert_eq!(doc.children[1].path, dir.join("a.txt"));
}

#[test]
fn display_size_uses_kb_through_gb() {
    for (bytes, expected) in [
        (0, "0 KB"),
        (1, "<0.1 KB"),
        (512, "0.5 KB"),
        (1024, "1 KB"),
        ((1 << 20) - 1, "1023.9 KB"),
        (1 << 20, "1 MB"),
        ((1 << 30) - 1, "1023.9 MB"),
        (1 << 30, "1 GB"),
        (1 << 40, "1024 GB"),
    ] {
        assert_eq!(quickpeek::core::size(bytes), expected);
    }
}
