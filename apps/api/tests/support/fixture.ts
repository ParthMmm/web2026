import { Effect } from "effect";
import { Digest } from "effect/crypto";

import { ByteCount, photoIdOf, Sha256 } from "../../src/photos/domain.ts";
import type { Manifest, VariantKind } from "../../src/photos/domain.ts";

export type Bodies = Record<VariantKind, Uint8Array>;

const WEBP_BYTES = 64;
const ORIGINAL_BYTES = 2048;
const VP8X_EXIF = 0x08;
const VP8X_ANIMATION = 0x02;

const ascii = (text: string) => new TextEncoder().encode(text);

export const webp = (
  chunk: "VP8 " | "VP8L" | "VP8X",
  width: number,
  height: number,
  { flags = 0, totalBytes = WEBP_BYTES } = {}
): Uint8Array => {
  const out = new Uint8Array(totalBytes);
  const view = new DataView(out.buffer);
  out.set(ascii("RIFF"), 0);
  view.setUint32(4, totalBytes - 8, true);
  out.set(ascii("WEBP"), 8);
  out.set(ascii(chunk), 12);
  view.setUint32(16, totalBytes - 20, true);
  if (chunk === "VP8 ") {
    out.set([0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a], 20);
    view.setUint16(26, width, true);
    view.setUint16(28, height, true);
  } else if (chunk === "VP8L") {
    out[20] = 0x2f;
    view.setUint32(21, (width - 1) | ((height - 1) << 14), true);
  } else {
    out[20] = flags;
    view.setUint32(24, width - 1, true);
    view.setUint32(27, height - 1, true);
  }
  return out;
};

export const withExif = { flags: VP8X_EXIF };
export const animated = { flags: VP8X_ANIMATION };

export const EDGES = {
  w1200: [1200, 800],
  w2400: [2400, 1600],
  w480: [480, 320],
} as const;

export const factsOf = (bytes: Uint8Array) =>
  Effect.gen(function* digestFacts() {
    const digest = yield* Digest.Digest;
    return {
      bytes: ByteCount.make(bytes.length),
      sha256: Sha256.make(yield* digest.digestHex(Digest.sha256, bytes)),
    };
  });

export const declare = (bodies: Bodies) =>
  Effect.gen(function* declareManifest() {
    const manifest: Manifest = {
      original: yield* factsOf(bodies.original),
      w1200: {
        ...(yield* factsOf(bodies.w1200)),
        height: EDGES.w1200[1],
        width: EDGES.w1200[0],
      },
      w2400: {
        ...(yield* factsOf(bodies.w2400)),
        height: EDGES.w2400[1],
        width: EDGES.w2400[0],
      },
      w480: {
        ...(yield* factsOf(bodies.w480)),
        height: EDGES.w480[1],
        width: EDGES.w480[0],
      },
    };
    return { bodies, manifest, photoId: photoIdOf(manifest) };
  });

export const validBodies = (seed = 0): Bodies => ({
  original: Uint8Array.from(
    { length: ORIGINAL_BYTES },
    (_, index) => (index + seed) % 251
  ),
  w1200: webp("VP8L", ...EDGES.w1200),
  w2400: webp("VP8X", ...EDGES.w2400),
  w480: webp("VP8 ", ...EDGES.w480),
});

export const fixture = () => declare(validBodies());
