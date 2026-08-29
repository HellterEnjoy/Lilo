# Lilo analytics Worker

This Worker accepts opt-in daily aggregate counters from Lilo and protected
repository-traffic snapshots from GitHub Actions. The D1 binding is named `DB`.

The public deployment currently used by official Lilo builds is
`https://lilo-analytics.miaccu23.workers.dev`. Its routes are:

- `GET /health` — service availability only;
- `POST /v1/daily` — validated opt-in desktop aggregates;
- `DELETE /v1/data` — deletion by installation identifier;
- `POST /v1/github-traffic` — token-protected repository traffic ingestion.

## Deploy

1. Apply `migrations/0001_schema.sql` to the remote `database_lilo` database.
2. Create a random secret of at least 32 characters and store it as the Worker
   secret `GITHUB_TRAFFIC_INGEST_TOKEN`.
3. Deploy the Worker from this directory or connect this directory to the
   existing `lilo-analytics` Worker in the Cloudflare dashboard.
4. Store the same secret in GitHub Actions as
   `LILO_TRAFFIC_INGEST_TOKEN`.
5. Store a fine-grained GitHub token with read-only `Administration` access to
   the Lilo repository as the Actions secret `TRAFFIC_TOKEN`.

Never commit either token. The public desktop endpoint and D1 identifiers in
`wrangler.jsonc` are configuration, not credentials.

The application endpoint accepts only the public feature whitelist. The GitHub
traffic endpoint additionally requires the Worker secret. There is no public
endpoint for reading D1 data. Data handling and user controls are documented in
the repository's [`PRIVACY.md`](../PRIVACY.md).
