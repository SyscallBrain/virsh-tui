//! Hardware editor (PLAN.md section 9.5 Hardware tab, P7).
//!
//! Three columns: Devices (38) | editor (min) | right stack (60).
//! Changes accumulate per domain until `:w`.

use std::collections::{HashMap, HashSet};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::theme::Theme;
use crate::ui::widgets::form::{Field, FieldKind, Form};

/// Device entry.
#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub icon: &'static str,
    pub name: String,
    pub summary: String,
    pub color: &'static str,
}

const DEVICES: [(&str, &str, &str, &str, &str); 14] = [
    ("overview", "◈", "Overview", "", "fg2"),
    ("cpus", "▣", "CPUs", "8 / 16", "blue"),
    ("memory", "▤", "Memory", "16 GiB", "magenta"),
    ("boot", "⏻", "Boot options", "uefi", "fg2"),
    ("disk-vda", "◫", "Disk vda", "120G", "green"),
    ("disk-vdb", "◫", "Disk vdb", "500G", "green"),
    ("cdrom-sda", "◎", "CDROM sda", "iso", "yellow"),
    ("nic-vnet3", "⇄", "NIC vnet3", "virtio", "cyan"),
    ("display", "▭", "Display", "spice", "fg2"),
    ("video", "▢", "Video", "virtio-gpu", "fg2"),
    ("sound", "♪", "Sound", "ich9", "fg2"),
    ("input", "⌨", "Input", "tablet", "fg2"),
    ("tpm", "⛨", "TPM", "v2.0 crb", "fg2"),
    ("usb", "⇢", "USB redirect", "×2", "fg2"),
];

/// Map a field to its XML section (for pending counts + diff hunks).
fn section_of(device: &str, field: &str) -> &'static str {
    match (device, field) {
        ("cpus", "max" | "current") => "vcpu",
        ("cpus", _) => "cpu",
        ("memory", _) => "memory",
        ("boot", _) => "os",
        _ => "devices",
    }
}

fn num(id: &str, label: &str, value: &str, min: i64, max: i64) -> Field {
    Field::new(id, label, FieldKind::Number { min, max, step: 1 }, value)
}

/// Hardware editor state.
#[derive(Debug, Clone)]
pub struct HardwareState {
    pub devices: Vec<Device>,
    pub selected: usize,
    pub forms: HashMap<String, Form>,
    pub apply_live: bool,
    pub apply_config: bool,
    pub base_xml: String,
    /// The domain is running (decides whether `--live` steps are emitted).
    pub running: bool,
    /// Fields changed explicitly (for undo scope).
    explicit: HashSet<(String, String)>,
}

fn kib_to_size(kib: u64) -> String {
    if kib > 0 && kib.is_multiple_of(1024 * 1024) {
        format!("{}G", kib / 1024 / 1024)
    } else {
        format!("{}M", kib / 1024)
    }
}

/// Parse `16G`, `512M`, `1.5T`, `2048` (MiB) into KiB.
pub fn size_to_kib(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, mult) = match s.chars().last()? {
        'K' | 'k' => (&s[..s.len() - 1], 1.0),
        'M' | 'm' => (&s[..s.len() - 1], 1024.0),
        'G' | 'g' => (&s[..s.len() - 1], 1024.0 * 1024.0),
        'T' | 't' => (&s[..s.len() - 1], 1024.0 * 1024.0 * 1024.0),
        _ => (s, 1024.0),
    };
    let v: f64 = num.trim().parse().ok()?;
    (v > 0.0).then_some((v * mult) as u64)
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

