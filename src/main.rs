use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
    sync::atomic::AtomicU64,
};

use blobsman_graphics::{
    futures_util::StreamExt,
    gpui::{
        self, AssetSource, ClipboardItem, Div, EventEmitter, PathPromptOptions, ScrollHandle,
        WindowDecorations, anchored, deferred, svg,
    },
    gpui_tokio,
    iroh::{self, EndpointAddr, PublicKey},
    iroh_blobs::{
        self, Hash, HashAndFormat,
        api::{
            blobs::{AddPathOptions, AddProgressItem},
            downloader::DownloadProgressItem,
        },
        hashseq::HashSeq,
        ticket::BlobTicket,
    },
    tokio::{self, io::AsyncReadExt, sync::mpsc::Sender},
};
use gpui::{
    App, AppContext, Bounds, CursorStyle, ElementId, Entity, ExternalPaths, FocusHandle,
    GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, ParentElement, Pixels,
    Point, Render, SharedString, Style, Styled, Window, WindowOptions, div, prelude::*, px, rgb,
    size,
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
            _ => Ok(None),
        }
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
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
    Blob(Entity<EntryBlob>),
    Collection(Entity<EntryCollection>),
}

struct DownloadStatus {
    total: u64,
    speed: usize,
}

struct UploadStatus {
    total: usize,
    speed: usize,
}

pub enum EntryStatus {
    Importing {
        bytes: u64,
    },
    Known {
        hash: Hash,
    },
    Active {
        hash: Hash,
        downloading: HashMap<PublicKey, DownloadStatus>,
        uploading: HashMap<PublicKey, UploadStatus>,
        total: u64,
    },
}

impl EntryStatus {
    pub fn hash(&self) -> Option<Hash> {
        match self {
            EntryStatus::Importing { .. } => None,
            EntryStatus::Known { hash } | EntryStatus::Active { hash, .. } => Some(*hash),
        }
    }
}

pub struct EntryBlob {
    id: u64,
    status: EntryStatus,
    name: SharedString,

    expanded: bool,
    show_context_menu: bool,
    track_bounds: Entity<Bounds<Pixels>>,
    entry_header_hovered: bool,
    context_menu_hovered: bool,
    context_menu_offset_x: Pixels,
}

pub struct EntryCollection {
    id: u64,
    hash: Hash,
    name: SharedString,

    entries: Vec<Entity<EntryBlob>>,
    expanded: bool,
    show_context_menu: bool,
    track_bounds: Entity<Bounds<Pixels>>,
    entry_header_hovered: bool,
    context_menu_hovered: bool,
    context_menu_offset_x: Pixels,
}

impl EventEmitter<Message> for EntryCollection {}

