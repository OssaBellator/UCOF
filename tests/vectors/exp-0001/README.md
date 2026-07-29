# UCOF-EXP-0001 vectors

These files are reproducible fixtures for the first disposable wire experiment.

- `.hex` files are the repository-safe canonical byte fixtures.
- `.json` files describe expected validity or error categories.
- `.ucof` binaries are generated locally by `tools/generate_exp_0001_vectors.py` and are intentionally not required in Git because this repository interface stores text safely.

Generate binary copies with:

```console
python3 tools/generate_exp_0001_vectors.py
```

## Valid vectors

| Vector | Purpose |
|---|---|
| `minimal-valid` | One empty-root manifest and one directory |
| `two-objects` | Non-empty object, zero-length object, optional capability, and direct lookup |

## Invalid or unsupported vectors

| Vector | Expected category |
|---|---|
| `unknown-required-capability` | `unsupported_required_capability` |
| `digest-mismatch` | `digest_mismatch` |
| `truncated-footer` | structural truncation or footer-magic failure |
| `invalid-directory-offset` | `range_out_of_bounds` |

The exact error variant may become more specific during Phase 1, but it must retain a stable mapping to the conceptual category in the experimental specification.
