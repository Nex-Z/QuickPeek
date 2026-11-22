use crate::platform::caption;
use crate::{
    core::{self, Document, Kind},
    platform::{self, media::Player},
};
use std::{path::PathBuf, sync::Arc};
use windows_reactor::*;

pub fn run(path: Option<PathBuf>) -> Result<(), String> {
    platform::prepare_preview();
    App::run_component::<Preview>(path).map_err(|e| e.to_string())
}

struct Preview {
    path: Option<PathBuf>,
    files: Vec<PathBuf>,
    document: Option<Document>,
    error: String,
    generation: u64,
    loading: bool,
    reader_expected: bool,
    reader_navigated: bool,
    page: usize,
    pages: usize,
    zoom: f64,
    info: bool,
    pinned: bool,
    muted: bool,
    rate: f32,
    query: String,
    raw: bool,
    dark: bool,
    keys: Option<platform::KeyboardGuard>,
    wheel: Option<platform::wheel::WheelGuard>,
    player: Option<Player>,
    position: f64,
    duration: f64,
    volume: f64,
    task: Option<ComponentTask>,
    search_open: bool,
    search_ref: ElementRef<TextBox>,
    initial_focus: ElementRef<Button>,
    member_listener: std::rc::Rc<std::cell::RefCell<Option<windows_webview::EventRegistration>>>,
    reader: std::rc::Rc<std::cell::RefCell<Option<windows_webview::WebView>>>,
}
#[derive(Clone)]
enum Msg {
    Ready,
    Loaded(u64, Result<Document, String>),
    FilesLoaded(u64, Vec<PathBuf>),
    PdfReady(u64, Result<(), String>),
    ReaderReady(u64, Result<(), String>),
    Open,
    Browse(PathBuf),
    PreviewChild(PathBuf),
    Next(i32),
    Page(i32),
    Zoom(f64),
    Info,
    Pin,
    Mute,
    Rate,
    Maximize,
    Raw,
    Search(String),
    Find,
    FocusSearch,
    CaptionLayout,
    HideSearch,
    Key(u32),
    Close,
    External,
    Reveal,
    Error(String),
    Theme(ColorScheme),
    Play,
    Seek(f64),
    Volume(f64),
    Tick(u64),
}

impl Preview {
    fn load(&mut self, path: PathBuf, c: &ComponentContext<Self>) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        // WinRT accepts ordinary absolute paths; remove the Win32 extended prefix.
        let value = path.to_string_lossy();
        let path = if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
            PathBuf::from(format!("\\\\{unc}"))
        } else {
            PathBuf::from(value.strip_prefix("\\\\?\\").unwrap_or(&value))
        };
        if let Some(task) = self.task.take() {
            task.cancel();
        }
        self.reader_expected = core::detect(&path).web_reader() && !path.is_dir();
        self.reader_navigated = false;
        self.path = Some(path.clone());
        self.generation += 1;
        self.document = None;
        platform::wheel::image_zoom(false);
        platform::pdf_active(false);
        platform::reader_active(false);
        // Late initialization of the old WebView must not replace the new reader.
        self.member_listener = Default::default();
        self.reader = Default::default();
        self.player = None;
        self.position = 0.0;
        self.rate = 1.0;
        self.duration = 0.0;
        self.loading = true;
        self.error.clear();
        self.page = 0;
        self.pages = 1;
        self.query.clear();
        self.search_open = false;
        self.zoom = 1.0;
        self.raw = false;
        let generation = self.generation;
        self.task = Some(c.spawn_background(move |cancel| {
            Msg::Loaded(
                generation,
                core::load_cancellable(&path, || cancel.is_cancelled()),
            )
        }));
    }
    fn kind(&self) -> Kind {
        self.document
            .as_ref()
            .map(|d| d.kind)
            .unwrap_or(Kind::Unknown)
    }
    fn button(&self, text: &str, msg: Msg, c: &ViewContext<Self>) -> View {
        Button::new().on_click(c.message(msg)).content(text)
    }
    fn file_index(&self) -> usize {
        self.path
            .as_ref()
            .and_then(|p| self.files.iter().position(|f| f == p))
            .unwrap_or(0)
    }
}

