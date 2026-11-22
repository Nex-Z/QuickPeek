use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const TEXT_LIMIT: u64 = 16 * 1024 * 1024;
pub const PAGE_LINES: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Folder,
    Image,
    Text,
    Code,
    Markdown,
    Json,
    Pdf,
    Video,
    Audio,
    Html,
    Csv,
    Spreadsheet,
    Word,
    PowerPoint,
    Archive,
    Svg,
    Email,
    Rtf,
    Epub,
    Unknown,
}

impl Kind {
    pub fn rich_file(self) -> bool {
        matches!(
            self,
            Self::Word
                | Self::PowerPoint
                | Self::Archive
                | Self::Svg
                | Self::Email
                | Self::Rtf
                | Self::Epub
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Folder => "文件夹",
            Self::Image => "图片",
            Self::Text => "文本",
            Self::Code => "代码",
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
            Self::Pdf => "PDF 文档",
            Self::Video => "视频",
            Self::Audio => "音频",
            Self::Html => "HTML",
            Self::Csv => "CSV 表格",
            Self::Spreadsheet => "Excel 表格",
            Self::Word => "Word",
            Self::PowerPoint => "PowerPoint",
            Self::Archive => "压缩包",
            Self::Svg => "SVG",
            Self::Email => "邮件",
            Self::Rtf => "富文本",
            Self::Epub => "电子书",
            Self::Unknown => "文件",
        }
    }
}

pub fn detect(path: &Path) -> Kind {
    match path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tif" | "tiff" | "webp" | "ico" => Kind::Image,
        "mp4" | "m4v" | "mov" | "wmv" | "avi" | "mkv" | "webm" => Kind::Video,
        "wav" | "mp3" | "m4a" | "aac" | "flac" | "ogg" | "wma" => Kind::Audio,
        "html" | "htm" => Kind::Html,
        "csv" | "tsv" => Kind::Csv,
        "xlsx" | "xls" | "xlsb" | "ods" => Kind::Spreadsheet,
        "docx" => Kind::Word,
        "pptx" => Kind::PowerPoint,
        "zip" | "7z" | "rar" => Kind::Archive,
        "svg" => Kind::Svg,
        "eml" => Kind::Email,
        "rtf" => Kind::Rtf,
        "epub" => Kind::Epub,
        "pdf" => Kind::Pdf,
        "md" | "markdown" | "mdown" => Kind::Markdown,
        "json" | "jsonc" => Kind::Json,
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "c" | "h" | "cpp" | "hpp" | "cs" | "go"
        | "java" | "css" | "scss" | "sql" | "sh" | "ps1" | "toml" | "yaml" | "yml" | "xml"
        | "rb" | "php" | "swift" | "kt" | "lua" | "vue" => Kind::Code,
        "txt" | "log" | "ini" | "conf" | "cfg" | "gitignore" | "env" => Kind::Text,
        _ => Kind::Unknown,
    }
}

#[derive(Clone, Debug)]
pub struct Span {
    pub text: String,
    pub color: [u8; 3],
}
#[derive(Clone, Debug)]
pub struct FolderEntry {
    pub path: PathBuf,
    pub directory: bool,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub children: Vec<FolderEntry>,
    pub kind: Kind,
    pub text: String,
    pub lines: Vec<Vec<Span>>,
    pub notice: String,
    pub bytes: u64,
    pub source: String,
    pub preview: Option<std::sync::Arc<crate::reading::PreviewFile>>,
    pub image: Option<(std::sync::Arc<[u8]>, u32, u32)>,
}

pub fn load(path: &Path) -> Result<Document, String> {
    load_cancellable(path, || false)
}

pub fn load_cancellable(path: &Path, cancelled: impl Fn() -> bool) -> Result<Document, String> {
    load_document(path, cancelled, true)
}

#[cfg(feature = "gpui-preview")]
pub fn load_for_gpui(path: &Path) -> Result<Document, String> {
    load_document(path, || false, false)
}

