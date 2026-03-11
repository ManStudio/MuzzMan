use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    convert::Infallible,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::{Arc, atomic::AtomicU64},
    time::{Duration, Instant},
};

use blobsman_graphics::{
    futures_util::{self, StreamExt},
    gpui::{
        self, AnyElement, AssetSource, ClipboardItem, Div, EventEmitter, FutureExt,
        PathPromptOptions, Rgba, ScrollHandle, TitlebarOptions, WindowDecorations, anchored,
        deferred, svg,
    },
    gpui_platform, gpui_tokio,
    iroh::{
        self, EndpointAddr, PublicKey, Watcher,
        endpoint::{ConnectionInfo, PathInfoList},
    },
    iroh_blobs::{
        self, Hash, HashAndFormat,
        api::{
            blobs::{AddPathOptions, AddProgressItem},
            downloader::DownloadProgressItem,
        },
        format::collection::CollectionMeta,
        hashseq::HashSeq,
        ticket::BlobTicket,
    },
    tokio::{self, io::AsyncReadExt, sync::mpsc::Sender},
};
use gpui::{
    App, AppContext, Bounds, CursorStyle, ElementId, Entity, ExternalPaths, FocusHandle,
    GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, ParentElement, Pixels,
    Point, Render, SharedString, Style, Styled, Window, WindowOptions, div, prelude::*, px, size,
};
use serde::{Deserialize, Serialize};

use crate::text_input::{
    Backspace, Copy, Cut, Delete, End, Home, Left, Paste, Right, SelectAll, SelectLeft,
    SelectRight, Submit, TextInput,
};

mod text_input;

const SVG_ARROW_RIGHT: &str =
    include_str!("../assets/arrow_right_alt_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_ARROW_DOWNWARD: &str =
    include_str!("../assets/arrow_downward_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_CLOSE: &str = include_str!("../assets/close_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_SHARE: &str = include_str!("../assets/share_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_DOWNLOAD: &str =
    include_str!("../assets/download_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_UPLOAD: &str =
    include_str!("../assets/upload_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");

pub struct Assets {}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match path {
            "arrow_right" => Ok(Some(std::borrow::Cow::Borrowed(SVG_ARROW_RIGHT.as_bytes()))),
            "arrow_downward" => Ok(Some(std::borrow::Cow::Borrowed(
                SVG_ARROW_DOWNWARD.as_bytes(),
            ))),
            "close" => Ok(Some(std::borrow::Cow::Borrowed(SVG_CLOSE.as_bytes()))),
            "share" => Ok(Some(std::borrow::Cow::Borrowed(SVG_SHARE.as_bytes()))),
            "download" => Ok(Some(std::borrow::Cow::Borrowed(SVG_DOWNLOAD.as_bytes()))),
            // TODO: I need a better export icon
            "export" => Ok(Some(std::borrow::Cow::Borrowed(SVG_UPLOAD.as_bytes()))),
            _ => Ok(None),
        }
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

const fn rgba(hex: u32) -> Rgba {
    let [a, b, g, r] = hex.to_le_bytes();
    Rgba {
        r: (r as f32) / 255.,
        g: (g as f32) / 255.,
        b: (b as f32) / 255.,
        a: (a as f32) / 255.,
    }
}

const VERSION: &str = "V0.1.0";

const BLACK_1: Rgba = rgba(0x000000ff);
const BLACK_2: Rgba = rgba(0x212121ff);
const BLACK_3: Rgba = rgba(0x3a3a3aff);
const BLACK_4: Rgba = rgba(0x484848ff);
const BLACK_5: Rgba = rgba(0x505050ff);
const DOWNLOAD: Rgba = rgba(0x2f7f1fff);
const DOWNLOADING: Rgba = rgba(0x3f992dff);
const PING: Rgba = rgba(0x43cc00ff);
const UPLOAD: Rgba = rgba(0x2324b2ff);
const UPLOADING: Rgba = rgba(0x1e61ccff);
const WHITE: Rgba = rgba(0xffffffff);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);
pub fn next_connection_id() -> u64 {
    NEXT_CONNECTION_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

#[derive(Clone)]
pub struct TrackBounds {
    bounds: Entity<Bounds<Pixels>>,
}

impl Render for TrackBounds {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.clone()
    }
}

impl IntoElement for TrackBounds {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TrackBounds {
    type RequestLayoutState = ();

    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (
            window.request_layout(
                Style {
                    size: size(px(0.).into(), px(0.).into()),
                    ..Default::default()
                },
                vec![],
                cx,
            ),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.bounds.write(cx, bounds);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        cx: &mut App,
    ) {
        assert_eq!(*self.bounds.read(cx), bounds);
    }
}

#[derive(Clone)]
pub enum Entry {
    Connections(Entity<EntryConnections>),
    Blob(Entity<EntryBlob>),
    Collection(Entity<EntryCollection>),
}

enum BlobStatus {
    Importing {
        bytes: u64,
    },
    Known {
        hash: Hash,
    },
    Active {
        hash: Hash,
        current_size: u64,
        total_size: u64,
    },
}

impl BlobStatus {
    pub fn hash(&self) -> Option<Hash> {
        match self {
            BlobStatus::Importing { .. } => None,
            BlobStatus::Known { hash } | BlobStatus::Active { hash, .. } => Some(*hash),
        }
    }
}

pub struct EntryBlob {
    id: u64,
    status: BlobStatus,
    name: SharedString,

    base: EntryBase<EntryStatus>,
}

#[derive(Clone)]
pub enum EntryStatus {
    Downloading(Entity<EntryStatusDownloading>),
    Exporting(Entity<EntryStatusExporting>),
}

impl AsRef<EntryBase<EntryStatus>> for EntryBlob {
    fn as_ref(&self) -> &EntryBase<EntryStatus> {
        &self.base
    }
}

impl AsMut<EntryBase<EntryStatus>> for EntryBlob {
    fn as_mut(&mut self) -> &mut EntryBase<EntryStatus> {
        &mut self.base
    }
}

pub struct EntryStatusDownloadingPeer {
    base: EntryBase<Infallible>,
    public_key: PublicKey,
    total: u64,
    received: u64,
    speed: u64,
    second_received: u64,
    last_second: Instant,
}

impl AsRef<EntryBase<Infallible>> for EntryStatusDownloadingPeer {
    fn as_ref(&self) -> &EntryBase<Infallible> {
        &self.base
    }
}

impl AsMut<EntryBase<Infallible>> for EntryStatusDownloadingPeer {
    fn as_mut(&mut self) -> &mut EntryBase<Infallible> {
        &mut self.base
    }
}

impl Render for EntryStatusDownloadingPeer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let downloading = div()
            .text_color(DOWNLOADING)
            .child(format!("{}/s", format_bytes(self.speed)));

        let downloaded = div()
            .text_color(DOWNLOAD)
            .child(format_bytes(self.received));

        let precent = div().text_color(WHITE).child(format!(
            "{:0.2}%",
            (self.received as f64 / self.total as f64) * 100.
        ));

        entry_base(
            self,
            self.public_key.to_string().into(),
            |header, _| {
                header
                    .bg(BLACK_2)
                    .child(downloading)
                    .child(div().min_w(px(4.)))
                    .child(downloaded)
                    .child(div().min_w(px(4.)))
                    .child(precent)
            },
            |_, _| unreachable!(),
            |_| div().into_any_element(),
            window,
            cx,
        )
    }
}

pub struct EntryStatusDownloading {
    base: EntryBase<Entity<EntryStatusDownloadingPeer>>,
    active: Option<PublicKey>,
}

impl AsRef<EntryBase<Entity<EntryStatusDownloadingPeer>>> for EntryStatusDownloading {
    fn as_ref(&self) -> &EntryBase<Entity<EntryStatusDownloadingPeer>> {
        &self.base
    }
}

impl AsMut<EntryBase<Entity<EntryStatusDownloadingPeer>>> for EntryStatusDownloading {
    fn as_mut(&mut self) -> &mut EntryBase<Entity<EntryStatusDownloadingPeer>> {
        &mut self.base
    }
}

impl Render for EntryStatusDownloading {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        entry_base(
            self,
            SharedString::new_static("Downloading"),
            |header, _| header,
            |entry, _cx| entry.clone().into_any_element(),
            |_| div().into_any_element(),
            window,
            cx,
        )
    }
}

pub struct EntryStatusExporting {
    base: EntryBase<Infallible>,
    path: String,
    exported: u64,
    total: u64,
}

impl Render for EntryStatusExporting {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let precent = div().text_color(WHITE).child(format!(
            "{:0.2}",
            (self.exported as f64 / self.total as f64) * 100.
        ));

        entry_base(
            self,
            format!("Exporting to: {}", self.path).into(),
            |header, _| header.bg(BLACK_2).child(precent),
            |_, _| unreachable!(),
            |_| div().into_any_element(),
            window,
            cx,
        )
    }
}

impl AsRef<EntryBase<Infallible>> for EntryStatusExporting {
    fn as_ref(&self) -> &EntryBase<Infallible> {
        &self.base
    }
}

impl AsMut<EntryBase<Infallible>> for EntryStatusExporting {
    fn as_mut(&mut self) -> &mut EntryBase<Infallible> {
        &mut self.base
    }
}