impl Component for Preview {
    type Input = Option<PathBuf>;
    type Message = Msg;
    fn create(input: &Self::Input, c: &ComponentContext<Self>) -> Self {
        let mut app = Self {
            path: None,
            files: Vec::new(),
            document: None,
            error: String::new(),
            generation: 0,
            loading: false,
            reader_expected: false,
            reader_navigated: false,
            page: 0,
            pages: 1,
            zoom: 1.0,
            info: false,
            query: String::new(),
            raw: false,
            dark: false,
            keys: None,
            wheel: None,
            player: None,
            position: 0.0,
            duration: 0.0,
            volume: 0.8,
            task: None,
            pinned: false,
            muted: false,
            rate: 1.0,
            search_open: false,
            search_ref: ElementRef::new(),
            initial_focus: ElementRef::new(),
            reader: Default::default(),
            member_listener: Default::default(),
        };
        if let Some(path) = input {
            app.load(path.clone(), c);
        }
        c.spawn_background(|_| {
            std::thread::sleep(std::time::Duration::from_millis(120));
            Msg::Ready
        });
        app
    }
    fn update(&mut self, msg: Msg, c: &ComponentContext<Self>) {
        match msg {
            Msg::Ready => {
                platform::activate_preview();
                platform::set_window_icon();
                let pin_sender = c.sender();
                if let Err(e) = platform::caption::install(move || {
                    pin_sender.send(Msg::CaptionLayout);
                }) {
                    self.error = e;
                }

                let _ = self.initial_focus.request_focus();
                let sender = c.sender();
                match platform::wheel::install(move |delta| {
                    sender.send(Msg::Zoom(delta));
                }) {
                    Ok(wheel) => self.wheel = Some(wheel),
                    Err(e) => self.error = e,
                }
                let sender = c.sender();
                match platform::install_keys(move |key| {
                    sender.send(Msg::Key(key));
                }) {
                    Ok(keys) => self.keys = Some(keys),
                    Err(e) => self.error = e,
                }
            }
            Msg::Pin => {
                self.pinned = !self.pinned;
                platform::pin_preview(self.pinned);
            }
            Msg::Maximize => platform::toggle_maximize(),
            Msg::Mute => {
                self.muted = !self.muted;
                if let Some(p) = &self.player {
                    p.volume(if self.muted { 0.0 } else { self.volume });
                }
            }
            Msg::Rate => {
                let next = if self.rate >= 2.0 {
                    0.5
                } else {
                    self.rate + 0.5
                };
                if let Some(p) = &self.player
                    && p.rate(next).is_ok()
                {
                    self.rate = next;
                }
            }
            Msg::PreviewChild(path) => {
                if let Err(e) = std::env::current_exe().and_then(|exe| {
                    std::process::Command::new(exe)
                        .arg("--preview")
                        .arg(path)
                        .spawn()
                }) {
                    self.error = e.to_string();
                }
            }
            Msg::Browse(path) => self.load(path, c),
            Msg::Open => {
                if let Some(path) = platform::pick_file() {
                    self.load(path, c)
                }
            }
            Msg::FilesLoaded(g, files) if g == self.generation => self.files = files,
            Msg::Loaded(g, result) if g == self.generation => {
                self.loading = false;
                if let Some(path) = self.path.clone() {
                    self.task = Some(c.spawn_background(move |cancel| {
                        Msg::FilesLoaded(
                            g,
                            if cancel.is_cancelled() {
                                Vec::new()
                            } else {
                                core::siblings(&path)
                            },
                        )
                    }));
                }
                match result {
                    Ok(doc) => {
                        self.pages = if doc.kind == Kind::Folder {
                            doc.children.len()
                        } else {
                            doc.text.lines().count()
                        }
                        .div_ceil(core::PAGE_LINES)
                        .max(1);
                        platform::wheel::image_zoom(doc.kind == Kind::Image);
                        self.document = Some(doc);
                        if let Err(e) = self.navigate_reader() {
                            self.error = e;
                        }
                        if self.kind() == Kind::Pdf {
                            platform::pdf_active(true);
                        }
                        platform::reader_active(
                            self.document.as_ref().is_some_and(|d| d.preview.is_some()),
                        );
                        if matches!(self.kind(), Kind::Video | Kind::Audio)
                            && let (Some(path), Some(hwnd)) =
                                (self.path.as_ref(), platform::current_window())
                        {
                            match Player::open(path, hwnd) {
                                Ok(p) => {
                                    p.volume(if self.muted { 0.0 } else { self.volume });
                                    self.player = Some(p);
                                    c.sender().send(Msg::Tick(self.generation));
                                }
                                Err(e) => self.error = e,
                            }
                        }
                    }
                    Err(e) => self.error = e,
                }
            }
            Msg::Next(step) => {
                if !self.files.is_empty() {
                    let index = (self.file_index() as i32 + step)
                        .rem_euclid(self.files.len() as i32)
                        as usize;
                    self.load(self.files[index].clone(), c);
                }
            }
            Msg::ReaderReady(g, result) if g == self.generation => {
                if let Err(e) = result.and_then(|_| self.navigate_reader()) {
                    self.error = e
                }
            }
            Msg::PdfReady(generation, Err(error)) if generation == self.generation => {
                self.error = error;
            }
            Msg::Page(step) => {
                let next = (self.page as i32 + step).clamp(0, self.pages.saturating_sub(1) as i32)
                    as usize;
                if next != self.page {
                    self.page = next;
                }
            }
            Msg::Zoom(delta) => self.zoom = (self.zoom + delta).clamp(0.1, 8.0),
            Msg::Info => self.info = !self.info,
            Msg::Raw if self.kind() == Kind::Markdown => {}
            Msg::Raw => {
                if let Some(reader) = self.reader.borrow().as_ref() {
                    let _ =
                        reader.execute_script("document.getElementById('mode').click()", |_| {});
                } else {
                    self.raw = !self.raw;
                }
            }
            Msg::CaptionLayout => {}
            Msg::Find => {
                if let Some(reader) = self.reader.borrow().as_ref() {
                    self.search_open = true;
                    let _ = reader.execute_script("document.getElementById('find').hidden=false;document.getElementById('find').focus()", |_| {});
                    return;
                }
                if matches!(
                    self.kind(),
                    Kind::Text | Kind::Code | Kind::Json | Kind::Markdown
                ) {
                    self.search_open = true;
                    c.spawn_background(|_| {
                        std::thread::sleep(std::time::Duration::from_millis(35));
                        Msg::FocusSearch
                    });
                }
            }
            Msg::FocusSearch => {
                if self.search_open {
                    let _ = self.search_ref.request_focus();
                }
            }
            Msg::HideSearch => {
                self.search_open = false;
                if let Some(reader) = self.reader.borrow().as_ref() {
                    let _ = reader.execute_script("document.getElementById('find').hidden=true;document.getElementById('find').value='';query='';render()", |_| {});
                }
                c.sender().send(Msg::Search(String::new()));
                let _ = self.initial_focus.request_focus();
            }
            Msg::Search(q) => {
                self.query = q;
                self.page = 0;
                if let Some(doc) = &self.document {
                    let q = self.query.to_lowercase();
                    self.pages = doc
                        .text
                        .lines()
                        .filter(|l| q.is_empty() || l.to_lowercase().contains(&q))
                        .count()
                        .div_ceil(core::PAGE_LINES)
                        .max(1);
                }
            }
            Msg::Close => {
                self.player = None;
                let _ = c.window().request_close();
            }
            Msg::Key(key) => {
                let action = match key {
                    27 if self.search_open => Msg::HideSearch,
                    27 | 32 => Msg::Close,
                    33 => Msg::Page(-1),
                    34 => Msg::Page(1),
                    0x10046 => Msg::Find,
                    0x10049 => Msg::Info,
                    0x10052 => Msg::Raw,
                    0x1004f => Msg::Open,
                    0x1004c => Msg::Reveal,
                    37 => Msg::Next(-1),
                    39 => Msg::Next(1),
                    13 => Msg::External,
                    187 | 107 => Msg::Zoom(0.1),
                    189 | 109 => Msg::Zoom(-0.1),
                    _ => return,
                };
                c.sender().send(action);
            }
            Msg::External => {
                if let Some(p) = &self.path {
                    platform::open_external(p)
                }
            }
            Msg::Reveal => {
                if let Some(p) = &self.path {
                    platform::reveal(p)
                }
            }
            Msg::Error(e) => self.error = e,
            Msg::Theme(t) => {
                self.dark = t == ColorScheme::Dark;
            }
            Msg::Play => {
                if let Some(p) = &mut self.player
                    && let Err(e) = p.toggle()
                {
                    self.error = e
                }
            }
            Msg::Seek(value) => {
                self.position = value;
                if let Some(p) = &self.player
                    && let Err(e) = p.seek(value)
                {
                    self.error = e
                }
            }
            Msg::Volume(value) => {
                self.volume = value;
                self.muted = false;
                if let Some(p) = &self.player {
                    p.volume(value)
                }
            }
            Msg::Tick(generation) if generation == self.generation => {
                if let Some(p) = &self.player {
                    p.resize(self.info);
                    (self.position, self.duration) = p.progress();
                    c.spawn_background(move |cancel| {
                        if !cancel.is_cancelled() {
                            std::thread::sleep(std::time::Duration::from_millis(250));
                        }
                        Msg::Tick(generation)
                    });
                }
            }
            _ => {}
        }
        if !self.error.is_empty() {
            self.player = None;
            platform::pdf_active(false);
        }
        platform::paged_text_active(
            !self.raw
                && self
                    .document
                    .as_ref()
                    .is_some_and(|doc| !doc.lines.is_empty()),
        );
    }
    fn view(&self, _: &Self::Input, c: &mut ViewContext<Self>) -> View {
        c.window_title("");
        c.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Mica)
                .client_size(1040.0, 760.0)
                .constraints(WindowConstraints {
                    min_width: 760.0.into(),
                    min_height: 520.0.into(),
                    ..Default::default()
                }),
        );
        c.on_color_scheme(c.callback(Msg::Theme));
        let ink = if self.dark {
            Color::rgb(234, 238, 247)
        } else {
            Color::rgb(20, 34, 62)
        };
        let muted = if self.dark {
            Color::rgb(159, 174, 200)
        } else {
            Color::rgb(118, 133, 159)
        };
        let surface = if self.dark {
            Color::argb(210, 30, 36, 48)
        } else {
            Color::argb(210, 248, 250, 255)
        };
        let filename = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let pin = Button::new()
            .style(ButtonStyle::Subtle)
            .width(caption::BUTTON_WIDTH as f64)
            .height(caption::HEIGHT as f64)
            .element_ref(&self.initial_focus)
            .automation_name(if self.pinned {
                "取消置顶"
            } else {
                "固定置顶"
            })
            .on_click(c.message(Msg::Pin))
            .content(
                Viewbox::new()
                    .width(14.0)
                    .height(14.0)
                    .slot(ViewboxSlot::Child, SymbolIcon::new().symbol(Symbol::Pin)),
            );
        let pin = Border::new()
            .background(if self.pinned {
                Color::rgb(186, 213, 255)
            } else {
                Color::argb(0, 0, 0, 0)
            })
            .content(pin);
        let titlebar = Grid::new().height(caption::HEIGHT as f64).children((
            // RightHeader selects WinUI's 48-DIP template; shift its center onto
            // the 32-DIP native caption while compensating for its reserved inset.
            TitleBar::new()
                .height(48.0)
                .vertical_alignment(VerticalAlignment::Top)
                .margin(Thickness::new(
                    0.0,
                    -8.0,
                    -platform::caption::right_compensation(),
                    0.0,
                ))
                .slot(TitleBarSlot::RightHeader, pin),
            Image::new()
                .source_data(EncodedImage::new(Arc::<[u8]>::from(
                    include_bytes!("../assets/QuickPeek.ico").as_slice(),
                )))
                .width(caption::ICON_SIZE as f64)
                .height(caption::ICON_SIZE as f64)
                .margin(Thickness::xy(caption::ICON_LEFT as f64, 0.0))
                .horizontal_alignment(HorizontalAlignment::Left)
                .vertical_alignment(VerticalAlignment::Center)
                .automation_name("QuickPeek"),
            TextBlock::new()
                .text(filename)
                .foreground(ink)
                .font_size(caption::TEXT_SIZE as f64)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .margin(Thickness::new(caption::TEXT_LEFT as f64, 0.0, 200.0, 0.0))
                .vertical_alignment(VerticalAlignment::Center),
        ));
        let content = self.content(c, ink, muted);
        let mut panes = vec![KeyedView::new(
            format!("preview-{}", self.generation),
            content,
        )];
        if self.info {
            panes.push(KeyedView::new(
                "inspector",
                self.inspector(ink, muted, surface),
            ));
        }
        let workspace = Grid::new().grid_row(1).keyed_children(panes);
        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((titlebar, workspace))
    }
}

