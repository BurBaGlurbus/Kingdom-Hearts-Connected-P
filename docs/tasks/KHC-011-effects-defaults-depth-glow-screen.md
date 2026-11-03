# KHC-011: the effects' class defaults, scene depth, ground marks, bending shapes, refraction, glow, screen passes, cast voices

| | |
|---|---|
| Gate | None; the owner's request of 2026-11-01: "stuff like fire or blizzard doesnt travel or hit the enemy, and attacking effects are not 1:1 please also fix all of the stuff missing for a true 1:1" |
| Owner | burba |
| Prerequisites | KHC-009 |
| Source | The player's Steam KH3 install, only read, at run time |

## What was done

- **Class defaults, the largest fault.** A cooked object leaves out what equals its class default, and those defaults are in the executable's constructors. The reader held only a few. `bCurveParameterDirectScale` and `bCurveParameterDirectPosition` default to true: without them every node's scale track was read as a rate, and scales multiplied down the emitter chain, so Cure's flower reached about 80 times its size and timing, shapes and directions were off across the effects. `kh3_data/src/effect_defaults.rs` now holds the non-zero defaults of the 240 particle classes, for the properties the reader uses.
- **Spells travel.** Every clip has its socket track, so a cast leaves from the Keyblade's tip (committed before this record, aed7d28).
- **Scene depth.** The scene camera has a depth prepass once KH3 is loaded, and the room's materials draw into it, so soft edges meet the walls and characters as well as the floor.
- **Ground marks.** `FDeferredDecalPS` is translated; the box is drawn and the shader finds the scene point. Characters take none: a point more than 12 cm above the floor keeps its pixels.
- **Bending shapes.** A mesh particle's own vertex shader is translated (`harness_vertex.wgsl`) with a vertex shader per variant.
- **Refraction.** The material's distortion pass draws in the transmissive phase and reads the scene at the offset it asks for.
- **Attack trails.** Ribbons are smoothed with a Catmull-Rom curve between their samples, as the host's are.
- **Glow.** A second camera draws only the effects, in HDR, only their light past white, through Bevy's bloom at about UE4's total share (`kh3_post.rs`). That image is added over the scene.
- **Screen passes.** `RadialBlur`, `BloomRadialBlur` and `Filter` ADD and ALPHA, which are every pass KHC's effects ask for apart from character drives. Each is more images over the scene: a blur is the scene again at each of its ten taps, in cells carrying its ring's weight.
- **Cast voices.** `p_ex001_VoiceGroupSet` names `sora_magic:<no>`'s lines; 53 more sounds are read.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `d1`, `d2` (spell pages with class defaults), `d4` (attack combo), `g1`, `g2` (glow, filters, Thunder family), `g4` (Grand Magic), `g3` (voices reaching the player), `dump\systems.txt` (what each effect was read as). The class defaults came from `khxer`'s `tools/reflection.js` and `tools/sora_vfx_pack.js --defaults`; `scripts\gen_effect_defaults.py` writes the source file from them.

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy -p khc -p kh3_shader -p kh3_data --all-targets --locked -- -D warnings`, this commit alone | PASS. The shared folder's tree also has another piece of work in progress, whose three lints (`crash.rs`, `launcher.rs`, `duel_replay.rs`) are not in this commit |
| `cargo test --locked` | PASS, 1,410 tests, run on the shared folder with that other work in it; not run on this commit alone |
| Two instances over loopback, host casting from the tour | PASS for no crash and both loading the 261 sounds (run `g5`); the host's spells were not on the joiner's screen in the frames taken, so a joiner seeing the new passes is NOT RUN |
| Spell tour: Fire, Fira, Firaga, Blizzard tiers, Thunder tiers, Water tiers, Aero tiers, Cure tiers | PASS by eye against the data: they travel, hit the dummy, and sizes are sensible |
| Grand Magic rows 110 to 117 | Grand Fire and Grand Blizzard kinds PASS by eye (run `g4`); the later rows of that run show little and were not looked into; rows 118 to 125 NOT RUN |
| Hearing the cast voices | NOT RUN: the log shows the line reaching the player, nobody listened |
| Against the real game side by side | NOT RUN: no footage comparison, so this is not a claim of 1:1 |

## Uncertainty

- Water's wet ground decal (`kdc_wat_zmn_00a`) has a white base colour and an emissive of 0.1. The game lights it as a wet surface; here only the emissive shows, so the floor turns a flat grey for a moment.
- Thundara's flash in the sky reads as saturated cyan, with a hard edge. The data asks for it, but I could not compare it with the game.
- `SQEX_Material_*` character drives (the cast's glow on the body, 23 effects) are not applied: Sora is KH2's model and has no such material.
- A glow from an effect behind a character still shows over him: the glow camera has no scene depth.
- The 12 cm rule for ground marks is a stand-in for the game's stencil class, and treats sloped floors as not floor.
- `GLOW_INTENSITY` comes from UE4's tints, but Bevy's bloom is not UE4's chain.
- `effect_defaults.rs` holds names and numbers read from the executable's constructors. They are facts the reader needs, as the crate's other constants are, but they are in Git, and they reached `origin` in b886443.

## Verdict

PASS for the spells and attacks driven in the tours. 1:1 against the game is not shown.

## Next

Compare captures with footage of each spell; decide whether the wet decal should be lit; the rest of the Grand Magic rows.
