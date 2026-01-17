use std::path::{Path, PathBuf};

use blobsman_graphics::{
    gpui::{
        self, AssetSource, ClipboardItem, Div, PathPromptOptions, ScrollHandle, WindowDecorations,
        anchored, deferred, svg,
    },
    iroh_blobs::Hash,
    tokio,
};
use gpui::{
    App, AppContext, Bounds, CursorStyle, ElementId, Entity, ExternalPaths, FocusHandle,
    GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, ParentElement, Pixels,
    Point, Render, SharedString, Style, Styled, Window, WindowOptions, div, prelude::*, px, rgb,
    size,
};

use crate::text_input::{
    Backspace, Copy, Cut, Delete, End, Home, Left, Paste, Right, SelectAll, SelectLeft,
    SelectRight, TextInput,
};

mod text_input;

const SVG_ARROW_RIGHT: &str =
    include_str!("../assets/arrow_right_alt_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_ARROW_DOWNWARD: &str =
    include_str!("../assets/arrow_downward_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_CLOSE: &str = include_str!("../assets/close_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");
const SVG_SHARE: &str = include_str!("../assets/share_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg");

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
            _ => Ok(None),
        }
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(vec![])
    }
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

pub enum Entry {
    Blob(Entity<EntryBlob>),
    Collection(Entity<EntryCollection>),
}

pub struct EntryBlob {
    hash: Hash,
    name: SharedString,

    expanded: bool,
    show_context_menu: bool,
    track_bounds: Entity<Bounds<Pixels>>,
    entry_header_hovered: bool,
    context_menu_hovered: bool,
    context_menu_offset_x: Pixels,
}

pub struct EntryCollection {
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
                                .max_h(px(scale(14.))),
                        )
                        .child(self.name.clone())
                        .child(div().flex().flex_grow())
                        .child(
                            svg()
                                .flex()
                                .text_color(rgb(0xffffff))
                                .path(SharedString::new_static("share"))
                                .min_w(px(scale(14.)))
                                .min_h(px(scale(14.)))
                                .max_w(px(scale(14.)))
                                .max_h(px(scale(14.))),
                        )
                        .child(div().min_w(px(scale(2.)))),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.expanded = !this.expanded;
                    cx.notify();
                }))
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
                                .max_h(px(scale(14.))),
                        )
                        .child(self.name.clone())
                        .child(div().flex().flex_grow())
                        .child(
                            svg()
                                .flex()
                                .text_color(rgb(0xffffff))
                                .path(SharedString::new_static("share"))
                                .min_w(px(scale(14.)))
                                .min_h(px(scale(14.)))
                                .max_w(px(scale(14.)))
                                .max_h(px(scale(14.))),
                        )
                        .child(div().min_w(px(scale(2.)))),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.expanded = !this.expanded;
                    cx.notify();
                }))
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

pub struct Tree {
    entries: Vec<Entry>,
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
}

pub struct BlobsManApp {
    focus_handle: FocusHandle,
    text_input: Entity<TextInput>,
    tree: Entity<Tree>,
    settings: Settings,
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

                add_files(this, files, cx);
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
                                            _ = this.update(cx, move |this, cx| {
                                                let files = files
                                                    .iter()
                                                    .map(|path| get_files(path))
                                                    .reduce(|mut acc, e| {
                                                        acc.extend(e);
                                                        acc
                                                    })
                                                    .unwrap_or_default();

                                                add_files(this, files, cx);
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

#[tokio::main]
async fn main() {
    // let endpoint = iroh::Endpoint::builder().bind().await.unwrap();

    // let store = iroh_blobs::store::mem::MemStore::new();

    // let (sender, mut receiver) = iroh_blobs::provider::events::EventSender::channel(
    //     1024,
    //     iroh_blobs::provider::events::EventMask::DEFAULT,
    // );
    // let _task_status_receiver = tokio::spawn(async move {
    //     while let Some(msg) = receiver.recv().await {
    //         println!("EVENT: {msg:?}");
    //     }
    // });
    // let blobs = iroh_blobs::BlobsProtocol::new(&store, Some(sender));

    // let node = iroh::protocol::RouterBuilder::new(endpoint)
    //     .accept(iroh_blobs::ALPN, blobs.clone())
    //     .spawn();
    // println!("NODE ID: {}", node.endpoint().id());

    let application = gpui::Application::new().with_assets(Assets {});

    // gpui_component::v_virtual_list(view, id, item_sizes, f)

    // gpui_component::list::List

    application.run(|cx| {
        // gpui_component::init(cx);

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
        ]);

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
                cx.new(|cx| {
                    let text_input = cx.new(|cx| TextInput {
                        id: "URL_input".into(),
                        focus_handle: cx.focus_handle(),
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
                    });
                    BlobsManApp {
                        text_input,
                        focus_handle: cx.focus_handle(),
                        tree: cx.new(|_cx| Tree{ entries: vec![] } ),
                        settings: Settings{
                            auto_collection: true
                        },
                    }
                })
            },
        )
        .expect("Cannot create Main Window");
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

fn add_files(this: &mut BlobsManApp, files: Vec<PathBuf>, cx: &mut Context<'_, BlobsManApp>) {
    let auto_collection = this.settings.auto_collection;

    this.tree.update(cx, move |tree, cx| {
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

            let mut entries = Vec::new();
            for file in files.iter() {
                let mut filename = PathBuf::default();

                for component in file.components().skip(common_index + 1) {
                    filename = filename.join(component);
                }

                let name = SharedString::from(filename.to_string_lossy().to_string());

                entries.push(cx.new(|cx| EntryBlob {
                    hash: Hash::EMPTY,
                    name,
                    expanded: false,
                    show_context_menu: false,
                    track_bounds: cx.new(|_cx| Bounds::default()),
                    entry_header_hovered: false,
                    context_menu_hovered: false,
                    context_menu_offset_x: px(0.),
                }));
            }

            let mut filename = PathBuf::default();

            for (i, component) in files[0].components().enumerate() {
                if i == common_index + 1 {
                    break;
                }
                filename = filename.join(component);
            }

            let name = SharedString::from(filename.to_string_lossy().to_string());

            tree.entries
                .push(Entry::Collection(cx.new(|cx| EntryCollection {
                    hash: Hash::EMPTY,
                    name,
                    entries,
                    expanded: false,
                    show_context_menu: false,
                    track_bounds: cx.new(|_cx| Bounds::default()),
                    entry_header_hovered: false,
                    context_menu_hovered: false,
                    context_menu_offset_x: px(0.),
                })));

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
            tree.entries.push(Entry::Blob(cx.new(|cx| EntryBlob {
                hash: Hash::EMPTY,
                name,
                expanded: false,
                show_context_menu: false,
                track_bounds: cx.new(|_cx| Bounds::default()),
                entry_header_hovered: false,
                context_menu_hovered: false,
                context_menu_offset_x: px(0.),
            })));
        }
    });

    cx.notify();
}

const fn scale(input: f32) -> f32 {
    input * 2.
}
