# Phase 3 private-writer consolidation stack

**Status:** non-normative implementation index  
**Tracking:** issue #11  
**Governance:** does not select FCP-0003 D1–D7 or allocate EXP-0003

This file records which experiment owns each current private-writer/restart invariant. It exists to prevent later work from reviving older competing branch implementations merely because they contain useful historical evidence.

## Integration spine

The active integration spine is the durability/restart line, augmented with unique confidentiality evidence from the earlier encrypted-tree line.

The current order is:

| Experiment | Current role | Status |
|---|---|---|
| 0170 | real AES-256-GCM retained-descriptor integration baseline | historical prerequisite |
| 0171 | encrypted bounded sorter descriptor payloads | historical prerequisite retained as sorter confidentiality evidence |
| 0172 | encrypted locator/page-reference fixed-stage contract | historical source of unique tree-stage confidentiality evidence |
| 0173 | real HMAC restart-journal authentication | historical source of real keyed journal-auth evidence |
| 0174 | durable nonce/restart continuation spine | accepted prerequisite |
| 0175 | durable publication and crash-authoritative retirement | accepted prerequisite |
| 0176 | unified encrypted private-storage lifecycle quota | accepted |
| 0177 | encrypted locator/page-reference staging transplanted onto restart spine | accepted |
| 0178 | authenticated caller-owned external source-set/order authority | accepted |
| 0179 | bounded restart-metadata checkpoint/compaction | pending |

## Ownership by invariant

### Deterministic bounded ordering

The current sorter is the bounded deterministic external-sort implementation carried by the durability/restart stack. Earlier encrypted-sorter experiments contribute confidentiality/regression evidence but do not justify maintaining a second merge/order/duplicate implementation.

### Private-stage confidentiality

Current consolidated confidentiality is:

- encrypted sorter descriptor payloads;
- retained encrypted descriptors;
- encrypted locators;
- encrypted page references at every tree level.

Clear sorter ordering keys/object IDs and spill geometry remain explicit confidentiality non-claims.

### Nonce authority

The current authority chain is:

- append-only authenticated nonce generations;
- one exact fresh combined descriptor+tree lease on restart;
- no counter reuse after committed-generation failure;
- Experiment 0179 checkpoint is intended to replace only obsolete history, never a generation record still required by live restart evidence.

Local HMAC/checkpoint integrity is not an anti-rollback anchor against external deletion/replay.

### Restart-stage authority

A restart stage is not authority merely because ciphertext exists. The current chain is:

1. durable nonce generation;
2. exact encrypted stage identity;
3. authenticated stage manifest;
4. authenticated caller-owned external source-set identity when source-bound restart is required;
5. verification of all of the above before a fresh restart lease.

`strong_version` remains an immutable payload-view token, not a provider/resource identity.

### Publication authority

Only parent-directory-synced no-replace publication produces `PublishedAndDurable` evidence capable of authorizing old restart-state retirement. Destination-exists and publication-indeterminate outcomes do not mint destructive cleanup authority.

### Destructive retirement

Experiment 0175 remains the sole retirement authority format:

`durable publication -> durable Prepared -> classify both cleanup targets -> final identity checks -> unlink -> directory sync -> durable Terminal`

Later experiments reuse this protocol rather than inventing another cleanup journal.

### Private-storage quota

Experiment 0176 supplies the lifecycle accounting vocabulary; 0177 replaces plaintext tree-stage widths with encrypted frame widths; 0178 adds persistent source-set authority bytes. Quota rejection on restart is intended to occur before a fresh nonce generation or private-output mutation.

Experiment 0179 must not make checkpoint/compaction metadata free: any production integration must charge the transient checkpoint plus surviving protected metadata.

## Historical branches versus current implementation

Older branches remain useful for:

- failure corpora;
- exact cryptographic framing evidence;
- nonce/restart crash-cut evidence;
- negative tests;
- design provenance.

They are not automatically merge dependencies. A feature is carried forward only when its unique invariant is reproduced on the current integration spine with current CI.

## Remaining #11 classes after 0179

Even if 0179 is accepted, issue #11 still requires qualification/policy work including:

- physical power-loss/filesystem qualification of the asserted sync ordering;
- explicit supported-filesystem/network-filesystem policy;
- stronger isolation or an explicit production policy for the remaining Linux same-UID final check -> unlink race;
- production AES/HMAC key provisioning, rotation, and failure policy;
- a non-rollbackable freshness anchor if rollback resistance is claimed;
- filesystem free-space/concurrent-consumption policy beyond arithmetic quota planning;
- production qualification beyond native Linux x86_64 crypto evidence.

## Broader Phase 3 boundary

This private-writer stack does not complete Phase 3 by itself. Normative EXP-0003 convergence, live provider qualification under #10, and independent implementation/external clean-room evidence under #12 remain separate Phase 3 exit gates.
