# MPI Demonstration

Teaching demo of distributed matrix multiplication (`C = A × B`) using OpenMPI collective operations, built for *Konzepte verteilter Systeme* at the University of Augsburg.

## Usage
### Distributed Demo

Start the container on each machine's host network:

```sh
docker run -it --net=host ghcr.io/elite-se/vs.mpi
```

The launcher menu opens automatically. Node discovery is manual:

1. Each student selects **advertise**. It prints that node's IP address — the student reads it out to the presenter and leaves it running.
2. The presenter selects **run** and types in every node's IP (their own machine first, as rank 0), then a blank line to launch.

Each node's log streams in the terminal and is also visible in Docker Desktop. Press **Ctrl-C** at any point to quit.

**Windows note:** Docker Desktop on Windows runs containers inside WSL2, so the address `advertise` shows may be a WSL/Docker-internal one (`172.17.x`, `192.168.65.x`) instead of the real LAN address. `advertise` flags this and reminds you to start the container with `--net=host`. On Windows/Mac host networking still can't reach the LAN, so get the real IP from the host instead:

```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass; irm https://raw.githubusercontent.com/elite-se/vs.mpi/main/get-ip.ps1 | iex
```

If nodes can't reach each other over the LAN at all (common with Docker Desktop on Windows), use the **Local Fallback** below instead.

### Local Fallback

Run everything on one machine — no LAN required:

```sh
docker run -it ghcr.io/elite-se/vs.mpi
# select: local
```

This starts 11 containers on a private bridge network, one rank each. Logs are visible per-container in Docker Desktop.

## How it works

The image bundles two components:

**Demonstration** (`demonstration/main.c`) computes `C = A × B` (6×4 × 4×5) in four steps:

1. Root builds `A` and `B`, then broadcasts `B` to all ranks via `MPI_Bcast`
2. Root distributes row-blocks of `A` via `MPI_Scatterv` — each rank gets a contiguous slice
3. Every rank multiplies its block locally and computes its rows of `C`
4. Root collects the slices via `MPI_Gatherv` and logs the assembled result

**Launcher** (`launcher/src/main.rs`) is an interactive orchestrator:

| Mode | What it does |
|------|-------------|
| `advertise` | Prints this node's IP (warning if it looks Docker-internal), then idles so the container's sshd stays reachable and streams `/tmp/demo.log` |
| `run` | Prompts for every node's IP, writes an OpenMPI hostfile (`slots=1` per host), launches `mpirun`, streams `/tmp/demo.log` from rank 0, then returns to the menu |
| `local` | Creates a bridge network, starts N containers, runs the demo inside them |

Discovery is deliberately manual: the presenter types the IPs that workers read out from `advertise`. `mpirun` then reaches the workers over SSH (port 2222, `StrictHostKeyChecking=no`), which is pre-configured with a shared keypair baked into the image so no per-node setup is needed. This keeps the launcher simple, but it does require that every node is directly reachable on the LAN — which is why the local fallback exists for Windows/Docker-Desktop setups where that isn't the case.

## SSH Key

The keypair in `ssh/` is baked into every image. **It is not a secret** — do not reuse it outside this demo.

To regenerate:

```sh
ssh-keygen -t ed25519 -N "" -f ssh/id_ed25519
gh secret set SSH_PRIVATE_KEY < ssh/id_ed25519
```

CI writes the secret to `ssh/id_ed25519` before building, so the key in the image is always current.

## CI

`.github/workflows/docker.yml` builds for `linux/amd64` and `linux/arm64` and pushes a multi-arch manifest to `ghcr.io/elite-se/vs.mpi` on every push.

## License
[MIT](LICENSE).
