/* The /photos/ page in the browser. The page ships its shell; this fetches
   the manifest, fills the grid, and runs the larger view: a modal <dialog>
   with Left, Right, Home, End, Escape and a touch swipe, whose URL is
   `/photos/#<id>`. */

import type { Photo } from "../lib/gallery";
import {
  altText,
  aspectRatio,
  caption,
  firstRowCount,
  idFromHash,
  imageUrl,
  sizesFor,
  srcset,
  step,
  swipeStep,
  tileSizes,
} from "../lib/gallery";
import { parseManifest } from "../lib/gallery-manifest";

const LOAD_TIMEOUT_MS = 15_000;
/** The unit on a computed length, such as the root's `16px`. */
const PX = /px$/u;
/** The larger view never asks for more than this. */
const LARGE_SIZES = ["w1200", "w2400"] as const;

/** How far each key moves through the photos. Home and End go to the ends. */
const KEY_STEPS = new Map([
  ["ArrowLeft", -1],
  ["ArrowRight", 1],
  ["End", Number.POSITIVE_INFINITY],
  ["Home", Number.NEGATIVE_INFINITY],
]);

type State = "loading" | "ready" | "empty" | "failed";

const MESSAGES: Record<Exclude<State, "ready">, string> = {
  empty: "No photos yet.",
  failed: "The photos didn’t load.",
  loading: "Loading photos…",
};

const part = <T extends Element>(root: ParentNode, selector: string): T => {
  const element = root.querySelector<T>(selector);
  if (element === null) {
    throw new Error(`The photo gallery is missing ${selector}.`);
  }
  return element;
};

const fetchPhotos = async (base: string): Promise<Photo[] | null> => {
  try {
    const response = await fetch(`${base}/v1.json`, {
      signal: AbortSignal.timeout(LOAD_TIMEOUT_MS),
    });
    if (!response.ok) {
      return null;
    }
    const body: unknown = await response.json();
    return parseManifest(body)?.photos ?? null;
  } catch {
    return null;
  }
};

const tileImage = (base: string, photo: Photo, eager: boolean) => {
  const { w480 } = photo.sizes;
  const img = document.createElement("img");
  img.alt = altText(photo);
  img.width = w480.width;
  img.height = w480.height;
  img.decoding = "async";
  img.loading = eager ? "eager" : "lazy";
  /* srcset and sizes before src, so the browser picks once. */
  img.sizes = tileSizes(photo);
  img.srcset = srcset(base, photo);
  img.src = imageUrl(base, w480);
  return img;
};

