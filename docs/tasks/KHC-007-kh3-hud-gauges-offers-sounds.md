# KHC-007: KH3's gauges, commands on offer, button icons and HUD sounds

| | |
|---|---|
| Gate | None; the owner's request of 2026-11-01: "fix whats missing" |
| Owner | burba |
| Prerequisites | KHC-006 |
| Source | The player's Steam KH3 install, only read, at run time. Nothing from it is in this repository |

## What was done

- `HudStatus` and Sora's face movie `FacePcExSora01` load from the paks and draw the HP, MP and focus gauges at the bottom right, on a third drawing thread. `hud_status.rs` is ported from StrawHatPro's kh-ui (MIT). KH2's gauge and its low-HP alarm stand down through `Game::hide_gauge`.
- The gauges read the room's HP and MP, as both sides see them, and KH3's Sora for MP Charge and the focus maximum. A joiner has no core, so it reads the host's numbers.
- Commands on offer are on the deck, in `HudCommand`'s own slots with their timers. The host reads the core's whole list. A joiner shows the one command the wire carries. The old text lines give way when the movies are up.
- `UI/IconText` and the pad set's table load from the paks, so `<icon(...)>` draws in text. The shotlock panel's rows have their buttons. The set is `xboxone`, the player's default in `pad_glyphs`.
- The movies' sound requests play KH3's own `se00000_NNN`, flat. KH2's menu sounds only stand in when the movies are missing. A loop the HUD asks for (the low-HP alarm) loops whole if the clip has no loop points of its own.
- A multiply layer over an empty part of the canvas now darkens what is behind it. Before, it painted its own colour over the scene, which was the shotlock's haze.
- Test KH3 mode's two text lines moved up, off the focus gauge. `--kh3-deck-test` now fills the style gauge, runs the offer with Triangle, then drops HP low and restores it.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `fix1` (gauges and shotlock), `fix4` (offer, timer, MP Charge bar), `fix6` (Triangle, low HP), `fixduo` (loopback duel).

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked --offline -- -D warnings`, `cargo test --locked --offline` | PASS: no warnings, 1364 tests passed |
| Movies load: `HudStatus` 5 of 5 textures, the face 3 of 3, 188 of 188 sounds | PASS by the logs of `fix1` |
| Gauges, portrait, focus arc, pink MP Charge bar, red portrait at low HP; the shotlock's panel with icons, the scene no longer tinted | PASS by captures of `fix1`, `fix4`, `fix6` |
| A full style gauge offers Waterza on the deck with its timer; Triangle takes it off the list | PASS by `kh3_situations` in `fix6` and captures of `fix4` |
| The HUD's sounds are requested: cursor, open, cancel, refused; the low-HP loop starts and stops with the HP | PASS by `kh3_hud_sound` in `fix6`; requests only, nobody listened |
| Loopback duel: the joiner's gauges and offer, from the host's numbers | PASS by `fixduo` |
| Real pad and keys on the new display; two PCs; the sounds and the haze compared with KH3 by a person | NOT RUN |

## Uncertainty

- Command names come from `kh3_sora`'s English table, "Second Form" by hand, and the game's enum in words for anything else, since the string tables are not read. Some finish commands will read like "Finish An2".
- The multiply layer darkens in grey over the scene. A tinted layer loses its tint there. The look has not been compared with KH3.
- A joiner sees one command on offer, not the whole list, and with no timer.
- Button icons stay the Xbox set whatever the pad is. Keyboard players see them too.
- The focus gauge has no red preview of a shotlock's cost, as in the reference.
- The foe's HP bar at the top right is still KH2's.
- The bar lengths use the room's level 99 numbers. Long bars are cut by the movie's own limits (`hp_max_frames`), as in KH3.

## Verdict

PASS for what was asked, within the checks above. Not a claim of equivalence with KH3.

## Next

Burba tries it on a real pad and listens. Then KH3's foe gauge and the string tables, if wanted.
