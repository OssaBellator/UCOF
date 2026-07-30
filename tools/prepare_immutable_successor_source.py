#!/usr/bin/env python3
"""Apply pending structural and compile fixes before successor verification."""

from pathlib import Path

PATH = Path("crates/ucof-experiments/src/immutable_successor.rs")


def replace_once(text: str, needle: str, replacement: str, label: str) -> str:
    if needle not in text:
        raise SystemExit(f"{label} insertion point not found")
    return text.replace(needle, replacement, 1)


def main() -> None:
    text = PATH.read_text(encoding="utf-8")
    if "fn checked_range<'a>(" not in text:
        text = replace_once(
            text,
            "fn checked_range(\n    data: &[u8],",
            "fn checked_range<'a>(\n    data: &'a [u8],",
            "checked-range lifetime input",
        )
        text = replace_once(
            text,
            ") -> Result<&[u8], ImmutableError> {",
            ") -> Result<&'a [u8], ImmutableError> {",
            "checked-range lifetime output",
        )
    if "    root: PageRef,\n    footer_offset: usize," in text:
        text = text.replace(
            "    root: PageRef,\n    footer_offset: usize,",
            "    footer_offset: usize,",
            1,
        )
        text = text.replace(
            "        locators,\n        root,\n        footer_offset,",
            "        locators,\n        footer_offset,",
            1,
        )
    if "#[allow(clippy::too_many_arguments)]\nfn parse_page(" not in text:
        text = replace_once(
            text,
            "fn parse_page(\n",
            "// The traversal state remains explicit so every bounded collection is visible.\n#[allow(clippy::too_many_arguments)]\nfn parse_page(\n",
            "parse-page lint scope",
        )
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
