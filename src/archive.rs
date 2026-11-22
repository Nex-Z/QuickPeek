use crate::core::{Kind, detect};
use serde::Serialize;
use std::{
    fs::File,
    io::Read,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Serialize)]
pub struct Entry {
    pub path: String,
    pub size: Option<u64>,
    size_label: Option<String>,
    pub directory: bool,
    pub kind: Kind,
}
const MAX_ENTRIES: usize = 10_000;
const MEMBER_LIMIT: u64 = 64 * 1024 * 1024;

fn zip(path: &Path) -> Result<zip::ZipArchive<File>, String> {
    zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn is_zip(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

pub fn entries(path: &Path, cancelled: &impl Fn() -> bool) -> Result<(Vec<Entry>, bool), String> {
    if is_zip(path) {
        let mut archive = zip(path)?;
        let count = archive.len();
        let mut entries = Vec::new();
        let mut bytes = 0;
        for i in 0..count.min(MAX_ENTRIES) {
            if cancelled() {
                return Err("预览已取消".into());
            }
            let item = archive.by_index_raw(i).map_err(|e| e.to_string())?;
            bytes += item.name().len();
            if bytes > 8 * 1024 * 1024 {
                return Ok((entries, true));
            }
            entries.push(Entry {
                kind: detect(Path::new(item.name())),
                path: item.name().to_owned(),
                directory: item.is_dir(),
                size: Some(item.size()),
                size_label: Some(crate::core::size(item.size())),
            });
        }
        return Ok((entries, count > MAX_ENTRIES));
    }
    // mtree encodes whitespace and control characters as octal escapes and carries
    // metadata separately from the name. No members are written to disk.
    let mut command = tar()?;
    let source = path.canonicalize().map_err(|e| e.to_string())?;
    let mut archive_arg = std::ffi::OsString::from("@");
    archive_arg.push(source);
    command
        .args(["-cf", "-", "--format=mtree", "--options", "!all,type,size"])
        .arg(archive_arg);
    let (bytes, mut truncated) = output(command, 8 * 1024 * 1024)?;
    let mut entries = Vec::new();
    for line in bytes.split_inclusive(|&c| c == b'\n') {
        if cancelled() {
            return Err("预览已取消".into());
        }
        if !line.starts_with(b"./") || (truncated && !line.ends_with(b"\n")) {
            continue;
        }
        if entries.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        let mut fields = line
            .split(|c| c.is_ascii_whitespace())
            .filter(|s| !s.is_empty());
        let name = fields.next().unwrap();
        let path = decode_name(&name[2..]);
        let mut entry = Entry {
            kind: detect(Path::new(&path)),
            path,
            directory: false,
            size: None,
            size_label: None,
        };
        for f in fields {
            if f == b"type=dir" {
                entry.directory = true
            }
            if let Some(size) = f.strip_prefix(b"size=") {
                entry.size = std::str::from_utf8(size).ok().and_then(|s| s.parse().ok())
            }
        }
        entry.size_label = entry.size.map(crate::core::size);
        entries.push(entry);
    }
    Ok((entries, truncated))
}

fn decode_name(name: &[u8]) -> String {
    let mut raw = Vec::new();
    let mut i = 0;
    while i < name.len() {
        if name[i] == b'\\'
            && let Some(digits) = name.get(i + 1..i + 4)
            && digits.iter().all(|d| (b'0'..=b'7').contains(d))
        {
            raw.push(((digits[0] - 48) << 6) | ((digits[1] - 48) << 3) | (digits[2] - 48));
            i += 4;
        } else {
            raw.push(name[i]);
            i += 1;
        }
    }
    use windows::Win32::Globalization::{CP_ACP, MultiByteToWideChar};
    let mut wide = vec![0; raw.len()];
    let n = unsafe { MultiByteToWideChar(CP_ACP, Default::default(), &raw, Some(&mut wide)) };
    String::from_utf16_lossy(&wide[..n as usize])
}

fn tar() -> Result<Command, String> {
    let root = std::env::var_os("SystemRoot").ok_or("找不到 Windows 系统目录")?;
    let mut cmd = Command::new(Path::new(&root).join("System32/tar.exe"));
    cmd.creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    Ok(cmd)
}
fn output(mut cmd: Command, limit: u64) -> Result<(Vec<u8>, bool), String> {
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    let read = child
        .stdout
        .take()
        .unwrap()
        .take(limit + 1)
        .read_to_end(&mut data);
    let truncated = data.len() as u64 > limit;
    if truncated || read.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    read.map_err(|e| e.to_string())?;
    if !status.success() && !truncated {
        return Err("无法读取压缩包条目，文件可能已加密、损坏或系统 tar 不支持该格式".into());
    }
    Ok((data, truncated))
}

pub fn member(path: &Path, name: &str) -> Result<(tempfile::TempDir, PathBuf), String> {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or("");
    if leaf.is_empty() || leaf.contains(':') || detect(Path::new(leaf)) == Kind::Unknown {
        return Err("此文件类型暂未提供内容预览".into());
    }
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    // Keep the member recognizable without using its directories or a DOS device name.
    let leaf = Path::new(leaf);
    let stem: String = leaf
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(70)
        .map(|c| {
            if c.is_control() || "<>|?*\"".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let ext = leaf.extension().ok_or("文件没有扩展名")?.to_string_lossy();
    let target = dir.path().join(format!("包内 - {stem}.{ext}"));
    if is_zip(path) {
        let mut archive = zip(path)?;
        let entry = archive.by_name(name).map_err(|e| e.to_string())?;
        if entry.is_dir() || entry.size() > MEMBER_LIMIT {
            return Err("仅支持预览 64 MB 以内的文件".into());
        }
        let mut out = File::create(&target).map_err(|e| e.to_string())?;
        let size = std::io::copy(&mut entry.take(MEMBER_LIMIT + 1), &mut out)
            .map_err(|e| e.to_string())?;
        if size > MEMBER_LIMIT {
            return Err("文件超过 64 MB".into());
        }
    } else {
        let mut cmd = tar()?;
        // bsdtar accepts shell patterns even with --; escape metacharacters.
        let pattern = name
            .chars()
            .map(|c| match c {
                '*' => "[*]".into(),
                '?' => "[?]".into(),
                '[' => "[[]".into(),
                c => c.to_string(),
            })
            .collect::<String>();
        cmd.args(["-xOf"])
            .arg(path)
            .args(["--no-recursion", "--"])
            .arg(pattern);
        let (bytes, truncated) = output(cmd, MEMBER_LIMIT)?;
        if truncated {
            return Err("文件超过 64 MB".into());
        }
        std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    }
    Ok((dir, target))
}
