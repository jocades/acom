use std::cell::Cell;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

#[macro_export]
macro_rules! pin {
    ($x:ident) => {
        let mut $x = core::pin::pin!($x);
    };
}

pub async fn yield_now() {
    struct YieldNow(bool);

    impl Future for YieldNow {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            println!("yield_now.poll())");
            if !self.0 {
                self.0 = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(())
        }
    }

    YieldNow(false).await
}

pub async fn join<F1: Future, F2: Future>(f1: F1, f2: F2) -> (F1::Output, F2::Output) {
    pin_project! {
        struct Join<F1: Future, F2: Future> {
            #[pin]
            f1: Option<F1>,
            o1: Option<F1::Output>,
            #[pin]
            f2: Option<F2>,
            o2: Option<F2::Output>
        }
    }

    impl<F1: Future, F2: Future> Future for Join<F1, F2> {
        type Output = (F1::Output, F2::Output);

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut this = self.project();

            if let Some(f1) = this.f1.as_mut().as_pin_mut() {
                println!("poll f1");
                if let Poll::Ready(o1) = f1.poll(cx) {
                    println!("ready f1");
                    *this.o1 = Some(o1);
                    this.f1.set(None);
                }
            }

            if let Some(f2) = this.f2.as_mut().as_pin_mut() {
                println!("poll f2");
                if let Poll::Ready(o2) = f2.poll(cx) {
                    println!("ready f2");
                    *this.o2 = Some(o2);
                    this.f2.set(None);
                }
            }

            match (this.o1.take(), this.o2.take()) {
                (Some(o1), Some(o2)) => Poll::Ready((o1, o2)),
                (maybe1, maybe2) => {
                    *this.o1 = maybe1;
                    *this.o2 = maybe2;
                    Poll::Pending
                }
            }
        }
    }

    Join {
        f1: Some(f1),
        o1: None,
        f2: Some(f2),
        o2: None,
    }
    .await
}

fn random_bool() -> bool {
    thread_local! {
        static RNG_STATE: Cell<u64> = Cell::new({
            let x = 0;
            ((&x as *const i32 as usize as u64) ^ 0x123456789abcdef) | 1
        });
    }

    RNG_STATE.with(|state| {
        let mut s = state.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        state.set(s);
        s & 1 == 1
    })
}

pub async fn select<O, F1, F2>(f1: F1, f2: F2) -> O
where
    F1: Future<Output = O>,
    F2: Future<Output = O>,
{
    pin_project! {
        struct Select<F1: Future, F2: Future> {
            #[pin]
            f1: F1,
            #[pin]
            f2: F2,
        }
    }

    impl<O, F1, F2> Future for Select<F1, F2>
    where
        F1: Future<Output = O>,
        F2: Future<Output = O>,
    {
        type Output = O;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut this = self.project();

            if random_bool() {
                if let p1 @ Poll::Ready(_) = this.f1.as_mut().poll(cx) {
                    return p1;
                }
                if let p2 @ Poll::Ready(_) = this.f2.as_mut().poll(cx) {
                    return p2;
                }
            } else {
                if let p2 @ Poll::Ready(_) = this.f2.as_mut().poll(cx) {
                    return p2;
                }
                if let p1 @ Poll::Ready(_) = this.f1.as_mut().poll(cx) {
                    return p1;
                }
            }

            Poll::Pending
        }
    }

    Select { f1, f2 }.await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{pending, ready};
    use std::task::Waker;

    fn spin_on<F: Future>(fut: F) -> F::Output {
        let mut cx = Context::from_waker(Waker::noop());

        pin!(fut);

        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Pending => {}
                Poll::Ready(o) => return o,
            }
        }
    }

    #[test]
    fn spin_on_join() {
        let f1 = ready(1);
        let f2 = async {
            yield_now().await;
            2
        };
        let (o1, o2) = spin_on(join(f1, f2));
        assert_eq!(o1, 1);
        assert_eq!(o2, 2);
    }

    #[test]
    fn spin_on_select() {
        let f1 = ready(1);
        let f2 = pending();
        let o = spin_on(select(f1, f2));
        assert_eq!(o, 1);

        let f1 = pending();
        let f2 = ready(2);
        let o = spin_on(select(f1, f2));
        assert_eq!(o, 2);
    }
}
