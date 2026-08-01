# Experiment 0083: Persistent mixed append suffix

## Question

Can the authenticated canonical mixed-operation writer avoid cloning the complete predecessor and successor while preserving the exact bytes, absolute references, page reuse, and linked commit identity of the existing persistent writer?

## Construction

The experiment validates the predecessor and inventories its current pages as before, but keeps the predecessor bytes borrowed. New object records, changed pages, snapshot, and footer are written into a suffix buffer whose logical position begins at the predecessor length. Every locator and page reference therefore records its final absolute file offset.

The commit start for an exact-end append is the byte immediately after the predecessor footer, which is also the predecessor length. The new commit digest is computed over the suffix through the snapshot plus the footer semantics. Exact predecessor pages remain reusable only when their complete locator or ordered child-reference bodies match the final canonical groups.

`append_persistent_mixed_suffix` returns only the append bytes and authenticated report. `write_persistent_mixed_batch_to` writes the borrowed prefix followed by that suffix using bounded requests.

## Evidence

- stable-height, root-collapse, and root-growth outputs are byte-identical to `append_persistent_mixed_batch` after prefix/suffix concatenation;
- report, page-write, and page-reuse accounting match the full writer;
- the concatenated file passes canonical occupancy validation;
- caller operation order cannot change the suffix;
- sink requests remain within the configured bound;
- invalid configuration fails before output, while a sink failure after writing begins is terminal and returns no report;
- the existing mixed-operation fuzz target also checks suffix/full-writer equivalence across bounded root-leaf and multi-leaf shapes.

## Boundary

The input remains an in-memory slice and validation still retains bounded locator and page inventories. The append suffix itself is buffered, so this removes the complete predecessor/successor clone rather than proving constant memory. Writing a complete replacement file still copies the predecessor bytes to the sink. Atomic visibility, durable staging, encryption, and cleanup belong to the separate spill publication protocol.
