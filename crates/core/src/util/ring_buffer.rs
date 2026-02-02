#[derive(Clone, Debug)]
pub struct RingBuffer<T> {
    buf: Vec<Option<T>>,
    head: usize,
    len: usize,
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        let mut buf = Vec::with_capacity(capacity);
        buf.resize_with(capacity, || None);
        Self {
            buf,
            head: 0,
            len: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push(&mut self, value: T) -> Option<T> {
        let cap = self.capacity();
        let idx = (self.head + self.len) % cap;

        if self.len < cap {
            self.buf[idx] = Some(value);
            self.len += 1;
            None
        } else {
            let overwritten = self.buf[self.head].take();
            self.buf[self.head] = Some(value);
            self.head = (self.head + 1) % cap;
            overwritten
        }
    }

    pub fn get(&self, index_from_oldest: usize) -> Option<&T> {
        if index_from_oldest >= self.len {
            return None;
        }
        let cap = self.capacity();
        let idx = (self.head + index_from_oldest) % cap;
        self.buf[idx].as_ref()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        (0..self.len).filter_map(move |i| self.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_overwrites_oldest() {
        let mut rb = RingBuffer::new(3);
        assert!(rb.is_empty());

        assert_eq!(rb.push(1), None);
        assert_eq!(rb.push(2), None);
        assert_eq!(rb.push(3), None);
        assert_eq!(rb.len(), 3);
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);

        let overwritten = rb.push(4);
        assert_eq!(overwritten, Some(1));
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    }
}
