//! mDNS launcher for the MPI demo.
//!
//! Discovers MPI nodes on the local network via mDNS, writes an OpenMPI
//! hostfile, and launches mpirun. No managed service, single LAN only.
//!
//! Usage:
//!     launcher advertise                 # on each worker node
//!     launcher run [mpirun args...]      # on the coordinator (rank 0)
//!
//! Examples:
//!     launcher advertise
//!     launcher run --mca btl_tcp_if_include 192.168.1.0/24
//!     launcher run --binary /workspace/demonstration/demonstration

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, Write};
use std::net::UdpSocket;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use crossterm::{cursor, execute, terminal};
use crossterm::event::{read as ct_read, Event, KeyCode};
use crossterm::style::Print;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

const SERVICE_TYPE: &str = "_mpi._tcp.local.";
const HOSTFILE: &str = "/tmp/mpi-hosts.txt";
const PORT: u16 = 4242; // only identifies the advert; MPI does not use it

#[derive(Parser)]
#[command(about = "mDNS launcher for the MPI demo")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Publish this node on the LAN.
    Advertise,
    /// Discover nodes, write hostfile, launch mpirun.
    Run {
        /// MPI binary to execute on all nodes.
        #[arg(long, default_value = "./demonstration/demonstration")]
        binary: String,
        /// Extra arguments forwarded to mpirun before the binary (e.g. --mca flags).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        mpirun_args: Vec<String>,
    },
    /// Start local Docker containers as a self-contained fallback demo.
    Local {
        /// Docker image to start containers from (e.g. ghcr.io/org/repo:latest).
        #[arg(long)]
        image: String,
        /// Number of containers (MPI processes) to start.
        #[arg(long, default_value_t = 11usize)]
        count: usize,
        /// MPI binary to execute inside the containers.
        #[arg(long, default_value = "/workspace/demonstration/demonstration")]
        binary: String,
    },
}

/// Best-effort primary LAN IP of this machine (no packets are sent).
fn local_ip() -> String {
    let fallback = || "127.0.0.1".to_string();
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return fallback();
    };
    match sock.connect(("8.8.8.8", 80)).and_then(|()| sock.local_addr()) {
        Ok(addr) => addr.ip().to_string(),
        Err(_) => fallback(),
    }
}

/// Publish this machine on the LAN until interrupted.
fn advertise() -> Result<(), Box<dyn Error>> {
    let daemon = ServiceDaemon::new()?;
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let ip = local_ip();

    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &format!("mpi-{host}"),
        &format!("{host}.local."),
        ip.as_str(),
        PORT,
        &[("role", "mpi-node")][..],
    )?;
    let fullname = info.get_fullname().to_string();
    daemon.register(info)?;
    println!("Advertising mpi-{host} ({ip}) as {SERVICE_TYPE}. Ctrl-C to stop.");
    println!("\n--- /tmp/demo.log ---");
    thread::spawn(|| tail_log("/tmp/demo.log"));

    wait_for_ctrl_c();

    if let Ok(rx) = daemon.unregister(&fullname) {
        let _ = rx.recv();
    }
    let _ = daemon.shutdown();
    Ok(())
}

