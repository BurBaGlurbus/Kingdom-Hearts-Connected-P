# KHC-016: Lobby v2: each player's ruleset, ruleset card, presets, rules code

| | |
|---|---|
| Gate | Brief 5, WP-07: each player picks their own ruleset, both see a ruleset card, choices are fair, and the host's rules can be saved, loaded and shared as a short code |
| Owner | burba |
| Prerequisites | KHC-013 (WP-04), KHC-015 (WP-06), KHC-029 (WP-18, the capability exchange) |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. One Windows PC, loopback |

## What was done

1. **Each player's ruleset.** The host's *Ruleset* row is now the policy only: *KH2*, *KH3* or *Free pick* (`Ruleset::Each`, renamed from "Each player"; the enum is unchanged). Under Free pick each player picks under *Your ruleset* on their own *Ready* page, with the other player's pick below it; under KH2 or KH3 the row shows the host's choice, locked, and says so. The KH3 ability pages now come after Ready, so a pick that adds them does not move the Ready page under the cursor.
2. **Ruleset card.** A *Rulesets* page after Bans, from `duel_rules::CARDS` beside `RULESETS` (a new ruleset is a row): name, home game, gauges, a few signature moves, and a bench row. The bench row says "Not measured yet": KHC-015's numbers are bot-limited, so none are shown.
3. **Presets.** `duel_rules::PRESETS`, a table like `MODES`: *Standard* (`STANDARD`, today's defaults), *Quick KO*, *Long fight* (Best of 3, x2, 30% cap) and *Timed 90*. Each is a `MatchRules` constant, Authentic. A *Preset* row on the Match page steps them; choosing one sets the mode, damage, magic, cap, time limit and Parity, and keeps the arena, the ruleset and the netcode rows. A changed rule shows *Custom*. [Duel choice] *Quick KO*'s numbers (x5 attacks, x2.5 magic, 50% cap) are mine: the brief names it only.
4. **Rules code.** `duel_wire::encode_code` / `decode_code`: `PROTOCOL`, the rules message, the host's bans, then two bytes of a SHA-256, in base32 (RFC 4648 letters and 2 to 7) in groups of four, about 45 letters. The Match page's *Rules code* row shows its start and the help line all of it. It is accepted by `--duel-code CODE` and by a new *Rules code* field on the launcher's Multiplayer screen (kept in the launcher settings only when it reads, and passed to any host it starts). A code wins over the kept rules; its bans fill the host's first ban rows; each player's own pick stays theirs. Wrong letters, a bad check, another protocol or out-of-range rules are refused with a reason.
5. **Versioned prefs.** `duel-prefs.json` has `"v": 2`. Version 1 (no field) named the ruleset by its lobby name; it is migrated as it is read ("Each player" to `each`, the others to their command-line names). Values the next duel does not allow still keep their defaults.
6. **Hidden bans.** `Lobby::seen_by`: until both players have confirmed, each one's bans are hidden from the other (only that they are done). The host sends the joiner the hidden copy; the host's own screen shows "Chosen, hidden".
7. **Capability exchange.** Already in place from KHC-029: *KH3* and *Free pick* are offered only once both installs report the same KH3 bytes, and the Mirage Arena only when both have the same Birth by Sleep files; otherwise the row is locked and its help line says why. Unchanged here.
8. **A fair coin.** `duel_vote::Coin`: each player sends `COIN_COMMIT` (SHA-256 of "khc coin", the topic and a random 16 bytes), then `COIN_REVEAL` (the bytes) once the other's commitment is in. The seed is a SHA-256 of the host's and the joiner's numbers. A reveal that does not match its commitment is refused (`coin_refused`), and the host drops the vote; the joiner logs `coin_mismatch` if the host's flip seed is not its own reckoning. The host's commitment waits until the vote has gone out, so the joiner has a coin to take it.
9. **Parity row.** *Ruleset scale*: *Authentic* or *Parity* (KHC-015's `--duel-parity`), on the Match page.

**Wire:** two new message kinds (`COIN_COMMIT` 16, `COIN_REVEAL` 17); the rules message is unchanged. **PROTOCOL 13**, one bump for the brief.

**Owner's questions, defaults used:** presets Standard, Quick KO, Long fight, Timed 90 (yes); bans hidden until both confirm (yes).

## Evidence

Private, outside the repo: `%USERPROFILE%\kh2-kh3-ref\runs\brief5\lobby\duo-h2j3-1\` (host and joiner logs, net logs, state hashes), binary pinned at `runs\brief3\bin\b5perf` (release, `perf-diag`).

The first run found a bug: the host's commitment reached the joiner before the vote, so the joiner had no coin to take it, refused the host's number (`coin_refused`, "before its commitment"), and the lobby waited on the vote until the run was killed. Fixed (the commitment waits for the vote); the second run is the one recorded.

## Checks

| Check | Group | Result |
|---|---|---|
| Rules code round trip, any case, a typo's check refused, bad letters, too short (`a_rules_code_round_trips_and_a_bad_check_is_refused`) | Agent | PASS |
| v1 to v2 prefs migration (`a_version_1_file_is_migrated`) | Agent | PASS |
| Coin commit and reveal: same seed on both, a mismatched or early reveal refused (`the_coin_is_both_players_and_a_wrong_reveal_is_refused`) | Agent | PASS |
| Presets valid, Authentic, keep arena and ruleset; a card a ruleset (`presets_are_rows_and_keep_the_arena_and_ruleset`) | Agent | PASS |
| Bans hidden until both confirm (`bans_stay_hidden_until_both_have_confirmed`) | Agent | PASS |
| Scripted loopback lobby: joins, Free pick with the host on KH2 and the joiner on KH3, bans, the test vote's tie goes to the shared coin (host flip 2221949940, both screens land on choice 0, no mismatch), both ready, the match starts and ends on a knock-out | Local | PASS |
| The picks were made by `--duel-pick`, not by pressing the Ready page's row | Local | NOT RUN |
| Screen captures of the Rulesets page, the new rows and a greyed-out option, for overflow | Local | NOT RUN (short-run rule) |
| README lobby section | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `build.bat` (its script) | Agent | PASS |

## Uncertainty

- The new rows' text was not seen on screen. *Rules code*'s value is clipped on purpose; the Rulesets page's values ("KINGDOM HEARTS II FINAL MIX", "Combos, Drive Forms, Reflect") are the longest and may overflow the value column.
- The coin depends on the other player answering. A player who never reveals stalls a tied vote (the vote is the F3 test vote only today).
- The launcher's *Rules code* field was not driven.

## Verdict

**PARTIAL.** All eight steps are built and the Agent checks pass; the scripted lobby with a different ruleset per seat starts and finishes the match. Captures and a pick made through the Ready page's row are NOT RUN.

## Next

Captures of the Match, Rulesets and Ready pages (`--duel-shots`). KHC-020 (WP-11) can add a ruleset as a row of `RULESETS` and `CARDS`.