pub struct EntryCollection {
    base: EntryBase<Entity<EntryBlob>>,
    id: u64,
    hash: Hash,
    name: SharedString,
}

impl AsRef<EntryBase<Entity<EntryBlob>>> for EntryCollection {
    fn as_ref(&self) -> &EntryBase<Entity<EntryBlob>> {
        &self.base
    }
}

impl AsMut<EntryBase<Entity<EntryBlob>>> for EntryCollection {
    fn as_mut(&mut self) -> &mut EntryBase<Entity<EntryBlob>> {
        &mut self.base
    }
}

impl EventEmitter<Event> for EntryCollection {}

pub struct EntryBase<Entry> {
    entries: Vec<Entry>,
    expanded: bool,
    show_context_menu: bool,
    track_bounds: Entity<Bounds<Pixels>>,
    entry_header_hovered: bool,
    context_menu_hovered: bool,
    context_menu_offset_x: Pixels,
}

impl<T> EntryBase<T> {
    pub fn new(cx: &mut App) -> Self {
        Self {
            entries: Vec::default(),
            expanded: false,
            show_context_menu: false,
            track_bounds: cx.new(|_| Bounds::default()),
            entry_header_hovered: false,
            context_menu_hovered: false,
            context_menu_offset_x: px(0.),
        }
    }
}

fn entry_base<E: 'static, T: AsRef<EntryBase<E>> + AsMut<EntryBase<E>> + 'static>(
    entry_base: &mut T,
    name: SharedString,
    header_buttons: impl FnOnce(Div, &mut Context<T>) -> Div,
    get_entry: impl Fn(&E, &mut Context<T>) -> AnyElement,
    context_menu: impl FnOnce(&mut Context<T>) -> AnyElement,
    _window: &mut Window,
    cx: &mut Context<T>,
) -> impl IntoElement {
    let mut result = div().flex().flex_col().child(
        div()
            .flex()
            .flex_col()
            .min_h(px(25.))
            .max_h(px(25.))
            .bg(BLACK_4)
            .text_size(px(16.0))
            .id("entry")
            .child(TrackBounds {
                bounds: entry_base.as_ref().track_bounds.clone(),
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .min_h(px(25.))
                    .max_h(px(25.))
                    .items_center()
                    .when(
                        std::any::TypeId::of::<E>() != std::any::TypeId::of::<Infallible>(),
                        |this| {
                            this.child(
                                svg()
                                    .flex()
                                    .text_color(WHITE)
                                    .path(SharedString::new_static(
                                        if entry_base.as_ref().expanded {
                                            "arrow_downward"
                                        } else {
                                            "arrow_right"
                                        },
                                    ))
                                    .min_w(px(25.))
                                    .min_h(px(25.))
                                    .max_w(px(25.))
                                    .max_h(px(25.))
                                    .id("expand")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.as_mut().expanded = !this.as_ref().expanded;
                                        cx.notify();
                                    })),
                            )
                        },
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .id("header_name")
                            .overflow_scroll()
                            .text_size(px(16.))
                            .child(div().child(name.clone())),
                    )
                    .child(div().flex().flex_grow())
                    .map(|this| header_buttons(this, cx))
                    .child(div().min_w(px(4.))),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.as_mut().show_context_menu = true;
                    let t = this.as_ref().track_bounds.read(cx);
                    this.as_mut().context_menu_offset_x = event.position.x - t.origin.x;
                    cx.notify();
                }),
            )
            .on_hover(cx.listener(|this, hovered, _window, cx| {
                this.as_mut().entry_header_hovered = *hovered;
                cx.notify();
            })),
    ); // header

    if entry_base.as_ref().expanded && !entry_base.as_ref().entries.is_empty() {
        let mut content = div().flex().flex_col().flex_grow().bg(BLACK_2).pb(px(4.));

        for entry in entry_base.as_ref().entries.iter() {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .child(get_entry(entry, cx))
                    .pt(px(4.0))
                    .pl(px(4.0)),
            );
        }

        let outer_body = div().flex().flex_col().pl(px(11.)).child(
            div()
                .flex()
                .bg(BLACK_4)
                .flex_col()
                .child(div().flex().flex_col().child(content).pl(px(3.)).pb(px(3.))),
        );

        result = result.child(outer_body); //body
    }

    if !(entry_base.as_ref().entry_header_hovered || entry_base.as_ref().context_menu_hovered) {
        entry_base.as_mut().show_context_menu = false;
    }

    result.when(entry_base.as_ref().show_context_menu, |this| {
        this.child(deferred(
            anchored()
                .anchor(gpui::Corner::TopLeft)
                .offset(Point {
                    x: entry_base.as_ref().context_menu_offset_x - px(20.),
                    y: px(25.),
                })
                .position_mode(gpui::AnchoredPositionMode::Local)
                .snap_to_window()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .bg(BLACK_2)
                        .child(
                            div()
                                .mb(px(2.))
                                .ml(px(2.))
                                .mr(px(2.))
                                .bg(BLACK_3)
                                .text_size(px(16.))
                                .child(context_menu(cx)),
                        )
                        .id("context_menu")
                        .on_hover(cx.listener(|this, hovered, _, cx| {
                            this.as_mut().context_menu_hovered = *hovered;
                            cx.notify();
                        })),
                ),
        ))
    })
}
impl Render for EntryCollection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut show_download = false;
        let mut show_export = false;
        let show_share = self.hash != Hash::EMPTY;

        for entry in self.base.entries.iter() {
            if let BlobStatus::Known { .. } = entry.read(cx).status {
                show_download = true;
            }

            if let BlobStatus::Active {
                total_size,
                current_size,
                ..
            } = entry.read(cx).status
                && current_size == total_size
            {
                show_export = true;
            }
        }

        entry_base(
            self,
            self.name.clone(),
            move |header, cx| {
                header.when(show_share, |this| {
                    this.when(show_download, |this| {
                        this.child(
                            svg()
                                .flex()
                                .text_color(DOWNLOAD)
                                .path(SharedString::new_static("download"))
                                .min_w(px(28.))
                                .min_h(px(28.))
                                .max_w(px(28.))
                                .max_h(px(28.))
                                .id("download")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.emit(Event::StartDownload { entry_id: this.id });
                                })),
                        )
                    })
                    .when(show_export, |this| {
                        this.child(
                            svg()
                                .flex()
                                .text_color(WHITE)
                                .path(SharedString::new_static("export"))
                                .min_w(px(28.))
                                .min_h(px(28.))
                                .max_w(px(28.))
                                .max_h(px(28.))
                                .id("export")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.emit(Event::Export { entry_id: this.id });
                                })),
                        )
                    })
                    .child(
                        svg()
                            .flex()
                            .text_color(WHITE)
                            .path(SharedString::new_static("share"))
                            .min_w(px(25.))
                            .min_h(px(25.))
                            .max_w(px(25.))
                            .max_h(px(25.))
                            .id("share")
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.emit(Event::ShareCollection {
                                    entry_id: this.id,
                                    me: false,
                                });
                            })),
                    )
                })
            },
            |entry, _cx| entry.clone().into_any_element(),
            move |cx| {
                div()
                    .when(show_share, |this| {
                        this.child(
                            div()
                                .bg(BLACK_2)
                                .mt(px(2.0))
                                .child("Share")
                                .id("share")
                                .hover(|s| s.bg(BLACK_3))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.emit(Event::ShareCollection {
                                        entry_id: this.id,
                                        me: false,
                                    });
                                })),
                        )
                    })
                    .when(show_export, |this| {
                        this.child(
                            div()
                                .bg(BLACK_2)
                                .mt(px(2.0))
                                .child("Export")
                                .id("export")
                                .hover(|s| s.bg(BLACK_3))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.emit(Event::Export { entry_id: this.id })
                                })),
                        )
                    })
                    .into_any_element()
            },
            window,
            cx,
        )
    }
}

impl EventEmitter<Event> for EntryBlob {}

impl Render for EntryBlob {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_export = if let BlobStatus::Active {
            current_size,
            total_size,
            ..
        } = &self.status
        {
            current_size == total_size
        } else {
            false
        };

