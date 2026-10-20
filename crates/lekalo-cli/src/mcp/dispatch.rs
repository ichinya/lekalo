//! The JSON-RPC dispatch for the MCP server (issue #33): the pinned
//! 2025-11-25 lifecycle over stdio.
//!
//! After `initialize`/`notifications/initialized`, the served methods are
//! ping, tools/list, tools/call, resources/list, resources/templates/list,
//! resources/read and bounded resource subscriptions. Resource changes are
//! polled through the same admitted, supervised read as resources/read.
//! The concrete catalog is fixed at startup, so listChanged stays false. The
//! launcher `--group` is the disclosure ceiling of every tool call and
//! resource read (F3/F4); inbound frames refuse duplicate keys (F10);
//! the lifecycle is a real state machine (r2 F14/codex 12: the envelope
//! must be valid JSON-RPC 2.0, requests and notifications are distinct,
//! and `notifications/initialized` cannot unlock a session that never
//! ran `initialize`). `initialize` itself is single-shot (r3 F7): a
//! second `initialize` after the handshake was answered is the typed
//! `-32600` lifecycle refusal, never a fresh result. A timeout produces
//! a typed bounded tool error while
//! the client is still waiting; the abandoned worker keeps holding its
//! concurrency slot until it finishes (F14/F19). A cancellation
//! notification suppresses the late result and never gets a reply; it is
//! registered only for an outstanding id, so a reused id can never be
//! poisoned (r2 F6/F19).

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use lekalo_core::mcp::policy::Limits;
use lekalo_core::mcp::resources::{self, Cursor, Family, UriScope};
use lekalo_core::mcp::tools::{self, parse_request, RequestError};

use super::catalog::ServerCatalog;
use super::execution;
use super::stdio::log_event;
use super::stdio::Writer;
use super::work::{self, WorkGuard};

/// The slot-ownership states of one dispatched worker (`SLOT_BUSY` while
/// the worker still holds its concurrency slot; `SLOT_RELEASED` once the
/// dispatcher or the worker handed it back — exactly once).
const SLOT_BUSY: usize = 0;
const SLOT_RELEASED: usize = 1;

/// The JSON-RPC protocol codes this server uses.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// How often the supervised wait re-polls the inbound channel while a
/// worker runs: the granularity at which a cancellation notification is
/// observed during live work.
const WAIT_POLL: Duration = Duration::from_millis(25);
const RESOURCE_POLL: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct Subscription {
    uri: String,
    scope: UriScope,
    family: Family,
    qualifier: Option<String>,
    revision: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResourceAction {
    Read,
    Subscribe,
    Unsubscribe,
}

/// One live server session: the immutable catalog plus the mutable
/// lifecycle state (admission counters, the handshake, and the
/// cancellation bookkeeping over outstanding request ids only).
pub struct Session {
    pub catalog: ServerCatalog,
    pub limits: Limits,
    /// The trusted launcher group: the default disclosure scope of every
    /// tool call that does not name one explicitly.
    pub default_group: Option<String>,
    pub writer: Writer,
    initialized: Mutex<bool>,
    /// `initialize` was answered; `notifications/initialized` only
    /// unlocks from here (r2 F14: the notification alone never unlocks).
    handshake: Mutex<bool>,
    /// Admitted request ids that have not been answered yet: the only ids
    /// a cancellation notification may register (r2 F6/F19 — unknown,
    /// completed, or malformed ids are ignored, so a reused id can never
    /// be poisoned).
    outstanding: Mutex<BTreeSet<String>>,
    cancelled: Mutex<BTreeSet<String>>,
    live: AtomicUsize,
    /// Workers abandoned after their deadline: they keep executing, so
    /// they stay counted against the concurrency bound until they
    /// finish (their slot is never freed early).
    detached: AtomicUsize,
    subscriptions: Mutex<VecDeque<Subscription>>,
}

impl Session {
    /// One admitted session over the resolved catalog.
    pub fn new(
        catalog: ServerCatalog,
        limits: Limits,
        default_group: Option<String>,
        writer: Writer,
    ) -> Self {
        Self {
            catalog,
            limits,
            default_group,
            writer,
            initialized: Mutex::new(false),
            handshake: Mutex::new(false),
            outstanding: Mutex::new(BTreeSet::new()),
            cancelled: Mutex::new(BTreeSet::new()),
            live: AtomicUsize::new(0),
            detached: AtomicUsize::new(0),
            subscriptions: Mutex::new(VecDeque::new()),
        }
    }

    /// The scalar-id cancellation key of one request id; malformed ids
    /// never enter the bookkeeping (r2 F19).
    fn key_of(id: &Value) -> Option<String> {
        id_to_key(id)
    }
}

/// The incoming request, structurally validated before routing
/// (r2 F14/codex 12: full JSON-RPC 2.0 envelope validation).
struct Incoming {
    id: Value,
    method: String,
    params: Option<Value>,
}

/// Why one frame is not a servable request.
enum HeaderOutcome {
    /// A request with a validated scalar id.
    Request(Incoming),
    /// A notification: never answered, whatever its validity.
    Notification {
        method: String,
        params: Option<Value>,
    },
    /// A structurally valid request whose scalar id exceeds the
    /// correlation bound (r8 omp F6): answered with a NULL-id refusal
    /// on the session's error budget. The id is deliberately NOT carried:
    /// it is never echoed (sanitize-by-shape).
    Overlong,
    /// An invalid request envelope: answered with `-32600` when an id is
    /// known, dropped silently when the frame is a notification.
    Invalid { id: Value },
}

fn header_from_value(document: &Value) -> HeaderOutcome {
    let id_member = document.get("id");
    let scalar_id = match id_member {
        Some(Value::String(text)) => Some(Value::String(text.clone())),
        Some(Value::Number(number)) => Some(Value::Number(number.clone())),
        _ => None,
    };
    if scalar_id
        .as_ref()
        .is_some_and(|id| !super::stdio::id_admitted(id))
    {
        return HeaderOutcome::Overlong;
    }
    let method = document.get("method").and_then(Value::as_str);
    let has_id = id_member.is_some();
    match (method, has_id) {
        // A frame without a method is never a notification: the id is
        // unknowable, so the bounded error carries a null id.
        (None, _) => HeaderOutcome::Invalid {
            id: scalar_id.unwrap_or(Value::Null),
        },
        (Some(method), false) => {
            // A notification can mutate lifecycle/cancellation state only
            // after the SAME JSON-RPC 2.0 envelope admission as requests
            // (r4 F8). Invalid notifications never act.
            if document.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
                return HeaderOutcome::Invalid { id: Value::Null };
            }
            HeaderOutcome::Notification {
                method: method.to_owned(),
                params: document.get("params").cloned(),
            }
        }
        (Some(method), true) => {
            // The envelope must be exactly JSON-RPC 2.0, and the id must
            // be a scalar; explicit null ids and structured ids are
            // invalid requests (r2 F14). An admitted scalar id is ALSO
            // bounded (r8 omp F6): a valid-but-huge id would consume the
            // whole response budget in both the reply and its refusal
            // twin, so beyond the admission bound the request is refused
            // with a NULL-id `-32600` — never a
            // silently unanswered request.
            let jsonrpc_ok = document.get("jsonrpc").and_then(Value::as_str) == Some("2.0");
            if !jsonrpc_ok || scalar_id.is_none() {
                return HeaderOutcome::Invalid {
                    id: scalar_id.unwrap_or(Value::Null),
                };
            }
            if method.starts_with("notifications/") {
                // A notification carrying an id is an invalid request.
                return HeaderOutcome::Invalid {
                    id: scalar_id.unwrap_or(Value::Null),
                };
            }
            let id = scalar_id.unwrap_or(Value::Null);
            HeaderOutcome::Request(Incoming {
                id,
                method: method.to_owned(),
                params: document.get("params").cloned(),
            })
        }
    }
}

/// The cancellation key of one wire id: scalars only (r2 F19 — every
/// non-scalar id used to collapse into one shared `"other"` bucket).
fn id_to_key(id: &Value) -> Option<String> {
    match id {
        Value::String(text) => Some(format!("s:{text}")),
        Value::Number(number) => Some(format!("n:{number}")),
        _ => None,
    }
}

