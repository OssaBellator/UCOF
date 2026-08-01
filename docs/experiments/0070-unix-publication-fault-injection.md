# Experiment 0070: Unix publication authority-boundary fault injection

## Question

Do the publication outcome labels remain aligned with observable Unix filesystem side effects when execution stops at each authority boundary?

## Harness

The fault-injection harness performs real private staging, exclusive `0600` file creation, validation, staged-file synchronization, hard-link publication, destination-directory synchronization, and private-name retirement. It can stop deterministically:

- before destination link creation;
- after link creation but before destination-directory synchronization;
- after destination-directory synchronization;
- during private-name retirement.

## Evidence

The pinned cases establish:

- a pre-link failure leaves no destination and reports `NotPublished`;
- a post-link/pre-sync failure leaves an observable destination and reports `PublicationIndeterminate`;
- a post-directory-sync failure leaves the destination and reports `PublishedAndDurable` even while the private name remains;
- retirement failure cannot downgrade durable success and retains an explicit cleanup-policy error;
- the no-fault path retires the private name after durable publication.

## Boundary

The injection is process-level and deterministic. It is not power-loss testing, does not model storage-controller caches, does not restart a new process to inspect recovery policy, and remains subject to the Unix/filesystem limitations documented in Experiment 0066.
