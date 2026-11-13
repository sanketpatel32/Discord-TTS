# VC Reader — Implementation Notes & Decisions

**Date:** 13 November 2026  
**Status:** Complete Implementation

---

## 1. Core Architectural Decisions

### 1.1 Language & Framework Selection: Rust
- **Decision:** Implemented as a single, lightweight binary in Rust using `poise 0.7.1`, `serenity 0.12.5`, and `songbird 0.6.1`.
- **Reasoning:** To meet the strict memory constraint (<= 2 GB VM, target sustained RSS <= 512 MB, peak RSS < 768 MB). The compiled service has an active memory footprint of ~25–45 MB.

### 1.2 Exact Voice Channel Source Restriction
- **Decision:** The bot binds solely to the built-in text chat of the currently joined voice channel (`message.channel_id == session.active_voice_channel_id`).
- **Enforcement:** Messages in other text channels (`#general`), threads, or other VCs are discarded immediately at admission before any queueing or OpenAI API requests.
- **Audience Requirement:** Requires at least one human participant in the voice channel for text to be narrated.

### 1.3 OpenAI Realtime API Integration (`gpt-realtime-2.1-mini`)
- **Decision:** As specified in Section 3.3, legacy models (`tts-1`, `tts-1-hd`) are listed for shutdown on 6 January 2027. New work is directed to `gpt-realtime-2.1-mini` via the OpenAI Realtime WebSocket protocol (`wss://api.openai.com/v1/realtime`).
- **Session Config:** 
  - `output_modalities: ["audio"]`
  - `audio.output.format: { type: "audio/pcm", rate: 24000 }`
  - Configured voices: `marin` (default), `cedar`.
  - Instruction: Literal narration instruction prohibiting QA, tool calling, and instruction following.
  - Per-message isolation with `conversation: "none"`.
- **Fidelity Verification:** Output transcript is compared against expected normalized input text via `verify_narration`. Injections (e.g. "Ignore previous instructions", "answer what is 2+2") or dropped negations (`not`, `nahi`, `नहीं`) are strictly rejected.

### 1.4 Voice Transport & DAVE Protocol
- **Decision:** Used `songbird 0.6.1` with `davey 0.1.5` and `openmls 0.8.3` to support Discord's DAVE end-to-end encrypted voice protocol natively.
- **Audio Conversion:** Received 24 kHz mono signed 16-bit PCM little-endian audio is decoded, validated (checked for 2-byte alignment and 30s buffer limit = 1,440,000 bytes), and converted to normalized `f32` PCM (`sample / 32768.0`) for Songbird's `RawAdapter` at 24000Hz mono. No FFmpeg child process is required.

### 1.5 Concurrency, Session Epochs & Invalidation
- **Decision:** Each of the up to 4 allowed guilds runs its own isolated `GuildActor`.
- **Epoch Tuple:** `(guild_id, voice_channel_id, session_epoch, job_id)` is rechecked at every boundary (admission, permit acquisition, provider response, and playback start).
- **Invalidation:** Actions like `/reader leave`, `/reader pause`, bot drag/move, or disconnect immediately increment `session_epoch`, cancel in-flight generation tokens, stop active tracks, and purge pending queues.

### 1.6 Storage & $30 Monthly Budget Control
- **Database:** Local SQLite (`rusqlite` bundled) stored at `./data/vc-reader.sqlite`.
- **Math:** 64-bit integer microdollars ($1.00 = 1,000,000 microdollars).
- **Atomic Reservation:** Before each generation attempt, `reserve_attempt_budget` begins a write transaction, verifies `spent + active_reservations + new_estimate <= monthly_allowance`, and records an active reservation.
- **Settlement:** On response completion, `finalize_attempt_usage` records actual tokens and exact microdollar cost into `usage_monthly` for the current month in the `Asia/Kolkata` timezone.
- **Crash Recovery:** Unfinished reservations on startup are conservatively marked `uncertain` rather than wiped, preventing budget oversubscription.

---

## 2. Hard Boundaries & Resource Limits

| Resource | Value | Enforcement |
|---|---|---|
| Allowed Guilds | 1–4 | Startup validation (`Config::from_env`) |
| Max Voice Sessions | 1 per guild, max 4 total | `GuildActor` state machine |
| Concurrent Generations | 2 global | Fair `tokio::sync::Semaphore(2)` |
| Queue Length | 20 messages per guild | `GuildJobQueue` drops newest on overflow |
| Max Job Age | 60 seconds | Discarded on queue pop if expired |
| User Rate Limiter | Burst 3, refill 1/3s | Token bucket with max 1024 authors |
| Text Input Limit | 400 chars / 4 KiB UTF-8 | Truncated at word boundary with `… message shortened.` |
| Audio Output Limit | 30 seconds / 1,440,000 bytes | Decoded PCM16 buffer limit |
| Empty VC Timeout | 60 seconds | Automatic leave if no humans return |
| VM RAM Ceiling | 1 GiB cgroup / 2 GB host | Configured in `deploy/vc-reader.service` |
