# KHC-010: Each player picks a ruleset

| | |
|---|---|
| Gate | None; the owner's request of 2026-11-01: the host can let each player pick a ruleset when every player has the copy it needs, so that a KH2 player and a KH3 player can fight each other |
| Owner | burba |
| Prerequisites | KHC-004 to KHC-009 |
| Source | The player's Steam KH3 install, only read, at run time. Nothing from it is in this repository |

## What was done

- The host's Ruleset row has a third choice, *Each player*. `MatchRules` holds `Ruleset::Each` and each seat's pick, and `seat_kh3(seat)` answers for one Sora. The picks travel with the rules.
- Under *Each player* the Match page has *Your ruleset* and the other player's pick. A joiner's pick goes to the host as `Intent::Pick`. A change un-readies both players, and a player's pick is kept in `duel-prefs.json`.
- KH3 and *Each player* are only on offer when both players' KH3 installs passed `kh3_data::admit` and their identities match. Otherwise the host's row shows KH2 alone, locked, with the reason in the help line. If a rule asked for KH3 before the joiner answered, the host falls back to KH2 (`duel_kh3_fallback`).
- Both PCs need KH3 even when one player picks KH2. The host simulates both Soras, so it needs KH3's rules, and each PC draws the other Sora, so it needs KH3's animation and effects.
- Simulation: each Sora runs on his own seat's ruleset, on the host (`sora_step`, `rival_play::tick`). A KH2 Sora is KH2's controller, a KH3 Sora is `kh3_sora`.
- Each player has their own presentation: KH3's deck, HUD, camera and ability pages follow this player's seat (`duel_kh3::mine`), and each Sora is drawn by his own ruleset's presenter (`kh3_model::shows_seat`).
- A snapshot carries the KH3 part only for a KH3 seat.
- KH3 hit on a KH2 Sora: KH3 finds the contact, using `core::kh2_body`, which gives KH3 his gauge's share at a level 99 defense. `duel_hits::kh3_hit_on_kh2` then applies KH2's rules: a Reflect barrier or a Guard facing the attacker takes it, invulnerable frames miss it, then damage and a flinch or blow.
- KH2 hit on a KH3 Sora: KH2 finds the contact with its own windows and formula, and `core_take_kh2_hit` hands it to KH3's own victim side (guard, dodge, reaction).
- [Duel choice] A cross-ruleset hit takes the same share of the victim's gauge as of a gauge on the attacker's own ruleset. KH2 hits use the host's multipliers and cap. KH3 hits count the multipliers from their defaults, x3 and x1.5, the numbers KH3's own damage stands beside, and stop at the cap. This is a balance choice with no game reference.
- `duel_kh3::settle_seats` runs as a match starts. A Sora going back to KH2 drops KH3's Sora and has KH2's max HP and MP back. Before this, moving from KH3 to KH2 between matches kept KH3's gauges.
- `--duel-ruleset each`, `--duel-pick kh2|kh3` and `--duel-pick-flip` are test flags.
- Single player: `source/engine` was not changed, and every new check is behind `game.versus`.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`, with `scripts\duo_mixed.ps1` and `scripts\mixed_read.py`:

- `mixed-a`: KH3 host, KH2 joiner.
- `mixed-b`: KH2 host (bench), KH3 joiner.
- `mixed-c`: KH2 host, KH3 joiner, a knock-out, then a later match on KH2.
- `mixed-d`: a joiner without KH3.
- `mixed-e`: the lobby's page.
- `mixed-f`, `mixed-g`: both on KH3, both on KH2.

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | PASS: no warnings, 1383 tests passed, 2 ignored; run on this task's files alone, because another thread's unfinished files did not compile for tests |
| KH3 hits on a KH2 joiner: 22 hits, flinches and blows, gauge falling (`mixed-a`) | PASS on loopback |
| KH2 hits on a KH3 joiner: 31 hits, KH3's HP 265 down to 87 (`mixed-b`) | PASS on loopback |
| KH3 joiner knocks out a KH2 host, round and match end, result (`mixed-c`) | PASS on loopback |
| Next match with the joiner's pick changed: both KH2, KH2's HUD and full gauges (`mixed-c`) | PASS on loopback, by log and captures read by Claude |
| Each side's HUD and drawing of the other Sora (captures of `mixed-a`, `mixed-b`) | PASS by stills read by Claude |
| Joiner without KH3: fallback, reason shown, match on KH2 (`mixed-d`) | PASS on loopback, joiner's folder set to a path that does not exist |
| Lobby shows *Each player*, *Your ruleset*, the other's pick (`mixed-e`) | PASS by capture |
| Both on KH3, both on KH2 still fight (`mixed-f`, `mixed-g`) | PASS on loopback, short runs; not compared hit for hit with an earlier build |
| A knock-out of a KH3 Sora by a KH2 Sora: `mixed-b` got to 87 HP before the host's last capture | NOT RUN |
| A KH2 Guard or Reflect meeting a KH3 hit; KH3's guard meeting a KH2 hit: no such moment was logged | NOT RUN |
| A person pressing the *Your ruleset* row, a real pad, playing it for feel and balance, two PCs | NOT RUN |

## Uncertainty

The scripts' KH2 Sora does not chase well, so most KH2 hits come from the host's bench. Two Soras of different rulesets can stand overlapped: KH3's Sora pushes off a KH2 body, and KH2's has no push against his. Whether the damage shares feel fair needs a person.

## Verdict

PASS on loopback for the checks above. Not whole-game proof: the NOT RUN rows remain open.