/// Serve one stdio session to EOF.
///
/// A dedicated reader thread feeds frames into a channel, so the
/// dispatcher observes `notifications/cancelled` (and EOF) while a tool
/// call is in flight instead of only after its reply shipped (r2 F6).
/// Requests are processed sequentially — the replies stay ordered and
/// deterministic — and the frames that arrive during a supervised call
/// are queued in arrival order behind it, bounded by the recorded queue
/// limit.
pub fn serve(session: Arc<Session>) {
    // Rendezvous channel: the reader cannot accumulate an unbounded
    // second queue while the dispatcher enforces queuedRequests=16.
    // The one frame in the reader and the bounded pending deque are the
    // only retained application frames (r4 F5). A full pending queue is
    // still actively drained for cancel/EOF priority below.
    let (frame_tx, frame_rx) = mpsc::sync_channel::<Result<String, super::stdio::FrameError>>(0);
    let reader = std::thread::Builder::new()
        .name("lekalo-mcp-reader".to_owned())
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut reader = stdin.lock();
            loop {
                match super::stdio::read_frame(&mut reader) {
                    Ok(frame) => {
                        if frame_tx.send(Ok(frame)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = frame_tx.send(Err(error));
                        if error == super::stdio::FrameError::Eof {
                            break;
                        }
                    }
                }
            }
        });
    drop(reader);
    let mut pending: VecDeque<Result<String, super::stdio::FrameError>> = VecDeque::new();
    let mut last_resource_poll = Instant::now();
    loop {
        if last_resource_poll.elapsed() >= RESOURCE_POLL {
            poll_subscription(&session, &frame_rx, &mut pending);
            last_resource_poll = Instant::now();
        }
        let frame = match pending.pop_front() {
            Some(frame) => frame,
            None => match frame_rx.recv_timeout(RESOURCE_POLL) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
        };
        let alive = match frame {
            Ok(text) => process_frame(&session, &text, &frame_rx, &mut pending),
            Err(super::stdio::FrameError::Eof) => false,
            Err(super::stdio::FrameError::OverLimit) => {
                // The oversized line was drained inside the reader; the
                // suffix can never resurface as a frame (r2 F8).
                session
                    .writer
                    .send_error(&Value::Null, INVALID_REQUEST, "mcp.request-limit");
                true
            }
            Err(_) => {
                session
                    .writer
                    .send_error(&Value::Null, PARSE_ERROR, "parse-error");
                true
            }
        };
        if !alive {
            break;
        }
    }
    // EOF shuts down outstanding work; the abandoned workers hold their
    // slots only as long as the process lives, and their receivers are
    // gone, so no late result can ship.
    log_event("eof-shutdown");
}

/// Process one frame; `false` ends the session (EOF).
fn process_frame(
    session: &Arc<Session>,
    text: &str,
    frame_rx: &mpsc::Receiver<Result<String, super::stdio::FrameError>>,
    pending: &mut VecDeque<Result<String, super::stdio::FrameError>>,
) -> bool {
    // One checked parse: duplicate keys are refused before any map
    // insertion (last-key-wins is never an admission path), then the
    // nesting depth, then the header.
    let document = match lekalo_core::mcp::parse_checked(text) {
        Ok(document) => document,
        Err(lekalo_core::mcp::ParseFailure::Duplicate) => {
            session
                .writer
                .send_error(&Value::Null, INVALID_REQUEST, "mcp.duplicate-key");
            return true;
        }
        Err(lekalo_core::mcp::ParseFailure::Malformed) => {
            session
                .writer
                .send_error(&Value::Null, PARSE_ERROR, "parse-error");
            return true;
        }
    };
    if super::stdio::check_depth(&document).is_err() {
        session
            .writer
            .send_error(&Value::Null, INVALID_REQUEST, "mcp.request-limit");
        return true;
    }
    let incoming = match header_from_value(&document) {
        HeaderOutcome::Request(incoming) => incoming,
        HeaderOutcome::Notification { method, params } => {
            handle_notification(session, &method, params);
            return true;
        }
        HeaderOutcome::Overlong => {
            // A bounded refusal ships (the id is not echoed) and the
            // session continues: the host cannot correlate on that id,
            // but the protocol outcome is explicit — never a black-
            // holed request, never a dropped session (r8 omp F6).
            return session
                .writer
                .send_error(&Value::Null, INVALID_REQUEST, "mcp.request-limit");
        }
        HeaderOutcome::Invalid { id } => {
            session
                .writer
                .send_error(&id, INVALID_REQUEST, "invalid-request");
            return true;
        }
    };
    if !session.writer.admits_id(&incoming.id) {
        return session
            .writer
            .send_error(&Value::Null, INVALID_REQUEST, "mcp.request-limit");
    }
    // The lifecycle gate: before `notifications/initialized`, the only
    // served requests are `initialize` and `ping` (hosts probe during the
    // handshake); every other method is refused.
    if !*session.initialized.lock().expect("flag")
        && !matches!(incoming.method.as_str(), "initialize" | "ping")
    {
        session.writer.send_error(
            &incoming.id,
            INVALID_REQUEST,
            "mcp.lifecycle-not-initialized",
        );
        return true;
    }
    route(session, &incoming, frame_rx, pending)
}

fn handle_notification(session: &Session, method: &str, params: Option<Value>) {
    match method {
        "notifications/initialized" => {
            // Only a session that ran `initialize` unlocks (r2 F14).
            if *session.handshake.lock().expect("flag") {
                *session.initialized.lock().expect("flag") = true;
            }
        }
        "notifications/cancelled" => {
            // Cancelled notifications themselves have no reply. The id is
            // registered only when it is outstanding (admitted and
            // unanswered); unknown, completed, or malformed ids are
            // ignored, so a cancelled-then-reused id executes normally
            // (r2 F6). The 2025-11-25 profile names the member
            // `requestId`; the legacy `id` spelling is tolerated.
            let id = params
                .as_ref()
                .and_then(|params| params.get("requestId").or_else(|| params.get("id")));
            if let Some(key) = id.and_then(Session::key_of) {
                if session.outstanding.lock().expect("flag").contains(&key) {
                    session.cancelled.lock().expect("flag").insert(key);
                }
            }
        }
        _ => {
            // Unknown notifications are ignored per the JSON-RPC rules.
        }
    }
}

/// Route one request. Instant methods answer inline; supervised methods
/// (tools/call, resources/read) run on a bounded worker while this loop
/// keeps draining cancellation notifications.
fn route(
    session: &Arc<Session>,
    incoming: &Incoming,
    frame_rx: &mpsc::Receiver<Result<String, super::stdio::FrameError>>,
    pending: &mut VecDeque<Result<String, super::stdio::FrameError>>,
) -> bool {
    match incoming.method.as_str() {
        "initialize" => {
            // Require a structurally recognized InitializeRequestParams
            // before advancing the handshake (r4 F8/F11). Negotiation
            // still answers the pinned profile; garbage cannot unlock it.
            if !valid_initialize_params(incoming.params.as_ref()) {
                session
                    .writer
                    .send_error(&incoming.id, INVALID_PARAMS, "invalid-params");
                return true;
            }
            // MCP treats `initialize` as single-shot (r3 F7): once the
            // handshake was answered, a second `initialize` is the typed
            // lifecycle violation — never a fresh result that resets the
            // session state under an initialized client.
            let already_answered = {
                let mut handshake = session.handshake.lock().expect("flag");
                let already = *handshake;
                if !already {
                    *handshake = true;
                }
                already
            };
            if already_answered {
                session.writer.send_error(
                    &incoming.id,
                    INVALID_REQUEST,
                    "mcp.lifecycle-already-initialized",
                );
            } else {
                session.writer.send_result(&incoming.id, initialize());
            }
        }
        "ping" => {
            session
                .writer
                .send_result(&incoming.id, serde_json::json!({}));
        }
        "tools/list" => {
            session.writer.send_result(&incoming.id, tools_list());
        }
        "resources/templates/list" => {
            session.writer.send_result(&incoming.id, templates_list());
        }
        "resources/list" => {
            if let Some(response) = resources_list(session, incoming) {
                session.writer.send_result(&incoming.id, response);
            }
        }
        "tools/call" => {
            tools_call(session, incoming, frame_rx, pending);
        }
        "resources/read" => {
            resources_read(session, incoming, frame_rx, pending, ResourceAction::Read);
        }
        "resources/subscribe" | "resources/unsubscribe" => {
            let action = if incoming.method == "resources/subscribe" {
                ResourceAction::Subscribe
            } else {
                ResourceAction::Unsubscribe
            };
            resources_read(session, incoming, frame_rx, pending, action);
        }
        _ => {
            session
                .writer
                .send_error(&incoming.id, METHOD_NOT_FOUND, "method-not-found");
        }
    }
    true
}

