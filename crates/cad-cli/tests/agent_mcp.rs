//! A real external JSON-RPC client exercises the stdio agent boundary.
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: Receiver<Result<Value, String>>,
    id: u64,
}
impl Client {
    fn start(workspace: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cad"))
            .args(["mcp", "--workspace", workspace.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let value = line.map_err(|e| e.to_string()).and_then(|line| {
                    serde_json::from_str(&line)
                        .map_err(|e| format!("nonprotocol stdout: {e}: {line}"))
                });
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            replies,
            id: 0,
        };
        let initialized = client.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"cad-acceptance-client","version":"1"}}));
        assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
        assert!(initialized["result"]["capabilities"]["tools"].is_object());
        client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        client
    }
    fn send(&mut self, value: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        serde_json::to_writer(&mut *stdin, &value).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.send(json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params}));
        loop {
            let response = self
                .replies
                .recv_timeout(Duration::from_secs(85))
                .expect("MCP response timeout or unexpected EOF")
                .expect("invalid JSON-RPC stdout");
            if response.get("id").is_none() {
                continue;
            }
            assert_eq!(response["id"], self.id);
            assert_eq!(response["jsonrpc"], "2.0");
            return response;
        }
    }
    fn tool(&mut self, name: &str, args: Value, expect_error: bool) -> Value {
        let response = self.request("tools/call", json!({"name":name,"arguments":args}));
        assert!(response.get("error").is_none(), "{response}");
        let result = &response["result"];
        assert_eq!(
            result["isError"].as_bool().unwrap_or(false),
            expect_error,
            "{response}"
        );
        let structured = result["structuredContent"].clone();
        assert!(structured.is_object(), "{response}");
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, structured);
        structured
    }
    fn artifact(&mut self, job: &Value, name: &str) -> String {
        let entry = job["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .unwrap_or_else(|| panic!("missing {name} in {}", job["artifacts"]));
        let mut text = String::new();
        loop {
            let chunk = self.tool(
                "cad_read_artifact",
                json!({"job_id":job["job_id"],"name":name,"offset":text.len(),"max_bytes":4096}),
                false,
            );
            assert_eq!(chunk["offset"], text.len());
            assert_eq!(chunk["blake3"], entry["blake3"]);
            assert_eq!(chunk["total_bytes"], entry["bytes"]);
            text.push_str(chunk["text"].as_str().unwrap());
            assert_eq!(chunk["next_offset"], text.len());
            if chunk["eof"] == true {
                break;
            }
        }
        assert_eq!(text.len() as u64, entry["bytes"].as_u64().unwrap());
        assert_eq!(
            blake3::hash(text.as_bytes()).to_hex().as_str(),
            entry["blake3"].as_str().unwrap()
        );
        text
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn agent_proposes_diagnoses_repairs_verifies_and_replays_without_workspace_writes() {
    let workspace = tempfile::tempdir().unwrap();
    let original = include_str!("../../../examples/clearance_station.star");
    fs::write(workspace.path().join("station.star"), original).unwrap();
    fs::write(
        workspace.path().join("accepted-output.txt"),
        "previous accepted generation",
    )
    .unwrap();
    let mut client = Client::start(workspace.path());
    let listed = client.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    for name in [
        "cad_capabilities",
        "cad_snapshot",
        "cad_read_source",
        "cad_revise",
        "cad_evaluate",
        "cad_job",
        "cad_read_artifact",
        "cad_verify",
    ] {
        assert!(tools.iter().any(|tool| tool["name"] == name));
    }
    let capabilities = client.tool(
        "cad_capabilities",
        json!({"operation":"feature.extrude"}),
        false,
    );
    assert_eq!(capabilities["success"], true);
    let base = client.tool("cad_snapshot", json!({"recipe":"station.star"}), false);
    assert_eq!(base["source"], original);
    assert_eq!(base["workspace_modified"], false);
    let compiled = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":base["snapshot_id"],"mode":"compile"}),
        false,
    );
    assert_eq!(compiled["worker"]["success"], true);
    // A proposed occurrence placement leaves only 1 mm between the jaws while the 2 mm
    // engineering clearance requirement stays unchanged throughout repair.
    let proposal = original
        .replace(
            "name = \"gap\", default = 10",
            "name = \"gap\", default = 1",
        )
        .replace(
            "name = \"y_gap\", default = 5",
            "name = \"y_gap\", default = -20",
        )
        .replace(
            "name = \"z_gap\", default = 4",
            "name = \"z_gap\", default = -10",
        )
        .replace(
            "name = \"turn\", default = 1",
            "name = \"turn\", default = 0",
        );
    assert_ne!(proposal, original);
    let bad = client.tool(
        "cad_revise",
        json!({"snapshot_id":base["snapshot_id"],"source":proposal}),
        false,
    );
    assert_eq!(bad["parent_snapshot_id"], base["snapshot_id"]);
    let rejected = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":bad["snapshot_id"],"mode":"inspect","outputs":["jaw"]}),
        true,
    );
    assert_eq!(rejected["worker"]["success"], false);
    let diagnostic = &rejected["worker"]["result"];
    assert_eq!(diagnostic["code"], "clearance_check_failed");
    assert_eq!(diagnostic["check_id"], "jaw_separation");
    assert_eq!(diagnostic["output_name"], "clearance_station");
    assert!(
        diagnostic["lower_bound_mm"]
            .as_f64()
            .is_some_and(|v| v < 2.),
        "{diagnostic}"
    );
    assert_eq!(rejected["artifacts"], json!([]));
    // Repair source, preserving the rejected candidate and the baseline snapshot.
    let fixed_source = format!("# Reviewed clearance repair\n{original}");
    let fixed = client.tool(
        "cad_revise",
        json!({"snapshot_id":bad["snapshot_id"],"source":fixed_source}),
        false,
    );
    assert_eq!(fixed["parent_snapshot_id"], bad["snapshot_id"]);
    assert_ne!(fixed["snapshot_id"], bad["snapshot_id"]);
    let built = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":fixed["snapshot_id"],"mode":"build","outputs":["jaw"],"params":{}}),
        false,
    );
    assert_eq!(built["worker"]["success"], true);
    assert_eq!(built["publication"], "immutable_candidate_only");
    let manifest_text = client.artifact(&built, "manifest.json");
    assert_eq!(
        blake3::hash(manifest_text.as_bytes()).to_hex().as_str(),
        built["manifest_hash"].as_str().unwrap()
    );
    let manifest: Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(client.artifact(&built, "recipe.star"), fixed_source);
    let captured_source = client.artifact(&built, "recipe.star");
    let verified = client.tool(
        "cad_verify",
        json!({"job_id":built["job_id"],"manifest_digest":built["manifest_hash"]}),
        false,
    );
    assert_eq!(verified["success"], true);
    client.tool(
        "cad_verify",
        json!({"job_id":built["job_id"],"manifest_digest":"0".repeat(64)}),
        true,
    );
    assert_eq!(
        client.tool("cad_job", json!({"job_id":rejected["job_id"]}), true),
        rejected
    );
    assert_eq!(
        client.tool("cad_job", json!({"job_id":built["job_id"]}), false),
        built
    );
    let replay = tempfile::tempdir().unwrap();
    fs::write(replay.path().join("captured.star"), captured_source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_cad"))
        .current_dir(replay.path())
        .args([
            "build",
            "captured.star",
            "--output",
            "jaw",
            "--out-dir",
            "generation",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let replay_manifest: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        manifest["outputs"]["jaw"]["content_hash"],
        replay_manifest["outputs"]["jaw"]["content_hash"]
    );
    for name in ["jaw.brep.json", "jaw.step", "recipe.star"] {
        assert_eq!(
            client.artifact(&built, name).as_bytes(),
            fs::read(replay.path().join("generation").join(name)).unwrap()
        );
    }
    assert_eq!(
        fs::read_to_string(workspace.path().join("station.star")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("accepted-output.txt")).unwrap(),
        "previous accepted generation"
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 2);
}

#[test]
fn agent_rejects_untrusted_paths_fields_ids_and_artifact_read_budgets() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("part.star"), "output(name=\"part\", feature=extrude(id=\"body\", profile=rectangle(id=\"profile\",width=2,depth=3),height=4))\n").unwrap();
    let mut client = Client::start(workspace.path());
    for recipe in [
        "../part.star",
        "/etc/passwd",
        "missing.star",
        "part.star/child",
        "",
    ] {
        client.tool("cad_snapshot", json!({"recipe":recipe}), true);
    }
    client.tool(
        "cad_snapshot",
        json!({"recipe":"part.star","unexpected":true}),
        true,
    );
    let snapshot = client.tool("cad_snapshot", json!({"recipe":"part.star"}), false);
    client.tool(
        "cad_revise",
        json!({"snapshot_id":"stale_unknown","source":""}),
        true,
    );
    client.tool(
        "cad_evaluate",
        json!({"snapshot_id":"stale_unknown","mode":"build"}),
        true,
    );
    client.tool(
        "cad_evaluate",
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"publish"}),
        true,
    );
    client.tool(
        "cad_evaluate",
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build","out_dir":"../escape"}),
        true,
    );
    client.tool(
        "cad_evaluate",
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build","params":{"width":"--help"}}),
        true,
    );
    let built = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build"}),
        false,
    );
    for id in ["../generation", "/etc", "job_ffffffffffffffff"] {
        client.tool("cad_job", json!({"job_id":id}), true);
        client.tool(
            "cad_read_artifact",
            json!({"job_id":id,"name":"manifest.json","max_bytes":100}),
            true,
        );
    }
    for name in [
        "../manifest.json",
        "/etc/passwd",
        "..\\manifest.json",
        "unknown.json",
    ] {
        client.tool(
            "cad_read_artifact",
            json!({"job_id":built["job_id"],"name":name,"max_bytes":100}),
            true,
        );
    }
    for args in [
        json!({"max_bytes":0}),
        json!({"max_bytes":65537}),
        json!({"max_bytes":100,"offset":usize::MAX}),
        json!({"max_bytes":100,"path":"/etc/passwd"}),
    ] {
        let mut args = args.as_object().unwrap().clone();
        args.insert("job_id".into(), built["job_id"].clone());
        args.insert("name".into(), json!("manifest.json"));
        client.tool("cad_read_artifact", Value::Object(args), true);
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("part.star", workspace.path().join("linked.star")).unwrap();
        client.tool("cad_snapshot", json!({"recipe":"linked.star"}), true);
    }
    #[cfg(target_os = "linux")]
    {
        rustix::fs::mknodat(
            rustix::fs::CWD,
            workspace.path().join("pipe.star"),
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        client.tool("cad_snapshot", json!({"recipe":"pipe.star"}), true);
    }
    assert_eq!(
        client.tool("cad_job", json!({"job_id":built["job_id"]}), false),
        built
    );
}

