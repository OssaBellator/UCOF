use std::collections::BTreeSet;
use std::fmt;

/// Minimal locator information used by the Phase 3 directory experiments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectLocator {
    pub object_id: u64,
    pub kind: u16,
    pub offset: u64,
    pub stored_len: u64,
    pub logical_len: u64,
}

/// Summary of a paged directory shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryStats {
    pub entries: u64,
    pub pages: u64,
    pub leaf_pages: u64,
    pub internal_pages: u64,
    pub depth: u32,
}

impl DirectoryStats {
    pub fn estimated_bytes(self, page_size: u64) -> Result<u64, DirectoryBuildError> {
        self.pages
            .checked_mul(page_size)
            .ok_or(DirectoryBuildError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryBuildError {
    InvalidLeafCapacity,
    InvalidFanout,
    DuplicateObjectId(u64),
    ArithmeticOverflow,
}

impl fmt::Display for DirectoryBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLeafCapacity => write!(f, "leaf capacity must be non-zero"),
            Self::InvalidFanout => write!(f, "internal fanout must be at least two"),
            Self::DuplicateObjectId(id) => write!(f, "duplicate object identifier {id}"),
            Self::ArithmeticOverflow => write!(f, "directory size arithmetic overflow"),
        }
    }
}

impl std::error::Error for DirectoryBuildError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryLookupError {
    PageLimitExceeded,
    PageCycle,
    InvalidPageReference,
    EmptyInternalPage,
    InvalidPageRange,
    UnorderedLeaf,
    DuplicateLeafKey(u64),
    UnorderedChildRange,
    OverlappingChildRange,
    ChildRangeMismatch,
}

impl fmt::Display for DirectoryLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PageLimitExceeded => write!(f, "directory page-read limit exceeded"),
            Self::PageCycle => write!(f, "directory page cycle"),
            Self::InvalidPageReference => write!(f, "invalid directory page reference"),
            Self::EmptyInternalPage => write!(f, "empty internal directory page"),
            Self::InvalidPageRange => write!(f, "invalid directory page range"),
            Self::UnorderedLeaf => write!(f, "unordered directory leaf"),
            Self::DuplicateLeafKey(id) => write!(f, "duplicate directory leaf key {id}"),
            Self::UnorderedChildRange => write!(f, "unordered directory child ranges"),
            Self::OverlappingChildRange => write!(f, "overlapping directory child ranges"),
            Self::ChildRangeMismatch => write!(f, "child range does not match referenced page"),
        }
    }
}

impl std::error::Error for DirectoryLookupError {}

/// Result of one bounded directory lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupResult {
    pub locator: Option<ObjectLocator>,
    pub pages_read: u32,
}

/// Non-normative canonical ordered-page research model.
#[derive(Debug, Clone)]
pub struct PagedDirectory {
    pages: Vec<Page>,
    root: usize,
    stats: DirectoryStats,
}

