use std::{path::PathBuf, time::Instant};
fn main() {
    let output = std::env::var_os("QUICKPEEK_EXPORT_PREVIEWS").map(PathBuf::from);
    if let Some(dir) = &output {
        std::fs::create_dir_all(dir).unwrap();
    }
    for (index, arg) in std::env::args_os().skip(1).enumerate() {
        let path = PathBuf::from(arg);
        let start = Instant::now();
        match quickpeek::core::load(&path) {
            Ok(doc) => {
                println!(
                    "{:?}: {} bytes, {} ms, image {:?}",
                    doc.kind,
                    doc.bytes,
                    start.elapsed().as_millis(),
                    doc.image.as_ref().map(|(_, w, h)| (*w, *h))
                );
                if let (Some(dir), Some(preview)) = (&output, &doc.preview) {
                    std::fs::copy(&preview.0, dir.join(format!("{index}.html"))).unwrap();
                    for resource in preview.resources() {
                        std::fs::copy(resource, dir.join(resource.file_name().unwrap())).unwrap();
                    }
                }
            }
            Err(error) => panic!("{}: {error}", path.display()),
        }
    }
}
