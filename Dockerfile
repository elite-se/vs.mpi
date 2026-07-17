# syntax=docker/dockerfile:1
FROM rust:1.97-bullseye

# Install OpenMPI and OpenSSH
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libopenmpi-dev \
        openmpi-bin \
        openssh-client \
        openssh-server \
    && rm -rf /var/lib/apt/lists/*

# OpenMPI refuses to run as root, so create an unprivileged user to run under.
# /run/sshd is the runtime directory sshd expects to exist.
RUN useradd --create-home --shell /bin/bash mpi \
    && mkdir -p /run/sshd

# Bake in the shared demo keypair. The key is mandatory and never generated here:
# nodes can only ssh to each other if their images were built from the same key
RUN --mount=type=bind,source=docker-setup-ssh.sh,target=/tmp/setup-ssh.sh \
    --mount=type=secret,id=ssh_private_key,required=true \
    sh /tmp/setup-ssh.sh

WORKDIR /workspace
COPY --chown=mpi:mpi . /workspace

# The entrypoint starts sshd as root, then drops to `mpi` to run this.
# Run e.g. `mpirun -np 4 target/release/demonstration`.
COPY docker-entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["/bin/bash"]