impl PagedDirectory {
    pub fn build(
        mut entries: Vec<ObjectLocator>,
        leaf_capacity: usize,
        fanout: usize,
    ) -> Result<Self, DirectoryBuildError> {
        if leaf_capacity == 0 {
            return Err(DirectoryBuildError::InvalidLeafCapacity);
        }
        if fanout < 2 {
            return Err(DirectoryBuildError::InvalidFanout);
        }

        entries.sort_unstable_by_key(|entry| entry.object_id);
        for pair in entries.windows(2) {
            if pair[0].object_id == pair[1].object_id {
                return Err(DirectoryBuildError::DuplicateObjectId(pair[0].object_id));
            }
        }

        let entry_count =
            u64::try_from(entries.len()).map_err(|_| DirectoryBuildError::ArithmeticOverflow)?;
        let mut pages = Vec::new();
        let mut current_level = Vec::new();

        if entries.is_empty() {
            pages.push(Page::Leaf {
                range: None,
                entries: Vec::new(),
            });
            current_level.push(0);
        } else {
            for chunk in entries.chunks(leaf_capacity) {
                let range = Some(KeyRange {
                    min: chunk.first().expect("non-empty chunk").object_id,
                    max: chunk.last().expect("non-empty chunk").object_id,
                });
                let page_id = pages.len();
                pages.push(Page::Leaf {
                    range,
                    entries: chunk.to_vec(),
                });
                current_level.push(page_id);
            }
        }

        let leaf_pages = u64::try_from(current_level.len())
            .map_err(|_| DirectoryBuildError::ArithmeticOverflow)?;
        let mut depth = 1_u32;
        let mut internal_pages = 0_u64;

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for child_ids in current_level.chunks(fanout) {
                let mut children = Vec::with_capacity(child_ids.len());
                for &page_id in child_ids {
                    let range = pages[page_id]
                        .range()
                        .expect("non-empty child below an internal page");
                    children.push(ChildLink { range, page_id });
                }
                let range = KeyRange {
                    min: children.first().expect("internal children").range.min,
                    max: children.last().expect("internal children").range.max,
                };
                let page_id = pages.len();
                pages.push(Page::Internal { range, children });
                next_level.push(page_id);
                internal_pages = internal_pages
                    .checked_add(1)
                    .ok_or(DirectoryBuildError::ArithmeticOverflow)?;
            }
            current_level = next_level;
            depth = depth
                .checked_add(1)
                .ok_or(DirectoryBuildError::ArithmeticOverflow)?;
        }

