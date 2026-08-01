# Experiment 0066: Unix no-overwrite spill publication harness

## Question

Can the spill publication policy be exercised against real Unix filesystem primitives without collapsing destination-exists, not-published, indeterminate, and durable outcomes?

## Harness

The research harness:

- requires a real, non-symlink staging directory with no group or world permission bits;
- derives one private staged name from the non-zero operation ownership token;
- creates the staged file exclusively with mode `0600`;
- verifies regular-file type, one link, owner consistency with the staging directory, private permissions, and exact length;
- invokes caller validation before publication;
- synchronizes the staged file;
- uses a same-filesystem hard link as a no-overwrite publication primitive;
- synchronizes the destination directory before reporting durable success;
- retires the private staged name and synchronizes the staging directory;
- rejects encrypted-spill-required policy before creating plaintext bytes.

## Evidence

Tests cover durable publication, destination preservation when already present, validation failure cleanup, encryption-policy rejection, and symlink staging-directory rejection.

## Boundary

This is Unix research evidence, not a qualified production adapter. It does not encrypt spill bytes, obtain an effective-user identity independently, hold directory file descriptors across every path operation, prove mount durability semantics, inject power loss, or establish behavior for network and non-Unix filesystems.
