//! Persistent copy-on-write ordered-directory model.
//!
//! This model measures page reuse and split propagation before Candidate 1
//! defines byte-level page reuse. Page identifiers are in-memory indexes, not
//! wire offsets, and entry revisions stand in for changed physical locators.

use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CowEntry {
    pub key: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CowChild {
    pub minimum: u64,
    pub maximum: u64,
    pub page_id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CowPage {
    Leaf(Vec<CowEntry>),
    Internal(Vec<CowChild>),
}

impl CowPage {
    fn range(&self) -> Option<(u64, u64)> {
        match self {
            Self::Leaf(entries) => Some((entries.first()?.key, entries.last()?.key)),
            Self::Internal(children) => {
                Some((children.first()?.minimum, children.last()?.maximum))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CowDirectoryLimits {
    pub max_pages_visited: usize,
    pub max_new_pages: usize,
    pub max_depth: usize,
}

impl Default for CowDirectoryLimits {
    fn default() -> Self {
        Self {
            max_pages_visited: 1024,
            max_new_pages: 1024,
            max_depth: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CowDirectoryError {
    EmptyDirectory,
    InvalidCapacity,
    InvalidKey,
    DuplicateKey(u64),
    PageLimitExceeded,
    NewPageLimitExceeded,
    DepthLimitExceeded,
    InvalidPageReference,
    InvalidPageRange,
    UnorderedEntries,
    OverlappingRanges,
    Cycle,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CowUpdateReport {
    pub directory: CowDirectory,
    pub replaced_existing: bool,
    pub pages_visited: usize,
    pub new_pages: Vec<usize>,
    pub reused_pages: usize,
    pub old_page_count: usize,
    pub new_total_page_count: usize,
    pub old_depth: usize,
    pub new_depth: usize,
}

impl CowUpdateReport {
    #[must_use]
    pub fn copied_page_count(&self) -> usize {
        self.new_pages.len()
    }

    #[must_use]
    pub fn ideal_copy_bytes(&self, page_bytes: usize) -> Option<u64> {
        u64::try_from(self.new_pages.len())
            .ok()?
            .checked_mul(u64::try_from(page_bytes).ok()?)
    }

    #[must_use]
    pub fn full_rebuild_bytes(&self, page_bytes: usize) -> Option<u64> {
        u64::try_from(self.directory.reachable_page_count().ok()?)
            .ok()?
            .checked_mul(u64::try_from(page_bytes).ok()?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CowDirectory {
    pages: Vec<CowPage>,
    root: usize,
    leaf_capacity: usize,
    internal_fanout: usize,
}

impl CowDirectory {
    pub fn build(
        entries: Vec<CowEntry>,
        leaf_capacity: usize,
        internal_fanout: usize,
    ) -> Result<Self, CowDirectoryError> {
        validate_capacities(leaf_capacity, internal_fanout)?;
        if entries.is_empty() {
            return Err(CowDirectoryError::EmptyDirectory);
        }
        let mut entries = entries;
        entries.sort_by_key(|entry| entry.key);
        validate_entries(&entries)?;

        let mut pages = Vec::new();
        let mut level = Vec::new();
        for chunk in entries.chunks(leaf_capacity) {
            let page_id = pages.len();
            pages.push(CowPage::Leaf(chunk.to_vec()));
            let (minimum, maximum) = pages[page_id]
                .range()
                .ok_or(CowDirectoryError::InvalidPageRange)?;
            level.push(CowChild {
                minimum,
                maximum,
                page_id,
            });
        }
        while level.len() > 1 {
            let mut next = Vec::new();
            for chunk in level.chunks(internal_fanout) {
                let children = chunk.to_vec();
                let page_id = pages.len();
                pages.push(CowPage::Internal(children));
                let (minimum, maximum) = pages[page_id]
                    .range()
                    .ok_or(CowDirectoryError::InvalidPageRange)?;
                next.push(CowChild {
                    minimum,
                    maximum,
                    page_id,
                });
            }
            level = next;
        }
        let root = level
            .first()
            .ok_or(CowDirectoryError::EmptyDirectory)?
            .page_id;
        let directory = Self {
            pages,
            root,
            leaf_capacity,
            internal_fanout,
        };
        directory.validate(CowDirectoryLimits {
            max_pages_visited: directory.pages.len(),
            max_new_pages: directory.pages.len(),
            max_depth: 128,
        })?;
        Ok(directory)
    }

    #[must_use]
    pub const fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn depth(&self) -> Result<usize, CowDirectoryError> {
        let mut depth = 0_usize;
        let mut page_id = self.root;
        loop {
            depth = depth
                .checked_add(1)
                .ok_or(CowDirectoryError::ArithmeticOverflow)?;
            match self
                .pages
                .get(page_id)
                .ok_or(CowDirectoryError::InvalidPageReference)?
            {
                CowPage::Leaf(_) => return Ok(depth),
                CowPage::Internal(children) => {
                    page_id = children
                        .first()
                        .ok_or(CowDirectoryError::InvalidPageRange)?
                        .page_id;
                }
            }
        }
    }

    pub fn reachable_page_count(&self) -> Result<usize, CowDirectoryError> {
        Ok(self.reachable_pages()?.len())
    }

    pub fn lookup(&self, key: u64) -> Result<Option<CowEntry>, CowDirectoryError> {
        if key == 0 {
            return Err(CowDirectoryError::InvalidKey);
        }
        let mut page_id = self.root;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(page_id) {
                return Err(CowDirectoryError::Cycle);
            }
            match self
                .pages
                .get(page_id)
                .ok_or(CowDirectoryError::InvalidPageReference)?
            {
                CowPage::Leaf(entries) => {
                    return match entries.binary_search_by_key(&key, |entry| entry.key) {
                        Ok(index) => Ok(Some(entries[index])),
                        Err(_) => Ok(None),
                    };
                }
                CowPage::Internal(children) => {
                    let Some(child) = children
                        .iter()
                        .find(|child| child.minimum <= key && key <= child.maximum)
                    else {
                        return Ok(None);
                    };
                    page_id = child.page_id;
                }
            }
        }
    }

    pub fn upsert(
        &self,
        entry: CowEntry,
        limits: CowDirectoryLimits,
    ) -> Result<CowUpdateReport, CowDirectoryError> {
        if entry.key == 0 {
            return Err(CowDirectoryError::InvalidKey);
        }
        let old_reachable = self.reachable_pages()?;
        let old_depth = self.depth()?;
        let mut pages = self.pages.clone();
        let mut context = UpdateContext {
            pages: &mut pages,
            limits,
            pages_visited: 0,
            new_pages: Vec::new(),
            replaced_existing: false,
        };
        let result = context.upsert_page(self.root, entry, 1)?;
        let root = if let Some(right) = result.right {
            let children = vec![result.left, right];
            context.push_page(CowPage::Internal(children))?
        } else {
            result.left.page_id
        };
        let directory = Self {
            pages,
            root,
            leaf_capacity: self.leaf_capacity,
            internal_fanout: self.internal_fanout,
        };
        directory.validate(limits)?;
        let new_reachable = directory.reachable_pages()?;
        let reused_pages = old_reachable.intersection(&new_reachable).count();
        let new_depth = directory.depth()?;
        let new_total_page_count = directory.pages.len();
        Ok(CowUpdateReport {
            directory,
            replaced_existing: context.replaced_existing,
            pages_visited: context.pages_visited,
            new_pages: context.new_pages,
            reused_pages,
            old_page_count: old_reachable.len(),
            new_total_page_count,
            old_depth,
            new_depth,
        })
    }

    pub fn validate(&self, limits: CowDirectoryLimits) -> Result<(), CowDirectoryError> {
        validate_capacities(self.leaf_capacity, self.internal_fanout)?;
        let mut stack = vec![(self.root, 1_usize)];
        let mut seen = HashSet::new();
        while let Some((page_id, depth)) = stack.pop() {
            if depth > limits.max_depth {
                return Err(CowDirectoryError::DepthLimitExceeded);
            }
            if !seen.insert(page_id) {
                return Err(CowDirectoryError::Cycle);
            }
            if seen.len() > limits.max_pages_visited {
                return Err(CowDirectoryError::PageLimitExceeded);
            }
            let page = self
                .pages
                .get(page_id)
                .ok_or(CowDirectoryError::InvalidPageReference)?;
            match page {
                CowPage::Leaf(entries) => {
                    if entries.is_empty() || entries.len() > self.leaf_capacity {
                        return Err(CowDirectoryError::InvalidPageRange);
                    }
                    validate_entries(entries)?;
                }
                CowPage::Internal(children) => {
                    if children.is_empty() || children.len() > self.internal_fanout {
                        return Err(CowDirectoryError::InvalidPageRange);
                    }
                    let mut previous = None;
                    for child in children {
                        if child.minimum == 0 || child.minimum > child.maximum {
                            return Err(CowDirectoryError::InvalidPageRange);
                        }
                        if previous.is_some_and(|maximum| maximum >= child.minimum) {
                            return Err(CowDirectoryError::OverlappingRanges);
                        }
                        let actual = self
                            .pages
                            .get(child.page_id)
                            .ok_or(CowDirectoryError::InvalidPageReference)?
                            .range()
                            .ok_or(CowDirectoryError::InvalidPageRange)?;
                        if actual != (child.minimum, child.maximum) {
                            return Err(CowDirectoryError::InvalidPageRange);
                        }
                        stack.push((child.page_id, depth + 1));
                        previous = Some(child.maximum);
                    }
                }
            }
        }
        Ok(())
    }

    fn reachable_pages(&self) -> Result<BTreeSet<usize>, CowDirectoryError> {
        let mut reachable = BTreeSet::new();
        let mut stack = vec![self.root];
        while let Some(page_id) = stack.pop() {
            if !reachable.insert(page_id) {
                return Err(CowDirectoryError::Cycle);
            }
            match self
                .pages
                .get(page_id)
                .ok_or(CowDirectoryError::InvalidPageReference)?
            {
                CowPage::Leaf(_) => {}
                CowPage::Internal(children) => {
                    stack.extend(children.iter().map(|child| child.page_id));
                }
            }
        }
        Ok(reachable)
    }
}

struct UpdateContext<'a> {
    pages: &'a mut Vec<CowPage>,
    limits: CowDirectoryLimits,
    pages_visited: usize,
    new_pages: Vec<usize>,
    replaced_existing: bool,
}

struct PageUpdate {
    left: CowChild,
    right: Option<CowChild>,
}

impl UpdateContext<'_> {
    fn upsert_page(
        &mut self,
        page_id: usize,
        entry: CowEntry,
        depth: usize,
    ) -> Result<PageUpdate, CowDirectoryError> {
        if depth > self.limits.max_depth {
            return Err(CowDirectoryError::DepthLimitExceeded);
        }
        self.pages_visited = self
            .pages_visited
            .checked_add(1)
            .ok_or(CowDirectoryError::ArithmeticOverflow)?;
        if self.pages_visited > self.limits.max_pages_visited {
            return Err(CowDirectoryError::PageLimitExceeded);
        }
        let page = self
            .pages
            .get(page_id)
            .ok_or(CowDirectoryError::InvalidPageReference)?
            .clone();
        match page {
            CowPage::Leaf(mut entries) => {
                match entries.binary_search_by_key(&entry.key, |current| current.key) {
                    Ok(index) => {
                        entries[index] = entry;
                        self.replaced_existing = true;
                    }
                    Err(index) => entries.insert(index, entry),
                }
                if entries.len() <= self.leaf_capacity()? {
                    let new_id = self.push_page(CowPage::Leaf(entries))?;
                    return Ok(PageUpdate {
                        left: child_for(self.pages, new_id)?,
                        right: None,
                    });
                }
                let split = entries.len() / 2;
                let right_entries = entries.split_off(split);
                let left_id = self.push_page(CowPage::Leaf(entries))?;
                let right_id = self.push_page(CowPage::Leaf(right_entries))?;
                Ok(PageUpdate {
                    left: child_for(self.pages, left_id)?,
                    right: Some(child_for(self.pages, right_id)?),
                })
            }
            CowPage::Internal(mut children) => {
                let index = select_child_index(&children, entry.key);
                let selected = children
                    .get(index)
                    .ok_or(CowDirectoryError::InvalidPageReference)?
                    .page_id;
                let updated = self.upsert_page(selected, entry, depth + 1)?;
                children[index] = updated.left;
                if let Some(right) = updated.right {
                    children.insert(index + 1, right);
                }
                if children.len() <= self.internal_fanout()? {
                    let new_id = self.push_page(CowPage::Internal(children))?;
                    return Ok(PageUpdate {
                        left: child_for(self.pages, new_id)?,
                        right: None,
                    });
                }
                let split = children.len() / 2;
                let right_children = children.split_off(split);
                let left_id = self.push_page(CowPage::Internal(children))?;
                let right_id = self.push_page(CowPage::Internal(right_children))?;
                Ok(PageUpdate {
                    left: child_for(self.pages, left_id)?,
                    right: Some(child_for(self.pages, right_id)?),
                })
            }
        }
    }

    fn leaf_capacity(&self) -> Result<usize, CowDirectoryError> {
        self.pages
            .iter()
            .find_map(|page| match page {
                CowPage::Leaf(entries) => Some(entries.capacity().max(entries.len())),
                CowPage::Internal(_) => None,
            })
            .ok_or(CowDirectoryError::InvalidCapacity)
    }

    fn internal_fanout(&self) -> Result<usize, CowDirectoryError> {
        self.pages
            .iter()
            .find_map(|page| match page {
                CowPage::Internal(children) => Some(children.capacity().max(children.len())),
                CowPage::Leaf(_) => None,
            })
            .unwrap_or(usize::MAX)
            .checked_add(0)
            .ok_or(CowDirectoryError::ArithmeticOverflow)
    }

    fn push_page(&mut self, page: CowPage) -> Result<usize, CowDirectoryError> {
        if self.new_pages.len() >= self.limits.max_new_pages {
            return Err(CowDirectoryError::NewPageLimitExceeded);
        }
        let page_id = self.pages.len();
        self.pages.push(page);
        self.new_pages.push(page_id);
        Ok(page_id)
    }
}

fn select_child_index(children: &[CowChild], key: u64) -> usize {
    children
        .iter()
        .position(|child| key <= child.maximum)
        .unwrap_or_else(|| children.len().saturating_sub(1))
}

fn child_for(pages: &[CowPage], page_id: usize) -> Result<CowChild, CowDirectoryError> {
    let (minimum, maximum) = pages
        .get(page_id)
        .ok_or(CowDirectoryError::InvalidPageReference)?
        .range()
        .ok_or(CowDirectoryError::InvalidPageRange)?;
    Ok(CowChild {
        minimum,
        maximum,
        page_id,
    })
}

fn validate_capacities(
    leaf_capacity: usize,
    internal_fanout: usize,
) -> Result<(), CowDirectoryError> {
    if leaf_capacity < 2 || internal_fanout < 2 {
        Err(CowDirectoryError::InvalidCapacity)
    } else {
        Ok(())
    }
}

fn validate_entries(entries: &[CowEntry]) -> Result<(), CowDirectoryError> {
    let mut previous = None;
    for entry in entries {
        if entry.key == 0 {
            return Err(CowDirectoryError::InvalidKey);
        }
        if previous.is_some_and(|key| key >= entry.key) {
            return Err(if previous == Some(entry.key) {
                CowDirectoryError::DuplicateKey(entry.key)
            } else {
                CowDirectoryError::UnorderedEntries
            });
        }
        previous = Some(entry.key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(count: u64) -> Vec<CowEntry> {
        (1..=count)
            .map(|key| CowEntry { key, revision: 0 })
            .collect()
    }

    #[test]
    fn replacement_copies_one_page_per_level_and_reuses_the_rest() {
        let directory = CowDirectory::build(entries(100_000), 185, 255).expect("directory");
        let depth = directory.depth().expect("depth");
        let old_pages = directory.reachable_page_count().expect("old pages");
        let report = directory
            .upsert(
                CowEntry {
                    key: 77_777,
                    revision: 1,
                },
                CowDirectoryLimits::default(),
            )
            .expect("update");
        assert!(report.replaced_existing);
        assert_eq!(report.pages_visited, depth);
        assert_eq!(report.copied_page_count(), depth);
        assert_eq!(report.reused_pages, old_pages - depth);
        assert_eq!(
            report.directory.lookup(77_777).expect("lookup"),
            Some(CowEntry {
                key: 77_777,
                revision: 1
            })
        );
    }

    #[test]
    fn insertion_split_propagates_without_rewriting_unrelated_pages() {
        let directory = CowDirectory::build(entries(185 * 255), 185, 255).expect("directory");
        let old_pages = directory.reachable_page_count().expect("old pages");
        let report = directory
            .upsert(
                CowEntry {
                    key: 185 * 255 + 1,
                    revision: 0,
                },
                CowDirectoryLimits::default(),
            )
            .expect("insert");
        assert!(!report.replaced_existing);
        assert!(report.copied_page_count() <= 2 * report.old_depth + 1);
        assert!(report.reused_pages >= old_pages.saturating_sub(report.old_depth));
        assert_eq!(
            report.directory.lookup(185 * 255 + 1).expect("lookup"),
            Some(CowEntry {
                key: 185 * 255 + 1,
                revision: 0
            })
        );
    }

    #[test]
    fn copy_bytes_are_far_below_full_rebuild_at_scale() {
        let directory = CowDirectory::build(entries(1_000_000), 185, 255).expect("directory");
        let report = directory
            .upsert(
                CowEntry {
                    key: 500_000,
                    revision: 1,
                },
                CowDirectoryLimits::default(),
            )
            .expect("update");
        let copied = report.ideal_copy_bytes(16 * 1024).expect("copied bytes");
        let rebuilt = report
            .full_rebuild_bytes(16 * 1024)
            .expect("rebuild bytes");
        assert!(copied <= 64 * 1024);
        assert!(rebuilt > 80 * 1024 * 1024);
        assert!(rebuilt / copied > 1_000);
    }

    #[test]
    fn limits_fail_before_unbounded_page_creation() {
        let directory = CowDirectory::build(entries(1_000), 8, 4).expect("directory");
        assert_eq!(
            directory.upsert(
                CowEntry {
                    key: 500,
                    revision: 1,
                },
                CowDirectoryLimits {
                    max_new_pages: 1,
                    ..CowDirectoryLimits::default()
                }
            ),
            Err(CowDirectoryError::NewPageLimitExceeded)
        );
    }
}
