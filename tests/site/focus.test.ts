import { expect, test } from "bun:test";

import { builtSite } from "./built-site";

const RULE = /(?<selector>[^{}]+)\{(?<body>[^{}]*)\}/gu;
const FOCUS = /:focus(?:-visible)?(?![\w-])/u;
const REMOVES_OUTLINE = /outline(?:-style)?\s*:\s*(?:none|0)(?![\w.])/u;

test("no rule removes the focus outline from an element the keyboard can reach", () => {
  const removed = [...builtSite().stylesheets].flatMap(([where, css]) =>
    [...css.matchAll(RULE)]
      .filter(
        (rule) =>
          FOCUS.test(rule.groups?.selector ?? "") &&
          REMOVES_OUTLINE.test(rule.groups?.body ?? "")
      )
      .map((rule) => `${where}: ${rule.groups?.selector?.trim()}`)
  );

  expect(removed).toEqual([]);
});
