# Discord VC Reader

> A private, ultra-lightweight Discord voice bot in Rust that joins an ordinary voice channel and reads aloud **new text messages posted in that exact voice channel's built-in text chat**.

---

## Features

- **Exact Voice Channel Isolation:** Reads *only* messages posted in the active voice channel's own text chat. Messages in `#general`, other text channels, other VCs, threads, or DMs are completely ignored before any TTS processing.
- **Multilingual Support:** Naturally pronounces **English**, **Devanagari Hindi** (`भाई, कल शाम को खेलेंगे।`), **Romanized Hindi**, and **mixed Hinglish** (`Bhai kal evening mein game khelenge?`).
- **OpenAI Realtime Speech:** Uses OpenAI's latest `gpt-realtime-2.1-mini` via the Realtime API with `marin` and `cedar` voices, replacing legacy `tts-1`.
- **Narration Fidelity & Safety:** Checks model transcripts to ensure text is read literally without following prompt injections, answering questions, or dropping negations.
- **Discord DAVE Voice Support:** Natively supports Discord's end-to-end encrypted voice protocol (DAVE) using `songbird 0.6.1` and `davey`.
- **Ultra-Low Memory Footprint:** Built with Rust, running in **~25–45 MB RAM** (fits easily on a small 512MB–2GB VPS).
- **Strict Budget Controls:** Enforces a configurable **$30 monthly budget** with atomic microdollar SQLite reservations across up to 4 servers in the `Asia/Kolkata` timezone.
- **Audience Awareness:** Automatically pauses speech when no human participants are in the voice channel, and disconnects after a 60-second grace period.

---

## Slash Commands (`/reader`)

| Command | Description |
|---|---|
| `/reader join` | Joins the caller's current ordinary VC, posts disclosure, and enables reading |
| `/reader leave` | Cancels pending speech, clears the queue, and disconnects |
| `/reader pause` | Stops current speech, clears queue, remains connected in paused state |
| `/reader resume` | Resumes reading for fresh incoming messages (no backlog replay) |
| `/reader skip` | Skips the message currently being generated or spoken |
| `/reader clear` | Clears all pending queued messages without stopping current speech |
| `/reader status` | Displays session status, queue length, current voice, and monthly usage |
| `/reader voice <name>` | Sets OpenAI voice (`marin` or `cedar`, operator only) |
| `/reader volume <0-100>` | Adjusts playback volume locally (operator only) |
| `/reader help` | Shows usage instructions, language support, and command references |

---

## Quick Start (Developer Setup)

### 1. Prerequisites
- Rust 1.85+ stable toolchain
- OpenSSL / OS crypto libraries
- Discord Bot Token with **Message Content Intent** enabled
- OpenAI API Key

### 2. Configure Environment
```bash
cp .env.example .env
```
Edit `.env` and fill in your keys:
```dotenv
DISCORD_TOKEN=your_bot_token_here
DISCORD_APPLICATION_ID=your_application_id_here
ALLOWED_GUILD_IDS=123456789012345678
OPENAI_API_KEY=sk-proj-...
```

### 3. Verify Configuration
```bash
cargo run -- --check-config
```

### 4. Run the Bot
```bash
cargo run
```

---

## Verification & Code Quality Commands

All commands run cleanly with zero warnings:

```bash
# 1. Verify code formatting
cargo fmt --check

# 2. Strict linter (-D warnings)
cargo clippy --locked --all-targets -- -D warnings

# 3. Complete test suite (23 unit tests + 6 integration tests)
cargo test --locked

# 4. Compile optimized release binary
cargo build --release --locked
```

---

## Deployment

Refer to [`docs/deployment.md`](docs/deployment.md) for full instructions:
- **VPS with Systemd:** Uses [`deploy/vc-reader.service`](deploy/vc-reader.service) with a 1 GiB cgroup memory ceiling.
- **Docker Container:** Multi-stage [`Dockerfile`](Dockerfile) based on `debian:bookworm-slim`.
- **Fly.io / Render:** Containerized deployment with persistent SQLite volumes.

For engineering decisions, architectural notes, and verification matrices, see [`docs/implementation-notes.md`](docs/implementation-notes.md) and [`docs/verification.md`](docs/verification.md).
