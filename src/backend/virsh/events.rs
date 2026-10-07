//! Libvirt event streams: long-running `virsh event --all`, `net-event` and
//! `pool-event` (one process per network/pool event type) with `--loop --timestamp`.

use tokio::sync::mpsc;

/// Parse one `virsh event --timestamp` line:
/// `2026-10-06 14:31:55.123+0000: event 'lifecycle' for domain 'name': Started Booted`.
/// Network and pool events do not quote the name:
/// `… event 'lifecycle' for storage pool default: Started`,
/// `… event 'refresh' for storage pool default`, `… for network default: Defined`.
///
/// The timestamp itself contains `:`, so the line is split on `: event '`.
/// Only `lifecycle` tails are `<Event> <Detail>`; every other type gets a short
/// label from its type and a readable detail (see [`describe`]).
pub fn parse_event_line(line: &str) -> Option<crate::model::LibvirtEvent> {
    let (ts, rest) = line.split_once(": event '")?;
    let (kind, rest) = rest.split_once('\'')?;
    let rest = rest.strip_prefix(" for ")?;
    let (obj_kind, name, tail) = if let Some(r) = rest.strip_prefix("storage pool ") {
        let (name, tail) = r.split_once(": ").unwrap_or((r, ""));
        ("pool", name, tail)
    } else if let Some(r) = rest.strip_prefix("network ") {
        let (name, tail) = r.split_once(": ").unwrap_or((r, ""));
        ("network", name, tail)
    } else {
        let (obj_kind, r) = rest.split_once(" '")?;
        let (name, tail) = r.split_once('\'')?;
        (obj_kind, name, tail)
    };
    let tail = tail.strip_prefix(':').unwrap_or(tail).trim();
    let (event, detail) = describe(kind, tail);
    // Keep the clock time only (HH:MM:SS) for display.
    let time = ts
        .split_whitespace()
        .nth(1)
        .and_then(|t| t.get(..8))
        .unwrap_or(ts)
        .to_string();
    let clean = crate::model::sanitize;
    Some(crate::model::LibvirtEvent {
        timestamp: clean(&time),
        object: clean(name),
        event: clean(&event),
        detail: clean(&detail),
        kind: clean(kind),
        scope: clean(obj_kind),
    })
}

/// Value of `key: 'value'` inside an event tail.
fn quoted<'a>(tail: &'a str, key: &str) -> Option<&'a str> {
    let start = tail.find(&format!("{key}: '"))? + key.len() + 3;
    let len = tail[start..].find('\'')?;
    Some(&tail[start..start + len])
}

/// (event label, detail) for an event type and its tail.
///
/// Formats from virsh's printers, e.g. `rtc-change: -1`,
/// `agent-lifecycle: state: 'connected' reason: 'channel event'`,
/// `channel-lifecycle: channel name: 'org.qemu.guest_agent.0' state: …`,
/// `balloon-change: 4194304`, `tray-change` → ` disk 'sda' opened`.
fn describe(kind: &str, tail: &str) -> (String, String) {
    let state_reason = |tail: &str| match (quoted(tail, "state"), quoted(tail, "reason")) {
        (Some(s), Some(r)) => format!("{s} ({r})"),
        (Some(s), None) => s.to_string(),
        _ => tail.to_string(),
    };
    match kind {
        "lifecycle" => {
            let mut parts = tail.splitn(2, ' ');
            let event = parts.next().filter(|e| !e.is_empty()).unwrap_or(kind);
            (event.to_string(), parts.next().unwrap_or("").to_string())
        }
        "rtc-change" => (String::from("rtc"), format!("guest clock offset {tail} s")),
        "agent-lifecycle" => (String::from("agent"), state_reason(tail)),
        "channel-lifecycle" => {
            let channel = quoted(tail, "name").unwrap_or("?");
            (
                String::from("channel"),
                format!("{channel} {}", state_reason(tail)),
            )
        }
        "balloon-change" => match tail.parse::<u64>() {
            Ok(kib) => (String::from("balloon"), format!("{} MiB", kib / 1024)),
            Err(_) => (String::from("balloon"), tail.to_string()),
        },
        _ => {
            let label = kind.strip_suffix("-change").unwrap_or(kind);
            (label.to_string(), tail.to_string())
        }
    }
}

