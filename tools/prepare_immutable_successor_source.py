#!/usr/bin/env python3
"""Apply pending structural checks before rustfmt exports the successor API."""

from pathlib import Path

PATH = Path("crates/ucof-experiments/src/immutable_successor.rs")


def replace_once(text: str, needle: str, replacement: str, label: str) -> str:
    if needle not in text:
        raise SystemExit(f"{label} insertion point not found")
    return text.replace(needle, replacement, 1)


def main() -> None:
    text = PATH.read_text(encoding="utf-8")
    if 'Invalid("page overlap")' not in text:
        text = replace_once(
            text,
            '''    if end > snapshot_offset {
        return Err(ImmutableError::Invalid("page range"));
    }
    if seen.len() >= limits.max_pages {
''',
            '''    if end > snapshot_offset {
        return Err(ImmutableError::Invalid("page range"));
    }
    if structural_ranges
        .iter()
        .any(|(start, stop)| offset < *stop && *start < end)
    {
        return Err(ImmutableError::Invalid("page overlap"));
    }
    if seen.len() >= limits.max_pages {
''',
            "page-overlap",
        )
    if 'footer.page_count_current != u64_from_usize(current_pages)?' not in text:
        text = replace_once(
            text,
            '''    while let Some(reference) = stack.pop() {
        parse_page(
            data,
            &reference,
            snapshot_offset,
            limits,
            &mut seen,
            &mut stack,
            &mut locators,
            &mut structural_ranges,
        )?;
    }
    if locators.is_empty()
''',
            '''    while let Some(reference) = stack.pop() {
        parse_page(
            data,
            &reference,
            snapshot_offset,
            limits,
            &mut seen,
            &mut stack,
            &mut locators,
            &mut structural_ranges,
        )?;
    }
    let current_pages = seen
        .iter()
        .filter(|offset| **offset >= commit_start)
        .count();
    if footer.page_count_current != u64_from_usize(current_pages)? {
        return Err(ImmutableError::Invalid("page count"));
    }
    if locators.is_empty()
''',
            "page-count",
        )
    PATH.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
