use gpui::{
    App, Application, Bounds, Context, FocusHandle, KeyDownEvent, ListAlignment, ListState,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, img, list, prelude::*, px, rgb,
    size,
};
use quickpeek::core::{self, Kind};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn supports(path: &Path) -> bool {
    if path.is_dir() {
        return false;
    }
    matches!(core::detect(path), Kind::Text | Kind::Code | Kind::Json)
        || path.extension().is_some_and(|ext| {
            ["jpg", "jpeg", "png", "webp", "bmp", "gif"]
                .iter()
                .any(|s| ext.eq_ignore_ascii_case(s))
        })
}

use super::platform::caption::{self, HEIGHT as TITLE_HEIGHT};

struct Picture {
    width: f32,
    height: f32,
    zoom: f32,
    center: (f32, f32),
    drag: Option<gpui::Point<gpui::Pixels>>,
}

impl Picture {
    fn rect(&self, window: &Window) -> (f32, f32, f32, f32) {
        let viewport = window.viewport_size();
        let vw = f32::from(viewport.width);
        let vh = (f32::from(viewport.height) - TITLE_HEIGHT).max(1.);
        let scale = (vw / self.width).min(vh / self.height) * self.zoom;
        let w = self.width * scale;
        let h = self.height * scale;
        let axis = |v: f32, s: f32, c: f32| {
            if s <= v {
                (v - s) / 2.
            } else {
                (v / 2. - s * c).clamp(v - s, 0.)
            }
        };
        (axis(vw, w, self.center.0), axis(vh, h, self.center.1), w, h)
    }
    fn zoom_at(&mut self, window: &Window, at: gpui::Point<gpui::Pixels>, factor: f32) {
        let (x, y, w, h) = self.rect(window);
        if w <= 0. || h <= 0. {
            return;
        }
        let next = (self.zoom * factor).clamp(0.1, 16.);
        let ratio = next / self.zoom;
        let (mx, my) = (f32::from(at.x), f32::from(at.y) - TITLE_HEIGHT);
        let viewport = window.viewport_size();
        self.center = (
            (mx - x) / w + (f32::from(viewport.width) / 2. - mx) / (w * ratio),
            (my - y) / h + ((f32::from(viewport.height) - TITLE_HEIGHT) / 2. - my) / (h * ratio),
        );
        self.zoom = next;
    }
}
struct Reader {
    path: PathBuf,
    focus: FocusHandle,
    image: Option<Picture>,
    pinned: bool,
    text: String,
    starts: Vec<usize>,
    rows: ListState,
    notice: String,
    thumb_grab: Option<f32>,
}

