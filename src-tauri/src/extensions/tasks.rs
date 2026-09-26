//! 异步副作用执行器：有限并发，网络等待期间不占用宿主或直播状态锁。
use super::{EffectResult, ExtensionEffect, ExtensionHost};
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{collections::VecDeque, future::Future, sync::Arc, time::Duration};

pub async fn run(host: Arc<ExtensionHost>) {
    run_tasks(
        || host.take_effects(),
        |(id, effect)| {
            let host = host.clone();
            async move {
                let result = match effect {
                    ExtensionEffect::FetchVideoInfo {
                        request_id,
                        video_id,
                    } => {
                        let result = super::video_info::fetch_video_info(&video_id).await;
                        EffectResult::VideoInfo { request_id, result }
                    }
                };
                host.complete_effect(&id, result);
            }
        },
    )
    .await;
}

async fn run_tasks<T, F: Future<Output = ()>>(
    mut take_tasks: impl FnMut() -> Vec<T>,
    run_task: impl Fn(T) -> F,
) {
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pending = VecDeque::new();
    // 在途 future 由执行器直接持有，退出时一并取消。
    let mut running = FuturesUnordered::new();
    loop {
        tokio::select! {
            _ = ticker.tick() => pending.extend(take_tasks()),
            Some(()) = running.next(), if !running.is_empty() => {},
        }
        // 新任务和单个任务完成均可填补空闲槽，无需等待整批结束。
        while running.len() < 4 {
            let Some(task) = pending.pop_front() else {
                break;
            };
            running.push(run_task(task));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{mpsc, oneshot};

    struct ActiveTask(Arc<AtomicUsize>);

    impl Drop for ActiveTask {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct TestExecutor {
        tasks: mpsc::UnboundedSender<(usize, oneshot::Receiver<()>)>,
        started: mpsc::UnboundedReceiver<usize>,
        completed: mpsc::UnboundedReceiver<usize>,
        active: Arc<AtomicUsize>,
        handle: tokio::task::JoinHandle<()>,
    }

    impl TestExecutor {
        fn new() -> Self {
            let (tasks, mut tasks_rx) = mpsc::unbounded_channel();
            let (started_tx, started) = mpsc::unbounded_channel();
            let (completed_tx, completed) = mpsc::unbounded_channel();
            let active = Arc::new(AtomicUsize::new(0));
            let active_count = active.clone();
            let handle = tokio::spawn(async move {
                run_tasks(
                    || {
                        let mut tasks = Vec::new();
                        while let Ok(task) = tasks_rx.try_recv() {
                            tasks.push(task);
                        }
                        tasks
                    },
                    |(id, finish): (usize, oneshot::Receiver<()>)| {
                        let active = active_count.clone();
                        let started = started_tx.clone();
                        let completed = completed_tx.clone();
                        async move {
                            active.fetch_add(1, Ordering::SeqCst);
                            let _active_task = ActiveTask(active);
                            started.send(id).unwrap();
                            let _ = finish.await;
                            completed.send(id).unwrap();
                        }
                    },
                )
                .await;
            });
            Self {
                tasks,
                started,
                completed,
                active,
                handle,
            }
        }

        fn enqueue(&self, id: usize) -> oneshot::Sender<()> {
            let (finish, receiver) = oneshot::channel();
            self.tasks.send((id, receiver)).unwrap();
            finish
        }
    }

    impl Drop for TestExecutor {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    async fn receive(receiver: &mut mpsc::UnboundedReceiver<usize>) -> usize {
        tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("任务调度超时")
            .expect("执行器提前退出")
    }

    #[tokio::test]
    async fn later_tasks_use_free_slots_while_first_task_is_pending() {
        let mut executor = TestExecutor::new();
        let _slow = executor.enqueue(0);
        assert_eq!(receive(&mut executor.started).await, 0);

        let quick = executor.enqueue(1);
        assert_eq!(receive(&mut executor.started).await, 1);
        quick.send(()).unwrap();
        assert_eq!(receive(&mut executor.completed).await, 1);
        assert_eq!(executor.active.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrency_is_bounded_and_each_completion_fills_one_slot() {
        let mut executor = TestExecutor::new();
        let mut tasks: Vec<_> = (0..6).map(|id| executor.enqueue(id)).collect();
        for id in 0..4 {
            assert_eq!(receive(&mut executor.started).await, id);
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(250), executor.started.recv())
                .await
                .is_err()
        );
        assert_eq!(executor.active.load(Ordering::SeqCst), 4);

        tasks.remove(2).send(()).unwrap();
        assert_eq!(receive(&mut executor.completed).await, 2);
        assert_eq!(receive(&mut executor.started).await, 4);
        assert_eq!(executor.active.load(Ordering::SeqCst), 4);

        tasks.remove(0).send(()).unwrap();
        assert_eq!(receive(&mut executor.completed).await, 0);
        assert_eq!(receive(&mut executor.started).await, 5);
        assert_eq!(executor.active.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn stopping_executor_cancels_in_flight_tasks() {
        let mut executor = TestExecutor::new();
        let task = executor.enqueue(0);
        assert_eq!(receive(&mut executor.started).await, 0);
        executor.handle.abort();
        assert!((&mut executor.handle).await.unwrap_err().is_cancelled());
        assert_eq!(executor.active.load(Ordering::SeqCst), 0);
        assert!(task.send(()).is_err());
        assert!(executor.completed.try_recv().is_err());
    }
}