/// The MCP initialize request's required structural members. Capabilities
/// and clientInfo are objects, version/name strings, and the offered
/// protocolVersion is an explicit string (never missing or garbage).
fn valid_initialize_params(params: Option<&Value>) -> bool {
    let Some(Value::Object(params)) = params else {
        return false;
    };
    params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .is_some_and(|v| !v.is_empty())
        && params.get("capabilities").is_some_and(Value::is_object)
        && params
            .get("clientInfo")
            .and_then(Value::as_object)
            .is_some_and(|info| {
                info.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|v| !v.is_empty())
                    && info
                        .get("version")
                        .and_then(Value::as_str)
                        .is_some_and(|v| !v.is_empty())
            })
}

/// The initialize handshake: the supported profile is answered exactly;
/// only implemented capabilities are advertised. A host advertising a
/// different date gets the pinned profile back and may disconnect; no
/// universal compatibility is claimed.
fn initialize() -> Value {
    let profile = lekalo_core::mcp::version::PROTOCOL_PROFILE;
    let version = env!("CARGO_PKG_VERSION");
    serde_json::json!({
        "protocolVersion": profile,
        "capabilities": {
            "tools": {"listChanged": false},
            "resources": {"subscribe": true, "listChanged": false}
        },
        "serverInfo": {"name": "lekalo", "version": version}
    })
}

/// The exact seven read tools with their embedded closed schemas and
/// read-only annotations.
fn tools_list() -> Value {
    let tools: Vec<Value> = tools::CATALOG
        .iter()
        .map(|tool| {
            let slug = tool.slug;
            let find = |table: &'static [(&'static str, &'static [u8])]| {
                table
                    .iter()
                    .find(|(name, _)| *name == slug)
                    .and_then(|(_, bytes)| serde_json::from_slice::<Value>(bytes).ok())
                    .unwrap_or(Value::Null)
            };
            let input_schema = find(&lekalo_core::mcp::schemas::INPUT_SCHEMAS);
            let output_schema = find(&lekalo_core::mcp::schemas::OUTPUT_SCHEMAS);
            serde_json::json!({
                "name": tool.name,
                "title": tool.title,
                "description": format!("Read-only Lekalo {slug} projection over the admitted project scope; the output matches its pinned core contract."),
                "inputSchema": input_schema,
                "outputSchema": output_schema,
                "annotations": {
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "openWorldHint": false
                }
            })
        })
        .collect();
    serde_json::json!({ "tools": tools })
}

/// The outcome of one supervised worker wait.
enum Supervised<T> {
    /// The worker delivered its payload.
    Done(T),
    /// The worker panicked or died without delivering: a recoverable
    /// fault the dispatcher answers with the typed result-invalid
    /// refusal; the slot was released (r2 F18 — a dead worker never
    /// holds one).
    Fault,
    /// The deadline expired; the worker keeps running detached and keeps
    /// its concurrency slot until it finishes (F14).
    Timeout,
    /// The request was cancelled while in flight: no reply ships at all
    /// (the 2025-11-25 cancellation contract), and the worker keeps its
    /// slot until it finishes.
    Cancelled,
}

/// The payload one supervised worker hands back: its work result, or the
/// fault marker for a panicked worker (the dispatcher answers with the
/// typed result-invalid refusal, and the slot still releases — a
/// recoverable fault never leaks the bound, r2 F18).
enum WorkerPayload<T> {
    Ok(T),
    Fault,
}

/// Run one unit of work on a bounded supervised worker.
///
/// While the worker runs, this wait keeps draining the inbound channel:
/// a cancellation notification for THIS id marks the request cancelled
/// (no reply ships when the worker completes); every other frame is
/// queued in arrival order behind the current one. The concurrency slot
/// is released exactly once through the shared flag: a completed or
/// faulted outcome releases here; a timed-out or cancelled outcome
/// leaves it to the worker, which releases when it finishes (r2 F18 —
/// the `detached` counter moves only in the arm that incremented it,
/// and a dead worker never holds a slot).
fn supervise<T: Send + 'static>(
    session: &Arc<Session>,
    key: Option<String>,
    timeout_ms: u64,
    guard: WorkGuard,
    work: impl FnOnce() -> T + Send + 'static,
    frame_rx: &mpsc::Receiver<Result<String, super::stdio::FrameError>>,
    pending: &mut VecDeque<Result<String, super::stdio::FrameError>>,
) -> Supervised<T> {
    session.live.fetch_add(1, Ordering::SeqCst);
    let worker_session = Arc::clone(session);
    let worker_key = key.clone();
    let (sender, receiver) = mpsc::channel::<WorkerPayload<T>>();
    let slot_released = Arc::new(AtomicUsize::new(SLOT_BUSY));
    let worker_released = Arc::clone(&slot_released);
    let spawned = std::thread::Builder::new()
        .name("lekalo-mcp-worker".to_owned())
        .spawn(move || {
            // A panicked worker is a recoverable fault: the fault marker
            // reaches the dispatcher, which answers the typed refusal,
            // and the slot still releases below (r2 F18).
            let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
            // The bookkeeping keys leave the outstanding/cancelled sets
            // at actual completion, so a detached (timed-out) worker's
            // late completion cannot poison its id for a reuse.
            if let Some(key) = &worker_key {
                worker_session.outstanding.lock().expect("flag").remove(key);
                worker_session.cancelled.lock().expect("flag").remove(key);
            }
            let _ = sender.send(match payload {
                Ok(value) => WorkerPayload::Ok(value),
                Err(_) => WorkerPayload::Fault,
            });
            if worker_released.swap(SLOT_RELEASED, Ordering::SeqCst) == SLOT_BUSY {
                worker_session.live.fetch_sub(1, Ordering::SeqCst);
            }
        });
    if spawned.is_err() {
        // A spawn failure frees the slot it reserved.
        session.live.fetch_sub(1, Ordering::SeqCst);
        if let Some(key) = &key {
            session.outstanding.lock().expect("flag").remove(key);
        }
        return Supervised::Fault;
    }
    drop(spawned);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut cancelled = false;
    let outcome = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Supervised::<T>::Timeout;
        }
        let received = match receiver.recv_timeout(remaining.min(WAIT_POLL)) {
            Ok(payload) => payload,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                drain_while_waiting(session, key.as_deref(), &mut cancelled, frame_rx, pending);
                if cancelled {
                    break Supervised::Cancelled;
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // The worker died without delivering; release the slot
                // here (a dead worker never holds one, r2 F18).
                if slot_released.swap(SLOT_RELEASED, Ordering::SeqCst) == SLOT_BUSY {
                    session.live.fetch_sub(1, Ordering::SeqCst);
                }
                if let Some(key) = &key {
                    session.outstanding.lock().expect("flag").remove(key);
                }
                break Supervised::Fault;
            }
        };
        // The slot release is shared with the worker's own: exactly one
        // of the two wins the swap (r2 F18).
        if slot_released.swap(SLOT_RELEASED, Ordering::SeqCst) == SLOT_BUSY {
            session.live.fetch_sub(1, Ordering::SeqCst);
        }
        break if guard.should_stop() {
            // A worker may notice the deadline at a read/spawn checkpoint
            // and return before the supervisor's next poll. Its response
            // is still a timeout, never a backend-unavailable candidate.
            Supervised::Timeout
        } else {
            match received {
                WorkerPayload::Ok(value) => Supervised::Done(value),
                WorkerPayload::Fault => Supervised::Fault,
            }
        };
    };
    match outcome {
        Supervised::Timeout | Supervised::Cancelled => {
            guard.abandon(); // No new child process after abandonment.
                             // The worker keeps executing, so it keeps its slot and
                             // releases on completion; `detached` moves only here, in the
                             // arm that records the abandonment (r2 F18).
            if slot_released.load(Ordering::SeqCst) == SLOT_BUSY {
                session.detached.fetch_add(1, Ordering::SeqCst);
            }
        }
        _ => {}
    }
    if let Some(key) = &key {
        session.outstanding.lock().expect("flag").remove(key);
    }
    // A cancellation that arrived while the worker ran but after the
    // payload was received still suppresses the reply: one final drain
    // before the caller writes anything (r2 F6).
    if matches!(outcome, Supervised::Done(_)) && key.is_some() {
        let mut late_cancel = false;
        drain_while_waiting(session, key.as_deref(), &mut late_cancel, frame_rx, pending);
        if late_cancel {
            guard.abandon();
            return Supervised::Cancelled;
        }
    }
    outcome
}

