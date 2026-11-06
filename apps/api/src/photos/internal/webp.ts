export const WEBP_PREFIX_BYTES = 30;

export type WebpHeader =
  | {
      readonly _tag: "Webp";
      readonly width: number;
      readonly height: number;
      readonly hasMetadata: boolean;
      readonly animated: boolean;
    }
  | { readonly _tag: "NotWebp" };

const NOT_WEBP: WebpHeader = { _tag: "NotWebp" };
const VP8X_ICC = 0x20;
const VP8X_EXIF = 0x08;
const VP8X_XMP = 0x04;
const VP8X_ANIMATION = 0x02;
const VP8L_SIGNATURE = 0x2f;
const FOURTEEN_BITS = 0x3f_ff;

const byte = (bytes: Uint8Array, at: number) => bytes[at] ?? 0;
const u16 = (bytes: Uint8Array, at: number) => byte(bytes, at) | (byte(bytes, at + 1) << 8);
const u24 = (bytes: Uint8Array, at: number) => u16(bytes, at) | (byte(bytes, at + 2) << 16);
const u32 = (bytes: Uint8Array, at: number) => (u24(bytes, at) | (byte(bytes, at + 3) << 24)) >>> 0;
const fourcc = (bytes: Uint8Array, at: number) =>
  String.fromCodePoint(...bytes.subarray(at, at + 4));

const still = (width: number, height: number): WebpHeader => ({
  _tag: "Webp",
  animated: false,
  hasMetadata: false,
  height,
  width,
});

const SIMPLE_CHUNK_START = 20;
const VP8L_VERSION_SHIFT = 29;

// A simple-format file is one chunk, so its own size must fill the object exactly.
const fillsObject = (prefix: Uint8Array, totalBytes: number) => {
  const chunkBytes = u32(prefix, 16);
  return SIMPLE_CHUNK_START + chunkBytes + (chunkBytes & 1) === totalBytes;
};

const parseVp8 = (prefix: Uint8Array, totalBytes: number): WebpHeader => {
  const keyFrame = (byte(prefix, 20) & 1) === 0;
  const startCode =
    byte(prefix, 23) === 0x9d && byte(prefix, 24) === 0x01 && byte(prefix, 25) === 0x2a;
  return keyFrame && startCode && fillsObject(prefix, totalBytes)
    ? still(u16(prefix, 26) & FOURTEEN_BITS, u16(prefix, 28) & FOURTEEN_BITS)
    : NOT_WEBP;
};

const parseVp8l = (prefix: Uint8Array, totalBytes: number): WebpHeader => {
  const bits = u32(prefix, 21);
  return byte(prefix, 20) === VP8L_SIGNATURE &&
    bits >>> VP8L_VERSION_SHIFT === 0 &&
    fillsObject(prefix, totalBytes)
    ? still((bits & FOURTEEN_BITS) + 1, ((bits >>> 14) & FOURTEEN_BITS) + 1)
    : NOT_WEBP;
};

const parseVp8x = (prefix: Uint8Array): WebpHeader => {
  const flags = byte(prefix, 20);
  return {
    _tag: "Webp",
    animated: (flags & VP8X_ANIMATION) !== 0,
    hasMetadata: (flags & (VP8X_ICC | VP8X_EXIF | VP8X_XMP)) !== 0,
    height: u24(prefix, 27) + 1,
    width: u24(prefix, 24) + 1,
  };
};

const parsers: Readonly<
  Partial<Record<string, (prefix: Uint8Array, totalBytes: number) => WebpHeader>>
> = {
  "VP8 ": parseVp8,
  VP8L: parseVp8l,
  VP8X: parseVp8x,
};

// The RIFF size must match the verified object size, so a truncated file fails even with a valid header.
export const parseWebpHeader = (prefix: Uint8Array, totalBytes: number): WebpHeader => {
  const framed =
    prefix.length >= WEBP_PREFIX_BYTES &&
    fourcc(prefix, 0) === "RIFF" &&
    fourcc(prefix, 8) === "WEBP" &&
    u32(prefix, 4) === totalBytes - 8;
  const parse = framed ? parsers[fourcc(prefix, 12)] : undefined;
  return parse === undefined ? NOT_WEBP : parse(prefix, totalBytes);
};
