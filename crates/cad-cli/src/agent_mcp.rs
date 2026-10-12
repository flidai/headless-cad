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
    parents: BTreeMap<String, Option<String>>,
    durable: bool,
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
    request_id: Option<String>,
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
struct OpenGenerationStream {
    job_id: String,
    manifest_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadGenerationStream {
    stream_id: String,
    name: String,
    #[serde(default)]
    offset: usize,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseGenerationStream {
    stream_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accept {
    request_id: String,
    job_id: String,
    manifest_digest: String,
    expected_base_revision: Option<String>,
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
        Tool::new("cad_snapshot", "Capture a confined workspace-relative recipe and its locked modules into an immutable snapshot; retained across restarts only with --state-dir.", schema(json!({"recipe":s}), &["recipe"])),
        Tool::new("cad_snapshots", "List retained immutable snapshot IDs and first-observed parentage, including snapshots recovered after restart.", schema(json!({}), &[])),
        Tool::new("cad_read_source", "Read the authoritative captured root source in bounded UTF-8 chunks.", schema(json!({"snapshot_id":s,"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":1,"maximum":65536}}), &["snapshot_id","max_bytes"])),
        Tool::new("cad_revise", "Create a candidate root-source revision from an existing snapshot. Preserve locked modules. Does not modify workspace files.", schema(json!({"snapshot_id":s,"source":{"type":"string","maxLength":1048576}}), &["snapshot_id","source"])),
        Tool::new("cad_evaluate", "Compile, inspect, or build a snapshot in a bounded private worker. Geometry errors are structured results. Build returns an immutable candidate, never replaces accepted output. Durable mode requires request_id; identical retries reuse the recorded outcome, interrupted intents never rerun automatically. Ephemeral mode rejects request_id.", schema(json!({"snapshot_id":s,"mode":{"type":"string","enum":["compile","inspect","build"]},"request_id":{"type":"string","minLength":1,"maxLength":128,"description":"Durable mode only: 1..128 UTF-8 bytes, no control characters. Bind retries to identical canonical inputs and executor identity."},"outputs":{"type":"array","items":s,"maxItems":64},"params":{"type":"object","additionalProperties":{"type":"number"},"maxProperties":128}}), &["snapshot_id","mode"])),
        Tool::new("cad_jobs", "List retained jobs, including completed results whose client response was lost or cancelled.", schema(json!({}), &[])),
        Tool::new("cad_job", "Read a completed retained job report.", schema(json!({"job_id":s}), &["job_id"])),
        Tool::new("cad_read_artifact", "Read a bounded UTF-8 artifact chunk from a job's verified inventory. Offset and length are bytes.", schema(json!({"job_id":s,"name":s,"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":1,"maximum":65536}}), &["job_id","name","max_bytes"])),
        Tool::new("cad_verify", "Verify a candidate against the expected manifest digest.", schema(json!({"job_id":s,"manifest_digest":s}), &["job_id","manifest_digest"])),
        Tool::new("cad_open_generation_stream", "Open one bounded session stream for a durable verified generation. Pins the manifest digest and checks the whole job and artifact inventory. Returned chunks are bound to the opened digest; unrelated later changes are caught on close or cad_verify, not on every read. Publish downloaded data only after successful close.", schema(json!({"job_id":s,"manifest_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"}}), &["job_id","manifest_digest"])),
        Tool::new("cad_read_generation_stream", "Read a named UTF-8 artifact chunk from an open durable generation stream. Each touched block is checked against the digest captured at open; offset and max_bytes are bytes. An unrelated later change is caught on close or cad_verify. Publish downloaded data only after successful close.", schema(json!({"stream_id":s,"name":s,"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":1,"maximum":65536}}), &["stream_id","name","max_bytes"])),
        Tool::new("cad_close_generation_stream", "Close a durable generation stream and recheck the whole retained job. The token is consumed on success or failure. Only a successful sealed close completes a download for publication.", schema(json!({"stream_id":s}), &["stream_id"])),
        Tool::new("cad_release", "Read the current locally accepted candidate revision. Requires durable --state-dir. This is not a manufacturing release or external delivery.", schema(json!({}), &[])),
        Tool::new("cad_accept", "Accept a verified completed build as the next local revision only when expected_base_revision matches the current revision. Requires durable --state-dir and an idempotency request_id. Does not change recipe source or deliver a manufacturing release.", schema(json!({"request_id":{"type":"string","minLength":1,"maxLength":128},"job_id":s,"manifest_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},"expected_base_revision":{"type":["string","null"],"pattern":"^[0-9a-f]{64}$"}}), &["request_id","job_id","manifest_digest","expected_base_revision"])),
    ].into_iter().map(|mut t| {
        let mut a=ToolAnnotations::default();
        if t.name == "cad_release" || t.name == "cad_read_generation_stream" { a.read_only_hint=Some(true); }
        a.destructive_hint=Some(t.name == "cad_accept");
        a.open_world_hint=Some(false);
        t.annotations=Some(a);
        t
    }).collect()
}
impl State {
    fn remember(&mut self, snapshot: Snapshot, parent: Option<&str>) -> Result<Value, String> {
        let bytes = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?;
        let id = blake3::hash(&bytes).to_hex().to_string();
        if !self.snapshots.contains_key(&id) {
            if self.snapshots.len() >= MAX_SNAPSHOTS {
                return Err("retained snapshot limit reached".into());
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
                return Err("retained source byte limit reached".into());
            }
            let recorded_parent = if self.durable {
                self.jobs.persist_snapshot(&id, &snapshot, parent)?
            } else {
                parent.map(str::to_owned)
            };
            self.parents.insert(id.clone(), recorded_parent);
            self.snapshots.insert(id.clone(), snapshot.clone());
        }
        Ok(
            json!({"snapshot_id":id,"parent_snapshot_id":self.parents.get(&id).and_then(Option::as_deref),"parentage":"first_observed_parent","language":snapshot.language,"source_name":snapshot.source_name,"source_bytes":snapshot.source.len(),"modules":snapshot.modules.keys().collect::<Vec<_>>(),"source":if snapshot.source.len()<=65536 {Some(&snapshot.source)} else {None},"source_read_tool":"cad_read_source","authority":if self.durable {"immutable_durable_snapshot"} else {"immutable_session_snapshot"},"workspace_modified":false}),
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
                report["agent_session"] = json!({"transport":"stdio","source_capture_platform":"linux","publication":if self.durable {"immutable_candidates_and_local_expected_base_acceptance"} else {"immutable_candidate_only"},"viewer":"read_only","maximum_snapshots":MAX_SNAPSHOTS,"maximum_snapshot_bytes":MAX_SNAPSHOT_BYTES,"maximum_jobs":crate::agent_jobs::MAX_JOBS,"maximum_job_bytes":crate::agent_jobs::MAX_JOB_BYTES,"maximum_retained_job_bytes":crate::agent_jobs::MAX_RETAINED_BYTES,"maximum_acceptance_records":crate::agent_store::MAX_ACCEPTANCES,"maximum_frame_bytes":MAX_FRAME,"maximum_response_bytes":MAX_RESPONSE,"maximum_session_frames":4096,"worker_timeout_seconds":60,"maximum_active_tools":1,"hard_os_memory_sandbox":false,"worker_process_admission":crate::worker_limits::policy(),"retention":if self.durable {"durable_state_directory"} else {"until_process_exit"},"durable_state_enabled":self.durable,"request_id_required":self.durable,"request_id_max_bytes":128,"request_id_controls_allowed":false,"evaluate_request_id":if self.durable {"required; same payload reuses recorded outcome; interrupted intents require a new key"} else {"unsupported"},"snapshot_parentage":"first_observed_parent", "release_publication":if self.durable {"local_expected_base_only"} else {"unavailable"},"manufacturing_release":false,"external_delivery":false});
                report["agent_session"]["generation_stream"] = json!({
                    "available":self.durable,
                    "maximum_open_streams":crate::agent_jobs::MAX_OPEN_STREAMS,
                    "chunk_bytes":crate::agent_jobs::STREAM_BLOCK_BYTES,
                    "maximum_metadata_bytes":crate::agent_jobs::MAX_STREAM_METADATA_BYTES,
                    "completion":"successful_close_required_before_client_publication",
                });
                Ok(report)
            }
            "cad_snapshot" => {
                let a: Capture = decode(args)?;
                let s = source::agent_source::capture(&self.workspace, &a.recipe)
                    .map_err(|e| e.to_string())?;
                self.remember(s, None)
            }
            "cad_snapshots" => {
                let _: Empty = decode(args)?;
                let snapshots:Vec<_> = self.snapshots.iter().map(|(id,snapshot)| json!({
                    "snapshot_id":id,"parent_snapshot_id":self.parents.get(id).and_then(Option::as_deref),
                    "source_name":snapshot.source_name,"language":snapshot.language,"source_bytes":snapshot.source.len(),
                })).collect();
                Ok(
                    json!({"snapshots":snapshots,"parentage":"first_observed_parent","retention":if self.durable {"durable_state_directory"} else {"until_process_exit"}}),
                )
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
                    json!({"snapshot_id":a.snapshot_id,"offset":a.offset,"next_offset":end,"total_bytes":s.len(),"text":&s[a.offset..end],"parent_snapshot_id":self.parents.get(&a.snapshot_id).and_then(Option::as_deref),"eof":end==s.len()}),
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
                if self.durable {
                    let request_id = a
                        .request_id
                        .as_deref()
                        .ok_or("durable evaluation requires request_id")?;
                    self.jobs.evaluate_with_request_id(
                        &a.snapshot_id,
                        &bytes,
                        &a.mode,
                        &a.outputs,
                        &a.params,
                        request_id,
                        cancel,
                    )
                } else {
                    if a.request_id.is_some() {
                        return Err("request_id requires --state-dir; ephemeral jobs have no retry identity".into());
                    }
                    self.jobs.evaluate(
                        &a.snapshot_id,
                        &bytes,
                        &a.mode,
                        &a.outputs,
                        &a.params,
                        cancel,
                    )
                }
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
            "cad_open_generation_stream" => {
                if !self.durable {
                    return Err("cad_open_generation_stream requires --state-dir".into());
                }
                let a: OpenGenerationStream = decode(args)?;
                self.jobs.open_stream(&a.job_id, &a.manifest_digest)
            }
            "cad_read_generation_stream" => {
                if !self.durable {
                    return Err("cad_read_generation_stream requires --state-dir".into());
                }
                let a: ReadGenerationStream = decode(args)?;
                self.jobs
                    .read_stream(&a.stream_id, &a.name, a.offset, a.max_bytes)
            }
            "cad_close_generation_stream" => {
                if !self.durable {
                    return Err("cad_close_generation_stream requires --state-dir".into());
                }
                let a: CloseGenerationStream = decode(args)?;
                self.jobs.close_stream(&a.stream_id)
            }
            "cad_release" => {
                let _: Empty = decode(args)?;
                if !self.durable {
                    return Err("cad_release requires --state-dir".into());
                }
                self.jobs.release()
            }
            "cad_accept" => {
                if !self.durable {
                    return Err("cad_accept requires --state-dir".into());
                }
                if args.get("expected_base_revision").is_none() {
                    return Err("cad_accept requires explicit expected_base_revision (null for the first revision)".into());
                }
                let a: Accept = decode(args)?;
                if a.request_id.is_empty()
                    || a.request_id.len() > 128
                    || a.request_id.chars().any(char::is_control)
                {
                    return Err("request_id must be 1..128 bytes without control characters".into());
                }
                self.jobs.accept(
                    &a.request_id,
                    &a.job_id,
                    &a.manifest_digest,
                    a.expected_base_revision.as_deref(),
                    cancel,
                )
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
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions("Experimental original Rust headless CAD. Capture source, create candidate revisions, evaluate and inspect native evidence. Source recipes are authoritative. Jobs are immutable candidates: ephemeral by default, durable only when launched with --state-dir. Durable evaluation requires an explicit request_id; interrupted requests never rerun automatically. Durable generation streams pin a verified manifest; read chunks check touched blocks, and only successful close checks the whole job after transfer. Publish downloaded data only after successful close. Durable mode can accept a verified candidate into a local revision with an explicit expected base; it does not write source or deliver a manufacturing release. Viewer is read-only. One active tool call; limits enforced, not an OS memory sandbox.")
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
pub fn serve(workspace: &Path, state_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let workspace_dir = Dir::open_ambient_dir(workspace, ambient_authority())?;
    let jobs = match state_dir {
        Some(directory) => AgentJobs::new_durable(directory, workspace, &workspace_dir),
        None => AgentJobs::new(),
    }
    .map_err(std::io::Error::other)?;
    let recovered = if state_dir.is_some() {
        jobs.recovered_snapshots().map_err(std::io::Error::other)?
    } else {
        BTreeMap::new()
    };
    let mut snapshots = BTreeMap::new();
    let mut parents = BTreeMap::new();
    for (id, (snapshot, parent)) in recovered {
        snapshots.insert(id.clone(), snapshot);
        parents.insert(id, parent);
    }
    let server = CadServer {
        state: Arc::new(Mutex::new(State {
            workspace: workspace_dir,
            snapshots,
            parents,
            durable: state_dir.is_some(),
            jobs,
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
