import { expect, it } from "@effect/vitest";
import { Exit, Schema } from "effect";

import fixtures from "../fixtures/contract.json" with { type: "json" };
import { BeginRequest, Unauthorized } from "../src/http/contract.ts";
import {
  FinalizeRejected,
  HashMismatch,
  LengthMismatch,
  PhotoAlreadyDraft,
  PhotoManifestConflict,
  PhotoNotFound,
  PhotoView,
  StorageUnavailable,
} from "../src/photos/domain.ts";

const errors = [
  Unauthorized,
  PhotoNotFound,
  PhotoManifestConflict,
  PhotoAlreadyDraft,
  LengthMismatch,
  HashMismatch,
  FinalizeRejected,
  StorageUnavailable,
];

/** One JSON document from fixtures/contract.json, which the Rust client tests share. */
type FixtureDocument =
  | (typeof fixtures)["begin"]
  | (typeof fixtures)["views"][number]
  | (typeof fixtures)["errors"][number]["body"];

const roundTrip = <S extends Schema.Codec<unknown, unknown>>(
  schema: S,
  value: FixtureDocument | undefined
) => Schema.encodeUnknownSync(schema)(Schema.decodeUnknownSync(schema)(value));

it("every shared view fixture decodes and re-encodes to the same JSON", () => {
  for (const view of fixtures.views) {
    expect(roundTrip(PhotoView, view)).toEqual(view);
  }
  expect(roundTrip(BeginRequest, fixtures.begin)).toEqual(fixtures.begin);
});

it("every shared error fixture carries its error's tag and status and round-trips", () => {
  expect(fixtures.errors).toHaveLength(errors.length);
  for (const [index, error] of errors.entries()) {
    const fixture = fixtures.errors[index];
    expect(error.ast.annotations?.httpApiStatus).toBe(fixture?.status);
    expect(roundTrip(error, fixture?.body)).toEqual(fixture?.body);
  }
});

it("every invalid begin fixture fails the request schema", () => {
  for (const begin of fixtures.invalidBegins) {
    expect(Exit.isFailure(Schema.decodeUnknownExit(BeginRequest)(begin))).toBe(
      true
    );
  }
});
