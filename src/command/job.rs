//! Sequential owned command batches.

use super::{
    plan::CommandPlan,
    process::{RunError, run_step_with_started},
};
use tokio::sync::{mpsc, watch};

/// Started-step records retained independently of worker completion and progress delivery.
pub type StartedLedger = std::sync::Arc<std::sync::Mutex<Vec<CommandEvent>>>;

tokio::task_local! {
    static STARTED_LEDGER: StartedLedger;
}

/// Spawn an owned workflow with a durable record of the steps it starts.
pub fn spawn_tracked(
    future: impl std::future::Future<Output = BatchOutcome> + Send + 'static,
) -> (tokio::task::JoinHandle<BatchOutcome>, StartedLedger) {
    let ledger = StartedLedger::default();
    let task = tokio::spawn(STARTED_LEDGER.scope(ledger.clone(), future));
    (task, ledger)
}

/// Inputs and temporary-file guards retained for the lifetime of a batch.
pub struct OwnedBatch {
    pub uri: String,
    pub demo: bool,
    pub dry_run: bool,
    pub plans: Vec<(String, CommandPlan)>,
    pub temporary_files: Vec<tempfile::NamedTempFile>,
}

/// A step that actually started.
#[derive(Debug, Clone)]
pub struct CommandEvent {
    pub target: String,
    pub step: usize,
    pub total: usize,
    pub command: String,
    pub program: String,
    pub argv: Vec<String>,
}

/// Terminal state of a batch.
#[derive(Debug)]
pub enum BatchResult {
    Success,
    Failed {
        target: String,
        step: usize,
        message: String,
    },
    Cancelled,
    Timeout,
}

/// Partial successes survive a later failure.
#[derive(Debug)]
pub struct BatchOutcome {
    pub started_steps: Vec<CommandEvent>,
    pub completed_steps: Vec<CommandEvent>,
    pub outputs: Vec<super::process::StepOutput>,
    pub completed_targets: Vec<String>,
    pub result: BatchResult,
}

/// Execute targets and their steps in order; never block on progress delivery.
pub async fn run_batch(
    batch: OwnedBatch,
    mut control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let mut started_steps = Vec::new();
    let mut completed_steps = Vec::new();
    let mut outputs = Vec::new();
    let mut completed_targets = Vec::new();
    let mut result = BatchResult::Success;
    'targets: for (target, plan) in &batch.plans {
        for (index, step) in plan.steps.iter().enumerate() {
            if *control.borrow() {
                result = BatchResult::Cancelled;
                break 'targets;
            }
            if batch.demo || batch.dry_run || super::exec::is_demo() {
                continue;
            }
            let mut argv = Vec::new();
            if step.program == "virsh" {
                argv.extend(["-c".to_string(), batch.uri.clone()]);
            }
            argv.extend(step.argv.iter().cloned());
            let event = CommandEvent {
                program: step.program.clone(),
                argv,
                target: target.clone(),
                step: index + 1,
                total: plan.steps.len(),
                command: super::display::display(step),
            };
            match run_step_with_started(&batch.uri, step, &mut control, || {
                started_steps.push(event.clone());
                let _ = STARTED_LEDGER.try_with(|ledger| {
                    ledger
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(event.clone());
                });
                let _ = events.try_send(event);
            })
            .await
            {
                Ok(output) if output.status.success() => {
                    if let Some(step) = started_steps.last() {
                        completed_steps.push(step.clone());
                    }
                    // The raw-command path needs only the first step's output.
                    // Do not retain another pair of buffers for every bulk target.
                    if outputs.is_empty() {
                        outputs.push(output);
                    }
                }
                Ok(output) => {
                    let message = crate::model::sanitize(
                        String::from_utf8_lossy(&output.stderr)
                            .lines()
                            .next()
                            .unwrap_or("unknown error"),
                    );
                    result = BatchResult::Failed {
                        target: target.clone(),
                        step: index + 1,
                        message,
                    };
                    break 'targets;
                }
                Err(RunError::Cancelled) => {
                    result = BatchResult::Cancelled;
                    break 'targets;
                }
                Err(RunError::Timeout) => {
                    result = BatchResult::Timeout;
                    break 'targets;
                }
                Err(error) => {
                    result = BatchResult::Failed {
                        target: target.clone(),
                        step: index + 1,
                        message: error.to_string(),
                    };
                    break 'targets;
                }
            }
        }
        completed_targets.push(target.clone());
    }
    BatchOutcome {
        started_steps,
        completed_steps,
        outputs,
        completed_targets,
        result,
    }
}

