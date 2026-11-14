//! Entry point: CLI parsing, logging, terminal setup/teardown, runtime start.

use clap::Parser;
use color_eyre::Result;
use tracing_subscriber::EnvFilter;
use virsh_tui::{app, cli::Cli, config::Config};

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut cli = Cli::parse();
    install_panic_hook();
    init_logging()?;
    let config = match &cli.config {
        Some(path) => Config::load_file(std::path::Path::new(path)),
        None => Config::load(),
    };
    let uri_was_flag = !cli.connect.is_empty();
    // No -c: use the default connection from the config.
    if cli.connect.is_empty() {
        cli.connect = config.general.default_uri.clone();
    }

    if cli.doctor {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let failed = runtime.block_on(doctor(&cli, &config, uri_was_flag));
        std::process::exit(i32::from(failed));
    }

    if cli.probe {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        return runtime.block_on(probe(&cli));
    }

    run_tui(&cli, &config)
}

/// Read-only probe: print parsed domains, host info, networks, and pools as JSON.
async fn probe(cli: &Cli) -> Result<()> {
    use virsh_tui::backend::{Backend, virsh::VirshBackend};
    let backend = VirshBackend::new(&cli.connect);
    let domains = backend.list_domains().await?;
    let host = backend.host_info().await.unwrap_or(virsh_tui::model::HostInfo {
        hostname: String::from("unknown"),
        cpu_model: String::from("unknown"),
        threads: 0,
        mem_kib: 0,
    });
    let networks = backend.networks().await.unwrap_or_default();
    let pools = backend.pools().await.unwrap_or_default();
    let out = serde_json::json!({
        "uri": cli.connect,
        "domains": domains,
        "host": host,
        "networks": networks,
        "pools": pools,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

/// `--doctor`: print the report; true when a check failed.
async fn doctor(cli: &Cli, config: &Config, uri_was_flag: bool) -> bool {
    use std::io::IsTerminal;
    use virsh_tui::doctor;
    let tty = std::io::stdout().is_terminal();
    let report = if cli.demo {
        doctor::demo_report()
    } else {
        let size = if tty {
            crossterm::terminal::size().ok()
        } else {
            None
        };
        let ctx = doctor::ctx_from(cli, config, uri_was_flag, size);
        doctor::run(&ctx, &doctor::runner::SystemRunner).await
    };
    let color = tty && std::env::var_os("NO_COLOR").is_none();
    print!("{}", report.to_text(color));
    report.has_failures()
}

fn run_tui(cli: &Cli, config: &Config) -> Result<()> {
    // --dump bypasses the alternate screen.
    if cli.dump.is_some() {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        return runtime.block_on(app::run(cli, config));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let shutdown = runtime.block_on(async { install_signal_hook() })?;
    let observed = shutdown.clone();
    let mut stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableBracketedPaste
    )?;
    let result = runtime.block_on(app::run_with_shutdown(cli, config, shutdown));
    // A hangup can make the last draw fail before the signal task records
    // SIGHUP: give it a moment, so a closed terminal exits as 129 instead
    // of reporting an I/O error to a stderr that no longer exists.
    if result.is_err() && observed.borrow().is_none() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let signal = *observed.borrow();
    drop(runtime);
    let _ = crossterm::execute!(
        stdout,
        crossterm::event::DisableBracketedPaste,
        crossterm::terminal::LeaveAlternateScreen
    );
    let _ = crossterm::terminal::disable_raw_mode();
    if let Some(signal) = signal {
        std::process::exit(128 + signal);
    }
    result
}

/// Register OS signals before the terminal is switched into raw mode.
fn install_signal_hook() -> Result<tokio::sync::watch::Receiver<Option<i32>>> {
    let (sender, receiver) = tokio::sync::watch::channel(None);
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;
        // A closed terminal (SIGHUP) or SIGQUIT must also stop the children
        // (they run in their own process groups and would be orphaned).
        let mut sighup = signal(SignalKind::hangup())?;
        let mut sigquit = signal(SignalKind::quit())?;
        tokio::spawn(async move {
            let signal = tokio::select! {
                _ = sigint.recv() => libc::SIGINT,
                _ = sigterm.recv() => libc::SIGTERM,
                _ = sighup.recv() => libc::SIGHUP,
                _ = sigquit.recv() => libc::SIGQUIT,
            };
            virsh_tui::app::external::notify_shutdown();
            let _ = sender.send(Some(signal));
        });
    }
    #[cfg(not(unix))]
    drop(sender);
    Ok(receiver)
}

fn install_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Worker tasks (jobs, reads) may panic and the app reports and
        // survives that: only a main-thread panic ends the TUI, so only then
        // is the terminal handed back.
        if std::thread::current().name() == Some("main") {
            let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
            let _ = crossterm::terminal::disable_raw_mode();
            hook(info);
        } else {
            tracing::error!("background task panicked: {info}");
        }
    }));
}

/// Log to `$XDG_STATE_HOME/virsh-tui/virsh-tui.log` (appending; the previous
/// log is kept as `.old` past 4 MiB). Logging is best effort: without a state
/// directory, or when the file cannot be opened, the app runs without it.
fn init_logging() -> Result<()> {
    let Some(dir) = directories::ProjectDirs::from("", "", "virsh-tui")
        .and_then(|d| d.state_dir().map(|p| p.to_path_buf()))
    else {
        return Ok(());
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("virsh-tui.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 4 * 1024 * 1024) {
        let _ = std::fs::rename(&path, dir.join("virsh-tui.log.old"));
    }
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return Ok(());
    };
    tracing_subscriber::fmt()
        // RUST_LOG overrides the default level (e.g. RUST_LOG=virsh_tui=debug).
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("virsh_tui=info")),
        )
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .init();
    Ok(())
}
