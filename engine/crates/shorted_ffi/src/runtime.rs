//! Engine threads (docs/05 §5): a single command thread that owns project state, and a background
//! pool with priorities and cancellable tasks. Nothing here ever runs on the UI isolate.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;

use crate::error::{EngineError, ErrorCode};
use crate::frb_generated::StreamSink;

const MAX_BACKGROUND_THREADS: usize = 4;

/// Events of one background task, in order: `Started`, any number of `Progress`, then exactly one
/// terminal event.
#[derive(Clone, Debug, PartialEq)]
pub enum TaskEvent {
    Started { task_id: u64 },
    Progress { fraction: f64 },
    Finished,
    Cancelled,
    Failed { code: ErrorCode },
}

/// Where a task reports its events. Lets tests observe tasks without the bridge.
pub trait TaskReporter: Send + 'static {
    fn report(&self, event: TaskEvent);
}

impl TaskReporter for StreamSink<TaskEvent> {
    fn report(&self, event: TaskEvent) {
        // The Dart side may have stopped listening; the task then ends on its own.
        let _ = self.add(event);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Yields to everything else; used while playback is running.
    Low,
    Normal,
    High,
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Handle given to a running task.
pub struct TaskContext<'a> {
    token: &'a CancellationToken,
    reporter: &'a dyn TaskReporter,
}

impl TaskContext<'_> {
    pub fn check_cancelled(&self) -> Result<(), EngineError> {
        if self.token.is_cancelled() {
            Err(EngineError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn progress(&self, fraction: f64) {
        self.reporter.report(TaskEvent::Progress { fraction });
    }
}

/// Runs `task` as a job would, without a queue or a listener.
#[cfg(test)]
pub(crate) fn with_test_context<R>(task: impl FnOnce(&TaskContext<'_>) -> R) -> R {
    struct Silent;
    impl TaskReporter for Silent {
        fn report(&self, _event: TaskEvent) {}
    }
    task(&TaskContext {
        token: &CancellationToken::default(),
        reporter: &Silent,
    })
}

type Job = Box<dyn FnOnce() + Send>;

struct QueuedJob {
    priority: Priority,
    // Reversed so that, within a priority, the oldest job runs first.
    order: Reverse<u64>,
    job: Job,
}

impl PartialEq for QueuedJob {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
    }
}

impl Eq for QueuedJob {}

impl PartialOrd for QueuedJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.priority, self.order).cmp(&(other.priority, other.order))
    }
}

#[derive(Default)]
struct PoolState {
    queue: Mutex<BinaryHeap<QueuedJob>>,
    available: Condvar,
    tokens: Mutex<HashMap<u64, CancellationToken>>,
    next_id: AtomicU64,
}

// A panicking job must not take the whole pool down, so poisoned locks are recovered: the data
// they protect is a queue or a map that stays valid between operations.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Background pool for analysis, transcription, proxies, thumbnails, export and journal writes.
pub struct BackgroundPool {
    state: Arc<PoolState>,
}

impl BackgroundPool {
    pub fn new(threads: usize) -> Result<Self, EngineError> {
        let state = Arc::new(PoolState::default());
        for index in 0..threads {
            let state = Arc::clone(&state);
            thread::Builder::new()
                .name(format!("shorted-bg-{index}"))
                .spawn(move || worker(&state))?;
        }
        Ok(Self { state })
    }

