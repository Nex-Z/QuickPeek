# QuickPeek

Windows 11 x64 原生文件快速预览工具。Rust + GPUI，经典阅读器使用 WinUI 3，无需 .NET。

## 使用

安装版运行 Windows x64 安装程序，安装后从开始菜单打开 QuickPeek。便携版解压 **整个便携包** 后运行 `quickpeek.exe`，图标出现在系统托盘。
文件夹也可用空格预览：点击子目录进入，点击文件单独预览。图片可直接用滚轮缩放，GPUI 图片放大后可按住左键拖动。右上角图钉切换窗口置顶。
在资源管理器中选中文件，按 **Space** 打开预览。右键托盘图标可以打开预览或退出。
文本、代码源码、JSON 和常见图片默认使用 GPUI。Esc 或空格关闭窗口，按 Enter 或右键选择“完整预览”切换到经典阅读器，使用搜索、选择复制、代码高亮、图片缩放和同目录导航。PDF、音视频、Markdown、HTML 和表格直接使用经典阅读器。命令行可用 `quickpeek.exe --classic "文件路径"` 直接进入经典阅读器。

### 经典阅读器

预览默认在当前屏幕工作区居中，不固定置顶。图钉与右上角系统窗口按钮对齐，可切换置顶状态，开启后显示蓝色。左上角显示 QuickPeek 图标和文件名；不保留“更多”、底部提示栏或常驻搜索框。
文本、代码、Markdown、JSON 按 Ctrl+F 打开搜索并自动聚焦；Esc 先收起搜索并恢复全文，再按一次关闭预览。PDF 使用内嵌 WebView2 阅读器，支持连续滚动、文字选择复制及 Ctrl+F 搜索。
普通滚轮上下滚动；图片直接用滚轮缩放，PDF 按住 Ctrl 滚轮缩放。图片默认按窗口宽高等比适配，不强制拉满宽度；不再显示适配模式按钮，长图放大后可以滚动查看。普通文本支持跨行选择，Markdown 源码模式也支持连续选择复制。
音频和视频打开后自动播放，使用深色圆角控制区，支持播放/暂停、拖动进度、静音/音量、0.5–2.0 倍速及最大化/还原。
也可以把文件拖到 exe 上，或运行 `quickpeek.exe "D:\文件\示例.pdf"`。

| 操作 | 快捷键 |
| --- | --- |
| 关闭预览 | Space / Esc（PDF 使用 Esc，Space 用于向下阅读） |
| 同目录上一个 / 下一个文件 | ← / →（PDF 中保留阅读器导航行为） |
| 缩放 | Ctrl+滚轮；图片 / 文本也可用 − / +；PDF 可用 Ctrl+− / Ctrl++ |
| 使用默认应用打开 | Enter（PDF 中保留阅读器输入行为） |
| 文本 / 代码 / Markdown / JSON 搜索 | Ctrl+F |
| 上一页 / 下一页 | PgUp / PgDn |
| 打开其他文件 | Ctrl+O |
| 文件信息 | Ctrl+I |
| Markdown / HTML / CSV / 代码 / JSON 源码模式切换 | Ctrl+R |
| 在文件夹中显示 | Ctrl+L |

搜索、重命名和地址输入框中的空格保留正常输入行为。
已有预览未关闭时仍可从资源管理器按空格打开新预览。新窗口激活到前台，默认不永久置顶。
关闭预览窗口不会退出托盘服务。不写注册表、不设置开机启动、不改动被预览的文件。

## 首版格式

