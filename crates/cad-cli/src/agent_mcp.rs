//! Local MCP transport. Recipe authority remains in captured source, never the viewer.
use crate::{
    agent_jobs::AgentJobs,
    source::{self, Snapshot},
};
use cap_std::{ambient_authority, fs::Dir};
use futures_util::StreamExt;
use rmcp::{
    model::*,
    service::{RequestContext, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::async_rw::JsonRpcMessageCodec,
    ErrorData, RoleServer, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio_util::codec::{FramedRead, FramedWrite};
const MAX_FRAME: usize = 16 * 1024 * 1024;
const MAX_RESPONSE: usize = 1024 * 1024;
const MAX_SNAPSHOTS: usize = 32;
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
struct State {
    workspace: Dir,
    snapshots: BTreeMap<String, Snapshot>,
    jobs: AgentJobs,
}
#[derive(Clone)]
struct CadServer {
    state: Arc<Mutex<State>>,
    gate: Arc<tokio::sync::Semaphore>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    recipe: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revise {
    snapshot_id: String,
    source: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadSource {
    snapshot_id: String,
    #[serde(default)]
    offset: usize,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evaluate {
    snapshot_id: String,
    mode: String,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    params: BTreeMap<String, Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    job_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArtifact {
    job_id: String,
    name: String,
    #[serde(default)]
    offset: usize,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Verify {
    job_id: String,
    manifest_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capabilities {
    operation: Option<String>,
    object_type: Option<String>,
}
fn decode<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| e.to_string())
}
fn schema(properties: Value, required: &[&str]) -> serde_json::Map<String, Value> {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}).as_object().unwrap().clone()
}
fn tools() -> Vec<Tool> {
    let s = json!({"type":"string"});
    vec![
        Tool::new("cad_capabilities", "Discover native operations, schemas, limits, and executor identity.", schema(json!({"operation":s,"object_type":s}), &[])),
        Tool::new("cad_snapshot", "Capture a confined workspace-relative recipe and its locked modules into an immutable session snapshot.", schema(json!({"recipe":s}), &["recipe"])),
        Tool::new("cad_read_source", "Read the authoritative captured root source in bounded UTF-8 chunks.", schema(json!({"snapshot_id":s,"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":1,"maximum":65536}}), &["snapshot_id","max_bytes"])),
        Tool::new("cad_revise", "Create a candidate root-source revision from an existing snapshot. Preserve locked modules. Does not modify workspace files.", schema(json!({"snapshot_id":s,"source":{"type":"string","maxLength":1048576}}), &["snapshot_id","source"])),
        Tool::new("cad_evaluate", "Compile, inspect, or build a snapshot in a bounded private worker. Geometry errors are structured results. Build returns an immutable candidate, never replaces accepted output.", schema(json!({"snapshot_id":s,"mode":{"type":"string","enum":["compile","inspect","build"]},"outputs":{"type":"array","items":s,"maxItems":64},"params":{"type":"object","additionalProperties":{"type":"number"},"maxProperties":128}}), &["snapshot_id","mode"])),
        Tool::new("cad_jobs", "List retained session jobs, including completed results whose client response was lost or cancelled.", schema(json!({}), &[])),
        Tool::new("cad_job", "Read a completed session job report.", schema(json!({"job_id":s}), &["job_id"])),
        Tool::new("cad_read_artifact", "Read a bounded UTF-8 artifact chunk from a job's verified inventory. Offset and length are bytes.", schema(json!({"job_id":s,"name":s,"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":1,"maximum":65536}}), &["job_id","name","max_bytes"])),
        Tool::new("cad_verify", "Verify a candidate against the expected manifest digest.", schema(json!({"job_id":s,"manifest_digest":s}), &["job_id","manifest_digest"])),
    ].into_iter().map(|mut t| { let mut a=ToolAnnotations::default(); a.destructive_hint=Some(false); a.open_world_hint=Some(false); t.annotations=Some(a); t }).collect()
}
impl State {
    fn remember(&mut self, snapshot: Snapshot, parent: Option<&str>) -> Result<Value, String> {
        let bytes = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?;
        let id = blake3::hash(&bytes).to_hex().to_string();
        if !self.snapshots.contains_key(&id) {
            if self.snapshots.len() >= MAX_SNAPSHOTS {
                return Err("session snapshot limit reached".into());
            }
            let size: usize = self
                .snapshots
                .values()
                .map(|s| {
                    s.source.len()
                        + s.modules.values().map(String::len).sum::<usize>()
                        + s.lock_source.as_ref().map_or(0, String::len)
                })
                .sum();
            if size.saturating_add(bytes.len()) > MAX_SNAPSHOT_BYTES {
                return Err("session source byte limit reached".into());
            }
            self.snapshots.insert(id.clone(), snapshot.clone());
        }
        Ok(
            json!({"snapshot_id":id,"parent_snapshot_id":parent,"language":snapshot.language,"source_name":snapshot.source_name,"source_bytes":snapshot.source.len(),"modules":snapshot.modules.keys().collect::<Vec<_>>(),"source":if snapshot.source.len()<=65536 {Some(&snapshot.source)} else {None},"source_read_tool":"cad_read_source","authority":"immutable_session_snapshot","workspace_modified":false}),
        )
    }
    fn call(&mut self, name: &str, args: Value, cancel: Arc<AtomicBool>) -> Result<Value, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("request cancelled".into());
        }
        match name {
            "cad_capabilities" => {
                let a: Capabilities = decode(args)?;
                let mut report = self.jobs.capabilities(
                    a.operation.as_deref(),
                    a.object_type.as_deref(),
                    cancel,
                )?;
                report["agent_session"] = json!({"transport":"stdio","source_capture_platform":"linux","publication":"immutable_candidate_only","viewer":"read_only","maximum_snapshots":MAX_SNAPSHOTS,"maximum_snapshot_bytes":MAX_SNAPSHOT_BYTES,"maximum_jobs":crate::agent_jobs::MAX_JOBS,"maximum_job_bytes":crate::agent_jobs::MAX_JOB_BYTES,"maximum_retained_job_bytes":crate::agent_jobs::MAX_RETAINED_BYTES,"maximum_frame_bytes":MAX_FRAME,"maximum_response_bytes":MAX_RESPONSE,"maximum_session_frames":4096,"worker_timeout_seconds":60,"maximum_active_tools":1,"hard_os_memory_sandbox":false,"worker_process_admission":crate::worker_limits::policy(),"retention":"until_process_exit"});
                Ok(report)
            }
            "cad_snapshot" => {
                let a: Capture = decode(args)?;
                let s = source::agent_source::capture(&self.workspace, &a.recipe)
                    .map_err(|e| e.to_string())?;
                self.remember(s, None)
            }
            "cad_read_source" => {
                let a: ReadSource = decode(args)?;
                let s = &self
                    .snapshots
                    .get(&a.snapshot_id)
                    .ok_or("unknown snapshot")?
                    .source;
                if a.max_bytes == 0
                    || a.max_bytes > 65536
                    || a.offset > s.len()
                    || !s.is_char_boundary(a.offset)
                {
                    return Err("invalid source read range".into());
                }
                let mut end = a.offset.saturating_add(a.max_bytes).min(s.len());
                while !s.is_char_boundary(end) {
                    end -= 1
                }
                if end == a.offset && end < s.len() {
                    return Err("read budget cannot fit next character".into());
                }
                Ok(
                    json!({"snapshot_id":a.snapshot_id,"offset":a.offset,"next_offset":end,"total_bytes":s.len(),"text":&s[a.offset..end],"eof":end==s.len()}),
                )
            }
            "cad_revise" => {
                let a: Revise = decode(args)?;
                let s = self
                    .snapshots
                    .get(&a.snapshot_id)
                    .ok_or("unknown parent snapshot")?;
                let revised =
                    source::agent_source::revise(s, a.source).map_err(|e| e.to_string())?;
                self.remember(revised, Some(&a.snapshot_id))
            }
            "cad_evaluate" => {
                let a: Evaluate = decode(args)?;
                let s = self
                    .snapshots
                    .get(&a.snapshot_id)
                    .ok_or("unknown snapshot")?;
                let bytes = serde_json::to_string(s).map_err(|e| e.to_string())?;
                self.jobs.evaluate(
                    &a.snapshot_id,
                    &bytes,
                    &a.mode,
                    &a.outputs,
                    &a.params,
                    cancel,
                )
            }
            "cad_jobs" => {
                let _: Empty = decode(args)?;
                Ok(self.jobs.list_jobs())
            }
            "cad_job" => {
                let a: Id = decode(args)?;
                self.jobs.job(&a.job_id)
            }
            "cad_read_artifact" => {
                let a: ReadArtifact = decode(args)?;
                self.jobs
                    .read_artifact(&a.job_id, &a.name, a.offset, a.max_bytes)
            }
            "cad_verify" => {
                let a: Verify = decode(args)?;
                self.jobs.verify(&a.job_id, &a.manifest_digest, cancel)
            }
            _ => Err("unknown tool".into()),
        }
    }
}
fn result(value: Result<Value, String>) -> CallToolResponse {
    let (v, failed) = match value {
        Ok(v) => {
            let failed = v.get("worker").is_some_and(|w| w["success"] == false)
                || v.get("success") == Some(&json!(false));
            (v, failed)
        }
        Err(e) => (json!({"code":"agent_request_failed","error":e}), true),
    };
    let text = serde_json::to_string(&v).unwrap();
    if text.len() > MAX_RESPONSE {
        return result(Err(
            "Response exceeds session limit; select fewer outputs or a specific capability.".into(),
        ));
    }
    let mut r = if failed {
        CallToolResult::error(vec![ContentBlock::text(text)])
    } else {
        CallToolResult::success(vec![ContentBlock::text(text)])
    };
    r.structured_content = Some(v);
    r.into()
}
impl ServerHandler for CadServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions("Experimental original Rust headless CAD. Capture source, create candidate revisions, evaluate and inspect native evidence. Source recipes are authoritative. Jobs are session-local immutable candidates; no automatic source writes or release publication. Viewer is read-only. One active tool call; limits enforced, not an OS memory sandbox.")
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tools(),
            ..Default::default()
        })
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|t| t.name == name)
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Ok(permit) = self.gate.clone().try_acquire_owned() else {
            return Ok(result(Err(
                "worker busy; retry after current tool call completes".into(),
            )));
        };
        let state = self.state.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        struct CancelOnDrop(Arc<AtomicBool>);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let _guard = CancelOnDrop(cancel.clone());
        let mut task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            state
                .lock()
                .map_err(|_| "agent state unavailable".to_string())?
                .call(
                    &request.name,
                    Value::Object(request.arguments.unwrap_or_default()),
                    cancel,
                )
        });
        tokio::select! {
            r=&mut task=>Ok(result(r.unwrap_or_else(|e|Err(e.to_string())))),
            _=context.ct.cancelled()=>{_guard.0.store(true,Ordering::Relaxed);Ok(result(task.await.unwrap_or_else(|e|Err(e.to_string()))))}
        }
    }
}
pub fn serve(workspace: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let server = CadServer {
        state: Arc::new(Mutex::new(State {
            workspace: Dir::open_ambient_dir(workspace, ambient_authority())?,
            snapshots: BTreeMap::new(),
            jobs: AgentJobs::new().map_err(std::io::Error::other)?,
        })),
        gate: Arc::new(tokio::sync::Semaphore::new(1)),
    };
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(2)
        .enable_all()
        .build()?
        .block_on(async move {
            // Stop on malformed/oversized frames. No unbounded line buffering.
            let read = FramedRead::new(
                tokio::io::stdin(),
                JsonRpcMessageCodec::<RxJsonRpcMessage<RoleServer>>::new_with_max_length(MAX_FRAME),
            )
            .scan((BTreeSet::new(), 0usize), |state, r| {
                let message = r.ok().and_then(|m| {
                    state.1 += 1;
                    if state.1 > 4096 {
                        return None;
                    }
                    if let JsonRpcMessage::Request(ref request) = m {
                        let id = serde_json::to_string(&request.id).ok()?;
                        if id.len() > 256 || !state.0.insert(id) {
                            return None;
                        }
                    }
                    Some(m)
                });
                std::future::ready(message)
            });
            let write = FramedWrite::new(
                tokio::io::stdout(),
                JsonRpcMessageCodec::<TxJsonRpcMessage<RoleServer>>::new_with_max_length(
                    MAX_RESPONSE * 3,
                ),
            );
            let running = server.serve((write, read)).await?;
            running.waiting().await?;
            Ok::<(), Box<dyn std::error::Error>>(())
        })
}