impl Render for Reader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if let Some(pic) = &self.image {
            let (x, y, w, h) = pic.rect(window);
            div()
                .relative()
                .size_full()
                .overflow_hidden()
                .on_scroll_wheel(cx.listener(|this, e: &gpui::ScrollWheelEvent, window, cx| {
                    if let Some(pic) = &mut this.image {
                        let delta = f32::from(e.delta.pixel_delta(px(16.)).y);
                        pic.zoom_at(window, e.position, (delta * 0.006).clamp(-2., 2.).exp());
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, e: &gpui::MouseDownEvent, _, _| {
                        if let Some(pic) = &mut this.image {
                            pic.drag = Some(e.position);
                        }
                    }),
                )
                .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, window, cx| {
                    if let Some(pic) = &mut this.image {
                        if e.pressed_button != Some(gpui::MouseButton::Left) {
                            pic.drag = None;
                            return;
                        }
                        if let Some(prev) = pic.drag.replace(e.position) {
                            let (x, y, w, h) = pic.rect(window);
                            if w > 0. && h > 0. {
                                let viewport = window.viewport_size();
                                pic.center = (
                                    (f32::from(viewport.width) / 2.
                                        - x
                                        - f32::from(e.position.x - prev.x))
                                        / w,
                                    ((f32::from(viewport.height) - TITLE_HEIGHT) / 2.
                                        - y
                                        - f32::from(e.position.y - prev.y))
                                        / h,
                                );
                                cx.notify();
                            }
                        }
                    }
                }))
                .child(
                    img(self.path.clone())
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(w))
                        .h(px(h)),
                )
                .into_any_element()
        } else {
            let reader = cx.entity();
            let lines = list(self.rows.clone(), move |i, _, cx| {
                let doc = reader.read(cx);
                let end = doc.starts.get(i + 1).copied().unwrap_or(doc.text.len());
                let line = doc.text[doc.starts[i]..end].trim_end_matches(['\r', '\n']);
                div()
                    .w_full()
                    .min_h(px(23.))
                    .line_height(px(23.))
                    .px_4()
                    .child(line.to_owned())
                    .into_any_element()
            })
            .size_full();
            div()
                .relative()
                .size_full()
                .child(lines)
                .child(self.scrollbar(cx))
                .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x19232c))
            .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
                if e.pressed_button != Some(gpui::MouseButton::Left) {
                    this.thumb_grab = None;
                }
                if let Some(grab) = this.thumb_grab {
                    this.scroll_thumb(e.position.y, grab);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.thumb_grab = None),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(TITLE_HEIGHT))
                    .bg(rgb(caption::LIGHT_BACKGROUND))
                    .flex_shrink_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .pl(px(caption::ICON_LEFT))
                            .gap(px(caption::TEXT_LEFT
                                - caption::ICON_LEFT
                                - caption::ICON_SIZE))
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .window_control_area(gpui::WindowControlArea::Drag)
                            .child({
                                static ICON: std::sync::LazyLock<std::sync::Arc<gpui::Image>> =
                                    std::sync::LazyLock::new(|| {
                                        std::sync::Arc::new(gpui::Image::from_bytes(
                                            gpui::ImageFormat::Png,
                                            include_bytes!("../assets/caption-icon.png").to_vec(),
                                        ))
                                    });
                                img(ICON.clone())
                                    .size(px(caption::ICON_SIZE))
                                    .flex_shrink_0()
                            })
                            .child(
                                div()
                                    .truncate()
                                    .font_family("Segoe UI")
                                    .text_size(px(caption::TEXT_SIZE))
                                    .child(
                                        self.path
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned(),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("pin")
                            .flex_shrink_0()
                            .w(px(caption::BUTTON_WIDTH))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .font_family("Segoe Fluent Icons")
                            .text_size(px(14.))
                            .bg(rgb(if self.pinned {
                                0xbad5ff
                            } else {
                                caption::LIGHT_BACKGROUND
                            }))
                            .hover({
                                let pinned = self.pinned;
                                move |v| v.bg(rgb(if pinned { 0xa8c8ff } else { 0xeaecef }))
                            })
                            .text_color(rgb(if self.pinned { 0x145dab } else { 0x445365 }))
                            .child("\u{e840}")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pinned = !this.pinned;
                                super::platform::pin_preview(this.pinned);
                                cx.notify();
                            })),
                    )
                    .children(
                        [
                            (gpui::WindowControlArea::Min, "\u{e921}"),
                            (
                                gpui::WindowControlArea::Max,
                                if window.is_maximized() {
                                    "\u{e923}"
                                } else {
                                    "\u{e922}"
                                },
                            ),
                            (gpui::WindowControlArea::Close, "\u{e8bb}"),
                        ]
                        .into_iter()
                        .map(|(area, icon)| {
                            let is_close = matches!(area, gpui::WindowControlArea::Close);
                            div()
                                .id(icon)
                                .w(px(caption::BUTTON_WIDTH))
                                .h_full()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .window_control_area(area)
                                .hover(move |v| {
                                    if is_close {
                                        v.bg(rgb(0xe81123)).text_color(rgb(0xffffff))
                                    } else {
                                        v.bg(rgb(0xeaecef))
                                    }
                                })
                                .font_family("Segoe MDL2 Assets")
                                .text_size(px(10.))
                                .child(icon)
                        }),
                    ),
            )
            .when(!self.notice.is_empty(), |view| {
                view.child(div().px_4().py_2().child(self.notice.clone()))
            })
            .relative()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                        match e.keystroke.key.as_str() {
                            "escape" | "space" if !e.is_held => window.remove_window(),
                            "pagedown" | "pageup" if this.image.is_none() => {
                                let height = this.rows.viewport_bounds().size.height;
                                this.rows.scroll_by(if e.keystroke.key == "pagedown" {
                                    height
                                } else {
                                    -height
                                });
                                cx.notify();
                            }
                            "enter" => this.classic(cx),
                            _ => {}
                        }
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(|this, _, _, cx| {
                            if super::platform::classic_menu() {
                                this.classic(cx);
                            }
                        }),
                    )
                    .child(body),
            )
    }
}