impl HardwareState {
    /// Demo arch-dev state (clean; tests/snapshots add pending edits).
    pub fn demo() -> Self {
        let devices = DEVICES
            .iter()
            .map(|(id, icon, name, sum, color)| Device {
                id: id.to_string(),
                icon,
                name: name.to_string(),
                summary: sum.to_string(),
                color,
            })
            .collect();
        let mut forms: HashMap<String, Form> = HashMap::new();
        forms.insert(
            String::from("cpus"),
            Form::new(vec![
                num("current", "vCPUs (current)", "8", 1, 32),
                num("max", "vCPUs (maximum)", "8", 1, 32),
                num("sockets", "sockets", "1", 1, 8),
                num("cores", "cores", "4", 1, 32),
                num("threads", "threads", "2", 1, 8),
                Field::new(
                    "model",
                    "model",
                    FieldKind::Select(vec![
                        String::from("host-model"),
                        String::from("host-passthrough"),
                        String::from("custom"),
                    ]),
                    "host-passthrough",
                ),
                Field::new("svm", "nested virtualization", FieldKind::Checkbox, "x"),
                Field::new("pin0", "v0 → host", FieldKind::Text, "2"),
                Field::new("emulatorpin", "emulatorpin", FieldKind::Text, ""),
                Field::new("iothreads", "iothreads", FieldKind::Text, "2"),
            ]),
        );
        forms.insert(
            String::from("memory"),
            Form::new(vec![
                Field::new("current", "Memory (current)", FieldKind::Size, "16G"),
                Field::new("max", "Memory (maximum)", FieldKind::Size, "16G"),
                Field::new("balloon", "balloon", FieldKind::Checkbox, "x"),
                Field::new("hugepages", "hugepages", FieldKind::Checkbox, ""),
                Field::new("shared", "shared memory", FieldKind::Checkbox, ""),
            ]),
        );
        forms.insert(
            String::from("boot"),
            Form::new(vec![
                Field::new(
                    "firmware",
                    "firmware",
                    FieldKind::Select(vec![String::from("bios"), String::from("uefi")]),
                    "uefi",
                ),
                Field::new("secure", "secure boot", FieldKind::Checkbox, ""),
                Field::new("order", "boot order", FieldKind::Text, "hd,cdrom,network"),
                Field::new("menu", "boot menu", FieldKind::Checkbox, ""),
            ]),
        );
        forms.insert(
            String::from("disk-vda"),
            Form::new(vec![
                Field::new(
                    "source",
                    "source",
                    FieldKind::Text,
                    "/var/lib/libvirt/images/arch-dev.qcow2",
                ),
                Field::new(
                    "bus",
                    "bus",
                    FieldKind::Select(vec![
                        String::from("virtio"),
                        String::from("sata"),
                        String::from("scsi"),
                    ]),
                    "virtio",
                ),
                Field::new(
                    "cache",
                    "cache",
                    FieldKind::Select(vec![
                        String::from("default"),
                        String::from("none"),
                        String::from("writeback"),
                    ]),
                    "default",
                ),
                Field::new("size", "size", FieldKind::Size, "120G"),
            ]),
        );
        forms.insert(
            String::from("nic-vnet3"),
            Form::new(vec![
                Field::new("source", "source", FieldKind::Text, "default"),
                Field::new(
                    "model",
                    "model",
                    FieldKind::Select(vec![String::from("virtio"), String::from("e1000e")]),
                    "virtio",
                ),
                Field::new("mac", "MAC", FieldKind::Text, "52:54:00:a3:1f:7c"),
                Field::new("link", "link up", FieldKind::Checkbox, "x"),
            ]),
        );
        forms.insert(
            String::from("display"),
            Form::new(vec![
                Field::new(
                    "type",
                    "type",
                    FieldKind::Select(vec![
                        String::from("spice"),
                        String::from("vnc"),
                        String::from("none"),
                    ]),
                    "spice",
                ),
                Field::new("listen", "listen", FieldKind::Text, "127.0.0.1"),
                Field::new("port", "port", FieldKind::Text, "5901"),
            ]),
        );
        for (id, model) in [
            ("video", "virtio-gpu"),
            ("sound", "ich9"),
            ("input", "tablet"),
            ("tpm", "v2.0 crb"),
            ("usb", "×2"),
        ] {
            forms.insert(
                String::from(id),
                Form::new(vec![Field::new("model", "model", FieldKind::Text, model)]),
            );
        }
        let base_xml = crate::backend::demo::fixtures::ARCH_DEV_XML.to_string();
        Self {
            devices,
            selected: 1,
            forms,
            apply_live: true,
            apply_config: true,
            base_xml,
            running: true,
            explicit: HashSet::new(),
        }
    }

