/* The public gallery's photos and the pure rules the /photos/ page runs on.
   `gallery-manifest.ts` parses the manifest into these types, so every image
   URL here is one of the three published WebP sizes under the base. */

export const SIZE_KEYS = ["w480", "w1200", "w2400"] as const;
export type SizeKey = (typeof SIZE_KEYS)[number];

export interface Size {
  width: number;
  height: number;
  /** Relative to the gallery base: `photos/<id>/<key>-<sha256>.webp`. */
  path: string;
}

export interface Photo {
  id: string;
  publishedAt: string;
  /** Camera local date-time text, such as EXIF's "2024:05:03 14:22:01". */
  capturedAt?: string;
  alt?: string;
  film?: string;
  sizes: Record<SizeKey, Size>;
}

export interface Manifest {
  version: 1;
  generatedAt: string;
  /** Newest first. */
  photos: Photo[];
}

export const HEX_64 = /^[0-9a-f]{64}$/u;
const CALENDAR_DATE = /^(?<year>\d{4})[-:](?<month>\d{2})[-:](?<day>\d{2})/u;
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** `PUBLIC_GALLERY_URL` as a base without a trailing slash, or `null` when
    it is unset or not an http(s) URL. */
export const galleryBase = (value?: string): string | null => {
  const trimmed = value?.trim() ?? "";
  if (!URL.canParse(trimmed)) {
    return null;
  }
  const { protocol } = new URL(trimmed);
  return protocol === "https:" || protocol === "http:"
    ? trimmed.replace(/\/+$/u, "")
    : null;
};

/** The address of one published size. */
export const imageUrl = (base: string, size: Size): string =>
  `${base}/${size.path}`;

/** A `srcset` over the given sizes. Small originals are never upscaled, so
    two sizes can share a width; each width is offered once. */
export const srcset = (
  base: string,
  photo: Photo,
  keys: readonly SizeKey[] = SIZE_KEYS
): string => {
  const widths = new Set<number>();
  const candidates: string[] = [];
  for (const key of keys) {
    const size = photo.sizes[key];
    if (!widths.has(size.width)) {
      widths.add(size.width);
      candidates.push(`${imageUrl(base, size)} ${size.width}w`);
    }
  }
  return candidates.join(", ");
};

/** Width over height, to three places. */
export const aspectRatio = (photo: Photo): number => {
  const { width, height } = photo.sizes.w2400;
  return Math.round((width / height) * 1000) / 1000;
};

/** `sizes` for the larger view. It fits the photo inside the viewport less
    the dialog's margins and its caption bar, so its width is capped by the
    viewport's width or, for a tall photo, by its height. Keep the two
    lengths in step with the dialog's CSS. */
export const sizesFor = (photo: Photo): string =>
  `min(100vw - 2rem, (100dvh - 7rem) * ${aspectRatio(photo)})`;

const calendarDate = (text: string | undefined): string | null => {
  const match = text?.match(CALENDAR_DATE)?.groups;
  if (match === undefined) {
    return null;
  }
  const month = MONTHS[Number(match.month) - 1];
  const day = Number(match.day);
  if (month === undefined || day < 1 || day > 31) {
    return null;
  }
  return `${day} ${month} ${match.year}`;
};

const film = (photo: Photo): string | null => {
  const name = photo.film?.trim() ?? "";
  return name === "" ? null : name;
};

/** What the image shows, for `alt`. Falls back to when and on what it was
    taken, then to when it was published. Never the file name or hash. */
export const altText = (photo: Photo): string => {
  const own = photo.alt?.trim() ?? "";
  if (own !== "") {
    return own;
  }
  const taken = calendarDate(photo.capturedAt);
  const stock = film(photo);
  if (taken !== null || stock !== null) {
    return [
      "Photo",
      taken === null ? null : `taken ${taken}`,
      stock === null ? null : `on ${stock}`,
    ]
      .filter((part) => part !== null)
      .join(" ");
  }
  const published = calendarDate(photo.publishedAt);
  return published === null ? "Photo" : `Photo published ${published}`;
};

/** The larger view's caption: capture date and film, or empty. */
export const caption = (photo: Photo): string =>
  [calendarDate(photo.capturedAt), film(photo)]
    .filter((part) => part !== null)
    .join(" · ");

/** The index after moving `delta` from `index`. Stops at the ends. */
export const step = (index: number, delta: number, length: number): number =>
  Math.min(Math.max(index + delta, 0), Math.max(length - 1, 0));

/** The photo id a shared `/photos/#<id>` link names, or `null`. */
export const idFromHash = (hash: string): string | null => {
  const id = hash.replace(/^#/u, "");
  return HEX_64.test(id) ? id : null;
};
