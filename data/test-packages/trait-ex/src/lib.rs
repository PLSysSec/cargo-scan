/*
    Example of implementing methods for a dyn trait object

    Adapted from the object crate in write/util.rs
*/

/// Trait for writable buffer.
pub trait WritableBuffer {
    /// Returns position/offset for data to be written at.
    fn len(&self) -> usize;

    /// Buffer is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reserves specified number of bytes in the buffer.
    fn reserve(&mut self, size: usize);

    /// Writes the specified slice of bytes at the end of the buffer.
    fn write_bytes(&mut self, val: &[u8]);
}

impl dyn WritableBuffer {
    /// Writes the specified value at the end of the buffer.
    pub fn write(&mut self, s: &str) {
        self.write_bytes(s.as_bytes())
    }

    /// Writes the specified `Pod` slice at the end of the buffer.
    pub fn write_slice(&mut self, val: &[u8]) {
        self.write_bytes(val)
    }
}
