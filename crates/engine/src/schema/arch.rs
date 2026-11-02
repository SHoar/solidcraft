//! Contracts for the commands in `cmd/arch.rs`: architecture / floor-plan helpers.

use schemars::JsonSchema;
use serde::Deserialize;

use super::types::{LengthExpr, OperationParam, Point2, StringList, ThinSideParam, present};

/// Build a wall as a thin extrude along a plan centerline.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct WallParams {
    /// Centerline start in the sketch plane (mm or length expressions), usually plan XY.
    pub a: Point2,
    /// Centerline end.
    pub b: Point2,
    /// Wall height (extrude distance).
    pub height: LengthExpr,
    /// Wall thickness.
    pub thickness: LengthExpr,
    /// Which side of the centerline the wall lies on. Default: center.
    #[serde(default)]
    pub side: Option<ThinSideParam>,
    /// Sketch plane: XY|XZ|YZ or a construction plane name. Default: XY.
    #[serde(default)]
    pub plane: Option<String>,
    /// Feature name for the extrude.
    #[serde(default)]
    pub name: Option<String>,
    /// Body name for the wall solid.
    #[serde(default)]
    pub body_name: Option<String>,
    /// How the wall combines with existing bodies. Default: new.
    #[serde(default)]
    pub operation: Option<OperationParam>,
    #[serde(default)]
    pub targets: Option<StringList>,
}

/// Cut an opening (door/window) through a wall along a plan centerline.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct OpeningParams {
    /// Wall centerline start (plan XY mm / length expressions).
    pub a: Point2,
    /// Wall centerline end.
    pub b: Point2,
    /// Distance along the wall from `a` to the opening's near edge.
    pub offset_from_a: LengthExpr,
    /// Opening width along the wall.
    pub width: LengthExpr,
    /// Opening height.
    pub height: LengthExpr,
    /// Sill height above the wall's base plane. Default: 0.
    #[serde(default, deserialize_with = "present")]
    #[schemars(with = "LengthExpr")]
    pub sill_height: Option<LengthExpr>,
    /// How far the cut box extends through the wall (full depth). Default: 500 mm.
    #[serde(default, deserialize_with = "present")]
    #[schemars(with = "LengthExpr")]
    pub cut_depth: Option<LengthExpr>,
    /// Feature name.
    #[serde(default)]
    pub name: Option<String>,
    /// Body name for the cut tool (usually consumed by the cut).
    #[serde(default)]
    pub body_name: Option<String>,
}
