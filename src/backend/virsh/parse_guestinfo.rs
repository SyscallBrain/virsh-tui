//! `virsh guestinfo <domain>`: what the QEMU guest agent reports.

/// One mounted file system.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuestFs {
    pub mountpoint: String,
    pub fstype: String,
    pub total_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
}

/// The agent's view of the guest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuestInfo {
    pub os: String,
    pub kernel: String,
    pub hostname: String,
    pub timezone: String,
    pub users: Vec<String>,
    pub filesystems: Vec<GuestFs>,
}

/// Parse `key : value` lines (`os.pretty-name`, `fs.0.mountpoint`, …).
pub fn parse(text: &str) -> GuestInfo {
    let mut info = GuestInfo::default();
    let mut os_name = String::new();
    let mut fs: std::collections::BTreeMap<usize, GuestFs> = std::collections::BTreeMap::new();
    let mut users: std::collections::BTreeMap<usize, String> = std::collections::BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), crate::model::sanitize(value.trim()));
        match key {
            "os.pretty-name" => info.os = value,
            "os.name" => os_name = value,
            "os.kernel-release" => info.kernel = value,
            "hostname" => info.hostname = value,
            "timezone.name" => info.timezone = value,
            _ => {
                let mut parts = key.splitn(3, '.');
                let (Some(group), Some(index), Some(field)) = (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                let Ok(index) = index.parse::<usize>() else {
                    continue;
                };
                match group {
                    "user" if field == "name" => {
                        users.insert(index, value);
                    }
                    "fs" => {
                        let entry = fs.entry(index).or_default();
                        match field {
                            "mountpoint" => entry.mountpoint = value,
                            "fstype" => entry.fstype = value,
                            "total-bytes" => entry.total_bytes = value.parse().ok(),
                            "used-bytes" => entry.used_bytes = value.parse().ok(),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if info.os.is_empty() {
        info.os = os_name;
    }
    info.users = users.into_values().collect();
    info.users.dedup();
    info.filesystems = fs.into_values().filter(|f| !f.mountpoint.is_empty()).collect();
    info
}

/// The user-facing reason a `guestinfo` call failed.
pub fn error_reason(stderr: &str) -> String {
    if stderr.contains("read only access") {
        String::from("needs a read-write connection")
    } else if stderr.contains("not connected") || stderr.contains("not responding") {
        String::from("agent not responding (is qemu-guest-agent running in the guest?)")
    } else if stderr.contains("unsupported") || stderr.contains("not supported") {
        String::from("not supported by this agent or libvirt")
    } else {
        stderr.lines().next().unwrap_or("failed").trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_os_users_and_filesystems() {
        let text = "user.count          : 2
user.0.name         : root
user.0.login-time   : 1700000000000
user.1.name         : tiago
os.id               : arch
os.name             : Arch Linux
os.pretty-name      : Arch Linux
os.kernel-release   : 6.11.2-arch1-1
timezone.name       : UTC
timezone.offset     : 0
hostname            : web-01
fs.count            : 2
fs.0.name           : vda2
fs.0.mountpoint     : /
fs.0.fstype         : ext4
fs.0.total-bytes    : 42000000000
fs.0.used-bytes     : 13000000000
fs.1.name           : vda1
fs.1.mountpoint     : /boot
fs.1.fstype         : vfat
";
        let info = parse(text);
        assert_eq!(info.os, "Arch Linux");
        assert_eq!(info.kernel, "6.11.2-arch1-1");
        assert_eq!(info.hostname, "web-01");
        assert_eq!(info.timezone, "UTC");
        assert_eq!(info.users, ["root", "tiago"]);
        assert_eq!(info.filesystems.len(), 2);
        assert_eq!(info.filesystems[0].used_bytes, Some(13_000_000_000));
        assert_eq!(info.filesystems[1].total_bytes, None);
    }

    #[test]
    fn errors_read_as_reasons() {
        assert_eq!(
            error_reason("error: Guest agent is not responding: QEMU guest agent is not connected"),
            "agent not responding (is qemu-guest-agent running in the guest?)"
        );
        assert_eq!(
            error_reason("error: operation forbidden: read only access prevents virDomainGetGuestInfo"),
            "needs a read-write connection"
        );
    }
}