impl Render for EntryCollection {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut result = div().flex().flex_col().child(
            div()
                .flex()
                .flex_col()
                .min_h(px(scale(17.)))
                .max_h(px(scale(17.)))
                .bg(rgb(0x495057))
                .text_size(px(scale(14.0)))
                .id("entry")
                .child(TrackBounds {
                    bounds: self.track_bounds.clone(),
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .min_h(px(scale(17.)))
                        .max_h(px(scale(17.)))
                        .id("header_name")
                        .items_center()
                        .overflow_x_scroll()
                        .child(div().min_w(px(scale(3.))).max_w(px(scale(3.))))
                        .child(
                            svg()
                                .flex()
                                .text_color(rgb(0xffffff))
                                .path(SharedString::new_static(if self.expanded {
                                    "arrow_downward"
                                } else {
                                    "arrow_right"
                                }))
                                .min_w(px(scale(14.)))
                                .min_h(px(scale(14.)))
                                .max_w(px(scale(14.)))
                                .max_h(px(scale(14.)))
                                .id("expand")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.expanded = !this.expanded;
                                    cx.notify();
                                })),
                        )
                        .child(self.name.clone())
                        .child(div().flex().flex_grow())
                        .when(self.hash != Hash::EMPTY, |this| {
                            this.child(
                                svg()
                                    .flex()
                                    .text_color(rgb(0xffffff))
                                    .path(SharedString::new_static("share"))
                                    .min_w(px(scale(14.)))
                                    .min_h(px(scale(14.)))
                                    .max_w(px(scale(14.)))
                                    .max_h(px(scale(14.)))
                                    .id("share")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.emit(Message::ShareCollection {
                                            entry_id: this.id,
                                            me: false,
                                        });
                                    })),
                            )
                        })
                        .child(div().min_w(px(scale(2.)))),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        this.show_context_menu = true;
                        let t = this.track_bounds.read(cx);
                        this.context_menu_offset_x = event.position.x - t.origin.x;
                        cx.notify();
                    }),
                )
                .on_hover(cx.listener(|this, hovered, _window, cx| {
                    this.entry_header_hovered = *hovered;
                    cx.notify();
                })),
        ); // header

        if self.expanded && !self.entries.is_empty() {
            let mut content = div()
                .flex()
                .flex_col()
                .flex_grow()
                .bg(rgb(0x212529))
                .pb(px(scale(4.)))
                .pt(px(scale(4.)));

            for entry in self.entries.iter() {
                content = content.child(
                    div()
                        .flex()
                        .flex_col()
                        .child(entry.clone())
                        .pt(px(scale(4.0)))
                        .pb(px(scale(4.0)))
                        .pl(px(scale(8.0))),
                );
            }

            let outer_body = div().flex().flex_col().pl(px(scale(9.0))).child(
                div().flex().bg(rgb(0x495057)).flex_col().child(
                    div()
                        .flex()
                        .flex_col()
                        .child(content)
                        .pl(px(scale(2.)))
                        .pb(px(scale(2.))),
                ),
            );

            result = result.child(outer_body); //body
        }

        if !(self.entry_header_hovered || self.context_menu_hovered) {
            self.show_context_menu = false;
        }

        result.when(self.show_context_menu, |this| {
            this.child(deferred(
                anchored()
                    .anchor(gpui::Corner::TopLeft)
                    .offset(Point {
                        x: self.context_menu_offset_x - px(scale(20.)),
                        y: px(scale(17.)),
                    })
                    .position_mode(gpui::AnchoredPositionMode::Local)
                    .snap_to_window()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .bg(rgb(0x212121))
                            .child(
                                div()
                                    .mb(px(scale(1.)))
                                    .ml(px(scale(1.)))
                                    .mr(px(scale(1.)))
                                    .bg(rgb(0x495057)) // 0x343a40
                                    .text_size(px(scale(8.)))
                                    .child(
                                        div()
                                            .bg(rgb(0x212121))
                                            .mt(px(scale(1.0)))
                                            .child("Copy Ticket")
                                            .id("copy-ticket")
                                            .hover(|s| s.bg(rgb(0x2f2f2f)))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    this.hash.to_hex(),
                                                ));
                                            })),
                                    )
                                    .child(
                                        div()
                                            .bg(rgb(0x212121))
                                            .mt(px(scale(1.0)))
                                            .child("Export")
                                            .id("export")
                                            .hover(|s| s.bg(rgb(0x2f2f2f))),
                                    )
                                    .child(
                                        div()
                                            .bg(rgb(0x212121))
                                            .mt(px(scale(1.0)))
                                            .child("Remove")
                                            .id("remove")
                                            .hover(|s| s.bg(rgb(0x2f2f2f))),
                                    ),
                            )
                            .id("context_menu")
                            .on_hover(cx.listener(|this, hovered, _, cx| {
                                this.context_menu_hovered = *hovered;
                                cx.notify();
                            })),
                    ),
            ))
        })
    }
}

impl EventEmitter<Message> for EntryBlob {}

