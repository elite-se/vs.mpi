#!/bin/sh
# Build-time setup of the shared demo SSH keypair for the `mpi` user.
#
# mpirun reaches the other nodes over ssh, so every node needs the same key and
# has to trust it. The key is read from ssh/id_ed25519 in the build context,
# which must be present before building — either committed locally or written
# by CI from the SSH_PRIVATE_KEY repository secret.
set -e

secret=/workspace/ssh/id_ed25519
ssh_dir=/home/mpi/.ssh

if [ ! -s "$secret" ]; then
    echo 'ERROR: ssh/id_ed25519 is missing or empty.' >&2
    echo '       Generate a keypair with:' >&2
    echo '       ssh-keygen -t ed25519 -f ssh/id_ed25519 -N ""' >&2
    exit 1
fi

mkdir -p "$ssh_dir"

# Rewrite rather than copy: OpenSSH rejects a key whose final newline is missing,
# which is what pasting into the GitHub secrets box produces. awk terminates every
# record with a newline, restoring it (and drops any CR along the way).
awk '{ sub(/\r$/, ""); print }' "$secret" > "$ssh_dir/id_ed25519"
chmod 600 "$ssh_dir/id_ed25519"

if ! ssh-keygen -y -f "$ssh_dir/id_ed25519" > "$ssh_dir/id_ed25519.pub"; then
    echo 'ERROR: ssh/id_ed25519 is not a valid OpenSSH private key.' >&2
    echo '       Regenerate with: ssh-keygen -t ed25519 -f ssh/id_ed25519 -N ""' >&2
    exit 1
fi

# One shared key: every node authenticates with it and accepts it.
cp "$ssh_dir/id_ed25519.pub" "$ssh_dir/authorized_keys"

# sshd listens on 2222, so the client has to dial it. Host-key checks are off
# because containers are throwaways with a fresh host key on every boot.
cat > "$ssh_dir/config" <<'EOF'
Host *
    Port 2222
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
EOF

chown -R mpi:mpi "$ssh_dir"
chmod 700 "$ssh_dir"
chmod 600 "$ssh_dir/id_ed25519" "$ssh_dir/authorized_keys"
