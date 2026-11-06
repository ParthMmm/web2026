import { Effect, Layer, Redacted } from "effect";
import { Digest, Mac } from "effect/crypto";

import { OwnerAuth, Unauthorized } from "./contract.ts";

const encode = (text: string) => new TextEncoder().encode(text);

// Digests have one fixed length, so the constant-time compare never leaks the token's length.
export const ownerTokenAuth = (token: Redacted.Redacted<string>) =>
  Layer.effect(
    OwnerAuth,
    Effect.gen(function* () {
      const digest = yield* Digest.Digest;
      const wanted = yield* digest.digest(
        Digest.sha256,
        encode(Redacted.value(token))
      );
      return OwnerAuth.of({
        bearer: (httpEffect, { credential }) =>
          Effect.gen(function* () {
            const given = yield* digest.digest(
              Digest.sha256,
              encode(Redacted.value(credential))
            );
            return Mac.timingSafeEqual(wanted, given)
              ? yield* httpEffect
              : yield* new Unauthorized();
          }),
      });
    })
  );
