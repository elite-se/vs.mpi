# MPI demonstration

A small teaching demo of distributed message passing with [OpenMPI](https://www.open-mpi.org/),
written in Rust with the [`mpi`](https://crates.io/crates/mpi) crate. Built for the
*Konzepte verteilter Systeme* course at the University of Augsburg.

The program distributes a dense matrix multiplication `C = A * B` across all MPI
ranks, using `MPI_Pack` to serialize each message by hand.

## How it works

Rank 0 is the **root** and drives the demo; every other rank is a **worker**.

1. The root builds two deterministic input matrices — `A` is `6x4`, `B` is `4x5`,
   both filled with `1, 2, 3, ...` so the result is easy to check by hand — and
   prints them.
2. It splits the rows of `A` across all ranks as evenly as possible (the first
   `M % size` ranks get one extra row) and sends each worker a `Task`: that
   worker's row-block of `A`, the full matrix `B`, and the dimensions.
3. Each worker multiplies its block and sends back a `ResultBlock` carrying the
   rows it computed and their absolute offset in `C`.
4. The root computes its own block, copies each worker's block into place, and
   prints `C`.

The interesting part for the course is `demonstration/src/message.rs`. `Task` and
`ResultBlock` mix fixed-size `i32` dimensions with variable-length `f64` payloads,
so each one is packed field-by-field into a single buffer with `MPI_Pack` and read
back in the same order. Because a block's size varies in both directions, packing
lets every message carry exactly its own block and nothing more.

The problem size is set by the `M`, `K`, and `N` constants in `message.rs`.

## Layout

| Path | What it is |
| --- | --- |
| `demonstration/` | The MPI program itself (root, worker, matrix helpers, packed messages). |
| `launcher/` | Optional mDNS helper that discovers nodes on a LAN and launches `mpirun`. |
| `Dockerfile` | Rust + OpenMPI + sshd image; also used as the devcontainer. |
| `docker-setup-ssh.sh` | Build-time: installs the shared keypair from the build secret. |
| `docker-entrypoint.sh` | Runtime: starts `sshd`, then drops from root to the `mpi` user. |
| `.github/workflows/docker.yml` | Builds the image and pushes it to GHCR. |

## Running it

### In Docker (simplest)

The published image already has Rust, OpenMPI, and the source tree at `/workspace`:

```sh
docker run --rm -it ghcr.io/<owner>/<repo>:latest
# then, inside the container:
cd demonstration
cargo build --release
mpirun -np 4 target/release/demonstration
```

The root waits for you to press Enter before it starts, so you can see the world
size first. A single container needs no SSH — `mpirun` forks the ranks locally.

### As a devcontainer

`.devcontainer/devcontainer.json` builds the same `Dockerfile`, so "Reopen in
Container" in VS Code gives you the toolchain plus rust-analyzer.

The build needs the `ssh_private_key` secret (see [below](#the-demo-ssh-key)), so
you need a copy of the private key at `ssh/id_ed25519` first, and the devcontainer
build has to pass it through:

```jsonc
"build": {
    "dockerfile": "../Dockerfile",
    "options": ["--secret", "id=ssh_private_key,src=ssh/id_ed25519"]
}
```

### Locally

You need OpenMPI and its headers on the host (`libopenmpi-dev` and `openmpi-bin`
on Debian/Ubuntu), since the `mpi` crate links against them:

```sh
cd demonstration
cargo build --release
mpirun -np 4 target/release/demonstration
```

## Running across several nodes

This is where the SSH setup in the image matters: `mpirun` reaches other nodes over
SSH, so every container runs an `sshd` (on port **2222**) as well as being a client.

### Several containers on one host

Put them on a shared Docker network and mount the repo so every node sees the same
binary at the same path:

```sh
docker network create mpi-net
for n in node1 node2; do
  docker run -d --name "$n" --hostname "$n" --network mpi-net \
    -v "$PWD:/workspace" ghcr.io/<owner>/<repo>:latest sleep infinity
done

docker exec -u mpi -it node1 bash
# inside node1:
cd demonstration && cargo build --release
mpirun -np 4 -H node1,node2 target/release/demonstration
```

### Across real machines with the launcher

`launcher/` discovers peers over mDNS instead of you writing a hostfile by hand.
It only works on a single LAN and depends on nothing but the network.

On each worker machine:

```sh
launcher advertise
```

On the coordinator (which becomes rank 0):

```sh
launcher run /path/to/demonstration
```

It browses for `_mpi._tcp.local.` for 5 seconds (override with `DISCOVER_SECS`),
writes `hosts.txt` with itself first and every peer after, then runs `mpirun` with
`-np` set to the node count. Anything you pass after `run` goes through to `mpirun`,
which is how you select an interface if a node is multi-homed:

```sh
launcher run --mca btl_tcp_if_include 192.168.1.0/24 /path/to/demonstration
```

The binary must exist at the same path on every node.

## The demo SSH key

⚠️ **The keypair baked into the image is not a secret.** The build takes the private
key as a BuildKit build secret (`ssh_private_key`), which keeps it out of the build
cache — but the key file it writes is a real layer in the published image. Anyone who
can pull the image can extract the private key.

That is fine for a throwaway classroom demo, and it is the reason the containers trust
each other out of the box. Do not reuse this keypair anywhere else.

The key is **not** optional and the build will not invent one, because two images
built from different keys produce nodes that cannot ssh to each other — a failure
that would otherwise only show up at `mpirun` time. Every build must be given the
same key:

```sh
docker build --secret id=ssh_private_key,src=ssh/id_ed25519 -t mpi-demo .
```

To start over with a fresh key, generate one and update the CI secret to match, or
CI images and local images will no longer interoperate:

```sh
ssh-keygen -t ed25519 -N "" -C mpi-demo-key -f ssh/id_ed25519
gh secret set MPI_SSH_PRIVATE_KEY < ssh/id_ed25519
```

Set the secret from the file as shown rather than pasting it: a paste can drop the
key's trailing newline, which OpenSSH rejects as `invalid format`.

`ssh/id_ed25519` is git-ignored and Docker-ignored, so the private key never lands in
the repo or the build context — it lives only in the CI secret and on your machine.
Keep a copy somewhere safe; it cannot be recovered from the tracked public key.

## CI

`.github/workflows/docker.yml` builds for `linux/amd64` and `linux/arm64` and pushes
to `ghcr.io/<owner>/<repo>` on every branch and tag; pull requests build without
pushing. It reads the private key from the repository secret **`MPI_SSH_PRIVATE_KEY`**
and passes it to the build as the `ssh_private_key` build secret, so you need to set
that secret before the first run.

## License

[MIT](LICENSE).
