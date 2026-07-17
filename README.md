# MPI demonstration

A teaching demo of distributed matrix multiplication with [OpenMPI](https://www.open-mpi.org/),
written in C. Built for the *Konzepte verteilter Systeme* course at the University of Augsburg.

## Distributed Demo

Start the container on the hosts network:
```sh
docker run --net=host ghcr.io/elite-se/vs.mpi
```

Then advertise your node:
```sh
launcher advertise
```

The presenter instead runs:
```sh
launcher run
```

## What it does

Computes `C = A × B` (6×4 times 4×5) across all MPI ranks using collective operations:

- **Root (rank 0)** broadcasts `B` to everyone, scatters rows of `A`, gathers the result blocks, and logs the full matrices.
- **Workers** each receive their row-block of `A`, compute their slice of `C`, send it back, and log what they received and produced.

Every rank writes its output to `/tmp/demo.log` inside its container, which is visible in Docker Desktop.

## Layout

| Path | What it is |
| --- | --- |
| `demonstration/` | The MPI demo in C (`main.c` + `Makefile`). |
| `launcher/` | mDNS helper that discovers LAN nodes and launches `mpirun`; also has a local Docker mode. |
| `ssh/id_ed25519` | Shared keypair baked into the image so containers can SSH to each other. |
| `Dockerfile` | Builds the demo binary, the launcher, and sets up SSH. |
| `docker-entrypoint.sh` | Starts `sshd` on port 2222, then drops to the `mpi` user. |

## Running it

### Single container

```sh
docker run --rm -it ghcr.io/<owner>/<repo>:latest
# inside the container:
mpirun -np 4 /workspace/demonstration/demonstration
```

### Local Docker containers (fallback demo)

Run 10 containers on one machine — no LAN required. Runs from the host:

```sh
launcher local --image ghcr.io/<owner>/<repo>:latest
```

Each container's log is visible in Docker Desktop. Press Enter when done to stop everything.

### Distributed over a LAN

On each worker machine, advertise over mDNS:

```sh
launcher advertise
```

On the coordinator:

```sh
launcher run
```

The launcher discovers peers for 5 s (override with `DISCOVER_SECS`), writes a hostfile, and launches `mpirun`. Pass extra flags before the binary:

```sh
launcher run --mca btl_tcp_if_include 192.168.1.0/24
```

## SSH key

The keypair in `ssh/` is baked into every image so containers can SSH to each other without manual setup. **It is not a secret** — anyone with the image can extract it. Do not reuse it outside this demo.

To regenerate (e.g. after a leak):

```sh
ssh-keygen -t ed25519 -N "" -f ssh/id_ed25519
gh secret set SSH_PRIVATE_KEY < ssh/id_ed25519
```

CI reads `SSH_PRIVATE_KEY` from the repository secret and writes it to `ssh/id_ed25519` before building.

## CI

`.github/workflows/docker.yml` builds for `linux/amd64` and `linux/arm64` on native runners and pushes a multi-arch manifest to `ghcr.io/<owner>/<repo>` on every branch push and tag.

## License

[MIT](LICENSE).