    /// Editor state for a real domain, built from its inactive XML.
    ///
    /// Only fields that `will_run` can apply are editable; every other device is
    /// listed read-only (its form is informational).
    pub fn from_config(cfg: &crate::model::DomainConfig, xml: &str, running: bool) -> Self {
        let dev =
            |id: String, icon: &'static str, name: String, summary: String, color: &'static str| Device {
                id,
                icon,
                name,
                summary,
                color,
            };
        let mut devices = vec![
            dev("overview".into(), "◈", "Overview".into(), String::new(), "fg2"),
            dev(
                "cpus".into(),
                "▣",
                "CPUs".into(),
                format!("{} / {}", cfg.current_vcpus, cfg.vcpus),
                "blue",
            ),
            dev(
                "memory".into(),
                "▤",
                "Memory".into(),
                kib_to_size(cfg.max_mem_kib),
                "magenta",
            ),
            dev(
                "boot".into(),
                "⏻",
                "Boot options".into(),
                cfg.firmware_label().to_lowercase(),
                "fg2",
            ),
        ];
        let mut forms: HashMap<String, Form> = HashMap::new();
        let (s, c, t) = cfg.topology.unwrap_or((1, cfg.vcpus.max(1), 1));
        let mut modes = vec![
            String::from("host-model"),
            String::from("host-passthrough"),
            String::from("custom"),
        ];
        if !modes.contains(&cfg.cpu_mode) {
            modes.push(cfg.cpu_mode.clone());
        }
        forms.insert(
            String::from("cpus"),
            Form::new(vec![
                num(
                    "current",
                    "vCPUs (current)",
                    &cfg.current_vcpus.to_string(),
                    1,
                    512,
                ),
                num("max", "vCPUs (maximum)", &cfg.vcpus.to_string(), 1, 512),
                num("sockets", "sockets", &s.to_string(), 1, 64),
                num("cores", "cores", &c.to_string(), 1, 512),
                num("threads", "threads", &t.to_string(), 1, 8),
                Field::new("model", "model", FieldKind::Select(modes), &cfg.cpu_mode),
            ]),
        );
        forms.insert(
            String::from("memory"),
            Form::new(vec![
                Field::new(
                    "current",
                    "Memory (current)",
                    FieldKind::Size,
                    &kib_to_size(cfg.mem_kib),
                ),
                Field::new(
                    "max",
                    "Memory (maximum)",
                    FieldKind::Size,
                    &kib_to_size(cfg.max_mem_kib),
                ),
            ]),
        );
        forms.insert(
            String::from("boot"),
            Form::new(vec![Field::new(
                "firmware",
                "firmware (read-only)",
                FieldKind::Text,
                cfg.firmware_label(),
            )]),
        );
        for d in &cfg.disks {
            if d.device == "cdrom" {
                let id = format!("cdrom-{}", d.target);
                let label = if d.source.is_empty() {
                    String::from("empty")
                } else {
                    basename(&d.source)
                };
                devices.push(dev(
                    id.clone(),
                    "◎",
                    format!("CDROM {}", d.target),
                    label,
                    "yellow",
                ));
                forms.insert(
                    id,
                    Form::new(vec![Field::new(
                        "source",
                        "media (␣mi to change)",
                        FieldKind::Text,
                        &d.source,
                    )]),
                );
            } else {
                let id = format!("disk-{}", d.target);
                devices.push(dev(
                    id.clone(),
                    "◫",
                    format!("Disk {}", d.target),
                    d.bus.clone(),
                    "green",
                ));
                forms.insert(
                    id,
                    Form::new(vec![
                        Field::new("source", "source (read-only)", FieldKind::Text, &d.source),
                        Field::new("size", "new size (blockresize)", FieldKind::Size, ""),
                    ]),
                );
            }
        }
        for (i, n) in cfg.nics.iter().enumerate() {
            let tag = if n.target.is_empty() {
                format!("{i}")
            } else {
                n.target.clone()
            };
            let id = format!("nic-{tag}");
            devices.push(dev(
                id.clone(),
                "⇄",
                format!("NIC {tag}"),
                n.model.clone(),
                "cyan",
            ));
            forms.insert(
                id,
                Form::new(vec![
                    Field::new("source", "source (read-only)", FieldKind::Text, &n.source),
                    Field::new("mac", "MAC (read-only)", FieldKind::Text, &n.mac),
                    Field::new("link", "link up", FieldKind::Checkbox, "x"),
                ]),
            );
        }
        if let Some(g) = &cfg.graphics {
            devices.push(dev(
                "display".into(),
                "▭",
                "Display".into(),
                g.kind.clone(),
                "fg2",
            ));
        }
        Self {
            devices,
            selected: 1,
            forms,
            apply_live: running,
            apply_config: true,
            base_xml: xml.to_string(),
            running,
            explicit: HashSet::new(),
        }
    }

