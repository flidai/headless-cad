//! Durable local agent state requires Linux's fd-relative NOFOLLOW and flock APIs.
//! The ephemeral CLI remains available on other platforms.
#![allow(dead_code)]
use crate::source::Snapshot;
use cap_std::fs::Dir;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;
pub(crate) const SCHEMA: u32 = 2;
#[derive(Clone)]
pub(crate) struct Intent {
    pub schema_version: u32,
    pub request_id: String,
    pub fingerprint: String,
    pub job_id: String,
    pub snapshot_id: String,
    pub mode: String,
    pub executable_digest: String,
}
pub(crate) struct Receipt {
    pub report: Value,
    pub artifacts: BTreeMap<String, (u64, String)>,
}
pub(crate) struct AgentStore;
fn unavailable<T>() -> Result<T> {
    Err("durable agent state requires Linux".into())
}
pub(crate) fn request_hash(_: &str) -> Result<String> {
    unavailable()
}
impl AgentStore {
    pub(crate) fn open(_: &Path, _: &Path, _: &Dir) -> Result<Self> {
        unavailable()
    }
    pub(crate) fn executable_digest(&self) -> &str {
        ""
    }
    pub(crate) fn snapshots(&self) -> Result<BTreeMap<String, (Snapshot, Option<String>)>> {
        unavailable()
    }
    pub(crate) fn persist_snapshot(
        &self,
        _: &str,
        _: &Snapshot,
        _: Option<&str>,
    ) -> Result<Option<String>> {
        unavailable()
    }
    pub(crate) fn intents(&self) -> Result<BTreeMap<String, Intent>> {
        unavailable()
    }
    pub(crate) fn persist_intent(&self, _: &Intent) -> Result<()> {
        unavailable()
    }
    pub(crate) fn receipts(
        &self,
        _: &BTreeMap<String, Intent>,
    ) -> Result<BTreeMap<String, Receipt>> {
        unavailable()
    }
    pub(crate) fn retained_bytes(&self, _: &BTreeMap<String, Intent>) -> Result<u64> {
        unavailable()
    }
    pub(crate) fn copy_job_from_temp(&self, _: &str, _: &Path) -> Result<()> {
        unavailable()
    }
    pub(crate) fn open_job_dir(&self, _: &str) -> Result<Dir> {
        unavailable()
    }
    pub(crate) fn pinned_path(_: &Dir) -> PathBuf {
        PathBuf::new()
    }
    pub(crate) fn read_job_file(&self, _: &str, _: &[&str], _: usize) -> Result<Vec<u8>> {
        unavailable()
    }
    pub(crate) fn persist_receipt(
        &self,
        _: &str,
        _: &str,
        _: &Value,
        _: &BTreeMap<String, (u64, String)>,
    ) -> Result<()> {
        unavailable()
    }
    pub(crate) fn verify_job_seal(&self, _: &str) -> Result<()> {
        unavailable()
    }
    pub(crate) fn copy_job_to_temp(&self, _: &str, _: &Path) -> Result<()> {
        unavailable()
    }
}