fn wait_for_ctrl_c() {
    let (tx, rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .expect("failed to set Ctrl-C handler");
    let _ = rx.recv();
}

/// Discover nodes, write the hostfile, and launch mpirun.
fn run(binary: String, mut passthrough: Vec<String>) -> Result<(), Box<dyn Error>> {
    let daemon = ServiceDaemon::new()?;
    let mdns_rx = daemon.browse(SERVICE_TYPE)?;

    // Signal when the user presses Enter.
    let (enter_tx, enter_rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        let mut line = String::new();
        let _ = io::stdin().lock().read_line(&mut line);
        let _ = enter_tx.send(());
    });

    let me = local_ip();
    let mut discovered: BTreeSet<String> = BTreeSet::new();

    println!("Browsing {SERVICE_TYPE} — press Enter when all nodes are visible.\n");

    let mut prev_lines: u16 = 0;
    loop {
        // Drain mDNS events for up to one second.
        let tick = Instant::now() + Duration::from_secs(1);
        loop {
            let now = Instant::now();
            if now >= tick { break; }
            match mdns_rx.recv_timeout(tick - now) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    // Prefer an IPv4 address; fall back to the advertised hostname.
                    let host = info
                        .get_addresses_v4()
                        .iter()
                        .min()
                        .map(|a| a.to_string())
                        .or_else(|| {
                            let name = info.get_hostname().trim_end_matches('.');
                            (!name.is_empty()).then(|| name.to_string())
                        });
                    if let Some(host) = host {
                        discovered.insert(host);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }

        // Redraw the node list in-place using MoveUp so we don't rely on
        // SavePosition/RestorePosition (\x1B7/\x1B8), which aren't forwarded
        // reliably through Docker's PTY proxy on Windows.
        let peers: Vec<&String> = discovered.iter().filter(|h| *h != &me).collect();
        if prev_lines > 0 {
            execute!(io::stdout(),
                cursor::MoveUp(prev_lines),
                cursor::MoveToColumn(0),
                terminal::Clear(terminal::ClearType::FromCursorDown)
            )?;
        }
        println!("  ● {me}  (you)");
        for p in &peers {
            println!("  ○ {p}");
        }
        println!();
        print!("  {} node(s) — press Enter to start", 1 + peers.len());
        let _ = io::stdout().flush();
        // 1 (me) + peers.len() + 1 (blank) lines sit above the cursor.
        prev_lines = peers.len() as u16 + 2;

        if enter_rx.try_recv().is_ok() {
            println!("\n");
            break;
        }
    }

    let _ = daemon.shutdown();

    // This machine is rank 0, so it goes first; then every discovered peer.
    let me = local_ip();
    let mut hosts = vec![me.clone()];
    hosts.extend(discovered.into_iter().filter(|h| *h != me));

    fs::write(
        HOSTFILE,
        hosts.iter().map(|h| format!("{h} slots=1\n")).collect::<String>(),
    )?;

    println!("\nWrote {HOSTFILE} with {} node(s):", hosts.len());
    for h in &hosts {
        println!("  {h}");
    }

    // Strip an optional "--" separator before the mpirun arguments.
    if passthrough.first().is_some_and(|s| s == "--") {
        passthrough.remove(0);
    }

    let np = hosts.len().to_string();
    let args: Vec<&str> = ["--hostfile", HOSTFILE, "-np", &np]
        .into_iter()
        .chain(passthrough.iter().map(String::as_str))
        .chain(std::iter::once(binary.as_str()))
        .collect();
    println!("\nLaunching: mpirun {}\n", args.join(" "));

    let mut child = Command::new("mpirun").args(&args).spawn()?;

    println!("--- /tmp/demo.log  ---\n");
    thread::spawn(|| tail_log("/tmp/demo.log"));

    let _ = child.wait();
    thread::sleep(Duration::from_millis(500));
    Ok(())
}

/// Start N local containers on a private bridge network and run the demo inside them.
fn local(image: String, count: usize, binary: String) -> Result<(), Box<dyn Error>> {
    const NETWORK: &str = "mpi-local";
    let names: Vec<String> = (0..count).map(|i| format!("mpi-local-{i}")).collect();

    let outcome = run_local_demo(&names, &image, &binary, count, NETWORK);

    print!("\nContainers are still running. Press Enter to stop them and clean up... ");
    let _ = io::stdout().flush();
    let _ = io::stdin().lock().read_line(&mut String::new());

    println!("Cleaning up...");
    for name in &names {
        let _ = Command::new("docker").args(["stop", name]).output();
    }
    let _ = Command::new("docker").args(["network", "rm", NETWORK]).output();

    match outcome {
        Ok(code) => std::process::exit(code),
        Err(e) => Err(e),
    }
}

fn run_local_demo(
    names: &[String],
    image: &str,
    binary: &str,
    count: usize,
    network: &str,
) -> Result<i32, Box<dyn Error>> {
    // Remove any leftover network from a previous crashed run.
    let _ = Command::new("docker").args(["network", "rm", network]).output();

    println!("Creating Docker network '{network}'...");
    if !Command::new("docker")
        .args(["network", "create", network])
        .status()?
        .success()
    {
        return Err("docker network create failed".into());
    }

    println!("Starting {count} containers from '{image}'...");
    for name in names {
        if !Command::new("docker")
            .args([
                "run", "-d",
                "--name", name,
                "--hostname", name,
                "--network", network,
                "--rm",
                image,
                "sh", "-c", "touch /tmp/demo.log && exec tail -n +1 -f /tmp/demo.log",
            ])
            .status()?
            .success()
        {
            return Err(format!("failed to start container '{name}'").into());
        }
        println!("  started {name}");
    }

    // Give sshd time to come up in every container.
    println!("Waiting for sshd...");
    thread::sleep(Duration::from_secs(2));

    // Write the hostfile into the coordinator via stdin so we avoid shell-escaping issues.
    let mut child = Command::new("docker")
        .args(["exec", "-i", &names[0], "sh", "-c", "cat > /tmp/mpi-hosts"])
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        for name in names {
            writeln!(stdin, "{name} slots=1")?;
        }
    }
    child.wait()?;

    // Run mpirun inside the coordinator as the `mpi` user (OpenMPI refuses root).
    // The SSH client config baked into the image already sets port 2222,
    // StrictHostKeyChecking=no, and UserKnownHostsFile=/dev/null.
    // Each container's CMD is `tail -n +1 -f /tmp/demo.log`, so its output is
    // visible in Docker Desktop without the launcher needing to stream it.
    let np = count.to_string();
    println!("Launching: mpirun -np {np} {binary}\n");
    let status = Command::new("docker")
        .args([
            "exec", "-u", "mpi", &names[0],
            "mpirun",
            "--hostfile", "/tmp/mpi-hosts",
            "-np", &np,
            binary,
        ])
        .status()?;

    Ok(status.code().unwrap_or(1))
}