        let header =
            match &self.status {
                BlobStatus::Importing { bytes } => div()
                    .text_color(WHITE)
                    .text_size(px(16.))
                    .child(SharedString::from(format!(
                        "Importing: {}",
                        format_bytes(*bytes)
                    ))),
                BlobStatus::Known { .. } => div().child(
                    svg()
                        .flex()
                        .text_color(DOWNLOAD)
                        .path(SharedString::new_static("download"))
                        .min_w(px(28.))
                        .min_h(px(28.))
                        .max_w(px(28.))
                        .max_h(px(28.))
                        .id("download")
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.emit(Event::StartDownload { entry_id: this.id });
                        })),
                ),
                BlobStatus::Active {
                    total_size,
                    current_size,
                    ..
                } => {
                    div()
                        .flex()
                        .flex_row()
                        .when(total_size != current_size, |this| {
                            this.child(div().text_color(DOWNLOADING).text_size(px(16.)).child(
                                format!(
                                    "{:0.2}%",
                                    (*current_size as f64 / *total_size as f64) * 100.
                                ),
                            ))
                        })
                        .when(show_export, |this| {
                            this.child(
                                svg()
                                    .flex()
                                    .text_color(WHITE)
                                    .path(SharedString::new_static("export"))
                                    .min_w(px(28.))
                                    .min_h(px(28.))
                                    .max_w(px(28.))
                                    .max_h(px(28.))
                                    .id("export")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.emit(Event::Export { entry_id: this.id });
                                    })),
                            )
                        })
                }
            };
        entry_base(
            self,
            self.name.clone(),
            move |h, _cx| h.child(header),
            |entry, _cx| match entry.clone() {
                EntryStatus::Downloading(e) => e.into_any_element(),
                EntryStatus::Exporting(e) => e.into_any_element(),
            },
            move |cx| {
                div()
                    .when(show_export, |this| {
                        this.child(
                            div()
                                .bg(BLACK_2)
                                .mt(px(2.0))
                                .child("Export")
                                .id("export")
                                .hover(|s| s.bg(BLACK_3))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.emit(Event::Export { entry_id: this.id })
                                })),
                        )
                    })
                    .into_any_element()
            },
            window,
            cx,
        )
    }
}

pub struct EntryConnections {
    entry_base: EntryBase<Entity<EntryConnection>>,
}

impl AsRef<EntryBase<Entity<EntryConnection>>> for EntryConnections {
    fn as_ref(&self) -> &EntryBase<Entity<EntryConnection>> {
        &self.entry_base
    }
}

impl AsMut<EntryBase<Entity<EntryConnection>>> for EntryConnections {
    fn as_mut(&mut self) -> &mut EntryBase<Entity<EntryConnection>> {
        &mut self.entry_base
    }
}

impl Render for EntryConnections {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        entry_base(
            self,
            SharedString::new_static("Connections"),
            |h, _| h,
            |entry, _| entry.clone().into_any_element(),
            |_| div().into_any_element(),
            window,
            cx,
        )
    }
}

pub struct EntryConnection {
    base: EntryBase<Entity<EntryConnectionStats>>,
    name: SharedString,
}

impl AsRef<EntryBase<Entity<EntryConnectionStats>>> for EntryConnection {
    fn as_ref(&self) -> &EntryBase<Entity<EntryConnectionStats>> {
        &self.base
    }
}

impl AsMut<EntryBase<Entity<EntryConnectionStats>>> for EntryConnection {
    fn as_mut(&mut self) -> &mut EntryBase<Entity<EntryConnectionStats>> {
        &mut self.base
    }
}

impl Render for EntryConnection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        entry_base(
            self,
            self.name.clone(),
            |h, _| h,
            |entry, _| entry.clone().into_any_element(),
            |_| div().into_any_element(),
            window,
            cx,
        )
    }
}

pub struct EntryConnectionStats {
    base: EntryBase<Infallible>,
    name: SharedString,
    ping: Duration,
    download_total: u64,
    upload_total: u64,
}

impl AsRef<EntryBase<Infallible>> for EntryConnectionStats {
    fn as_ref(&self) -> &EntryBase<Infallible> {
        &self.base
    }
}

impl AsMut<EntryBase<Infallible>> for EntryConnectionStats {
    fn as_mut(&mut self) -> &mut EntryBase<Infallible> {
        &mut self.base
    }
}

impl Render for EntryConnectionStats {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header_buttons = div()
            .flex()
            .flex_row()
            .text_size(px(14.))
            .child(
                div()
                    .text_color(DOWNLOADING)
                    .child(format_bytes(self.download_total)),
            )
            .child(div().min_w(px(4.)))
            .child(
                div()
                    .text_color(UPLOADING)
                    .child(format_bytes(self.upload_total)),
            )
            .child(div().min_w(px(4.)))
            .child(
                div()
                    .text_color(PING)
                    .child(format!("{}ms", self.ping.as_millis())),
            );

        let context_menu = div();

        entry_base(
            self,
            self.name.clone(),
            |header, _| header.bg(BLACK_2).child(header_buttons),
            |_, _| div().into_any_element(),
            |_| context_menu.into_any_element(),
            window,
            cx,
        )
    }
}

pub struct Tree {
    entries: Vec<Entry>,
    all_entries: HashMap<u64, Entry>,
}

impl Render for Tree {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut res = div()
            .flex()
            .flex_col()
            .flex_grow()
            .id("tree")
            .ml(px(4.0))
            .mr(px(4.0))
            .overflow_scroll();

        for entry in self.entries.iter() {
            res = match entry {
                Entry::Blob(blob) => res.child(div().mt(px(4.)).child(blob.clone())),
                Entry::Collection(collection) => {
                    res.child(div().mt(px(4.)).child(collection.clone()))
                }
                Entry::Connections(connections) => {
                    res.child(div().mt(px(4.)).child(connections.clone()))
                }
            };
        }

        res
    }
}

#[derive(Default)]
pub struct Settings {
    auto_collection: bool,
    auto_download: bool,
    auto_expand: bool,
}

#[derive(Debug)]
pub enum Message {
    ImportCollection {
        entry_id: u64,
        entries: u64,
    },
    ImportProgress {
        progress: AddProgressItem,
        collection_id: Option<u64>,
        entry_id: u64,
    },
    FoundCollection {
        hash: Hash,
        entry_id: u64,
        name: String,
        provider: EndpointAddr,
    },
    Found {
        collection_id: Option<u64>,
        entry_id: u64,
        hash: Hash,
        name: String,
        provider: EndpointAddr,
    },
    DownloadProgress {
        entry_id: u64,
        progress: DownloadProgressItem,
        max_size: u64,
    },
    SetCollectionHash {
        entry_id: u64,
        hash: Hash,
    },
    Event(Event),
    ExportProgress {
        entry_id: u64,
        path: SharedString,
        size: u64,
    },
    Connections(BTreeMap<PublicKey, BTreeMap<u64, Vec<(SharedString, Duration, u64, u64)>>>),
}

#[derive(Debug, Clone)]
pub enum Event {
    ShareCollection { entry_id: u64, me: bool },
    StartDownload { entry_id: u64 },
    Export { entry_id: u64 },
}

pub struct MuzzManApp {
    focus_handle: FocusHandle,
    text_input: Entity<TextInput>,
    tree: Entity<Tree>,
    settings: Settings,

    sender: Sender<Message>,

    node: iroh::protocol::Router,
    blobs: iroh_blobs::BlobsProtocol,

    blob_info: HashMap<u64, (Hash, Vec<EndpointAddr>, u64)>,
}

impl MuzzManApp {
    fn handle_url(&mut self, url: &str, cx: &mut Context<'_, MuzzManApp>) {
        let Some((protocol, url)) = url.split_once(':') else {
            eprintln!("Invalid url");
            return;
        };

        match protocol {
            "sendme" => match BlobTicket::from_str(url) {
                Err(err) => {
                    eprintln!("Invalid sendme url: {err}");
                }
                Ok(ticket) => {
                    eprintln!("Ticket: {ticket}");

                    let sender = self.sender.clone();
                    let blobs = self.blobs.clone();
                    let node = self.node.clone();
                    gpui_tokio::Tokio::spawn(cx, async move {
                        let downloader = blobs.downloader(node.endpoint());
                        match node
                            .endpoint()
                            .connect(ticket.addr().clone(), iroh_blobs::ALPN)
                            .await
                        {
                            Err(err) => eprintln!("Cannot connect! {err}"),
                            Ok(_) => {
                                let progress = downloader.download(
                                    HashAndFormat::raw(ticket.hash()),
                                    [ticket.addr().id],
                                );
                                match progress.stream().await {
                                    Err(err) => {
                                        eprintln!("Downloading Meta error: {err}");
                                    }
                                    Ok(mut stream) => {
                                        while let Some(progress) = stream.next().await {
                                            println!("Download Meta: {progress:?}");
                                        }

                                        let mut reader = blobs.blobs().reader(ticket.hash());
                                        let mut hashes = Vec::<u8>::with_capacity(1024);
                                        reader.read_to_end(&mut hashes).await.unwrap();
                                        let hashes = HashSeq::new(hashes.into()).unwrap();
                                        let mut hashes_iterator = hashes.iter();

                                        let Some(meta_hash) = hashes_iterator.next() else {
                                            eprintln!(
                                                "Empty blob, cannot find the Collection Meta!"
                                            );
                                            return;
                                        };

                                        if let Err(err) = downloader
                                            .download(
                                                HashAndFormat::raw(meta_hash),
                                                [ticket.addr().id],
                                            )
                                            .await
                                        {
                                            eprintln!("Cannot download: {ticket}, Error {err}");
                                        };

                                        let mut new_buffer = Vec::<u8>::with_capacity(1024);
                                        let mut reader = blobs.blobs().reader(meta_hash);
                                        reader.read_to_end(&mut new_buffer).await.unwrap();

                                        let collection_meta =
                                            postcard::from_bytes::<CollectionMeta>(&new_buffer)
                                                .unwrap();
                                        if !collection_meta.check_header() {
                                            eprintln!("Is not a valid collection");
                                            return;
                                        }

                                        let collection_id = next_id();

                                        sender
                                            .send(Message::FoundCollection {
                                                hash: ticket.hash(),
                                                entry_id: collection_id,
                                                name: ticket.to_string(),
                                                provider: ticket.addr().clone(),
                                            })
                                            .await
                                            .unwrap();

                                        for name in collection_meta.names() {
                                            let hash = hashes_iterator.next().unwrap();
                                            let id = next_id();

                                            sender
                                                .send(Message::Found {
                                                    collection_id: Some(collection_id),
                                                    entry_id: id,
                                                    hash,
                                                    name: name.to_owned(),
                                                    provider: ticket.addr().clone(),
                                                })
                                                .await
                                                .unwrap();
                                        }
                                    }
                                }
                            }
                        }
                    })
                    .detach();
                }
            },
            protocol => {
                eprintln!("Unknown protocol: {protocol}")
            }
        }
    }

