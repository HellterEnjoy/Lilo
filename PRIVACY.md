# Lilo privacy notice

Effective for Lilo 0.2.1 and later.

Lilo is a local-first Markdown application. Notes, titles, tags, aliases,
attachments, search queries and vault paths stay on the user's device. Lilo
does not require an account.

## Optional usage analytics

Usage analytics are disabled until the user makes an explicit choice. Refusing
analytics does not disable or limit any Lilo feature. The choice can be changed
at any time under **Settings → Privacy & Analytics**.

When analytics are enabled, Lilo sends:

- a randomly generated installation identifier;
- the local calendar date;
- the Lilo version;
- daily numeric counters whose names come from a fixed public whitelist.

The whitelist is defined in `src/analytics.rs`. It represents broad product
actions such as creating a note, using Quick Capture, opening the graph or
adding an attachment. It never contains user-provided values.

Lilo does **not** send note contents, note titles, filenames, filesystem paths,
tags, aliases, search text, Quick Capture text, wiki-link targets, window names,
hardware identifiers, operating-system account details, location or crash
reports.

The installation identifier is a pseudonymous random UUID, not a hardware ID.
It cannot identify a person by itself, but it allows daily records from one
installation to be deduplicated. Removing Lilo's settings or disabling
analytics removes the active identifier from local settings.

Lilo sends an initial daily snapshot after startup and may update that day's
aggregate counters periodically while the application remains open. Repeated
snapshots replace the daily counters with the largest received value rather
than creating a detailed event timeline.

## Purpose and legal basis

The data is used only to estimate active installations, understand which broad
features are useful and prioritise product improvements. Processing is based on
the user's consent. Analytics data is not used for advertising, individual
profiling or sale to third parties.

## Processing and retention

The analytics endpoint runs on Cloudflare Workers and stores accepted fields in
Cloudflare D1. Cloudflare necessarily processes network connection data to
deliver the HTTPS request. Lilo's Worker does not insert the request IP address
or other network headers into D1 and does not log request bodies in application
code.

Rows containing installation identifiers are retained for no longer than 90
days. Disabling analytics queues deletion of all rows associated with that
installation identifier and retries automatically if the device is offline.

## Repository traffic

A separate GitHub Action archives repository-level views, unique visitors,
clones, referrers and popular paths supplied by GitHub's Traffic API. This data
comes from GitHub, not from the Lilo desktop application, and contains no Lilo
installation identifier or note data.

## Control and questions

The exact analytics fields and whitelist are visible inside Lilo. The user can
disable analytics and request deletion directly from the application without
providing a name or email address. General privacy questions can be opened in
the project's GitHub Issues; installation identifiers or other private details
should never be posted in a public issue.