/// Create a network using private XML owned until all ordered steps finish.
pub async fn run_network_create(
    uri: String,
    name: String,
    xml: String,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let outcome = |result| BatchOutcome {
        started_steps: vec![],
        completed_steps: vec![],
        outputs: vec![],
        completed_targets: vec![],
        result,
    };
    if *control.borrow() {
        return outcome(BatchResult::Cancelled);
    }
    if super::exec::is_demo() {
        return outcome(BatchResult::Success);
    }
    let temporary = match super::tempxml::write("net", &xml) {
        Ok(file) => file,
        Err(error) => {
            return outcome(BatchResult::Failed {
                target: name,
                step: 1,
                message: error.to_string(),
            });
        }
    };
    let path = super::tempxml::path_arg(&temporary);
    let steps = vec![
        super::plan::CommandStep {
            program: program.clone(),
            argv: vec!["net-define".into(), path],
            stdin: None,
        },
        super::plan::CommandStep {
            program: program.clone(),
            argv: vec!["net-start".into(), name.clone()],
            stdin: None,
        },
        super::plan::CommandStep {
            program,
            argv: vec!["net-autostart".into(), name.clone()],
            stdin: None,
        },
    ];
    let plan = CommandPlan {
        steps,
        summary: format!("Created network {name}"),
    };
    let batch = OwnedBatch {
        uri,
        demo: false,
        dry_run: false,
        plans: vec![(name, plan)],
        temporary_files: vec![temporary],
    };
    run_batch(batch, control, events).await
}

/// Run a structured read followed by a step prepared from its complete output.
async fn run_prepared<F>(
    uri: String,
    name: String,
    first_step: super::plan::CommandStep,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
    prepare: F,
) -> BatchOutcome
where
    F: FnOnce(
        &super::process::StepOutput,
    ) -> Result<(super::plan::CommandStep, Vec<tempfile::NamedTempFile>), String>,
{
    let (phase_tx, mut phase_rx) = mpsc::channel::<CommandEvent>(16);
    let forward = tokio::spawn(async move {
        while let Some(mut event) = phase_rx.recv().await {
            event.total = 2;
            let _ = events.try_send(event);
        }
    });
    let make_batch = |step, temporary_files| OwnedBatch {
        uri: uri.clone(),
        demo: false,
        dry_run: false,
        plans: vec![(
            name.clone(),
            CommandPlan {
                summary: String::new(),
                steps: vec![step],
            },
        )],
        temporary_files,
    };
    let mut first = run_batch(make_batch(first_step, vec![]), control.clone(), phase_tx.clone()).await;
    for event in first
        .started_steps
        .iter_mut()
        .chain(first.completed_steps.iter_mut())
    {
        event.total = 2;
    }
    if matches!(first.result, BatchResult::Success) && !super::exec::is_demo() {
        let prepared = first
            .outputs
            .first()
            .ok_or_else(|| "command produced no output".to_string())
            .and_then(prepare);
        match prepared {
            Err(message) => {
                first.result = BatchResult::Failed {
                    target: name.clone(),
                    step: 1,
                    message: crate::model::sanitize(&message),
                }
            }
            Ok((step, files)) => {
                let (second_tx, mut second_rx) = mpsc::channel::<CommandEvent>(16);
                let second_forward = phase_tx.clone();
                let bridge = tokio::spawn(async move {
                    while let Some(mut event) = second_rx.recv().await {
                        event.step = 2;
                        let _ = second_forward.try_send(event);
                    }
                });
                let mut second = run_batch(make_batch(step, files), control, second_tx).await;
                let _ = bridge.await;
                for event in second
                    .started_steps
                    .iter_mut()
                    .chain(second.completed_steps.iter_mut())
                {
                    event.step = 2;
                    event.total = 2;
                }
                if let BatchResult::Failed { step, .. } = &mut second.result {
                    *step = 2;
                }
                first.started_steps.extend(second.started_steps);
                first.completed_steps.extend(second.completed_steps);
                first.completed_targets = second.completed_targets;
                first.result = second.result;
            }
        }
    }
    if !matches!(first.result, BatchResult::Success) {
        first.completed_targets.clear();
    }
    drop(phase_tx);
    let _ = forward.await;
    first
}

