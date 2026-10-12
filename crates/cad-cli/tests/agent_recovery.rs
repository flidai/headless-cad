//! Real process restart and retry recovery through the local durable MCP boundary.
#![cfg(target_os = "linux")]
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
    fn start(workspace: &Path, state: &Path) -> Self {
        Self::start_binary(Path::new(env!("CARGO_BIN_EXE_cad")), workspace, state)
    }
    fn start_binary(binary: &Path, workspace: &Path, state: &Path) -> Self {
        let mut child = Command::new(binary)
            .args([
                "mcp",
                "--workspace",
                workspace.to_str().unwrap(),
                "--state-dir",
                state.to_str().unwrap(),
            ])
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
                json!({"job_id":job["job_id"],"name":name,"offset":text.len(),"max_bytes":65536}),
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
fn durable_revision_results_and_retry_identity_survive_process_death() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let source = include_str!("../../../examples/rounded_bearing_rotation.star");
    let recipe = workspace.path().join("bearing.star");
    fs::write(&recipe, source).unwrap();
    fs::write(workspace.path().join("accepted.txt"), "accepted generation").unwrap();
    let mut first = Client::start(workspace.path(), state.path());
    let original = first.tool("cad_snapshot", json!({"recipe":"bearing.star"}), false);
    let revision = source.replace(
        "name = \"bore_gap\", default = 0.15",
        "name = \"bore_gap\", default = 0.2",
    );
    assert_ne!(source, revision);
    let revised = first.tool(
        "cad_revise",
        json!({"snapshot_id":original["snapshot_id"],"source":revision}),
        false,
    );
    assert_eq!(revised["parent_snapshot_id"], original["snapshot_id"]);
    let args = json!({"snapshot_id":revised["snapshot_id"],"mode":"build","request_id":"bearing-release-1"});
    first.tool(
        "cad_evaluate",
        json!({"snapshot_id":revised["snapshot_id"],"mode":"build"}),
        true,
    );
    let built = first.tool("cad_evaluate", args.clone(), false);
    assert_eq!(built["worker"]["success"], true);
    assert_eq!(first.tool("cad_evaluate", args.clone(), false), built);
    let step = first.artifact(&built, "rounded_bearing_rotation.step");
    assert!(!step.is_empty());
    let manifest = first.artifact(&built, "manifest.json");
    let parsed: Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(
        parsed["clearance_checks"]["running_gap"]["rotation_sweep"]["proof_kind"],
        "bore_witness_orbit"
    );
    first.tool("cad_evaluate", json!({"snapshot_id":revised["snapshot_id"],"mode":"build","request_id":"bearing-release-1","params":{"required_clearance":0.3}}), true);
    assert_eq!(
        first.tool("cad_jobs", json!({}), false)["jobs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(fs::read_to_string(&recipe).unwrap(), source);
    drop(first); // Abrupt process exit after the durable result, no orderly shutdown.
    fs::remove_file(&recipe).unwrap(); // Recovery must use the captured source.
    let mut second = Client::start(workspace.path(), state.path());
    let jobs = second.tool("cad_jobs", json!({}), false);
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(
        second.tool("cad_job", json!({"job_id":built["job_id"]}), false),
        built
    );
    assert_eq!(second.tool("cad_evaluate", args, false), built);
    assert_eq!(
        second.artifact(&built, "rounded_bearing_rotation.step"),
        step
    );
    assert_eq!(second.artifact(&built, "manifest.json"), manifest);
    let captured = second.tool(
        "cad_read_source",
        json!({"snapshot_id":revised["snapshot_id"],"offset":0,"max_bytes":65536}),
        false,
    );
    assert_eq!(captured["text"], revision);
    second.tool(
        "cad_verify",
        json!({"job_id":built["job_id"],"manifest_digest":built["manifest_hash"]}),
        false,
    );
    let failed = second.tool("cad_evaluate", json!({"snapshot_id":revised["snapshot_id"],"mode":"build","request_id":"rejected-gap","params":{"required_clearance":0.3}}), true);
    assert_eq!(failed["worker"]["success"], false);
    drop(second);
    let mut third = Client::start(workspace.path(), state.path());
    assert_eq!(third.tool("cad_evaluate", json!({"snapshot_id":revised["snapshot_id"],"mode":"build","request_id":"rejected-gap","params":{"required_clearance":0.3}}), true), failed);
    assert_eq!(
        third.tool("cad_jobs", json!({}), false)["jobs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("accepted.txt")).unwrap(),
        "accepted generation"
    );
    assert!(!recipe.exists());
}

fn startup_refuses(workspace: &Path, state: &Path) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "mcp",
            "--workspace",
            workspace.to_str().unwrap(),
            "--state-dir",
            state.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(!status.success());
            let result = child.wait_with_output().unwrap();
            return String::from_utf8(result.stderr).unwrap();
        }
        if std::time::Instant::now() >= until {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("state admission must refuse promptly");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn state_admission_excludes_second_writer_other_workspace_and_symlink_root() {
    use std::os::unix::fs::symlink;
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut first = Client::start(workspace.path(), state.path());
    assert!(!startup_refuses(workspace.path(), state.path()).is_empty());
    assert_eq!(first.tool("cad_jobs", json!({}), false)["jobs"], json!([]));
    drop(first);
    assert!(!startup_refuses(other.path(), state.path()).is_empty());
    let link = other.path().join("state-link");
    symlink(state.path(), &link).unwrap();
    assert!(!startup_refuses(workspace.path(), &link).is_empty());
    assert!(!startup_refuses(workspace.path(), workspace.path()).is_empty());
}

#[test]
fn durable_artifacts_are_checked_again_after_live_or_offline_tampering() {
    use std::os::unix::fs::symlink;
    for variant in ["changed", "symlink", "hidden_extra"] {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("box.star"), "body = extrude(id=\"body\", profile=rectangle(id=\"outline\", width=10, depth=8), height=6)\noutput(name=\"box\", feature=body)\n").unwrap();
        let mut client = Client::start(workspace.path(), state.path());
        let snapshot = client.tool("cad_snapshot", json!({"recipe":"box.star"}), false);
        let job = client.tool("cad_evaluate", json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build","request_id":"tamper-case"}), false);
        let generation = state
            .path()
            .join("jobs")
            .join(job["job_id"].as_str().unwrap())
            .join("generation");
        let artifact = generation.join("box.step");
        assert!(artifact.is_file());
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, "unrelated outside data").unwrap();
        match variant {
            "changed" => fs::write(&artifact, "corrupted STEP").unwrap(),
            "symlink" => {
                fs::remove_file(&artifact).unwrap();
                symlink(&sentinel, &artifact).unwrap();
            }
            "hidden_extra" => {
                fs::write(generation.join(".tmp-unexpected"), "unsealed extra data").unwrap()
            }
            _ => unreachable!(),
        }
        if variant != "hidden_extra" {
            client.tool(
                "cad_read_artifact",
                json!({"job_id":job["job_id"],"name":"box.step","offset":0,"max_bytes":65536}),
                true,
            );
        }
        client.tool(
            "cad_verify",
            json!({"job_id":job["job_id"],"manifest_digest":job["manifest_hash"]}),
            true,
        );
        drop(client);
        assert!(!startup_refuses(workspace.path(), state.path()).is_empty());
        assert_eq!(
            fs::read_to_string(sentinel).unwrap(),
            "unrelated outside data"
        );
    }
}

#[test]
fn killed_worker_is_interrupted_and_same_request_never_reexecutes() {
    use rustix::process::{kill_process, Pid, Signal};
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let source = "def slow():\n    total = 0\n    for i in range(100000000):\n        total = total+i\n    return total\nx = slow()\nbody = extrude(id=\"body\", profile=rectangle(id=\"outline\", width=10, depth=8), height=6)\noutput(name=\"box\", feature=body)\n";
    fs::write(workspace.path().join("slow.star"), source).unwrap();
    let mut client = Client::start(workspace.path(), state.path());
    let snapshot = client.tool("cad_snapshot", json!({"recipe":"slow.star"}), false);
    let args =
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build","request_id":"crash-request"});
    client.id += 1;
    client.send(json!({"jsonrpc":"2.0","id":client.id,"method":"tools/call","params":{"name":"cad_evaluate","arguments":args}}));
    let until = std::time::Instant::now() + Duration::from_secs(5);
    let worker_pid = loop {
        let tasks = fs::read_dir(format!("/proc/{}/task", client.child.id())).unwrap();
        let found = tasks.filter_map(Result::ok).find_map(|entry| {
            fs::read_to_string(entry.path().join("children"))
                .ok()?
                .split_whitespace()
                .find_map(|s| s.parse::<i32>().ok())
        });
        if let Some(pid) = found {
            // Observe completed admission so the test does not suspend the
            // process before it can register its parent-death handler.
            let admitted = fs::read_link(format!("/proc/{pid}/cwd"))
                .is_ok_and(|cwd| cwd.join("worker-admission.json").is_file());
            if admitted && kill_process(Pid::from_raw(pid).unwrap(), Signal::STOP).is_ok() {
                break pid;
            }
        }
        assert!(
            std::time::Instant::now() < until,
            "bounded native worker was not observed"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(
        fs::read_dir(state.path().join("intents"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .count(),
        1
    );
    drop(client); // Parent death must kill even a stopped child, never leave it writing.
    let until = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let stat = fs::read_to_string(format!("/proc/{worker_pid}/stat"));
        let terminated = match stat {
            Err(_) => true,
            Ok(stat) => stat
                .rsplit_once(") ")
                .is_some_and(|(_, rest)| rest.starts_with('Z')),
        };
        if terminated {
            break;
        }
        if std::time::Instant::now() >= until {
            let _ = kill_process(Pid::from_raw(worker_pid).unwrap(), Signal::KILL);
            panic!("orphaned worker remained alive after parent death");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut recovered = Client::start(workspace.path(), state.path());
    let jobs = recovered.tool("cad_jobs", json!({}), false);
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 1);
    let retry = recovered.tool("cad_evaluate", args.clone(), true);
    assert_eq!(retry["status"], "interrupted");
    assert_eq!(retry["artifacts"], json!([]));
    assert_eq!(retry["worker"]["success"], false);
    recovered.tool("cad_accept",json!({"request_id":"interrupted-accept","job_id":retry["job_id"],"manifest_digest":"0".repeat(64),"expected_base_revision":null}),true);
    assert_eq!(
        recovered.tool("cad_release", json!({}), false)["revision"],
        Value::Null
    );
    assert_eq!(recovered.tool("cad_evaluate", args, true), retry);
    assert_eq!(
        recovered.tool("cad_jobs", json!({}), false)["jobs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("slow.star")).unwrap(),
        source
    );
}

#[test]
fn lost_delivery_recovers_full_drive_inventory_without_rebuilding() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let source = include_str!("../../../examples/drive_module_rotation.star");
    fs::write(workspace.path().join("drive.star"), source).unwrap();
    let mut client = Client::start(workspace.path(), state.path());
    let snapshot = client.tool("cad_snapshot", json!({"recipe":"drive.star"}), false);
    let args =
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build","request_id":"drive-delivery"});
    client.id += 1;
    client.send(json!({"jsonrpc":"2.0","id":client.id,"method":"tools/call","params":{"name":"cad_evaluate","arguments":args}}));
    let until = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let committed = fs::read_dir(state.path().join("receipts"))
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|x| x == "json"));
        if committed {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "drive did not commit durable receipt"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(client); // The caller never consumes the evaluation response.
    let mut recovered = Client::start(workspace.path(), state.path());
    let discovered = recovered.tool("cad_snapshots", json!({}), false);
    assert_eq!(
        discovered["snapshots"][0]["snapshot_id"],
        snapshot["snapshot_id"]
    );
    let jobs = recovered.tool("cad_jobs", json!({}), false);
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 1);
    let delivered = recovered.tool("cad_evaluate", args, false);
    assert_eq!(delivered["job_id"], jobs["jobs"][0]["job_id"]);
    assert!(delivered["artifacts"].as_array().unwrap().len() > 64);
    let manifest: Value =
        serde_json::from_str(&recovered.artifact(&delivered, "manifest.json")).unwrap();
    assert_eq!(manifest["clearance_checks"].as_object().unwrap().len(), 11);
    assert_eq!(
        manifest["rigid_rotation_groups"]["rotor"]["members"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    recovered.tool(
        "cad_verify",
        json!({"job_id":delivered["job_id"],"manifest_digest":delivered["manifest_hash"]}),
        false,
    );
    assert_eq!(
        recovered.tool("cad_jobs", json!({}), false)["jobs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("drive.star")).unwrap(),
        source
    );
}

#[test]
fn running_image_is_pinned_and_new_executable_requires_new_retry_identity() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let binary = bin.path().join("cad");
    fs::copy(env!("CARGO_BIN_EXE_cad"), &binary).unwrap();
    fs::write(workspace.path().join("box.star"), "body = extrude(id=\"body\", profile=rectangle(id=\"outline\", width=10, depth=8), height=6)\noutput(name=\"box\", feature=body)\n").unwrap();
    let mut first = Client::start_binary(&binary, workspace.path(), state.path());
    let snapshot = first.tool("cad_snapshot", json!({"recipe":"box.star"}), false);
    let args = json!({"snapshot_id":snapshot["snapshot_id"],"mode":"compile","request_id":"image-bound-request"});
    let original = first.tool("cad_evaluate", args.clone(), false);
    // A new on-disk ELF with the same code but a distinct byte identity.
    let replacement = bin.path().join("replacement");
    fs::copy(env!("CARGO_BIN_EXE_cad"), &replacement).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&replacement)
        .unwrap()
        .write_all(b"\nnew-executable-identity\n")
        .unwrap();
    // First replace the pathname with a program that cannot act as a CAD worker.
    // The live server must continue launching its already-running pinned image.
    let wrong_program = bin.path().join("wrong-program");
    fs::write(&wrong_program, b"not an executable image").unwrap();
    fs::set_permissions(&wrong_program, fs::metadata(&binary).unwrap().permissions()).unwrap();
    fs::rename(&wrong_program, &binary).unwrap();
    assert_eq!(first.tool("cad_evaluate", args.clone(), false), original);
    let new_on_old_process = first.tool("cad_evaluate", json!({"snapshot_id":snapshot["snapshot_id"],"mode":"compile","request_id":"still-running-original-image"}), false);
    assert_eq!(new_on_old_process["worker"]["success"], true);
    drop(first);
    fs::rename(&replacement, &binary).unwrap();
    let mut second = Client::start_binary(&binary, workspace.path(), state.path());
    second.tool("cad_evaluate", args, true);
    assert_eq!(
        second.tool("cad_job", json!({"job_id":original["job_id"]}), false),
        original
    );
    second.tool("cad_evaluate", json!({"snapshot_id":snapshot["snapshot_id"],"mode":"compile","request_id":"new-executable-request"}), false);
    assert_eq!(
        second.tool("cad_jobs", json!({}), false)["jobs"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn replacing_workspace_at_the_same_path_does_not_rebind_durable_state() {
    let parent = tempfile::tempdir().unwrap();
    let workspace = parent.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let state = tempfile::tempdir().unwrap();
    let first = Client::start(&workspace, state.path());
    drop(first);
    fs::rename(&workspace, parent.path().join("old-workspace")).unwrap();
    fs::create_dir(&workspace).unwrap();
    assert!(!startup_refuses(&workspace, state.path()).is_empty());
}

#[test]
fn restart_preserves_job_quota_but_allows_retrieval_at_capacity() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("box.star"), "body = extrude(id=\"body\", profile=rectangle(id=\"outline\", width=10, depth=8), height=6)\noutput(name=\"box\", feature=body)\n").unwrap();
    let mut client = Client::start(workspace.path(), state.path());
    let snapshot = client.tool("cad_snapshot", json!({"recipe":"box.star"}), false);
    let mut first = Value::Null;
    for index in 0..16 {
        let result = client.tool("cad_evaluate", json!({"snapshot_id":snapshot["snapshot_id"],"mode":"compile","request_id":format!("job-{index}")}), false);
        if index == 0 {
            first = result;
        }
    }
    drop(client);
    let mut recovered = Client::start(workspace.path(), state.path());
    assert_eq!(
        recovered.tool(
            "cad_evaluate",
            json!({"snapshot_id":snapshot["snapshot_id"],"mode":"compile","request_id":"job-0"}),
            false
        ),
        first
    );
    recovered.tool(
        "cad_evaluate",
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"compile","request_id":"job-17"}),
        true,
    );
    assert_eq!(
        recovered.tool("cad_jobs", json!({}), false)["jobs"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    assert_eq!(
        fs::read_dir(state.path().join("intents"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|x| x == "json"))
            .count(),
        16
    );
}

const ACCEPT_SOURCE: &str = "body = extrude(id=\"body\", profile=rectangle(id=\"outline\", width=10, depth=8), height=6)\noutput(name=\"box\", feature=body)\n";
fn acceptance_build(client: &mut Client, request: &str) -> Value {
    let snapshot = client.tool("cad_snapshot", json!({"recipe":"box.star"}), false);
    client.tool(
        "cad_evaluate",
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build","request_id":request}),
        false,
    )
}
fn accept_args(job: &Value, request: &str, base: Value) -> Value {
    json!({"request_id":request,"job_id":job["job_id"],"manifest_digest":job["manifest_hash"],"expected_base_revision":base})
}

#[test]
fn accepted_revisions_require_exact_base_and_recover_lost_reply() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("box.star"), ACCEPT_SOURCE).unwrap();
    let mut client = Client::start(workspace.path(), state.path());
    assert_eq!(
        client.tool("cad_release", json!({}), false),
        json!({"revision":null})
    );
    let original = acceptance_build(&mut client, "build-original");
    let first_args = accept_args(&original, "accept-original", Value::Null);
    let mut missing = first_args.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("expected_base_revision");
    client.tool("cad_accept", missing, true);
    assert_eq!(
        client.tool("cad_release", json!({}), false)["revision"],
        Value::Null
    );
    let accepted = client.tool("cad_accept", first_args.clone(), false);
    assert_eq!(accepted["accepted"], true);
    assert_eq!(accepted["revision"]["sequence"], 1);
    assert_eq!(accepted["revision"]["job_id"], original["job_id"]);
    let head = accepted["revision"]["revision_id"].clone();
    assert_eq!(head.as_str().unwrap().len(), 64);
    assert_eq!(
        client.tool("cad_release", json!({}), false)["revision"],
        accepted["revision"]
    );
    let snapshot=client.tool("cad_revise",json!({"snapshot_id":original["snapshot_id"],"source":ACCEPT_SOURCE.replace("height=6","height=7")}),false);
    let revised = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":snapshot["snapshot_id"],"mode":"build","request_id":"build-revised"}),
        false,
    );
    client.tool(
        "cad_accept",
        accept_args(&revised, "stale-sibling", Value::Null),
        true,
    );
    client.tool(
        "cad_accept",
        accept_args(&revised, "accept-original", head.clone()),
        true,
    );
    assert_eq!(
        client.tool("cad_release", json!({}), false)["revision"],
        accepted["revision"]
    );
    // Deliver the request but deliberately never consume its successful response.
    let second_args = accept_args(&revised, "accept-revised", head.clone());
    client.id += 1;
    client.send(json!({"jsonrpc":"2.0","id":client.id,"method":"tools/call","params":{"name":"cad_accept","arguments":second_args}}));
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let committed = fs::read_dir(state.path().join("acceptances"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .count();
        if committed == 2 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "acceptance did not commit"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(client);
    let mut recovered = Client::start(workspace.path(), state.path());
    let second = recovered.tool("cad_accept", second_args, false);
    assert_eq!(second["revision"]["sequence"], 2);
    assert_eq!(second["revision"]["expected_base_revision"], head);
    assert_eq!(second["revision"]["job_id"], revised["job_id"]);
    assert_eq!(recovered.tool("cad_accept", first_args, false), accepted);
    assert_eq!(
        recovered.tool("cad_release", json!({}), false)["revision"],
        second["revision"]
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("box.star")).unwrap(),
        ACCEPT_SOURCE
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 1);
}

#[test]
fn acceptance_rejects_nonbuild_failed_wrong_manifest_and_tampered_candidates() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("box.star"), ACCEPT_SOURCE).unwrap();
    let mut client = Client::start(workspace.path(), state.path());
    let good = acceptance_build(&mut client, "good");
    let compiled = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":good["snapshot_id"],"mode":"compile","request_id":"compiled"}),
        false,
    );
    let mut args = accept_args(&compiled, "reject-compile", Value::Null);
    args["manifest_digest"] = good["manifest_hash"].clone();
    client.tool("cad_accept", args, true);
    let bad=client.tool("cad_revise",json!({"snapshot_id":good["snapshot_id"],"source":"fail(\"deliberately rejected design\")"}),false);
    let failed = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":bad["snapshot_id"],"mode":"build","request_id":"failed"}),
        true,
    );
    let mut args = accept_args(&failed, "reject-failed", Value::Null);
    args["manifest_digest"] = good["manifest_hash"].clone();
    client.tool("cad_accept", args, true);
    let mut args = accept_args(&good, "reject-digest", Value::Null);
    args["manifest_digest"] = json!("0".repeat(64));
    client.tool("cad_accept", args, true);
    assert_eq!(
        client.tool("cad_release", json!({}), false)["revision"],
        Value::Null
    );
    let accepted = client.tool(
        "cad_accept",
        accept_args(&good, "accept-good", Value::Null),
        false,
    );
    let artifact = state
        .path()
        .join("jobs")
        .join(good["job_id"].as_str().unwrap())
        .join("generation/box.step");
    fs::write(artifact, "tampered").unwrap();
    client.tool(
        "cad_accept",
        accept_args(&good, "accept-good", Value::Null),
        true,
    );
    client.tool(
        "cad_accept",
        accept_args(
            &good,
            "reject-tampered",
            accepted["revision"]["revision_id"].clone(),
        ),
        true,
    );
    client.tool("cad_release", json!({}), true);
    drop(client);
    assert!(!startup_refuses(workspace.path(), state.path()).is_empty());
}