    fn update(&mut self, message: Message, cx: &mut Context<'_, MuzzManApp>) {
        match message {
            Message::ImportCollection { entry_id, entries } => {
                self.blob_info
                    .entry(entry_id)
                    .or_insert((Hash::EMPTY, Vec::default(), entries));
            }
            Message::ImportProgress {
                progress,
                collection_id,
                entry_id,
            } => match progress {
                AddProgressItem::CopyProgress(progress) => {
                    self.tree.update(cx, |tree, cx| {
                        let Some(entry) = tree.all_entries.get(&entry_id) else {
                            return;
                        };
                        if let Entry::Blob(entry) = entry {
                            entry.update(cx, |entry, cx| {
                                entry.status = BlobStatus::Importing { bytes: progress };
                                cx.notify();
                            });
                        }
                    });
                }
                AddProgressItem::Size(_) => {}
                AddProgressItem::CopyDone => {}
                AddProgressItem::OutboardProgress(_) => {}
                AddProgressItem::Done(mut temp_tag) => {
                    self.blob_info
                        .entry(entry_id)
                        .or_insert((temp_tag.hash(), Vec::default(), 0));

                    self.tree.update(cx, |tree, cx| {
                        let Some(entry) = tree.all_entries.get(&entry_id) else {
                            return;
                        };

                        if let Entry::Blob(entry) = entry {
                            entry.update(cx, |entry, cx| {
                                let bytes = if let BlobStatus::Importing { bytes } = entry.status {
                                    bytes
                                } else {
                                    0
                                };

                                entry.status = BlobStatus::Active {
                                    hash: temp_tag.hash(),
                                    total_size: bytes,
                                    current_size: bytes,
                                };
                                cx.notify();
                            });
                        }
                    });

                    temp_tag.leak();

                    if let Some(collection_id) = collection_id {
                        let collection = self.blob_info.get_mut(&collection_id).unwrap();
                        collection.2 -= 1;

                        if collection.2 == 0 {
                            let tree = self.tree.read(cx);
                            let Entry::Collection(collection_entity) =
                                tree.all_entries.get(&collection_id).unwrap()
                            else {
                                eprintln!("Some how was not a collection");
                                return;
                            };
                            let collection = collection_entity.read(cx);

                            let mut links_and_hashes = Vec::new();
                            for entry in collection.as_ref().entries.iter() {
                                let entry = entry.read(cx);
                                links_and_hashes
                                    .push((entry.name.as_str(), entry.status.hash().unwrap()));
                            }

                            let collection = iroh_blobs::format::collection::Collection::from_iter(
                                links_and_hashes,
                            );

                            let blobs = self.blobs.clone();
                            let sender = self.sender.clone();

                            gpui_tokio::Tokio::spawn(cx, async move {
                                let mut temp_tag = collection.store(&blobs).await.unwrap();
                                temp_tag.leak();
                                sender
                                    .send(Message::SetCollectionHash {
                                        entry_id: collection_id,
                                        hash: temp_tag.hash(),
                                    })
                                    .await
                                    .unwrap();
                            })
                            .detach();
                        }
                    }
                }
                AddProgressItem::Error(_) => {}
            },
            Message::FoundCollection {
                hash,
                entry_id,
                name,
                provider,
            } => {
                let entry = self
                    .blob_info
                    .entry(entry_id)
                    .or_insert((hash, Vec::default(), 0));
                entry.1.push(provider);

                let entry = Entry::Collection(cx.new(|cx| {
                    let sender = self.sender.clone();
                    cx.subscribe_self::<Event>(move |_, event, _| {
                        sender.try_send(Message::Event(event.clone())).unwrap();
                    })
                    .detach();
                    EntryCollection {
                        id: entry_id,
                        hash,
                        name: name.into(),
                        base: EntryBase {
                            entries: Vec::default(),
                            expanded: false,
                            show_context_menu: false,
                            track_bounds: cx.new(|_| Bounds::default()),
                            entry_header_hovered: false,
                            context_menu_hovered: false,
                            context_menu_offset_x: px(0.),
                        },
                    }
                }));

                self.tree.update(cx, |tree, cx| {
                    tree.entries.push(entry.clone());
                    tree.all_entries.insert(entry_id, entry);
                    cx.notify();
                });
            }
            Message::Found {
                collection_id,
                entry_id,
                hash,
                name,
                provider,
            } => {
                let entry = self
                    .blob_info
                    .entry(entry_id)
                    .or_insert((hash, Vec::default(), 0));
                entry.1.push(provider);

                if self.settings.auto_download {
                    self.sender
                        .try_send(Message::Event(Event::StartDownload { entry_id }))
                        .unwrap();
                }

                self.tree.update(cx, |tree, cx| {
                    if let Some(collection_id) = collection_id {
                        let Some(Entry::Collection(collection)) =
                            tree.all_entries.get(&collection_id).cloned()
                        else {
                            eprintln!("Cannot get collection with id: {collection_id}");
                            return;
                        };
                        collection.update(cx, |collection, cx| {
                            let entry = cx.new(|cx| {
                                let sender = self.sender.clone();
                                cx.subscribe_self::<Event>(move |_, event, _| {
                                    sender.try_send(Message::Event(event.clone())).unwrap();
                                })
                                .detach();
                                EntryBlob {
                                    id: entry_id,
                                    status: BlobStatus::Known { hash },
                                    name: name.into(),
                                    base: EntryBase {
                                        entries: Vec::default(),
                                        expanded: false,
                                        show_context_menu: false,
                                        track_bounds: cx.new(|_| Bounds::default()),
                                        entry_header_hovered: false,
                                        context_menu_hovered: false,
                                        context_menu_offset_x: px(0.),
                                    },
                                }
                            });

                            tree.all_entries
                                .insert(entry_id, Entry::Blob(entry.clone()));
                            collection.as_mut().entries.push(entry);
                        });
                    } else {
                        let entry = Entry::Blob(cx.new(|cx| {
                            let sender = self.sender.clone();
                            cx.subscribe_self::<Event>(move |_, event, _| {
                                sender.try_send(Message::Event(event.clone())).unwrap();
                            })
                            .detach();

                            EntryBlob {
                                id: entry_id,
                                status: BlobStatus::Known { hash },
                                name: name.into(),
                                base: EntryBase {
                                    entries: Vec::default(),
                                    expanded: false,
                                    show_context_menu: false,
                                    track_bounds: cx.new(|_| Bounds::default()),
                                    entry_header_hovered: false,
                                    context_menu_hovered: false,
                                    context_menu_offset_x: px(0.),
                                },
                            }
                        }));

                        tree.all_entries.insert(entry_id, entry.clone());
                        tree.entries.push(entry.clone());
                    }

                    cx.notify();
                });
            }
            Message::Event(Event::StartDownload { entry_id }) => {
                let mut to_download = Vec::default();

                self.tree.update(cx, |tree, cx| {
                    let entry = tree.all_entries.get(&entry_id).unwrap();

                    match entry {
                        Entry::Blob(entity) => {
                            entity.update(cx, |entry, cx| {
                                if let BlobStatus::Known { hash, .. } = entry.status {
                                    let Some(info) = self.blob_info.get(&entry.id) else {
                                        eprintln!("Cannot get info for: {}", entry.id);
                                        return;
                                    };

                                    to_download.push((info.0, info.1.clone(), entry.id));

                                    entry.status = BlobStatus::Active {
                                        hash,
                                        total_size: u64::MAX,
                                        current_size: 0,
                                    };

                                    if self.settings.auto_expand {
                                        entry.as_mut().expanded = true;
                                        cx.notify();
                                    }
                                }
                            });
                        }
                        Entry::Collection(entity) => entity.update(cx, |collection, cx| {
                            for entity in collection.base.entries.iter() {
                                entity.update(cx, |entry, cx| {
                                    if let BlobStatus::Known { hash, .. } = entry.status {
                                        let Some(info) = self.blob_info.get(&entry.id) else {
                                            eprintln!("Cannot get info for: {}", entry.id);
                                            return;
                                        };

                                        to_download.push((info.0, info.1.clone(), entry.id));

                                        entry.status = BlobStatus::Active {
                                            hash,
                                            total_size: u64::MAX,
                                            current_size: 0,
                                        };

                                        if self.settings.auto_expand {
                                            entry.as_mut().expanded = true;
                                            cx.notify();
                                        }
                                    }
                                });
                            }
                        }),
                        _ => {}
                    }
                });

                for (hash, providers, entry_id) in to_download {
                    let node = self.node.clone();
                    let blobs = self.blobs.clone();
                    let sender = self.sender.clone();

                    gpui_tokio::Tokio::spawn(cx, async move {
                        let mut stats = None;

                        for provider in providers.iter() {
                            match node
                                .endpoint()
                                .connect(provider.clone(), iroh_blobs::ALPN)
                                .await
                            {
                                Err(err) => eprintln!("Cannot connect to: {} {err}", provider.id),
                                Ok(connection) => {
                                    if let Ok(res) = iroh_blobs::get::request::get_unverified_size(
                                        &connection,
                                        &hash,
                                    )
                                    .await
                                    {
                                        stats = Some(res);
                                    }
                                }
                            }
                        }

                        let downloader = blobs.downloader(node.endpoint());

                        let progress = downloader.download(
                            HashAndFormat::raw(hash),
                            providers.iter().map(|p| p.id).collect::<Vec<_>>(),
                        );

                        let mut stream = progress.stream().await.unwrap();

                        while let Some(progress) = stream.next().await {
                            _ = sender
                                .send(Message::DownloadProgress {
                                    entry_id,
                                    progress,
                                    max_size: stats.as_ref().map(|s| s.0).unwrap_or_default(),
                                })
                                .await;
                        }
                    })
                    .detach();
                }
            }
            Message::DownloadProgress {
                entry_id,
                progress,
                max_size,
            } => {
                self.tree.update(cx, |tree, cx| {
                    if let Entry::Blob(entry) = tree.all_entries.get(&entry_id).unwrap() {
                        entry.update(cx, |entry, cx| {
                            let EntryBlob {
                                status:
                                    BlobStatus::Active {
                                        hash,
                                        total_size,
                                        current_size,
                                        ..
                                    },
                                base:
                                    EntryBase {
                                        expanded, entries, ..
                                    },
                                ..
                            } = entry
                            else {
                                return;
                            };

                            match progress {
                                DownloadProgressItem::Error(err) => {
                                    eprintln!("Download error for {hash}: {err}");

                                    entries.retain(|entry| {
                                        !matches!(entry, EntryStatus::Downloading(_))
                                    });

                                    entry.status = BlobStatus::Known { hash: *hash };
                                }
                                DownloadProgressItem::DownloadError => {
                                    eprintln!("Download error for {hash}: Unknown");

                                    entries.retain(|entry| {
                                        !matches!(entry, EntryStatus::Downloading(_))
                                    });

                                    entry.status = BlobStatus::Known { hash: *hash };
                                }
                                DownloadProgressItem::TryProvider { id, request } => {
                                    assert_eq!(request.hash, *hash);

                                    let EntryStatus::Downloading(downloading) = entries
                                        .iter()
                                        .find(|&entry| matches!(entry, EntryStatus::Downloading(_)))
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            entries.push(EntryStatus::Downloading(cx.new(|cx| {
                                                EntryStatusDownloading {
                                                    base: EntryBase::new(cx),
                                                    active: None,
                                                }
                                            })));
                                            entries.last().unwrap().clone()
                                        })
                                    else {
                                        unreachable!()
                                    };

                                    downloading.update(cx, |downloading, cx| {
                                        downloading.base.entries.push(cx.new(|cx| {
                                            EntryStatusDownloadingPeer {
                                                public_key: id,
                                                base: EntryBase::new(cx),
                                                total: *total_size,
                                                received: 0,
                                                speed: 0,
                                                second_received: 0,
                                                last_second: Instant::now(),
                                            }
                                        }));

                                        downloading.active = Some(id);
                                    });

                                    if *expanded {
                                        cx.notify();
                                    }
                                }
                                DownloadProgressItem::ProviderFailed { .. } => {}
                                DownloadProgressItem::PartComplete { .. } => {
                                    entries.retain(|entry| {
                                        !matches!(entry, EntryStatus::Downloading(_))
                                    });

                                    if *expanded {
                                        cx.notify();
                                    }
                                }
                                DownloadProgressItem::Progress(bytes) => {
                                    *current_size = bytes;
                                    *total_size = max_size;

                                    let EntryStatus::Downloading(downloading) = entries
                                        .iter()
                                        .find(|entry| matches!(entry, EntryStatus::Downloading(_)))
                                        .unwrap()
                                    else {
                                        unreachable!()
                                    };

                                    downloading.update(cx, |downloading, cx| {
                                        if let Some(id) = downloading.active {
                                            let entry = downloading
                                                .base
                                                .entries
                                                .iter()
                                                .find(|entry| entry.read(cx).public_key == id)
                                                .unwrap();

                                            entry.update(cx, |stats, _| {
                                                stats.second_received += bytes - stats.received;
                                                stats.received = bytes;
                                                stats.total = *total_size;

                                                if stats.last_second.elapsed()
                                                    >= Duration::from_secs(1)
                                                {
                                                    stats.speed = stats.second_received;
                                                    stats.second_received = 0;
                                                    stats.last_second = Instant::now();
                                                }
                                            })
                                        }

                                        if *expanded {
                                            cx.notify();
                                        }
                                    });
                                }
                            }
                        });
                    }
                });
            }
            Message::Event(Event::ShareCollection { entry_id, me }) => {
                if me {
                    let Some(info) = self.blob_info.get(&entry_id) else {
                        unreachable!()
                    };

                    let ticket = BlobTicket::new(
                        self.node.endpoint().addr(),
                        info.0,
                        iroh_blobs::BlobFormat::HashSeq,
                    );

                    write_to_clipboard(cx, ClipboardItem::new_string(format!("sendme:{ticket}")));
                } else {
                    let Some(info) = self.blob_info.get(&entry_id) else {
                        unreachable!()
                    };

                    let ticket = BlobTicket::new(
                        info.1
                            .first()
                            .cloned()
                            .unwrap_or_else(|| self.node.endpoint().addr()),
                        info.0,
                        iroh_blobs::BlobFormat::HashSeq,
                    );

                    write_to_clipboard(cx, ClipboardItem::new_string(format!("sendme:{ticket}")));
                }
            }
            Message::SetCollectionHash { entry_id, hash } => {
                let info = self.blob_info.get_mut(&entry_id).unwrap();
                info.0 = hash;

                self.tree.update(cx, |tree, cx| {
                    let Entry::Collection(collection) = tree.all_entries.get(&entry_id).unwrap()
                    else {
                        todo!()
                    };

                    collection.update(cx, |collection, cx| {
                        collection.hash = hash;
                        cx.notify();
                    });
                })
            }
            Message::Event(Event::Export { entry_id }) => {
                let tree = self.tree.read(cx);

                let Some(entity) = tree.all_entries.get(&entry_id).cloned() else {
                    return;
                };

                match entity {
                    Entry::Blob(entity) => {
                        if self.settings.auto_expand {
                            entity.update(cx, |blob, cx| {
                                blob.as_mut().expanded = true;
                                cx.notify();
                            });
                        }

                        let blob = entity.read(cx);

                        let res = cx.prompt_for_new_path(Path::new(""), Some(&blob.name));
                        let Some(hash) = blob.status.hash() else {
                            return;
                        };
                        let blobs = self.blobs.clone();
                        let sender = self.sender.clone();
                        let entry_id = blob.id;
                        let total_size = match blob.status {
                            BlobStatus::Active { total_size, .. } => total_size,
                            _ => return,
                        };
                        cx.spawn(async move |_, cx| {
                            let Ok(Ok(Some(path))) = res.await else {
                                return;
                            };

                            println!("Exporting: {hash} to {path:?}");
                            _ = gpui_tokio::Tokio::spawn(cx, async move {
                                let mut stream = blobs.export(hash, &path).stream().await;
                                let path = SharedString::from(path.to_string_lossy().to_string());

                                while let Some(progress) = stream.next().await {
                                    match progress{
                                        iroh_blobs::api::blobs::ExportProgressItem::Size(_) => {},
                                        iroh_blobs::api::blobs::ExportProgressItem::CopyProgress(size) => {
                                            _ = sender.send(Message::ExportProgress { entry_id, path: path.clone(), size }).await;
                                        },
                                        iroh_blobs::api::blobs::ExportProgressItem::Done => {
                                            _ = sender.send(Message::ExportProgress { entry_id, path: path.clone(), size: total_size }).await;
                                        },
                                        iroh_blobs::api::blobs::ExportProgressItem::Error(error) => {
                                            println!("Export error: {error}");
                                        },
                                    }
                                }

                                println!("Exported: {hash} to {path:?}");
                            })
                            .await;
                        })
                        .detach();
                    }
                    Entry::Collection(entity) => {
                        if self.settings.auto_expand {
                            entity.update(cx, |collection, cx| {
                                collection.as_mut().expanded = true;
                                cx.notify();
                            });
                        }

                        let collection = entity.read(cx);

                        let res = cx.prompt_for_paths(PathPromptOptions {
                            files: false,
                            directories: true,
                            multiple: false,
                            prompt: Some("Export To".into()),
                        });

                        let mut to_save = Vec::new();

                        for child in collection.as_ref().entries.iter() {
                            let entry = child.read(cx);

                            let Some(hash) = entry.status.hash() else {
                                continue;
                            };

                            let path = entry.name.replace('\\', "/");

                            to_save.push((hash, path));
                        }

                        let blobs = self.blobs.clone();

                        cx.spawn(async move |_, cx| {
                            let Ok(Ok(Some(path))) = res.await else {
                                return;
                            };

                            let Some(root_path) = path.first() else {
                                return;
                            };

                            for to_save in to_save.iter() {
                                let mut path = root_path.clone();
                                let path_str = to_save.1.trim();
                                let path_str = path_str.strip_suffix("/").unwrap_or(path_str);

                                for component in path_str.split('/') {
                                    path = path.join(component);
                                }

                                let blobs = blobs.clone();
                                let hash = to_save.0;

                                println!("{hash}: {path:?}");

                                gpui_tokio::Tokio::spawn(cx, async move {
                                    match blobs.export(hash, &path).await {
                                        Ok(size) => {
                                            println!(
                                                "Exported {hash} to {path:?}, with size: {}",
                                                format_bytes(size)
                                            );
                                        }
                                        Err(err) => {
                                            println!(
                                                "Cannot export: {hash} to {path:?}, because: {err}"
                                            );
                                        }
                                    }
                                })
                                .detach();
                            }
                        })
                        .detach();
                    }
                    _ => {}
                }
            }
            Message::ExportProgress {
                entry_id,
                path,
                size,
            } => {
                let tree = self.tree.read(cx);
                let Some(Entry::Blob(blob)) = tree.all_entries.get(&entry_id).cloned() else {
                    return;
                };

                blob.update(cx, |blob, cx| {
                    let BlobStatus::Active { total_size, .. } = &mut blob.status else {
                        return;
                    };

                    if size == *total_size {
                        blob.base.entries.retain(|e| {
                            if let EntryStatus::Exporting(e) = e {
                                e.read(cx).path != path
                            } else {
                                true
                            }
                        });

                        cx.notify();
                        return;
                    }

                    let EntryStatus::Exporting(exporting) = blob
                        .base
                        .entries
                        .iter()
                        .find(|e| {
                            if let EntryStatus::Exporting(e) = e {
                                e.read(cx).path == path
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .unwrap_or_else(|| {
                            blob.base.entries.push(EntryStatus::Exporting(cx.new(|cx| {
                                EntryStatusExporting {
                                    base: EntryBase::new(cx),
                                    path: path.to_string(),
                                    exported: 0,
                                    total: *total_size,
                                }
                            })));
                            blob.base.entries.last().unwrap().clone()
                        })
                    else {
                        unreachable!()
                    };

                    exporting.update(cx, |exporting, _| {
                        exporting.exported = size;
                    });

                    if blob.as_ref().expanded {
                        cx.notify();
                    }
                });
            }
            Message::Connections(mut peers_info) => {
                let tree = self.tree.read(cx);
                let Some(Entry::Connections(connections)) = tree.entries.first().cloned() else {
                    return;
                };

                connections.update(cx, |connections, cx| {
                    connections.as_mut().entries.retain(|entry| {
                        entry.update(cx, |peer_entry, cx| {
                            let mut peer_id_for_entry = None;

                            for peer_id in peers_info.keys() {
                                if peer_entry.name != peer_id.to_string() {
                                    continue;
                                }

                                peer_id_for_entry = Some(*peer_id);

                                break;
                            }

                            let Some(peer_id) = peer_id_for_entry else {
                                return false;
                            };

                            let mut peer_info = peers_info.remove(&peer_id).unwrap();

                            peer_entry.as_mut().entries.retain(|connections_entry| {
                                connections_entry.update(cx, |connection, _cx| {
                                    let mut peer_info_uid = None;
                                    let mut peer_info_idx = None;

                                    for (uid, peer_info) in peer_info.iter() {
                                        for (i, route) in peer_info.iter().enumerate() {
                                            if route.0 != connection.name {
                                                continue;
                                            }

                                            peer_info_uid = Some(*uid);
                                            peer_info_idx = Some(i);

                                            break;
                                        }
                                    }

                                    let Some(peer_info_uid) = peer_info_uid else {
                                        return false;
                                    };
                                    let Some(peer_info_idx) = peer_info_idx else {
                                        return false;
                                    };
                                    let Some(peer_info) = peer_info.get_mut(&peer_info_uid) else {
                                        return false;
                                    };

                                    let route = peer_info.remove(peer_info_idx);

                                    connection.ping = route.1;
                                    connection.download_total = route.2;
                                    connection.upload_total = route.3;
                                    true
                                })
                            });

                            for peer_info in peer_info.values() {
                                for peer_info in peer_info {
                                    peer_entry.as_mut().entries.push(cx.new(|cx| {
                                        EntryConnectionStats {
                                            base: EntryBase::new(cx),
                                            name: peer_info.0.clone(),
                                            ping: peer_info.1,
                                            download_total: peer_info.2,
                                            upload_total: peer_info.3,
                                        }
                                    }));
                                }
                            }

                            !peer_entry.as_ref().entries.is_empty()
                        })
                    });

                    for (peer_id, peer_info) in peers_info.iter() {
                        connections.as_mut().entries.push(cx.new(move |cx| {
                            let mut peer_entry = EntryConnection {
                                base: EntryBase::new(cx),
                                name: peer_id.to_string().into(),
                            };
                            for peer_info in peer_info.values() {
                                for peer_info in peer_info {
                                    peer_entry.as_mut().entries.push(cx.new(|cx| {
                                        EntryConnectionStats {
                                            base: EntryBase::new(cx),
                                            name: peer_info.0.clone(),
                                            ping: peer_info.1,
                                            download_total: peer_info.2,
                                            upload_total: peer_info.3,
                                        }
                                    }));
                                }
                            }
                            peer_entry
                        }));
                    }
                });
                cx.notify();
            }
        }
    }

    fn add_files(&mut self, files: Vec<PathBuf>, cx: &mut Context<'_, MuzzManApp>) {
        let auto_collection = self.settings.auto_collection;

        self.tree.update(cx, |tree, cx| {
            'auto_collection: {
                if !auto_collection {
                    break 'auto_collection;
                }
                let mut min = files[0].components().count();

                for path in files.iter() {
                    min = min.min(path.components().count());
                }

                if min < 1 {
                    break 'auto_collection;
                }

                min -= 1;

                let mut common_components = None;
                for i in 1..min {
                    let i = min - i;
                    common_components = Some(i);
                    let common = files[0].components().nth(i).unwrap();
                    for path in files.iter() {
                        if common != path.components().nth(i).unwrap() {
                            common_components.take();
                            break;
                        }
                    }

                    if common_components.is_some() {
                        break;
                    }
                }

                let Some(common_index) = common_components else {
                    break 'auto_collection;
                };

                let collection_id = next_id();

                self.sender
                    .try_send(Message::ImportCollection {
                        entry_id: collection_id,
                        entries: files.len() as u64,
                    })
                    .unwrap();

                let mut entries = Vec::new();
                for file in files.iter() {
                    let mut filename = PathBuf::default();

                    for component in file.components().skip(common_index + 1) {
                        filename = filename.join(component);
                    }

                    let name = SharedString::from(filename.to_string_lossy().to_string());

                    let id = next_id();

                    let blobs = self.blobs.clone();
                    let sender = self.sender.clone();

                    let file = file.clone();

                    cx.background_spawn(async move {
                        let result = blobs.add_path_with_opts(AddPathOptions {
                            path: file,
                            format: iroh_blobs::BlobFormat::Raw,
                            mode: iroh_blobs::api::blobs::ImportMode::TryReference,
                        });
                        let mut stream = result.stream().await;
                        while let Some(item) = stream.next().await {
                            if let Err(err) = sender
                                .send(Message::ImportProgress {
                                    progress: item,
                                    entry_id: id,
                                    collection_id: Some(collection_id),
                                })
                                .await
                            {
                                eprintln!("Cannot send message: {err}");
                            }
                        }
                    })
                    .detach();

                    let entry = cx.new(|cx| {
                        let sender = self.sender.clone();
                        cx.subscribe_self::<Event>(move |_, event, _| {
                            sender.try_send(Message::Event(event.clone())).unwrap()
                        })
                        .detach();
                        EntryBlob {
                            status: BlobStatus::Importing { bytes: 0 },
                            id,
                            name,
                            base: EntryBase {
                                entries: Vec::default(),
                                expanded: false,
                                show_context_menu: false,
                                track_bounds: cx.new(|_cx| Bounds::default()),
                                entry_header_hovered: false,
                                context_menu_hovered: false,
                                context_menu_offset_x: px(0.),
                            },
                        }
                    });

                    entries.push(entry.clone());
                    tree.all_entries.insert(id, Entry::Blob(entry));
                }

                let mut filename = PathBuf::default();

                for (i, component) in files[0].components().enumerate() {
                    if i == common_index + 1 {
                        break;
                    }
                    filename = filename.join(component);
                }

                let name = SharedString::from(filename.to_string_lossy().to_string());

                let collection = Entry::Collection(cx.new(|cx| {
                    let sender = self.sender.clone();
                    cx.subscribe_self::<Event>(move |_, event, _| {
                        sender.try_send(Message::Event(event.clone())).unwrap()
                    })
                    .detach();

                    EntryCollection {
                        id: collection_id,
                        hash: Hash::EMPTY,
                        name,
                        base: EntryBase {
                            entries,
                            expanded: false,
                            show_context_menu: false,
                            track_bounds: cx.new(|_cx| Bounds::default()),
                            entry_header_hovered: false,
                            context_menu_hovered: false,
                            context_menu_offset_x: px(0.),
                        },
                    }
                }));

                tree.entries.push(collection.clone());
                tree.all_entries.insert(collection_id, collection);

                cx.notify();
                return;
            }

            for file in files {
                let name = file
                    .file_name()
                    .map(|f| SharedString::from(f.to_string_lossy().to_string()))
                    .unwrap_or_else(|| {
                        SharedString::new_static("File with not filename, whats going on?")
                    });

                let blobs = self.blobs.clone();
                let sender = self.sender.clone();

                let id = next_id();

                cx.background_spawn(async move {
                    let result = blobs.add_path_with_opts(AddPathOptions {
                        path: file,
                        format: iroh_blobs::BlobFormat::Raw,
                        mode: iroh_blobs::api::blobs::ImportMode::TryReference,
                    });
                    let mut stream = result.stream().await;
                    while let Some(item) = stream.next().await {
                        if let Err(err) = sender
                            .send(Message::ImportProgress {
                                progress: item,
                                entry_id: id,
                                collection_id: None,
                            })
                            .await
                        {
                            eprintln!("Cannot send message: {err}");
                        }
                    }
                })
                .detach();

                let entry = Entry::Blob(cx.new(|cx| {
                    let sender = self.sender.clone();
                    cx.subscribe_self::<Event>(move |_, event, _| {
                        sender.try_send(Message::Event(event.clone())).unwrap()
                    })
                    .detach();
                    EntryBlob {
                        status: BlobStatus::Importing { bytes: 0 },
                        id,
                        name,
                        base: EntryBase {
                            entries: Vec::default(),
                            expanded: false,
                            show_context_menu: false,
                            track_bounds: cx.new(|_cx| Bounds::default()),
                            entry_header_hovered: false,
                            context_menu_hovered: false,
                            context_menu_offset_x: px(0.),
                        },
                    }
                }));

                tree.entries.push(entry.clone());
                tree.all_entries.insert(id, entry);
            }
        });

        cx.notify();
    }
}