// Express the viewport as fractional rows so unmeasured lines can stay virtual.
fn thumb(rows: &ListState) -> Option<(f32, f32, f32)> {
    let bounds = rows.viewport_bounds();
    let h = f32::from(bounds.size.height);
    let count = rows.item_count();
    let offset = rows.logical_scroll_top();
    let first = offset.item_ix;
    let first_bounds = rows.bounds_for_item(first)?;
    let start = first as f32
        + f32::from(offset.offset_in_item) / f32::from(first_bounds.size.height).max(1.);
    let mut end = start;
    for i in first..count {
        let Some(row) = rows.bounds_for_item(i) else {
            break;
        };
        let visible = (f32::from(bounds.bottom() - row.top()) / f32::from(row.size.height).max(1.))
            .clamp(0., 1.);
        end = i as f32 + visible;
        if visible < 1. {
            break;
        }
    }
    let range = (count as f32 - (end - start)).max(0.);
    if range < 0.0001 || h <= 1. {
        return None;
    }
    let length = (h * (end - start) / count as f32).max(24.).min(h);
    Some(((h - length) * (start / range).clamp(0., 1.), length, range))
}

impl Reader {
    fn scroll_thumb(&self, y: gpui::Pixels, grab: f32) {
        let Some((_, length, range)) = thumb(&self.rows) else {
            return;
        };
        let bounds = self.rows.viewport_bounds();
        let track = (f32::from(bounds.size.height) - length).max(1.);
        let position = ((f32::from(y - bounds.top()) - grab) / track).clamp(0., 1.);
        let row = position * range;
        let i = row.floor() as usize;
        let height = self
            .rows
            .bounds_for_item(i)
            .map_or(px(23.), |b| b.size.height);
        self.rows.scroll_to(gpui::ListOffset {
            item_ix: if position == 1. {
                self.rows.item_count()
            } else {
                i
            },
            offset_in_item: height * row.fract(),
        });
    }

    fn scrollbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        div()
            .absolute()
            .right_0()
            .top_0()
            .bottom_0()
            .w(px(12.))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                    if let Some((top, length, _)) = thumb(&this.rows) {
                        let y = f32::from(e.position.y - this.rows.viewport_bounds().top());
                        let grab = if y >= top && y <= top + length {
                            y - top
                        } else {
                            length / 2.
                        };
                        this.thumb_grab = Some(grab);
                        this.scroll_thumb(e.position.y, grab);
                        cx.notify();
                    }
                }),
            )
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if let Some((top, length, _)) = thumb(&rows) {
                            let rect = Bounds::new(
                                gpui::point(bounds.left() + px(3.), bounds.top() + px(top)),
                                size(px(6.), px(length)),
                            );
                            window.paint_quad(gpui::fill(rect, rgb(0x87929e)).corner_radii(px(3.)));
                        }
                    },
                )
                .size_full(),
            )
    }
}
impl Reader {
    fn classic(&mut self, cx: &mut Context<Self>) {
        match std::env::current_exe()
            .and_then(|exe| Command::new(exe).arg("--classic").arg(&self.path).spawn())
        {
            Ok(_) => cx.quit(),
            Err(e) => {
                self.notice = e.to_string();
                cx.notify();
            }
        }
    }
}

