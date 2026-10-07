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
    // No -c: use the default connection from the config.
    if cli.connect.is_empty() {
        cli.connect = config.general.default_uri.clone();
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

fn run_tui(cli: &Cli, config: &Config) -> Result<()> {
    // --dump bypasses the alternate screen.
    if cli.dump.is_some() {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        return runtime.block_on(app::run(cli, config));
    }
    let mut stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = runtime.block_on(async {
        install_signal_hook();
        app::run(cli, config).await
    });
    let _ = crossterm::execute!(stdout, crossterm::terminal::LeaveAlternateScreen);
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

/// Restore the terminal on SIGINT/SIGTERM, then exit with a signal code.
/// Must be called from inside a Tokio runtime (registers OS signal handlers).
fn install_signal_hook() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let sigint = signal(SignalKind::interrupt());
        let sigterm = signal(SignalKind::terminate());
        if let (Ok(mut sigint), Ok(mut sigterm)) = (sigint, sigterm) {
            tokio::spawn(async move {
                tokio::select! {
                    _ = sigint.recv() => {},
                    _ = sigterm.recv() => {},
                }
                let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
                let _ = crossterm::terminal::disable_raw_mode();
                std::process::exit(130);
            });
        }
    }
}

fn install_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        hook(info);
    }));
}

fn init_logging() -> Result<()> {
    let dir = directories::ProjectDirs::from("", "", "virsh-tui")
        .and_then(|d| d.state_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("virsh-tui.log");
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::File::create(&dir)?;
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
