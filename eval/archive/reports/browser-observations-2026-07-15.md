# Qualitative browser observations

Date: 2026-07-15

These are visible-product probes, not benchmark measurements. They were run
after the overlay was designed and therefore must not be mixed into recall
metrics or treated as held-out evidence.

## Bible Gateway (default NIV search)

- `the fall` promotes Genesis 3 as a suggested result, ahead of literal keyword
  matches.
- `fall of man` also promotes Genesis 3. This note originally described a
  hand-authored mapping that has since been retired. The current release gets
  `the fall` from comprehensive BSB event annotations; this historical probe
  remains regression evidence, not held-out evidence.
- `faith is the substance of things hoped for` returns zero Bible results and
  recommends trying another translation. This is the failure mode addressed by
  a hidden, reference-aligned alternate-text lane.

## YouVersion (default NIV search)

- `the fall` starts with Jeremiah 48:16 and Ezekiel 40:1, then broadens into
  unrelated Numbers results. It does not recover the Genesis 3 story intent.
- `faith is the substance of things hoped for` returns Hebrews 11:1 in NIV,
  demonstrating useful translation-variant recall while keeping the displayed
  translation canonical.
- The home page exposes named story routes for Adam and Eve, Noah's Ark, David
  and Goliath, the Prodigal Son, and the Good Samaritan. This supports treating
  named events as a typed retrieval lane rather than ordinary fuzzy text.

## Product implication

The two failure modes should remain separate:

1. Source-backed concept/event recognition maps a typed name to a passage.
2. Alternate-text retrieval maps a remembered translation surface to the same
   canonical BSB reference.

Both lanes may generate candidates, but only BSB references and BSB text are
returned. Low-confidence topics remain `Related`; they are not promoted to
`Recovered` merely because another product surfaces them.
