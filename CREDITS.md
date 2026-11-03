# Credits

Kingdom Hearts: Connected (KHC) is an unofficial fan project. Kingdom Hearts is a trademark of Square Enix and Disney. This repository contains no game files, textures, models, sounds or saves. The program reads them from the user's own copies of the games.

The project's own code and documents are Apache-2.0 (see [LICENSE](LICENSE)). The legal notices are in [NOTICE](NOTICE).

## Projects we build on

| Project | Licence | How we use it |
|---|---|---|
| [OpenKH](https://git.openkh.dev/) (Open Kingdom Hearts Research and Development) | Apache-2.0 | Adapted into our package, model, message, motion and font readers. Pinned at commit `7a3b945c538d32c6a285128c98aefba093f52ceb`. Changes are listed in NOTICE. |
| [kh2-combat-kit](https://github.com/steferro78-HIV/kh2-combat-kit) by steferro78 | MIT | Reviewed for the combat rules. We used it only as a cross-check for the variable-height jump and the Square-button moves (T105). No code or data was copied. |
| .NET runtime | MIT | Math ordering for the bind and coordinate profiles (NOTICE; licence text in `docs/release/dotnet-runtime-LICENSE.txt`). |
| bevy_mikktspace | Zlib, and MIT or Apache-2.0 | Tangent generation for native mesh inspection (licence texts in `docs/release/`). |
| zlib-rs | Zlib | Pulled in through the image codec feature (licence text in `docs/release/zlib-rs-LICENSE.txt`). |
| DirectXTex (Microsoft) | MIT | Used only as a private reference for checking pixel formats. Nothing from it is in this repository or the distributed program. |
| MapStudio, EgsTools, msetDoc (kenjiuno) | Public documentation and rules | Used as references for file layouts and naming rules. No code copied. |
| [kingdom-hearts-x-elden-ring](https://github.com/StrawHatPro/kingdom-hearts-x-elden-ring) by StrawHatPro | MIT (notice below) | The KH3 mode: file formats (`source/kh3_data`), the behaviour of KH3's Sora (`source/kh3_player`), the Cascade effect evaluator (`source/kh3_cascade`, from `crates/kh-vfx`), the command deck and shotlock display player (`source/kh3_ui`, from `crates/kh-ui` and `tools/lib`), KH3's Sora (`source/kh3_sora`, from `crates/kh-core`) and the auto SE detector (from `crates/kh-audio`). Its generated tables and anything embedding KH3 data were not copied. |

### kingdom-hearts-x-elden-ring licence

```text
MIT License

Copyright (c) 2026 StrawHatPro

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Rust libraries

The program is built on many Rust crates, including Bevy, wgpu, renet, aes, sha1, glam and serde. Each one keeps its own licence. `Cargo.lock` has the versions.
