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
RUN useradd --create-home --shell /bin/bash mpi

# Bake in the shared demo keypair, injected at build time rather than committed.
# The secret mount itself leaves no layer behind, but the key we write from it
# does: it ships in the image, so treat this keypair as public.
#
# The secret is optional. CI passes one so that every image of a given tag shares
# a key (including across architectures); a plain `docker build` or a devcontainer
# build has no way to supply it, and gets a throwaway key generated here instead.
# Nodes can only ssh to each other if their images were built with the same key.
RUN --mount=type=secret,id=ssh_private_key \
    mkdir -p /home/mpi/.ssh \
    && if [ -s /run/secrets/ssh_private_key ]; then \
           cp /run/secrets/ssh_private_key /home/mpi/.ssh/id_ed25519; \
       else \
           echo 'WARNING: no ssh_private_key build secret; generating a throwaway keypair.' >&2; \
           ssh-keygen -q -t ed25519 -N '' -C mpi-demo -f /home/mpi/.ssh/id_ed25519; \
       fi \
    && chmod 600 /home/mpi/.ssh/id_ed25519 \
    && ssh-keygen -y -f /home/mpi/.ssh/id_ed25519 > /home/mpi/.ssh/id_ed25519.pub \
    && cat /home/mpi/.ssh/id_ed25519.pub >> /home/mpi/.ssh/authorized_keys \
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