fn tail_log(path: &str) {
    // Touch the file so it exists before the MPI program starts writing, then close it.
    if let Err(e) = fs::OpenOptions::new().create(true).append(true).open(path) {
        eprintln!("tail_log: cannot create {path}: {e}");
        return;
    }
    // Re-open read-only so the write handle is released before we start streaming.
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => { eprintln!("tail_log: cannot open {path}: {e}"); return; }
    };
    let mut reader = io::BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(100)),
            Ok(_) => {
                print!("{line}");
                let _ = io::stdout().flush();
            }
            Err(_) => break,
        }
    }
}

fn prompt(label: &str, default: &str) -> Result<String, Box<dyn Error>> {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{}]: ", default);
    }
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let trimmed = line.trim().to_string();
    Ok(if trimmed.is_empty() { default.to_string() } else { trimmed })
}

fn prompt_required(label: &str) -> Result<String, Box<dyn Error>> {
    loop {
        print!("{label}: ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
        eprintln!("  (required — please enter a value)");
    }
}

fn interactive_menu(initial_sel: usize) -> Result<Cmd, Box<dyn Error>> {
    const OPTIONS: &[(&str, &str)] = &[
        ("advertise", "Publish this node on the LAN via mDNS"),
        ("run",       "Discover LAN nodes and launch mpirun"),
        ("local",     "Start local Docker containers (fallback demo)"),
    ];

    let mut sel = initial_sel.min(OPTIONS.len() - 1);
    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;
    execute!(stdout, cursor::Hide)?;

    execute!(stdout, Print("Select a command:\r\n\r\n"))?;

    let draw = |sel: usize, stdout: &mut dyn Write| -> io::Result<()> {
        for (i, (name, desc)) in OPTIONS.iter().enumerate() {
            let marker = if i == sel { '>' } else { ' ' };
            write!(stdout, "  {marker} {name:<12} {desc}\r\n")?;
        }
        write!(stdout, "\r\n  \u{2191}/\u{2193} move  Enter select  q quit")?;
        stdout.flush()
    };

    draw(sel, &mut stdout)?;

    // N option rows + 1 blank row sit above the instruction line where the cursor rests.
    let menu_lines = OPTIONS.len() as u16 + 1;

    let selected = loop {
        if let Event::Key(key) = ct_read()? {
            match key.code {
                KeyCode::Up   => { if sel > 0 { sel -= 1; } }
                KeyCode::Down => { if sel + 1 < OPTIONS.len() { sel += 1; } }
                KeyCode::Enter => break sel,
                KeyCode::Char('q') | KeyCode::Esc => {
                    terminal::disable_raw_mode()?;
                    execute!(stdout, cursor::Show)?;
                    std::process::exit(0);
                }
                _ => continue,
            }
            execute!(
                stdout,
                cursor::MoveUp(menu_lines),
                cursor::MoveToColumn(0),
                terminal::Clear(terminal::ClearType::FromCursorDown),
            )?;
            draw(sel, &mut stdout)?;
        }
    };

    terminal::disable_raw_mode()?;
    execute!(stdout, cursor::Show)?;
    println!("\r\n");

    match selected {
        0 => Ok(Cmd::Advertise),
        1 => {
            let binary = prompt("Binary", "./demonstration/demonstration")?;
            let args_str = prompt("Extra mpirun args (space-separated, blank for none)", "")?;
            let mpirun_args = if args_str.is_empty() {
                vec![]
            } else {
                args_str.split_whitespace().map(String::from).collect()
            };
            Ok(Cmd::Run { binary, mpirun_args })
        }
        2 => {
            let image = prompt_required("Docker image")?;
            let count_str = prompt("Container count", "11")?;
            let count = count_str.parse().unwrap_or(11);
            let binary = prompt(
                "Binary inside container",
                "/workspace/demonstration/demonstration",
            )?;
            Ok(Cmd::Local { image, count, binary })
        }
        _ => unreachable!(),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cmd = match Cli::parse().cmd {
        Some(cmd) => cmd,
        None => {
            use std::io::IsTerminal;
            if !io::stdin().is_terminal() {
                eprintln!("Error: no subcommand given and stdin is not a terminal.");
                eprintln!("  Hint: docker run -it --net=host ghcr.io/elite-se/vs.mpi");
                std::process::exit(1);
            }
            interactive_menu(0)?
        }
    };
    loop {
        match cmd {
            Cmd::Advertise => return advertise(),
            Cmd::Run { binary, mpirun_args } => {
                run(binary, mpirun_args)?;
                cmd = interactive_menu(1)?;
            }
            Cmd::Local { image, count, binary } => return local(image, count, binary),
        }
    }
}
