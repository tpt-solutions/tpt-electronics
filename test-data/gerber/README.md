# Gerber test fixtures

Synthetic 2-layer board used by `tpt-elec-gerber` and `tpt-elec-thermal`
golden tests.

| File | Content |
|---|---|
| `board.gtl` | Top copper: 2 traces, rectangular + round flash |
| `board.gbl` | Bottom copper: 1 trace |
| `board.gko` | Board outline: 46 × 30 mm rectangle |
| `board.drl` | Excellon drills: 2 × Ø0.3 mm, 1 × Ø0.9 mm |

All coordinates are millimeters in 3.6 format (leading zeros omitted).