    /// Dirty fields that `will_run` cannot apply (shown as an error on `:w`).
    pub fn unsupported_changes(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (dev, form) in &self.forms {
            for f in form.fields.iter().filter(|f| f.dirty()) {
                let ok = matches!(
                    (dev.as_str(), f.id.as_str()),
                    (
                        "cpus",
                        "current" | "max" | "sockets" | "cores" | "threads" | "model"
                    ) | ("memory", "current" | "max")
                ) || (dev.starts_with("disk-") && f.id == "size")
                    || (dev.starts_with("nic-") && f.id == "link");
                if !ok {
                    out.push(format!("{dev}.{}", f.id));
                }
            }
        }
        out.sort();
        out
    }

    fn is_dirty(&self, dev: &str, id: &str) -> bool {
        self.forms
            .get(dev)
            .and_then(|f| f.fields.iter().find(|x| x.id == id))
            .is_some_and(|f| f.dirty())
    }

    /// The define path is needed (topology or CPU model changed).
    fn needs_define(&self) -> bool {
        ["sockets", "cores", "threads", "model"]
            .iter()
            .any(|id| self.is_dirty("cpus", id))
    }

    /// Selected device.
    pub fn selected_device(&self) -> &Device {
        &self.devices[self.selected.min(self.devices.len().saturating_sub(1))]
    }

    /// Set a field value (INSERT editing). Auto-adjusts topology on max change.
    pub fn set_field(&mut self, device: &str, field: &str, value: &str) {
        if let Some(form) = self.forms.get_mut(device)
            && let Some(f) = form.fields.iter_mut().find(|f| f.id == field)
        {
            f.current = value.to_string();
        }
        self.explicit.insert((device.to_string(), field.to_string()));
        if device == "cpus"
            && field == "max"
            && let Some(max) = self.field_value("cpus", "max").parse::<u32>().ok()
        {
            let (s, c, t) = crate::xml::edit::auto_topology(max);
            for (id, v) in [
                ("sockets", s.to_string()),
                ("cores", c.to_string()),
                ("threads", t.to_string()),
            ] {
                if let Some(form) = self.forms.get_mut("cpus")
                    && let Some(f) = form.fields.iter_mut().find(|f| f.id == id)
                {
                    f.current = v;
                }
            }
        }
    }

    /// Current field value.
    pub fn field_value(&self, device: &str, field: &str) -> String {
        self.forms
            .get(device)
            .and_then(|f| f.fields.iter().find(|x| x.id == field))
            .map(|f| f.current.clone())
            .unwrap_or_default()
    }

    /// Undo one field (reverts topology side effects too).
    pub fn undo_field(&mut self, device: &str, field: &str) {
        if let Some(form) = self.forms.get_mut(device) {
            form.undo(field);
        }
        self.explicit.remove(&(device.to_string(), field.to_string()));
        if device == "cpus" && field == "max" {
            for id in ["sockets", "cores", "threads"] {
                if let Some(form) = self.forms.get_mut("cpus") {
                    form.undo(id);
                }
                self.explicit.remove(&(device.to_string(), id.to_string()));
            }
        }
    }

    /// Discard all pending changes (`:q!`).
    pub fn discard_all(&mut self) {
        for form in self.forms.values_mut() {
            for f in form.fields.iter_mut() {
                f.undo();
            }
        }
        self.explicit.clear();
    }