/// Render a wizard's XML and define it while retaining the private file.
pub async fn run_wizard_define(
    uri: String,
    name: String,
    print_step: super::plan::CommandStep,
    define_program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let file_name = name.clone();
    run_prepared(uri, name, print_step, control, events, move |out| {
        let bytes = out.structured_stdout().map_err(|error| error.to_string())?;
        let xml = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        if xml.trim().is_empty() {
            return Err("virt-install produced no XML".into());
        }
        let document = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
        if document.root_element().tag_name().name() != "domain" {
            return Err("expected domain XML".into());
        }
        let file = super::tempxml::write(&file_name, xml).map_err(|error| error.to_string())?;
        let path = super::tempxml::path_arg(&file);
        Ok((
            super::plan::CommandStep {
                program: define_program,
                argv: vec!["define".into(), path, "--validate".into()],
                stdin: None,
            },
            vec![file],
        ))
    })
    .await
}

/// Detect the captured domain's CD-ROM and eject its media.
pub async fn run_media_eject(
    uri: String,
    domain: String,
    live: bool,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    run_media_change(uri, domain, None, live, program, control, events).await
}

/// Detect the CD-ROM and apply a captured media change.
pub async fn run_media_change(
    uri: String,
    domain: String,
    iso: Option<String>,
    live: bool,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let first = super::plan::CommandStep {
        program: program.clone(),
        argv: vec!["dumpxml".into(), domain.clone()],
        stdin: None,
    };
    let target_domain = domain.clone();
    run_prepared(uri, domain, first, control, events, move |out| {
        let bytes = out.structured_stdout().map_err(|error| error.to_string())?;
        let xml = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        let config =
            crate::backend::virsh::parse_xml::parse_domain_xml(xml).map_err(|error| error.to_string())?;
        let target = config
            .disks
            .iter()
            .find(|disk| disk.device == "cdrom")
            .map(|disk| disk.target.as_str())
            .filter(|target| !target.is_empty())
            .ok_or_else(|| "no CD-ROM drive found".to_string())?;
        let mut step = super::builders::hardware::change_media(&target_domain, target, iso.as_deref(), live)
            .steps
            .remove(0);
        step.program = program;
        Ok((step, vec![]))
    })
    .await
}

