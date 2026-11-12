/* The gallery manifest's boundary. The /photos/ page fetches
   `<base>/v1.json` in the browser, so a publish shows without a rebuild, and
   nothing from it reaches the page until it parses here. A path must be one
   of the photo's three published WebP sizes, so the page can never request
   an original or a private object, whatever the response holds. */

import type { Manifest, Photo, Size, SizeKey } from "./gallery";
import { HEX_64 } from "./gallery";

const SIZE_FILE = /^w(?:480|1200|2400)-[0-9a-f]{64}\.webp$/u;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const isDimension = (value: unknown): value is number =>
  typeof value === "number" && Number.isInteger(value) && value > 0;

const parseSize = (value: unknown, id: string, key: SizeKey): Size | null => {
  if (
    !(isRecord(value) && isDimension(value.width) && isDimension(value.height))
  ) {
    return null;
  }
  const { path } = value;
  if (typeof path !== "string") {
    return null;
  }
  const [folder, photoId, file, ...rest] = path.split("/");
  const valid =
    rest.length === 0 &&
    folder === "photos" &&
    photoId === id &&
    file !== undefined &&
    SIZE_FILE.test(file) &&
    file.startsWith(`${key}-`);
  return valid ? { height: value.height, path, width: value.width } : null;
};

const optionalText = (value: unknown): string | undefined | null => {
  if (value === undefined) {
    return undefined;
  }
  return typeof value === "string" ? value : null;
};

const parsePhoto = (value: unknown): Photo | null => {
  if (!isRecord(value)) {
    return null;
  }
  const { id, publishedAt, sizes } = value;
  if (
    typeof id !== "string" ||
    !HEX_64.test(id) ||
    typeof publishedAt !== "string" ||
    !isRecord(sizes)
  ) {
    return null;
  }
  const w480 = parseSize(sizes.w480, id, "w480");
  const w1200 = parseSize(sizes.w1200, id, "w1200");
  const w2400 = parseSize(sizes.w2400, id, "w2400");
  const capturedAt = optionalText(value.capturedAt);
  const alt = optionalText(value.alt);
  const film = optionalText(value.film);
  if (
    w480 === null ||
    w1200 === null ||
    w2400 === null ||
    capturedAt === null ||
    alt === null ||
    film === null
  ) {
    return null;
  }
  const photo: Photo = { id, publishedAt, sizes: { w1200, w2400, w480 } };
  if (capturedAt !== undefined) {
    photo.capturedAt = capturedAt;
  }
  if (alt !== undefined) {
    photo.alt = alt;
  }
  if (film !== undefined) {
    photo.film = film;
  }
  return photo;
};

/** The manifest, or `null` when the value does not follow version 1 of the
    contract. A partly valid manifest is a failed load, not a shorter one. */
export const parseManifest = (value: unknown): Manifest | null => {
  if (
    !isRecord(value) ||
    value.version !== 1 ||
    typeof value.generatedAt !== "string" ||
    !Array.isArray(value.photos)
  ) {
    return null;
  }
  const photos: Photo[] = [];
  const seen = new Set<string>();
  for (const raw of value.photos) {
    const photo = parsePhoto(raw);
    if (photo === null || seen.has(photo.id)) {
      return null;
    }
    seen.add(photo.id);
    photos.push(photo);
  }
  return { generatedAt: value.generatedAt, photos, version: 1 };
};