    /// Mark a directly edited field as explicit (INSERT raw editing).
    pub fn explicit_insert(&mut self, device: &str, field: &str) {
        self.explicit.insert((device.to_string(), field.to_string()));
    }

    /// Unmark a field that matches its original again.
    pub fn explicit_remove(&mut self, device: &str, field: &str) {
        self.explicit.remove(&(device.to_string(), field.to_string()));
    }

    /// Recompute topology from max (INSERT side effects).
    pub fn auto_topology(&mut self) {
        if let Ok(max) = self.field_value("cpus", "max").parse::<u32>() {
            let (s, c, t) = crate::xml::edit::auto_topology(max);
            for (id, v) in [
                ("sockets", s.to_string()),
                ("cores", c.to_string()),
                ("threads", t.to_string()),
            ] {
                if let Some(form) = self.forms.get_mut("cpus")
                    && let Some(f) = form.fields.iter_mut().find(|f| f.id == id)
                {
                    f.current = v;
                }
            }
        }
    }

    /// Number of changed XML sections (the `● N pending` title).
    pub fn pending_count(&self) -> usize {
        let mut sections = HashSet::new();
        for form_name in self.forms.keys() {
            if let Some(form) = self.forms.get(form_name) {
                for f in &form.fields {
                    if f.dirty() {
                        sections.insert(section_of(form_name, &f.id));
                    }
                }
            }
        }
        sections.len()
    }

    /// Edited XML with all pending (persistent) changes applied.
    ///
    /// Only dirty values are written, so an unchanged domain round-trips as-is.
    pub fn edited_xml(&self) -> String {
        let mut xml = self.base_xml.clone();
        if (self.is_dirty("cpus", "current") || self.is_dirty("cpus", "max"))
            && let (Ok(cur), Ok(max)) = (
                self.field_value("cpus", "current").parse::<u32>(),
                self.field_value("cpus", "max").parse::<u32>(),
            )
            && let Ok(out) = crate::xml::edit::set_vcpus(&xml, cur.min(max), max)
        {
            xml = out;
        }
        if ["sockets", "cores", "threads"]
            .iter()
            .any(|id| self.is_dirty("cpus", id))
            && let (Ok(s), Ok(c), Ok(t)) = (
                self.field_value("cpus", "sockets").parse::<u32>(),
                self.field_value("cpus", "cores").parse::<u32>(),
                self.field_value("cpus", "threads").parse::<u32>(),
            )
            && let Ok(out) = crate::xml::edit::set_topology(&xml, s, c, t)
        {
            xml = out;
        }
        if self.is_dirty("cpus", "model")
            && let Ok(out) = crate::xml::edit::set_cpu_mode(&xml, &self.field_value("cpus", "model"))
        {
            xml = out;
        }
        if (self.is_dirty("memory", "current") || self.is_dirty("memory", "max"))
            && let (Some(cur), Some(max)) = (
                size_to_kib(&self.field_value("memory", "current")),
                size_to_kib(&self.field_value("memory", "max")),
            )
            && let Ok(out) = crate::xml::edit::set_memory(&xml, cur.min(max), max)
        {
            xml = out;
        }
        xml
    }

    /// Diff lines for the Pending panel.
    pub fn diff(&self) -> Vec<crate::xml::diff::DiffLine> {
        crate::xml::diff::diff_lines(&self.base_xml, &self.edited_xml())
    }