fn button_colors(color: Color) -> ResourceOverrides {
    ResourceOverrides::new()
        .set("ButtonPadding", Thickness::uniform(6.0))
        .set("SubtleButtonPadding", Thickness::uniform(6.0))
        .set("ButtonForeground", color)
        .set("ButtonForegroundPointerOver", color)
        .set("ButtonForegroundPressed", color)
        .set("SubtleButtonForeground", color)
        .set("SubtleButtonForegroundPointerOver", color)
        .set("SubtleButtonForegroundPressed", color)
}

fn row(views: Vec<View>, spacing: f64) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(spacing)
        .keyed_children(
            views
                .into_iter()
                .enumerate()
                .map(|(i, v)| KeyedView::new(i, v)),
        )
}
fn column(views: Vec<View>, spacing: f64) -> View {
    StackPanel::new().spacing(spacing).keyed_children(
        views
            .into_iter()
            .enumerate()
            .map(|(i, v)| KeyedView::new(i, v)),
    )
}

impl Preview {
    fn inspector(&self, ink: Color, muted: Color, surface: Color) -> View {
        let path = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let size = self
            .document
            .as_ref()
            .map(|d| core::size(d.bytes))
            .unwrap_or_default();
        Border::new()
            .width(245.0)
            .padding(18.0)
            .corner_radius(12.0)
            .background(surface)
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Top)
            .margin(8.0)
            .content(column(
                vec![
                    TextBlock::new()
                        .text(self.kind().label())
                        .foreground(ink)
                        .font_size(16.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .into(),
                    TextBlock::new().text(size).foreground(muted).into(),
                    TextBlock::new()
                        .text(path)
                        .foreground(muted)
                        .font_size(12.0)
                        .text_wrapping(TextWrapping::Wrap)
                        .is_text_selection_enabled(true)
                        .into(),
                ],
                10.0,
            ))
    }
    fn navigate_reader(&mut self) -> Result<(), String> {
        if self.reader_navigated {
            return Ok(());
        }
        let Some(doc) = self.document.as_ref() else {
            return Ok(());
        };
        let Some(preview) = doc.preview.as_ref() else {
            return Ok(());
        };
        let reader = self.reader.borrow();
        let Some(webview) = reader.as_ref() else {
            return Ok(());
        };
        if let Some(archive) = self.path.as_ref().filter(|_| doc.kind == Kind::Archive) {
            let origin = url::Url::from_file_path(&preview.0).map_err(|_| "预览路径无效")?;
            let archive = archive.clone();
            let registration = webview
                .on_web_message_received(move |msg| {
                    if url::Url::parse(&msg.source()).ok().as_ref() != Some(&origin) {
                        return;
                    }
                    if let Ok(name) = msg.try_web_message_as_string()
                        && let Ok(exe) = std::env::current_exe()
                    {
                        let _ = std::process::Command::new(exe)
                            .arg("--archive-member")
                            .arg(&archive)
                            .arg(name)
                            .spawn();
                    }
                })
                .map_err(|e| e.to_string())?;
            *self.member_listener.borrow_mut() = Some(registration);
        }
        platform::pdf::open(webview, &preview.0)?;

        self.reader_navigated = true;
        Ok(())
    }
    fn content(&self, c: &ViewContext<Self>, ink: Color, muted: Color) -> View {
        if !self.error.is_empty() {
            return self.center(&self.error, ink);
        }
        if (self.loading && self.reader_expected)
            || self.document.as_ref().is_some_and(|d| d.preview.is_some())
        {
            let reader = self.reader.clone();
            let generation = self.generation;
            return windows_webview::webview_result(c.callback(
                move |result: Result<windows_webview::WebView, IntegrationError>| {
                    let result = result
                        .map(|view| *reader.borrow_mut() = Some(view))
                        .map_err(|e| format!("阅读器初始化失败：{e:?}"));
                    Msg::ReaderReady(generation, result)
                },
            ));
        }
        if self.loading {
            return self.center("正在加载…", muted);
        }
        let Some(doc) = &self.document else {
            return Border::new()
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .content(self.button("打开文件…", Msg::Open, c));
        };
        match doc.kind {
            Kind::Folder => {
                let mut entries = Vec::new();
                let mut navigation = Vec::new();
                if let Some(parent) = self.path.as_deref().and_then(|p| p.parent()) {
                    navigation.push(self.button("↑ 上一级", Msg::Browse(parent.into()), c));
                }
                if self.pages > 1 {
                    if self.page > 0 {
                        navigation.push(self.button("上一页", Msg::Page(-1), c));
                    }
                    navigation.push(
                        TextBlock::new()
                            .text(format!("{} / {}", self.page + 1, self.pages))
                            .into(),
                    );
                    if self.page + 1 < self.pages {
                        navigation.push(self.button("下一页", Msg::Page(1), c));
                    }
                }
                entries.push(row(navigation, 8.));
                if doc.children.is_empty() {
                    entries.push(
                        TextBlock::new()
                            .text("此文件夹为空")
                            .foreground(muted)
                            .into(),
                    );
                }
                for entry in doc
                    .children
                    .iter()
                    .skip(self.page * core::PAGE_LINES)
                    .take(core::PAGE_LINES)
                {
                    let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
                    entries.push(
                        Button::new()
                            .style(ButtonStyle::Subtle)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .automation_name(name.as_ref())
                            .on_click(c.message(if entry.directory {
                                Msg::Browse(entry.path.clone())
                            } else {
                                Msg::PreviewChild(entry.path.clone())
                            }))
                            .content(row(
                                vec![
                                    SymbolIcon::new()
                                        .symbol(if entry.directory {
                                            Symbol::Folder
                                        } else {
                                            Symbol::Document
                                        })
                                        .into(),
                                    TextBlock::new()
                                        .text(name.as_ref())
                                        .text_trimming(TextTrimming::CharacterEllipsis)
                                        .into(),
                                ],
                                10.,
                            )),
                    );
                }
                ScrollViewer::new()
                    .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
                    .content(Border::new().padding(12.).content(column(entries, 3.)))
            }
            Kind::Pdf => {
                let path = self.path.as_ref().unwrap().clone();
                let generation = self.generation;
                windows_webview::webview_result(c.callback(move |result: Result<windows_webview::WebView, IntegrationError>| {
                    let result = result.map_err(|error| format!("PDF 阅读器初始化失败，请检查 Microsoft Edge WebView2 Runtime：{error:?}"))
                        .and_then(|webview| platform::pdf::open(&webview, &path));
                    Msg::PdfReady(generation, result)
                }))
            }
            Kind::Image => {
                let Some((bytes, width, height)) = &doc.image else {
                    return self.center("图片无法解码", ink);
                };
                let image = Image::new().source_data(EncodedImage::new(bytes.clone()));
                let available_width = platform::content_width(self.info);
                let available_height = platform::content_height();
                let scale = (available_width / *width as f64)
                    .min(available_height / *height as f64)
                    .min(1.0);
                let image = image
                    .stretch(Stretch::Uniform)
                    .automation_name(format!("图片 · {:.0}%", scale * self.zoom * 100.0))
                    .on_failed(c.message(Msg::Error(
                        "图片无法解码，文件可能损坏或当前系统不支持此格式".into(),
                    )));
                ScrollViewer::new()
                    .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)
                    .content(
                        image
                            .width(*width as f64 * scale * self.zoom)
                            .height(*height as f64 * scale * self.zoom),
                    )
            }
            Kind::Video | Kind::Audio => {
                let playing = self.player.as_ref().is_some_and(|p| !p.paused);
                let white = Color::rgb(235, 238, 245);
                let icon = |symbol, label: &str, msg| {
                    Button::new()
                        .style(ButtonStyle::Subtle)
                        .width(48.0)
                        .height(36.0)
                        .resource_overrides(button_colors(white))
                        .automation_name(label)
                        .on_click(c.message(msg))
                        .content(SymbolIcon::new().symbol(symbol))
                        .tooltip(label)
                };
                let compact = platform::content_width(false) < 520.0;
                let controls = Grid::new()
                    .rows(if compact {
                        vec![GridLength::Auto, GridLength::Auto]
                    } else {
                        vec![GridLength::Auto]
                    })
                    .columns([
                        GridLength::Auto,
                        GridLength::Auto,
                        GridLength::STAR,
                        GridLength::Auto,
                    ])
                    .children((
                        icon(
                            if playing { Symbol::Pause } else { Symbol::Play },
                            if playing { "暂停" } else { "播放" },
                            Msg::Play,
                        ),
                        TextBlock::new()
                            .grid_column(1)
                            .margin(Thickness::xy(12.0, 0.0))
                            .text(format!("{} / {}", time(self.position), time(self.duration)))
                            .foreground(white)
                            .font_size(12.0)
                            .vertical_alignment(VerticalAlignment::Center),
                        Slider::new()
                            .grid_column(2)
                            .margin(Thickness::xy(10.0, 0.0))
                            .minimum(0.0)
                            .maximum(self.duration.max(1.0))
                            .value(self.position)
                            .automation_name("播放进度")
                            .on_value_changed(c.callback(Msg::Seek)),
                        Grid::new()
                            .grid_row(if compact { 1 } else { 0 })
                            .grid_column(if compact { 0 } else { 3 })
                            .grid_column_span(if compact { 4 } else { 1 })
                            .children((row(
                                vec![
                                    icon(
                                        if self.muted || self.volume == 0.0 {
                                            Symbol::Mute
                                        } else {
                                            Symbol::Volume
                                        },
                                        if self.muted { "取消静音" } else { "静音" },
                                        Msg::Mute,
                                    ),
                                    Slider::new()
                                        .width(76.0)
                                        .minimum(0.0)
                                        .maximum(1.0)
                                        .value(if self.muted { 0.0 } else { self.volume })
                                        .automation_name("音量")
                                        .on_value_changed(c.callback(Msg::Volume))
                                        .into(),
                                    Button::new()
                                        .style(ButtonStyle::Subtle)
                                        .width(50.0)
                                        .resource_overrides(button_colors(white))
                                        .automation_name("播放速度")
                                        .on_click(c.message(Msg::Rate))
                                        .content(format!("{:.1}×", self.rate)),
                                    icon(Symbol::FullScreen, "最大化或还原", Msg::Maximize),
                                ],
                                4.0,
                            ),)),
                    ));
                Border::new().background(Color::rgb(12, 13, 16)).content(
                    Border::new()
                        .vertical_alignment(VerticalAlignment::Bottom)
                        .margin(Thickness::xy(24.0, 18.0))
                        .padding(Thickness::xy(12.0, 8.0))
                        .corner_radius(12.0)
                        .background(Color::rgb(36, 38, 44))
                        .content(controls),
                )
            }
            Kind::Unknown => self.center("暂不支持此文件类型 · 按 Enter 使用默认应用打开", muted),
            _ => self.text_content(doc, c, ink, muted),
        }
    }
    fn center(&self, text: &str, color: Color) -> View {
        TextBlock::new()
            .text(text)
            .font_size(16.0)
            .foreground(color)
            .text_wrapping(TextWrapping::Wrap)
            .max_width(650.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .into()
    }
    fn text_content(
        &self,
        doc: &Document,
        c: &ViewContext<Self>,
        ink: Color,
        muted: Color,
    ) -> View {
        let mut blocks = Vec::new();
        if !doc.notice.is_empty() {
            blocks.push(
                TextBlock::new()
                    .text(&doc.notice)
                    .foreground(muted)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            );
        }
        let query = self.query.to_lowercase();
        if doc.kind == Kind::Markdown {
            blocks.extend(markdown(&doc.text, self.zoom, ink, self.path.as_deref()));
        } else if doc.lines.is_empty() || self.raw {
            let text = doc
                .text
                .lines()
                .filter(|line| query.is_empty() || line.to_lowercase().contains(&query))
                .collect::<Vec<_>>()
                .join("\n");
            blocks.push(
                TextBlock::new()
                    .text(text)
                    .font_size(15.0 * self.zoom)
                    .foreground(ink)
                    .is_text_selection_enabled(true)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            );
        } else {
            let start = self.page * core::PAGE_LINES;
            for (i, line) in doc
                .text
                .lines()
                .enumerate()
                .filter(|(_, l)| query.is_empty() || l.to_lowercase().contains(&query))
                .skip(start)
                .take(core::PAGE_LINES)
            {
                if !doc.lines.is_empty() && i < doc.lines.len() {
                    let mut tokens = vec![
                        TextBlock::new()
                            .text(format!("{:>4}  ", i + 1))
                            .foreground(muted)
                            .font_size(13.0 * self.zoom)
                            .width(50.0 * self.zoom)
                            .into(),
                    ];
                    tokens.extend(doc.lines[i].iter().map(|s| {
                        let color = if self.dark {
                            Color::rgb(
                                s.color[0].saturating_add(90),
                                s.color[1].saturating_add(90),
                                s.color[2].saturating_add(90),
                            )
                        } else {
                            Color::rgb(s.color[0], s.color[1], s.color[2])
                        };
                        TextBlock::new()
                            .text(s.text.replace(' ', "\u{a0}"))
                            .foreground(color)
                            .font_size(14.0 * self.zoom)
                            .is_text_selection_enabled(true)
                            .into()
                    }));
                    blocks.push(row(tokens, 0.0));
                } else {
                    let shown = if line.len() > 16_000 {
                        format!(
                            "{} … [长行已截断]",
                            line.chars().take(4000).collect::<String>()
                        )
                    } else if line.is_empty() {
                        " ".into()
                    } else {
                        line.into()
                    };
                    blocks.push(
                        TextBlock::new()
                            .text(shown)
                            .font_size(15.0 * self.zoom)
                            .foreground(ink)
                            .is_text_selection_enabled(true)
                            .text_wrapping(TextWrapping::Wrap)
                            .into(),
                    );
                }
            }
        }
        let search = TextBox::new()
            .element_ref(&self.search_ref)
            .automation_name("在文档中搜索")
            .placeholder_text("在文档中搜索…")
            .text(&self.query)
            .on_text_changed(c.callback(Msg::Search));
        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((
                if self.search_open {
                    Border::new()
                        .margin(Thickness::new(8.0, 0.0, 8.0, 12.0))
                        .content(search)
                } else {
                    Border::new().height(0.0).content(TextBlock::new().text(""))
                },
                ScrollViewer::new()
                    .grid_row(1)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)
                    .content(
                        Border::new()
                            .padding(24.0)
                            .corner_radius(8.0)
                            .background(if self.dark {
                                Color::rgb(28, 32, 41)
                            } else {
                                Color::rgb(255, 255, 255)
                            })
                            .content(column(
                                blocks,
                                if doc.kind == Kind::Markdown && !self.raw {
                                    14.0
                                } else {
                                    3.0
                                },
                            )),
                    ),
            ))
    }
}

