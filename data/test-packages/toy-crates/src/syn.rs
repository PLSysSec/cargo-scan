/*
    Some unsafe fns from syn (in syn::buffer::Cursor, src/buffer.rs)
    that are causing inconsistent behavior.
*/

use std::marker::PhantomData;

pub struct Cursor<'a> {
    ptr: *const usize,
    marker: PhantomData<&'a usize>,
}

impl<'a> Cursor<'a> {
    /// Creates a cursor referencing a static empty TokenStream.
    pub fn empty() -> Self {
        // It's safe in this situation for us to put an `Entry` object in global
        // storage, despite it not actually being safe to send across threads
        // (`Ident` is a reference into a thread-local table). This is because
        // this entry never includes a `Ident` object.
        //
        // This wrapper struct allows us to break the rules and put a `Sync`
        // object in global storage.
        // struct UnsafeSyncEntry(Entry);
        // unsafe impl Sync for UnsafeSyncEntry {}
        // static EMPTY_ENTRY: UnsafeSyncEntry = UnsafeSyncEntry(Entry::End(0 as *const Entry));

        Cursor {
            ptr: &2,
            marker: Default::default(),
        }
    }

    /// This create method intelligently exits non-explicitly-entered
    /// `None`-delimited scopes when the cursor reaches the end of them,
    /// allowing for them to be treated transparently.
    pub unsafe fn create(ptr: *const usize) -> Self {
        Cursor {
            ptr,
            marker: Default::default(),
        }
    }

    /// Get the current entry.
    pub fn entry(&self) -> &'a usize {
        unsafe { &*self.ptr }
    }

    /// Bump the cursor to point at the next token after the current one. This
    /// is undefined behavior if the cursor is currently looking at an
    /// `Entry::End`.
    unsafe fn bump(self) -> Cursor<'a> {
        Cursor::create(self.ptr.offset(1))
    }

    /// While the cursor is looking at a `None`-delimited group, move it to look
    /// at the first token inside instead. If the group is empty, this will move
    /// the cursor past the `None`-delimited group.
    ///
    /// WARNING: This mutates its argument.
    pub fn ignore_none(&mut self) {
        loop {
            let &i = self.entry();
            if i == 0 {
                // NOTE: We call `Cursor::create` here to make sure that
                // situations where we should immediately exit the span after
                // entering it are handled correctly.
                unsafe {
                    *self = Cursor::create(&i);
                }
            } else {
                break;
            }
        }
    }

    /// Checks whether the cursor is currently pointing at the end of its valid
    /// scope.
    pub fn eof(self) -> bool {
        // We're at eof if we're at the end of our scope.
        self.ptr == &1
    }

    /// Skip over the next token without cloning it. Returns `None` if this
    /// cursor points to eof.
    ///
    /// This method treats `'lifetimes` as a single token.
    pub fn skip(self) -> Option<Cursor<'a>> {
        match self.entry() {
            0 => None,
            1 => {
                let next = unsafe { self.bump() };
                match next.entry() {
                    0 => Some(unsafe { next.bump() }),
                    _ => Some(next),
                }
            }
            _ => Some(unsafe { self.bump() }),
        }
    }
}