    /// Command plans for the Will-run panel.
    ///
    /// Without topology/model changes, native commands do everything. With them,
    /// the persistent config goes through ONE `virsh define` of the fully edited
    /// XML (so native `--config` steps cannot be reverted by a stale define), and
    /// only `--live` native steps follow for a running domain.
    pub fn will_run(&self, domain: &str) -> Vec<crate::command::plan::CommandPlan> {
        use crate::command::builders::hardware;
        let mut out = Vec::new();
        let live = self.apply_live && self.running;
        let define = self.needs_define();
        let config = self.apply_config && !define;
        if define {
            out.push(hardware::define(domain));
        }
        let vcpus_dirty = self.is_dirty("cpus", "max") || self.is_dirty("cpus", "current");
        if vcpus_dirty && (live || config) {
            let cur: u32 = self.field_value("cpus", "current").parse().unwrap_or(1);
            let max: u32 = self.field_value("cpus", "max").parse().unwrap_or(cur);
            let max_changed = self.is_dirty("cpus", "max") && config;
            out.extend(hardware::set_vcpus(
                domain,
                cur.min(max),
                max,
                max_changed,
                live,
                config,
            ));
        }
        let mem_dirty = self.is_dirty("memory", "current") || self.is_dirty("memory", "max");
        if mem_dirty && (live || config) {
            out.extend(hardware::set_memory(
                domain,
                &self.field_value("memory", "current"),
                &self.field_value("memory", "max"),
                live,
                config,
            ));
        }
        for dev in &self.devices {
            if let Some(target) = dev.id.strip_prefix("disk-")
                && self.is_dirty(&dev.id, "size")
                && !self.field_value(&dev.id, "size").is_empty()
            {
                out.push(hardware::block_resize(
                    domain,
                    target,
                    &self.field_value(&dev.id, "size"),
                ));
            }
            if let Some(iface) = dev.id.strip_prefix("nic-")
                && self.is_dirty(&dev.id, "link")
            {
                out.push(hardware::set_link(
                    domain,
                    iface,
                    self.field_value(&dev.id, "link") == "x",
                ));
            }
        }
        out
    }
}

fn panel_block(theme: &Theme, focused: bool, title: &str, right: Option<String>) -> Block<'static> {
    crate::ui::widgets::panel::block(
        theme,
        focused,
        title,
        right.map(|r| crate::ui::widgets::panel::right(r, theme)),
        None,
        None,
    )
}

fn token_color(theme: &Theme, name: &str) -> ratatui::style::Color {
    let t = &theme.tokens;
    match name {
        "blue" => t.blue,
        "magenta" => t.magenta,
        "green" => t.green,
        "yellow" => t.yellow,
        "cyan" => t.cyan,
        _ => t.fg2,
    }
}

/// Render the hardware tab: devices | editor | diff + will-run.
pub fn render(frame: &mut Frame, theme: &Theme, hw: &HardwareState, domain: &str, insert: bool, area: Rect) {
    let t = &theme.tokens;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(38), Constraint::Min(0), Constraint::Length(60)])
        .spacing(1)
        .split(area);
    render_devices(frame, theme, hw, cols[0]);
    render_editor(frame, theme, hw, insert, cols[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(12)])
        .split(cols[2]);
    render_diff(frame, theme, hw, right[0]);
    render_will_run(frame, theme, hw, domain, right[1]);
    let _ = t;
}

