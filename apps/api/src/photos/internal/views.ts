import type { DraftView, PublishedView, SettledView } from "../domain.ts";
import type { Draft, Published, Stored, Unpublishing } from "./ledger.ts";

export const draftView = ({
  finalizedAt,
  manifest,
  photoId,
}: Draft): DraftView => ({ _tag: "Draft", finalizedAt, manifest, photoId });

export const publishedView = ({
  details,
  finalizedAt,
  manifest,
  photoId,
  publishedAt,
}: Published): PublishedView => ({
  _tag: "Published",
  details,
  finalizedAt,
  manifest,
  photoId,
  publishedAt,
});

/** Every state past upload reads straight from its row; only Uploading needs storage. */
export const settledView = (
  stored: Exclude<Stored, { readonly _tag: "Uploading" }>
): SettledView => {
  switch (stored._tag) {
    case "Draft": {
      return draftView(stored);
    }
    case "Published": {
      return publishedView(stored);
    }
    case "Unpublishing": {
      const { details, finalizedAt, manifest, photoId }: Unpublishing = stored;
      return { _tag: "Unpublishing", details, finalizedAt, manifest, photoId };
    }
    default: {
      return stored satisfies never;
    }
  }
};
