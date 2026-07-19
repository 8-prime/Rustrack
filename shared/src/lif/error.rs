//! Errors produced when validating or resolving a LIF file.

use std::fmt;

/// Which kind of identifier collided, for [`LifError::DuplicateId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    Layout,
    Node,
    Edge,
    Station,
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IdKind::Layout => "layout",
            IdKind::Node => "node",
            IdKind::Edge => "edge",
            IdKind::Station => "station",
        };
        f.write_str(s)
    }
}

/// A single problem found in a LIF file.
///
/// Each variant carries the offending identifier so the message can name what
/// is wrong rather than just that something is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifError {
    /// The requested layout is not in the file, or no layout was requested and
    /// the file does not contain exactly one.
    NoSuchLayout {
        requested: Option<String>,
        available: Vec<String>,
    },
    /// No node or edge in the layout has properties for the requested vehicle
    /// type, so the resolved graph would be empty.
    NoSuchVehicleType {
        vehicle_type_id: String,
        available: Vec<String>,
    },
    /// An edge endpoint or station interaction node names a node that does not
    /// exist. The schema does not enforce referential integrity, so we must.
    UnknownNodeRef {
        layout_id: String,
        /// The edge or station holding the dangling reference.
        referenced_by: String,
        node_id: String,
    },
    DuplicateId {
        layout_id: String,
        kind: IdKind,
        id: String,
    },
    /// A trajectory's degree, knot vector, and control points are mutually
    /// inconsistent. Evaluating such a curve would panic.
    InvalidTrajectory {
        layout_id: String,
        edge_id: String,
        vehicle_type_id: String,
        reason: String,
    },
}

impl fmt::Display for LifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifError::NoSuchLayout {
                requested: Some(id),
                available,
            } => write!(
                f,
                "no layout with id '{id}' (file has: {})",
                join_preview(available)
            ),
            LifError::NoSuchLayout {
                requested: None,
                available,
            } => write!(
                f,
                "no layout selected and the file contains {} layouts ({}) — specify one",
                available.len(),
                join_preview(available)
            ),
            LifError::NoSuchVehicleType {
                vehicle_type_id,
                available,
            } => write!(
                f,
                "no nodes or edges declare properties for vehicle type '{vehicle_type_id}' \
                 (layout defines: {})",
                join_preview(available)
            ),
            LifError::UnknownNodeRef {
                layout_id,
                referenced_by,
                node_id,
            } => write!(
                f,
                "layout '{layout_id}': '{referenced_by}' references node '{node_id}', \
                 which is not defined"
            ),
            LifError::DuplicateId {
                layout_id,
                kind,
                id,
            } => write!(f, "layout '{layout_id}': duplicate {kind} id '{id}'"),
            LifError::InvalidTrajectory {
                layout_id,
                edge_id,
                vehicle_type_id,
                reason,
            } => write!(
                f,
                "layout '{layout_id}': edge '{edge_id}' (vehicle type '{vehicle_type_id}') \
                 has an invalid trajectory: {reason}"
            ),
        }
    }
}

impl std::error::Error for LifError {}

/// The collected result of validating a file.
///
/// Validation reports many problems at once rather than bailing on the first,
/// so a large exported layout can be fixed in one pass instead of one upload
/// per mistake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifErrors {
    pub errors: Vec<LifError>,
    /// Problems found beyond [`MAX_REPORTED_ERRORS`] and therefore omitted.
    pub truncated: usize,
}

/// Cap on reported problems. A file with a systematic fault (e.g. every edge
/// referencing a renamed node) would otherwise produce a response as large as
/// the upload itself.
pub const MAX_REPORTED_ERRORS: usize = 50;

impl LifErrors {
    pub fn single(error: LifError) -> Self {
        LifErrors {
            errors: vec![error],
            truncated: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.truncated == 0
    }

    /// Total problems found, including those omitted from `errors`.
    pub fn len(&self) -> usize {
        self.errors.len() + self.truncated
    }
}

impl fmt::Display for LifErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} problem(s) in LIF file", self.len())?;
        for e in &self.errors {
            write!(f, "\n  - {e}")?;
        }
        if self.truncated > 0 {
            write!(f, "\n  ... and {} more", self.truncated)?;
        }
        Ok(())
    }
}

impl std::error::Error for LifErrors {}

/// Render up to a handful of ids for an error message, so a 100k-node layout
/// does not produce a 100k-entry error string.
fn join_preview(items: &[String]) -> String {
    const PREVIEW: usize = 5;
    if items.is_empty() {
        return "none".to_string();
    }
    let shown = items.iter().take(PREVIEW).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > PREVIEW {
        format!("{shown}, ... ({} total)", items.len())
    } else {
        shown
    }
}
