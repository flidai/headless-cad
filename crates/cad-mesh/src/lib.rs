//! Generation-scoped reuse of certified derived meshes, never native geometry.
//!
//! The retention budget charges serialized snapshot witnesses, mesh vector
//! capacities, Arc headers and entry descriptors. It excludes allocator/BTree
//! bookkeeping, temporary serialization/triangulation, caller-held Arcs and output
//! strings; it is not a process RSS or peak-work bound. At most 1024 entries are
//! retained. Oversize successful results are returned uncached. Kernel work limits
//! and certification remain unchanged. Drop this explicit cache after publication.
use cad_kernel::{
    CylinderMesh, FaceId, KernelError, ModelSnapshot, TessellationPolicy, Tolerance, VertexId,
};
use serde::Serialize;
use std::{collections::BTreeMap, mem::size_of, sync::Arc};
use thiserror::Error;

pub const DEFAULT_RETAINED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CACHE_ENTRIES: usize = 1024;
pub const KERNEL_BUILD_ID: &str = cad_kernel::KERNEL_BUILD_ID;
/// Identity of this adapter and the underlying certified mesher implementation.
pub fn mesher_build_id() -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"cad-mesh-v1\0");
    h.update(KERNEL_BUILD_ID.as_bytes());
    h.update(include_bytes!("lib.rs"));
    h.update(include_bytes!("../Cargo.toml"));
    h.finalize().to_hex().to_string()
}
#[derive(Debug, Error)]
pub enum MeshError {
    #[error("certified mesh failed: {0}")]
    Kernel(#[from] KernelError),
    #[error("cannot encode native snapshot: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("native snapshot digest collision")]
    IdentityCollision,
    #[error("tessellation policy requires positive finite chordal error and at least 12 segments")]
    InvalidPolicy,
}
#[derive(Debug, Clone, Serialize)]
pub struct MeshCacheStats {
    pub hits: usize,
    /// Valid-policy requests with no matching certified entry, including failures.
    pub misses: usize,
    /// Successfully constructed certified meshes, whether retained or not.
    pub builds: usize,
    pub failures: usize,
    pub evictions: usize,
    pub retained_bytes: usize,
    pub entries: usize,
    pub max_retained_bytes: usize,
    pub max_entries: usize,
    pub kernel_build_id: String,
    pub mesher_build_id: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Request {
    Analytic {
        chordal_error: u64,
        max_segments: usize,
    },
    Planar {
        face: usize,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    implementation: [u8; 32],
    snapshot: [u8; 32],
    tolerance: u64,
    request: Request,
}
enum Mesh {
    Analytic(Arc<CylinderMesh>),
    Planar(Arc<Vec<[VertexId; 3]>>),
}
struct Entry {
    witness: Vec<u8>,
    mesh: Mesh,
    charged: usize,
    sequence: usize,
}
/// No caller-supplied hashes, no shared global state and no failure entries.
pub struct GenerationMeshCache {
    entries: BTreeMap<Key, Entry>,
    stats: MeshCacheStats,
    sequence: usize,
}
impl Default for GenerationMeshCache {
    fn default() -> Self {
        Self::new(DEFAULT_RETAINED_BYTES)
    }
}
impl GenerationMeshCache {
    /// Zero budget is the uncached reference path. Successful large meshes still
    /// return normally; retention is an optimization, not an export validity gate.
    pub fn new(max_retained_bytes: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            sequence: 0,
            stats: MeshCacheStats {
                hits: 0,
                misses: 0,
                builds: 0,
                failures: 0,
                evictions: 0,
                retained_bytes: 0,
                entries: 0,
                max_retained_bytes,
                max_entries: MAX_CACHE_ENTRIES,
                kernel_build_id: KERNEL_BUILD_ID.into(),
                mesher_build_id: mesher_build_id(),
            },
        }
    }
    pub fn stats(&self) -> MeshCacheStats {
        self.stats.clone()
    }
    /// Release retained entries; cumulative work counters remain available.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.stats.retained_bytes = 0;
        self.stats.entries = 0;
    }
    pub fn analytic(
        &mut self,
        model: &ModelSnapshot,
        tolerance: Tolerance,
        policy: TessellationPolicy,
    ) -> Result<Arc<CylinderMesh>, MeshError> {
        let result = self.analytic_inner(model, tolerance, policy);
        if result.is_err() {
            self.stats.failures += 1;
        }
        result
    }
    pub fn planar_face(
        &mut self,
        model: &ModelSnapshot,
        tolerance: Tolerance,
        face: FaceId,
    ) -> Result<Arc<Vec<[VertexId; 3]>>, MeshError> {
        let result = self.planar_inner(model, tolerance, face);
        if result.is_err() {
            self.stats.failures += 1;
        }
        result
    }
    fn key(
        &self,
        model: &ModelSnapshot,
        tolerance: Tolerance,
        request: Request,
    ) -> Result<(Key, Vec<u8>), MeshError> {
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance.into());
        }
        let witness = serde_json::to_vec(model)?;
        Ok((
            Key {
                implementation: *blake3::hash(self.stats.mesher_build_id.as_bytes()).as_bytes(),
                snapshot: *blake3::hash(&witness).as_bytes(),
                tolerance: tolerance.linear.to_bits(),
                request,
            },
            witness,
        ))
    }
    fn analytic_inner(
        &mut self,
        model: &ModelSnapshot,
        tolerance: Tolerance,
        policy: TessellationPolicy,
    ) -> Result<Arc<CylinderMesh>, MeshError> {
        // Exhaustive destructuring makes future policy fields require a key update.
        let TessellationPolicy {
            chordal_error,
            max_segments,
        } = policy;
        if !chordal_error.is_finite() || chordal_error <= 0. || max_segments < 12 {
            return Err(MeshError::InvalidPolicy);
        }
        let (key, witness) = self.key(
            model,
            tolerance,
            Request::Analytic {
                chordal_error: chordal_error.to_bits(),
                max_segments,
            },
        )?;
        if let Some(entry) = self.entries.get(&key) {
            if entry.witness != witness {
                return Err(MeshError::IdentityCollision);
            }
            if let Mesh::Analytic(mesh) = &entry.mesh {
                self.stats.hits += 1;
                return Ok(Arc::clone(mesh));
            }
            unreachable!("request kind belongs to cache key");
        }
        self.stats.misses += 1;
        let validation = model.validate(tolerance);
        if !validation.valid {
            return Err(KernelError::InvalidInput(validation).into());
        }
        let mesh = Arc::new(cad_kernel::tessellate_analytic(model, tolerance, policy)?);
        self.stats.builds += 1;
        let bytes = analytic_bytes(&mesh);
        self.insert(key, witness, Mesh::Analytic(Arc::clone(&mesh)), bytes);
        Ok(mesh)
    }
    fn planar_inner(
        &mut self,
        model: &ModelSnapshot,
        tolerance: Tolerance,
        face: FaceId,
    ) -> Result<Arc<Vec<[VertexId; 3]>>, MeshError> {
        let (key, witness) = self.key(model, tolerance, Request::Planar { face: face.0 })?;
        if let Some(entry) = self.entries.get(&key) {
            if entry.witness != witness {
                return Err(MeshError::IdentityCollision);
            }
            if let Mesh::Planar(mesh) = &entry.mesh {
                self.stats.hits += 1;
                return Ok(Arc::clone(mesh));
            }
            unreachable!("request kind belongs to cache key");
        }
        self.stats.misses += 1;
        let mesh = Arc::new(cad_kernel::triangulate_face(model, face, tolerance)?);
        self.stats.builds += 1;
        let bytes = size_of::<Vec<[VertexId; 3]>>()
            + 2 * size_of::<usize>()
            + mesh.capacity() * size_of::<[VertexId; 3]>();
        self.insert(key, witness, Mesh::Planar(Arc::clone(&mesh)), bytes);
        Ok(mesh)
    }
    fn insert(&mut self, key: Key, witness: Vec<u8>, mesh: Mesh, bytes: usize) {
        let charged = bytes
            .saturating_add(witness.capacity())
            .saturating_add(size_of::<Entry>())
            .saturating_add(size_of::<Key>());
        if charged > self.stats.max_retained_bytes {
            return;
        }
        while self.stats.retained_bytes.saturating_add(charged) > self.stats.max_retained_bytes
            || self.entries.len() >= MAX_CACHE_ENTRIES
        {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.sequence)
                .map(|(k, _)| k.clone())
                .unwrap();
            let entry = self.entries.remove(&oldest).unwrap();
            self.stats.retained_bytes -= entry.charged;
            self.stats.evictions += 1;
        }
        // FIFO, deterministic under the publication's deterministic request order.
        self.sequence = self.sequence.saturating_add(1);
        self.entries.insert(
            key,
            Entry {
                witness,
                mesh,
                charged,
                sequence: self.sequence,
            },
        );
        self.stats.retained_bytes += charged;
        self.stats.entries = self.entries.len();
    }
}
fn analytic_bytes(mesh: &CylinderMesh) -> usize {
    let mut bytes = size_of::<CylinderMesh>()
        + 2 * size_of::<usize>()
        + mesh.patches.capacity() * size_of::<cad_kernel::MeshPatch>();
    for p in &mesh.patches {
        bytes = bytes
            .saturating_add(p.points.capacity() * size_of::<[f64; 3]>())
            .saturating_add(p.boundary_edges.capacity() * size_of::<Option<cad_kernel::EdgeId>>());
    }
    bytes
}

#[cfg(test)]
mod tests;