impl Render for EntryBlob {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut result = div().flex().flex_col().child(
            div()
                .flex()
                .flex_col()
                .min_h(px(scale(17.)))
                .max_h(px(scale(17.)))
                .bg(rgb(0x495057))
                .text_size(px(scale(14.0)))
                .id("entry")
                .child(TrackBounds {
                    bounds: self.track_bounds.clone(),
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .min_h(px(scale(17.)))
                        .max_h(px(scale(17.)))
                        .id("header_name")
                        .items_center()
                        .overflow_x_scroll()
                        .child(div().min_w(px(scale(3.))).max_w(px(scale(3.))))
                        .child(
                            svg()
                                .flex()
                                .text_color(rgb(0xffffff))
                                .path(SharedString::new_static(if self.expanded {
                                    "arrow_downward"
                                } else {
                                    "arrow_right"
                                }))
                                .min_w(px(scale(14.)))
                                .min_h(px(scale(14.)))
                                .max_w(px(scale(14.)))
                                .max_h(px(scale(14.)))
                                .id("expand")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.expanded = !this.expanded;
                                    cx.notify();
                                })),
                        )
                        .child(self.name.clone())
                        .child(div().flex().flex_grow())
                        .child(div().min_w(px(scale(2.))))
                        .when(true, |this| match &self.status {
                            EntryStatus::Importing { bytes } => this.child(
                                div()
                                    .text_color(rgb(0x00ff00))
                                    .text_size(px(scale(8.)))
                                    .child(SharedString::from(format!(
                                        "Importing: {}",
                                        format_bytes(*bytes)
                                    ))),
                            ),
                            EntryStatus::Known { .. } => this.child(
                                svg()
                                    .flex()
                                    .text_color(rgb(0x00ff00))
                                    .path(SharedString::new_static("download"))
                                    .min_w(px(scale(14.)))
                                    .min_h(px(scale(14.)))
                                    .max_w(px(scale(14.)))
                                    .max_h(px(scale(14.)))
                                    .id("download")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.emit(Message::StartDownload { entry_id: this.id });
                                    })),
                            ),
                            EntryStatus::Active {
                                downloading, total, ..
                            } => this
                                .when(
                                    (downloading.iter().next().map(|e| e.1.total))
                                        .unwrap_or(*total)
                                        != *total,
                                    |this| {
                                        this.child(
                                            div()
                                                .text_color(rgb(0x00ff00))
                                                .text_size(px(scale(8.)))
                                                .child(format!(
                                                    "{:0.2}%",
                                                    ((downloading.iter().next().map(|e| e.1.total))
                                                        .unwrap_or(*total)
                                                        as f32
                                                        / (*total as f32))
                                                        * 100.
                                                )),
                                        )
                                    },
                                )
                                .when(
                                    self.status.hash().unwrap_or(Hash::EMPTY) != Hash::EMPTY,
                                    |this| {
                                        this.child(
                                            svg()
                                                .flex()
                                                .text_color(rgb(0xffffff))
                                                .path(SharedString::new_static("share"))
                                                .min_w(px(scale(14.)))
                                                .min_h(px(scale(14.)))
                                                .max_w(px(scale(14.)))
                                                .max_h(px(scale(14.)))
                                                .id("share")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    cx.emit(Message::Share {
                                                        entry_id: this.id,
                                                        me: false,
                                                    });
                                                })),
                                        )
                                    },
                                ),
                        })
                        .child(div().min_w(px(scale(2.)))),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        this.show_context_menu = true;
                        let t = this.track_bounds.read(cx);
                        this.context_menu_offset_x = event.position.x - t.origin.x;
                        cx.notify();
                    }),
                )
                .on_hover(cx.listener(|this, hovered, _window, cx| {
                    this.entry_header_hovered = *hovered;
                    cx.notify();
                })),
        ); // header

        if self.expanded {
            let content = div()
                .flex()
                .flex_col()
                .flex_grow()
                .bg(rgb(0x212529))
                .pb(px(scale(4.)))
                .pt(px(scale(4.)));

            let outer_body = div().flex().flex_col().pl(px(scale(9.0))).child(
                div().flex().bg(rgb(0x495057)).flex_col().child(
                    div()
                        .flex()
                        .flex_col()
                        .child(content)
                        .pl(px(scale(2.)))
                        .pb(px(scale(2.))),
                ),
            );

            result = result.child(outer_body); //body
        }

        if !(self.entry_header_hovered || self.context_menu_hovered) {
            self.show_context_menu = false;
        }

        result.when(self.show_context_menu, |this| {
            this.child(deferred(
                anchored()
                    .anchor(gpui::Corner::TopLeft)
                    .offset(Point {
                        x: self.context_menu_offset_x - px(scale(20.)),
                        y: px(scale(17.)),
                    })
                    .position_mode(gpui::AnchoredPositionMode::Local)
                    .snap_to_window()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .bg(rgb(0x212121))
                            .child(
                                div()
                                    .mb(px(scale(1.)))
                                    .ml(px(scale(1.)))
                                    .mr(px(scale(1.)))
                                    .bg(rgb(0x495057)) // 0x343a40
                                    .text_size(px(scale(8.)))
                                    .when(self.status.hash().is_some(), |this| {
                                        this.child(
                                            div()
                                                .bg(rgb(0x212121))
                                                .mt(px(scale(1.0)))
                                                .child("Copy Hash and format")
                                                .id("copy-hash-and-format")
                                                .hover(|s| s.bg(rgb(0x2f2f2f)))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(
                                                            this.status.hash().unwrap().to_string(),
                                                        ),
                                                    );
                                                })),
                                        )
                                    })
                                    .child(
                                        div()
                                            .bg(rgb(0x212121))
                                            .mt(px(scale(1.0)))
                                            .child("Export")
                                            .id("export")
                                            .hover(|s| s.bg(rgb(0x2f2f2f))),
                                    )
                                    .child(
                                        div()
                                            .bg(rgb(0x212121))
                                            .mt(px(scale(1.0)))
                                            .child("Remove")
                                            .id("remove")
                                            .hover(|s| s.bg(rgb(0x2f2f2f))),
                                    ),
                            )
                            .id("context_menu")
                            .on_hover(cx.listener(|this, hovered, _, cx| {
                                this.context_menu_hovered = *hovered;
                                cx.notify();
                            })),
                    ),
            ))
        })
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
            .ml(px(scale(4.0)))
            .mr(px(scale(4.0)))
            .mt(px(scale(4.0)))
            .overflow_scroll();

        for entry in self.entries.iter() {
            res = match entry {
                Entry::Blob(blob) => res.child(div().mt(px(scale(4.))).child(blob.clone())),
                Entry::Collection(collection) => {
                    res.child(div().mt(px(scale(4.))).child(collection.clone()))
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
    StartDownload {
        entry_id: u64,
    },
    DownloadProgress {
        entry_id: u64,
        progress: DownloadProgressItem,
        max_size: u64,
    },
    ShareCollection {
        entry_id: u64,
        me: bool,
    },
    Share {
        entry_id: u64,
        me: bool,
    },
    SetCollectionHash {
        entry_id: u64,
        hash: Hash,
    },
}

// This should me a Event
impl Clone for Message {
    fn clone(&self) -> Self {
        match self {
            Self::StartDownload { entry_id } => Self::StartDownload {
                entry_id: entry_id.clone(),
            },
            Self::ShareCollection { entry_id, me } => Self::ShareCollection {
                entry_id: *entry_id,
                me: *me,
            },
            Self::Share { entry_id, me } => Self::Share {
                entry_id: *entry_id,
                me: *me,
            },
            _ => todo!(),
        }
    }
}

pub struct BlobsManApp {
    focus_handle: FocusHandle,
    text_input: Entity<TextInput>,
    tree: Entity<Tree>,
    settings: Settings,

    sender: Sender<Message>,

    node: iroh::protocol::Router,
    blobs: iroh_blobs::BlobsProtocol,

    blob_info: HashMap<u64, (Hash, Vec<EndpointAddr>, u64)>,
}

/// The collection meta, stolen from iroh_blobs::format::collection
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct CollectionMeta {
    header: [u8; 13], // Must contain "CollectionV0."
    names: Vec<String>,
}

impl BlobsManApp {
    fn handle_url(&mut self, url: &str, cx: &mut Context<'_, BlobsManApp>) {
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
                                        dbg!(&hashes);
                                        let mut hashes_iterator = hashes.iter();

                                        let meta_hash = hashes_iterator.next().unwrap();

                                        dbg!(
                                            downloader
                                                .download(
                                                    HashAndFormat::raw(meta_hash),
                                                    [ticket.addr().id]
                                                )
                                                .await
                                        );

                                        let mut new_buffer = Vec::<u8>::with_capacity(1024);
                                        let mut reader = blobs.blobs().reader(meta_hash);
                                        reader.read_to_end(&mut new_buffer).await.unwrap();
                                        dbg!(new_buffer.len());

                                        let collection_meta =
                                            dbg!(postcard::from_bytes::<CollectionMeta>(
                                                &new_buffer
                                            ))
                                            .unwrap();
                                        assert_eq!(&collection_meta.header, b"CollectionV0.");

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

                                        for name in collection_meta.names {
                                            let hash = hashes_iterator.next().unwrap();
                                            let id = next_id();

                                            sender
                                                .send(Message::Found {
                                                    collection_id: Some(collection_id),
                                                    entry_id: id,
                                                    hash,
                                                    name,
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

    fn update(&mut self, message: Message, cx: &mut Context<'_, BlobsManApp>) {
        println!("Received: {message:?}");

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
                        match entry {
                            Entry::Blob(entry) => {
                                entry.update(cx, |entry, cx| {
                                    entry.status = EntryStatus::Importing { bytes: progress };
                                    cx.notify();
                                });
                            }
                            Entry::Collection(_) => {}
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

                        match entry {
                            Entry::Blob(entry) => {
                                entry.update(cx, |entry, cx| {
                                    let total =
                                        if let EntryStatus::Importing { bytes } = entry.status {
                                            bytes
                                        } else {
                                            0
                                        };

                                    entry.status = EntryStatus::Active {
                                        hash: temp_tag.hash(),
                                        downloading: HashMap::default(),
                                        uploading: HashMap::default(),
                                        total,
                                    };
                                    cx.notify();
                                });
                            }
                            Entry::Collection(_) => {}
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
                                todo!()
                            };
                            let collection = collection_entity.read(cx);

                            let mut links_and_hashes = Vec::new();
                            for entry in collection.entries.iter() {
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
                                dbg!(&collection);
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
                    cx.subscribe_self::<Message>(move |_, message, _| {
                        sender.try_send(message.clone()).unwrap();
                    })
                    .detach();
                    EntryCollection {
                        id: entry_id,
                        hash,
                        name: name.into(),
                        entries: Vec::default(),
                        expanded: false,
                        show_context_menu: false,
                        track_bounds: cx.new(|_| Bounds::default()),
                        entry_header_hovered: false,
                        context_menu_hovered: false,
                        context_menu_offset_x: px(0.),
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

                let entity = cx.entity();

                if self.settings.auto_download {
                    self.sender
                        .try_send(Message::StartDownload { entry_id })
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
                                cx.subscribe_self::<Message>(move |_, message, _| {
                                    sender.try_send(message.clone()).unwrap();
                                })
                                .detach();
                                EntryBlob {
                                    id: entry_id,
                                    status: EntryStatus::Known { hash },
                                    name: name.into(),
                                    expanded: false,
                                    show_context_menu: false,
                                    track_bounds: cx.new(|_| Bounds::default()),
                                    entry_header_hovered: false,
                                    context_menu_hovered: false,
                                    context_menu_offset_x: px(0.),
                                }
                            });

                            tree.all_entries
                                .insert(entry_id, Entry::Blob(entry.clone()));
                            collection.entries.push(entry);
                        });
                    } else {
                        let entry = Entry::Blob(cx.new(|cx| {
                            let sender = self.sender.clone();
                            cx.subscribe_self::<Message>(move |_, message, _| {
                                sender.try_send(message.clone()).unwrap();
                            })
                            .detach();

                            EntryBlob {
                                id: entry_id,
                                status: EntryStatus::Known { hash },
                                name: name.into(),
                                expanded: false,
                                show_context_menu: false,
                                track_bounds: cx.new(|_| Bounds::default()),
                                entry_header_hovered: false,
                                context_menu_hovered: false,
                                context_menu_offset_x: px(0.),
                            }
                        }));

                        tree.all_entries.insert(entry_id, entry.clone());
                        tree.entries.push(entry.clone());
                    }

                    cx.notify();
                });
            }
            Message::StartDownload { entry_id } => {
                let Some(info) = self.blob_info.get(&entry_id) else {
                    eprintln!("Cannot get info for: {entry_id}");
                    return;
                };

                let node = self.node.clone();
                let blobs = self.blobs.clone();
                let hash = info.0;
                let providers = info.1.clone();
                let sender = self.sender.clone();

                self.tree.update(cx, |tree, cx| {
                    let entry = tree.all_entries.get(&entry_id).unwrap();

                    match entry {
                        Entry::Blob(entity) => {
                            entity.update(cx, |entry, cx| {
                                entry.status = EntryStatus::Active {
                                    hash,
                                    downloading: HashMap::default(),
                                    uploading: HashMap::default(),
                                    total: 0,
                                };
                            });
                        }
                        _ => {}
                    }
                });

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
                        sender
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
            Message::DownloadProgress {
                entry_id,
                progress,
                max_size,
            } => {
                self.tree.update(cx, |tree, cx| {
                    match tree.all_entries.get(&entry_id).unwrap() {
                        Entry::Blob(entry) => {
                            entry.update(cx, |entry, cx| {
                                let EntryBlob {
                                    id,
                                    status:
                                        EntryStatus::Active {
                                            hash,
                                            downloading,
                                            uploading,
                                            total,
                                        },
                                    name,
                                    expanded,
                                    show_context_menu,
                                    track_bounds,
                                    entry_header_hovered,
                                    context_menu_hovered,
                                    context_menu_offset_x,
                                } = entry
                                else {
                                    return;
                                };

                                match progress {
                                    DownloadProgressItem::Error(error) => {}
                                    DownloadProgressItem::TryProvider { id, request } => {
                                        assert_eq!(request.hash, *hash);

                                        downloading
                                            .entry(id)
                                            .or_insert(DownloadStatus { total: 0, speed: 0 });
                                    }
                                    DownloadProgressItem::ProviderFailed { id, request } => {}
                                    DownloadProgressItem::PartComplete { request } => {}
                                    DownloadProgressItem::Progress(bytes) => {
                                        *total = max_size;

                                        let (_, stats) = downloading.iter_mut().next().unwrap();
                                        stats.total = bytes;
                                        cx.notify();
                                    }
                                    DownloadProgressItem::DownloadError => {}
                                }
                            });
                        }
                        Entry::Collection(entity) => {}
                    }
                });
            }
            Message::ShareCollection { entry_id, me } => {
                if me {
                    let Some(info) = self.blob_info.get(&entry_id) else {
                        unreachable!()
                    };

                    let ticket = BlobTicket::new(
                        self.node.endpoint().addr(),
                        info.0,
                        iroh_blobs::BlobFormat::HashSeq,
                    );

                    cx.write_to_clipboard(ClipboardItem::new_string(format!("sendme:{ticket}")));
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

                    cx.write_to_clipboard(ClipboardItem::new_string(format!("sendme:{ticket}")));
                }
            }
            Message::Share { entry_id, me } => {
                if me {
                    let Some(info) = self.blob_info.get(&entry_id) else {
                        unreachable!()
                    };

                    let ticket = BlobTicket::new(
                        self.node.endpoint().addr(),
                        info.0,
                        iroh_blobs::BlobFormat::HashSeq,
                    );

                    cx.write_to_clipboard(ClipboardItem::new_string(format!("iroh_blob:{ticket}")));
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

                    cx.write_to_clipboard(ClipboardItem::new_string(format!("iroh_blob:{ticket}")));
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
        }
    }

    fn add_files(&mut self, files: Vec<PathBuf>, cx: &mut Context<'_, BlobsManApp>) {
        let auto_collection = self.settings.auto_collection;

        self.tree.update(cx, |tree, cx| {
            'auto_collection: {
                if !auto_collection || files.len() < 2 {
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
                for i in 0..min {
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
                        cx.subscribe_self::<Message>(move |_, message: _, _| {
                            sender.try_send(message.clone()).unwrap()
                        })
                        .detach();
                        EntryBlob {
                            status: EntryStatus::Importing { bytes: 0 },
                            id,
                            name,
                            expanded: false,
                            show_context_menu: false,
                            track_bounds: cx.new(|_cx| Bounds::default()),
                            entry_header_hovered: false,
                            context_menu_hovered: false,
                            context_menu_offset_x: px(0.),
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
                    cx.subscribe_self::<Message>(move |_, message: _, _| {
                        sender.try_send(message.clone()).unwrap()
                    })
                    .detach();

                    EntryCollection {
                        id: collection_id,
                        hash: Hash::EMPTY,
                        name,
                        entries,
                        expanded: false,
                        show_context_menu: false,
                        track_bounds: cx.new(|_cx| Bounds::default()),
                        entry_header_hovered: false,
                        context_menu_hovered: false,
                        context_menu_offset_x: px(0.),
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
                    cx.subscribe_self::<Message>(move |_, message: _, _| {
                        sender.try_send(message.clone()).unwrap()
                    })
                    .detach();
                    EntryBlob {
                        status: EntryStatus::Importing { bytes: 0 },
                        id,
                        name,
                        expanded: false,
                        show_context_menu: false,
                        track_bounds: cx.new(|_cx| Bounds::default()),
                        entry_header_hovered: false,
                        context_menu_hovered: false,
                        context_menu_offset_x: px(0.),
                    }
                }));

                tree.entries.push(entry.clone());
                tree.all_entries.insert(id, entry);
            }
        });

        cx.notify();
    }
}

impl Render for BlobsManApp {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(gpui::rgb(0x343a40))
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
                    .bg(rgb(0x212529))
                    .min_h(px(scale(21.0)))
                    .max_h(px(scale(21.0)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(div().min_w(px(scale(2.))))
                    .child(
                        div()
                            .flex_grow()
                            .min_h(px(scale(18.0)))
                            .max_h(px(scale(18.0)))
                            .id("grab_1")
                            .cursor_grab()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .child("Blobs Man")
                            .text_size(px(scale(16.)))
                            .child(div().child("V1").text_size(px(scale(8.))))
                            .id("grab_2")
                            .cursor_grab()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            }),
                    )
                    .child(
                        div()
                            .flex_grow()
                            .min_h(px(scale(18.0)))
                            .max_h(px(scale(18.0)))
                            .id("grab_3")
                            .cursor_grab()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            }),
                    )
                    .child(
                        svg()
                            .text_color(rgb(0xffffff))
                            .path("close")
                            .min_w(px(scale(16.)))
                            .min_h(px(scale(16.)))
                            .max_w(px(scale(16.)))
                            .max_h(px(scale(16.)))
                            .id("close_button")
                            .cursor_pointer()
                            .on_click(|_, window, _| {
                                window.remove_window();
                            }),
                    )
                    .child(div().min_w(px(scale(2.)))),
            )
            .child(
                div()
                    .text_color(rgb(0xffd43b))
                    .text_size(px(scale(10.)))
                    .text_center()
                    .child("Using this application will leak your current IP address!"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_size(px(scale(8.)))
                    .child(
                        div().flex().flex_row().child("Drop files or").child(
                            div()
                                .left(px(scale(4.0)))
                                .child("Browse files")
                                .text_color(rgb(0x1c7ed6))
                                .id("Browse files")
                                .cursor(CursorStyle::PointingHand)
                                .hover(|s| s.text_color(rgb(0x1971c2)))
                                .on_click(cx.listener(|_this, _, _, cx| {
                                    let prompt = cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: cx.can_select_mixed_files_and_dirs(),
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
                    .ml(px(scale(4.)))
                    .mr(px(scale(4.)))
                    .bg(rgb(0x212529))
                    .text_color(rgb(0xffffffff))
                    .text_size(px(scale(14.)))
                    .child(self.text_input.clone())
                    .child(div().h(px(scale(1.0))).flex().flex_grow().bg(rgb(0xB1B2B5)))
                    .child(
                        div()
                            .text_size(px(scale(14.0)))
                            .child("Status: Waiting for Files or URL"),
                    ),
            )
            .text_color(gpui::white())
            .child(self.tree.clone())
            .child(window_resize_frame())
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
                        .w(px(scale(5.)))
                        .h(px(scale(5.)))
                        .cursor_nwse_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::TopLeft);
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_grow()
                        .min_h(px(scale(1.)))
                        .max_h(px(scale(1.)))
                        .cursor_n_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::Top);
                        }),
                )
                .child(
                    div()
                        .w(px(scale(5.)))
                        .h(px(scale(5.)))
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
                        .max_w(px(scale(1.0)))
                        .cursor_ew_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::Left);
                        }),
                )
                .child(div().flex_grow())
                .child(
                    div()
                        .flex_grow()
                        .max_w(px(scale(1.0)))
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
                        .w(px(scale(5.)))
                        .h(px(scale(5.)))
                        .cursor_nesw_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::BottomLeft);
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_grow()
                        .min_h(px(scale(1.)))
                        .max_h(px(scale(1.)))
                        .cursor_s_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::Bottom);
                        }),
                )
                .child(
                    div()
                        .w(px(scale(5.)))
                        .h(px(scale(5.)))
                        .cursor_nwse_resize()
                        .on_mouse_down(MouseButton::Left, |_, w, _| {
                            w.start_window_resize(gpui::ResizeEdge::BottomRight);
                        }),
                ),
        )
}

