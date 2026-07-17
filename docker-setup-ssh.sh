#!/bin/sh
# Build-time setup of the shared demo SSH keypair for the `mpi` user.
#
# mpirun reaches the other nodes over ssh, so every node needs the same key and
# has to trust it. The private key comes in as a BuildKit secret: the mount
# leaves no layer behind, but the key written from it does, so it ships in the
# image and must be treated as public. See README.md.
set -e

secret=/run/secrets/ssh_private_key
ssh_dir=/home/mpi/.ssh

if [ ! -s "$secret" ]; then
    echo 'ERROR: the ssh_private_key build secret is empty or missing.' >&2
    echo '       In CI, check the SSH_PRIVATE_KEY repository secret is set.' >&2
    echo '       Locally, build with:' >&2
    echo '       docker build --secret id=ssh_private_key,src=ssh/id_ed25519 .' >&2
    exit 1
fi

mkdir -p "$ssh_dir"

# Rewrite rather than copy: OpenSSH rejects a key whose final newline is missing,
# which is what pasting into the GitHub secrets box produces. awk terminates every
# record with a newline, restoring it (and drops any CR along the way).
awk '{ sub(/\r$/, ""); print }' "$secret" > "$ssh_dir/id_ed25519"
chmod 600 "$ssh_dir/id_ed25519"

if ! ssh-keygen -y -f "$ssh_dir/id_ed25519" > "$ssh_dir/id_ed25519.pub"; then
    echo 'ERROR: the ssh_private_key build secret is not a valid OpenSSH private key.' >&2
    echo '       Set it from the file rather than pasting it:' >&2
    echo '       gh secret set SSH_PRIVATE_KEY < ssh/id_ed25519' >&2
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
