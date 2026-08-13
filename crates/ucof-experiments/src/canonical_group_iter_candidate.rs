#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalGroupIterError {
    Invalid,
    Overflow,
}

#[derive(Clone, Debug)]
pub struct CanonicalGroupSizesIter {
    capacity: usize,
    full_remaining: usize,
    tail_first: Option<usize>,
    tail_second: Option<usize>,
    remaining_groups: usize,
}

impl CanonicalGroupSizesIter {
    pub fn new(
        total: usize,
        capacity: usize,
        minimum: usize,
    ) -> Result<Self, CanonicalGroupIterError> {
        if total == 0 || capacity == 0 || minimum == 0 || minimum > capacity {
            return Err(CanonicalGroupIterError::Invalid);
        }
        let groups = total
            .checked_add(capacity - 1)
            .ok_or(CanonicalGroupIterError::Overflow)?
            / capacity;
        if groups == 1 {
            return Ok(Self {
                capacity,
                full_remaining: 0,
                tail_first: Some(total),
                tail_second: None,
                remaining_groups: 1,
            });
        }

        let full_groups = total / capacity;
        let remainder = total % capacity;
        let (full_remaining, tail_first, tail_second) = if remainder == 0 {
            (full_groups, None, None)
        } else if remainder >= minimum {
            (full_groups, Some(remainder), None)
        } else {
            let prefix_groups = full_groups
                .checked_sub(1)
                .ok_or(CanonicalGroupIterError::Invalid)?;
            let transfer = minimum - remainder;
            let penultimate = capacity
                .checked_sub(transfer)
                .ok_or(CanonicalGroupIterError::Invalid)?;
            if penultimate < minimum {
                return Err(CanonicalGroupIterError::Invalid);
            }
            (prefix_groups, Some(penultimate), Some(minimum))
        };

        Ok(Self {
            capacity,
            full_remaining,
            tail_first,
            tail_second,
            remaining_groups: groups,
        })
    }
}

impl Iterator for CanonicalGroupSizesIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let next = if self.full_remaining > 0 {
            self.full_remaining -= 1;
            Some(self.capacity)
        } else if self.tail_first.is_some() {
            self.tail_first.take()
        } else {
            self.tail_second.take()
        };
        if next.is_some() {
            self.remaining_groups -= 1;
        }
        next
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining_groups, Some(self.remaining_groups))
    }
}

impl ExactSizeIterator for CanonicalGroupSizesIter {}
