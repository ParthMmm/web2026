# Photo gallery references

## Primary user reference

[YouTube reference — start at 1:05:28](https://www.youtube.com/watch?v=x8r5otKq75w&t=3928s)

The owner asked to save this as the reference for the intended result. The video could not be inspected through the available tools, so no visual or behavioral requirements have been inferred from it. Review this timestamp before turning it into implementation requirements.

## Desktop implementation references

The desktop app is drawn with [Rune](https://github.com/anomalyco/rune) and rune-kit, at the revision `apps/desktop/Cargo.toml` pins. Take patterns from Rune's own docs at that revision and from `apps/desktop/AGENTS.md`.

## Historical references

The app was built on GPUI Kit, then ZUI, before it moved to Rune. These links record that history. Do not follow them for design or structure.

- [ZUI](https://github.com/zeronsh/zui), the GPUI fork the app used before Rune
- [Zeron's gpui-component fork](https://github.com/zeronsh/gpui-component)
- [GPUI Kit](https://github.com/longbridge/gpui-kit), used before the move to ZUI
- [Zed GPUI image example](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/image/image.rs)
- [GPUI FPS monitor](https://gpui-kit.com/docs/fps/)
- [Zeron](https://github.com/zeronsh/zeron) and [the Zeron reference](zeron-reference.md), which pins revision `7a472fce41b6a834da87b460e189adb05eacf3d4`