/// On KDE Plasma 6.5.5 on wayland the `App.write_to_clipboard` is not working!
/// This is a really bad, hack that works!
fn write_to_clipboard(cx: &mut App, item: ClipboardItem) {
    cx.spawn(async move |cx| {
        let mut written = false;

        while !written {
            cx.update(|cx| {
                if cx.read_from_clipboard().as_ref() == Some(&item) {
                    written = true;
                    println!("Succesfult written to the clipboard");
                } else {
                    println!("Try to write to the clipboard");
                    cx.write_to_clipboard(item.clone());
                }
            });

            _ = std::future::pending::<()>()
                .with_timeout(Duration::from_secs_f32(0.1), cx.background_executor())
                .await;
        }
    })
    .detach();
}

impl Render for MuzzManApp {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(BLACK_3)
            .flex()
            .flex_col()
            .font_family("Roboto Mono")
            .on_drop::<ExternalPaths>(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                let files = paths
                    .paths()
                    .iter()
                    .map(|path| get_files(path))
                    .reduce(|mut acc, e| {
                        acc.extend(e);
                        acc
                    })
                    .unwrap_or_default();

                this.add_files(files, cx);
            }))
            .child(
                div()
                    .bg(BLACK_2)
                    .min_h(px(42.0))
                    .max_h(px(42.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(div().min_w(px(4.)))
                    .child(
                        div()
                            .flex_grow()
                            .min_h(px(30.0))
                            .max_h(px(30.0))
                            .map(title_bar_zone),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .text_color(WHITE)
                            .child("MuzzMan")
                            .text_size(px(32.))
                            .child(div().child(VERSION).text_size(px(16.)))
                            .map(title_bar_zone),
                    )
                    .child(
                        div()
                            .flex_grow()
                            .min_h(px(30.0))
                            .max_h(px(30.0))
                            .map(title_bar_zone),
                    )
                    .child(
                        svg()
                            .text_color(WHITE)
                            .path("close")
                            .id("close_button")
                            .min_w(px(32.))
                            .min_h(px(32.))
                            .max_w(px(32.))
                            .max_h(px(32.))
                            .cursor_pointer()
                            .when(cfg![target_os = "windows"], |this| {
                                this.window_control_area(gpui::WindowControlArea::Close)
                            })
                            .when(cfg![target_os = "linux"], |this| {
                                this.on_click(|_, window, _| {
                                    window.remove_window();
                                })
                            }),
                    )
                    .child(div().min_w(px(4.))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .track_focus(&self.focus_handle)
                    .child(
                        div()
                            .text_color(gpui::rgb(0xffd43b))
                            .text_size(px(20.))
                            .text_center()
                            .child("Using this application will leak your current IP address!"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .text_size(px(16.))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .bg(BLACK_2)
                                    .pl(px(8.))
                                    .pr(px(8.))
                                    .child("Drop files or")
                                    .child(
                                        div()
                                            .left(px(4.0))
                                            .child("Browse files")
                                            .text_color(UPLOAD)
                                            .id("Browse files")
                                            .cursor(CursorStyle::PointingHand)
                                            .hover(|s| s.text_color(UPLOADING))
                                            .on_click(cx.listener(|_this, _, _, cx| {
                                                let prompt =
                                                    cx.prompt_for_paths(PathPromptOptions {
                                                        files: true,
                                                        directories: cx
                                                            .can_select_mixed_files_and_dirs(),
                                                        multiple: true,
                                                        prompt: Some("Import files".into()),
                                                    });

                                                cx.spawn(async move |this, cx| {
                                                    let result = prompt.await;

                                                    if let Ok(Ok(Some(files))) = result {
                                                        let this = this.upgrade().unwrap();
                                                        this.update(cx, move |this, cx| {
                                                            let files = files
                                                                .iter()
                                                                .map(|path| get_files(path))
                                                                .reduce(|mut acc, e| {
                                                                    acc.extend(e);
                                                                    acc
                                                                })
                                                                .unwrap_or_default();

                                                            this.add_files(files, cx);
                                                        });
                                                    }
                                                })
                                                .detach();
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .ml(px(4.))
                            .mr(px(4.))
                            .bg(BLACK_2)
                            .text_color(WHITE)
                            .text_size(px(21.))
                            .child(self.text_input.clone())
                            .child(div().h(px(2.0)).flex().flex_grow().bg(BLACK_5))
                            .child(div().child("Status: Waiting for Files or URL")),
                    ),
            )
            .text_color(gpui::white())
            .child(self.tree.clone())
            .when(
                cfg![target_os = "linux"] && !(window.is_fullscreen() || window.is_maximized()),
                |this| this.child(window_resize_frame()),
            )
    }
}

pub fn window_resize_frame() -> Div {
    div()
        .absolute()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .w_full()
                .child(
                    div()
                        .w(px(5.))
                        .h(px(5.))
                        .cursor_nwse_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::TopLeft);
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_grow()
                        .min_h(px(3.))
                        .max_h(px(3.))
                        .cursor_n_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::Top);
                        }),
                )
                .child(
                    div()
                        .w(px(5.))
                        .h(px(5.))
                        .cursor_nesw_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::TopRight);
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .flex_grow()
                .child(
                    div()
                        .flex_grow()
                        .max_w(px(3.0))
                        .cursor_ew_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::Left);
                        }),
                )
                .child(div().flex_grow())
                .child(
                    div()
                        .flex_grow()
                        .max_w(px(3.0))
                        .cursor_ew_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::Right);
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .w_full()
                .items_end()
                .child(
                    div()
                        .w(px(5.))
                        .h(px(5.))
                        .cursor_nesw_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::BottomLeft);
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_grow()
                        .min_h(px(3.))
                        .max_h(px(3.))
                        .cursor_s_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::Bottom);
                        }),
                )
                .child(
                    div()
                        .w(px(5.))
                        .h(px(5.))
                        .cursor_nwse_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::BottomRight);
                        }),
                ),
        )
}

