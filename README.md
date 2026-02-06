# MuzzMan

Application made to transfer files.

Should only be used with persons you trust, your IP address will be exposed by using this application!

Go to [Leaked IP Address](#leaked-ip-address) if you worry about your IP address!

The user interface should be as simple as possible!

To send files you drag the files you want to share then click share, now you send that text to the person you want to obtain that files.

You need to keep the application open until that person finishes to download them!

The files to transfer and the downloaded files will live in the RAM Memory!

So currently you need as much Memory as the files you want to send/receive!

# Installation

You can download the latest version for Windows or linux from Releases!

# Motivation

I wanted to be able to send projects that I'm working on, with my friends directly.

When they recorded an video and I needed to edit that video, then sending back using Google Drive or Mega Gz was annoying. 

Also using permanent drive storage for a file transfer feelt really wasteful!

# Leaked IP Address

If you are not a popular public person you should not worry about your IP address.

An attacker could find your general location country or city, more information could get from your Internet Service Provider.

And if you are a public person and you have enimies or trolls that are willing to pay for you to not have internet access for some minutes.

And in case you don't know, any online services you use, knows you IP address also some multiplayer games leek your IP address.

If your really need to keep your IP address private you can use any VPN service, but now you also need to trust them and their ISP.

# Credits

All the networking logic is done by [iroh](https://github.com/n0-computer/iroh) and [iroh-blobs](https://github.com/n0-computer/iroh-blobs) from the [n0-computer](https://n0.computer/) team.

The Graphical application was made possible by [gpui](https://www.gpui.rs/) from [Zed](https://zed.dev/).

# Tries with other libraries

For the graphical application I tried using [eframe](https://docs.rs/eframe/0.33.3/eframe/) and [iced](https://docs.rs/iced/0.14.0/iced/)
Both eframe and iced failed because of lack of drag and drop, and also the lack of dialogs for saving and opening files.

[libp2p 0.53.2](https://docs.rs/libp2p/0.53.2/libp2p/) failed because was to heavy and inconsistent, also had a lot of memory leaks, I'm not sure if that was the library version.


# Unrelated

libp2p is trying to do too many things, at least the Rust version!

I really like how iced separates the logic from the graphics!

I really like how easy is to prototype something with egui from eframe, I love immediate mode!

I implemented multi windows support in egui in the `Multiple viewports/windows` pull, at least I did all the ground work, thanks alot to Emil Ernerfeldt.

I tried to make possible drag source in winit in private, but how drag sources works on X11, Wayland and Windows are completely opposed, I should have documented my adventure.

Even today the clipboard on wayland has problems.

My current hack to copy stuff to clipboard in gpui is:
```rust
fn write_to_clipboard(cx: &mut App, item: ClipboardItem) {
    cx.spawn(async move |cx| {
        let mut written = false;

        while !written {
            cx.update(|cx| {
                if cx.read_from_clipboard().as_ref() == Some(&item) {
                    written = true;
                } else {
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
```
But even this only works 99% of the time, the 1% of the time it doesn't work, possible gpui has his own clipboard state,
and this only makes the copy work better because the `read_from_clipboard` flushes the keyboard?

Is impossible to reproduce consistently, but `cx.write_to_clipboard` will always in my case need to be called twice to work, the first time dose nothing.

There is no issue opened in gpui for this, it can be and KDE issue, I don't know, I only open issues if I can reproduce the issue 100% of the time.

PS: Everything is made of shit, we only polishing the shit to not look like shit, but with Software, because is only logic we ware able to make something that was not shit, but was cheaper to make shit, is hard to not make shit when your are made of shit!
