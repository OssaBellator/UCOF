# Experiment 0104: Unix publication directory identity pinning

## Question

Can the path-based Unix staging backend detect replacement of its staging directory or destination parent during one publication lifecycle without claiming descriptor-relative race freedom?

## Policy

After the inherited Unix backend successfully begins private staging, the wrapper records each directory's filesystem device and inode. Every later path-dependent operation re-reads the directory metadata and fails closed if the observed identity differs:

- private validation and synchronization require the pinned staging directory;
- no-overwrite publication requires both pinned directories;
- destination-parent synchronization requires the pinned destination directory;
- private retirement requires both identities;
- abort requires the pinned staging directory.

A failed identity check does not follow the replacement path and does not attempt cleanup in that namespace. Private state remains unresolved for operator recovery.

## Evidence

Deterministic Unix tests cover:

- a stable lifecycle delegating private abort normally;
- staging-directory replacement after begin, which blocks cleanup and leaves the original staged file under the displaced directory;
- destination-parent replacement after begin, which blocks publication and creates no artifact in the replacement directory.

## Boundary

This remains path-based. An identity check and the following filesystem operation are separate syscalls, so a concurrent replacement can still race between them. The wrapper does not provide descriptor-relative `openat`/`linkat` handles, effective-user or namespace policy, authenticated journaling, encryption, physical power-loss qualification, or network-filesystem semantics. It is an incremental fail-closed detection layer, not production filesystem hardening.
