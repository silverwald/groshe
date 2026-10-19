# Self-Hosted Iroh Relay Server Setup Guide

Groshe is designed from the ground up for total sovereignty and privacy. By default, Groshe peers discover each other using public community relays run by Number 0 (n0) for NAT hole-punching and fallback DERP-style relaying when direct UDP paths are blocked.

For completely sovereign, air-gapped, or privacy-critical environments, you can host your own private relay server. With a private relay, zero metadata or traffic ever touches third-party infrastructure.

---

## 1. How the Groshe Relay Works

The relay server fulfills two core roles in Groshe's P2P networking stack:
1. **STUN / NAT Discovery**: Peers contact the relay over UDP (port 3478) to discover their external WAN IP address and UDP port mapping, which enables direct peer-to-peer QUIC hole-punching.
2. **Encrypted Fallback Relaying**: When peers are behind strict, symmetric, or CGNAT firewalls that prevent direct UDP punch-through, traffic is end-to-end encrypted (Noise / TLS 1.3) and forwarded through the relay server over standard HTTPS (port 443). The relay cannot read or decrypt the contents of your grocery lists or photos.

---

## 2. Server Requirements

- A Linux VPS (Ubuntu 22.04+ or Debian 12 recommended)
- 1 vCPU, 512 MB RAM (the relay is written in Rust and is extremely lightweight)
- A public static IPv4 address
- A domain name with an `A` record pointing to your server (e.g., `relay.example.com`)
- Inbound firewall ports:
  - `443/tcp`: HTTPS / WebSocket relay traffic
  - `3478/udp`: STUN server for NAT discovery
  - `80/tcp`: HTTP (only needed for Let's Encrypt certificate renewal)

---

## 3. Installation & Deployment

### Option A: Running with Docker (Recommended)

1. Obtain a valid TLS certificate using Certbot:
   ```bash
   sudo apt-get update && sudo apt-get install -y certbot
   sudo certbot certonly --standalone -d relay.example.com
   ```

2. Run the `iroh-relay` container:
   ```bash
   docker run -d \
     --name iroh-relay \
     --restart unless-stopped \
     -p 443:443/tcp \
     -p 3478:3478/udp \
     -v /etc/letsencrypt:/certs:ro \
     n0computer/iroh-relay:latest \
     --http-bind-addr 0.0.0.0:443 \
     --stun-bind-addr 0.0.0.0:3478 \
     --cert-path /certs/live/relay.example.com/fullchain.pem \
     --key-path /certs/live/relay.example.com/privkey.pem
   ```

### Option B: Compiling from Source with Cargo

1. Install Rust:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Install the `iroh-relay` binary:
   ```bash
   cargo install iroh-relay --locked
   ```

3. Create a `systemd` service (`/etc/systemd/system/iroh-relay.service`):
   ```ini
   [Unit]
   Description=Iroh Sovereign Relay Server for Groshe
   After=network.target

   [Service]
   Type=simple
   User=root
   ExecStart=/root/.cargo/bin/iroh-relay \
     --http-bind-addr 0.0.0.0:443 \
     --stun-bind-addr 0.0.0.0:3478 \
     --cert-path /etc/letsencrypt/live/relay.example.com/fullchain.pem \
     --key-path /etc/letsencrypt/live/relay.example.com/privkey.pem
   Restart=always
   RestartSec=5

   [Install]
   WantedBy=multi-user.target
   ```

4. Enable and start the service:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable --now iroh-relay
   ```

---

## 4. Configuring Groshe Clients

Once your relay is running, configure Groshe to point to your sovereign relay.

### Configuration File (`groshe.toml`)

Create or edit `groshe.toml` located in your data directory (e.g., `~/.local/share/groshe/` on Linux, `%APPDATA%\groshe\` on Windows, or the Android app internal data folder):

```toml
# Groshe P2P Node Configuration
# Documentation: docs/RELAY_SETUP.md

[network]
# Set to your self-hosted relay URL:
relay_url = "https://relay.example.com"
```

### Local / Air-Gapped Networks (Zero WAN Relays)

If you are running in an air-gapped environment or want to restrict synchronization entirely to local Wi-Fi / LAN broadcast (via local mDNS and UDP):

```toml
[network]
# Disable all external WAN relays entirely:
relay_url = "disabled"
```

---

## 5. Verification

To verify that your CLI or mobile client is connected to your sovereign relay:

Run the `peers` command:
```bash
grocery peers
```

Example output:
```text
Node ID:       k7b4...28da
Author ID:     4c8e...12fa
Target List:   019315bc...
Listening on:  [192.168.1.120:53210, 203.0.113.45:53210]
Relay (Custom): https://relay.example.com
```

In the Android app:
Open **Settings** (`⚙️`) and verify that **Relay Server** indicates `Custom: https://relay.example.com`.