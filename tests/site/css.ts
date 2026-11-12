const UTILITIES_LAYER = "@layer utilities{";
const NOT_SELECTORS = /url\([^)]*\)|"[^"]*"|'[^']*'/gu;
const CLASS_SELECTOR = /\.(?<name>-?[_a-zA-Z](?:\\.|[\w-])*)/gu;
const SCOPED_CLASS_SELECTOR =
  /\.(?<name>-?[_a-zA-Z](?:\\.|[\w-])*)\[data-astro-cid-/gu;

/** Splits a minified stylesheet into Tailwind's utilities layer and the rest. */
export const splitUtilities = (css: string) => {
  const start = css.indexOf(UTILITIES_LAYER);
  if (start === -1) {
    return { rest: css, utilities: "" };
  }
  let depth = 0;
  let end = start;
  for (; end < css.length; end += 1) {
    if (css[end] === "{") {
      depth += 1;
    } else if (css[end] === "}") {
      depth -= 1;
      if (depth === 0) {
        break;
      }
    }
  }
  return {
    rest: css.slice(0, start) + css.slice(end + 1),
    utilities: css.slice(start, end + 1),
  };
};

const names = (css: string, selector: RegExp): Set<string> =>
  new Set(
    [...css.replaceAll(NOT_SELECTORS, "").matchAll(selector)].map((match) =>
      (match.groups?.name ?? "").replaceAll("\\", "")
    )
  );

/** Every class name a stylesheet selects on. */
export const classNames = (css: string): Set<string> =>
  names(css, CLASS_SELECTOR);

/** The class names Astro components style in their own scoped `<style>`. */
export const scopedClassNames = (css: string): Set<string> =>
  names(css, SCOPED_CLASS_SELECTOR);
