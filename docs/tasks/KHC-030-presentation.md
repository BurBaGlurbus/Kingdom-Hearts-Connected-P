# KHC-030: Presentation and first-match hitches

| | |
|---|---|
| Gate | Brief 5, WP-19: a victory motion, K.O. slow motion, no hitch on the first spell, and a README that matches the build |
| Owner | burba |
| Prerequisites | KHC-024 (WP-13, the comparator), T249 |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. One Windows PC, loopback |

## What was done

1. **Victory motion.** KH2 has no victory pose: a private trace of every motion set Sora carries found none (`victory-pose-01`, from T244). The candidates on the install, now a table `duel_match_room::VICTORY_MOTIONS` chosen by `--duel-victory`:
   - `stance`: motion 6, field variant (`P_EX100.mset` slot 27, 70 ticks, holds its end), the battle-to-field stance switch, the only motion the game plays because a fight is over. Sora's kit now loads it as a scripted motion (`engine::test_room::STANCE_MOTION`) with the Keyblade's own slot.
   - `summon`: 0x82, the summon's appear motion (60 ticks), already loaded; blended over 8 ticks.
   - `none`: the default until the owner picks; the result frames the winner as he stands.
   At the result the host plays it on the winner's KH2 Sora with the Keyblade kept shown (the original puts the blade away after motion 6: a duel choice); the joiner sees it through the snapshots. KH3 seats play nothing.
2. **K.O. slow motion.** For 60 host ticks after a knock-out (RoundOver), the field steps at a third of its rate through the game's own game speed (`Game::game_speed`, DAT_140717424, the boss overlays' slow motion): the tick, the match clock, the network and the round timing stay at 60 Hz; motions, effects and physics step less often. Both screens key it to the host's clock. [Duel choice] One third, for about a second (the brief's default).
3. **Pre-warm.** During the countdown the spell effect files the Soras on KH2's moves can cast are readied one DPD a frame (`SpellFx::warm`): each model is parsed and each texture's page made in both address modes, so its upload goes before the fight. Every duel build has every spell, so both loadouts can use all of them. Not warmed: materials (their pipelines are still made on first draw), item and Drive effects, KH3's effects.
4. **README.** The stale trail limitation is gone, `Free pick` (`each`) is documented with the lobby's new pages, and the victory and slow-motion lines describe the build.

**Owner's questions:** which motion the winner plays was asked in the thread on 2026-11-02; the default stays `none` until burba answers. Slow motion length: about a second (default).

## Evidence

Private, outside the repo: `%USERPROFILE%\kh2-kh3-ref\runs\brief5\lobby\duo-h2j3-1\` (host and joiner logs) and `runs\brief5\perf-host`, `perf-join` (`frames.csv`, `--perf`), binary `runs\brief3\bin\b5perf` (release with `perf-diag`). Host: `--duel-mode stock --duel-damage 10 --duel-victory stance`, KH2 against a KH3 joiner.

## Checks

| Check | Group | Result |
|---|---|---|
| The field slows for a second after a knock-out, not for time-up (`a_knock_out_slows_the_field_for_a_second`) | Agent | PASS |
| Slow motion starts at the same host tick on both screens (`duel_ko_slow`: host 3301, the joiner's playhead 3299) | Local | PASS |
| The stance motion plays on the host's winner (`duel_victory_motion`, `played: true`) | Local | PASS |
| No frame over 33 ms in a match's first 10 seconds (target) | Local | FAIL: 13 logged hitches on the host and 13 on the joiner from the countdown to 10 s into the fight (worst 157 and 122 ms); about 100 frames over 33 ms per instance in the whole run, median 16.7 ms. Two instances and KH3's loading share one PC in this run, so the hitches are not the first spell's alone, and whether the warm-up helped is not measured |
| Frame rate 1% low within 5% of the build before | Local | NOT RUN (no run of the build before) |
| Captures of the victory motion and slow motion on both screens look right | Owner | NOT RUN |
| README changes | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `build.bat` (its script) | Agent | PASS |

## Uncertainty

- Slow motion slows the field's step, not a buffered picture: what happens in that second is simulated at the slower rate. The Soras are frozen in RoundOver, so no gameplay is decided in it.
- The warm-up's effect on the first spell is unmeasured; materials are not warmed, and they may be most of the cost.
- The stance motion was not seen on screen; nothing checked that its end pose and the shown Keyblade look right together.

## Verdict

**PARTIAL.** Victory motion slot, slow motion, warm-up and README are built; slow motion and the motion's start are confirmed in logs. The frame target FAILS in a one-PC loopback run, and the owner's picks and captures are open.

## Next

burba picks the victory motion (then it becomes the default). A frame log on two PCs, or one instance with the scripted rival, separates the first spell's cost from loopback contention; warm the materials if it is still there.