/// Whether a cancellable request with this scalar id is already queued.
/// The queue is bounded, so inspecting its checked headers on demand is
/// cheap; invalid/notification frames cannot poison a reused id (r4 F6).
fn pending_has_key(
    pending: &VecDeque<Result<String, super::stdio::FrameError>>,
    key: &str,
) -> bool {
    pending.iter().any(|frame| {
        let Ok(text) = frame else { return false };
        let Ok(document) = lekalo_core::mcp::parse_checked(text) else {
            return false;
        };
        matches!(header_from_value(&document), HeaderOutcome::Request(incoming)
            if Session::key_of(&incoming.id).as_deref() == Some(key)
                && matches!(incoming.method.as_str(), "tools/call" | "resources/read"))
    })
}

/// One poll of the inbound channel while a worker runs: a validated
/// cancellation for the live or a queued id is observed immediately;
/// other frames queue only up to the advertised bound (r4 F5/F6).
fn drain_while_waiting(
    session: &Arc<Session>,
    key: Option<&str>,
    cancelled: &mut bool,
    frame_rx: &mpsc::Receiver<Result<String, super::stdio::FrameError>>,
    pending: &mut VecDeque<Result<String, super::stdio::FrameError>>,
) {
    loop {
        match frame_rx.try_recv() {
            Ok(Ok(text)) => {
                // The fast path is permitted ONLY after the same checked
                // parse and header validation as process_frame. A malformed
                // `notifications/cancelled` with an id or JSON-RPC 1.0
                // queues for -32600; it never mutates cancellation state.
                let header = lekalo_core::mcp::parse_checked(&text)
                    .ok()
                    .map(|document| header_from_value(&document));
                if let Some(HeaderOutcome::Notification { method, params }) = &header {
                    if method == "notifications/cancelled" {
                        let id = params.as_ref().and_then(|params| {
                            params.get("requestId").or_else(|| params.get("id"))
                        });
                        if let Some(wait_key) = id.and_then(Session::key_of) {
                            if Some(wait_key.as_str()) == key {
                                *cancelled = true;
                            } else if session
                                .outstanding
                                .lock()
                                .expect("flag")
                                .contains(&wait_key)
                                || pending_has_key(pending, &wait_key)
                            {
                                session.cancelled.lock().expect("flag").insert(wait_key);
                            }
                        }
                        if *cancelled {
                            return;
                        }
                        continue;
                    }
                }
                if pending.len() >= session.limits.queued_requests {
                    // Cancel and EOF retain priority. A full queue
                    // refuses each excess request on its own id rather
                    // than retaining unbounded raw frames (r4 F5).
                    if let Some(HeaderOutcome::Request(incoming)) = header {
                        session.writer.send_error(
                            &incoming.id,
                            INVALID_REQUEST,
                            "mcp.request-limit",
                        );
                    } else if let Some(HeaderOutcome::Overlong) = header {
                        session.writer.send_error(
                            &Value::Null,
                            INVALID_REQUEST,
                            "mcp.request-limit",
                        );
                    } else if let Some(HeaderOutcome::Invalid { id }) = header {
                        session
                            .writer
                            .send_error(&id, INVALID_REQUEST, "invalid-request");
                    }
                    continue;
                }
                pending.push_back(Ok(text));
            }
            Ok(Err(error)) => {
                if error == super::stdio::FrameError::Eof {
                    // A client may half-close stdin while retaining the
                    // stdout read half (including the wire harness). EOF
                    // ends admission after queued calls, not their work.
                    pending.push_back(Err(error));
                    return;
                }
                if pending.len() < session.limits.queued_requests {
                    pending.push_back(Err(error));
                } else {
                    session
                        .writer
                        .send_error(&Value::Null, INVALID_REQUEST, "mcp.request-limit");
                }
            }
            Err(mpsc::TryRecvError::Empty) | Err(mpsc::TryRecvError::Disconnected) => return,
        }
    }
}

/// The tools/call admission and execution path: structural parameter
/// violations are -32602; a known tool's semantic failure is a typed
/// tool result; timeouts are typed bounded errors; a live cancellation
/// suppresses the late result entirely.
fn tools_call(
    session: &Arc<Session>,
    incoming: &Incoming,
    frame_rx: &mpsc::Receiver<Result<String, super::stdio::FrameError>>,
    pending: &mut VecDeque<Result<String, super::stdio::FrameError>>,
) {
    let Some(params) = incoming.params.as_ref() else {
        session
            .writer
            .send_error(&incoming.id, INVALID_PARAMS, "invalid-params");
        return;
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        session
            .writer
            .send_error(&incoming.id, INVALID_PARAMS, "invalid-params");
        return;
    };
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    if tools::descriptor(name).is_none() {
        // Unknown tools are -32602; the name is a wire token and never an
        // echoed operand.
        session
            .writer
            .send_error(&incoming.id, INVALID_PARAMS, "unknown-tool");
        return;
    }
    let request = match parse_request(name, &arguments) {
        Ok(mut request) => {
            // The trusted launcher group is the disclosure ceiling: a
            // client may name exactly that group, never any other; a
            // different explicit groupId is the typed widening refusal.
            match request.apply_launch_group(session.default_group.as_deref()) {
                Ok(()) => request,
                Err(boundary) => {
                    let set = lekalo_core::mcp::diagnostic::input_invalid_set(
                        boundary.field,
                        boundary.reason,
                    );
                    let envelope = execution::refusal_envelope(
                        name,
                        request_project(&arguments).as_deref(),
                        None,
                        set,
                        "invalid",
                        // r2 F12: the contract enum spells `invalid-input`.
                        "invalid-input",
                        "mcp.input-invalid",
                    );
                    session
                        .writer
                        .send_result(&incoming.id, tool_result(&incoming.id, &envelope, true));
                    return;
                }
            }
        }
        Err(RequestError::Params(field)) => {
            session
                .writer
                .send_error(&incoming.id, INVALID_PARAMS, field.field);
            return;
        }
        Err(RequestError::Semantic(boundary)) => {
            // The typed mcp.input-invalid refusal as a tool result.
            let set =
                lekalo_core::mcp::diagnostic::input_invalid_set(boundary.field, boundary.reason);
            let envelope = execution::refusal_envelope(
                name,
                request_project(&arguments).as_deref(),
                None,
                set,
                "invalid",
                "invalid-input",
                "mcp.input-invalid",
            );
            session
                .writer
                .send_result(&incoming.id, tool_result(&incoming.id, &envelope, true));
            return;
        }
    };
    // The pre-cancelled id executes nothing and answers nothing: the
    // client cancelled it before admission (r2 F6).
    let key = Session::key_of(&incoming.id);
    if let Some(key) = &key {
        let mut cancelled = session.cancelled.lock().expect("flag");
        if cancelled.remove(key) {
            session.outstanding.lock().expect("flag").remove(key);
            drop(cancelled);
            return;
        }
    }
    // Effective limits: caller parameters may reduce, never raise.
    let timeout_ms =
        Limits::effective_timeout_ms(request.common().timeout_ms, session.limits.timeout_ms);
    // Admission: bounded live workers. The sequential read loop is the
    // queue, so admitted concurrency never exceeds the recorded bound.
    if session.live.load(Ordering::SeqCst) >= session.limits.concurrent_calls.max(1) {
        let set = lekalo_core::mcp::diagnostic::request_limit_set("concurrency");
        let envelope = execution::refusal_envelope(
            name,
            Some(request.common().project_id.as_str()),
            request.common().group_id.as_deref(),
            set,
            "denied",
            "request-limit",
            "mcp.request-limit",
        );
        session
            .writer
            .send_result(&incoming.id, tool_result(&incoming.id, &envelope, true));
        return;
    }
    if let Some(key) = &key {
        session
            .outstanding
            .lock()
            .expect("flag")
            .insert(key.clone());
    }
    let worker_session = Arc::clone(session);
    let worker_request = request.clone();
    let guard = WorkGuard::new(timeout_ms);
    let worker_guard = guard.clone();
    let outcome = supervise(
        session,
        key,
        timeout_ms,
        guard,
        move || {
            work::with_guard(worker_guard, || {
                execution::execute(&worker_session.catalog, &worker_request)
            })
        },
        frame_rx,
        pending,
    );
    finish_tool(session, incoming, name, &request, timeout_ms, outcome);
}