| 类别 | 功能 |
| --- | --- |
| Word | DOCX 正文、标题、基本字符样式、表格和内嵌常见位图 |
| PowerPoint | PPTX 按演示顺序逐页浏览文字、表格和内嵌常见位图；不播放动画 |
| 压缩包 | ZIP、7z、RAR 按目录浏览，可进入子目录、返回上一级或点击路径跳转。点击支持的文件可单独预览（最大 64 MiB）。7z、RAR 通过 Windows 自带的 tar 读取，支持情况取决于系统版本 |
| 矢量图 | SVG 在隔离的 WebView2 阅读器内显示 |
| 邮件 | EML 的主题、发件人、收件人、日期、文字正文和附件名 |
| 富文本 | RTF 正文、段落、粗体、斜体和下划线 |
| 电子书 | EPUB 按书目阅读顺序翻章，显示正文和书内常见位图 |
| 图片 | PNG、JPEG、BMP、GIF、TIFF、WebP 等系统可解码格式，适应窗口、缩放 |
| 文本 | UTF-8、UTF-16 BOM、GBK 回退，连续滚动、跨行选择、文本搜索 |
| 代码 | Rust、Python、JavaScript、TypeScript、C/C++、Go 等，语法高亮 |
| JSON | 格式化显示、保留数字精度和键顺序；错误 JSON 显示原文和错误信息 |
| Markdown | 分级标题、嵌套列表、强调、代码块、表格、同目录本地图片；源码切换 |
| HTML | 默认渲染页面，支持源码切换；预览不执行页面脚本或加载远程资源 |
| CSV / TSV | 默认表格视图、分页、筛选、源码切换 |
| Excel | XLSX、XLS、XLSB、ODS 单元格值预览、工作表切换、分页和筛选 |
| 音频 | WAV、MP3、M4A、AAC、FLAC 等，自动播放与进度/音量控制，取决于系统解码器 |
| PDF | WebView2 原生阅读器，连续滚动、文字选择复制、搜索、缩放 |
| 视频 | Media Foundation 播放、暂停、进度定位和音量；格式取决于系统解码器 |

插件协议已预留，首版不执行第三方插件，详见 [插件接口](docs/PLUGINS.md)。

## 当前边界

- 首版优先把完整预览链路跑通，未宣称达到极限启动或内存指标。
- 每次打开启动独立预览进程；同一个预览窗口内可以连续切换文件。
- 文本读取上限 16 MiB，超出会提示；大文本按可见区域加载与着色。HTML、Markdown 源码和常见代码支持离线语法高亮。大型 Markdown / HTML 按完整区块分页渲染，源码保留完整读取内容。表格每页 100 行，CSV 最多 250,000 行；Excel 每工作表最多 250,000 行、256 列或 2,000,000 个单元格，达到上限会提示。
- 同目录切换按文件名字母顺序，不同步资源管理器的所有排序模式。
- 文本、代码、JSON、Markdown、HTML、表格和 PDF 使用 WebView2。Markdown / HTML 的渲染内容在禁用脚本的隔离框架内显示，不请求远程资源。Excel 显示已保存的值，不重新计算公式，不复刻图表、合并单元格和全部样式。
- PDF 需要系统安装 Microsoft Edge WebView2 Runtime；浏览器配置与缓存放在 `%LOCALAPPDATA%\QuickPeek\WebView2`。扫描图片型 PDF 若没有文字层，不能直接选择文字；文档权限限制仍然适用。
- 视频不是全格式播放器；HEVC、AV1 等可能需要系统已有解码器。GIF 等动图目前按原生图片控件行为显示。
- 便携包附带 WinUI 运行时；不要只复制 exe。未做代码签名，尚未在干净系统 / 多屏混合 DPI 上完整验收。

## 开发

需要 Windows、MSVC C++ 构建工具和 Rust 1.98+。首次构建会下载 Windows App SDK 运行时。

```powershell
cargo run -- --preview tests\fixtures\readme.md
cargo test
cargo clippy --all-targets -- -D warnings
powershell -ExecutionPolicy Bypass -File scripts\package.ps1
```

工程说明：[产品约定](docs/PRODUCT.md)、[验收记录](docs/ACCEPTANCE.md)。

默认 Release 构建使用 Windows SDK 的 x64 `fxc.exe`。直接运行 Cargo 时，可通过 PATH 或 `GPUI_FXC_PATH` 指定编译器。仅构建经典版可使用 `cargo build --release --no-default-features`。
