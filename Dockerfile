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
RUN useradd --create-home --shell /bin/bash mpi

# Bake in the shared demo keypair
COPY ssh/id_ed25519 ssh/id_ed25519.pub /home/mpi/.ssh/
RUN cat /home/mpi/.ssh/id_ed25519.pub >> /home/mpi/.ssh/authorized_keys \
    && printf 'Host *\n    Port 2222\n    StrictHostKeyChecking no\n    UserKnownHostsFile /dev/null\n    LogLevel ERROR\n' \
        > /home/mpi/.ssh/config \
    && chown -R mpi:mpi /home/mpi/.ssh \
    && chmod 700 /home/mpi/.ssh \
    && chmod 600 /home/mpi/.ssh/id_ed25519 /home/mpi/.ssh/authorized_keys \
    && mkdir -p /run/sshd

WORKDIR /workspace
COPY --chown=mpi:mpi . /workspace

# The entrypoint starts sshd as root, then drops to `mpi` to run this.
# Run e.g. `mpirun -np 4 target/release/demonstration`.
COPY docker-entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["/bin/bash"]