fn finish_tool(
    session: &Arc<Session>,
    incoming: &Incoming,
    name: &str,
    request: &tools::ToolRequest,
    timeout_ms: u64,
    outcome: Supervised<Result<execution::ToolOutput, String>>,
) {
    match outcome {
        Supervised::Cancelled => {
            // No reply ships for a cancelled request (2025-11-25).
        }
        Supervised::Done(Ok(execution::ToolOutput { envelope, is_error })) => {
            session
                .writer
                .send_result(&incoming.id, tool_result(&incoming.id, &envelope, is_error));
        }
        Supervised::Done(Err(_)) | Supervised::Fault => {
            // An execution seam fault (or a panicked worker) is the typed
            // result-invalid developer refusal; the fault text is never
            // echoed, and the slot was released (r2 F18).
            log_event("execution-fault");
            let set = lekalo_core::mcp::diagnostic::result_invalid_set("execution");
            let envelope = execution::refusal_envelope(
                name,
                Some(request.common().project_id.as_str()),
                request.common().group_id.as_deref(),
                set,
                "invalid",
                "result-invalid",
                "mcp.result-invalid",
            );
            session
                .writer
                .send_result(&incoming.id, tool_result(&incoming.id, &envelope, true));
        }
        Supervised::Timeout => {
            // Timeout: a typed bounded tool error while the client is
            // still waiting; the abandoned worker's late success has no
            // receiver left to reach.
            let set = lekalo_core::mcp::diagnostic::timeout_set(timeout_ms);
            let envelope = execution::refusal_envelope(
                name,
                Some(request.common().project_id.as_str()),
                request.common().group_id.as_deref(),
                set,
                "unavailable",
                "timeout",
                "mcp.timeout",
            );
            session
                .writer
                .send_result(&incoming.id, tool_result(&incoming.id, &envelope, true));
        }
    }
}

fn request_project(arguments: &Value) -> Option<String> {
    arguments
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The MCP tool result: `structuredContent` plus a text content block
/// carrying the same serialized JSON for compatible clients. A malformed
/// envelope is the typed `mcp.result-invalid` refusal, never a null
/// structuredContent (r2 F4).
fn tool_result(_id: &Value, envelope: &str, is_error: bool) -> Value {
    let structured: Value = match serde_json::from_str(envelope) {
        Ok(value) => value,
        Err(_) => {
            log_event("result-invalid");
            let fallback = execution::refusal_envelope(
                "lekalo_inspect",
                None,
                None,
                lekalo_core::mcp::diagnostic::result_invalid_set("envelope"),
                "invalid",
                "result-invalid",
                "mcp.result-invalid",
            );
            serde_json::from_str(&fallback).unwrap_or(Value::Null)
        }
    };
    let text = serde_json::to_string(&structured).unwrap_or_else(|_| "\"\"".to_owned());
    serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
        "isError": is_error
    })
}

/// The admitted resource identities: project- and group-qualified family
/// URIs in deterministic order; symbol identities are template-only.
///
/// Enumeration discloses exactly what `resources/read` admits (r2 F3/
/// codex 13): the identities come from the narrowed served project map,
/// and under a launch-group ceiling every whole-project family refuses on
/// read, so nothing concrete is enumerated — the list is honestly empty
/// and symbol identities stay template-addressed.
fn enumerate(session: &Session) -> (Vec<String>, String) {
    let mut uris = Vec::new();
    if session.default_group.is_none() {
        for project in session.catalog.projects.keys() {
            for family in [
                Family::ProjectSummary,
                Family::ModuleList,
                Family::TargetStatus,
                Family::DiagnosticsSummary,
            ] {
                uris.push(resources::resource_uri(project, family, None));
            }
        }
    }
    uris.sort();
    let revision = crate::mcp::snapshot::domain_digest("resources", uris.join("\n").as_bytes());
    let mut inputs = super::snapshot::Inputs::new();
    inputs.insert("resources:enumeration".to_owned(), revision);
    (uris, super::snapshot::snapshot_ref(&inputs))
}

/// resources/list with the bounded page and the snapshot-bound cursor; a
/// stale cursor refuses instead of answering. Enumeration discloses only
/// identities inside the effective launch-group scope (F4, r2 F3).
/// The stale-cursor refusal is answered on the method's own error
/// channel — `-32602` carrying the typed `mcp.snapshot-stale` token
/// (r3 F4) — because it is a parameter of THIS method that cannot be
/// served: the response never carries a tools/call-shaped envelope
/// mislabeled with a tool identity, a foreign schema discriminator, or
/// an `unknown` project scope.
fn resources_list(session: &Session, incoming: &Incoming) -> Option<Value> {
    let (uris, snapshot) = enumerate(session);
    let requested_cursor = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("cursor"))
        .and_then(Value::as_str);
    let offset = match requested_cursor {
        Some(text) => match Cursor::decode(text) {
            Some(cursor) if cursor.snapshot == snapshot => cursor.offset,
            _ => {
                session
                    .writer
                    .send_error(&incoming.id, INVALID_PARAMS, "mcp.snapshot-stale");
                return None;
            }
        },
        None => 0,
    };
    let page: Vec<&String> = uris
        .iter()
        .skip(offset)
        .take(session.limits.page_size)
        .collect();
    let items: Vec<Value> = page
        .iter()
        .map(|uri| {
            serde_json::json!({
                "uri": uri,
                "name": uri,
                "mimeType": "application/json"
            })
        })
        .collect();
    let next = offset + page.len();
    let mut response = serde_json::json!({ "resources": items });
    if next < uris.len() && !page.is_empty() {
        let cursor = Cursor {
            snapshot: snapshot.clone(),
            offset: next,
        }
        .encode();
        response["nextCursor"] = Value::from(cursor);
    }
    Some(response)
}

fn templates_list() -> Value {
    let templates: Vec<Value> = resources::templates()
        .iter()
        .map(|template| {
            serde_json::json!({
                "uriTemplate": template.uri_template,
                "name": template.name,
                "mimeType": template.mime_type
            })
        })
        .collect();
    serde_json::json!({ "resourceTemplates": templates })
}

/// Send a resource refusal on resources/read's OWN JSON-RPC error channel.
/// A tools/call-shaped envelope labeled `lekalo_inspect` is never a
/// resource result (r4 F7). The internal typed refusal still supplies a
/// closed class; only the registered `mcp.*` token survives to the wire.
fn send_resource_refusal(session: &Session, id: &Value, envelope: &str) {
    let parsed: Value = serde_json::from_str(envelope).unwrap_or(Value::Null);
    let class = parsed["failure"]["failureClass"]
        .as_str()
        .unwrap_or("result-invalid");
    let (code, token) = match class {
        "scope-denied" => (INVALID_PARAMS, "mcp.scope-denied"),
        "sensitive-output" => (INVALID_PARAMS, "mcp.sensitive-output"),
        "snapshot-stale" => (INVALID_PARAMS, "mcp.snapshot-stale"),
        "request-limit" => (INVALID_REQUEST, "mcp.request-limit"),
        "backend-unavailable" => (-32603, "mcp.backend-unavailable"),
        "timeout" => (-32603, "mcp.timeout"),
        "invalid-input" => (INVALID_PARAMS, "mcp.input-invalid"),
        _ => (-32603, "mcp.result-invalid"),
    };
    session.writer.send_error(id, code, token);
}