fn render_devices(frame: &mut Frame, theme: &Theme, hw: &HardwareState, area: Rect) {
    let t = &theme.tokens;
    let mut lines: Vec<Line> = Vec::new();
    for (i, d) in hw.devices.iter().enumerate() {
        let selected = i == hw.selected;
        let bg = if selected { t.hl } else { t.bg };
        let style = if selected {
            Style::default().fg(t.fg).bg(bg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.fg).bg(bg)
        };
        lines.push(
            Line::from(vec![
                Span::styled(
                    format!("{} ", d.icon),
                    Style::default().fg(token_color(theme, d.color)).bg(bg),
                ),
                Span::styled(format!("{:14}", d.name), style),
                Span::styled(d.summary.clone(), theme.dim()),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    lines.push(Line::from(vec![
        Span::styled("+ ", Style::default().fg(t.green).bg(t.bg)),
        Span::styled("add hardware  a", theme.dim()),
    ]));
    let right = format!("{}", hw.devices.len());
    let block = panel_block(theme, false, "Devices", Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// Section header shown before a field (mockup: Allocation, Topology, …).
fn section_before(device: &str, field: &str) -> Option<&'static str> {
    match (device, field) {
        ("cpus" | "memory", "current") => Some("Allocation"),
        ("cpus", "sockets") => Some("Topology"),
        ("cpus", "model") => Some("Model"),
        ("cpus", "svm") => Some("Features"),
        ("cpus", "pin0") => Some("Pinning · vcpupin"),
        ("memory", "balloon") => Some("Options"),
        (_, "source") => Some("Source"),
        ("boot", "firmware") => Some("Firmware"),
        (_, "link") => Some("Link"),
        (_, "size") => Some("Resize"),
        _ => None,
    }
}

fn render_editor(frame: &mut Frame, theme: &Theme, hw: &HardwareState, insert: bool, area: Rect) {
    let t = &theme.tokens;
    let dev = hw.selected_device();
    let section = Style::default().fg(t.blue).add_modifier(Modifier::BOLD);
    let mut lines: Vec<Line> = Vec::new();
    let value_box = |f: &crate::ui::widgets::form::Field, focused: bool| -> Vec<Span<'static>> {
        let text = if f.current.is_empty() && !focused {
            String::from("—")
        } else {
            f.current.clone()
        };
        let shown = format!(" {:>3} ", text);
        let mut v = Vec::new();
        if insert && focused {
            v.push(Span::styled("▕", Style::default().fg(t.green)));
            v.push(Span::styled(
                shown.trim_end().to_string(),
                Style::default().fg(t.fg).bg(t.sel),
            ));
            v.push(Span::styled(" ", Style::default().fg(t.bg).bg(t.fg)));
            v.push(Span::styled(" ", Style::default().bg(t.sel)));
            v.push(Span::styled("▏", Style::default().fg(t.green)));
        } else {
            let st = if focused {
                Style::default().fg(t.fg).bg(t.sel)
            } else {
                Style::default().fg(t.fg).bg(t.hl)
            };
            let st = if f.dirty() { st.fg(t.yellow) } else { st };
            v.push(Span::styled(shown, st));
        }
        v
    };
    if let Some(form) = hw.forms.get(dev.id.as_str()) {
        let mut i = 0;
        while i < form.fields.len() {
            let f = &form.fields[i];
            let focused = i == form.focus;
            if let Some(title) = section_before(&dev.id, &f.id) {
                if !lines.is_empty() {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(title, section)));
            }
            // Topology: sockets · cores · threads on one line with a product check.
            if dev.id == "cpus" && f.id == "sockets" && i + 2 < form.fields.len() {
                let mut spans = Vec::new();
                let mut product = 1u32;
                for j in i..i + 3 {
                    let g = &form.fields[j];
                    product = product.saturating_mul(g.current.parse().unwrap_or(1));
                    spans.push(Span::styled(format!("{:<9}", g.label), theme.secondary()));
                    spans.extend(value_box(g, j == form.focus));
                    spans.push(Span::raw("  "));
                }
                let max: u32 = hw.field_value("cpus", "max").parse().unwrap_or(0);
                spans.push(Span::styled("= ", theme.dim()));
                if product == max {
                    spans.push(Span::styled(format!("{product} ✓"), Style::default().fg(t.green)));
                    if form.fields[i..i + 3].iter().any(|g| g.dirty()) {
                        spans.push(Span::styled("  auto-adjusted", theme.dim()));
                    }
                } else {
                    spans.push(Span::styled(
                        format!("{product} ≠ {max} max"),
                        Style::default().fg(t.yellow),
                    ));
                }
                lines.push(Line::from(spans));
                i += 3;
                continue;
            }
            let mut spans = Vec::new();
            match &f.kind {
                FieldKind::Checkbox => {
                    let on = f.current == "x";
                    let st = if focused {
                        Style::default().bg(t.sel)
                    } else {
                        Style::default()
                    };
                    spans.push(Span::styled(
                        crate::ui::widgets::form::checkbox(on),
                        st.fg(if on { t.green } else { t.comment }),
                    ));
                    spans.push(Span::styled(format!(" {}", f.label), st.fg(t.fg)));
                }
                FieldKind::Select(opts) | FieldKind::Radio(opts) if opts.len() <= 4 => {
                    for o in opts {
                        let on = *o == f.current;
                        let st = if focused && on {
                            Style::default().bg(t.sel)
                        } else {
                            Style::default()
                        };
                        spans.push(Span::styled(
                            crate::ui::widgets::form::radio(on),
                            st.fg(if on { t.blue } else { t.comment }),
                        ));
                        spans.push(Span::styled(
                            format!(" {o}   "),
                            if on {
                                st.fg(t.fg).add_modifier(Modifier::BOLD)
                            } else {
                                st.fg(t.fg2)
                            },
                        ));
                    }
                }
                FieldKind::Select(_) => {
                    spans.push(Span::styled(format!("{:<17}", f.label), theme.secondary()));
                    let st = if focused {
                        Style::default().fg(t.fg).bg(t.sel)
                    } else {
                        Style::default().fg(t.fg).bg(t.hl)
                    };
                    spans.push(Span::styled(format!(" {} ▾ ", f.current), st));
                }
                _ => {
                    spans.push(Span::styled(format!("{:<17}", f.label), theme.secondary()));
                    spans.extend(value_box(f, focused));
                }
            }
            if let Some(w) = &f.warning {
                spans.push(Span::styled(format!("  ⚠ {w}"), Style::default().fg(t.yellow)));
            } else if dev.id == "cpus" && f.id == "max" && f.dirty() {
                spans.push(Span::styled("  ⚠ needs reboot", Style::default().fg(t.yellow)));
            } else if dev.id == "cpus" && f.id == "current" {
                spans.push(Span::styled("  live · hot-plug supported", theme.dim()));
            }
            lines.push(Line::from(spans));
            i += 1;
        }
        if matches!(dev.id.as_str(), "cpus" | "memory") {
            let check = |on: bool| {
                Span::styled(
                    crate::ui::widgets::form::checkbox(on),
                    Style::default().fg(if on { t.green } else { t.comment }),
                )
            };
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Apply to", section)));
            lines.push(Line::from(vec![
                check(hw.apply_config),
                Span::styled(" config ", theme.text()),
                Span::styled("(persistent, --config)", theme.dim()),
                Span::raw("   "),
                check(hw.apply_live),
                Span::styled(" live ", theme.text()),
                Span::styled("(running, --live)", theme.dim()),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No editable fields for this device.",
            theme.dim(),
        )));
    }
    let right = if hw.pending_count() > 0 {
        Some(Line::from(Span::styled(
            format!("● {} pending", hw.pending_count()),
            Style::default().fg(t.yellow),
        )))
    } else {
        None
    };
    let block = crate::ui::widgets::panel::block(theme, true, &dev.name, right, None, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_diff(frame: &mut Frame, theme: &Theme, hw: &HardwareState, area: Rect) {
    let t = &theme.tokens;
    let lines: Vec<Line> = hw
        .diff()
        .into_iter()
        .take(area.height.saturating_sub(2) as usize)
        .map(|l| {
            let (prefix, style) = match l.kind {
                crate::xml::diff::DiffKind::Minus => ("", Style::default().fg(t.red).bg(t.bg)),
                crate::xml::diff::DiffKind::Plus => ("", Style::default().fg(t.green).bg(t.bg)),
                crate::xml::diff::DiffKind::Hunk => ("", theme.dim()),
                crate::xml::diff::DiffKind::Context => ("", theme.secondary()),
            };
            let _ = prefix;
            Line::from(Span::styled(l.text, style))
        })
        .collect();
    let block = panel_block(
        theme,
        false,
        "Pending · XML diff",
        Some(String::from("<vcpu> · <cpu>")),
    );
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_will_run(frame: &mut Frame, theme: &Theme, hw: &HardwareState, domain: &str, area: Rect) {
    let t = &theme.tokens;
    let mut lines: Vec<Line> = Vec::new();
    let plans = hw.will_run(domain);
    for plan in &plans {
        for step in &plan.steps {
            lines.push(Line::from(crate::command::display::spans(theme, step)));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            " :w apply ",
            Style::default()
                .fg(t.bg2)
                .bg(t.green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(" u undo field ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::raw(" "),
        Span::styled(" :q! discard ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::raw(" "),
        Span::styled(" gx open XML ", Style::default().fg(t.fg2).bg(t.hl)),
    ]));
    let block = panel_block(theme, false, "Will run", Some(String::from("y copy")));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}
