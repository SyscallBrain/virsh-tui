//! Real backend/lifecycle integration on owned `vt-test-*` domains only.
//!
//! Run in the disposable Linux container described in CONTRIBUTING.md. Other
//! integration targets still have their own requirements; do not run all of
//! `--features it` against a personal libvirt session.

#![cfg(feature = "it")]

use std::process::{Command, Output};

use virsh_tui::{
    backend::{Backend, virsh::VirshBackend},
    command::{
        builders::{lifecycle, snapshot},
        exec::execute,
        plan::CommandPlan,
    },
    model::DomainState,
};

const URI: &str = "qemu:///session";

fn virsh(args: &[&str]) -> Output {
    Command::new("virsh")
        .arg("-c")
        .arg(URI)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .expect("virsh must be installed; integration tests never skip missing tools")
}

fn check(args: &[&str]) -> String {
    let out = virsh(args);
    assert!(
        out.status.success(),
        "virsh {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

struct Domain {
    // Keep each disk/XML in a private directory until domain cleanup is done.
    dir: tempfile::TempDir,
    name: String,
    uuid: String,
    defined: bool,
}

impl Domain {
    async fn define() -> Self {
        let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid")
            .expect("this integration target requires Linux")
            .trim()
            .to_string();
        let mut domain = Self {
            dir: tempfile::Builder::new()
                .prefix("vt-test-lifecycle-")
                .tempdir()
                .unwrap(),
            name: format!("vt-test-lifecycle-{uuid}"),
            uuid,
            defined: false,
        };

        // Never redefine/adopt an existing domain, even when its name starts
        // with vt-test-. An explicit UUID also makes libvirt reject a name
        // collision with a different domain at define time.
        let names = check(&["list", "--all", "--name"]);
        let uuids = check(&["list", "--all", "--uuid"]);
        assert!(!names.lines().any(|name| name.trim() == domain.name));
        assert!(!uuids.lines().any(|uuid| uuid.trim() == domain.uuid));

        let disk = domain.dir.path().join("disk.qcow2");
        let mk = Command::new("qemu-img")
            .args(["create", "-f", "qcow2"])
            .arg(&disk)
            .arg("32M")
            .output()
            .expect("qemu-img must be installed");
        assert!(mk.status.success(), "{}", String::from_utf8_lossy(&mk.stderr));

        // type=qemu deliberately uses TCG, so CI needs neither KVM nor a
        // privileged container. No guest network, host disks or install image.
        let xml = format!(
            r"<domain type='qemu'>
  <name>{}</name>
  <uuid>{}</uuid>
  <memory unit='KiB'>65536</memory>
  <vcpu placement='static'>1</vcpu>
  <os><type arch='x86_64' machine='pc'>hvm</type><boot dev='hd'/></os>
  <devices>
    <emulator>/usr/bin/qemu-system-x86_64</emulator>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
  </devices>
</domain>",
            domain.name,
            domain.uuid,
            virsh_tui::xml::escape(disk.to_str().unwrap()),
        );
        let path = domain.dir.path().join("domain.xml");
        std::fs::write(&path, xml).unwrap();
        let plan = CommandPlan::single(
            "virsh",
            vec!["define", path.to_str().unwrap(), "--validate"],
            "Defined integration domain",
        );
        execute(URI, &plan, false).await.unwrap();
        // Arm cleanup only after this process successfully defined the domain.
        domain.defined = true;
        assert_eq!(check(&["domuuid", &domain.name]).trim(), domain.uuid);
        domain
    }

    async fn assert_state(&self, state: DomainState) {
        let domains = VirshBackend::new(URI).list_domains().await.unwrap();
        let domain = domains.iter().find(|domain| domain.name == self.name).unwrap();
        assert_eq!(domain.state, state, "{}", self.name);
    }

    fn exists(&self) -> bool {
        check(&["list", "--all", "--uuid"])
            .lines()
            .any(|uuid| uuid.trim() == self.uuid)
    }
}

impl Drop for Domain {
    fn drop(&mut self) {
        if !self.defined {
            return;
        }
        // Never act on a name that has been replaced by another domain. Target
        // the owned UUID for cleanup, including when an assertion unwinds.
        let Ok(identity) = Command::new("virsh")
            .args(["-c", URI, "domuuid", &self.name])
            .env("LC_ALL", "C")
            .output()
        else {
            return;
        };
        if !identity.status.success() || String::from_utf8_lossy(&identity.stdout).trim() != self.uuid {
            return;
        }
        let _ = virsh(&["destroy", &self.uuid]);
        let _ = virsh(&[
            "undefine",
            &self.uuid,
            "--snapshots-metadata",
            "--managed-save",
            "--keep-nvram",
        ]);
    }
}

#[tokio::test]
async fn lifecycle_define_start_pause_destroy_undefine() {
    let mut domain = Domain::define().await;
    let name = &domain.name;
    domain.assert_state(DomainState::ShutOff).await;
    execute(URI, &lifecycle::autostart(name, false), false)
        .await
        .unwrap();
    execute(URI, &lifecycle::start(name), false).await.unwrap();
    domain.assert_state(DomainState::Running).await;

    // Dry-run must not perform a real destructive action.
    let preview = execute(URI, &lifecycle::destroy(name), true).await.unwrap();
    assert!(preview.starts_with("[dry-run]"));
    domain.assert_state(DomainState::Running).await;

    // Starting a running domain fails. The executor must report that failure
    // and must not run a destructive second step.
    let mut failed = lifecycle::start(name);
    failed.steps.extend(lifecycle::destroy(name).steps);
    let error = execute(URI, &failed, false).await.unwrap_err();
    assert!(error.to_string().contains("step 1/2 failed"), "{error}");
    domain.assert_state(DomainState::Running).await;

    execute(URI, &lifecycle::pause(name), false).await.unwrap();
    domain.assert_state(DomainState::Paused).await;
    execute(URI, &lifecycle::resume(name), false).await.unwrap();
    domain.assert_state(DomainState::Running).await;

    execute(URI, &snapshot::create(name, "it-snap", "", true, false), false)
        .await
        .unwrap();
    let backend = VirshBackend::new(URI);
    assert!(
        backend
            .snapshots(name)
            .await
            .unwrap()
            .iter()
            .any(|s| s.name == "it-snap")
    );
    execute(
        URI,
        &snapshot::delete(name, "it-snap", false, false, false),
        false,
    )
    .await
    .unwrap();
    assert!(backend.snapshots(name).await.unwrap().is_empty());

    execute(URI, &lifecycle::destroy(name), false).await.unwrap();
    domain.assert_state(DomainState::ShutOff).await;
    execute(URI, &lifecycle::undefine(name, false, false, true), false)
        .await
        .unwrap();
    assert!(!domain.exists());
    domain.defined = false;
}

#[tokio::test]
async fn cleanup_after_unwind_preserves_another_domain() {
    let survivor = Domain::define().await;
    let owned = Domain::define().await;
    let uuid = owned.uuid.clone();
    execute(URI, &lifecycle::start(&owned.name), false).await.unwrap();

    let result = std::panic::catch_unwind(|| {
        let _owned = owned;
        panic!("simulate a failed assertion after starting the owned domain");
    });
    assert!(result.is_err());
    assert!(
        !check(&["list", "--all", "--uuid"])
            .lines()
            .any(|id| id.trim() == uuid)
    );
    assert!(
        survivor.exists(),
        "cleanup must not touch another vt-test-* domain"
    );
}