export const mountGallery = (root: HTMLElement): void => {
  const base = root.dataset.galleryBase;
  if (base === undefined || base === "") {
    return;
  }
  const status = part<HTMLElement>(root, ".status");
  const retry = part<HTMLButtonElement>(root, ".retry");
  const grid = part<HTMLUListElement>(root, ".photo-grid");
  const viewer = part<HTMLDialogElement>(root, ".viewer");
  const stage = part<HTMLElement>(viewer, ".stage");
  const large = part<HTMLImageElement>(viewer, ".large");
  const count = part<HTMLElement>(viewer, ".count");
  const meta = part<HTMLElement>(viewer, ".meta");
  const previous = part<HTMLButtonElement>(viewer, ".previous");
  const next = part<HTMLButtonElement>(viewer, ".next");
  const close = part<HTMLButtonElement>(viewer, ".close");

  let photos: Photo[] = [];
  let tiles: HTMLAnchorElement[] = [];
  let current = 0;
  /* The touch a swipe follows, and whether the last one moved a photo. */
  let touch: { id: number; x: number; y: number } | null = null;
  let swiped = false;

  const setState = (state: State) => {
    root.dataset.state = state;
    status.textContent =
      state === "ready"
        ? `${photos.length} ${photos.length === 1 ? "photo" : "photos"}`
        : MESSAGES[state];
    grid.hidden = state !== "ready";
    retry.hidden = state !== "failed";
  };

  const show = (index: number) => {
    const photo = photos[index];
    if (photo === undefined) {
      return;
    }
    current = index;
    const { w2400 } = photo.sizes;
    /* The tile's image, already loaded, holds the frame until the larger
       one arrives, so switching photos never flashes an empty box. */
    const placeholder = tiles[index]?.querySelector("img")?.currentSrc ?? "";
    large.style.backgroundImage =
      placeholder === "" ? "" : `url("${placeholder}")`;
    large.style.setProperty("--ar", String(aspectRatio(photo)));
    large.alt = altText(photo);
    large.width = w2400.width;
    large.height = w2400.height;
    large.sizes = sizesFor(photo);
    large.srcset = srcset(base, photo, LARGE_SIZES);
    large.src = imageUrl(base, photo.sizes.w1200);
    count.textContent = `${index + 1} of ${photos.length}`;
    meta.textContent = caption(photo);
    previous.ariaDisabled = String(index === 0);
    next.ariaDisabled = String(index === photos.length - 1);
    history.replaceState(null, "", `#${photo.id}`);
  };

  const open = (index: number) => {
    show(index);
    if (!viewer.open) {
      viewer.showModal();
    }
  };

  const go = (delta: number) => {
    const target = step(current, delta, photos.length);
    if (target !== current) {
      show(target);
    }
  };

  const openFromHash = () => {
    const id = idFromHash(location.hash);
    const index = photos.findIndex((photo) => photo.id === id);
    if (index !== -1) {
      open(index);
    } else if (viewer.open) {
      viewer.close();
    }
  };

  const render = () => {
    /* Only the first row loads at once; the grid is hidden until it fills,
       so the column is the gallery's width. */
    const eager = firstRowCount(photos.map(aspectRatio), {
      column: root.clientWidth,
      rem: Number(
        getComputedStyle(document.documentElement).fontSize.replace(PX, "")
      ),
      viewport: window.innerWidth,
    });
    tiles = photos.map((photo, index) => {
      const link = document.createElement("a");
      link.className = "photo-tile";
      link.href = `#${photo.id}`;
      link.append(tileImage(base, photo, index < eager));
      link.addEventListener("click", (event) => {
        /* A modified click opens the shareable link in a new tab as usual. */
        const modified =
          event.metaKey || event.ctrlKey || event.shiftKey || event.altKey;
        if (modified || event.button !== 0) {
          return;
        }
        event.preventDefault();
        open(index);
      });
      return link;
    });
    grid.replaceChildren(
      ...tiles.map((link, index) => {
        const item = document.createElement("li");
        const photo = photos[index];
        if (photo !== undefined) {
          item.style.setProperty("--ar", String(aspectRatio(photo)));
        }
        item.append(link);
        return item;
      })
    );
  };

  const load = async () => {
    setState("loading");
    const loaded = await fetchPhotos(base);
    if (loaded === null) {
      setState("failed");
      return;
    }
    photos = loaded;
    render();
    setState(photos.length === 0 ? "empty" : "ready");
    openFromHash();
  };

  large.addEventListener("load", () => {
    large.style.backgroundImage = "";
  });
  previous.addEventListener("click", () => go(-1));
  next.addEventListener("click", () => go(1));
  close.addEventListener("click", () => viewer.close());
  /* A touch swipe across the photo moves one photo. A second finger, as in
     a pinch, or a scroll the browser takes over ends the swipe. */
  viewer.addEventListener("pointerdown", (event) => {
    swiped = false;
    const onStage =
      event.target instanceof Node && stage.contains(event.target);
    touch =
      onStage && event.pointerType === "touch" && event.isPrimary
        ? { id: event.pointerId, x: event.clientX, y: event.clientY }
        : null;
  });
  stage.addEventListener("pointerup", (event) => {
    if (touch?.id !== event.pointerId) {
      return;
    }
    const delta = swipeStep(touch, { x: event.clientX, y: event.clientY });
    touch = null;
    if (delta !== 0) {
      swiped = true;
      go(delta);
    }
  });
  stage.addEventListener("pointercancel", () => {
    touch = null;
  });
  /* A click on the empty space around the photo closes it, as Escape does,
     unless it ends a swipe. */
  viewer.addEventListener("click", (event) => {
    if (swiped) {
      swiped = false;
      return;
    }
    if (event.target === viewer || event.target === stage) {
      viewer.close();
    }
  });
  viewer.addEventListener("keydown", (event) => {
    if (event.altKey || event.ctrlKey || event.metaKey) {
      return;
    }
    const delta = KEY_STEPS.get(event.key);
    if (delta !== undefined) {
      event.preventDefault();
      go(delta);
    }
  });
  /* Focus goes back to the tile of the photo on screen, which is not the
     tile that opened the view once the reader has moved through a few. */
  viewer.addEventListener("close", () => {
    history.replaceState(null, "", `${location.pathname}${location.search}`);
    large.removeAttribute("src");
    large.removeAttribute("srcset");
    tiles[current]?.focus();
  });
  retry.addEventListener("click", () => {
    load();
  });
  window.addEventListener("hashchange", openFromHash);

  load();
};
