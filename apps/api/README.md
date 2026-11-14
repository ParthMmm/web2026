# Photo API

[Issue #7](https://github.com/ParthMmm/web2026/issues/7) and PAL-858, PAL-854, PAL-2204. The desktop uploads one photo, an original and three WebP derivatives, as a verified private draft. The original goes straight to R2 through a presigned URL; the derivatives go through the Worker. The owner then publishes the draft to the public gallery, or unpublishes it.

## Calls

Every owner call needs `Authorization: Bearer <OWNER_TOKEN>`. Every call is safe to repeat.

| Call | Route | Result |
| --- | --- | --- |
| `photos.begin` | `POST /v1/photos` with `{ manifest, photoId }` | The photo's view. A new photo is `Uploading` and lists the missing objects |
| `photos.list` | `GET /v1/photos?after=<photoId>&limit=<1..200>` | `{ photos, next }`, in photo ID order. `limit` defaults to 100. Pass `next` as `after` until it is `null` |
| `photos.get` | `GET /v1/photos/{photoId}` | The photo's view |
| `photos.originalUpload` | `POST /v1/photos/{photoId}/original/upload-url` | `{ url, method, headers, expiresAt }`: a presigned R2 PUT for the original. `PhotoAlreadyDraft` (409) once the photo is past upload |
| `photos.put` | `PUT /v1/photos/{photoId}/{kind}` with the raw bytes and `Content-Length` | 204. For derivatives, and for an original of at most 100 MB |
| `photos.finalize` | `POST /v1/photos/{photoId}/finalize` | `Draft` view, or the view of a photo that is already past upload |
| `photos.publish` | `POST /v1/photos/{photoId}/publish` with `{ alt?, capturedAt?, film? }` | `Published` view |
| `photos.unpublish` | `POST /v1/photos/{photoId}/unpublish` | `Draft` view |

A view is one of `Uploading`, `Draft`, `Published` or `Unpublishing`. `Published` adds `publishedAt` and `details`. `Unpublishing` means public removal started and has not finished; another `unpublish` finishes it. `publish` and `unpublish` answer `PhotoNotDraft` (409) while the photo is `Uploading`.

`publish` details are optional text: `alt` up to 300 characters, `capturedAt` up to 40 (the camera's local date-time, with no zone), and `film` up to 64. Publishing a published photo again replaces its details and keeps its first `publishedAt`.

`begin` with a new manifest for a `Draft` reopens it for upload, so the desktop can send re-rendered derivatives. Objects it already holds under the new manifest count as uploaded. The next `finalize` removes the objects of the old manifest. `begin` with a new manifest for a `Published` or `Unpublishing` photo answers `PhotoManifestConflict`; unpublish it first.

`photoId` is the SHA-256 of the original. The manifest declares the size and SHA-256 of each object, and the width and height of each derivative. `kind` is `original`, `w480`, `w1200` or `w2400`. An original may be up to 5 GB, the most R2 takes in one PUT. A derivative may be up to 100 MB, the most the Worker takes in one request.

The Worker never trusts a declared hash. It passes the hash to R2 with each write, and R2 refuses bytes that do not match before it stores them. For the original, `originalUpload` signs the declared `content-length` and `x-amz-checksum-sha256` into a SigV4 URL for the original's final key, valid for 15 minutes. The client sends the bytes to that URL with exactly those headers. R2 refuses another length or other headers (403), and bytes whose SHA-256 differs (400 `BadDigest`), before it stores them. The original's bytes never pass through the Worker, so the Worker's 100 MB request limit and its upload speed do not apply. The URL can write only that one key, and only those bytes. The signing key comes from an API token that Alchemy mints at deploy, scoped to writing objects in the private bucket. `finalize` reads each WebP header, refuses metadata, animation and wrong dimensions, then copies the objects from `staging/` to `photos/`. R2 checks that copy too. `publish` checks each copy into the public bucket the same way.

Errors are tagged JSON bodies. `fixtures/contract.json` holds one of each, and the TypeScript and Rust tests both read it.

## Public gallery

Anyone can read these routes. They need no token, and they read only the public bucket.

| Route | Result |
| --- | --- |
| `GET /gallery/v1.json` | The gallery manifest. `Cache-Control: public, max-age=60`. Before the first publish it is an empty gallery, not a 404 |
| `GET /gallery/photos/{photoId}/{kind}-{sha256}.webp` | One derivative. `Content-Type: image/webp`, `Cache-Control: public, max-age=31536000, immutable` |

Both send `Access-Control-Allow-Origin: *`. Any other path under `/gallery/`, an original, or a draft's derivative answers 404 with `Cache-Control: no-store`.

The manifest is `{ version: 1, generatedAt, photos }`. Each photo has `id`, `publishedAt`, the optional `alt`, `capturedAt` and `film`, and `sizes.w480`, `sizes.w1200` and `sizes.w2400`, each `{ width, height, path }`. `path` is relative to the gallery base, so the site fetches `<base>/gallery/<path>`. Photos with a `capturedAt` come first, newest first by that text, then the rest; ties go newest `publishedAt` first, then by `id`. The manifest holds no original, no private key and no EXIF beyond these fields. `id` is the photo ID, which is the original's SHA-256: it names the photo but cannot recover it.

`GalleryManifest` in `src/photos/gallery-manifest.ts` is the schema. `fixtures/gallery-v1.json` is a shared example that the tests decode.

A browser or proxy that already holds an image or a manifest keeps it until its cache expires: a minute for the manifest, a year for an image. Unpublishing cannot recall a copy that someone downloaded or that a cache kept.

## State model

One D1 row per photo. Its state follows from its columns, so there is no state column to keep in step:

| State | `finalized_at` | `publication` | `published_at`, `details` |
| --- | --- | --- | --- |
| `Uploading` | null | null | null |
| `Draft` | set | null | null |
| `Published` | set | `'Published'` | set |
| `Unpublishing` | set | `'Unpublishing'` | set |

Migration `0002_publication.sql` adds checks that refuse any other combination. Every transition is one `UPDATE`, because D1 has no transactions and R2 shares none with it. The gallery manifest is derived: every `publish` and `unpublish` rewrites it from the `Published` rows.

`publish` runs these steps, and each step only does what is still missing:

1. Copy the three derivatives from `photos/` in the private bucket to the public bucket, each verified against its hash. Copies already there are skipped. The original is never copied.
2. Mark the row `Published` with the details, only if it still holds the manifest that was copied.
3. Rewrite the manifest.

`unpublish` runs these steps:

1. Mark a `Published` row `Unpublishing`.
2. Rewrite the manifest, which now leaves the photo out.
3. Delete every public object under `photos/<photoId>/`.
4. Mark an `Unpublishing` row `Draft`.

A failure at any step leaves a state that is safe for visitors, and the same call repeated finishes the work. A visitor never sees a listed photo whose copies are missing: copies exist before the mark, and the manifest drops the photo before the delete. The API never reports `Draft` while public copies remain. A failed publish can leave unlisted copies of a draft; `unpublish` of a `Draft` deletes them too. Concurrent `publish` and `unpublish` of the same photo end in the state of the call that finishes last, and either call repeated converges.

## Layout

- `src/photos/photos.ts`: the `Photos` module, with `begin`, `list`, `get`, `store` and `finalize`.
- `src/photos/publishing.ts`: the `Publishing` module, with `publish` and `unpublish`.
- `src/photos/public-gallery.ts`: the `PublicGallery` module, which reads the manifest and the derivatives. Its layer needs only the public bucket.
- `src/photos/gallery-manifest.ts`: the `GalleryManifest` schema and the function that builds it.
- `src/photos/upload-urls.ts`: the `UploadUrls` port, which signs the original's presigned PUT with the private bucket's S3 credentials (Alchemy's `R2.S3Credentials`).
- `src/photos/blobs.ts`: the blob port, under two tags: `Blobs` for the private bucket and `PublicBlobs` for the public one. `blobs-r2.ts` is the Worker adapter. `blobs-memory.ts` is the test adapter.
- `src/photos/internal/`: object keys, the D1 ledger and the WebP header reader. Only `src/photos` may import it.
- `src/photos/infra.ts`: the private R2 bucket, with a 7-day expiry on `staging/`, the public R2 bucket and the D1 database. All three are retained in the `prod` stage.
- `src/http/`: the HttpApi contract, the handlers, owner auth, the request log and the public gallery routes.
- `src/worker.ts` and `alchemy.run.ts`: the `PhotosApi` Worker in the `web2026-photos` stack.
- `client/`: the hand-written Rust client, its `Gallery` reader for the public routes, and the smoke binary.

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

- `test` runs on Node. It covers `Photos` and `Publishing` on in-memory blobs and the real migrations on SQLite, with faults injected between every step of publish and unpublish. It also covers the HTTP routes, including the public ones without a token, the OpenAPI export, the shared fixtures, the stack's removal policies and the import boundaries.
- `test:workerd` runs in workerd through Miniflare. It runs the same blob contract on the R2 adapter with a real R2 binding, and upload, publish, unpublish and list on real R2 and D1. It also bundles the Worker the way Alchemy does, checks that runtime init answers with 401, not a crash, and reads the public gallery from the bundled Worker.
- `smoke` bundles the Worker into workerd with local R2 and D1, then runs the Rust client over HTTP. The original is 100 MiB plus one byte, larger than the Worker takes. `scripts/local-s3.ts` stands in for R2's S3 endpoint: it checks each presigned PUT's signature and expiry and refuses bytes that do not match the signed SHA-256 with `BadDigest`, as R2 does, then writes them into the local bucket. Alchemy's own local S3 endpoint ignores `x-amz-checksum-sha256`, so it cannot show that refusal. The client sends a wrong-hash upload, a short upload, a wrong-hash original to storage, all four objects with the original straight to storage, and two finalize calls. It then publishes twice, lists, reads the manifest and all three derivatives without a token, checks that the draft's derivatives and the original were never public, unpublishes twice, and checks that the public copies are gone.
- `bun run dev` serves the same local Worker, with the local S3 stand-in. Set `OWNER_TOKEN` first.

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

Also check these by hand before destroy. The wrong-hash upload leaves no object in the bucket. That fixture's original is small; also run the client against a fixture with an original over 100 MB (`bun scripts/photo-fixture.ts /tmp/photo-proof-large 104857601`), and check that R2 answered the wrong-hash original with 400 `BadDigest` and that `finalize` found the presigned original. `finalize` finds it only if R2 records the SHA-256 from `x-amz-checksum-sha256` in the object's checksums, which the local stand-in cannot prove. The deploy mints an account API token for the Worker, so Alchemy's credentials must be allowed to create one. The `staging/` and `photos/` keys are not reachable from a public URL. Empty the bucket before destroy, because only the `prod` stage retains it.

Last run on 2026-11-06 against real R2 and D1: the full Rust client flow passed, including wrong-hash and short uploads. The final objects matched their SHA-256, staging was empty after finalize, the private bucket had no r2.dev URL or custom domain, and the `staging/` rule was live. Repeat it before each Effect or Alchemy upgrade.

## Cost

Counted from the code, one upload costs about 7 R2 writes, about 15 R2 reads and about 10 D1 statements. At list price that is a small fraction of a cent, and it fits inside the R2 and D1 free tiers. Storage costs most: about $0.015 per GB-month after the 10 GB free tier. Staged copies live until `finalize` deletes them, or at most 7 days. The original is written to its final key, not to `staging/`, because R2 checks it as it arrives. If its upload never finishes, the original stays there; the `staging/` rule does not remove it.

One first publish costs 4 R2 Class A operations (3 copies and the manifest), 6 Class B operations (3 checks in the public bucket and 3 reads in the private one) and 3 D1 statements. The last statement reads every published row to rebuild the manifest. Publishing again with copies in place costs 1 Class A, 3 Class B and 3 D1 statements. One unpublish costs 2 Class A operations (the manifest and the list; deletes are free) and 4 D1 statements. Published derivatives are stored twice, once in each bucket.

Each public read is one Worker request and one Class B operation, because the Worker does not cache. Browsers keep images for a year and the manifest for a minute, which limits repeat reads from one visitor. At list price ($4.50 per million Class A, $0.36 per million Class B), a gallery of a few hundred photos stays inside the free tiers.

## Contract drift

CI in `.github/workflows/api-contract.yml` runs the local checks above and **oasdiff 1.31.0** against `baseline/openapi-v1.json`:

```sh
oasdiff breaking --allow-external-refs=false --fail-on WARN baseline/openapi-v1.json openapi.json
```

The baseline was reset once, to the first export of this API. The old baseline described the contract probe, and no client was ever installed against it, so the reset hides no break.

The baseline was reset a second time for publishing (PAL-858, PAL-854). The `Published` and `Unpublishing` view tags extend the views of `begin` and `get`, and `finalize` now answers the photo's real view instead of only `Draft`. oasdiff reports both as breaking. No desktop release has ever called this API, because the desktop app had no upload yet, so the reset hides no break from an installed client. From now on, do not regenerate the baseline to pass a breaking change.

Keep the operation IDs stable. Installed clients ignore unknown fields, so new optional response fields are safe. Removing a field, requiring a new request field or adding a view or error variant breaks installed clients. Add a new version and a migration window for those.

## Rust generator decision

The client in `client/` is hand-written on `reqwest` **0.13.5** and Serde, and checked against `fixtures/contract.json`.

OpenAPI Generator **7.23.0**, generator `rust`, library `reqwest`, says its OpenAPI 3.1 support is beta. Against the probe's validated document it generated `ProbePhotoState` as a struct that required `_tag: Published` and `url`. It lost the valid `Draft` branch of Effect's `anyOf`. Compilation succeeded, but fixture decoding failed:

```text
"Draft": Err(Error("unknown variant `Draft`, expected `Published`", ...))
"Published": Ok(...)
```

The experiment crate was removed because CI never ran it. This result is its record. It is evidence about one generator, configuration and version, not a claim that all generators fail.

## Client transport

The client keeps idle connections open and reuses them. The Worker can refuse an upload before it reads the body, for example a `LengthMismatch`, and then close the connection. When a request fails to send on a connection that closed, the client sends it once more on a new connection. A timeout is never retried. `tests/transport.rs` proves the retry against a server that closes a reused connection, and fails when the retry is removed.

`put_file(photo_id, kind, path)` streams a file from disk with its `Content-Length`, so a camera original is never read into memory. `put` takes `Bytes`, so a retry reuses the body without a copy.

Measured on 2026-11-12 against `bun scripts/serve.ts` on localhost. Each run sent 20 uploads and 2 refused short uploads. The figure is the median of 5 interleaved runs, under a load average of 18 to 30 on 10 cores:

| Upload             | New connection each time | Reused connection |
| ------------------ | ------------------------ | ----------------- |
| 8 MiB original     | 173.2 ms                 | 171.8 ms          |
| 64-byte derivative | 4.4 ms                   | 3.8 ms            |

On localhost there is no TLS handshake to save, so the gain is small. Repeat the run with `client/examples/upload_bench.rs`:

```sh
bun scripts/photo-fixture.ts /tmp/bench-photo 8388608
OWNER_TOKEN=... cargo run --release --manifest-path client/Cargo.toml --example upload_bench -- http://127.0.0.1:8787 /tmp/bench-photo 20 original
```

## Versions

Effect **4.1.2**, Alchemy **2.0.0-beta.85** and their supporting packages are pinned exactly in `package.json` and `bun.lock`. Bun **1.4.1** owns this directory's TypeScript dependencies. Cargo owns `client/`. The Worker runs with compatibility date 2026-08-01, the newest date the pinned Miniflare supports.

References: [Effect HTTP API](https://alchemy.run/cloudflare/apis/effect-http-api), [Alchemy Worker](https://alchemy.run/providers/cloudflare/workers/worker/), [oasdiff](https://github.com/oasdiff/oasdiff).
