//! LIF — the VDMA Layout Interchange Format, version 1.0.0.
//!
//! LIF is the layout-side counterpart to VDA5050: where VDA5050 (see
//! [`crate::vda5050`]) is the runtime MQTT protocol, LIF is the static
//! description of the track a vehicle integrator hands to a fleet control
//! system. The two deliberately share terminology — nodes, edges, actions,
//! blocking types — but are separate documents with separate version numbers.
//!
//! There is no crates.io support for LIF, so these types are maintained here.
//! VDMA publishes the specification only as a PDF; the reference machine-readable
//! schema is the community-maintained
//! <https://github.com/continua-systems/vdma-lif> (`schema/lif-schema.json`).
//!
//! Typical use:
//!
//! ```ignore
//! let lif: Lif = serde_json::from_slice(&bytes)?;
//! validate(&lif)?;
//! let layout = lif.resolve(None, "my-vehicle-type")?;
//! ```
//!
//! Layouts can be tens of megabytes, which makes parsing expensive enough to
//! belong on a blocking thread. Prefer holding a [`LifSummary`] over a parsed
//! [`Lif`] anywhere the full graph is not actually needed.

pub mod error;
pub mod map;
pub mod model;
pub mod resolve;
pub mod validate;

pub use error::{IdKind, LifError, LifErrors, MAX_REPORTED_ERRORS};
pub use map::{Bounds, MapEdge, MapNode, MapStation, MapView};
pub use model::*;
pub use resolve::{ResolvedEdge, ResolvedLayout, ResolvedNode, DEFAULT_MAX_SPEED};
pub use validate::validate;

use serde::{Deserialize, Serialize};

/// A cheap description of a stored layout.
///
/// This is what callers keep in memory and what the API returns alongside
/// system info — the parsed document itself stays on disk. It is small enough
/// to include in a response that is polled every few seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifSummary {
    pub project_identification: String,
    pub lif_version: String,
    pub layout_count: usize,
    pub node_count: usize,
    pub edge_count: usize,
    pub station_count: usize,
    /// Size of the uncompressed source document, in bytes.
    pub raw_bytes: u64,
    /// RFC 3339 timestamp of when the layout was uploaded.
    pub uploaded_at: String,
}

impl LifSummary {
    /// Derive a summary from a parsed document, so the parsed form can be
    /// dropped immediately afterwards.
    pub fn derive(lif: &Lif, raw_bytes: u64, uploaded_at: String) -> Self {
        LifSummary {
            project_identification: lif.meta_information.project_identification.clone(),
            lif_version: lif.meta_information.lif_version.clone(),
            layout_count: lif.layouts.len(),
            node_count: lif.layouts.iter().map(|l| l.nodes.len()).sum(),
            edge_count: lif.layouts.iter().map(|l| l.edges.len()).sum(),
            station_count: lif.layouts.iter().map(|l| l.stations.len()).sum(),
            raw_bytes,
            uploaded_at,
        }
    }
}

#[cfg(test)]
mod tests;
