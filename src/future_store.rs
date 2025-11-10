use std::{
    alloc::Layout,
    fmt,
    future::{self, Future},
    mem,
    pin::Pin,
    ptr,
    task::{Context, Poll},
};

use futures_util::future::{BoxFuture, LocalBoxFuture};
use scopeguard::defer;

// If the layouts of T and U are different, return Err(new_value)
fn try_reuse_store<T: ?Sized, U, F>(store: Pin<Box<T>>, new_val: U, cb: F) -> Result<(), U>
where
    F: FnOnce(Box<U>),
{
    let t_layout = Layout::for_value::<T>(&*store);
    let u_layout = Layout::new::<U>();
    if t_layout != u_layout {
        return Err(new_val);
    }
    // Consume the box to convert to a raw pointer, now we manage this pinned memory
    let raw: *mut T = Box::into_raw(unsafe { Pin::into_inner_unchecked(store) });
    // Regardless of whether dropping the old value succeeds, we always update, then
    // call the callback on the new value
    defer!({
        // SAFETY: T and U have the same layout, so `raw` is a valid pointer.
        let raw: *mut U = raw.cast::<U>();
        unsafe { raw.write(new_val) };
        let store = unsafe { Box::from_raw(raw) };
        cb(store)
    });
    unsafe { ptr::drop_in_place(raw) };
    Ok(())
}

pub mod sync {
    use super::*;
    pub struct BoxedFutureStore<'a, T> {
        store: BoxFuture<'a, T>,
    }

    impl<'a, T: 'a> BoxedFutureStore<'a, T> {
        pub fn new<F>(fut: F) -> Self
        where
            F: Future<Output = T> + Send + 'a,
        {
            Self {
                store: Box::pin(fut),
            }
        }

        pub fn set<F>(&mut self, fut: F)
        where
            F: Future<Output = T> + Send + 'a,
        {
            // Temporarily swap out the current boxed future so we can take ownership of it.
            let store = mem::replace(&mut self.store, Box::pin(future::pending()));
            // Try to reuse the box
            if let Err(fut) = try_reuse_store(store, fut, |b| self.store = Pin::from(b)) {
                // If reuse fails due to different memory layouts, fall back to reallocation.
                *self = Self::new(fut);
            }
        }

        pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<T> {
            self.store.as_mut().poll(cx)
        }
    }

    impl<T> Future for BoxedFutureStore<'_, T> {
        type Output = T;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
            Pin::into_inner(self).store.as_mut().poll(cx)
        }
    }

    unsafe impl<T> Sync for BoxedFutureStore<'_, T> {}

    impl<T> fmt::Debug for BoxedFutureStore<'_, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("BoxedFutureStore").finish()
        }
    }
}

pub mod unsync {
    use super::*;

    pub struct LocalBoxedFutureStore<'a, T> {
        store: LocalBoxFuture<'a, T>,
    }

    impl<'a, T: 'a> LocalBoxedFutureStore<'a, T> {
        pub fn new<F>(fut: F) -> Self
        where
            F: Future<Output = T> + 'a,
        {
            Self {
                store: Box::pin(fut),
            }
        }

        pub fn set<F>(&mut self, fut: F)
        where
            F: Future<Output = T> + 'a,
        {
            // Temporarily swap out the current boxed future so we can take ownership of it.
            let store = mem::replace(&mut self.store, Box::pin(future::pending()));
            // Try to reuse the box
            if let Err(fut) = try_reuse_store(store, fut, |b| self.store = Pin::from(b)) {
                // If reuse fails due to different memory layouts, fall back to reallocation.
                *self = Self::new(fut);
            }
        }

        pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<T> {
            self.store.as_mut().poll(cx)
        }
    }

    impl<T> Future for LocalBoxedFutureStore<'_, T> {
        type Output = T;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
            Pin::into_inner(self).store.as_mut().poll(cx)
        }
    }

    unsafe impl<T> Sync for LocalBoxedFutureStore<'_, T> {}

    impl<T> fmt::Debug for LocalBoxedFutureStore<'_, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("LocalBoxedFutureStore").finish()
        }
    }
}