/// resources/read: parse, authorize, and produce the closed resource
/// envelope as application/json text — on a bounded supervised worker
/// like every tool call, so a slow resource can never run past the
/// recorded deadline (r2 F2/codex 5). Content never contains raw source,
/// dotfiles, or absolute private roots. The launch group is the enforced
/// ceiling (F4): an unqualified URI resolves under the launch group; a
/// group-qualified URI must name exactly that group; whole-project
/// families refuse under any effective group (r2 F1), and a symbol URI
/// must resolve inside the group's closure.
fn resources_read(
    session: &Arc<Session>,
    incoming: &Incoming,
    frame_rx: &mpsc::Receiver<Result<String, super::stdio::FrameError>>,
    pending: &mut VecDeque<Result<String, super::stdio::FrameError>>,
    action: ResourceAction,
) {
    if !incoming
        .params
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|params| {
            params.get("uri").is_some_and(Value::is_string)
                && params.keys().all(|key| key == "uri" || key == "_meta")
                && params.get("_meta").map_or(true, Value::is_object)
        })
    {
        session
            .writer
            .send_error(&incoming.id, INVALID_PARAMS, "invalid-resource-params");
        return;
    }
    let uri = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (mut scope, family, qualifier) = match resources::parse_uri(uri) {
        Ok(parsed) => parsed,
        Err(_) => {
            session
                .writer
                .send_error(&incoming.id, INVALID_PARAMS, "invalid-resource-uri");
            return;
        }
    };
    // The launcher's --group is the disclosure ceiling for the whole
    // resource surface too: an unqualified identity resolves under the
    // launch group, and a group-qualified identity must name exactly it.
    if let Some(launch_group) = session.default_group.as_deref() {
        match &scope.group {
            None => scope.group = Some(launch_group.to_owned()),
            Some(requested) if requested == launch_group => {}
            Some(_) => {
                let set = lekalo_core::mcp::diagnostic::scope_denied_set(
                    "group",
                    "widen-past-launch-group",
                );
                let envelope = execution::refusal_envelope(
                    "lekalo_inspect",
                    Some(&scope.project),
                    Some(launch_group),
                    set,
                    "denied",
                    "scope-denied",
                    "mcp.scope-denied",
                );
                send_resource_refusal(session, &incoming.id, &envelope);
                return;
            }
        }
    }
    // Authorization: the project must be admitted, and the effective
    // group (if any) must belong to exactly that project.
    if !session.catalog.core.admits_project(&scope.project) {
        let envelope = refused(&scope, "project", "unregistered");
        send_resource_refusal(session, &incoming.id, &envelope);
        return;
    }
    if let Some(group) = &scope.group {
        if lekalo_core::mcp::policy::ScopeDecision::Admitted
            != session.catalog.core.admits_group(&scope.project, group)
        {
            let envelope = refused(&scope, "group", "unregistered");
            send_resource_refusal(session, &incoming.id, &envelope);
            return;
        }
    }
    // Family closure (r2 F1): under an effective group the whole-project
    // families refuse exactly like their tool equivalents, and a symbol
    // URI must name a grammar-valid module inside the allow-list before
    // any read.
    if let Some(group) = &scope.group {
        let head = qualifier.as_deref().and_then(qualifier_head);
        let outside = match (&family, head.as_deref()) {
            (Family::SymbolMetadata, Some(head)) => {
                let grant_modules = session
                    .catalog
                    .core
                    .group(&scope.project, group)
                    .map(|grant| grant.modules.clone())
                    .unwrap_or_default();
                !grant_modules.contains(head)
            }
            (Family::SymbolMetadata, None) => true,
            _ => true,
        };
        if outside {
            let reason = match (&family, head.as_deref()) {
                (Family::SymbolMetadata, None) => "unresolved-module",
                _ => "requires-full-scope",
            };
            let set = lekalo_core::mcp::diagnostic::scope_denied_set("module", reason);
            let envelope = execution::refusal_envelope(
                "lekalo_inspect",
                Some(&scope.project),
                Some(group),
                set,
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            );
            send_resource_refusal(session, &incoming.id, &envelope);
            return;
        }
    }
    if action != ResourceAction::Read {
        let mut subscriptions = session.subscriptions.lock().expect("subscriptions");
        if action == ResourceAction::Unsubscribe {
            subscriptions.retain(|subscription| subscription.uri != uri);
            session
                .writer
                .send_result(&incoming.id, serde_json::json!({}));
            return;
        }
        if subscriptions
            .iter()
            .any(|subscription| subscription.uri == uri)
        {
            session
                .writer
                .send_result(&incoming.id, serde_json::json!({}));
            return;
        }
        if !session.writer.admits_resource_update(uri) {
            session
                .writer
                .send_error(&incoming.id, INVALID_REQUEST, "mcp.output-limit");
            return;
        }
        if subscriptions.len() >= lekalo_core::mcp::version::MAX_WATCHED_IDENTITIES {
            session
                .writer
                .send_error(&incoming.id, INVALID_REQUEST, "mcp.request-limit");
            return;
        }
    }
    // The pre-cancelled id executes nothing and answers nothing (r2 F6).
    let key = Session::key_of(&incoming.id);
    if let Some(key) = &key {
        let mut cancelled = session.cancelled.lock().expect("flag");
        if cancelled.remove(key) {
            session.outstanding.lock().expect("flag").remove(key);
            drop(cancelled);
            return;
        }
    }
    if session.live.load(Ordering::SeqCst) >= session.limits.concurrent_calls.max(1) {
        let set = lekalo_core::mcp::diagnostic::request_limit_set("concurrency");
        let envelope = execution::refusal_envelope(
            "lekalo_inspect",
            Some(&scope.project),
            scope.group.as_deref(),
            set,
            "denied",
            "request-limit",
            "mcp.request-limit",
        );
        send_resource_refusal(session, &incoming.id, &envelope);
        return;
    }
    if let Some(key) = &key {
        session
            .outstanding
            .lock()
            .expect("flag")
            .insert(key.clone());
    }
    let worker_session = Arc::clone(session);
    let worker_scope = scope.clone();
    let worker_qualifier = qualifier.clone();
    let guard = WorkGuard::new(session.limits.timeout_ms);
    let worker_guard = guard.clone();
    let outcome = supervise(
        session,
        key,
        session.limits.timeout_ms,
        guard,
        move || {
            work::with_guard(worker_guard, || {
                build_resource_checked(
                    &worker_session,
                    &worker_scope,
                    family,
                    worker_qualifier.as_deref(),
                )
            })
        },
        frame_rx,
        pending,
    );
    match outcome {
        Supervised::Cancelled => {}
        Supervised::Done(Ok(document)) => {
            if action == ResourceAction::Subscribe {
                let revision = resource_document_revision(&document);
                session
                    .subscriptions
                    .lock()
                    .expect("subscriptions")
                    .push_back(Subscription {
                        uri: uri.to_owned(),
                        scope,
                        family,
                        qualifier,
                        revision,
                    });
                session
                    .writer
                    .send_result(&incoming.id, serde_json::json!({}));
                return;
            }
            session.writer.send_result(
                &incoming.id,
                serde_json::json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": document
                    }]
                }),
            );
        }
        Supervised::Done(Err(refusal_envelope)) => {
            send_resource_refusal(session, &incoming.id, &refusal_envelope);
        }
        Supervised::Fault | Supervised::Timeout => {
            // A worker fault is the typed result-invalid refusal; a
            // deadline expiry is the typed timeout refusal — both bounded,
            // both carrying the registered diagnostic (codex 5).
            let (set, status, class, detail) = if matches!(outcome, Supervised::Timeout) {
                (
                    lekalo_core::mcp::diagnostic::timeout_set(session.limits.timeout_ms),
                    "unavailable",
                    "timeout",
                    "mcp.timeout",
                )
            } else {
                (
                    lekalo_core::mcp::diagnostic::result_invalid_set("resource-worker"),
                    "invalid",
                    "result-invalid",
                    "mcp.result-invalid",
                )
            };
            let envelope = execution::refusal_envelope(
                "lekalo_inspect",
                Some(&scope.project),
                scope.group.as_deref(),
                set,
                status,
                class,
                detail,
            );
            send_resource_refusal(session, &incoming.id, &envelope);
        }
    }
}

