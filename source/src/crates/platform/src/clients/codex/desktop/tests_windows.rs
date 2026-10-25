// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

fn pipe(path: &std::path::Path, first: bool) -> NamedPipeServer {
    ServerOptions::new()
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .create(path)
        .unwrap()
}
async fn read(socket: &mut NamedPipeServer) -> Value {
    let size = socket.read_u32_le().await.unwrap();
    assert!(size < 65536);
    let mut bytes = vec![0; size as usize];
    socket.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
async fn send(socket: &mut NamedPipeServer, value: Value) {
    let bytes = serde_json::to_vec(&value).unwrap();
    socket.write_u32_le(bytes.len() as u32).await.unwrap();
    socket.write_all(&bytes).await.unwrap();
}
async fn response(socket: &mut NamedPipeServer, request: &Value, result: Value) {
    send(socket, json!({"type":"response","requestId":request["requestId"],"resultType":"success","handledByClientId":"owner","result":result})).await;
}
async fn handshake(socket: &mut NamedPipeServer) {
    socket.connect().await.unwrap();
    let init = read(socket).await;
    assert_eq!(init["method"], "initialize");
    response(socket, &init, json!({"clientId":"fixture-client"})).await;
    let discovery = read(socket).await;
    assert_eq!(discovery["method"], "thread-owner-discovery");
    response(socket, &discovery, json!({})).await;
}
fn wait(stage: &str, mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !condition() {
        assert!(Instant::now() < end, "fixture deadline: {stage}");
        thread::sleep(Duration::from_millis(5));
    }
}
fn follow(observer: &CodexDesktopObserver) {
    observer.update_sessions(&[json!({"id":THREAD,"source":AGENT.id,"state":"running","navigationTarget":{"kind":"codex-thread","id":THREAD}})]);
}
fn unique_endpoint() -> PathBuf {
    PathBuf::from(format!(
        r"\\.\pipe\agenthud-observer-test-{}",
        super::super::super::desktop_io::request_id().unwrap()
    ))
}

#[test]
fn named_pipe_observer_reply_receipts_and_withdrawal() {
    for (decision, outcome) in [
        ("always", "confirmed"),
        ("deny", "confirmed"),
        ("allow", "uncertain"),
    ] {
        let endpoint = unique_endpoint();
        let server_endpoint = endpoint.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let server = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                tokio::time::timeout(Duration::from_secs(12), async move {
                    let mut stream = pipe(&server_endpoint, true);
                    ready_tx.send(()).unwrap();
                    handshake(&mut stream).await;
                    assert_eq!(read(&mut stream).await["params"]["following"], true);
                    // Create the answer instance only after the stream has connected:
                    // Windows may select any available instance of the pipe name.
                    let mut answer = pipe(&server_endpoint, false);
                    let mut state = snapshot(vec![permission("computer-use", json!(["session","always"]))]);
                    state["threadRuntimeStatus"] = json!({"type":"active","activeFlags":["waitingOnApproval"]});
                    send(&mut stream, json!({"type":"broadcast","method":"thread-stream-state-changed","version":11,"sourceClientId":"owner","params":{"hostId":"local","conversationId":THREAD,"change":{"type":"snapshot","revision":1,"conversationState":state}}})).await;
                    handshake(&mut answer).await;
                    let mutation = read(&mut answer).await;
                    assert_eq!(mutation["method"], pending::ELICITATION_METHOD);
                    assert_eq!(mutation["targetClientId"], "owner");
                    assert_eq!(mutation["params"]["requestId"], 7);
                    if decision == "deny" {
                        assert_eq!(mutation["params"]["response"]["action"], "decline");
                    } else {
                        assert_eq!(mutation["params"]["response"]["_meta"]["persist"], if decision == "always" {"always"} else {"session"});
                    }
                    if outcome == "uncertain" { drop(answer); }
                    else { response(&mut answer, &mutation, json!({"method":pending::ELICITATION_METHOD,"result":{"ok":true}})).await; }
                    finish_rx.await.unwrap();
                    send(&mut stream, json!({"type":"broadcast","method":"thread-stream-state-changed","version":11,"sourceClientId":"owner","params":{"hostId":"local","conversationId":THREAD,"change":{"type":"patches","baseRevision":1,"revision":2,"patches":[{"op":"replace","path":["requests"],"value":[]}]}}})).await;
                    // Keep stream alive until the observer processes the patch and stops.
                    let mut byte = [0];
                    let _ = stream.read(&mut byte).await;
                }).await.expect("named pipe fixture deadline");
            });
        });
        ready_rx.recv_timeout(Duration::from_secs(4)).unwrap();
        let requests = Requests::default();
        let mut observer =
            CodexDesktopObserver::start_at_endpoint(endpoint, requests.clone(), Arc::new(|| {}))
                .unwrap();
        follow(&observer);
        wait("visible request", || {
            requests.views(crate::hooks::now_ms()).len() == 1
        });
        assert!(requests
            .native_waiting_sessions()
            .get(AGENT.id)
            .is_some_and(|ids| ids.contains(THREAD)));
        let id = requests.views(crate::hooks::now_ms())[0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        requests
            .answer(&id, &json!({"kind":decision}), crate::hooks::now_ms())
            .unwrap();
        wait("delivery receipt", || {
            requests
                .deliveries(crate::hooks::now_ms())
                .iter()
                .any(|row| row["id"] == id && row["state"] == outcome)
        });
        assert!(
            requests
                .answer(&id, &json!({"kind":"allow"}), crate::hooks::now_ms())
                .is_err(),
            "never retry an answered identity"
        );
        finish_tx.send(()).unwrap();
        wait("owner settlement", || {
            observer.shared.state.lock().unwrap().pending.is_empty()
        });
        observer.stop();
        assert!(requests.views(crate::hooks::now_ms()).is_empty());
        assert!(requests
            .native_waiting_sessions()
            .get(AGENT.id)
            .is_none_or(|ids| ids.is_empty()));
        server.join().unwrap();
    }
}

