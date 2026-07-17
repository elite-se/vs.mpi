#!/bin/sh
set -e

# mpirun reaches the other nodes over ssh, so every container is also a server.
# Host keys are per-container throwaways, generated on first boot.
ssh-keygen -A
/usr/sbin/sshd -p 2222

# OpenMPI refuses to run as root, and sshd needs it, so start sshd above and
# drop to the demo user here. HOME must be set explicitly, otherwise ssh keeps
# looking for its keys in root's home.
exec setpriv --reuid mpi --regid mpi --init-groups \
    env HOME=/home/mpi USER=mpi "$@"
