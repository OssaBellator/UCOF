# UCOF-EXP-0001 vectors

These files are reproducible fixtures for the first disposable wire experiment.

- `.hex` files are the repository-safe canonical byte fixtures.
- `.json` files describe expected validity or error categories.
- `.ucof` binaries are generated locally by `tools/generate_exp_0001_vectors.py` and are intentionally not required in Git because this repository interface stores text safely.

Generate binary copies with:

```console
python3 tools/generate_exp_0001_vectors.py
```

Validate the entire corpus with the independent stdlib parser:

```console
python3 tools/validate_exp_0001.py --vectors tests/vectors/exp-0001
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
| `truncated-footer` | `invalid_magic` because exact-end discovery reads a shifted footer candidate |
| `invalid-directory-offset` | `range_out_of_bounds` |

The error category is part of this experimental corpus. A later epoch may choose different footer discovery and therefore classify the same physical truncation differently.
