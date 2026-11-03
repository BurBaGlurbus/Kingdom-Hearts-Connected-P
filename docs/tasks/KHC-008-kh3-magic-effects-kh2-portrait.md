# KHC-008: KH3's magic effects show; KH2's Sora in the gauge's ring

| | |
|---|---|
| Gate | None; the owner's request of 2026-11-01: "the health bar sora icon should be using the kh2 one, make sure all magic effects actually show up in game because they are all invisible" |
| Owner | burba |
| Prerequisites | KHC-007 |
| Source | The player's Steam KH3 install, only read, at run time. Nothing from it is in this repository |

## What was wrong

`p_ex001_MagicSet` writes each spell's command as `TRES_CMD_KIND_FIRE`, and the book's table looked up `FIRE`. All 38 spells were skipped, so no spell's blueprint, body effect or cast effect was read. The core still cast them, with their sounds and damage, and nothing to draw.

## What was done

- The book finds the command through `ETresCommandKind::from_game_name`. It now holds 38 spells and their stages, and the loaded effects went from 197 systems to 267.
- `kh3_data::material` also reads the textures a root samples with no parameter, in the root and in the material functions it calls. Those materials were dropped before. Cooking removes the graph, so which texture feeds the colour is a choice: the material's own picture first, shared noise and gradients last, normal maps and masks never.
- Shared masks show as grey, because their channels are data and not colour. The particle's colour goes grey with them, and a mesh of them is drawn at 35%.
- A material with no texture draws on a mesh in plain colour. A root marked as a rim shader (`f` in its kind letters, or "fres") shows by a view-angle term worked out per vertex from the mesh's faces.
- A material that only bends the scene is not drawn, since this renderer has no refraction for effects.
- Aero's wind wall (`jtx_mzl_*_clmp`) is not drawn: its shader scrolls and cuts it to streaks, and whole it is a solid wall.
- Thin gradients with no 4 x 4 mip are decoded instead of dropped. `constraint_tex` counts as an opacity mask, which narrows Thunder's meshes.
- Effects from a one-shot request stop looping after 4 s. Fire's scorch and Aero's pad otherwise loop forever: they stayed on screen for the rest of the run.
- The ring's portrait is KH2's Sora. `Gauge::draw_face` draws the gauge's own face sprite, with its normal, low HP and KO states, in the room's HUD image. `HudStatus` is drawn under that image, and the KH3 face movie is no longer loaded.
- `KHC_SPELL_TOUR` with `--kh3-deck-test` casts all 34 spells in turn, 300 ticks apart from tick 600.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `tour2` (the book's refusals), `tour7` (materials by spell), `fxduo` (loopback), `tour11` to `tour14` (contact pages of every spell, `tour_pages.py`).

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked --offline -- -D warnings`, `cargo test --locked --offline` | PASS: no warnings, 1369 tests passed |
| Tour: all 34 commands requested; effects on screen by contact pages | PASS for what the frames show, read from stills by Claude. The test's own camp menu covers Firaza's row and most of Fira's and Firaga's (commands 110 to 112), and those three are not checked |
| Portrait: KH2's Sora in the ring | PASS by captures of `tour11` |
| Loopback duel: both sides load and exit clean with the new effects | PASS by `fxduo`; no spell was cast in it, so a joiner's view of the spells is not checked |
| The look of each spell compared with KH3; a person playing; two PCs | NOT RUN |

## Uncertainty

- The materials' shader math does not survive cooking, so these are approximations and not KH3's look. Aero's wind walls, Thunder's strike meshes and Water's dome are the furthest off.
- Aerora and Aeroga still show a bright green wall for about two seconds. Hiding the `jtx_mzl_*_clmp` meshes cleared it from Aero only, and the material behind the others was not found.
- Close to the camera, big shells fill the screen. KH3's camera may keep clear of them in ways this one does not.
- The 4 s end is a choice. What ends those two effects in the game is not traced.
- The face's size and place in the ring are set by eye.
- Grand Magic in the tour was cast directly, not through its offer.

## Verdict

PASS for spell effects being visible and for the portrait. Not a claim that they match KH3.

## Next

Burba plays it and names the spells that look wrong; those materials can then get a recipe of their own.
