//! Fixed-capacity ring buffer.

/// Ring buffer keeping the last `capacity` samples.
#[derive(Debug, Clone)]
pub struct Ring {
    capacity: usize,
    data: std::collections::VecDeque<f32>,
}

impl Ring {
    /// Create an empty ring.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            data: std::collections::VecDeque::with_capacity(capacity.max(1)),
        }
    }

    /// Push a sample, evicting the oldest when full.
    pub fn push(&mut self, v: f32) {
        if self.data.len() == self.capacity {
            self.data.pop_front();
        }
        self.data.push_back(v);
    }

    /// Number of stored samples.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Most recent sample.
    pub fn last(&self) -> Option<f32> {
        self.data.back().copied()
    }

    /// Samples of the selected history window (`t` cycles 60s / 5m / 1h),
    /// oldest to newest.
    pub fn values(&self) -> Vec<f32> {
        let n = super::store::window_len();
        let skip = self.data.len().saturating_sub(n);
        self.data.iter().skip(skip).copied().collect()
    }

    /// Every stored sample, oldest to newest.
    pub fn all(&self) -> Vec<f32> {
        self.data.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Ring;

    #[test]
    fn evicts_oldest() {
        let mut r = Ring::new(3);
        for i in 0..5 {
            r.push(i as f32);
        }
        assert_eq!(r.all(), vec![2.0, 3.0, 4.0]);
    }
}
