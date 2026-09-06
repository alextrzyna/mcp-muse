# mcp-muse audio engine analysis (2026-09-05)

Findings from a full read of the server, data model, player/mixer, synthesis
engines, preset library, tests and docs. `cargo clippy -- -D warnings` and
`cargo test` were both clean at the time of analysis (commit 381dd88).

Several of the workarounds documented in CLAUDE.md (max 3 effects per channel,
2x gain compensation, volume-corrected presets, drum volume boost) compensate
for the underlying bugs listed in Tier 1.

## Tier 1: bugs affecting every play call

1. **Per-sample logging fills the disk.** `src/midi/player.rs` logged a debug
   line per non-silent sample per channel, and the subscriber ran at TRACE.
   The log directory held 12 GB across 165 daily files; the largest single day
   was 7.6 GB. (Cleaned during this work; only the current day's file was kept.)
2. **Mixer effects chain is stateless.** `ChannelEffectsChain::process_sample`
   calls the processor with a one-sample slice; every effect allocates fresh
   delay buffers per call. Reverb/delay/chorus/compressor/filter therefore only
   attenuate. Every preset triggers this through its signature effects.
3. **Synth filter is a gain multiplier.** `ExpressiveSynth::apply_filter`
   multiplies by a coefficient and adds a sine at the cutoff frequency as
   "resonance". A 700 Hz cutoff scales the signal to ~18% with no filtering.
4. **Pitch sweeps use `sin(2π·f(t)·t)` instead of accumulated phase.** R2D2
   contour, kick, zap and swoosh in both `synth.rs` and `fundsp_synth.rs`.
   Perceived pitch is f + t·f', so descending contours bounce back up.
5. **"FunDSP" routed types discard envelope, filter and effects.** Pad, Kick,
   Snare, HiHat, Cymbal, FM, Granular, Zap, Swoosh, Texture keep only
   frequency, amplitude and duration. The `fundsp` crate is not referenced
   anywhere; neither is `base64`.
6. **Drum routing duplicates the whole MIDI mix.** When a channel-9 note is
   sounding, the entire OxiSynth output is copied to channel 9 at 3x. Output is
   collapsed to mono with 14x gain, nullifying pan/balance.
7. **Resource churn per call.** Each tool call opens two or three audio output
   streams, then leaks the player. (SoundFont parse measured at 78 ms in
   release, so caching it is not worth the complexity.)

## Tier 2: MCP protocol and tool interface

8. Tool failures returned as JSON-RPC errors instead of `isError` results.
   Unknown notifications get "Method not found" responses. `ping` unhandled.
   Null ids serialize as the string "unknown". `serverInfo.version` hardcoded.
9. Fire-and-forget playback: no stop tool, no duration returned, overlapping
   calls play simultaneously.
10. Schema drift: `musical_duration` declared `type: object` with number/string
    alternatives; `MusicalDuration` untagged enum has three identical f64
    variants (Beats, Seconds unreachable); `quantize_grid` never applied; 4/4
    hardcoded in pattern resolution and playback; `organ`/`arp` categories
    advertised but empty (note silently plays as piano); README claims 31
    presets, actual 29.
11. No catalog tool; resources capability advertised but empty.
12. CLAUDE.md and examples reference `play_midi` / `play_r2d2_expression`.

## Tier 3: architecture and hygiene

13. Dead parallel architecture: voice manager and realtime polyphonic source
    unused; `src/tests` not declared; ~1,365 lines of listen-by-ear commands in
    `main.rs`; note-to-synth converter duplicated; three effect type enums.
14. DX7 emulation: algorithm ignored, 3 of 6 operators, detune as radians,
    low-ratio operators summed additively.
15. Naive oscillators alias; chorus uses nearest-neighbour reads.
16. SoundFont download has no checksum.
17. Unused/oversized deps: `fundsp`, `base64`, `tokio` "full", `midly` only
    under `cfg(test)`.