fn resource_document_revision(document: &str) -> Option<String> {
    serde_json::from_str::<Value>(document)
        .ok()?
        .get("revision")?
        .as_str()
        .map(str::to_owned)
}

/// Round-robin one admitted identity per tick. No broad filesystem watcher,
/// unbounded worker queue or raw source is introduced. Timeout/cancel and
/// concurrency ownership are exactly the resource-read supervision policy.
fn poll_subscription(
    session: &Arc<Session>,
    frame_rx: &mpsc::Receiver<Result<String, super::stdio::FrameError>>,
    pending: &mut VecDeque<Result<String, super::stdio::FrameError>>,
) {
    if session.live.load(Ordering::SeqCst) >= session.limits.concurrent_calls.max(1) {
        return;
    }
    let Some(mut subscription) = session
        .subscriptions
        .lock()
        .expect("subscriptions")
        .pop_front()
    else {
        return;
    };
    let worker_session = Arc::clone(session);
    let scope = subscription.scope.clone();
    let family = subscription.family;
    let qualifier = subscription.qualifier.clone();
    let guard = WorkGuard::new(session.limits.timeout_ms);
    let worker_guard = guard.clone();
    let outcome = supervise(
        session,
        None,
        session.limits.timeout_ms,
        guard,
        move || {
            work::with_guard(worker_guard, || {
                build_resource_checked(&worker_session, &scope, family, qualifier.as_deref())
            })
        },
        frame_rx,
        pending,
    );
    let revision = match outcome {
        Supervised::Done(Ok(document)) => resource_document_revision(&document),
        _ => None,
    };
    if revision != subscription.revision {
        // Notify invalidation as well as recovery. The client re-reads through
        // the ordinary authorized surface; a failure never leaks its operand.
        if session.writer.resource_updated(&subscription.uri) {
            subscription.revision = revision;
        }
    }
    session
        .subscriptions
        .lock()
        .expect("subscriptions")
        .push_back(subscription);
}

/// The module head of a symbol qualifier, when it is a grammar-valid
/// module id (r2 F1 — the same rule the tool surface applies).
fn qualifier_head(qualifier: &str) -> Option<String> {
    qualifier
        .split('.')
        .next()
        .filter(|head| lekalo_core::mcp::policy::admit_module_id(head).is_ok())
        .map(str::to_owned)
}

/// Build the closed resource document with the admitted-input recheck
/// (codex 7): the model and lock inputs are digested before the build and
/// re-checked after it; a drift or a failed re-check refuses with the
/// stale-snapshot diagnostic instead of answering.
fn build_resource_checked(
    session: &Session,
    scope: &UriScope,
    family: Family,
    qualifier: Option<&str>,
) -> Result<String, String> {
    let entry = session
        .catalog
        .projects
        .get(&scope.project)
        .ok_or_else(|| refused(scope, "project", "unregistered"))?;
    let allow_git = session.catalog.core.admits_git_changed_mode();
    let before = execution::admit_project_inputs(entry, allow_git)
        .ok_or_else(|| refused(scope, "inputs", "unavailable"))?;
    let document = build_resource(session, scope, family, qualifier, &before.git)?;
    let after = execution::admitted_inputs_for(entry, allow_git).ok_or_else(|| {
        execution::refusal_envelope(
            "lekalo_inspect",
            Some(&scope.project),
            scope.group.as_deref(),
            lekalo_core::mcp::diagnostic::snapshot_stale_set("model"),
            "unavailable",
            "snapshot-stale",
            "mcp.snapshot-stale",
        )
    })?;
    if before.inputs != after {
        return Err(execution::refusal_envelope(
            "lekalo_inspect",
            Some(&scope.project),
            scope.group.as_deref(),
            lekalo_core::mcp::diagnostic::snapshot_stale_set("model"),
            "unavailable",
            "snapshot-stale",
            "mcp.snapshot-stale",
        ));
    }
    Ok(document)
}

/// The resource revision: the domain digest re-spelled into the
/// contract's closed sha256-`<hex>` form.
fn resource_revision(domain: &str, bytes: &[u8]) -> String {
    let digest = crate::mcp::snapshot::domain_digest(domain, bytes);
    format!(
        "sha256-{}",
        digest.split_once(':').expect("domain prefix").1
    )
}