/// Read inactive XML, update a description and define it from a private file.
pub async fn run_description_update(
    uri: String,
    domain: String,
    description: String,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let first = super::plan::CommandStep {
        program: program.clone(),
        argv: vec!["dumpxml".into(), domain.clone(), "--inactive".into()],
        stdin: None,
    };
    let file_name = domain.clone();
    run_prepared(uri, domain, first, control, events, move |out| {
        use xmltree::{Element, XMLNode};
        let bytes = out.structured_stdout().map_err(|error| error.to_string())?;
        let mut root = Element::parse(bytes).map_err(|error| error.to_string())?;
        if root.name != "domain" {
            return Err("expected domain XML".into());
        }
        root.children
            .retain(|child| !matches!(child, XMLNode::Element(element) if element.name == "description"));
        let mut element = Element::new("description");
        element.children.push(XMLNode::Text(description));
        let position = root
            .children
            .iter()
            .position(|child| matches!(child, XMLNode::Element(element) if element.name == "title"))
            .map(|position| position + 1)
            .unwrap_or(0);
        root.children.insert(position, XMLNode::Element(element));
        let mut xml = Vec::new();
        root.write(&mut xml).map_err(|error| error.to_string())?;
        let xml = String::from_utf8(xml).map_err(|error| error.to_string())?;
        let file = super::tempxml::write(&file_name, &xml).map_err(|error| error.to_string())?;
        let path = super::tempxml::path_arg(&file);
        Ok((
            super::plan::CommandStep {
                program,
                argv: vec!["define".into(), path, "--validate".into()],
                stdin: None,
            },
            vec![file],
        ))
    })
    .await
}

/// Read one node device and attach its generated XML to a captured domain.
pub async fn run_hostdev_attach(
    uri: String,
    domain: String,
    node: String,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let first = super::plan::CommandStep {
        program: program.clone(),
        argv: vec!["nodedev-dumpxml".into(), node],
        stdin: None,
    };
    let target = domain.clone();
    run_prepared(uri, domain, first, control, events, move |out| {
        let bytes = out.structured_stdout().map_err(|error| error.to_string())?;
        let xml = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        let hostdev = super::builders::hardware::hostdev_xml(xml)
            .ok_or_else(|| "node is not a valid PCI or USB device".to_string())?;
        let file = super::tempxml::write("hostdev", &hostdev).map_err(|error| error.to_string())?;
        let path = super::tempxml::path_arg(&file);
        Ok((
            super::plan::CommandStep {
                program,
                argv: vec!["attach-device".into(), target, path, "--config".into()],
                stdin: None,
            },
            vec![file],
        ))
    })
    .await
}

/// Apply captured hardware plans with their private XML owned by the worker.
pub async fn run_hardware_apply(
    uri: String,
    domain: String,
    hardware: crate::ui::views::detail::hardware::HardwareState,
    plans: Vec<CommandPlan>,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let failed = |result| BatchOutcome {
        started_steps: vec![],
        completed_steps: vec![],
        outputs: vec![],
        completed_targets: vec![],
        result,
    };
    if *control.borrow() {
        return failed(BatchResult::Cancelled);
    }
    if super::exec::is_demo() {
        return failed(BatchResult::Success);
    }
    let mut steps: Vec<_> = plans.into_iter().flat_map(|plan| plan.steps).collect();
    let needs_xml = steps
        .iter()
        .any(|step| step.program == "virsh" && step.argv.first().is_some_and(|arg| arg == "define"));
    let mut files = Vec::new();
    if needs_xml {
        let xml = hardware.edited_xml();
        if roxmltree::Document::parse(&xml).is_err() {
            return failed(BatchResult::Failed {
                target: domain,
                step: 1,
                message: "invalid edited XML".into(),
            });
        }
        let file = match super::tempxml::write(&domain, &xml) {
            Ok(file) => file,
            Err(error) => {
                return failed(BatchResult::Failed {
                    target: domain,
                    step: 1,
                    message: error.to_string(),
                });
            }
        };
        let path = super::tempxml::path_arg(&file);
        for step in &mut steps {
            if step.program == "virsh" && step.argv.first().is_some_and(|arg| arg == "define") {
                step.argv = vec!["define".into(), path.clone(), "--validate".into()];
            }
        }
        files.push(file);
    }
    for step in &mut steps {
        if step.program == "virsh" {
            step.program.clone_from(&program);
        }
    }
    let plan = CommandPlan {
        summary: format!("Applied hardware changes on {domain}"),
        steps,
    };
    run_batch(
        OwnedBatch {
            uri,
            demo: false,
            dry_run: false,
            plans: vec![(domain, plan)],
            temporary_files: files,
        },
        control,
        events,
    )
    .await
}

