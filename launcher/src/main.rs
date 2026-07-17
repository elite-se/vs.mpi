//! Launcher for the MPI demo.
//!
//! The presenter runs `run`, which opens a small webserver on port 8080.
//! Workers run `advertise`, type in the presenter's IP, and hit that
//! webserver once to register themselves; `run` reads the connecting
//! peer's address off the socket and remembers it. No mDNS or multicast
//! required — since containers are started with `--net=host`, the peer
//! address the presenter observes is the worker's real LAN address.
//!
//! Usage:
//!     launcher advertise                 # on each worker node
//!     launcher run [mpirun args...]      # on the coordinator (rank 0)

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use crossterm::{cursor, execute, terminal};
use crossterm::event::{read as ct_read, Event, KeyCode, KeyModifiers};
use crossterm::style::Print;

const HOSTFILE: &str = "/tmp/mpi-hosts.txt";
const PORT: u16 = 8080;

#[derive(Parser)]
#[command(about = "launcher for the MPI demo")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Register this node with the presenter.
    Advertise {
        /// IP address of the presenter (running `run`).
        #[arg(long)]
        presenter: Option<String>,
    },
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

/// Register this machine with the presenter's webserver, then idle until interrupted.
fn advertise(presenter: Option<String>) -> Result<(), Box<dyn Error>> {
    let presenter = match presenter {
        Some(p) => p,
        None => prompt_required("Presenter IP")?,
    };
    let ip = local_ip();

    match TcpStream::connect((presenter.as_str(), PORT)) {
        Ok(mut stream) => {
            let _ = stream.write_all(
                format!("GET /register HTTP/1.1\r\nHost: {presenter}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            );
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            println!("Registered with presenter {presenter}:{PORT} as {ip}.");
        }
        Err(e) => {
            eprintln!("Could not reach presenter {presenter}:{PORT}: {e}");
        }
    }

    println!("\n--- /tmp/demo.log ---");
    thread::spawn(|| tail_log("/tmp/demo.log"));

    wait_for_ctrl_c();
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
    let listener = TcpListener::bind(("0.0.0.0", PORT))?;
    let (reg_tx, reg_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let peer = stream.peer_addr().ok().map(|a| a.ip().to_string());
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK",
            );
            if let Some(ip) = peer {
                let _ = reg_tx.send(ip);
            }
        }
    });

    // Signal when the user presses Enter.
    let (enter_tx, enter_rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        let mut line = String::new();
        let _ = io::stdin().lock().read_line(&mut line);
        let _ = enter_tx.send(());
    });

    let me = local_ip();
    let mut discovered: BTreeSet<String> = BTreeSet::new();

    println!("Listening on 0.0.0.0:{PORT} — tell each worker to `advertise` to {me}.");
    println!("Press Enter when all nodes are visible.\n");

    let mut prev_lines: u16 = 0;
    loop {
        // Drain registrations for up to one second.
        let tick = Instant::now() + Duration::from_secs(1);
        loop {
            let now = Instant::now();
            if now >= tick { break; }
            match reg_rx.recv_timeout(tick - now) {
                Ok(ip) => {
                    discovered.insert(ip);
                }
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

    // Allow the presenter to add any nodes that failed to register (e.g. blocked port).
    println!("Add missing node IPs (one per line, blank to continue):");
    loop {
        print!("  IP: ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        let ip = line.trim().to_string();
        if ip.is_empty() { break; }
        discovered.insert(ip);
    }

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
        ("advertise", "Register this node with the presenter"),
        ("run",       "Wait for nodes to register and launch mpirun"),
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
                // Raw mode disables automatic signal generation (ISIG), so
                // Ctrl-C arrives here as a plain key event instead of SIGINT.
                // Handle it explicitly so the menu is always killable.
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    terminal::disable_raw_mode()?;
                    execute!(stdout, cursor::Show)?;
                    std::process::exit(130);
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
        0 => {
            let presenter = prompt_required("Presenter IP")?;
            Ok(Cmd::Advertise { presenter: Some(presenter) })
        }
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
            Cmd::Advertise { presenter } => return advertise(presenter),
            Cmd::Run { binary, mpirun_args } => {
                run(binary, mpirun_args)?;
                cmd = interactive_menu(1)?;
            }
            Cmd::Local { image, count, binary } => return local(image, count, binary),
        }
    }
}
