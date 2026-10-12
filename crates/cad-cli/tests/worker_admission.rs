//! Native operation admission through the actual bounded stdio worker boundary.
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
    input: Option<ChildStdin>,
    replies: Receiver<Value>,
    id: u64,
}
impl Client {
    fn new(workspace: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cad"))
            .args(["mcp", "--workspace", workspace.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (sender, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value = serde_json::from_str(&line.unwrap()).unwrap();
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            replies,
            id: 0,
        };
        let response=client.request("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"worker-admission-test","version":"1"}}));
        assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
        client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        client
    }
    fn send(&mut self, v: Value) {
        let input = self.input.as_mut().unwrap();
        serde_json::to_writer(&mut *input, &v).unwrap();
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.send(json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params}));
        loop {
            let value = self
                .replies
                .recv_timeout(Duration::from_secs(85))
                .expect("bounded worker response");
            if value.get("id").is_none() {
                continue;
            }
            assert_eq!(value["id"], self.id);
            return value;
        }
    }
    fn tool(&mut self, name: &str, args: Value, failed: bool) -> Value {
        let response = self.request("tools/call", json!({"name":name,"arguments":args}));
        assert!(response.get("error").is_none(), "{response}");
        let r = &response["result"];
        assert_eq!(
            r["isError"].as_bool().unwrap_or(false),
            failed,
            "{response}"
        );
        let value = r["structuredContent"].clone();
        assert!(value.is_object(), "{response}");
        value
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
const SMALL:&str="p=rectangle(id=\"p\",width=8,depth=4)\nb=extrude(id=\"b\",profile=p,height=2)\noutput(name=\"part\",feature=b)\n";
// Native orthogonal grid classification, not a source-size/feature-count probe.
// At union_25, 26 disjoint boxes give51^3 cells and156 boundary faces:
// 132651*156 = 20693556 > the kernel's20000000 classification budget.
const OVERSIZED:&str="def model():\n    p=rectangle(id=\"p\",width=1,depth=1)\n    b=extrude(id=\"b\",profile=p,height=1)\n    result=b\n    for i in range(1,36):\n        placed=translate(id=\"at_\"+str(i),body=b,offset=[2*i,2*i,2*i])\n        result=union(id=\"union_\"+str(i),left=result,right=placed)\n    output(name=\"oversized\",feature=result)\nmodel()\n";
#[test]
fn oversized_native_operation_is_structured_preserves_candidate_and_allows_next_job() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("main.star"), SMALL).unwrap();
    let mut client = Client::new(workspace.path());
    let source = client.tool("cad_snapshot", json!({"recipe":"main.star"}), false);
    let good = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":source["snapshot_id"],"mode":"build"}),
        false,
    );
    assert_eq!(good["worker"]["success"], true);
    assert_eq!(
        good["worker"]["admission"]["effective_limits"]["address_space_bytes"]["hard"],
        2u64 * 1024 * 1024 * 1024
    );
    let revision = client.tool(
        "cad_revise",
        json!({"snapshot_id":source["snapshot_id"],"source":OVERSIZED}),
        false,
    );
    let failed = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":revision["snapshot_id"],"mode":"build"}),
        true,
    );
    assert_eq!(failed["worker"]["success"], false);
    assert_eq!(
        failed["worker"]["result"]["code"],
        "kernel_operation_failed"
    );
    assert_eq!(failed["worker"]["result"]["feature"], "union_25");
    assert!(failed["worker"]["result"]["diagnostic"]
        .as_str()
        .unwrap()
        .contains("coordinate classification exceeds operation budget"));
    assert_eq!(failed["artifacts"].as_array().unwrap().len(), 0);
    assert!(failed["manifest_hash"].is_null());
    let unchanged = client.tool("cad_job", json!({"job_id":good["job_id"]}), false);
    assert_eq!(unchanged, good);
    let verified = client.tool(
        "cad_verify",
        json!({"job_id":good["job_id"],"manifest_digest":good["manifest_hash"]}),
        false,
    );
    assert_eq!(verified["success"], true);
    let next = client.tool(
        "cad_evaluate",
        json!({"snapshot_id":source["snapshot_id"],"mode":"build"}),
        false,
    );
    assert_eq!(next["worker"]["success"], true);
    assert_ne!(next["job_id"], good["job_id"]);
    assert_eq!(next["manifest_hash"], good["manifest_hash"]);
    assert_eq!(
        fs::read_to_string(workspace.path().join("main.star")).unwrap(),
        SMALL
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 1);
}
#[test]
fn retained_drive_full_publication_runs_under_installed_hard_worker_limits() {
    let directory = tempfile::tempdir().unwrap();
    let recipe =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/drive_module_retained.star");
    let result = Command::new(env!("CARGO_BIN_EXE_cad"))
        .current_dir(directory.path())
        .args([
            "_agent-worker",
            "build",
            recipe.to_str().unwrap(),
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
    let record: Value =
        serde_json::from_slice(&fs::read(directory.path().join("worker-admission.json")).unwrap())
            .unwrap();
    assert_eq!(
        record["enforcement"],
        "process_rlimits_before_recipe_evaluation"
    );
    assert_eq!(record["effective_limits"]["core_bytes"]["hard"], 0);
    assert_eq!(
        record["effective_limits"]["address_space_bytes"]["hard"],
        2u64 * 1024 * 1024 * 1024
    );
    assert_eq!(record["effective_limits"]["cpu_seconds"]["soft"], 45);
    assert_eq!(record["effective_limits"]["cpu_seconds"]["hard"], 46);
    assert_eq!(
        record["effective_limits"]["file_bytes"]["hard"],
        128u64 * 1024 * 1024
    );
    assert_eq!(record["hard_rss_limit"], false);
    let verify = Command::new(env!("CARGO_BIN_EXE_cad"))
        .current_dir(directory.path())
        .args(["verify", "generation"])
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
}