/// Export complete domain XML without replacing an existing file.
pub async fn run_xml_export(
    uri: String,
    domain: String,
    path: std::path::PathBuf,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let step = super::plan::CommandStep {
        program,
        argv: vec!["dumpxml".into(), domain.clone()],
        stdin: None,
    };
    let batch = OwnedBatch {
        uri,
        demo: false,
        dry_run: false,
        plans: vec![(
            domain.clone(),
            CommandPlan {
                summary: String::new(),
                steps: vec![step],
            },
        )],
        temporary_files: vec![],
    };
    let mut outcome = run_batch(batch, control.clone(), events).await;
    if matches!(outcome.result, BatchResult::Success) && !super::exec::is_demo() {
        let data = outcome
            .outputs
            .first()
            .ok_or_else(|| "command produced no XML".to_string())
            .and_then(|out| out.structured_stdout().map_err(|error| error.to_string()))
            .and_then(|bytes| std::str::from_utf8(bytes).map_err(|error| error.to_string()))
            .and_then(|xml| {
                roxmltree::Document::parse(xml)
                    .map_err(|error| error.to_string())
                    .and_then(|document| {
                        if document.root_element().tag_name().name() == "domain" {
                            Ok(xml)
                        } else {
                            Err("expected domain XML".into())
                        }
                    })
            });
        let written = data.and_then(|xml| {
            if *control.borrow() {
                return Err("cancelled".into());
            }
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
            file.write_all(xml.as_bytes())
                .map_err(|error| format!("cannot write {}: {error}", path.display()))
        });
        if let Err(message) = written {
            outcome.completed_targets.clear();
            outcome.result = if *control.borrow() {
                BatchResult::Cancelled
            } else {
                BatchResult::Failed {
                    target: domain,
                    step: 1,
                    message: crate::model::sanitize(&message),
                }
            };
        }
    }
    outcome
}

/// Validate edited XML and define it from a private file owned by the job.
pub async fn run_xml_define(
    uri: String,
    target: String,
    xml: String,
    network: bool,
    program: String,
    control: watch::Receiver<bool>,
    events: mpsc::Sender<CommandEvent>,
) -> BatchOutcome {
    let failed = |result| BatchOutcome {
        started_steps: vec![],
        completed_steps: vec![],
        outputs: vec![],
        completed_targets: vec![],
        result,
    };
    if *control.borrow() {
        return failed(BatchResult::Cancelled);
    }
    if super::exec::is_demo() {
        return failed(BatchResult::Success);
    }
    let expected = if network { "network" } else { "domain" };
    let valid = xml.len() <= 1024 * 1024
        && roxmltree::Document::parse(&xml)
            .is_ok_and(|document| document.root_element().tag_name().name() == expected);
    if !valid {
        return failed(BatchResult::Failed {
            target,
            step: 1,
            message: format!("invalid or oversized {expected} XML"),
        });
    }
    let file = match super::tempxml::write(expected, &xml) {
        Ok(file) => file,
        Err(error) => {
            return failed(BatchResult::Failed {
                target,
                step: 1,
                message: error.to_string(),
            });
        }
    };
    let path = super::tempxml::path_arg(&file);
    let step = super::plan::CommandStep {
        program,
        argv: vec![
            if network { "net-define" } else { "define" }.into(),
            path,
            "--validate".into(),
        ],
        stdin: None,
    };
    let plan = CommandPlan {
        summary: format!("Defined {target}"),
        steps: vec![step],
    };
    run_batch(
        OwnedBatch {
            uri,
            demo: false,
            dry_run: false,
            plans: vec![(target, plan)],
            temporary_files: vec![file],
        },
        control,
        events,
    )
    .await
}
