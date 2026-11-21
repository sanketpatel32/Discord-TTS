# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Discord community members: server owners who install the bot and everyday voice-channel users who hear it read chat aloud. Confirmed: the landing page serves both equally.

## Product Purpose

VC Reader joins an ordinary Discord voice channel and reads new messages posted in that channel's built-in text chat out loud, so people in voice hear chat without looking at a screen. Success = a message typed in VC chat is spoken naturally within seconds, and nobody mistakes it for spam or a robot.

## Positioning

It reads only the exact voice channel's built-in chat — never #general or other channels — and normalizes real chat stylization (stretched letters, spaced letters, punctuation runs, emoticons, mentions) so the output sounds like a person, not a spelling bee. English, Devanagari Hindi, and Romanized Hinglish are first-class. A free OpenRouter/HeyGen voice route makes it usable without an OpenAI bill; a Rust core keeps it ~25–45 MB RAM.

## Operating Context

Inside Discord: `/reader join` from inside a voice channel, then typing in that VC's chat panel. Operators control voice, volume, pause/skip/clear via slash commands. Hosted on Fly.io; configured by env; persistent SQLite for usage/budget.

## Capabilities and Constraints

- Two speech providers: OpenAI Realtime (default on the Fly deploy, voice `coral`, 9 allowed voices) and OpenRouter `heygen/heygen-voice-1-turbo:free` (voice `nadine`, text-only input, no prosody controls).
- Narration fidelity check rejects responses that answer questions, follow injections, or drop negations.
- Max 4 guilds, 400 chars/message, ~30s audio, per-user rate limits, monthly USD budget with atomic SQLite reservations.
- Reads only humans' messages; pauses when no humans in VC; disconnects after 60s empty.
- Discord DAVE end-to-end-encrypted voice supported via songbird/davey.
- The bot itself is an accessibility aid: VC participants who cannot watch chat still hear it.

## Brand Commitments

- Name: "VC Reader" (bot display name; Discord application id 1570678966795247708).
- Real invite URL (OAuth authorize link) is the primary action.
- `docs/assets/avatar.jpg` is the bot avatar and page favicon.

## Evidence on Hand

- README with verified feature list and command table; docs/deployment.md, docs/implementation-notes.md, docs/verification.md.
- Live deployment on Fly.io (app `discord-tts-vcreader`, region sin).
- No real audio samples, screenshots, testimonials, or user counts on hand — never fabricate them.

## Product Principles

- Prove the mechanism, don't claim quality: show what the bot actually does with real example text.
- Verbatim narration only: the bot never converses, answers, or editorializes.
- Exact-channel isolation is a promise, not a feature flag.
- Human-sounding output comes from honest text normalization, not marketing adjectives.
