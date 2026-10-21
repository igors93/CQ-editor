//! The Python worker process that runs CadQuery.
//!
//! The worker is started as `python -m cq_worker` and speaks the line-based
//! JSON protocol of [`protocol`]. Three helper threads keep the UI thread
//! non-blocking: one writes requests, one parses messages from stdout (mesh
//! decoding included) and one forwards stderr lines to the log.
//!
//! Stopping a runaway script is done by killing the process and starting a
//! fresh one, which is also how the GUI recovers from a crashed worker.

pub mod launch;
pub mod protocol;

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Instant, SystemTime};

use protocol::{Message, ReadyInfo, Request, RequestId};

/// How to start the worker.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchConfig {
    pub python: PathBuf,
    /// Directory containing the `cq_worker` package.
    pub worker_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub enum Status {
    NotStarted,
    /// Process started, CadQuery still importing.
    Starting,
    Ready(ReadyInfo),
    /// The worker could not start or exited; holds a user-facing reason.
    Failed(String),
}

/// Something that happened in the worker process.
#[derive(Debug)]
pub enum Event {
    Message(Message),
    /// A line the worker printed on stderr (native library warnings, crashes).
    Stderr(String),
    /// The process ended (or its stdout closed).
    Exited,
}

pub struct Worker {
    ctx: egui::Context,
    config: Option<LaunchConfig>,
    process: Option<Process>,
    status: Status,
    next_id: RequestId,
    /// Distinguishes events of the current process from those of killed ones.
    generation: u64,
    events_tx: Sender<(u64, Event)>,
    events_rx: Receiver<(u64, Event)>,
}

struct Process {
    child: Child,
    requests: Sender<String>,
}

impl Worker {
    pub fn new(ctx: egui::Context) -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        Self {
            ctx,
            config: None,
            process: None,
            status: Status::NotStarted,
            next_id: 1,
            generation: 0,
            events_tx,
            events_rx,
        }
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn info(&self) -> Option<&ReadyInfo> {
        match &self.status {
            Status::Ready(info) => Some(info),
            _ => None,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.status, Status::Ready(_))
    }

    pub fn config(&self) -> Option<&LaunchConfig> {
        self.config.as_ref()
    }

    /// (Re)starts the worker with `config`.
    pub fn start(&mut self, config: LaunchConfig) {
        self.kill();
        self.generation += 1;
        self.config = Some(config.clone());
        match self.spawn(&config) {
            Ok(process) => {
                log::info!(
                    "Starting the CadQuery worker with {}",
                    config.python.display()
                );
                self.process = Some(process);
                self.status = Status::Starting;
            }
            Err(err) => {
                let reason = format!(
                    "Could not start Python ({}): {err}",
                    config.python.display()
                );
                log::error!("{reason}");
                self.status = Status::Failed(reason);
            }
        }
    }

    pub fn next_id(&mut self) -> RequestId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Queues `request` for the worker. Returns `false` when no worker runs.
    pub fn send(&mut self, request: &Request) -> bool {
        let Some(process) = &self.process else {
            return false;
        };
        let line = match serde_json::to_string(request) {
            Ok(line) => line,
            Err(err) => {
                log::error!("Cannot encode worker request: {err}");
                return false;
            }
        };
        process.requests.send(line).is_ok()
    }

    /// Drains pending events of the current process, updating the status.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok((generation, event)) = self.events_rx.try_recv() {
            if generation != self.generation {
                continue; // from a process we already killed
            }
            match &event {
                Event::Message(Message::Ready(info)) => {
                    if info.protocol != protocol::PROTOCOL_VERSION {
                        log::warn!(
                            "Worker protocol {} differs from the GUI's {}",
                            info.protocol,
                            protocol::PROTOCOL_VERSION
                        );
                    }
                    self.status = Status::Ready(info.clone());
                }
                Event::Message(Message::Fatal { message, .. }) => {
                    self.status = Status::Failed(message.clone());
                }
                Event::Exited => {
                    let code = self
                        .process
                        .take()
                        .and_then(|mut p| p.child.wait().ok())
                        .and_then(|status| status.code());
                    if !matches!(self.status, Status::Failed(_)) {
                        self.status = Status::Failed(match code {
                            Some(code) => format!("The Python worker exited (code {code})"),
                            None => "The Python worker stopped unexpectedly".to_owned(),
                        });
                    }
                }
                _ => {}
            }
            events.push(event);
        }
        events
    }

    /// Asks the worker to exit, giving it a moment to clean up before it is
    /// killed.
    pub fn shutdown(&mut self) {
        if let Some(process) = &mut self.process {
            let _ = process
                .requests
                .send(serde_json::to_string(&Request::Shutdown).unwrap_or_default());
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
            while std::time::Instant::now() < deadline {
                if matches!(process.child.try_wait(), Ok(Some(_))) {
                    break;
                }
                thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        self.kill();
    }

    fn kill(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
        self.status = Status::NotStarted;
    }

    fn spawn(&self, config: &LaunchConfig) -> std::io::Result<Process> {
        let mut command = Command::new(&config.python);
        command
            .args(["-u", "-m", "cq_worker"])
            .env("PYTHONPATH", python_path(&config.worker_dir))
            .env("PYTHONIOENCODING", "utf-8")
            // Namespaces the worker's scene keys: objects of a killed worker
            // stay on screen, and a new worker must not reuse their keys.
            .env("CQ_EDITOR_SESSION", self.generation.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let (requests, pending) = mpsc::channel::<String>();
        thread::Builder::new()
            .name("worker-writer".into())
            .spawn(move || write_requests(stdin, pending))?;

        let (tx, ctx, generation) = (self.events_tx.clone(), self.ctx.clone(), self.generation);
        thread::Builder::new()
            .name("worker-reader".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let received_at = SystemTime::now();
                    let started = Instant::now();
                    match serde_json::from_str::<Message>(&line) {
                        Ok(mut message) => {
                            if let Message::RunFinished(result) = &mut message {
                                result.receipt = protocol::Receipt {
                                    received_at: Some(received_at),
                                    parse: started.elapsed(),
                                };
                            }
                            let _ = tx.send((generation, Event::Message(message)));
                        }
                        Err(err) => log::warn!(
                            "Unreadable worker message ({err}): {}",
                            line.chars().take(200).collect::<String>()
                        ),
                    }
                    ctx.request_repaint();
                }
                let _ = tx.send((generation, Event::Exited));
                ctx.request_repaint();
            })?;

        let (tx, ctx) = (self.events_tx.clone(), self.ctx.clone());
        thread::Builder::new()
            .name("worker-stderr".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    let _ = tx.send((generation, Event::Stderr(line)));
                    ctx.request_repaint();
                }
            })?;

        Ok(Process { child, requests })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn write_requests(mut stdin: ChildStdin, pending: Receiver<String>) {
    for line in pending {
        if stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .is_err()
        {
            break; // the process is gone; the reader reports it
        }
    }
}

/// `worker_dir` in front of any `PYTHONPATH` the user already has.
fn python_path(worker_dir: &std::path::Path) -> std::ffi::OsString {
    let mut paths = vec![worker_dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).unwrap_or_else(|_| worker_dir.as_os_str().to_owned())
}
