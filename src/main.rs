use as_raw_xcb_connection::ValidConnection;
use pam::Client;
use xcb::Xid;
use xcb::x;
use xkbcommon::xkb;
use xkbcommon::xkb::keysyms;

fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

fn authenticate(username: &str, password: &str) -> bool {
    let mut client = match Client::with_password("login") {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .conversation_mut()
        .set_credentials(username, password);
    client.authenticate().is_ok() && client.open_session().is_ok()
}

fn main() -> xcb::Result<()> {
    let username = get_username();
    let mut password_buf = String::new();

    let (conn, screen_num) = xcb::Connection::connect(None)?;

    let xkb_cookie = conn.send_request(&xcb::xkb::UseExtension {
        wanted_major: xkb::x11::MIN_MAJOR_XKB_VERSION,
        wanted_minor: xkb::x11::MIN_MINOR_XKB_VERSION,
    });
    let xkb_reply: xcb::xkb::UseExtensionReply = conn.wait_for_reply(xkb_cookie)?;
    if !xkb_reply.supported() {
        eprintln!("XKB extension not supported");
        return Ok(());
    }

    // make keymap
    let raw = conn.get_raw_conn();
    let raw_conn = unsafe { ValidConnection::new(raw as *mut _) };

    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let device_id = xkb::x11::get_core_keyboard_device_id(&raw_conn);
    let keymap = xkb::x11::keymap_new_from_device(
        &context,
        &raw_conn,
        device_id,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    );
    let mut state = xkb::x11::state_new_from_device(&keymap, &raw_conn, device_id);

    let setup = conn.get_setup();
    let screen = setup.roots().nth(screen_num as usize).unwrap();

    let window: x::Window = conn.generate_id();

    let cookie = conn.send_request_checked(&x::CreateWindow {
        depth: x::COPY_FROM_PARENT as u8,
        wid: window,
        parent: screen.root(),
        x: 0,
        y: 0,
        width: screen.width_in_pixels(),
        height: screen.height_in_pixels(),
        border_width: 0,
        class: x::WindowClass::InputOutput,
        visual: screen.root_visual(),
        value_list: &[
            x::Cw::BackPixel(screen.black_pixel()),
            x::Cw::OverrideRedirect(true),
            x::Cw::EventMask(
                x::EventMask::EXPOSURE
                    | x::EventMask::KEY_PRESS
                    | x::EventMask::BUTTON_PRESS
                    | x::EventMask::POINTER_MOTION,
            ),
        ],
    });
    conn.check_request(cookie)?;

    conn.send_request(&x::MapWindow { window });
    conn.flush()?;

    let grab_keyboard = conn.send_request(&x::GrabKeyboard {
        owner_events: false,
        grab_window: window,
        time: x::CURRENT_TIME,
        pointer_mode: x::GrabMode::Async,
        keyboard_mode: x::GrabMode::Async,
    });

    let reply = conn.wait_for_reply(grab_keyboard)?;
    if reply.status() != x::GrabStatus::Success {
        eprintln!("Failed to grab keyboard");
        return Ok(());
    }

    let grab_pointer = conn.send_request(&x::GrabPointer {
        owner_events: false,
        grab_window: window,
        event_mask: x::EventMask::BUTTON_PRESS | x::EventMask::POINTER_MOTION,
        pointer_mode: x::GrabMode::Async,
        keyboard_mode: x::GrabMode::Async,
        confine_to: x::Window::none(),
        cursor: x::Cursor::none(),
        time: x::CURRENT_TIME,
    });

    let reply = conn.wait_for_reply(grab_pointer)?;
    if reply.status() != x::GrabStatus::Success {
        eprintln!("Failed to grab pointer");
        return Ok(());
    }

    // We enter the main event loop
    loop {
        match conn.wait_for_event()? {
            xcb::Event::X(x::Event::KeyPress(ev)) => {
                let keycode = ev.detail();
                state.update_key(keycode.into(), xkb::KeyDirection::Down);

                let keysym = state.key_get_one_sym(keycode.into());
                let c = xkb::keysym_to_utf8(keysym);
                match keysym.raw() {
                    keysyms::KEY_Return => {
                        if authenticate(&username, &password_buf) {
                            conn.send_request(&x::UngrabKeyboard {
                                time: x::CURRENT_TIME,
                            });
                            conn.send_request(&x::UngrabPointer {
                                time: x::CURRENT_TIME,
                            });
                            conn.flush()?;
                            break Ok(());
                        } else {
                            eprintln!("Authentication failed");
                            password_buf.clear();
                        }
                    }

                    keysyms::KEY_BackSpace => {
                        password_buf.pop();
                    }

                    keysyms::KEY_Escape => {
                        password_buf.clear();
                    }

                    _ => {
                        if !c.is_empty() {
                            password_buf.push_str(&c);
                        }
                    }
                }
            }
            xcb::Event::X(x::Event::KeyRelease(ev)) => {
                let keycode = ev.detail();
                state.update_key(keycode.into(), xkb::KeyDirection::Up);
            }
            _ => {}
        }
    }
}
