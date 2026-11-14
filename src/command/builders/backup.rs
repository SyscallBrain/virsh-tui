//! Push backups (`virsh backup-begin`): every disk to a qcow2 file in a folder.

use crate::command::plan::{CommandPlan, CommandStep};
use crate::model::DiskInfo;

/// `<domainbackup>` XML: each disk to `<dir>/<domain>-<target>-<stamp>.qcow2`;
/// CD-ROMs and floppies are skipped.
pub fn xml(domain: &str, disks: &[DiskInfo], dir: &str, stamp: &str) -> Result<String, String> {
    if !dir.starts_with('/') {
        return Err(String::from("the backup folder must be an absolute path"));
    }
    let dir = dir.trim_end_matches('/');
    let mut out = String::from("<domainbackup>\n  <disks>\n");
    let mut any = false;
    for d in disks {
        let target = crate::xml::escape(&d.target);
        if d.device == "disk" {
            any = true;
            let file = crate::xml::escape(&format!("{dir}/{domain}-{}-{stamp}.qcow2", d.target));
            out.push_str(&format!(
                "    <disk name='{target}' type='file'>\n      <target file='{file}'/>\n      <driver type='qcow2'/>\n    </disk>\n"
            ));
        } else {
            out.push_str(&format!("    <disk name='{target}' backup='no'/>\n"));
        }
    }
    if !any {
        return Err(format!("{domain} has no disk to back up"));
    }
    out.push_str("  </disks>\n</domainbackup>\n");
    Ok(out)
}

/// `backup-begin` with the XML file, then wait for libvirt's backup job.
pub fn plan(domain: &str, xml_path: &str, dir: &str) -> CommandPlan {
    CommandPlan {
        summary: format!("Backed up {domain} to {dir}"),
        steps: vec![
            CommandStep {
                program: String::from("virsh"),
                argv: ["backup-begin", domain, "--backupxml", xml_path]
                    .map(String::from)
                    .to_vec(),
                stdin: None,
            },
            CommandStep {
                program: String::from(crate::command::process::WAIT_JOB),
                argv: ["wait", "for", "the", "backup", "job", "of", domain]
                    .map(String::from)
                    .to_vec(),
                stdin: None,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(target: &str, device: &str) -> DiskInfo {
        DiskInfo {
            target: target.into(),
            device: device.into(),
            ..Default::default()
        }
    }

    #[test]
    fn disks_go_to_the_folder_and_cdroms_are_skipped() {
        let doc = xml(
            "web",
            &[disk("vda", "disk"), disk("sda", "cdrom")],
            "/srv/backup/",
            "20261114-1500",
        )
        .unwrap();
        assert!(doc.contains("<disk name='vda' type='file'>"));
        assert!(doc.contains("<target file='/srv/backup/web-vda-20261114-1500.qcow2'/>"));
        assert!(doc.contains("<disk name='sda' backup='no'/>"));
        assert!(xml("web", &[disk("sda", "cdrom")], "/srv", "x").is_err());
        assert!(xml("web", &[disk("vda", "disk")], "backup", "x").is_err());
        let p = plan("web", "/run/x.xml", "/srv");
        assert_eq!(
            crate::command::display::display(&p.steps[1]),
            "# wait for the backup job of web"
        );
    }
}
