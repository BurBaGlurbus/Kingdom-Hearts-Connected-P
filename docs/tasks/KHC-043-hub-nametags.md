# KHC-043: Nametags above heads

| | |
|---|---|
| Gate | Brief 6, WP-22: a readable name, level badge and status mark over every avatar, and bubbles |
| Owner | burba |
| Prerequisites | KHC-042 |
| Source | The player's Steam KH 1.5+2.5 and Birth by Sleep (the Mirage Arena), only read, at run time. One Windows PC. Builds pinned privately as `b6hub`, `b6hub2`, `b6hub3` |
| Wire | none |

## What was done

- `room/hub_tags.rs`: [hub choice, KHC-043] the point 190 units above an avatar's feet goes through the room's own 3D camera to the 2D layer (416 high, as wide as the window's shape, centred on x 256), so tags spread on a wide window. Behind the camera, nearer than 30 units, or off the window's sides: no tag.
- A dark badge edged in a tint holding the level, the name with a shadow, and a status mark by shape: a filled disc available or looking, a ring in a match or a challenge, a dash away or do-not-disturb.
- Sized by distance between 0.55 and 1.3, faded from 1800 to 3200 units. At most 16, but friends and challengers always. Tags closer than 24 px are pushed apart, the nearest in front and not moved. Muted players have no tag.
- Names drawn with the name rules' characters only; the rest become `?`. The `#number` shows only when two players nearby share a name.
- A bubble above the tag for 3 seconds for an emote or phrase; up to three toasts at the top.
- Settings in `profile.json` (`hub`): tags All, Nearby, Friends or Off; size; own tag; opacity; bubbles; high contrast. There is no settings page for them yet.

## Results

`bots-32` shows a tag over each avatar in view; `bots-4` and a run with the point at the feet confirmed the projection lands on the feet. Tags whose body the arena's fog had taken still showed with the first fade (2500 to 4000), and the next was too short (1400 to 2200) in `net-8`; the fade now runs 1800 to 3200, which was not captured again.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief6\`: `empty-32` (the hub alone), `bots-32` (`--hub-bots 32`), `bots-4` (a projection check), `net-8` (`khc-hub` on loopback, `khc-hub --bots 8`, one game client). Runs: `scripts\brief6_runs.ps1 -Kind empty|bots|net -Bots N`; build: `scripts\brief6_build.sh NAME`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Projection with a fixed camera: in front, behind, at the edge, at the near plane, a wider window | Agent | PASS |
| Nudged tags never pile up; counts, settings, twins, mutes, fading | Agent | PASS |
| Captures with 32 bots at three distances, readable, no pile-ups | Local | PARTIAL: one distance captured; no pile-ups seen |
| Tags cost 0.5 ms a frame or less | Local | NOT RUN: the overlay does not split the tags' cost out |
| Readable from TV distance with a controller | Owner | NOT RUN |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Uncertainty

The fade distances follow the Mirage Arena's fog by eye. Own tag on or off is kept but not drawn (the player's own Sora has no tag).

## Verdict

PARTIAL. Tags work and are tested; their cost and the owner's reading are not measured.
