# KHC-004: KH3's command deck and magic list

| | |
|---|---|
| Gate | None of the split plan's phases; the owner's request of 2026-10-31: "start working on recreating the command deck and magic system for kh3 mode only have attack magic and items" |
| Owner | burba |
| Prerequisites | Earlier KHC work |
| Source | The player's Steam KH2FM and KH3 installs, only read. The rules are `kh3_sora::command`'s, in this repository |

## What was done

KH3's Sora already held KH3's command system and spell rules (`kh3_sora::command`, `magic`, the pouch, MP Charge), but the KH3 ruleset's menu was KH2's: four rows, Magnet's slot casting Aero, Reflect's casting Water, the room's item path, and a Drive row.

- `source/khc/src/room/kh3_deck.rs` is the deck, on in the KH3 ruleset once KH3's data is read. It takes the pad's menu presses before the room's tick, and writes the rows onto the engine's `CommandMenu` before the HUD's tick, so no engine change was needed and the room's own Magic, Items, Drive and shortcut paths never run.
- Main list: Attack, Magic, Items. The second deck was already off in a duel.
- Magic: one row per element in the order they are learned (Fire, Water, Cure, Blizzard, Thunder, Aero), at the tier KH3's Sora knows (the third), KH3's names. A row short of MP is recoloured and still castable unless MP Safety is on; everything is greyed during MP Charge; Cure takes what is left and is never short. Costs are read from the same rows KH3's Sora uses.
- Items: the pouch's distinct items with their counts. The build's items (the room's ids) fill the pouch between fights; in a fight the room's list mirrors the pouch, which is how a joiner sees his own.
- A chosen command goes to KH3's Sora through `CommandRequest::Execute`, as KH3's own `ETresCommandKind`. A joiner's goes to the host in the existing `rival::Command::Cast` and `Item`, numbers from 29 and 58 up being KH3 commands and those below the scripts' types and slots.
- MP Charge now reaches the room from KH3's Sora (`player.mp`), so the HUD's gauge shows it, and the joiner through a new `Shown::charge` byte. This changes the wire format; both sides must be the same build, which the build identity already requires.
- A round start refills KH3's Sora's HP and MP (`core_round_start`).
- Shortcuts: L1 with a face button runs that slot of KH3's pages (Firaga, Blizzaga, Thundaga, Curaga; Waterga, Aeroga, Potion, Ether; empty), L1 with up or down turns the page.
- `--kh3-deck-test`: Sora stands while a script walks the deck, for unattended runs.

An empty row sits under the last row of the main list and of each sub-list: the engine lays the rows up from a place that a level 99 HP gauge covers, which hid Items.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `deck04`, `deck05`, `deck06` (Test KH3 mode: `log.jsonl`, `cap`, `sheet.png`), `deckduo6` (two instances over loopback: `host.jsonl`, `join.jsonl`).

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo test` | PASS: 227 passed, one ignored (it needs a KH3 install), 13 of them new |
| Test KH3 mode, unattended, release: the three rows on screen, Items not hidden; Magic's six rows in order at the third tier | PASS by the log and captures of `deck06` |
| The same: Waterga, Thundaga, Curaga and a Potion each reach KH3's Sora's state (`Firemagic`, `UseItem`); MP falls 169, 147, 119; Curaga takes it to 0 and starts MP Charge; Magic refused and greyed during it; the Potion's count falls; Cancel closes a list | PASS by the log of `deck06` |
| Loopback duel, release: the joiner's Waterga, Thundaga, Potion and Curaga run on the host's slot 1 Sora; the joiner sees its MP fall and the charge | PASS by the logs of `deckduo6`. Run before the script's last addition and a one-word clippy fix, neither of which touches the path |
| Single player | No engine file changed. `kh3_deck` returns at once unless the KH3 ruleset is on. Not run separately |
| Real pad and keys on the deck | NOT RUN: every press here came from the script, which replaces the room's queued presses at the same place in the code |
| Shortcuts in use | NOT RUN |
| Two PCs | NOT RUN |

## Uncertainty

- The shortcut menu is not on screen: the engine here draws none, so the slots run but cannot be seen.
- The deck's look is the engine's KH2 art. No KH3 HUD art is drawn.
- "Short of MP" uses a spell's listed cost. KH3's Sora also lowers it by the magic combo's abilities (`effective_mp_cost`), which only it knows, so during a combo a row can read short and still cast at full colour in KH3's own rule. It never blocks a cast it should not, except with MP Safety on, where the row is greyed a little early.
- Every Sora knows all six spells at the third tier (`magic.known` in `kh3_player::core`). The lobby's bans and the room's spell levels do not reach KH3's magic.
- `book.spells` did not give a cost for the third-tier commands in the run, and the reason was not looked into. The deck reads `kh3_sora::magic::spells::unit` instead.
- A build's items map to KH3's by name. Refocusers and Panacea are not offered, as the build's list has no such items.
- Cancel keeps the main cursor, as `ui_tick` does. Its comment says KH3 itself returned to Attack.

## Verdict

PASS for the deck and the magic list in Test KH3 mode and a loopback duel, within the checks above. Not a claim of equivalence with KH3's own menu.

## Next

Burba tries it on a real pad.
