# Effect → Rust contract prototype

[Issue #4](https://github.com/ParthMmm/web2026/issues/4). A stateless contract probe, not the gallery API. It stores nothing, uploads nothing, and echoes photo metadata and serves two fixed fixture pages. The test bearer token is not production authentication.

## Boundaries and versions

- Effect **4.0.0-rc.112**, Alchemy **2.0.0-beta.76**, and their supporting packages are pinned exactly in `package.json` and `bun.lock`. Newer releases were blocked by the machine's three-day minimum-release-age policy; that policy was not overridden. This RC uses `Schema.TaggedError`, not the newer docs' `TaggedErrorClass`.
- Bun **1.4.1** owns this directory's TypeScript dependencies. Local validation used 1.4.1-canary.1, Node 24.15.0, Rust 1.97.1, and macOS 26.6.2. CI pins the stable Bun 1.4.1 release.
- Cargo owns the independent `client/` build. Root TypeScript excludes `apps/api`; this package uses its own compiler and Bun types. Existing Astro dependencies, the pnpm lockfile, and unrelated Effect v3 work are untouched; no second lockfile was introduced for that package. Migrate the site deliberately in its hosting ticket.
- `src/contract.ts` is the source for HTTP validation, response encoding, and OpenAPI **3.1.0**. `HttpApiBuilder` implements it. The Worker creates/disposes an Effect scope per request; no request I/O promise is shared across isolates or requests.
- Bearer auth is declared on both operations. Schema errors produce the pinned RC's empty 400 response, documented as such. Unknown cursors produce a tagged `InvalidCursor` 400; bad/missing credentials produce a tagged `Unauthorized` 401.

## Local checks

```sh
cd apps/api
bun install --frozen-lockfile
bun run typecheck
bun run test
bun run openapi --check
cargo test --locked --manifest-path client/Cargo.toml
bun run smoke
```

`smoke` starts the real Worker entrypoint on an ephemeral loopback port with a fresh test token, runs the Rust executable over HTTP, and stops the server. It checks both photo variants, optional/nullable fields, UTC timestamp strings, both pages, structured errors, six invalid inputs, and repeated requests. The Rust transport is blocking: call it off GPUI's UI thread if integrating this prototype. Production async/upload integration is not part of this ticket.

## Cloudflare proof

Alchemy's existing `default` profile can authenticate the deployment. Login with `bun run alchemy login` if needed. Do not put Cloudflare credentials in this project. Create an ignored `.env` with a fresh, random `PROBE_TOKEN` (at least 32 random bytes); use permissions 0600. See `.env.example` for the variable name.

```sh
umask 077
bun run alchemy plan --stage contract-probe
bun run alchemy deploy --stage contract-probe --yes
# Copy only the returned workers.dev URL, not credentials.
set -a; . ./.env; set +a
cargo run --locked --manifest-path client/Cargo.toml -- https://THE-RETURNED-HOST.workers.dev
bun run alchemy destroy --stage contract-probe --yes
```

The stack creates one isolated Worker and a secret binding. It uses local state (`.alchemy/`), no R2/D1, no custom domains, no production resources, and no persistent request logs. Do not delete local state before destroying the Worker: state tracks ownership and may contain secret material. Do not use `--adopt` for this probe.

**Measured cloud evidence, 2026-09-10 UTC:** Alchemy uploaded a 277.59 KB Worker. The Rust executable called the actual `workers.dev` deployment successfully: **2 photo round-trips, 2 pages, 2 structured errors, 6 validation errors, 10 repeated requests**. This is not just generated-type compilation or local emulation. No personal JPEGs or EXIF were sent. The isolated Worker and its secret binding were then destroyed through Alchemy; no test service remains deployed.

## Rust generator decision

Select the small `reqwest` **0.13.5** / Serde client in `client/`, checked against `fixtures/contract.json`. Keep the server schema authoritative; add shared fixtures whenever the wire contract changes.

OpenAPI Generator **7.23.0**, generator `rust`, library `reqwest`, warns that OpenAPI 3.1 support is beta. Against this actual, validated document it generates `ProbePhotoState` as a struct requiring `_tag: Published` and `url`. It loses the valid `Draft` branch of Effect's `anyOf`. Compilation succeeds, but fixture decoding fails:

```text
"Draft": Err(Error("unknown variant `Draft`, expected `Published`", ...))
"Published": Ok(...)
```

Reproduce with OpenAPI Generator 7.23.0 and Java installed:

```sh
openapi-generator version  # must be 7.23.0 for this recorded result
openapi-generator generate -i openapi.json -g rust --library reqwest \
  -o client/generated --additional-properties=packageName=photo_probe_sdk
cargo run --locked --manifest-path generator-check/Cargo.toml
```

The last command intentionally exits **1** when the generator rejects a valid fixture. It is a compatibility experiment, not a failing test in the selected client's suite. Generated code/build output stays ignored. No document relabeling, 3.0 conversion, generated-code hand edits, or assumption of Progenitor compatibility was used. This is evidence about one generator/configuration/version, not a claim that all generators fail.

## Drift and installed-client compatibility

CI in `.github/workflows/api-contract.yml` validates/export-checks the contract, runs TS and Rust fixtures, compiles the selected client, exercises it over real local HTTP, and runs **oasdiff 1.31.0** against retained `baseline/openapi-v1.json`:

```sh
go install github.com/oasdiff/oasdiff@v1.31.0
oasdiff breaking --allow-external-refs=false --fail-on WARN baseline/openapi-v1.json openapi.json
```

The unchanged contract passes. A local negative control that removed `POST /v1/probe/photos` failed with `api-removed-without-deprecation` and exit 1. The retained baseline must not be regenerated to hide a breaking change. CI does not deploy or require Cloudflare credentials; repeat the cloud proof before Effect, Alchemy, or transport upgrades.

Keep operation IDs `probe.echoPhoto` and `probe.listPhotos` stable. An absent `caption` is not a null caption. `capturedAt` and `nextCursor` are required keys that can be null. Timestamps travel as strings, normalized to UTC by Effect; this RC's exported JSON schema does not advertise a date-time format, so shared runtime tests also check invalid date input. Rust retains the wire timestamp string, rather than adding an independent parser with different date rules.

For lagging installed clients, add optional response fields while retaining old fields and behavior. The older-client fixture proves that this client ignores unknown object fields. Do not remove fields, require new request fields, change null semantics, or add response union variants that older clients cannot decode under the same version. Introduce a new version and a migration/deprecation window for those changes. Schema diffing is necessary, not sufficient: keep semantic fixtures and real Worker smoke tests.

References: [Effect HTTP API](https://alchemy.run/cloudflare/apis/effect-http-api), [Alchemy Worker](https://alchemy.run/providers/cloudflare/workers/worker/), [OpenAPI Generator Rust](https://openapi-generator.tech/docs/generators/rust/), [Progenitor](https://github.com/oxidecomputer/progenitor), [oasdiff](https://github.com/oasdiff/oasdiff).

## Film simulation handoff

`Photo` accepts an optional string `filmSimulation`, for example `Classic Chrome`. It means the camera simulation recorded in preserved EXIF. It does not name a Lightroom rendering profile or guarantee the JPEG's final appearance. Older JSON without the key remains valid. Explicit null and non-string values fail validation, matching the optional caption contract. The v1 operation IDs and retained baseline remain unchanged.

The desktop's explicit `export-film-metadata` command writes a versioned manifest of privacy-safe probe records. Exercise those exact records through the real local HTTP server and the Rust client:

```sh
cd apps/api
bun run smoke /path/to/film-metadata.json
```

The client checks manifest version 1 and compares each echoed record with the exported input. It also runs the shared contract fixtures and error probes. The desktop has no HTTP dependency, and this stateless probe does not publish images or store a cloud gallery.
