# KHC-020: New rulesets: KH1, then Birth by Sleep, then Chain of Memories

| | |
|---|---|
| Gate | Brief 4, WP-11: a new ruleset meets its checklist (data reader, controller layer, seam variant, wire block, lobby row, parity row, evidence) |
| Owner | burba |
| Prerequisites | KHC-014 (the seam), KHC-015 (the bench matrix), and Brief 5's WP-07 (the ruleset card table), which is not started |
| Source | The player's Steam KH 1.5+2.5 install, only listed (file names), not read |

**Update 2026-11-02:** Brief 5's WP-07 is done ([KHC-016](KHC-016-lobby-v2.md)): a new ruleset adds a row to `duel_rules::RULESETS` and `duel_rules::CARDS`. KH1 still has no reader, so this stays BLOCKED.

This record covers the package for all three rulesets until one of them starts. KHC-021 (BBS) and KHC-022 (Chain of Memories) take their own records when their work begins.

## What was done

Nothing that adds a ruleset. What a ruleset will plug into is ready:

- `source/khc/src/room/ruleset.rs` (KHC-014): `SeatRule` is the one place a ruleset's outgoing hit, incoming hit, defensive state and attack damage differ. A new ruleset is a new variant and its arms.
- `--bench-matrix` (KHC-015) measures any pairing a one-process duel can play, so a new ruleset gets its parity row by adding its pick.
- `duel_rules::PARITY` is a table by ruleset name, so a new ruleset is a new row.

Listing the install (names only) shows `Image\kh1_first..fifth.hed/pkg`, `Recom.hed/pkg` and `bbs_first..fourth.hed/pkg` beside KH2's. `bbs_data` already reads BBS's packages for the Mirage Arena (maps only).

## Why it stops here

- **Brief 5's WP-07 is not started.** Step 5 of the checklist (a lobby row on the ruleset card) has nothing to add a row to. The brief makes WP-11 depend on it.
- **Scope.** The KH3 ruleset took T279, T282, T300 and KHC-004 to KHC-009, and parts are still PARTIAL. A KH1 ruleset needs, before any rule: a reader for KH1's battle tables and Sora's motions in `kh1_*.pkg` (KH1's formats are not read anywhere in this repository), then a controller layer. No reference evidence for KH1's rules has been gathered, and the repo rules forbid inventing them.
- **Chain of Memories needs the owner's decision** (below) before any code.

## Plan when unblocked (KH1 first)

1. `source/kh1_data`: validate the KH1 packages like `kh3_data::admit`, then read Sora's battle numbers and attack rows. Synthetic bytes in tests.
2. `source/kh1_player`: a layer over the `player` controller, as `kh3_player` is, driven by those numbers.
3. `SeatRule::Kh1` with its arms in `ruleset.rs`; `MatchRules` picks become a ruleset per seat instead of a KH3 bit (a wire change).
4. An optional `kh1` block in the snapshot inside Brief 3's byte budget; one wire change.
5. The ruleset card row (WP-07) and an install check that generalizes `needs_kh3`.
6. Its rows in `--bench-matrix`, inside the target spread.
7. Event logs against KH2 and KH3, a human playtest, a README section, and a record of what is read and what is approximated.

## Checks

| Check | Group | Verdict |
|---|---|---|
| KH1 ruleset checklist | Agent, Local, Owner | BLOCKED: WP-07 not started; no KH1 reader |
| BBS ruleset checklist (KHC-021) | Agent, Local, Owner | BLOCKED: after KH1, and WP-07 |
| Chain of Memories (KHC-022) | Owner | BLOCKED: the owner's decision below |

## Questions for the owner

| Question | Default until answered |
|---|---|
| Chain of Memories: keep cards as the attack system, or adapt them into a real-time hotbar? | Defer Chain of Memories |
| Is the order KH1, then BBS, then Chain of Memories right? | Yes |

## Verdict

BLOCKED. Next: Brief 5's WP-07, then step 1 above.
