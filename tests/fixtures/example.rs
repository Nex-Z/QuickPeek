use std::path::Path;

/// A small preview request.
fn preview(path: &Path) -> Result<(), String> {
    let name = path.file_name().unwrap_or_default();
    println!("预览文件: {name:?}");
    Ok(())
}

fn main() {
    preview(Path::new("QuickPeek.png")).unwrap();
}