    /// Queues a task and returns its id. The reporter gets exactly one terminal event, including
    /// when the task is cancelled before it starts or when it panics.
    pub fn spawn<R, F>(&self, priority: Priority, reporter: R, task: F) -> u64
    where
        R: TaskReporter,
        F: FnOnce(&TaskContext<'_>) -> Result<(), EngineError> + Send + 'static,
    {
        let id = self.state.next_id.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::default();
        lock(&self.state.tokens).insert(id, token.clone());
        reporter.report(TaskEvent::Started { task_id: id });
        let state = Arc::clone(&self.state);
        let job = Box::new(move || {
            let context = TaskContext {
                token: &token,
                reporter: &reporter,
            };
            let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
                context.check_cancelled()?;
                task(&context)
            }));
            lock(&state.tokens).remove(&id);
            reporter.report(match outcome {
                Ok(Ok(())) => TaskEvent::Finished,
                Ok(Err(EngineError::Cancelled)) => TaskEvent::Cancelled,
                Ok(Err(error)) => {
                    tracing::error!(%error, "background task failed");
                    TaskEvent::Failed { code: error.code() }
                }
                Err(_) => {
                    tracing::error!("background task panicked");
                    TaskEvent::Failed {
                        code: ErrorCode::Internal,
                    }
                }
            });
        });
        lock(&self.state.queue).push(QueuedJob {
            priority,
            order: Reverse(id),
            job,
        });
        self.state.available.notify_one();
        id
    }

    pub fn cancel(&self, id: u64) {
        if let Some(token) = lock(&self.state.tokens).get(&id) {
            token.cancel();
        }
    }
}

fn worker(state: &PoolState) {
    loop {
        let job = {
            let mut queue = lock(&state.queue);
            loop {
                match queue.pop() {
                    Some(queued) => break queued.job,
                    None => {
                        queue = state
                            .available
                            .wait(queue)
                            .unwrap_or_else(PoisonError::into_inner);
                    }
                }
            }
        };
        job();
    }
}

/// Single writer: commands run one at a time, in submission order.
pub struct CommandThread {
    sender: Sender<Job>,
}

impl CommandThread {
    pub fn new() -> Result<Self, EngineError> {
        let (sender, receiver) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("shorted-command".into())
            .spawn(move || {
                for job in receiver {
                    job();
                }
            })?;
        Ok(Self { sender })
    }

    pub fn submit(&self, command: impl FnOnce() + Send + 'static) -> Result<(), EngineError> {
        self.sender
            .send(Box::new(command))
            .map_err(|_| EngineError::Internal("command thread stopped".into()))
    }
}

pub struct Runtime {
    pub commands: CommandThread,
    pub background: BackgroundPool,
}

static RUNTIME: OnceLock<Result<Runtime, ErrorCode>> = OnceLock::new();

fn background_threads() -> usize {
    thread::available_parallelism().map_or(2, |count| {
        (count.get() / 2).clamp(1, MAX_BACKGROUND_THREADS)
    })
}

