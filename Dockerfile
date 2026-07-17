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

WORKDIR /workspace
COPY --chown=mpi:mpi . /workspace

RUN sh /workspace/docker-setup-ssh.sh
RUN cd /workspace/demonstration && make
RUN cd /workspace/launcher \
    && cargo build --release \
    && ln -s /workspace/launcher/target/release/launcher /usr/local/bin/launcher

# The entrypoint starts sshd as root, then drops to `mpi` to run this.
# Run e.g. `mpirun -np 4 /workspace/demonstration/demonstration`.
COPY docker-entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["/bin/bash"]
