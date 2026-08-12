//! Persistent copy-on-write ordered-directory model.
//!
//! This model measures page reuse and split propagation before Candidate 1
//! defines byte-level page reuse. Page identifiers are in-memory indexes, not
//! wire offsets, and entry revisions stand in for changed physical locators.

use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub key: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Child {
    minimum: u64,
    maximum: u64,
    page_id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Page {
    Leaf(Vec<Entry>),
    Internal(Vec<Child>),
}

impl Page {
    fn range(&self) -> Option<(u64, u64)> {
        match self {
            Self::Leaf(entries) => Some((entries.first()?.key, entries.last()?.key)),
            Self::Internal(children) => Some((children.first()?.minimum, children.last()?.maximum)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum pages copied or inspected along one update path.
    pub max_pages_visited: usize,
    /// Maximum new persistent pages produced by one update.
    pub max_new_pages: usize,
    /// Maximum root-to-leaf depth.
    pub max_depth: usize,
    /// Maximum unique pages inspected by whole-directory validation.
    pub max_validation_pages: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pages_visited: 64,
            max_new_pages: 128,
            max_depth: 64,
            max_validation_pages: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
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
pub struct Directory {
    pages: Vec<Page>,
    root: usize,
    leaf_capacity: usize,
    internal_fanout: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateReport {
    pub directory: Directory,
    pub replaced_existing: bool,
    pub pages_visited: usize,
    pub new_pages: Vec<usize>,
    pub reused_pages: usize,
    pub old_reachable_pages: usize,
    pub new_reachable_pages: usize,
    pub old_depth: usize,
    pub new_depth: usize,
}

impl UpdateReport {
    #[must_use]
    pub fn copied_page_count(&self) -> usize {
        self.new_pages.len()
    }

    #[must_use]
    pub fn copied_bytes(&self, page_bytes: usize) -> Option<u64> {
        u64::try_from(self.new_pages.len())
            .ok()?
            .checked_mul(u64::try_from(page_bytes).ok()?)
    }

    #[must_use]
    pub fn full_rebuild_bytes(&self, page_bytes: usize) -> Option<u64> {
        u64::try_from(self.new_reachable_pages)
            .ok()?
            .checked_mul(u64::try_from(page_bytes).ok()?)
    }
}

impl Directory {
    pub fn build(
        mut entries: Vec<Entry>,
        leaf_capacity: usize,
        internal_fanout: usize,
    ) -> Result<Self, Error> {
        validate_capacities(leaf_capacity, internal_fanout)?;
        if entries.is_empty() {
            return Err(Error::EmptyDirectory);
        }
        entries.sort_by_key(|entry| entry.key);
        validate_entries(&entries)?;

        let mut pages = Vec::new();
        let mut level = Vec::new();
        for chunk in entries.chunks(leaf_capacity) {
            let page_id = pages.len();
            pages.push(Page::Leaf(chunk.to_vec()));
            level.push(child_for(&pages, page_id)?);
        }
        while level.len() > 1 {
            let mut next = Vec::new();
            for chunk in level.chunks(internal_fanout) {
                let page_id = pages.len();
                pages.push(Page::Internal(chunk.to_vec()));
                next.push(child_for(&pages, page_id)?);
            }
            level = next;
        }

        let root = level.first().ok_or(Error::EmptyDirectory)?.page_id;
        let directory = Self {
            pages,
            root,
            leaf_capacity,
            internal_fanout,
        };
        directory.validate(Limits {
            max_pages_visited: directory.pages.len(),
            max_new_pages: directory.pages.len(),
            max_depth: 128,
            max_validation_pages: directory.pages.len(),
        })?;
        Ok(directory)
    }

    pub fn lookup(&self, key: u64) -> Result<Option<Entry>, Error> {
        if key == 0 {
            return Err(Error::InvalidKey);
        }
        let mut page_id = self.root;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(page_id) {
                return Err(Error::Cycle);
            }
            if seen.len() > self.pages.len() {
                return Err(Error::PageLimitExceeded);
            }
            match self.pages.get(page_id).ok_or(Error::InvalidPageReference)? {
                Page::Leaf(entries) => {
                    return match entries.binary_search_by_key(&key, |entry| entry.key) {
                        Ok(index) => Ok(Some(entries[index])),
                        Err(_) => Ok(None),
                    };
                }
                Page::Internal(children) => {
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

    pub fn depth(&self) -> Result<usize, Error> {
        self.depth_with_limit(self.pages.len())
    }

    pub fn reachable_page_count(&self) -> Result<usize, Error> {
        Ok(self.reachable_pages(self.pages.len())?.len())
    }

    pub fn upsert(&self, entry: Entry, limits: Limits) -> Result<UpdateReport, Error> {
        if entry.key == 0 {
            return Err(Error::InvalidKey);
        }

        let old_reachable = self.reachable_pages(limits.max_validation_pages)?;
        let old_depth = self.depth_with_limit(limits.max_depth)?;
        let mut pages = self.pages.clone();
        let mut context = Context {
            pages: &mut pages,
            leaf_capacity: self.leaf_capacity,
            internal_fanout: self.internal_fanout,
            limits,
            pages_visited: 0,
            new_pages: Vec::new(),
            replaced_existing: false,
        };
        let update = context.upsert_page(self.root, entry, 1)?;
        let root = if let Some(right) = update.right {
            context.push(Page::Internal(vec![update.left, right]))?
        } else {
            update.left.page_id
        };
        let new_pages = context.new_pages.clone();
        let pages_visited = context.pages_visited;
        let replaced_existing = context.replaced_existing;
        drop(context);

        let directory = Self {
            pages,
            root,
            leaf_capacity: self.leaf_capacity,
            internal_fanout: self.internal_fanout,
        };
        directory.validate(limits)?;
        let new_reachable = directory.reachable_pages(limits.max_validation_pages)?;
        let reused_pages = old_reachable.intersection(&new_reachable).count();
        let new_depth = directory.depth_with_limit(limits.max_depth)?;

        Ok(UpdateReport {
            directory,
            replaced_existing,
            pages_visited,
            new_pages,
            reused_pages,
            old_reachable_pages: old_reachable.len(),
            new_reachable_pages: new_reachable.len(),
            old_depth,
            new_depth,
        })
    }

    pub fn validate(&self, limits: Limits) -> Result<(), Error> {
        validate_capacities(self.leaf_capacity, self.internal_fanout)?;
        let mut stack = vec![(self.root, 1_usize)];
        let mut seen = HashSet::new();
        while let Some((page_id, depth)) = stack.pop() {
            if depth > limits.max_depth {
                return Err(Error::DepthLimitExceeded);
            }
            if !seen.insert(page_id) {
                return Err(Error::Cycle);
            }
            if seen.len() > limits.max_validation_pages {
                return Err(Error::PageLimitExceeded);
            }
            match self.pages.get(page_id).ok_or(Error::InvalidPageReference)? {
                Page::Leaf(entries) => {
                    if entries.is_empty() || entries.len() > self.leaf_capacity {
                        return Err(Error::InvalidPageRange);
                    }
                    validate_entries(entries)?;
                }
                Page::Internal(children) => {
                    if children.is_empty() || children.len() > self.internal_fanout {
                        return Err(Error::InvalidPageRange);
                    }
                    let mut previous = None;
                    for child in children {
                        if child.minimum == 0 || child.minimum > child.maximum {
                            return Err(Error::InvalidPageRange);
                        }
                        if previous.is_some_and(|maximum| maximum >= child.minimum) {
                            return Err(Error::OverlappingRanges);
                        }
                        let actual = self
                            .pages
                            .get(child.page_id)
                            .ok_or(Error::InvalidPageReference)?
                            .range()
                            .ok_or(Error::InvalidPageRange)?;
                        if actual != (child.minimum, child.maximum) {
                            return Err(Error::InvalidPageRange);
                        }
                        stack.push((child.page_id, depth + 1));
                        previous = Some(child.maximum);
                    }
                }
            }
        }
        Ok(())
    }

    fn depth_with_limit(&self, max_depth: usize) -> Result<usize, Error> {
        let mut depth = 0_usize;
        let mut page_id = self.root;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(page_id) {
                return Err(Error::Cycle);
            }
            depth = depth.checked_add(1).ok_or(Error::ArithmeticOverflow)?;
            if depth > max_depth {
                return Err(Error::DepthLimitExceeded);
            }
            match self.pages.get(page_id).ok_or(Error::InvalidPageReference)? {
                Page::Leaf(_) => return Ok(depth),
                Page::Internal(children) => {
                    page_id = children.first().ok_or(Error::InvalidPageRange)?.page_id;
                }
            }
        }
    }

    fn reachable_pages(&self, max_pages: usize) -> Result<BTreeSet<usize>, Error> {
        let mut reachable = BTreeSet::new();
        let mut stack = vec![self.root];
        while let Some(page_id) = stack.pop() {
            if !reachable.insert(page_id) {
                return Err(Error::Cycle);
            }
            if reachable.len() > max_pages {
                return Err(Error::PageLimitExceeded);
            }
            match self.pages.get(page_id).ok_or(Error::InvalidPageReference)? {
                Page::Leaf(_) => {}
                Page::Internal(children) => {
                    stack.extend(children.iter().map(|child| child.page_id));
                }
            }
        }
        Ok(reachable)
    }
}

struct Context<'a> {
    pages: &'a mut Vec<Page>,
    leaf_capacity: usize,
    internal_fanout: usize,
    limits: Limits,
    pages_visited: usize,
    new_pages: Vec<usize>,
    replaced_existing: bool,
}

struct PageUpdate {
    left: Child,
    right: Option<Child>,
}

impl Context<'_> {
    fn upsert_page(
        &mut self,
        page_id: usize,
        entry: Entry,
        depth: usize,
    ) -> Result<PageUpdate, Error> {
        if depth > self.limits.max_depth {
            return Err(Error::DepthLimitExceeded);
        }
        self.pages_visited = self
            .pages_visited
            .checked_add(1)
            .ok_or(Error::ArithmeticOverflow)?;
        if self.pages_visited > self.limits.max_pages_visited {
            return Err(Error::PageLimitExceeded);
        }

        let page = self
            .pages
            .get(page_id)
            .ok_or(Error::InvalidPageReference)?
            .clone();
        match page {
            Page::Leaf(mut entries) => {
                match entries.binary_search_by_key(&entry.key, |current| current.key) {
                    Ok(index) => {
                        entries[index] = entry;
                        self.replaced_existing = true;
                    }
                    Err(index) => entries.insert(index, entry),
                }
                if entries.len() <= self.leaf_capacity {
                    let left_id = self.push(Page::Leaf(entries))?;
                    return Ok(PageUpdate {
                        left: child_for(self.pages, left_id)?,
                        right: None,
                    });
                }

                let split = entries.len() / 2;
                let right_entries = entries.split_off(split);
                let left_id = self.push(Page::Leaf(entries))?;
                let right_id = self.push(Page::Leaf(right_entries))?;
                Ok(PageUpdate {
                    left: child_for(self.pages, left_id)?,
                    right: Some(child_for(self.pages, right_id)?),
                })
            }
            Page::Internal(mut children) => {
                let index = select_child_index(&children, entry.key);
                let selected = children
                    .get(index)
                    .ok_or(Error::InvalidPageReference)?
                    .page_id;
                let updated = self.upsert_page(selected, entry, depth + 1)?;
                children[index] = updated.left;
                if let Some(right) = updated.right {
                    children.insert(index + 1, right);
                }
                if children.len() <= self.internal_fanout {
                    let left_id = self.push(Page::Internal(children))?;
                    return Ok(PageUpdate {
                        left: child_for(self.pages, left_id)?,
                        right: None,
                    });
                }

                let split = children.len() / 2;
                let right_children = children.split_off(split);
                let left_id = self.push(Page::Internal(children))?;
                let right_id = self.push(Page::Internal(right_children))?;
                Ok(PageUpdate {
                    left: child_for(self.pages, left_id)?,
                    right: Some(child_for(self.pages, right_id)?),
                })
            }
        }
    }

    fn push(&mut self, page: Page) -> Result<usize, Error> {
        if self.new_pages.len() >= self.limits.max_new_pages {
            return Err(Error::NewPageLimitExceeded);
        }
        let page_id = self.pages.len();
        self.pages.push(page);
        self.new_pages.push(page_id);
        Ok(page_id)
    }
}

fn select_child_index(children: &[Child], key: u64) -> usize {
    children
        .iter()
        .position(|child| key <= child.maximum)
        .unwrap_or_else(|| children.len().saturating_sub(1))
}

fn child_for(pages: &[Page], page_id: usize) -> Result<Child, Error> {
    let (minimum, maximum) = pages
        .get(page_id)
        .ok_or(Error::InvalidPageReference)?
        .range()
        .ok_or(Error::InvalidPageRange)?;
    Ok(Child {
        minimum,
        maximum,
        page_id,
    })
}

fn validate_capacities(leaf_capacity: usize, internal_fanout: usize) -> Result<(), Error> {
    if leaf_capacity < 2 || internal_fanout < 2 {
        Err(Error::InvalidCapacity)
    } else {
        Ok(())
    }
}

fn validate_entries(entries: &[Entry]) -> Result<(), Error> {
    let mut previous = None;
    for entry in entries {
        if entry.key == 0 {
            return Err(Error::InvalidKey);
        }
        if previous.is_some_and(|key| key >= entry.key) {
            return Err(if previous == Some(entry.key) {
                Error::DuplicateKey(entry.key)
            } else {
                Error::UnorderedEntries
            });
        }
        previous = Some(entry.key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(count: u64) -> Vec<Entry> {
        (1..=count).map(|key| Entry { key, revision: 0 }).collect()
    }

    #[test]
    fn replacement_copies_one_page_per_level_and_reuses_the_rest() {
        let directory = Directory::build(entries(100_000), 185, 255).expect("directory");
        let depth = directory.depth().expect("depth");
        let old_pages = directory.reachable_page_count().expect("old pages");
        let report = directory
            .upsert(
                Entry {
                    key: 77_777,
                    revision: 1,
                },
                Limits::default(),
            )
            .expect("update");
        assert!(report.replaced_existing);
        assert_eq!(report.pages_visited, depth);
        assert_eq!(report.copied_page_count(), depth);
        assert_eq!(report.reused_pages, old_pages - depth);
        assert_eq!(
            report.directory.lookup(77_777).expect("lookup"),
            Some(Entry {
                key: 77_777,
                revision: 1
            })
        );
    }

    #[test]
    fn insertion_split_propagates_without_rewriting_unrelated_pages() {
        let count = 185_u64 * 255;
        let directory = Directory::build(entries(count), 185, 255).expect("directory");
        let old_pages = directory.reachable_page_count().expect("old pages");
        let report = directory
            .upsert(
                Entry {
                    key: count + 1,
                    revision: 0,
                },
                Limits::default(),
            )
            .expect("insert");
        assert!(!report.replaced_existing);
        assert!(report.copied_page_count() <= 2 * report.old_depth + 1);
        assert!(report.reused_pages >= old_pages.saturating_sub(report.old_depth));
        assert_eq!(
            report.directory.lookup(count + 1).expect("lookup"),
            Some(Entry {
                key: count + 1,
                revision: 0
            })
        );
    }

    #[test]
    fn copy_bytes_are_far_below_full_rebuild_at_scale() {
        let directory = Directory::build(entries(1_000_000), 185, 255).expect("directory");
        let report = directory
            .upsert(
                Entry {
                    key: 500_000,
                    revision: 1,
                },
                Limits::default(),
            )
            .expect("update");
        let copied = report.copied_bytes(16 * 1024).expect("copied bytes");
        let rebuilt = report.full_rebuild_bytes(16 * 1024).expect("rebuild bytes");
        assert!(copied <= 64 * 1024);
        assert!(rebuilt > 80 * 1024 * 1024);
        assert!(rebuilt / copied > 1_000);
    }

    #[test]
    fn path_and_validation_budgets_are_independent() {
        let directory = Directory::build(entries(100_000), 185, 255).expect("directory");
        let entry = Entry {
            key: 50_000,
            revision: 1,
        };
        assert_eq!(
            directory.upsert(
                entry,
                Limits {
                    max_pages_visited: 2,
                    ..Limits::default()
                }
            ),
            Err(Error::PageLimitExceeded)
        );
        assert_eq!(
            directory.upsert(
                entry,
                Limits {
                    max_validation_pages: 2,
                    ..Limits::default()
                }
            ),
            Err(Error::PageLimitExceeded)
        );
    }

    #[test]
    fn limits_fail_before_unbounded_page_creation() {
        let directory = Directory::build(entries(1_000), 8, 4).expect("directory");
        assert_eq!(
            directory.upsert(
                Entry {
                    key: 500,
                    revision: 1,
                },
                Limits {
                    max_new_pages: 1,
                    ..Limits::default()
                }
            ),
            Err(Error::NewPageLimitExceeded)
        );
    }
}
