use std::{
    collections::{BTreeMap, BTreeSet},
    path::{self, PathBuf},
    rc::Rc,
    sync::Arc,
};

use gpui::{
    AppContext, Edges, ElementId, Empty, EntityId, ExternalPaths, FileDropEvent, ParentElement,
    Render, SharedString, Style, Styled, WindowOptions, colors::DefaultColors, div, prelude::*, px,
};
use iroh::{discovery::Discovery, protocol::DynProtocolHandler};

pub struct Simple {
    state: SimpleState,
}

#[derive(PartialEq, Eq)]
enum SimpleState {
    Download,
    Upload,
}

#[derive(Default)]
struct FilesToTransfer(BTreeSet<Rc<path::Path>>);
impl gpui::Global for FilesToTransfer {}

impl Render for Simple {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let entity_id = cx.entity_id();

        let files_to_transfer = cx.default_global::<FilesToTransfer>();

        let count = files_to_transfer.0.len();
        let mut element_idx_with_max_size = 0usize;
        {
            let mut len = 0;
            for (i, path) in files_to_transfer.0.iter().enumerate() {
                let l = path.to_string_lossy().len();
                if l > len {
                    element_idx_with_max_size = i;
                    len = l;
                }
            }
        }

        let topbar = div()
            .flex()
            .min_h_8()
            .max_h_8()
            .w_full()
            .bg(gpui::rgb(0x202020))
            .id("topbar")
            // .on_mouse_down(gpui::MouseButton::Left, |_, w, _| w.start_window_move())
            .child(
                div()
                    .flex()
                    .flex_grow()
                    .text_color(gpui::white())
                    .child(
                        div()
                            .px_4()
                            .border_color(gpui::rgb(0x216621))
                            .bg(if self.state == SimpleState::Download {
                                gpui::rgb(0x216621)
                            } else {
                                gpui::rgb(0x212121)
                            })
                            .border_1()
                            .rounded(px(30.))
                            .hover(|s| s.bg(gpui::rgb(0x217721)))
                            .child("Download")
                            .id("button:simple:tab_download")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state = SimpleState::Download;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .px_4()
                            .border_color(gpui::rgb(0x216621))
                            .border_1()
                            .bg(if self.state == SimpleState::Upload {
                                gpui::rgb(0x216621)
                            } else {
                                gpui::rgb(0x212121)
                            })
                            .rounded(px(30.))
                            .hover(|s| s.bg(gpui::rgb(0x217721)))
                            .child("Upload")
                            .id("button:simple:tab_upload")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state = SimpleState::Upload;
                                cx.notify();
                            })),
                    )
                    .justify_center(),
            );

        let body = match self.state {
            SimpleState::Download => div().into_any_element(),
            SimpleState::Upload => div()
                .flex()
                .flex_col()
                .flex_grow()
                .overflow_hidden()
                .bg(gpui::rgb(0x404040))
                .id("simple:drop_zone")
                .on_drop::<ExternalPaths>(move |data, _, cx| {
                    println!("Drop: {data:?}");

                    cx.update_global::<FilesToTransfer, _>(|files, cx| {
                        files
                            .0
                            .extend(data.paths().iter().cloned().map(|i| i.into()));
                        cx.notify(entity_id);
                    });
                })
                .drag_over::<ExternalPaths>(|s, _, _, _| s.bg(gpui::rgb(0x3030aa)))
                .child(
                    div()
                        .flex()
                        .child("Drop files or")
                        .text_color(gpui::white())
                        .justify_center(),
                )
                .child(
                    div()
                        .flex()
                        .child(
                            div()
                                .flex()
                                .child("Browse Files")
                                .bg(gpui::rgb(0x202070))
                                .hover(|s| s.bg(gpui::rgb(0x4040a0)))
                                .text_color(gpui::white())
                                .justify_center()
                                .id("simple:browse_files")
                                .on_click(move |_, _, cx| {
                                    let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: true,
                                        prompt: Some("Add files to transfer".into()),
                                    });

                                    cx.spawn(async move |cx| {
                                        let res = receiver.await;

                                        if let Ok(Ok(Some(files))) = res {
                                            cx.update_global::<FilesToTransfer, _>(
                                                move |files_to_transfer, cx| {
                                                    files_to_transfer.0.extend(
                                                        files.into_iter().map(|i| i.into()),
                                                    );
                                                    cx.notify(entity_id);
                                                },
                                            )
                                            .expect("The app is already dead!");
                                        }
                                    })
                                    .detach();
                                }),
                        )
                        .justify_center(),
                )
                .child(
                    div()
                        .flex()
                        .m(px(8.))
                        .p(px(8.))
                        .flex_grow()
                        .bg(gpui::rgb(0xff0000))
                        .overflow_hidden()
                        .child(
                            gpui::uniform_list("paths", count, move |range, window, cx| {
                                cx.read_global::<FilesToTransfer, _>(|files, _| {
                                    let mut out = Vec::with_capacity(range.len());
                                    let mut iterator = files.0.iter().skip(range.start);
                                    let mut id = range.start;
                                    for _ in range {
                                        let file = iterator.next().expect("WTF").clone();
                                        out.push(
                                            div()
                                                .flex()
                                                .flex_nowrap()
                                                .text_color(gpui::white())
                                                .child(
                                                    // file.file_name()
                                                    //     .map(|filename| {
                                                    //         filename.to_string_lossy().to_string()
                                                    //     })
                                                    //     .unwrap_or_else(|| {
                                                    //         file.to_string_lossy().to_string()
                                                    //     }),
                                                    file.to_string_lossy().to_string(),
                                                )
                                                .id(ElementId::NamedInteger(
                                                    "simple:files".into(),
                                                    id as u64,
                                                ))
                                                .on_click(move |_, _, cx| {
                                                    cx.update_global::<FilesToTransfer, _>(
                                                        |files, cx| {
                                                            files.0.remove(&file);
                                                            cx.notify(entity_id);
                                                        },
                                                    );
                                                })
                                                .hover(|s| s.bg(gpui::red())),
                                        );
                                        id += 1;
                                    }
                                    out
                                })
                            })
                            .w_full()
                            .with_width_from_item(Some(element_idx_with_max_size))
                            .with_sizing_behavior(gpui::ListSizingBehavior::Infer)
                            .with_horizontal_sizing_behavior(
                                gpui::ListHorizontalSizingBehavior::Unconstrained,
                            )
                            .bg(gpui::black()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_center()
                        .text_color(gpui::white())
                        .mb_1()
                        .child(
                            div()
                                .bg(gpui::rgb(0x216621))
                                .hover(|s| s.bg(gpui::rgb(0x218821)))
                                .rounded(px(10.))
                                .px_2()
                                .child("Start Transfer"),
                        ),
                )
                .into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(gpui::rgb(0x212121))
            .child(topbar)
            .child(body)
    }
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

    let application = gpui::Application::new();

    // gpui_component::v_virtual_list(view, id, item_sizes, f)

    // gpui_component::list::List

    application.run(|cx| {
        // gpui_component::init(cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                    None,
                    gpui::size(400f32.into(), 300f32.into()),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title("Blobs Man");
                cx.new(|_| Simple {
                    state: SimpleState::Upload,
                })
            },
        )
        .expect("Cannot create Main Window");
    });
}
