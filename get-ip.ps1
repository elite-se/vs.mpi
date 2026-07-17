# Prints the IP address of the network interface used to reach the internet.
# Run this on Windows when the address shown inside the Docker container is a
# WSL-internal address (e.g. 192.168.65.x) instead of your real LAN address.

$socket = New-Object System.Net.Sockets.UdpClient
try {
    $socket.Connect("8.8.8.8", 80)
    $ip = $socket.Client.LocalEndPoint.Address.ToString()
    Write-Host $ip
} finally {
    $socket.Close()
}