/// Starts the runtime once. Failing to create OS threads at launch is unrecoverable.
#[allow(clippy::expect_used)] // Proven invariant: without threads the engine cannot run at all.
pub fn global() -> &'static Runtime {
    RUNTIME
        .get_or_init(|| {
            Ok(Runtime {
                commands: CommandThread::new().map_err(|error| error.code())?,
                background: BackgroundPool::new(background_threads())
                    .map_err(|error| error.code())?,
            })
        })
        .as_ref()
        .expect("engine threads could not start")
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{channel, Receiver};
    use std::time::Duration;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(5);

    struct ChannelReporter(Mutex<Sender<TaskEvent>>);

    impl TaskReporter for ChannelReporter {
        fn report(&self, event: TaskEvent) {
            lock(&self.0).send(event).unwrap();
        }
    }

    fn reporter() -> (ChannelReporter, Receiver<TaskEvent>) {
        let (sender, receiver) = channel();
        (ChannelReporter(Mutex::new(sender)), receiver)
    }

    fn terminal(events: &Receiver<TaskEvent>) -> TaskEvent {
        loop {
            match events.recv_timeout(TIMEOUT).unwrap() {
                TaskEvent::Started { .. } | TaskEvent::Progress { .. } => {}
                event => return event,
            }
        }
    }

    #[test]
    fn long_task_reports_progress_then_cancels() {
        let pool = BackgroundPool::new(1).unwrap();
        let (reporter, events) = reporter();
        let id = pool.spawn(Priority::Normal, reporter, |task| loop {
            task.check_cancelled()?;
            task.progress(0.5);
            thread::sleep(Duration::from_millis(1));
        });
        assert_eq!(
            events.recv_timeout(TIMEOUT).unwrap(),
            TaskEvent::Started { task_id: id }
        );
        assert_eq!(
            events.recv_timeout(TIMEOUT).unwrap(),
            TaskEvent::Progress { fraction: 0.5 }
        );
        pool.cancel(id);
        assert_eq!(terminal(&events), TaskEvent::Cancelled);
    }

    #[test]
    fn task_failure_reports_a_safe_code() {
        let pool = BackgroundPool::new(1).unwrap();
        let (reporter, events) = reporter();
        pool.spawn(Priority::Normal, reporter, |_| {
            Err(EngineError::Internal("secret path".into()))
        });
        assert_eq!(
            terminal(&events),
            TaskEvent::Failed {
                code: ErrorCode::Internal
            }
        );
    }

    #[test]
    fn panicking_task_fails_and_the_pool_survives() {
        let pool = BackgroundPool::new(1).unwrap();
        let (first, first_events) = reporter();
        pool.spawn(Priority::Normal, first, |_| panic!("boom"));
        assert_eq!(
            terminal(&first_events),
            TaskEvent::Failed {
                code: ErrorCode::Internal
            }
        );
        let (second, second_events) = reporter();
        pool.spawn(Priority::Normal, second, |_| Ok(()));
        assert_eq!(terminal(&second_events), TaskEvent::Finished);
    }

    #[test]
    fn task_cancelled_while_queued_never_runs() {
        let pool = BackgroundPool::new(1).unwrap();
        let (gate_open, gate) = channel::<()>();
        let (blocker, blocker_events) = reporter();
        pool.spawn(Priority::High, blocker, move |_| {
            gate.recv_timeout(TIMEOUT).ok();
            Ok(())
        });
        let (queued, queued_events) = reporter();
        let id = pool.spawn(Priority::Normal, queued, |_| panic!("must not run"));
        pool.cancel(id);
        gate_open.send(()).unwrap();
        assert_eq!(terminal(&blocker_events), TaskEvent::Finished);
        assert_eq!(terminal(&queued_events), TaskEvent::Cancelled);
    }

    #[test]
    fn higher_priority_runs_first_then_oldest_first() {
        let pool = BackgroundPool::new(1).unwrap();
        let (gate_open, gate) = channel::<()>();
        let (blocker, blocker_events) = reporter();
        pool.spawn(Priority::High, blocker, move |_| {
            gate.recv_timeout(TIMEOUT).ok();
            Ok(())
        });
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut done = Vec::new();
        for (name, priority) in [
            ("low", Priority::Low),
            ("normal-1", Priority::Normal),
            ("high", Priority::High),
            ("normal-2", Priority::Normal),
        ] {
            let order = Arc::clone(&order);
            let (task_reporter, events) = reporter();
            done.push(events);
            pool.spawn(priority, task_reporter, move |_| {
                lock(&order).push(name);
                Ok(())
            });
        }
        gate_open.send(()).unwrap();
        assert_eq!(terminal(&blocker_events), TaskEvent::Finished);
        for events in &done {
            assert_eq!(terminal(events), TaskEvent::Finished);
        }
        assert_eq!(*lock(&order), ["high", "normal-1", "normal-2", "low"]);
    }

    #[test]
    fn commands_run_in_order_off_the_calling_thread() {
        let commands = CommandThread::new().unwrap();
        let (sender, receiver) = channel();
        let caller = thread::current().id();
        for value in 0..50 {
            let sender = sender.clone();
            commands
                .submit(move || sender.send((value, thread::current().id())).unwrap())
                .unwrap();
        }
        for expected in 0..50 {
            let (value, thread_id) = receiver.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(value, expected);
            assert_ne!(thread_id, caller);
        }
    }
}
