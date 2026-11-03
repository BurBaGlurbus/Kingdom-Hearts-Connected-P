# KHC-009: KH3's effects drawn with KH3's own shaders; a bigger portrait

| | |
|---|---|
| Gate | None; the owner's request of 2026-11-01: "the magics effects are not 1:1 please fix them as they are visually very broken, and soras hud portrait is really small" |
| Owner | burba |
| Prerequisites | KHC-008 |
| Source | The player's Steam KH3 install, only read, at run time. Nothing from it is in this repository |

## What was done

A cooked material has no math left, so KHC-008's textures and guesses could not look right. The full truth is in the package: each material's compiled D3D11 shaders.

- `kh3_data::shader_map` reads `FMaterialShaderMap`: the uniform expressions and the bytecode of the base-pass and depth-only pixel shaders, per vertex factory.
- `kh3_shader` is new. `dxbc.rs` reads the bytecode, `translate.rs` turns it instruction by instruction into WGSL, and `harness.wgsl` gives the shader its inputs by D3D semantic and constant-buffer slot. Ported from StrawHatPro's `tools/sora_vfx_shaders.js`, `tools/lib/dxbc_disasm.js` and `er_vfx.wgsl` (MIT). Each result is validated with naga before it can reach a pipeline.
- `kh3_shaded.rs` finds a material's owner of the shader map up its parent chain, evaluates the constant buffer with the material's parameters (rows that follow the clock are updated each frame), resolves its textures by slot through the engine's texture list, and binds textures with sRGB as the shaders expect.
- Translated materials draw through `ShadedMaterial`, one pipeline per shader. The old path stays for what could not be translated.
- Blend as the game's: translucent `SrcAlpha, InvSrcAlpha`, additive `One, InvSrcAlpha`, modulate `Dst, InvSrcAlpha`, and the emitter's own override.
- HDR: each fragment passes the fitted ACES curve and an sRGB encode, because the room's frame blends stored sRGB values.
- Soft particles read the distance to a flat floor at Sora's feet, since the room has no depth to read. This removes the hard lines where effects meet the floor.
- The portrait is 1.5 times bigger, drawn over the ring.
- `KHC_SPELL_TOUR=<row>` with `--kh3-deck-test` casts from that row of the 34 spells. `build_render` on the room extension adds plugins that need the renderer.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `sh2a`, `sh2b` (contact pages of the spells).

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked --offline -- -D warnings`, `cargo test --locked --offline` | PASS |
| At load: 397 shaders translated, 479 materials on them; refused: 2 invalid masks, 2 blur loops, 3 others | PASS by the log of `sh2a` |
| Tour pages for Fire, Blizzard, Thunder, Water tiers, Aero, Cure and Grand Magic | Read from stills by Claude. The test's camp menu covers rows at about ticks 5700 to 6300 in each run (Water tiers, Grand Thundaga to Watera among them) |
| Compared with KH3 itself, a person playing, two PCs | NOT RUN |

## Uncertainty

- Mesh particles are baked on the CPU, so a material's own vertex shader (world position offset) does not run, and mesh tangents are made up. Shapes that bend in their vertex shader will not bend.
- The distortion pass is not drawn: refraction has no scene copy here.
- Soft particles fade against a flat floor at Sora's feet, not the real scene: walls and characters give no fade.
- ACES per fragment is not KH3's tone map over the whole frame, and there is no bloom or eye adaptation on these effects. Very bright overlaps will differ.
- Materials not on a translated shader keep KHC-008's generic look.
- One-sided meshes draw both sides.
- Decals (`list.decals`, scorch marks and ground circles) are not drawn.

## Verdict

PASS for effects running on KH3's own shaders where translated. Closer, but not a claim of a 1:1 match until compared with KH3.

## Next

Burba plays it. Then the vertex shaders, decals and the distortion pass, in the order burba's eye asks.
