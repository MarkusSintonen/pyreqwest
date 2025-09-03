use pyo3::prelude::*;
use std::pin::{Pin, pin};
use std::task::{Context, Poll};

#[repr(transparent)]
#[pin_project::pin_project]
pub struct AllowThreads<T>(#[pin] pub T);

impl<F> Future for AllowThreads<F>
where
    F: Future + Send,
    F::Output: Send,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let waker = cx.waker();
        Python::attach(|py| py.detach(|| this.0.poll(&mut Context::from_waker(waker))))
    }
}
