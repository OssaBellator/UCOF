# Experiment 0085: Spill restart classification

## Question

What may a fresh process conclude from surviving private staging and destination names without confusing observable filesystem state with durable publication authority?

## Policy

A destination name alone never proves durable publication. Durable classification requires all of:

- an authenticated, ownership-bound restart journal;
- a journal phase at or beyond successful destination-directory synchronization;
- a present regular destination whose length and SHA-256 match the journal's expected artifact;
- no contradictory retirement or ownership facts.

A valid owned staged artifact with no destination is retained for retry. An invalid owned artifact may be removed. Foreign or unverifiable private state is never removed automatically. A destination surviving without qualifying directory-sync evidence is publication-indeterminate. Contradictory durable or retired records require manual intervention rather than automatic downgrade or repair.

## Unix inspection

`inspect_unix_spill_after_restart` inspects regular files without following symlinks, derives private-name ownership from the expected ownership token, and validates matching artifacts by length and SHA-256. Expected artifact facts must come from separately authenticated durable metadata. The inspector only classifies; it does not remove, link, synchronize, or publish files.

A fresh-process integration test creates staged, linked, synced, and retired states in one parent process, then launches separate test processes to classify:

- valid owned stage retained for retry;
- surviving link without directory-sync authority as indeterminate;
- directory-synced destination with owned stage as durable cleanup pending;
- retired private name with valid destination as durable success.

## Independent and hostile evidence

Sixteen independent Python traces pin empty, retry, invalid cleanup, foreign preservation, no-journal destination, link ambiguity, invalid link, durable cleanup, durable completion, missing durable destination, retired-name contradiction, unauthenticated journal, wrong owner, missing linked state, and foreign post-durability cases. Aggregate SHA-256: `ba51372e0ef74192c0ee36f628a14c40a51eab886a3db0e8e4b89630d8a35929`.

The `spill_restart_classification` fuzz target checks determinism and proves that:

- foreign or unverifiable state is never selected for owned cleanup;
- a destination without a journal is never classified durable;
- every durable disposition requires authenticated matching journal authority, a synced-or-retired phase, and a valid destination;
- invalid-stage removal requires an owned invalid stage and no destination.

## Boundary

This is fresh-process logical and filesystem-state evidence, not physical power-loss qualification. It does not prove that journal or directory synchronization survives storage-controller caches, torn writes, filesystem bugs, or network filesystems. Journal authentication, encryption, descriptor-relative secure handles, effective-user ownership, atomic journal replacement, cleanup execution, and platform qualification remain production requirements.