fn time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn markdown(text: &str, zoom: f64, ink: Color, path: Option<&std::path::Path>) -> Vec<View> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let mut blocks = Vec::new();
    let mut inlines = Vec::new();
    let mut heading = 0u8;
    let mut bold = false;
    let mut italic = false;
    let mut code = false;
    let mut link: Option<(String, String)> = None;
    let mut image: Option<(String, String)> = None;
    let flush =
        |blocks: &mut Vec<View>, inlines: &mut Vec<RichTextInline>, heading: u8, code: bool| {
            if inlines.is_empty() {
                return;
            }
            if heading > 0 {
                let text = inlines
                    .iter()
                    .filter_map(|r| {
                        if let RichTextInline::Run(r) = r {
                            Some(r.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<String>();
                blocks.push(
                    TextBlock::new()
                        .text(text)
                        .font_size(
                            (if heading == 1 {
                                30.0
                            } else if heading == 2 {
                                22.0
                            } else {
                                18.0
                            }) * zoom,
                        )
                        .font_weight(FontWeight::SEMI_BOLD)
                        .is_text_selection_enabled(true)
                        .foreground(ink)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                );
                inlines.clear();
            } else {
                let rich = RichTextBlock::new()
                    .paragraphs(RichText::new([RichTextParagraph::new(std::mem::take(
                        inlines,
                    ))]))
                    .font_size(if code { 14.0 * zoom } else { 16.0 * zoom })
                    .is_text_selection_enabled(true)
                    .text_wrapping(TextWrapping::Wrap);
                blocks.push(if code {
                    Border::new()
                        .padding(16.0)
                        .corner_radius(8.0)
                        .background(Color::argb(35, 120, 140, 170))
                        .content(rich)
                } else {
                    rich.into()
                });
            }
        };
    for event in Parser::new_ext(
        text,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
    ) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some((dest_url.to_string(), String::new()))
            }
            Event::End(TagEnd::Link) => {
                if let Some((uri, text)) = link.take() {
                    if uri.starts_with("https://")
                        || uri.starts_with("http://")
                        || uri.starts_with("mailto:")
                    {
                        inlines.push(RichTextInline::Hyperlink(RichTextHyperlink { text, uri }));
                    } else {
                        inlines.push(RichTextInline::Run(RichTextRun::plain(text)));
                    }
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                flush(&mut blocks, &mut inlines, heading, code);
                image = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Image) => {
                if let Some((src, alt)) = image.take() {
                    let local = path.and_then(|p| p.parent()).map(|p| p.join(&src));
                    if !src.contains(':')
                        && !src.starts_with(['/', '\\'])
                        && local.as_ref().is_some_and(|p| p.is_file())
                    {
                        if let Ok(img) = Image::new().source_file(local.unwrap()) {
                            blocks.push(
                                img.stretch(Stretch::Uniform)
                                    .max_height(400.0 * zoom)
                                    .into(),
                            );
                        }
                    } else {
                        blocks.push(
                            TextBlock::new()
                                .text(format!("▧  {alt}  ·  外部图片未加载"))
                                .foreground(ink)
                                .into(),
                        );
                    }
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut blocks, &mut inlines, heading, code);
                heading = level as u8;
            }
            Event::End(TagEnd::Heading(_)) => {
                flush(&mut blocks, &mut inlines, heading, code);
                heading = 0;
            }
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut blocks, &mut inlines, heading, code);
                code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush(&mut blocks, &mut inlines, heading, code);
                code = false;
            }
            Event::Start(Tag::Item) => inlines.push(RichTextInline::Run(RichTextRun {
                text: "•  ".into(),
                is_bold: false,
                is_italic: false,
            })),
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, alt)) = &mut image {
                    alt.push_str(&t);
                } else if let Some((_, text)) = &mut link {
                    text.push_str(&t);
                } else {
                    inlines.push(RichTextInline::Run(RichTextRun {
                        text: t.to_string(),
                        is_bold: bold,
                        is_italic: italic,
                    }));
                }
            }
            Event::SoftBreak | Event::HardBreak => inlines.push(RichTextInline::LineBreak),
            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::TableRow) => flush(&mut blocks, &mut inlines, heading, code),
            Event::End(TagEnd::TableCell) => inlines.push(RichTextInline::Run(RichTextRun {
                text: "    │    ".into(),
                is_bold: false,
                is_italic: false,
            })),
            Event::TaskListMarker(done) => inlines.push(RichTextInline::Run(RichTextRun {
                text: if done { "☑ " } else { "☐ " }.into(),
                is_bold: false,
                is_italic: false,
            })),
            _ => {}
        }
    }
    flush(&mut blocks, &mut inlines, heading, code);
    blocks
}
