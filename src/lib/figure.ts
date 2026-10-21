/* Client helpers for the homepage figures. Each figure renders its finished
   state on the server, so readers without JavaScript still see the result;
   these helpers rewind it and play it through once it scrolls into view. */

export const reducedMotion = (): boolean =>
	matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Calls `play` the first time half of `el` is on screen. */
export const whenVisible = (el: Element, play: () => void): void => {
	const observer = new IntersectionObserver(
		(entries) => {
			if (entries.some((entry) => entry.isIntersecting)) {
				observer.disconnect();
				play();
			}
		},
		{ threshold: 0.5 },
	);
	observer.observe(el);
};

const PLAYED = "figures-played";

const playedFigures = (): string[] => {
	try {
		// SAFETY: only playOnce writes this key, always as a JSON array of keys.
		return JSON.parse(sessionStorage.getItem(PLAYED) ?? "[]") as string[];
	} catch {
		return [];
	}
};

/** Calls `play` the first time half of `el` is on screen, once per visit:
    coming back to the page in the same session leaves the figure finished. */
export const playOnce = (key: string, el: Element, play: () => void): boolean => {
	if (playedFigures().includes(key)) {
		return false;
	}
	whenVisible(el, () => {
		try {
			sessionStorage.setItem(PLAYED, JSON.stringify([...playedFigures(), key]));
		} catch {
			/* Storage can be unavailable; the figure then plays every visit. */
		}
		play();
	});
	return true;
};

/** Calls `onChange` with whether `el` is on screen, so loops can pause. */
export const onVisibility = (el: Element, onChange: (visible: boolean) => void): void => {
	new IntersectionObserver((entries) => {
		for (const entry of entries) {
			onChange(entry.isIntersecting);
		}
	}).observe(el);
};

/** A sequence that a replay can cancel partway through. */
export const sequence = () => {
	let run = 0;
	return {
		/** Starts a new run and cancels the one before it. */
		start: () => {
			run += 1;
			const id = run;
			return {
				/** Resolves `true` after `ms`, or `false` once a newer run started. */
				wait: (ms: number) =>
					new Promise<boolean>((resolve) => {
						setTimeout(() => resolve(id === run), reducedMotion() ? 0 : ms);
					}),
			};
		},
	};
};

/** Animates elements from their old positions to their new ones (FLIP). */
export const flip = (items: HTMLElement[], change: () => void): void => {
	const before = new Map(items.map((item) => [item, item.getBoundingClientRect().top]));
	change();
	if (reducedMotion()) {
		return;
	}
	for (const item of items) {
		const delta = (before.get(item) ?? 0) - item.getBoundingClientRect().top;
		if (delta !== 0) {
			item.animate([{ transform: `translateY(${delta}px)` }, { transform: "none" }], {
				duration: 420,
				easing: "cubic-bezier(0.645, 0.045, 0.355, 1)",
			});
		}
	}
};