#[test]
fn named_pipe_observer_stop_interrupts_partial_frame() {
    let endpoint = unique_endpoint();
    let server_endpoint = endpoint.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (partial_tx, partial_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
    let server = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            tokio::time::timeout(Duration::from_secs(8), async move {
                let mut stream = pipe(&server_endpoint, true);
                ready_tx.send(()).unwrap();
                handshake(&mut stream).await;
                assert_eq!(read(&mut stream).await["params"]["following"], true);
                stream.write_u32_le(4096).await.unwrap();
                stream.write_all(b"{").await.unwrap();
                partial_tx.send(()).unwrap();
                finish_rx.await.unwrap();
            })
            .await
            .unwrap();
        });
    });
    ready_rx.recv_timeout(Duration::from_secs(4)).unwrap();
    let mut observer =
        CodexDesktopObserver::start_at_endpoint(endpoint, Requests::default(), Arc::new(|| {}))
            .unwrap();
    follow(&observer);
    partial_rx.recv_timeout(Duration::from_secs(4)).unwrap();
    let began = Instant::now();
    observer.stop();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "stop must cancel a frame with a 120s read deadline"
    );
    assert!(observer.shared.state.lock().unwrap().connection.is_none());
    finish_tx.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn disconnected_answer_stays_dismissed_until_owner_settles() {
    // Exercise replay reconciliation without ever connecting to a production pipe.
    let requests = Requests::default();
    let mut observer = CodexDesktopObserver {
        shared: Arc::new(Shared {
            active: AtomicBool::new(true),
            state: Mutex::new(State {
                reported: HashMap::new(),
                known: HashMap::new(),
                pending: HashMap::new(),
                connection: None,
                answer_connections: HashMap::new(),
            }),
            changed: Condvar::new(),
            requests: requests.clone(),
            wake: Arc::new(|| {}),
            endpoint: unique_endpoint(),
        }),
        worker: None,
    };
    let state = snapshot(vec![permission("computer-use", json!(["session"]))]);
    observer.shared.reconcile(THREAD, "owner", state.clone());
    let id = requests.views(crate::hooks::now_ms())[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    requests
        .answer(&id, &json!({"kind":"allow"}), crate::hooks::now_ms())
        .unwrap();
    observer.shared.forget_connection();
    observer.shared.reconcile(THREAD, "owner", state.clone());
    assert!(
        requests.views(crate::hooks::now_ms()).is_empty(),
        "reconnect must not retry a dismissed request"
    );
    observer.shared.reconcile(THREAD, "owner", snapshot(vec![]));
    observer.shared.reconcile(THREAD, "owner", state);
    assert_eq!(
        requests.views(crate::hooks::now_ms()).len(),
        1,
        "observed settlement permits a fresh request"
    );
    observer.stop();
}

#[test]
fn named_pipe_command_and_file_permissions_use_exact_desktop_routes() {
    for (files, decision, outcome) in [
        (false, "allow", "confirmed"),
        (false, "deny", "confirmed"),
        (false, "allow", "uncertain"),
        (true, "allow", "confirmed"),
        (true, "deny", "confirmed"),
        (true, "allow", "uncertain"),
    ] {
        let endpoint = unique_endpoint();
        let server_endpoint = endpoint.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let server = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                tokio::time::timeout(Duration::from_secs(12), async move {
                    let mut stream = pipe(&server_endpoint, true);
                    ready_tx.send(()).unwrap();
                    handshake(&mut stream).await;
                    assert_eq!(read(&mut stream).await["params"]["following"], true);
                    let mut answer = pipe(&server_endpoint, false);
                    let mut state = if files { file_permissions_snapshot() } else { command_snapshot() };
                    state["threadRuntimeStatus"] = json!({"type":"active","activeFlags":["waitingOnApproval"]});
                    send(&mut stream, json!({"type":"broadcast","method":"thread-stream-state-changed","version":11,"sourceClientId":"owner","params":{"hostId":"local","conversationId":THREAD,"change":{"type":"snapshot","revision":1,"conversationState":state}}})).await;
                    handshake(&mut answer).await;
                    let mutation = read(&mut answer).await;
                    let method = if files { pending::PERMISSIONS_METHOD } else { pending::COMMAND_METHOD };
                    assert_eq!(mutation["method"], method);
                    assert_eq!(mutation["targetClientId"], "owner");
                    assert_eq!(mutation["params"]["conversationId"], THREAD);
                    assert_eq!(mutation["params"]["requestId"], if files {"files-fixture"} else {"command-fixture"});
                    if files {
                        assert_eq!(mutation["params"]["response"]["scope"], "turn");
                        let expected = if decision == "allow" {file_permissions_snapshot()["requests"][0]["params"]["permissions"].clone()} else {json!({})};
                        assert_eq!(mutation["params"]["response"]["permissions"], expected);
                        assert!(mutation["params"].get("decision").is_none());
                    } else {
                        assert_eq!(mutation["params"]["decision"], if decision == "allow" {"accept"} else {"decline"});
                        assert!(mutation["params"].get("response").is_none());
                    }
                    if outcome == "uncertain" { drop(answer); }
                    else { response(&mut answer, &mutation, json!({"method":method,"result":{"ok":true}})).await; }
                    finish_rx.await.unwrap();
                    send(&mut stream, json!({"type":"broadcast","method":"thread-stream-state-changed","version":11,"sourceClientId":"owner","params":{"hostId":"local","conversationId":THREAD,"change":{"type":"patches","baseRevision":1,"revision":2,"patches":[{"op":"replace","path":["requests"],"value":[]}]}}})).await;
                    let mut byte = [0];
                    let _ = stream.read(&mut byte).await;
                }).await.expect("command approval pipe fixture deadline");
            });
        });
        ready_rx.recv_timeout(Duration::from_secs(4)).unwrap();
        let requests = Requests::default();
        let mut observer =
            CodexDesktopObserver::start_at_endpoint(endpoint, requests.clone(), Arc::new(|| {}))
                .unwrap();
        follow(&observer);
        wait("command card", || {
            requests.views(crate::hooks::now_ms()).len() == 1
        });
        let id = requests.views(crate::hooks::now_ms())[0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(requests
            .answer(&id, &json!({"kind":"always"}), crate::hooks::now_ms())
            .is_err());
        requests
            .answer(&id, &json!({"kind":decision}), crate::hooks::now_ms())
            .unwrap();
        wait("command receipt", || {
            requests
                .deliveries(crate::hooks::now_ms())
                .iter()
                .any(|row| row["id"] == id && row["state"] == outcome)
        });
        observer.shared.forget_connection();
        observer.shared.reconcile(
            THREAD,
            "owner",
            if files {
                file_permissions_snapshot()
            } else {
                command_snapshot()
            },
        );
        assert!(
            requests.views(crate::hooks::now_ms()).is_empty(),
            "a reconnect must not replay the one-shot decision"
        );
        assert!(requests
            .answer(&id, &json!({"kind":"allow"}), crate::hooks::now_ms())
            .is_err());
        finish_tx.send(()).unwrap();
        wait("command withdrawal", || {
            observer.shared.state.lock().unwrap().pending.is_empty()
        });
        observer.stop();
        assert!(requests.views(crate::hooks::now_ms()).is_empty());
        server.join().unwrap();
    }
}
