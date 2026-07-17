#!/usr/bin/env python3
"""mDNS launcher for the MPI demo.

Discovers MPI nodes on the local network via mDNS (zeroconf), writes an
OpenMPI hostfile, and launches mpirun. No managed service, single LAN only.

Usage:
    python launcher.py advertise                 # on each worker node
    python launcher.py run [mpirun args...]      # on the coordinator (rank 0)

Examples:
    python launcher.py advertise
    python launcher.py run --mca btl_tcp_if_include 192.168.1.0/24 \\
                           /home/user/demo/target/release/demonstration

Requires: pip install zeroconf   (Python 3.9+)
"""

import argparse
import os
import socket
import subprocess
import sys
import time

from zeroconf import ServiceBrowser, ServiceInfo, Zeroconf

SERVICE_TYPE = "_mpi._tcp.local."
HOSTFILE = "hosts.txt"
PORT = 4242  # only identifies the advert; MPI does not use it


def local_ip() -> str:
    """Best-effort primary LAN IP of this machine (no packets are sent)."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("8.8.8.8", 80))
        return s.getsockname()[0]
    except OSError:
        return "127.0.0.1"
    finally:
        s.close()


def advertise() -> None:
    """Publish this machine on the LAN until interrupted."""
    zc = Zeroconf()
    host = socket.gethostname()
    ip = local_ip()
    info = ServiceInfo(
        SERVICE_TYPE,
        f"mpi-{host}.{SERVICE_TYPE}",
        addresses=[socket.inet_aton(ip)],
        port=PORT,
        properties={"role": "mpi-node"},
        server=f"{host}.local.",
    )
    zc.register_service(info)
    print(f"Advertising mpi-{host} ({ip}) as {SERVICE_TYPE}. Ctrl-C to stop.")
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        pass
    finally:
        zc.unregister_service(info)
        zc.close()


class Collector:
    """Zeroconf listener that resolves and collects discovered hosts."""

    def __init__(self) -> None:
        self.hosts: set[str] = set()

    def add_service(self, zc: Zeroconf, type_: str, name: str) -> None:
        info = zc.get_service_info(type_, name)
        if not info:
            return
        # Prefer an IPv4 address; fall back to the advertised hostname.
        host = next((a for a in info.parsed_addresses() if ":" not in a), None)
        if host is None:
            host = (info.server or "").rstrip(".") or None
        if host:
            self.hosts.add(host)
            print(f"  found {name} -> {host}")

    def update_service(self, zc: Zeroconf, type_: str, name: str) -> None:
        pass

    def remove_service(self, zc: Zeroconf, type_: str, name: str) -> None:
        pass


def run(passthrough: list[str]) -> None:
    """Discover nodes, write the hostfile, and launch mpirun."""
    secs = float(os.environ.get("DISCOVER_SECS", "5"))

    zc = Zeroconf()
    collector = Collector()
    print(f"Browsing {SERVICE_TYPE} for {secs:g}s ...")
    ServiceBrowser(zc, SERVICE_TYPE, collector)
    time.sleep(secs)
    zc.close()

    # This machine is rank 0, so it goes first; then every discovered peer.
    me = local_ip()
    hosts = [me] + [h for h in sorted(collector.hosts) if h != me]

    with open(HOSTFILE, "w") as f:
        f.writelines(f"{h}\n" for h in hosts)

    print(f"\nWrote {HOSTFILE} with {len(hosts)} node(s):")
    for h in hosts:
        print(f"  {h}")

    # Strip an optional "--" separator before the mpirun arguments.
    if passthrough and passthrough[0] == "--":
        passthrough = passthrough[1:]

    cmd = ["mpirun", "--hostfile", HOSTFILE, "-np", str(len(hosts)), *passthrough]
    print("\nLaunching:", " ".join(cmd), "\n")
    sys.exit(subprocess.call(cmd))


def main() -> None:
    parser = argparse.ArgumentParser(description="mDNS launcher for the MPI demo")
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("advertise", help="publish this node on the LAN")
    run_p = sub.add_parser("run", help="discover nodes, write hostfile, launch mpirun")
    run_p.add_argument(
        "mpirun_args",
        nargs=argparse.REMAINDER,
        help="arguments passed through to mpirun (e.g. the binary path)",
    )

    args = parser.parse_args()
    if args.cmd == "advertise":
        advertise()
    else:
        run(args.mpirun_args)


if __name__ == "__main__":
    main()
