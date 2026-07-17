# Canonical BSB display with hidden evidence

## Status

Accepted

Date: 2026-07-17

## Context

Users remember KJV (and other) wording, but multi-translation display and
storage raise licensing and UX complexity. ESV distribution is separately
licensed and must not be assumed.

## Decision

- **Canonical display text is always BSB** for every result.
- Other translations (starting with public-domain KJV) may be indexed as
  **hidden evidence** (exact / bounded-fuzzy phrase match) mapped to the same
  canonical coordinates.
- ESV is an **adapter boundary only**: no ESV text or generated ESV overlay is
  shipped by default; hosts supply one only under appropriate rights.
- Reference parsing and multi-translation *display* remain host concerns.

## Consequences

- Strong KJV-drift recall without relicensing display.
- Users always see one consistent corpus (BSB).
- Hosts that want multi-version UI must layer their own licensed text.