fn open(cx: &mut App, path: PathBuf) -> Result<(), String> {
    super::platform::trace("reader received");
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let kind = core::detect(&path);
    let image = if kind == Kind::Image {
        let (width, height) = image::image_dimensions(&path).map_err(|e| e.to_string())?;
        Some(Picture {
            width: width as f32,
            height: height as f32,
            zoom: 1.,
            center: (0.5, 0.5),
            drag: None,
        })
    } else {
        None
    };
    let (text, notice) = if kind == Kind::Image {
        (String::new(), String::new())
    } else {
        let doc = core::load_for_gpui(&path)?;
        (doc.text, doc.notice)
    };
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    let title = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    super::platform::prepare_preview();
    let bounds = Bounds::centered(None, size(px(920.), px(640.)), cx);
    // Start decoding while Windows creates the swap chain. The view and this
    // task share GPUI's loader, including animated formats.
    let image_resource = image
        .as_ref()
        .map(|_| gpui::Resource::Path(path.clone().into()));
    let decoding = image_resource
        .as_ref()
        .map(|resource| cx.fetch_asset::<gpui::ImgResourceLoader>(resource).0);
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: false,
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            cx.new(|cx| {
                if image.is_some() {
                    let asset = gpui::ImageSource::from(path.clone());
                    cx.on_release(move |_, cx| asset.remove_asset(cx)).detach();
                }
                let focus = cx.focus_handle();
                window.focus(&focus);
                // The broker has granted foreground permission. GPUI's Windows
                // activation also synthesizes Alt, which can redirect focus to Explorer.
                super::platform::present_reader();
                Reader {
                    path,
                    focus,
                    image,
                    pinned: false,
                    thumb_grab: None,
                    text,
                    rows: ListState::new(starts.len(), ListAlignment::Top, px(160.)),
                    starts,
                    notice,
                }
            })
        },
    );
    match result {
        Ok(window) => {
            if let Some(decoding) = decoding {
                cx.spawn(async move |cx| {
                    let _ = decoding.await;
                    let _ = window.update(cx, |_, window, _| window.refresh());
                })
                .detach();
            }
            Ok(())
        }
        Err(err) => {
            if let Some(resource) = image_resource {
                cx.remove_asset::<gpui::ImgResourceLoader>(&resource);
            }
            Err(err.to_string())
        }
    }
}

fn open_or_quit(cx: &mut App, path: PathBuf) {
    if let Err(e) = open(cx, path) {
        super::platform::show_error(&e);
        cx.quit();
    }
}

pub fn run(path: PathBuf) -> Result<(), String> {
    Application::new().run(move |cx| open_or_quit(cx, path));
    Ok(())
}

fn next_path() -> Option<PathBuf> {
    use std::{io::Read, os::windows::ffi::OsStringExt};
    let mut pipe = std::io::stdin().lock();
    let mut length = [0; 4];
    pipe.read_exact(&mut length).ok()?;
    let n = u32::from_le_bytes(length) as usize;
    if n == 0 || n > 65536 || !n.is_multiple_of(2) {
        return None;
    }
    let mut bytes = vec![0; n];
    pipe.read_exact(&mut bytes).ok()?;
    let wide = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect::<Vec<_>>();
    Some(PathBuf::from(std::ffi::OsString::from_wide(&wide)))
}

pub fn standby() {
    let app = Application::new();
    let Some(first) = next_path() else { return };
    app.run(move |cx| {
        // GPUI's Windows backend exits its message loop when its last window closes.
        let anchor = cx.open_window(
            WindowOptions {
                show: false,
                focus: false,
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1.), px(1.)),
                    cx,
                ))),
                ..Default::default()
            },
            |_, cx| cx.new(|_| gpui::EmptyView),
        );
        let Ok(anchor) = anchor else {
            open_or_quit(cx, first);
            return;
        };
        if let Err(e) = open(cx, first) {
            super::platform::show_error(&e)
        }
        cx.spawn(async move |cx| {
            loop {
                let path = cx.background_executor().spawn(async { next_path() }).await;
                let Some(path) = path else { break };
                if cx
                    .update(|cx| {
                        if cx.windows().len() == 1 {
                            if let Err(e) = open(cx, path) {
                                super::platform::show_error(&e)
                            }
                        } else if let Ok(exe) = std::env::current_exe()
                            && let Ok(child) = Command::new(exe).arg("--preview").arg(path).spawn()
                        {
                            super::platform::grant_foreground(child.id());
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
            let _ = anchor.update(cx, |_, window, _| window.remove_window());
        })
        .detach();
    });
}
