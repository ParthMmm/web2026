# Third-party notices

Code bundled with this prototype that carries someone else's licence.

## Zeron

Parts of `src/ui/design/` are ported from [Zeron](https://github.com/zeronsh/zeron)
(`zeronsh/zeron`), revision
[`7a472fce41b6a834da87b460e189adb05eacf3d4`](https://github.com/zeronsh/zeron/tree/7a472fce41b6a834da87b460e189adb05eacf3d4),
which is made available under the MIT License below.

Ported modules and their upstream sources:

| Here | Upstream |
| --- | --- |
| `src/ui/design/color.rs` | `crates/ui/src/theme.rs` (colour math, contrast helpers, ink and hairline scales) |
| `src/ui/design/mod.rs` | `crates/ui/src/theme.rs` (planes, elevation ladder, geometry constants) |
| `src/ui/design/glass.rs` | `crates/ui/src/glass.rs`, `crates/ui/src/frost.rs`, glass section of `crates/ui/src/theme.rs` |
| `src/ui/design/motion.rs` | `crates/ui/src/motion.rs` (curves, catalog, frame clock) |
| `src/ui/design/hover.rs` | `crates/ui/src/motion.rs` (`HoverFades`) |

The reasoning and the upstream measurements are recorded in
`docs/photo-gallery/zeron-reference.md`.

Zeron's own dependencies are not carried with this port. The bundled Geist fonts
(SIL Open Font License 1.1) and Solar Icons (CC BY 4.0) in that repository were
not copied.

```
MIT License

Copyright (c) 2026 Wing

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
