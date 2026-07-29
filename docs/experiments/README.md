# Phase 1 Experiments

These experiments collect reproducible evidence for disposable `UCOF-EXP-0001` decisions. They do not stabilize wire bytes.

| Experiment | Question | Reproduction |
|---|---|---|
| [0001 — framing widths](0001-framing-widths.md) | What storage cost does the fixed 40-byte record header impose relative to a compact variable-width strawman? | `python3 tools/experiment_framing_widths.py` |
| [0002 — footer discovery](0002-footer-discovery.md) | What ambiguity and work are introduced by bounded backward footer search? | `python3 tools/experiment_footer_discovery.py` |
| [0003 — scale limits](0003-scale-limits.md) | Can the current directory and record model satisfy UC-02 object counts? | `python3 tools/experiment_scale_limits.py` |

Results are deterministic size and candidate-count measurements. Wall-clock benchmark claims are deliberately excluded until representative implementations and range-backed I/O exist.
