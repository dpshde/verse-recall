# Trust hierarchy and confidence policy

## Status

Accepted

Date: 2026-07-17

## Context

Mixing BM25, expansions, dense scores, topics, and graph edges without
authority rules over-claims recovery and breaks hard-negative safety. Score
magnitudes are not comparable across lanes or engine versions.

## Decision

1. **Rank and confidence are separate.** Public scores are order-only; do not
   threshold them across versions.
2. **Source authority is typed** at build/open time:
   - Strong lexical / selective translation may claim `Recovered` under strict
     coverage and ambiguity rules.
   - Bounded BSB events and scored topics remain `Related`.
   - Graph people/places/event shadows are **score-zero explanation** only.
   - Reviewed corrections use exact claim match → `OftenMisremembered`.
3. Dense and low-authority evidence cannot mint recovery alone or promote graph
   shadows into ranked aliases.
4. Only the lead result may present a high recovery claim; demote followers
   that would over-claim.
5. Prefer conservative fusion over unrestricted all-lane RRF or raw dense-rank
   summation when those harm breadth or hard negatives (reverted historically).

## Consequences

- Safer product language (`Recovered` means something).
- Graph breadth without polluting ranking.
- Fusion remains nuanced and harder to explain than “just RRF.”