#[test]
fn oversized_protocol_frame_closes_transport_without_waiting_for_newline() {
    let workspace = tempfile::tempdir().unwrap();
    let mut client = Client::start(workspace.path());
    let mut stdin = client.stdin.take().unwrap();
    // No newline: an unbounded read_line implementation would wait forever.
    let _ = stdin.write_all(&vec![b' '; 16 * 1024 * 1024 + 1]);
    let ended = client.replies.recv_timeout(Duration::from_secs(10));
    assert!(
        matches!(ended, Err(mpsc::RecvTimeoutError::Disconnected)),
        "transport should close on frame overflow: {ended:?}"
    );
    drop(stdin);
}

#[test]
fn large_unicode_source_is_chunked_and_snapshots_do_not_cross_sessions() {
    let workspace = tempfile::tempdir().unwrap();
    let source = format!("#é\n{}\noutput(name=\"part\",feature=extrude(id=\"body\",profile=rectangle(id=\"profile\",width=2,depth=3),height=4))\n", "# source review evidence\n".repeat(3000));
    fs::write(workspace.path().join("large.star"), &source).unwrap();
    let mut client = Client::start(workspace.path());
    let captured = client.tool("cad_snapshot", json!({"recipe":"large.star"}), false);
    assert_eq!(captured["source"], Value::Null);
    assert_eq!(captured["source_read_tool"], "cad_read_source");
    assert_eq!(captured["source_bytes"], source.len());
    let mut recovered = String::new();
    loop {
        let chunk = client.tool("cad_read_source", json!({"snapshot_id":captured["snapshot_id"],"offset":recovered.len(),"max_bytes":4096}), false);
        assert_eq!(chunk["offset"], recovered.len());
        recovered.push_str(chunk["text"].as_str().unwrap());
        assert_eq!(chunk["next_offset"], recovered.len());
        assert_eq!(chunk["total_bytes"], source.len());
        if chunk["eof"] == true {
            break;
        }
    }
    assert_eq!(recovered, source);
    for (offset, max_bytes) in [(2, 100), (1, 1), (0, 0), (0, 65537), (usize::MAX, 10)] {
        client.tool(
            "cad_read_source",
            json!({"snapshot_id":captured["snapshot_id"],"offset":offset,"max_bytes":max_bytes}),
            true,
        );
    }
    drop(client);
    let mut next_session = Client::start(workspace.path());
    next_session.tool(
        "cad_evaluate",
        json!({"snapshot_id":captured["snapshot_id"],"mode":"compile"}),
        true,
    );
    next_session.tool(
        "cad_read_source",
        json!({"snapshot_id":captured["snapshot_id"],"max_bytes":10}),
        true,
    );
}

#[test]
fn malformed_frames_repeated_and_oversized_request_ids_close_the_protocol_stream() {
    let workspace = tempfile::tempdir().unwrap();
    for case in 0..3 {
        let mut client = Client::start(workspace.path());
        if case == 0 {
            client
                .stdin
                .as_mut()
                .unwrap()
                .write_all(b"{not-json}\n")
                .unwrap();
            client.stdin.as_mut().unwrap().flush().unwrap();
        } else if case == 1 {
            // initialize already used request ID 1; reuse could confuse stale replies.
            client.send(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}));
        } else {
            // Unique-ID retention must not admit individually unbounded strings.
            client.send(
                json!({"jsonrpc":"2.0","id":"x".repeat(257),"method":"tools/list","params":{}}),
            );
        }
        let ended = client.replies.recv_timeout(Duration::from_secs(10));
        assert!(
            matches!(ended, Err(mpsc::RecvTimeoutError::Disconnected)),
            "invalid frame must close transport: {ended:?}"
        );
    }
}
