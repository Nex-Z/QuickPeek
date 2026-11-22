use std::path::Path;
use windows_webview::WebView;

pub fn open(webview: &WebView, path: &Path) -> Result<(), String> {
    let uri = url::Url::from_file_path(path).map_err(|_| "PDF 路径无效")?;
    webview.navigate(uri.as_str()).map_err(|e| e.to_string())
}
