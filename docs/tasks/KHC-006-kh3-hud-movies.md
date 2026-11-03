# KHC-006: KH3's own command deck and shotlock display

| | |
|---|---|
| Gate | None; the owner's request of 2026-11-01: "i want the original kh3 command deck, shocklock, using ghidra extract those assets" |
| Owner | burba |
| Prerequisites | KHC-004, KHC-005 |
| Source | The player's Steam KH3 install, only read, at run time. Nothing from it is in this repository |

## What was done

KH3's HUD is Scaleform GFx: `UI/Scaleform/01_hud/hud_command/HudCommand` and `01_hud/hud_target/HudTarget` are `SwfMovie` packages, with their textures beside them and the fonts in `91_font/TresFont`. So the art comes from the paks. Ghidra was not needed, and no new reverse engineering was done: the display rules are the ported ones from StrawHatPro's kh-ui and its `sora-ui-spec.md`, which cite the executable.

- `source/kh3_ui` is new, ported from StrawHatPro's kh-ui (MIT): the timeline player, the software compositor, `HudCommand`, `HudTarget` and the display half of the command system.
- `gfx.rs` reads a GFx movie directly: tags, shapes, morphs, sprites, text fields, compacted fonts, the frame scripts out of its AS3 bytecode, and BC7, BC1, BC3, BC4 and BC5 textures to RGBA. The reference does this offline in Node; here it happens at load, in Rust, on the KH3 load thread. `load.rs` finds the packages through `kh3_data`.
- `kh3_deck.rs` keeps its state machine and now builds a `CommandModel` each tick for the movie. The real deck is drawn, and the engine's KH2 deck stays underneath, hidden (`Game::hide_command_menu`).
- The shotlock's scope, lock counter, MAX and marks on the target play from `HudTarget`, and the deck switches to its SHOTLOCK panel. The old "SHOTLOCK n / 24" text line goes when the movies are up.
- Each movie is drawn on its own thread, and only when its state changed. A frame never waits for one: a movie that changes during a draw waits its turn.

A failure to read the movies leaves the KH2 deck and the old text line, and logs `kh3_hud_failed`.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `hud6` and `hud9` (contact sheets of the deck, the lists and the shotlock), `hud10`, `hudduo1`, and the final runs named in the checks below.

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked --offline -- -D warnings`, `cargo test --locked --offline` | PASS: no warnings, every test passed |
| Movies load from the install: `HudCommand` 9 of 9 textures, `HudTarget` 27 of 27, 4 fonts, no frame script statement unread | PASS by `kh3_hud` in the logs |
| Test KH3 mode, unattended: Attack, Magic, Items in KH3's art; Magic and Items lists with cursor, x2 count; Magic dimmed in MP Charge | PASS by captures of `hud6` |
| The same: the scope, the lock counter to 24, MAX, the SHOTLOCK panel with its Ragnarok row | PASS by captures of `hud9` |
| Loopback duel: the movies load on both sides, and the joiner's deck commands still run on the host's Sora | PASS by `hudduo1`, before the drawing moved to threads |
| Real pad and keys on the new display; two PCs | NOT RUN |

## Uncertainty

- The HP, MP and focus gauges are still KH2's. Its long level 99 HP bar crosses the bottom of a long list.
- The shotlock's full-screen filter is composited over the scene with straight alpha, as the reference does, so a Flash multiply layer shows as a plain tint. It may read stronger than in KH3. It has not been compared against the game.
- HUD sounds the movies ask for (`UiSound`, KH3's static SE) are dropped. The deck's KH2 menu sounds and the KH3 Sora's own sounds still play.
- `^KEY` text fields keep their authored placeholder, since the string tables are not read. No such field has shown up in the deck or the scope so far. Spell and item names come from `kh3_sora`'s English table.
- Pad-button icons in text are not drawn, so the SHOTLOCK panel's rows have no button marks and the shortcut menu has no icons.
- World skins (`HudCommand<XX>`) are not used.
- `simple_hud` is on, and no situation commands are fed to the movie: Grand Magic's offer is still the old text line.
- The movies add about 8 s or less to load, within the KH3 load thread; not measured alone.

## Verdict

PASS for KH3's own deck and shotlock display in Test KH3 mode, within the checks above. Not a claim of equivalence with KH3.

## Next

Burba tries it on a real pad. Then, in the owner's order: situation commands on the deck, pad icons, KH3's HP and MP gauges (`HudStatus`).