#[test]
fn broken_acceptance_chain_refuses_recovery() {
    use std::os::unix::fs::symlink;
    for variant in ["missing_predecessor", "truncated", "symlink"] {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("box.star"), ACCEPT_SOURCE).unwrap();
        let mut client = Client::start(workspace.path(), state.path());
        let job = acceptance_build(&mut client, "build");
        let first = client.tool("cad_accept", accept_args(&job, "one", Value::Null), false);
        client.tool(
            "cad_accept",
            accept_args(&job, "two", first["revision"]["revision_id"].clone()),
            false,
        );
        drop(client);
        let mut records: Vec<_> = fs::read_dir(state.path().join("acceptances"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        records.sort();
        assert_eq!(records.len(), 2);
        match variant {
            "missing_predecessor" => fs::remove_file(&records[0]).unwrap(),
            "truncated" => fs::write(&records[1], "{").unwrap(),
            "symlink" => {
                fs::remove_file(&records[1]).unwrap();
                symlink(&records[0], &records[1]).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            !startup_refuses(workspace.path(), state.path()).is_empty(),
            "{variant}"
        );
    }
}

#[test]
fn acceptance_replay_survives_executable_upgrade_and_records_new_verifier() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let binary = bin.path().join("cad");
    fs::copy(env!("CARGO_BIN_EXE_cad"), &binary).unwrap();
    fs::write(workspace.path().join("box.star"), ACCEPT_SOURCE).unwrap();
    let mut client = Client::start_binary(&binary, workspace.path(), state.path());
    let job = acceptance_build(&mut client, "old-build");
    let args = accept_args(&job, "old-accept", Value::Null);
    let accepted = client.tool("cad_accept", args.clone(), false);
    drop(client);
    fs::OpenOptions::new()
        .append(true)
        .open(&binary)
        .unwrap()
        .write_all(b"\nnew-acceptance-verifier-identity\n")
        .unwrap();
    let mut recovered = Client::start_binary(&binary, workspace.path(), state.path());
    assert_eq!(recovered.tool("cad_accept", args, false), accepted);
    let new = recovered.tool(
        "cad_accept",
        accept_args(
            &job,
            "new-accept",
            accepted["revision"]["revision_id"].clone(),
        ),
        false,
    );
    assert_eq!(
        new["revision"]["build_executable_digest"],
        accepted["revision"]["build_executable_digest"]
    );
    assert_ne!(
        new["revision"]["verifier_executable_digest"],
        accepted["revision"]["verifier_executable_digest"]
    );
}

#[test]
fn acceptance_quota_survives_restart_and_preserves_idempotent_replay() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("box.star"), ACCEPT_SOURCE).unwrap();
    let mut client = Client::start(workspace.path(), state.path());
    let job = acceptance_build(&mut client, "quota-build");
    let mut head = Value::Null;
    let mut first = Value::Null;
    for index in 0..64 {
        let accepted = client.tool(
            "cad_accept",
            accept_args(&job, &format!("accept-{index}"), head),
            false,
        );
        head = accepted["revision"]["revision_id"].clone();
        if index == 0 {
            first = accepted;
        }
    }
    drop(client);
    let mut recovered = Client::start(workspace.path(), state.path());
    assert_eq!(
        recovered.tool("cad_release", json!({}), false)["revision"]["sequence"],
        64
    );
    assert_eq!(
        recovered.tool(
            "cad_accept",
            accept_args(&job, "accept-0", Value::Null),
            false
        ),
        first
    );
    recovered.tool(
        "cad_accept",
        accept_args(&job, "accept-65", head.clone()),
        true,
    );
    assert_eq!(
        recovered.tool("cad_release", json!({}), false)["revision"]["revision_id"],
        head
    );
    assert_eq!(
        fs::read_dir(state.path().join("acceptances"))
            .unwrap()
            .count(),
        64
    );
}
