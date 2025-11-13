use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Parallel<'a, R> {
    closures: Vec<Box<dyn FnOnce() -> R + Send + 'a>>,
}

impl<'a, R: Send + 'a> Parallel<'a, R> {
    pub fn new() -> Self {
        Self { closures: vec![] }
    }

    pub fn add<F>(mut self, f: F) -> Self
    where
        F: FnOnce() -> R + Send + 'a,
    {
        self.closures.push(Box::new(f));
        self
    }

    pub fn each<I, T, F>(mut self, iter: I, f: F) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Send + 'a,
        F: FnOnce(T) -> R + Clone + Send + 'a,
    {
        for t in iter.into_iter() {
            let f = f.clone();
            self.closures.push(Box::new(|| f(t)));
        }
        self
    }

    pub fn add_with<V, F>(mut self, v: V, f: F) -> Self
    where
        V: Send + 'a,
        F: FnOnce(V) -> R + Send + 'a,
    {
        self.closures.push(Box::new(|| f(v)));
        self
    }

    pub fn each_with<V, I, T, F>(mut self, v: &V, iter: I, f: F) -> Self
    where
        V: Send + Clone + 'a,
        I: IntoIterator<Item = T>,
        T: Send + 'a,
        F: FnOnce(V, T) -> R + Clone + Send + 'a,
    {
        for t in iter.into_iter() {
            let (f, v) = (f.clone(), v.clone());
            self.closures.push(Box::new(|| f(v, t)));
        }
        self
    }

    pub fn run(self) -> Vec<R> {
        thread::scope(|s| {
            let (tx, rx) = mpsc::channel();

            for (i, f) in self.closures.into_iter().enumerate() {
                let tx = tx.clone();
                s.spawn(move || _ = tx.send((i, f())));
            }
            drop(tx);

            let mut results: Vec<_> = rx.iter().collect();
            results.sort_by_key(|(i, _)| *i);
            results.into_iter().map(|(_, r)| r).collect()
        })
    }

    pub fn run_unordered(self) -> Vec<R> {
        thread::scope(|s| {
            let (tx, rx) = mpsc::channel();

            for f in self.closures.into_iter() {
                let tx = tx.clone();
                s.spawn(move || _ = tx.send(f()));
            }
            drop(tx);

            rx.iter().collect()
        })
    }

    pub fn run_with_timeout(self, timeout: Duration) -> Vec<Option<R>> {
        thread::scope(|s| {
            let (tx, rx) = mpsc::channel();
            let count = self.closures.len();

            for (i, f) in self.closures.into_iter().enumerate() {
                let tx = tx.clone();
                s.spawn(move || tx.send((i, f())).ok());
            }
            drop(tx);

            let mut results: Vec<_> = (0..count).map(|_| None).collect();
            let deadline = Instant::now() + timeout;

            for (i, r) in rx.iter() {
                if Instant::now() >= deadline {
                    break;
                }
                results[i] = Some(r);
            }

            results
        })
    }
}

impl<'a: 'static, R: Send + 'a> Parallel<'a, R> {
    pub fn spawn(self) -> Vec<JoinHandle<R>> {
        self.closures
            .into_iter()
            .map(|f| thread::spawn(f))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn sum() {
        let result = Parallel::new()
            .add(|| 2)
            .add(|| 3)
            .each(0..5, |_| 1)
            .run_unordered()
            .iter()
            .sum::<i32>();

        assert_eq!(result, 10);
    }

    #[test]
    fn capture() {
        let mut v = vec![1, 2];
        Parallel::new().each(&mut v, |x| *x += 1).run();
        assert_eq!(v, vec![2, 3]);
    }

    #[test]
    fn counter() {
        let count = AtomicUsize::new(0);
        Parallel::new()
            .each(0..10, |_| count.fetch_add(1, Ordering::SeqCst))
            .run();
        assert_eq!(count.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn spawn() {
        let mut sum = 0;
        for jh in Parallel::new().add(|| 1).each(vec![1, 2], |x| x).spawn() {
            sum += jh.join().unwrap();
        }
        assert_eq!(sum, 4);
    }

    #[test]
    fn add_with() {
        let m = Arc::new(Mutex::new(false));
        Parallel::new()
            .add_with(&m, |m| *m.lock().unwrap() = true)
            .run();
        assert!(*m.lock().unwrap());
    }

    #[test]
    fn each_with() {
        let m = Arc::new(Mutex::new(0));
        Parallel::new()
            .each_with(&m, 0..5, |m, _| *m.lock().unwrap() += 1)
            .spawn()
            .drain(..)
            .for_each(|jh| jh.join().unwrap());
        assert_eq!(*m.lock().unwrap(), 5);
    }
}