pub fn title_bar_zone(this: Div) -> Div {
    this.on_mouse_down(MouseButton::Right, |event, window, _| {
        window.show_window_menu(event.position);
    })
    .when(cfg![target_os = "windows"], |this| {
        this.window_control_area(gpui::WindowControlArea::Drag)
    })
    .when(cfg![target_os = "linux"], |this| {
        this.on_mouse_down(MouseButton::Left, |event, window, _| {
            if event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
    })
}

#[derive(Clone)]
pub struct Connections {
    peers: Arc<tokio::sync::RwLock<BTreeMap<PublicKey, Vec<(ConnectionInfo, u64)>>>>,
    sender: tokio::sync::mpsc::Sender<()>,
}

struct IrohHooks(Connections);

impl std::fmt::Debug for IrohHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("IrohHooks").finish()
    }
}
impl iroh::endpoint::EndpointHooks for IrohHooks {
    async fn before_connect<'a>(
        &'a self,
        _remote_addr: &'a EndpointAddr,
        _alpn: &'a [u8],
    ) -> iroh::endpoint::BeforeConnectOutcome {
        iroh::endpoint::BeforeConnectOutcome::Accept
    }

    async fn after_handshake<'a>(
        &'a self,
        conn: &'a iroh::endpoint::ConnectionInfo,
    ) -> iroh::endpoint::AfterHandshakeOutcome {
        {
            let mut nodes = self.0.peers.write().await;
            let entry = nodes.entry(conn.remote_id()).or_default();
            entry.push((conn.clone(), next_connection_id()));
        }
        _ = self.0.sender.send(()).await;

        iroh::endpoint::AfterHandshakeOutcome::accept()
    }
}

