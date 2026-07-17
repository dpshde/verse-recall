# Modular pack degradation

## Status

Accepted

Date: 2026-07-17

## Context

Not every product tier can ship dense vectors or host an encoder. Web and
constrained mobile clients may need lexical-only recall. Forcing a single
monolithic artifact would either bloat small clients or strip advanced lanes
from large ones.

## Decision

Ship **independently optional packs** with one shared canonical identity
(BSB, 31,086 verses):

| Profile | Packs | Approx. size |
|---|---|---:|
| Lexical | base (`META+TEXT+LEX+POST`) + `EXPAND` | 11.5 MiB |
| Recall | + overlay (`VROVL001`) | 18.6 MiB |
| Semantic | + `LVEC` | 26.2 MiB |
| Optional depth | + `PVEC` passage table | ~27.6 MiB |

Opening extra packs must never change the display corpus identity. Missing
optional packs simply disable those lanes.

## Consequences

- Clean degrade path when encoder or download budget is limited.
- Build and verify complexity: multiple artifacts, hashes, and normalizer
  identity coupling (LEX/EXPAND must match).
- Product messaging must name which profile is in use.
