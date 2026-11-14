//! Notifications: a crashed domain, a long job that finished.
//!
//! `desktop` runs `notify-send`; `osc9` / `osc777` write the terminal's
//! notification escape (works over SSH and in tmux with passthrough).

use std::io::Write;

/// How a notification is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Method {
    Desktop,
    Osc9,
    Osc777,
    Off,
}

/// Resolve `notifications.method` (`auto` looks at the session).
pub(super) fn method(name: &str, env: impl Fn(&str) -> Option<String>, has_notify_send: bool) -> Method {
    match name {
        "desktop" => Method::Desktop,
        "osc9" => Method::Osc9,
        "osc777" => Method::Osc777,
        "off" => Method::Off,
        _ => {
            let graphical = env("WAYLAND_DISPLAY").is_some() || env("DISPLAY").is_some();
            let remote = env("SSH_CONNECTION").is_some() || env("SSH_TTY").is_some();
            if graphical && !remote && has_notify_send {
                Method::Desktop
            } else {
                Method::Osc9
            }
        }
    }
}

fn notify_send_in_path() -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("notify-send").is_file()))
}

/// Control characters out; `;` too for OSC 777, whose fields it separates.
fn clean(text: &str, method: Method) -> String {
    let text = crate::model::sanitize(text).replace('\u{fffd}', " ");
    if method == Method::Osc777 {
        text.replace(';', ",")
    } else {
        text
    }
}

#[cfg(test)]
thread_local! {
    /// Notifications sent by tests (nothing is delivered).
    pub(super) static SENT: std::cell::RefCell<Vec<(String, String)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Send one notification with the configured method.
pub(super) fn send(config: &crate::config::Notifications, title: &str, body: &str) {
    #[cfg(test)]
    {
        let _ = config;
        SENT.with(|s| s.borrow_mut().push((title.to_string(), body.to_string())));
        return;
    }
    #[allow(unreachable_code)]
    {
        let method = method(&config.method, |k| std::env::var(k).ok(), notify_send_in_path());
        let (title, body) = (clean(title, method), clean(body, method));
        match method {
            Method::Off => {}
            Method::Desktop => {
                super::external::detached_run(&["notify-send", "--app-name=virsh-tui", &title, &body]);
            }
            Method::Osc9 | Method::Osc777 => {
                let seq = if method == Method::Osc9 {
                    format!("\x1b]9;{title}: {body}\x1b\\")
                } else {
                    format!("\x1b]777;notify;{title};{body}\x1b\\")
                };
                let mut out = std::io::stdout();
                let _ = out.write_all(seq.as_bytes());
                let _ = out.flush();
            }
        }
    }
}

/// `Crashed` lifecycle events (`Crashed …` or `Stopped Crashed`).
pub(super) fn is_crash(ev: &crate::model::LibvirtEvent) -> bool {
    ev.kind == "lifecycle"
        && (ev.event == "Crashed" || (ev.event == "Stopped" && ev.detail.contains("Crashed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prefers_the_desktop_only_locally() {
        let env = |vars: &'static [&'static str]| move |k: &str| vars.contains(&k).then(|| String::from("x"));
        assert_eq!(method("auto", env(&["WAYLAND_DISPLAY"]), true), Method::Desktop);
        assert_eq!(method("auto", env(&["WAYLAND_DISPLAY"]), false), Method::Osc9);
        assert_eq!(
            method("auto", env(&["DISPLAY", "SSH_CONNECTION"]), true),
            Method::Osc9
        );
        assert_eq!(method("auto", env(&[]), true), Method::Osc9);
        assert_eq!(method("osc777", env(&[]), true), Method::Osc777);
        assert_eq!(method("off", env(&["DISPLAY"]), true), Method::Off);
    }

    #[test]
    fn text_cannot_escape_the_sequence() {
        assert_eq!(clean("vm\x1b]52;x\x07", Method::Osc9), "vm ]52;x ");
        assert_eq!(clean("a;b", Method::Osc777), "a,b");
    }

    #[test]
    fn crash_events() {
        let ev = |event: &str, detail: &str| crate::model::LibvirtEvent {
            timestamp: String::new(),
            object: "vm".into(),
            event: event.into(),
            detail: detail.into(),
            kind: "lifecycle".into(),
            scope: "domain".into(),
        };
        assert!(is_crash(&ev("Crashed", "Panicked")));
        assert!(is_crash(&ev("Stopped", "Crashed")));
        assert!(!is_crash(&ev("Stopped", "Shutdown")));
    }
}