        let root = current_level[0];
        let pages_count =
            u64::try_from(pages.len()).map_err(|_| DirectoryBuildError::ArithmeticOverflow)?;
        Ok(Self {
            pages,
            root,
            stats: DirectoryStats {
                entries: entry_count,
                pages: pages_count,
                leaf_pages,
                internal_pages,
                depth,
            },
        })
    }

    #[must_use]
    pub const fn stats(&self) -> DirectoryStats {
        self.stats
    }

    pub fn lookup(
        &self,
        object_id: u64,
        max_pages: u32,
    ) -> Result<LookupResult, DirectoryLookupError> {
        let mut page_id = self.root;
        let mut pages_read = 0_u32;
        let mut visited = BTreeSet::new();

        loop {
            pages_read = pages_read
                .checked_add(1)
                .ok_or(DirectoryLookupError::PageLimitExceeded)?;
            if pages_read > max_pages {
                return Err(DirectoryLookupError::PageLimitExceeded);
            }
            if !visited.insert(page_id) {
                return Err(DirectoryLookupError::PageCycle);
            }
            let page = self
                .pages
                .get(page_id)
                .ok_or(DirectoryLookupError::InvalidPageReference)?;
            page.validate(&self.pages)?;

            match page {
                Page::Leaf { entries, .. } => {
                    let locator = entries
                        .binary_search_by_key(&object_id, |entry| entry.object_id)
                        .ok()
                        .map(|index| entries[index]);
                    return Ok(LookupResult {
                        locator,
                        pages_read,
                    });
                }
                Page::Internal { children, .. } => {
                    let Some(child) = children
                        .iter()
                        .find(|child| child.range.contains(object_id))
                    else {
                        return Ok(LookupResult {
                            locator: None,
                            pages_read,
                        });
                    };
                    page_id = child.page_id;
                }
            }
        }
    }

    pub fn validate(&self, max_pages: u64) -> Result<(), DirectoryLookupError> {
        if u64::try_from(self.pages.len()).map_or(true, |count| count > max_pages) {
            return Err(DirectoryLookupError::PageLimitExceeded);
        }
        for page in &self.pages {
            page.validate(&self.pages)?;
        }
        self.validate_reachable(self.root, max_pages)?;
        Ok(())
    }

    fn validate_reachable(&self, root: usize, max_pages: u64) -> Result<(), DirectoryLookupError> {
        let mut stack = vec![root];
        let mut visited = BTreeSet::new();
        while let Some(page_id) = stack.pop() {
            if !visited.insert(page_id) {
                return Err(DirectoryLookupError::PageCycle);
            }
            if u64::try_from(visited.len()).map_or(true, |count| count > max_pages) {
                return Err(DirectoryLookupError::PageLimitExceeded);
            }
            match self
                .pages
                .get(page_id)
                .ok_or(DirectoryLookupError::InvalidPageReference)?
            {
                Page::Leaf { .. } => {}
                Page::Internal { children, .. } => {
                    stack.extend(children.iter().map(|child| child.page_id));
                }
            }
        }
        Ok(())
    }

    pub fn estimate_shape(
        entry_count: u64,
        leaf_capacity: u64,
        fanout: u64,
    ) -> Result<DirectoryStats, DirectoryBuildError> {
        if leaf_capacity == 0 {
            return Err(DirectoryBuildError::InvalidLeafCapacity);
        }
        if fanout < 2 {
            return Err(DirectoryBuildError::InvalidFanout);
        }

        let mut level_pages = if entry_count == 0 {
            1
        } else {
            div_ceil(entry_count, leaf_capacity)?
        };
        let leaf_pages = level_pages;
        let mut pages = level_pages;
        let mut internal_pages = 0_u64;
        let mut depth = 1_u32;
        while level_pages > 1 {
            level_pages = div_ceil(level_pages, fanout)?;
            pages = pages
                .checked_add(level_pages)
                .ok_or(DirectoryBuildError::ArithmeticOverflow)?;
            internal_pages = internal_pages
                .checked_add(level_pages)
                .ok_or(DirectoryBuildError::ArithmeticOverflow)?;
            depth = depth
                .checked_add(1)
                .ok_or(DirectoryBuildError::ArithmeticOverflow)?;
        }
        Ok(DirectoryStats {
            entries: entry_count,
            pages,
            leaf_pages,
            internal_pages,
            depth,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyRange {
    min: u64,
    max: u64,
}

impl KeyRange {
    const fn contains(self, value: u64) -> bool {
        self.min <= value && value <= self.max
    }

    const fn is_valid(self) -> bool {
        self.min <= self.max
    }
}

#[derive(Debug, Clone, Copy)]
struct ChildLink {
    range: KeyRange,
    page_id: usize,
}

#[derive(Debug, Clone)]
enum Page {
    Leaf {
        range: Option<KeyRange>,
        entries: Vec<ObjectLocator>,
    },
    Internal {
        range: KeyRange,
        children: Vec<ChildLink>,
    },
}

impl Page {
    const fn range(&self) -> Option<KeyRange> {
        match self {
            Self::Leaf { range, .. } => *range,
            Self::Internal { range, .. } => Some(*range),
        }
    }

    fn validate(&self, pages: &[Page]) -> Result<(), DirectoryLookupError> {
        match self {
            Self::Leaf { range, entries } => {
                if entries.is_empty() {
                    if range.is_some() {
                        return Err(DirectoryLookupError::InvalidPageRange);
                    }
                    return Ok(());
                }
                let declared = range.ok_or(DirectoryLookupError::InvalidPageRange)?;
                if !declared.is_valid()
                    || declared.min != entries.first().expect("non-empty leaf").object_id
                    || declared.max != entries.last().expect("non-empty leaf").object_id
                {
                    return Err(DirectoryLookupError::InvalidPageRange);
                }
                for pair in entries.windows(2) {
                    if pair[0].object_id > pair[1].object_id {
                        return Err(DirectoryLookupError::UnorderedLeaf);
                    }
                    if pair[0].object_id == pair[1].object_id {
                        return Err(DirectoryLookupError::DuplicateLeafKey(pair[0].object_id));
                    }
                }
                Ok(())
            }
            Self::Internal { range, children } => {
                if children.is_empty() {
                    return Err(DirectoryLookupError::EmptyInternalPage);
                }
                if !range.is_valid()
                    || range.min != children.first().expect("children").range.min
                    || range.max != children.last().expect("children").range.max
                {
                    return Err(DirectoryLookupError::InvalidPageRange);
                }
                for (index, child) in children.iter().enumerate() {
                    if !child.range.is_valid() {
                        return Err(DirectoryLookupError::InvalidPageRange);
                    }
                    let actual = pages
                        .get(child.page_id)
                        .ok_or(DirectoryLookupError::InvalidPageReference)?
                        .range()
                        .ok_or(DirectoryLookupError::ChildRangeMismatch)?;
                    if actual != child.range {
                        return Err(DirectoryLookupError::ChildRangeMismatch);
                    }
                    if let Some(previous) = index.checked_sub(1).map(|i| children[i]) {
                        if previous.range.min > child.range.min {
                            return Err(DirectoryLookupError::UnorderedChildRange);
                        }
                        if previous.range.max >= child.range.min {
                            return Err(DirectoryLookupError::OverlappingChildRange);
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

fn div_ceil(value: u64, divisor: u64) -> Result<u64, DirectoryBuildError> {
    value
        .checked_add(divisor - 1)
        .ok_or(DirectoryBuildError::ArithmeticOverflow)
        .map(|adjusted| adjusted / divisor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locator(id: u64) -> ObjectLocator {
        ObjectLocator {
            object_id: id,
            kind: 1,
            offset: id * 100,
            stored_len: 10,
            logical_len: 10,
        }
    }

    #[test]
    fn lookup_reads_only_one_root_to_leaf_path() {
        let entries = (1..=100_000).rev().map(locator).collect();
        let directory = PagedDirectory::build(entries, 128, 64).expect("directory");
        directory.validate(10_000).expect("valid directory");

        let result = directory.lookup(77_777, 8).expect("lookup");
        assert_eq!(result.locator, Some(locator(77_777)));
        assert_eq!(result.pages_read, directory.stats().depth);
        assert!(result.pages_read <= 4);
    }

    #[test]
    fn absent_key_stops_on_search_path() {
        let entries = (0..1000).map(|index| locator(index * 2)).collect();
        let directory = PagedDirectory::build(entries, 32, 16).expect("directory");
        let result = directory.lookup(777, 10).expect("lookup");
        assert_eq!(result.locator, None);
        assert!(result.pages_read <= directory.stats().depth);
    }

    #[test]
    fn duplicate_keys_are_rejected_before_page_creation() {
        let error =
            PagedDirectory::build(vec![locator(7), locator(7)], 8, 4).expect_err("duplicate key");
        assert_eq!(error, DirectoryBuildError::DuplicateObjectId(7));
    }

    #[test]
    fn hundred_million_entry_shape_is_bounded_without_allocation() {
        let stats = PagedDirectory::estimate_shape(100_000_000, 192, 128).expect("shape");
        assert_eq!(stats.depth, 4);
        assert!(stats.pages < 530_000);
        assert!(stats.estimated_bytes(16 * 1024).expect("bytes") < 9 * 1024 * 1024 * 1024);
    }

    #[test]
    fn page_limit_fails_before_unbounded_traversal() {
        let entries = (1..=10_000).map(locator).collect();
        let directory = PagedDirectory::build(entries, 16, 4).expect("directory");
        let error = directory.lookup(9_999, 1).expect_err("page limit");
        assert_eq!(error, DirectoryLookupError::PageLimitExceeded);
    }

    #[test]
    fn forged_child_range_is_rejected() {
        let entries = (1..=100).map(locator).collect();
        let mut directory = PagedDirectory::build(entries, 10, 4).expect("directory");
        let Page::Internal { children, .. } = &mut directory.pages[directory.root] else {
            panic!("expected internal root");
        };
        children[0].range.max += 1;

        let error = directory.validate(100).expect_err("forged child range");
        assert!(matches!(
            error,
            DirectoryLookupError::ChildRangeMismatch | DirectoryLookupError::OverlappingChildRange
        ));
    }

    #[test]
    fn cycle_is_rejected() {
        let entries = (1..=100).map(locator).collect();
        let mut directory = PagedDirectory::build(entries, 10, 4).expect("directory");
        let root = directory.root;
        let root_range = directory.pages[root].range().expect("root range");
        let Page::Internal { children, .. } = &mut directory.pages[root] else {
            panic!("expected internal root");
        };
        children[0].page_id = root;
        children[0].range = root_range;

        let error = directory.validate(100).expect_err("cycle");
        assert!(matches!(
            error,
            DirectoryLookupError::PageCycle
                | DirectoryLookupError::ChildRangeMismatch
                | DirectoryLookupError::OverlappingChildRange
        ));
    }
}
