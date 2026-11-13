# VC Reader — Deployment & Operations Guide

This guide describes how to run **VC Reader** on a small Linux Virtual Machine (VPS) or cloud container platform with at most 2 GB of RAM.

---

## 1. Hosting Requirements

- **Operating System:** Linux (Ubuntu 22.04/24.04 LTS, Debian 12 Bookworm, etc.)
- **Memory:** 512 MB to 2 GB RAM (Service uses < 100 MB RSS under load)
- **Disk:** 10 GB persistent storage (for OS, binary, and SQLite database)
- **Network:**
  - Outbound TCP/HTTPS (`443`) and WSS to `discord.com`, `gateway.discord.gg`, and `api.openai.com`.
  - Outbound and bidirectional UDP traffic for Discord voice RTP transport.
  - No inbound open ports or domain reverse proxies are needed.

---

## 2. Discord Developer Portal Setup

1. Go to [Discord Developer Portal](https://discord.com/developers/applications) and create a **New Application** (e.g. `VC Reader`).
2. Navigate to **Bot**:
   - Reset and copy the **Bot Token** (`DISCORD_TOKEN`).
   - Under **Privileged Gateway Intents**, enable:
     - [x] **Message Content Intent** (Required to read new messages in the VC text chat)
     - [x] **Server Members Intent** (Optional, not strictly required)
3. Navigate to **OAuth2 -> URL Generator**:
   - Scopes: `bot`, `applications.commands`
   - Bot Permissions:
     - `View Channels`
     - `Send Messages`
     - `Connect`
     - `Speak`
   - Copy the generated URL and invite the bot to your allowed server(s).

---

## 3. Deployment Option A: Native Binary with Systemd (Recommended for VPS)

### Step 1: Install runtime dependencies
```bash
sudo apt-get update
sudo apt-get install -y ca-certificates libopus0 libssl3
```

### Step 2: Build release binary
On your build machine (or server):
```bash
cargo build --release --locked
```
Copy `target/release/vc-reader` to `/usr/local/bin/vc-reader`:
```bash
sudo cp target/release/vc-reader /usr/local/bin/vc-reader
sudo chmod +x /usr/local/bin/vc-reader
```

### Step 3: Create dedicated service user and directory
```bash
sudo useradd -r -s /bin/false -U vcreader
sudo mkdir -p /var/lib/vc-reader/data
sudo chown -R vcreader:vcreader /var/lib/vc-reader
```

### Step 4: Configure environment
Create `/etc/vc-reader.env`:
```bash
sudo cp .env.example /etc/vc-reader.env
sudo chmod 600 /etc/vc-reader.env
sudo chown vcreader:vcreader /etc/vc-reader.env
```
Edit `/etc/vc-reader.env` with your actual Discord token, application ID, allowed guild IDs, and OpenAI API key.

Verify configuration before running:
```bash
sudo -u vcreader /usr/local/bin/vc-reader --check-config
```

### Step 5: Install systemd service
```bash
sudo cp deploy/vc-reader.service /etc/systemd/system/vc-reader.service
sudo systemctl daemon-reload
sudo systemctl enable --now vc-reader
```

### Step 6: Monitor logs
```bash
sudo journalctl -u vc-reader -f
```

---

## 4. Deployment Option B: Docker Container

Build and run using Docker:
```bash
docker build -t vc-reader:latest .

docker run -d \
  --name vc-reader \
  --restart unless-stopped \
  --env-file .env \
  -v $(pwd)/data:/app/data \
  --memory=1g \
  vc-reader:latest
```

---

## 5. Deployment Option C: Fly.io

1. Install `flyctl`: `curl -L https://fly.io/install.sh | sh`
2. Run `fly launch` (select 512MB RAM machine).
3. Create persistent storage volume for SQLite:
   ```bash
   fly volumes create vc_reader_data --size 1 -r <region>
   ```
4. Set secrets:
   ```bash
   fly secrets set DISCORD_TOKEN="..." OPENAI_API_KEY="..." ALLOWED_GUILD_IDS="..."
   ```
5. Deploy:
   ```bash
   fly deploy
   ```

---

## 6. Verification and Troubleshooting

- **Bot not reading messages:**
  - Verify that **Message Content Intent** is checked in Discord Developer Portal.
  - Verify that you ran `/reader join` in the voice channel.
  - Verify that you are typing in **that specific voice channel's text chat**, not `#general`.
- **Bot leaves immediately:**
  - Check if any humans are in the voice channel. The bot pauses/stops reading if there is no human audience.
- **Budget Exceeded message:**
  - Check `/reader status`. If monthly spend reached the $30 allowance, the bot halts paid generation until the calendar month resets in `Asia/Kolkata`.
