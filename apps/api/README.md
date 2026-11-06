# Photo upload API

[Issue #7](https://github.com/ParthMmm/web2026/issues/7). The desktop uploads one photo, an original and three WebP derivatives, as a verified private draft. Nothing is published yet.

## Calls

Every call needs `Authorization: Bearer <OWNER_TOKEN>`. Every call is safe to repeat.

| Call | Route | Result |
| --- | --- | --- |
| `photos.begin` | `POST /v1/photos` with `{ manifest, photoId }` | `Uploading` view that lists the missing objects |
| `photos.get` | `GET /v1/photos/{photoId}` | `Uploading` or `Draft` view |
| `photos.put` | `PUT /v1/photos/{photoId}/{kind}` with the raw bytes and `Content-Length` | 204 |
| `photos.finalize` | `POST /v1/photos/{photoId}/finalize` | `Draft` view |

`photoId` is the SHA-256 of the original. The manifest declares the size and SHA-256 of each object, and the width and height of each derivative. `kind` is `original`, `w480`, `w1200` or `w2400`.

The Worker never trusts a declared hash. It passes the hash to R2 with each write, and R2 refuses bytes that do not match before it stores them. `finalize` reads each WebP header, refuses metadata, animation and wrong dimensions, then copies the objects from `staging/` to `photos/`. R2 checks that copy too. One D1 row per photo holds the manifest and the time it became a draft. Each write is one statement, because D1 has no transactions.

Errors are tagged JSON bodies. `fixtures/contract.json` holds one of each, and the TypeScript and Rust tests both read it.

## Layout

- `src/photos/photos.ts`: the `Photos` module, with `begin`, `get`, `store` and `finalize`.
- `src/photos/blobs.ts`: the `Blobs` port. `blobs-r2.ts` is the Worker adapter. `blobs-memory.ts` is the test adapter.
- `src/photos/internal/`: object keys, the D1 ledger and the WebP header reader. Only `src/photos` may import it.
- `src/photos/infra.ts`: the private R2 bucket, with a 7-day expiry on `staging/`, and the D1 database. Both are retained in the `prod` stage.
- `src/http/`: the HttpApi contract, the handlers, owner auth and the request log.
- `src/worker.ts` and `alchemy.run.ts`: the `PhotosApi` Worker in the `web2026-photos` stack. The stack also declares the `Public` bucket that publishing (#9) will use.
- `client/`: the hand-written Rust client and the smoke binary.

## Local checks

```sh
cd apps/api
bun install --frozen-lockfile
bun run typecheck
bun run test
bun run test:workerd
bun run openapi --check
cargo fmt --manifest-path client/Cargo.toml --all -- --check
cargo test --locked --manifest-path client/Cargo.toml
bun run smoke
```

- `test` runs on Node. It covers `Photos` on in-memory blobs and the real migration on SQLite, the HTTP routes, the OpenAPI export, the shared fixtures and the import boundaries.
- `test:workerd` runs in workerd through Miniflare. It runs the same blob contract on the R2 adapter with a real R2 binding and the ledger on a real D1. It also bundles the Worker the way Alchemy does and checks that runtime init answers with 401, not a crash.
- `smoke` bundles the Worker into workerd with local R2 and D1, then runs the Rust client over HTTP. The client sends a wrong-hash upload, a short upload, all four objects, and two finalize calls.
- `bun run dev` serves the same local Worker. Set `OWNER_TOKEN` first.

No check deploys or needs Cloudflare credentials.

## Cloud proof

Run this by hand on a throwaway stage. Alchemy's `default` profile can authenticate. Keep Cloudflare credentials out of this directory. Create an ignored `.env` with a fresh `OWNER_TOKEN` of at least 32 random characters and mode 0600. See `.env.example`.

```sh
umask 077
bun run alchemy plan --stage photos-proof
bun run alchemy deploy --stage photos-proof --yes
set -a; . ./.env; set +a
bun scripts/photo-fixture.ts /tmp/photo-proof
cargo run --locked --manifest-path client/Cargo.toml -- https://THE-RETURNED-HOST.workers.dev /tmp/photo-proof
bun run alchemy destroy --stage photos-proof --yes
```

Also check these by hand before destroy. The wrong-hash upload leaves no object in the bucket. The `staging/` and `photos/` keys are not reachable from a public URL. Empty the bucket before destroy, because only the `prod` stage retains it.

Not yet proven in the cloud: R2's checksum refusal, the streaming copy and the staging expiry rule on production R2, and the single-statement ledger on production D1. Local workerd proves the same code paths.

## Cost

Counted from the code, one photo costs about 8 R2 writes, about 15 R2 reads and about 10 D1 statements. At list price that is a small fraction of a cent, and it fits inside the R2 and D1 free tiers. Storage costs most: about $0.015 per GB-month after the 10 GB free tier. Staged copies live until `finalize` deletes them, or at most 7 days.

## Contract drift

CI in `.github/workflows/api-contract.yml` runs the local checks above and **oasdiff 1.31.0** against `baseline/openapi-v1.json`:

```sh
oasdiff breaking --allow-external-refs=false --fail-on WARN baseline/openapi-v1.json openapi.json
```

The baseline was reset once, to the first export of this API. The old baseline described the contract probe, and no client was ever installed against it, so the reset hides no break. From now on, do not regenerate the baseline to pass a breaking change.

Keep the operation IDs stable. Installed clients ignore unknown fields, so new optional response fields are safe. Removing a field, requiring a new request field or adding a view or error variant breaks installed clients. Add a new version and a migration window for those.

## Rust generator decision

The client in `client/` is hand-written on `reqwest` **0.13.5** and Serde, and checked against `fixtures/contract.json`.

OpenAPI Generator **7.23.0**, generator `rust`, library `reqwest`, says its OpenAPI 3.1 support is beta. Against the probe's validated document it generated `ProbePhotoState` as a struct that required `_tag: Published` and `url`. It lost the valid `Draft` branch of Effect's `anyOf`. Compilation succeeded, but fixture decoding failed:

```text
"Draft": Err(Error("unknown variant `Draft`, expected `Published`", ...))
"Published": Ok(...)
```

The experiment crate was removed because CI never ran it. This result is its record. It is evidence about one generator, configuration and version, not a claim that all generators fail.

## Versions

Effect **4.1.2**, Alchemy **2.0.0-beta.85** and their supporting packages are pinned exactly in `package.json` and `bun.lock`. Bun **1.4.1** owns this directory's TypeScript dependencies. Cargo owns `client/`. The Worker runs with compatibility date 2026-08-01, the newest date the pinned Miniflare supports.

References: [Effect HTTP API](https://alchemy.run/cloudflare/apis/effect-http-api), [Alchemy Worker](https://alchemy.run/providers/cloudflare/workers/worker/), [oasdiff](https://github.com/oasdiff/oasdiff).