/// Spawn the event stream; restarts with backoff if virsh dies.
pub fn subscribe(uri: &str) -> mpsc::Receiver<crate::model::LibvirtEvent> {
    let (tx, rx) = mpsc::channel(512);
    // Domain, network and storage-pool events share one channel. `net-event`
    // and `pool-event` have no `--all`: they need one process per event type.
    let streams: [(&'static str, &'static [&'static str]); 5] = [
        ("event", &["--all"]),
        ("net-event", &["--event", "lifecycle"]),
        ("net-event", &["--event", "metadata-change"]),
        ("pool-event", &["--event", "lifecycle"]),
        ("pool-event", &["--event", "refresh"]),
    ];
    for (sub, filter) in streams {
        let uri = uri.to_string();
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut failures: u32 = 0;
            loop {
                if tx.is_closed() {
                    break;
                }
                match stream_once(&uri, sub, filter, &tx).await {
                    Ok(()) => failures = 0,
                    Err(e) => {
                        failures += 1;
                        tracing::warn!("{sub} stream: {e}");
                    }
                }
                // Always back off before respawning: a stream that ends immediately
                // (daemon down, bad URI) must not turn into a busy loop.
                let secs = crate::app::backoff::delay_secs(failures.max(1));
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
        });
    }
    rx
}

async fn stream_once(
    uri: &str,
    sub: &str,
    filter: &[&str],
    tx: &mpsc::Sender<crate::model::LibvirtEvent>,
) -> color_eyre::Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;
    let mut child = Command::new("virsh")
        .arg("-c")
        .arg(uri)
        .arg(sub)
        .args(filter)
        .args(["--loop", "--timestamp"])
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        // Never inherit stderr: it would draw over the TUI.
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| color_eyre::eyre::eyre!("no stdout"))?;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(ev) = parse_event_line(&line)
            && tx.send(ev).await.is_err()
        {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_event_line;

    #[test]
    fn parses_real_timestamped_lifecycle_line() {
        let ev = parse_event_line(
            "2026-10-06 14:31:55.123+0000: event 'lifecycle' for domain 'k8s-worker-02': Started Booted",
        )
        .unwrap();
        assert_eq!(ev.timestamp, "14:31:55");
        assert_eq!(ev.object, "k8s-worker-02");
        assert_eq!(ev.event, "Started");
        assert_eq!(ev.detail, "Booted");
        assert_eq!(ev.kind, "lifecycle");
        assert_eq!(ev.scope, "domain");
    }

    #[test]
    fn parses_event_without_detail_and_quotes_in_tail() {
        let ev =
            parse_event_line("2026-10-06 14:32:07.000+0000: event 'reboot' for domain 'arch-dev'").unwrap();
        assert_eq!(ev.event, "reboot");
        assert_eq!(ev.detail, "");
        let ev = parse_event_line(
            "2026-10-06 14:32:07.000+0000: event 'tray-change' for domain 'a': disk 'sda' opened",
        )
        .unwrap();
        assert_eq!(ev.object, "a");
        assert_eq!(ev.event, "tray");
        assert_eq!(ev.detail, "disk 'sda' opened");
        assert!(parse_event_line("garbage").is_none());
    }

    /// Pool and network lines (captured from libvirt 11) do not quote the name;
    /// they were all dropped, and the streams were started with an `--all`
    /// that net-event/pool-event reject.
    #[test]
    fn pool_and_network_events() {
        let ev = parse_event_line(
            "2026-10-07 12:47:14.767+0000: event 'lifecycle' for storage pool vt-test-pool: Started",
        )
        .unwrap();
        assert_eq!(
            (ev.scope.as_str(), ev.object.as_str(), ev.event.as_str()),
            ("pool", "vt-test-pool", "Started")
        );
        let ev =
            parse_event_line("2026-10-07 12:47:14.781+0000: event 'refresh' for storage pool vt-test-pool")
                .unwrap();
        assert_eq!(
            (ev.scope.as_str(), ev.event.as_str(), ev.detail.as_str()),
            ("pool", "refresh", "")
        );
        let ev = parse_event_line(
            "2026-10-07 12:47:14.828+0000: event 'lifecycle' for network vt-test-net: Defined",
        )
        .unwrap();
        assert_eq!(
            (ev.scope.as_str(), ev.object.as_str(), ev.event.as_str()),
            ("network", "vt-test-net", "Defined")
        );
    }

    /// Lines captured from `virsh event --all --loop --timestamp` (libvirt 11).
    /// The dashboard showed `-1` as the event of an rtc-change and `state:` for
    /// agent-lifecycle.
    #[test]
    fn non_lifecycle_events_get_readable_labels() {
        let p = |l: &str| {
            let ev = parse_event_line(l).unwrap();
            (ev.event, ev.detail)
        };
        assert_eq!(
            p("2026-10-07 11:58:40.000+0000: event 'rtc-change' for domain 'a': -1"),
            ("rtc".into(), "guest clock offset -1 s".into())
        );
        assert_eq!(
            p(
                "2026-10-07 12:05:22.665+0000: event 'agent-lifecycle' for domain 'a': state: 'disconnected' reason: 'domain started'"
            ),
            ("agent".into(), "disconnected (domain started)".into())
        );
        assert_eq!(
            p(
                "2026-10-07 12:05:22.665+0000: event 'channel-lifecycle' for domain 'a': channel name: 'org.qemu.guest_agent.0' state: 'disconnected' reason: 'domain started'"
            ),
            (
                "channel".into(),
                "org.qemu.guest_agent.0 disconnected (domain started)".into()
            )
        );
        assert_eq!(
            p("2026-10-07 12:05:23.000+0000: event 'balloon-change' for domain 'a': 4194304"),
            ("balloon".into(), "4096 MiB".into())
        );
        assert_eq!(
            p(
                "2026-10-07 12:05:23.746+0000: event 'metadata-change' for domain 'a': type description, uri <null>"
            ),
            ("metadata".into(), "type description, uri <null>".into())
        );
        assert_eq!(
            p(
                "2026-10-07 12:05:23.961+0000: event 'lifecycle' for domain 'a': Shutdown Finished after host request"
            ),
            ("Shutdown".into(), "Finished after host request".into())
        );
    }
}