fn main() {
    let application = gpui_platform::application().with_assets(Assets {});

    application.run(|cx| {
        gpui_tokio::init(cx);

        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("ctrl-a", SelectAll, None),
            KeyBinding::new("ctrl-v", Paste, None),
            KeyBinding::new("ctrl-c", Copy, None),
            KeyBinding::new("ctrl-x", Cut, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
            KeyBinding::new("enter", Submit, None),
        ]);

        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Message>(1024);
        let app_sender = sender.clone();

        let init = gpui_tokio::Tokio::spawn(cx, async {
            let (connections_sender, mut connections_receiver) = tokio::sync::mpsc::channel(1024);
            let connections = Connections{ peers: Arc::default(), sender: connections_sender };

            {
                let peers = connections.peers.clone();
                Box::leak(Box::new(tokio::spawn(async move{
                    let mut streams = BTreeSet::default();
                    let mut tasks = Vec::<Pin<Box<dyn Future<Output = (Option<PathInfoList>, u64, _)> + Send + Sync>>>::default();

                    loop{
                        {
                            let mut peers = peers.write().await;
                            let mut peers_info = BTreeMap::default();
                            for (node_id, routes) in peers.iter_mut(){
                                routes.retain(|c|c.0.is_alive());

                                let mut connections = BTreeMap::default();
                                for (connection_info, id) in routes.iter_mut(){
                                    let id = *id;
                                    let alpn = String::from_utf8(connection_info.alpn().to_vec()).unwrap_or_else(|_|format!("{:?}", connection_info.alpn()));
                                    let mut paths = Vec::default();
                                    for path in connection_info.paths().get(){
                                        let info = (match path.remote_addr() {
                                           iroh::TransportAddr::Relay(relay_url) => {
                                               SharedString::new(format!("Alpn: {alpn}, Relay: {}", relay_url))
                                           },
                                           iroh::TransportAddr::Ip(socket_addr) => {
                                               SharedString::new(format!("Alpn: {alpn}, Ip: {}", socket_addr))
                                           },
                                           _ => {eprintln!("Unknown TransportAddr"); SharedString::new_static("Unknown TransportAddr")},
                                       },path.stats().rtt, path.stats().udp_rx.bytes, path.stats().udp_tx.bytes);

                                       paths.push(info);
                                    }

                                    connections.insert(id, paths);
                                    if streams.contains(&id){
                                        continue;
                                    }

                                    streams.insert(id);
                                    let mut stream = connection_info.paths().stream();
                                    tasks.push(Box::pin(async move{(stream.next().await, id, stream)}));
                                }
                                peers_info.insert(*node_id, connections);
                            }
                            _ = app_sender.send(Message::Connections(peers_info)).await;
                        }

                        if tasks.is_empty(){
                            _ = app_sender.send(Message::Connections(BTreeMap::default())).await;

                            tokio::select! {
                                _ = connections_receiver.recv() => {
                                }
                            }
                        }else{
                            let mut to_wait = futures_util::future::select_all(std::mem::take(&mut tasks));
                            let _to_wait = std::pin::pin!(&mut to_wait);
                            tokio::select! {
                                _ = tokio::time::timeout(std::time::Duration::from_secs(1), connections_receiver.recv()) => {
                                    tasks = to_wait.into_inner();
                                }
                                (result, _, remaining) = _to_wait => {
                                    {
                                        tasks.extend(remaining);
                                        if result.0.is_some(){
                                            let id = result.1;
                                            let mut stream = result.2;
                                            tasks.push(Box::pin(async move{(stream.next().await, id, stream)}));
                                        }
                                    }
                                }
                            }
                        }
                    }
                })));
            }

            let endpoint = iroh::Endpoint::builder().hooks(IrohHooks(connections.clone()))
                .bind().await.unwrap();

            let store = iroh_blobs::store::mem::MemStore::new();

            // let (sender, mut receiver) = iroh_blobs::provider::events::EventSender::channel(1, iroh_blobs::provider::events::EventMask{ connected: iroh_blobs::provider::events::ConnectMode::Notify, get: iroh_blobs::provider::events::RequestMode::NotifyLog, get_many: iroh_blobs::provider::events::RequestMode::NotifyLog, push: iroh_blobs::provider::events::RequestMode::NotifyLog, observe: iroh_blobs::provider::events::ObserveMode::Notify, throttle: iroh_blobs::provider::events::ThrottleMode::None});
            // Box::leak(Box::new(tokio::spawn(async move{
            //     while let Some(msg) = receiver.recv().await{
            //         eprintln!("iroh-blobs event: {msg:?}");
            //     }
            // })));

            // let blobs = iroh_blobs::BlobsProtocol::new(&store, Some(sender));
            let blobs = iroh_blobs::BlobsProtocol::new(&store, None);

            let node = iroh::protocol::RouterBuilder::new(endpoint)
                .accept(iroh_blobs::ALPN, blobs.clone())
                .spawn();
            println!("NODE ID: {}", node.endpoint().id());

            (node, blobs)
        });

        cx.spawn(async move |cx| {
            let (node, blobs) = init.await.unwrap();


            cx.update(move|cx|{
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                            None,
                            gpui::size(800f32.into(), 300f32.into()),
                            cx,
                        ))),
                        window_decorations: Some(WindowDecorations::Client),
                        window_min_size: Some(size(px(800.), px(300.))),
                        titlebar: Some(TitlebarOptions{title: Some("MuzzMan".into()), appears_transparent: true, traffic_light_position: None}),
                        window_background: gpui::WindowBackgroundAppearance::Opaque,
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new::<MuzzManApp>(|cx| {
                            let on_submit = cx.listener(|this, text: &SharedString, _, cx|{
                                this.handle_url(text, cx);
                            });
                            let text_input = cx.new(|text_input_cx| TextInput {
                                id: "URL_input".into(),
                                focus_handle: text_input_cx.focus_handle(),
                                content: SharedString::new(""),
                                placeholder: SharedString::new(
                                    "Get URL: sendme:blob54686973206973206e6f742061207265616c20636f6c6c656374696f6e",
                                ),
                                placeholder_color: BLACK_5.into(),
                                selected_range: 0..0,
                                selection_reversed: false,
                                marked_range: None,
                                last_layout: None,
                                last_bounds: None,
                                is_selecting: false,
                                scroll_handle: ScrollHandle::new(),
                                on_submit: Box::new(on_submit)
                            });

                            cx.spawn(async move |this, cx|{
                                while let Some(message) = receiver.recv().await{
                                    if let Some(app) = this.upgrade()
                                        {
                                            app.update(cx, |this, cx|{ this.update(message, cx); });
                                        }
                                }
                            }).detach();
                            MuzzManApp {
                                text_input,
                                focus_handle: cx.focus_handle(),
                                tree: cx.new(|cx| Tree{ entries: vec![Entry::Connections(cx.new(|cx|EntryConnections{entry_base: EntryBase::new(cx)}))], all_entries: HashMap::default() } ),
                                settings: Settings {
                                    auto_collection: true,
                                    auto_download: false,
                                    auto_expand: true,
                                },

                                sender,

                                node,
                                blobs,

                                blob_info: HashMap::default()
                            }
                        })
                    },
                )
                .expect("Cannot create Main Window");
            });


        }).detach();


    });
}

fn get_files(path: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::default();

    let mut new_paths = vec![path.to_owned()];
    while !new_paths.is_empty() {
        for new_path in std::mem::take(&mut new_paths) {
            if new_path.is_file() {
                paths.push(new_path);
                continue;
            }

            match new_path.read_dir() {
                Err(err) => {
                    eprintln!("Cannot read: {new_path:?} as a directory!, error: {err}");
                }
                Ok(dir) => {
                    for path in dir {
                        match path {
                            Err(err) => {
                                eprintln!(
                                    "Cannot get directory entry, for: {new_path:?}, error: {err}"
                                );
                            }
                            Ok(entry) => {
                                new_paths.push(entry.path());
                            }
                        }
                    }
                }
            }
        }
    }

    paths
}

fn format_bytes(bytes: u64) -> String {
    match bytes {
        0..2 => format!("{bytes} byte"),
        2..1_000 => format!("{bytes} bytes"),
        1_000..1_000_000 => format!("{}kB", bytes / 1_000),
        1_000_000..1_000_000_000 => format!("{}MB", bytes / 1_000_000),
        _ => format!("{}GB", bytes / 1_000_000_000),
    }
}
