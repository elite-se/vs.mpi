# MPI Demonstration

Teaching demo of distributed matrix multiplication (`C = A × B`) using OpenMPI collective operations, built for *Konzepte verteilter Systeme* at the University of Augsburg.

## Usage
### Distributed Demo

Start the container on each machine's host network:

```sh
docker run -it --net=host ghcr.io/elite-se/vs.mpi
```

The launcher menu opens automatically. Students select **advertise**; the presenter selects **run**, waits until all nodes appear in the list, then presses Enter to start. Each node's log streams in the terminal and is also visible in Docker Desktop.

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

**Launcher** (`launcher/src/main.rs`) is an interactive mDNS-based orchestrator:

| Mode | What it does |
|------|-------------|
| `advertise` | Publishes this node via mDNS (`_mpi._tcp.local.`) and streams `/tmp/demo.log` |
| `run` | Discovers LAN nodes, writes an OpenMPI hostfile (`slots=1` per host), launches `mpirun`, streams `/tmp/demo.log` from rank 0, then returns to the menu |
| `local` | Creates a bridge network, starts N containers, runs the demo inside them |

Nodes find each other via mDNS. SSH transport (port 2222, `StrictHostKeyChecking=no`) is pre-configured with a shared keypair so `mpirun` can reach workers without any manual setup.

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