/// Build the closed `lekalo/mcp-resource/v0.8.0` payload for one
/// authorized read. The typed projections reuse the same command seams as
/// the tools so both surfaces stay semantically identical; the underlying
/// failures project as the family's typed refusal. The document is a
/// closed `serde_json::Value`; `kind` carries the contract enum spelling
/// (F7), never the URI family key.
fn build_resource(
    session: &Session,
    scope: &UriScope,
    family: Family,
    qualifier: Option<&str>,
    git: &lekalo_core::doctor::GitFacts,
) -> Result<String, String> {
    let entry = session
        .catalog
        .projects
        .get(&scope.project)
        .ok_or_else(|| refused(scope, "project", "unregistered"))?;
    let project = Some(entry.selector.clone());
    let (revision, data) = match family {
        Family::ProjectSummary => {
            let selection = lekalo_core::loader::LoadSelection {
                project: project.clone(),
            };
            let model = match lekalo_core::loader::normalize_model(&selection) {
                Ok(model) => model,
                Err(_) => return Err(denied_summary(scope)),
            };
            let compilation = match lekalo_core::ir::compile(&model) {
                Ok(compilation) => compilation,
                Err(_) => return Err(denied_summary(scope)),
            };
            let revision = resource_revision(
                "model",
                lekalo_core::loader::canonical_model_bytes(&model).as_bytes(),
            );
            (
                revision,
                format!(
                    "{{\"project\":\"{}\",\"modelVersion\":\"{}\",\"moduleCount\":{},\"definitionCount\":{},\"unknownStates\":[]}}",
                    scope.project,
                    model.model_version.as_str(),
                    compilation.project.modules.len(),
                    compilation.project.definitions.len(),
                ),
            )
        }
        Family::ModuleList => {
            let selection = lekalo_core::loader::LoadSelection {
                project: project.clone(),
            };
            let model = match lekalo_core::loader::normalize_model(&selection) {
                Ok(model) => model,
                Err(_) => return Err(denied_summary(scope)),
            };
            let compilation = match lekalo_core::ir::compile(&model) {
                Ok(compilation) => compilation,
                Err(_) => return Err(denied_summary(scope)),
            };
            let revision = resource_revision(
                "model",
                lekalo_core::loader::canonical_model_bytes(&model).as_bytes(),
            );
            // Definitions are counted per module from the sorted semantic
            // ids; the page is deterministic and bounded by the modules
            // the compilation itself contains.
            let mut counts: Vec<(String, usize)> = compilation
                .project
                .modules
                .iter()
                .map(|module| (module.id.as_str().to_owned(), 0))
                .collect();
            for definition in &compilation.project.definitions {
                if let Some(head) = definition.id().as_str().split('.').next() {
                    if let Some(count) = counts.iter_mut().find(|(id, _)| id == head) {
                        count.1 += 1;
                    }
                }
            }
            let modules: Vec<String> = counts
                .iter()
                .map(|(id, count)| format!("{{\"id\":\"{id}\",\"definitionCount\":{count}}}"))
                .collect();
            (revision, format!("{{\"modules\":[{}]}}", modules.join(",")))
        }
        Family::SymbolMetadata => {
            let Some(symbol) = qualifier else {
                return Err(refused(scope, "symbol", "required"));
            };
            let raw = crate::run_inspect(symbol, None, &project).to_json_string();
            let view = match execution::EnvelopeView::parse(&raw) {
                Ok(view) if view.is_valid() => view,
                _ => return Err(refused(scope, "symbol", "unknown")),
            };
            // The grouped ceiling also bounds the disclosed projection
            // (r2 F1): the symbol payload may only carry admitted-module
            // facts, resolved through the compiled graph.
            if let Some(group) = &scope.group {
                let payload = view
                    .payload_for(&execution::family_of("lekalo_inspect"))
                    .ok_or_else(|| refused(scope, "symbol", "invalid"))?;
                let payload: serde_json::Value = serde_json::from_str(payload.get())
                    .map_err(|_| refused(scope, "symbol", "invalid"))?;
                if let Some(output) = execution::disclosed_closure_refusal(
                    &session.catalog,
                    &scope.project,
                    group,
                    "lekalo_inspect",
                    &payload,
                ) {
                    return Err(output.envelope);
                }
            }
            // The raw inspect payload bytes, spliced verbatim.
            let payload = view
                .payload_for(&execution::family_of("lekalo_inspect"))
                .ok_or_else(|| refused(scope, "symbol", "invalid"))?;
            let revision = resource_revision("symbol", payload.get().as_bytes());
            (
                revision,
                format!(
                    "{{\"schemaVersion\":\"lekalo/inspect/v0.8.0\",\"identity\":\"dev.lekalo.inspect@0.8.0\",\"inspect\":{}}}",
                    payload.get()
                ),
            )
        }
        Family::TargetStatus => {
            // F9/r2 F2: the resource surface honors the same catalog-gated
            // Git admission as the tool surface — no `git` process is ever
            // reachable from a read of this family without the explicit
            // changed-mode grant.
            let result = crate::run_status_with_git(
                lekalo_core::loader::LoadSelection { project },
                git.clone(),
            );
            match result {
                result @ lekalo_core::DomainResult::Valid { .. } => {
                    let raw = result.to_json_string();
                    // The doctor document is the whole envelope.
                    let revision = resource_revision("doctor", raw.as_bytes());
                    (
                        revision,
                        format!(
                            "{{\"schemaVersion\":\"lekalo/doctor/v0.3.2\",\"identity\":\"dev.lekalo.doctor@0.3.2\",\"doctor\":{raw}}}"
                        ),
                    )
                }
                _ => return Err(refused(scope, "targets", "unavailable")),
            }
        }
        Family::DiagnosticsSummary => match crate::run_validate(project.clone(), None, false, true)
        {
            result @ lekalo_core::DomainResult::Valid { .. } => {
                let raw = result.to_json_string();
                let view: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                let count = view
                    .get("diagnostics")
                    .and_then(|diagnostics| diagnostics.as_array())
                    .map(|items| items.len())
                    .unwrap_or(0);
                let revision = resource_revision("validation", raw.as_bytes());
                (
                    revision,
                    format!(
                        "{{\"diagnosticCount\":{count},\"evidenceCount\":0,\"complete\":true,\"reasons\":[]}}"
                    ),
                )
            }
            _ => return Err(refused(scope, "diagnostics", "unavailable")),
        },
    };
    // The output screening pass runs over the resource data too (F17):
    // a secret-shaped payload refuses with `mcp.sensitive-output`.
    if lekalo_core::mcp::screen::screen_envelope(&data).is_err() {
        return Err(refused_sensitive(scope));
    }
    let data: serde_json::Value = serde_json::from_str(&data).map_err(|_| {
        let set = lekalo_core::mcp::diagnostic::result_invalid_set("resource-payload");
        execution::refusal_envelope(
            "lekalo_inspect",
            Some(&scope.project),
            scope.group.as_deref(),
            set,
            "invalid",
            "result-invalid",
            "mcp.result-invalid",
        )
    })?;
    let document = serde_json::json!({
        "schemaVersion": lekalo_core::mcp::version::RESOURCE_SCHEMA_VERSION,
        "identity": lekalo_core::mcp::version::RESOURCE_IDENTITY,
        "kind": family.contract_key(),
        "uri": resources::build_uri(scope, family, qualifier),
        "scope": {
            "projectId": scope.project,
            "groupId": scope.group
        },
        "revision": revision,
        "data": data
    });
    serde_json::to_string(&document).map_err(|_| "resource-serialize".to_owned())
}

/// The scope-refusal envelope for the resource dispatcher.
fn refused(scope: &UriScope, subject: &str, reason: &str) -> String {
    let set = lekalo_core::mcp::diagnostic::scope_denied_set(subject, reason);
    execution::refusal_envelope(
        "lekalo_inspect",
        Some(&scope.project),
        scope.group.as_deref(),
        set,
        "denied",
        "scope-denied",
        "mcp.scope-denied",
    )
}

/// The sensitive-output refusal for a resource payload: the registered
/// `mcp.sensitive-output` denial, never a redacted partial payload.
fn refused_sensitive(scope: &UriScope) -> String {
    let set =
        lekalo_core::mcp::diagnostic::sensitive_output_set("resource", "secret-shaped-output");
    execution::refusal_envelope(
        "lekalo_inspect",
        Some(&scope.project),
        scope.group.as_deref(),
        set,
        "denied",
        "sensitive-output",
        "mcp.sensitive-output",
    )
}

/// The summary refusal path before any model load.
fn denied_summary(scope: &UriScope) -> String {
    let set = lekalo_core::mcp::diagnostic::backend_unavailable_set("model");
    execution::refusal_envelope(
        "lekalo_inspect",
        Some(&scope.project),
        scope.group.as_deref(),
        set,
        "unavailable",
        "backend-unavailable",
        "mcp.backend-unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn session() -> (Arc<Session>, Arc<Mutex<Vec<u8>>>) {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let writer = Writer::new(Box::new(Sink(Arc::clone(&bytes))), 1024 * 1024);
        (
            Arc::new(Session::new(
                ServerCatalog::single(".", Path::new(".")),
                Limits::default(),
                None,
                writer,
            )),
            bytes,
        )
    }

    #[test]
    fn the_advertised_queue_bound_refuses_excess_frames_on_their_own_ids() {
        let (session, sink) = session();
        let (tx, rx) = mpsc::channel();
        let mut pending = VecDeque::new();
        for id in 0..40 {
            tx.send(Ok(
                serde_json::json!({"jsonrpc":"2.0","id":id,"method":"ping"}).to_string(),
            ))
            .unwrap();
        }
        let mut cancelled = false;
        drain_while_waiting(&session, None, &mut cancelled, &rx, &mut pending);
        assert_eq!(pending.len(), session.limits.queued_requests);
        assert!(!cancelled);
        let output = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
        let replies: Vec<Value> = output
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(replies.len(), 24);
        assert!(replies
            .iter()
            .all(|reply| reply["error"]["message"] == "mcp.request-limit"));
        assert_eq!(replies[0]["id"], 16);
        assert_eq!(replies[23]["id"], 39);
    }

    #[test]
    fn queued_cancel_is_retained_but_a_malformed_cancel_cannot_act() {
        let (session, sink) = session();
        let (tx, rx) = mpsc::channel();
        let mut pending = VecDeque::new();
        pending.push_back(Ok(serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lekalo_inspect","arguments":{"projectId":"project","symbol":"planner.focus_task"}}}).to_string()));
        tx.send(Ok(serde_json::json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}).to_string())).unwrap();
        tx.send(Ok(r#"{"jsonrpc":"1.0","id":99,"method":"notifications/cancelled","params":{"requestId":5}}"#.to_owned())).unwrap();
        let mut cancelled = false;
        drain_while_waiting(&session, Some("n:2"), &mut cancelled, &rx, &mut pending);
        assert!(!cancelled);
        assert!(session.cancelled.lock().unwrap().contains("n:3"));
        assert!(!session.cancelled.lock().unwrap().contains("n:5"));
        assert_eq!(
            pending.len(),
            2,
            "invalid cancellation queues for ordinary validation"
        );
        let invalid = pending.pop_back().unwrap().unwrap();
        process_frame(&session, &invalid, &rx, &mut pending);
        let output = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
        assert!(output.contains("\"id\":99"));
        assert!(output.contains("invalid-request"));
    }
}