fn load_document(
    path: &Path,
    cancelled: impl Fn() -> bool,
    prepare_reader: bool,
) -> Result<Document, String> {
    if cancelled() {
        return Err("预览已取消".into());
    }
    let metadata = path.metadata().map_err(|e| e.to_string())?;
    let mut children = Vec::new();
    let mut kind = if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            if cancelled() {
                return Err("预览已取消".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let child = entry.path();
            children.push(FolderEntry {
                directory: entry
                    .file_type()
                    .is_ok_and(|t| t.is_dir() || (t.is_symlink() && child.is_dir())),
                path: child,
            });
        }
        children.sort_by_cached_key(|e| {
            (
                !e.directory,
                e.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase(),
            )
        });
        Kind::Folder
    } else if metadata.is_file() {
        detect(path)
    } else {
        return Err("此项目无法预览".into());
    };
    let mut text = String::new();
    let mut notice = String::new();
    let mut source = String::new();
    if !kind.rich_file()
        && !matches!(
            kind,
            Kind::Folder | Kind::Image | Kind::Video | Kind::Audio | Kind::Pdf | Kind::Spreadsheet
        )
    {
        let mut data = Vec::new();
        File::open(path)
            .map_err(|e| e.to_string())?
            .take(TEXT_LIMIT + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        let truncated = data.len() as u64 > TEXT_LIMIT;
        data.truncate(TEXT_LIMIT as usize);
        // A bounded UTF-8 read may end inside a code point. Preserve the valid
        // prefix instead of misclassifying the entire document as legacy GBK.
        if truncated
            && let Err(error) = std::str::from_utf8(&data)
            && error.error_len().is_none()
            && error.valid_up_to() > 0
        {
            data.truncate(error.valid_up_to());
        }
        if kind == Kind::Unknown && !data.contains(&0) && std::str::from_utf8(&data).is_ok() {
            kind = Kind::Text;
        }
        if kind != Kind::Unknown {
            text = decode(&data)?;
            source = text.clone();
            if truncated {
                notice = "大文件：仅显示前 16 MB，原文件未修改".into();
            }
            if kind == Kind::Json && !truncated {
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(value) => {
                        text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
                    }
                    Err(e) => notice = format!("JSON 格式错误，显示原文：{e}"),
                }
            }
        }
    }
    // Syntax highlighting is deferred to visible reader chunks.
    let lines = Vec::new();
    if cancelled() {
        return Err("预览已取消".into());
    }
    let image = if kind == Kind::Image {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let reader = image::ImageReader::new(std::io::Cursor::new(&data))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        if reader.format() == Some(image::ImageFormat::WebP) {
            // WIC WebP support varies by system. Transcode only this format,
            // using fast compression rather than recompressing every image.
            use image::{
                ImageEncoder,
                codecs::png::{CompressionType, FilterType, PngEncoder},
            };
            let image = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
            let (width, height) = image.dimensions();
            let mut encoded = Vec::new();
            PngEncoder::new_with_quality(&mut encoded, CompressionType::Fast, FilterType::Adaptive)
                .write_image(
                    image.as_raw(),
                    width,
                    height,
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| e.to_string())?;
            Some((std::sync::Arc::from(encoded), width, height))
        } else {
            let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
            Some((std::sync::Arc::from(data), width, height))
        }
    } else {
        None
    };
    let mut document = Document {
        children,
        kind,
        text,
        lines,
        notice,
        bytes: metadata.len(),
        source,
        preview: None,
        image,
    };
    if prepare_reader
        && (kind.rich_file()
            || matches!(
                kind,
                Kind::Text
                    | Kind::Code
                    | Kind::Json
                    | Kind::Markdown
                    | Kind::Html
                    | Kind::Csv
                    | Kind::Spreadsheet
            ))
    {
        document.preview = Some(std::sync::Arc::new(crate::reading::prepare(
            path, &document, &cancelled,
        )?));
    }
    Ok(document)
}

fn decode(data: &[u8]) -> Result<String, String> {
    if data.starts_with(&[0xff, 0xfe]) {
        return Ok(encoding_rs::UTF_16LE.decode(&data[2..]).0.into_owned());
    }
    if data.starts_with(&[0xfe, 0xff]) {
        return Ok(encoding_rs::UTF_16BE.decode(&data[2..]).0.into_owned());
    }
    let data = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(data);
    if data.contains(&0) {
        return Err("文件包含二进制数据，无法作为文本预览".into());
    }
    Ok(match std::str::from_utf8(data) {
        Ok(s) => s.into(),
        Err(_) => encoding_rs::GBK.decode(data).0.into_owned(),
    })
}

pub fn siblings(path: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = path
        .parent()
        .and_then(|p| std::fs::read_dir(p).ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|e| e.path())
        .filter(|p| detect(p) != Kind::Unknown || p == path)
        .collect();
    files.sort_by_cached_key(|p| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    if !files.iter().any(|p| p == path) {
        files.push(path.into());
    }
    files
}

pub fn size(bytes: u64) -> String {
    let (unit, label) = match bytes {
        0 => return "0 KB".into(),
        1..=102 => return "<0.1 KB".into(),
        103..=1_048_575 => (1024, "KB"),
        1_048_576..=1_073_741_823 => (1024 * 1024, "MB"),
        _ => (1024 * 1024 * 1024, "GB"),
    };
    let whole = bytes / unit;
    let tenth = bytes % unit * 10 / unit;
    if tenth == 0 {
        format!("{whole} {label}")
    } else {
        format!("{whole}.{tenth} {label}")
    }
}

/// Versioned wire types: extension hosts communicate through bounded JSON messages.
/// A Rust trait object / Rust ABI must never cross a third-party process boundary.
pub mod plugin {
    use super::*;
    pub const PROTOCOL_VERSION: u32 = 1;
    pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Manifest {
        pub id: String,
        pub protocol_version: u32,
        pub executable: PathBuf,
        pub extensions: Vec<String>,
    }
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "method", rename_all = "snake_case")]
    pub enum Request {
        Probe {
            id: u64,
            path: PathBuf,
        },
        Preview {
            id: u64,
            path: PathBuf,
            max_bytes: u64,
        },
        Cancel {
            id: u64,
        },
    }
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum Response {
        Text { id: u64, text: String },
        Image { id: u64, png_path: PathBuf },
        Error { id: u64, message: String },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_and_case() {
        assert_eq!(detect(Path::new("a.MP4")), Kind::Video);
        assert_eq!(detect(Path::new("a.md")), Kind::Markdown);
    }
    #[test]
    fn encodings() {
        assert_eq!(decode(&[255, 254, 0x2d, 0x4e]).unwrap(), "中");
        assert_eq!(decode(&[239, 187, 191, 65]).unwrap(), "A");
        assert!(decode(&[0, 1]).is_err());
    }
    #[test]
    fn json_precision_and_order() {
        let s = "{\"z\":123456789012345678901234567890,\"a\":1}";
        let v: serde_json::Value = serde_json::from_str(s).unwrap();
        let pretty = serde_json::to_string_pretty(&v).unwrap();
        assert!(pretty.contains("123456789012345678901234567890"));
        assert!(pretty.find("\"z\"") < pretty.find("\"a\""));
    }
    #[test]
    fn plugin_contract() {
        let q = plugin::Request::Cancel { id: 9 };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains("cancel"));
        assert!(serde_json::from_str::<plugin::Request>(&s).is_ok());
    }
}