fn main() {
    let application = gpui::Application::new().with_assets(Assets {});

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

        let init = gpui_tokio::Tokio::spawn(cx, async {
            let endpoint = iroh::Endpoint::builder().bind().await.unwrap();

            let store = iroh_blobs::store::mem::MemStore::new();

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
                            gpui::size(scale(700f32).into(), scale(200f32).into()),
                            cx,
                        ))),
                        window_decorations: Some(WindowDecorations::Client),
                        window_min_size: Some(size(px(scale(350.)), px(scale(100.)))),
                        ..Default::default()
                    },
                    |window, cx| {
                        window.set_window_title("Blobs Man");
                        cx.new::<BlobsManApp>(|cx| {
                            let on_submit = cx.listener(|this, text: &SharedString, _, cx|{
                                this.handle_url(text, cx);
                            });
                            let text_input = cx.new(|text_input_cx| TextInput {
                                id: "URL_input".into(),
                                focus_handle: text_input_cx.focus_handle(),
                                content: SharedString::new(""),
                                placeholder: SharedString::new(
                                    "Get URL: sendme:6wsanfumhtkffsmsamckhbk2sruapvq6oi5rjso4t36ztthqas7a====",
                                ),
                                placeholder_color: rgb(0xadb5bd).into(),
                                selected_range: 0..0,
                                selection_reversed: false,
                                marked_range: None,
                                last_layout: None,
                                last_bounds: None,
                                is_selecting: false,
                                scroll_handle: ScrollHandle::new(),
                                on_submit: Box::new(on_submit)
                            });

                            let (sender, mut receiver) = tokio::sync::mpsc::channel::<Message>(1024);

                            cx.spawn(async move |this, cx|{
                                while let Some(message) = receiver.recv().await{
                                    if let Some(app) = this.upgrade()
                                        {
                                            app.update(cx, |this, cx|{ this.update(message, cx); });
                                        }
                                }
                            }).detach();
                            BlobsManApp {
                                text_input,
                                focus_handle: cx.focus_handle(),
                                tree: cx.new(|_cx| Tree{ entries: vec![], all_entries: HashMap::default() } ),
                                settings: Settings{
                                    auto_collection: true,
                                    auto_download: false,
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
        0..1000 => format!("{bytes}b"),
        1000..1_000_000 => format!("{}Kb", bytes / 1000),
        1_000_000..1_000_000_000 => format!("{}Mb", bytes / 1_000_000),
        _ => format!("{}Gb", bytes / 1_000_000_000),
    }
}

const fn scale(input: f32) -> f32 {
    input * 2.
}
