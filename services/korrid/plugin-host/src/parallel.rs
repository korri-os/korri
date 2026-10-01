//! Map a function over items on all CPU cores, keeping input order.
//!
//! Boot restore loads every enabled plugin's report this way: evaluating the
//! declarations one after another took about 4.4 s for 20 plugins on the
//! RG353M's four cores.
use std::sync::atomic::{AtomicUsize, Ordering};

/// Workers get the main thread's stack size: declaration evaluation
/// transpiles and runs plugin source, and a spawned thread's 2 MiB default is
/// smaller than the main thread it replaces.
const STACK_BYTES: usize = 8 * 1024 * 1024;

pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let workers = std::thread::available_parallelism()
        .map_or(1, |cores| cores.get())
        .min(items.len());
    if workers <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let mut results: Vec<Option<R>> = items.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                std::thread::Builder::new()
                    .stack_size(STACK_BYTES)
                    .spawn_scoped(scope, || {
                        let mut done = Vec::new();
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some(item) = items.get(index) else {
                                break;
                            };
                            done.push((index, f(item)));
                        }
                        done
                    })
            })
            .collect();
        for handle in handles {
            let done = match handle {
                Ok(handle) => handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
                // No worker thread: the remaining items run here.
                Err(_) => Vec::new(),
            };
            for (index, result) in done {
                results[index] = Some(result);
            }
        }
    });
    results
        .into_iter()
        .zip(items)
        .map(|(result, item)| result.unwrap_or_else(|| f(item)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::map;
    use std::{collections::HashSet, sync::Mutex, thread, time::Duration};

    #[test]
    fn keeps_input_order_and_maps_every_item_once() {
        let items: Vec<u32> = (0..50).collect();
        let calls = Mutex::new(Vec::new());
        let doubled = map(&items, |item| {
            calls.lock().unwrap().push(*item);
            item * 2
        });
        assert_eq!(doubled, (0..50).map(|item| item * 2).collect::<Vec<_>>());
        let mut calls = calls.into_inner().unwrap();
        calls.sort();
        assert_eq!(calls, items);
    }

    #[test]
    fn uses_more_than_one_thread_when_cores_allow() {
        if thread::available_parallelism().map_or(1, |cores| cores.get()) < 2 {
            return;
        }
        let threads = Mutex::new(HashSet::new());
        map(&[(); 8], |()| {
            threads.lock().unwrap().insert(thread::current().id());
            thread::sleep(Duration::from_millis(50));
        });
        assert!(threads.into_inner().unwrap().len() > 1);
    }

    #[test]
    fn empty_input_maps_to_empty_output() {
        assert!(map(&[] as &[u8], |item| *item).is_empty());
    }
}
