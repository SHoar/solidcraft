//! Architecture helpers for residential floor plans: walls and openings.
//!
//! These compose sketch + thin extrude / cut so agents and the homeplans compiler can speak
//! plan language (`a`/`b` centerlines) without hand-rolling each sketch.

use serde_json::{Value, json};
use solidcraft_doc::expr::Kind;
use solidcraft_geom::Vec2;

use super::features::check_expr;
use super::{CommandSpec, not_in_sketch};
use crate::params::{bad, expr, req_expr, str_, string_list, vec2};
use crate::schema::arch::{OpeningParams, WallParams};
use crate::schema::features::FeatureResult;
use crate::schema::{TypedContract, output_schema_for};
use crate::{Result, Session};

const WALL_CONTRACT: TypedContract<WallParams> = TypedContract::new();
const OPENING_CONTRACT: TypedContract<OpeningParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("arch.wall", "Wall", crate::checked!("arch.wall", wall))
        .at("SOLID", "ARCHITECTURE")
        .icon("extrude")
        .enabled(not_in_sketch)
        .contract(&WALL_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "a, b: [x, y] centerline ends in the sketch plane (mm); height, thickness: length expr; \
             side?: center|inside|outside|two_sides (default center; two_sides needs thickness2 on a later edit); \
             plane?: XY|XZ|YZ|construction plane name (default XY); operation?: new|join|cut|intersect (default new); \
             targets?; name?; body_name?. Builds a sketch of the centerline and a thin extrude",
        ),
    CommandSpec::new("arch.opening", "Opening", crate::checked!("arch.opening", opening))
        .at("SOLID", "ARCHITECTURE")
        .icon("hole")
        .enabled(not_in_sketch)
        .contract(&OPENING_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "a, b: [x, y] wall centerline (mm); offset_from_a, width, height: length expr (offset and width along the wall); \
             sill_height?: length expr (default 0); cut_depth?: length expr (through-wall cut depth, default 500 mm, centered on the centerline); \
             name?; body_name?. Cuts a rectangular opening through bodies along the wall",
        ),
];

fn run_cmd(s: &mut Session, id: &str, p: Value) -> Result<Value> {
    let spec = super::find_command(id).ok_or_else(|| bad(id, format!("internal: `{id}` is missing")))?;
    (spec.run)(s, &p)
}

fn unique_sketch_name(s: &Session, base: &str) -> String {
    if s.doc.find_feature(base).is_none() {
        return base.to_string();
    }
    for i in 2..10_000 {
        let n = format!("{base} ({i})");
        if s.doc.find_feature(&n).is_none() {
            return n;
        }
    }
    format!("{base} ({})", s.doc.features.len() + 1)
}

fn length_mm(s: &Session, cmd: &str, e: &str, what: &str) -> Result<f64> {
    check_expr(s, e, Kind::Length, cmd, what)?;
    s.doc.eval(&s.doc.with_design_unit(e, Kind::Length), Kind::Length).map_err(|err| bad(cmd, format!("`{what}`: {err}")))
}

fn wall(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "arch.wall";
    let a = p.get("a").and_then(vec2).ok_or_else(|| bad(cmd, "`a` must be a point [x, y]"))?;
    let b = p.get("b").and_then(vec2).ok_or_else(|| bad(cmd, "`b` must be a point [x, y]"))?;
    if a.dist(b) < 1e-6 {
        return Err(bad(cmd, "`a` and `b` must be distinct"));
    }
    let height = req_expr(cmd, p, "height")?;
    check_expr(s, &height, Kind::Length, cmd, "height")?;
    let thickness = req_expr(cmd, p, "thickness")?;
    check_expr(s, &thickness, Kind::Length, cmd, "thickness")?;
    let t = length_mm(s, cmd, &thickness, "thickness")?;
    let h = length_mm(s, cmd, &height, "height")?;
    if t <= 0.0 || !t.is_finite() {
        return Err(bad(cmd, "`thickness` must be positive"));
    }
    if h <= 0.0 || !h.is_finite() {
        return Err(bad(cmd, "`height` must be positive"));
    }
    let side = str_(p, "side").unwrap_or("center");
    if !matches!(side, "center" | "inside" | "outside" | "two_sides") {
        return Err(bad(cmd, "`side` must be center, inside, outside or two_sides"));
    }
    let plane = p.get("plane").cloned().unwrap_or(json!("XY"));
    let feature_name = str_(p, "name").unwrap_or("Wall");
    let sketch_name = unique_sketch_name(s, &format!("{feature_name} Sketch"));

    run_cmd(s, "sketch.create", json!({"plane": plane, "name": sketch_name}))?;
    run_cmd(s, "sketch.line", json!({"points": [[a.x, a.y], [b.x, b.y]]}))?;
    run_cmd(s, "sketch.finish", json!({}))?;
    // Nested `.run` skips Session::execute's refresh; extrude needs the sketch evaluated.
    s.refresh();

    let mut extrude = json!({
        "sketch": sketch_name,
        "thin": true,
        "thickness": thickness,
        "side": side,
        "distance": height,
        "name": feature_name,
        "operation": str_(p, "operation").unwrap_or("new"),
    });
    if let Some(b) = str_(p, "body_name") {
        extrude["body_name"] = json!(b);
    }
    if let Ok(targets) = string_list(cmd, p, "targets") {
        if !targets.is_empty() {
            extrude["targets"] = json!(targets);
        }
    }
    run_cmd(s, "solid.extrude", extrude)
}

fn opening(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "arch.opening";
    let a = p.get("a").and_then(vec2).ok_or_else(|| bad(cmd, "`a` must be a point [x, y]"))?;
    let b = p.get("b").and_then(vec2).ok_or_else(|| bad(cmd, "`b` must be a point [x, y]"))?;
    let along = b - a;
    let len = along.len();
    if len < 1e-6 {
        return Err(bad(cmd, "`a` and `b` must be distinct"));
    }
    let dir = along * (1.0 / len);
    let perp = Vec2::new(-dir.y, dir.x);

    let offset_e = req_expr(cmd, p, "offset_from_a")?;
    let width_e = req_expr(cmd, p, "width")?;
    let height_e = req_expr(cmd, p, "height")?;
    let sill_e = expr(p, "sill_height").unwrap_or_else(|| "0".into());
    let depth_e = expr(p, "cut_depth").unwrap_or_else(|| "500".into());

    let offset = length_mm(s, cmd, &offset_e, "offset_from_a")?;
    let width = length_mm(s, cmd, &width_e, "width")?;
    let height = length_mm(s, cmd, &height_e, "height")?;
    let sill = length_mm(s, cmd, &sill_e, "sill_height")?;
    let depth = length_mm(s, cmd, &depth_e, "cut_depth")?;
    if width <= 0.0 || height <= 0.0 || depth <= 0.0 {
        return Err(bad(cmd, "`width`, `height` and `cut_depth` must be positive"));
    }
    if offset < 0.0 || sill < 0.0 {
        return Err(bad(cmd, "`offset_from_a` and `sill_height` must be ≥ 0"));
    }
    if offset + width > len + 1e-6 {
        return Err(bad(cmd, "opening extends past the wall end (`offset_from_a` + `width`)"));
    }

    // Plan rectangle centered on the wall centerline, oriented with the wall.
    let start = a + dir * offset;
    let end = a + dir * (offset + width);
    let half = depth * 0.5;
    let p0 = start - perp * half;
    let p1 = end - perp * half;
    let p2 = end + perp * half;
    let p3 = start + perp * half;

    let feature_name = str_(p, "name").unwrap_or("Opening");
    let sketch_name = unique_sketch_name(s, &format!("{feature_name} Sketch"));

    run_cmd(s, "sketch.create", json!({"plane": "XY", "name": sketch_name}))?;
    run_cmd(
        s,
        "sketch.line",
        json!({"points": [[p0.x, p0.y], [p1.x, p1.y], [p2.x, p2.y], [p3.x, p3.y]], "closed": true}),
    )?;
    run_cmd(s, "sketch.finish", json!({}))?;
    s.refresh();

    let mut extrude = json!({
        "sketch": sketch_name,
        "distance": height_e,
        "start_offset": sill_e,
        "operation": "cut",
        "name": feature_name,
    });
    if let Some(b) = str_(p, "body_name") {
        extrude["body_name"] = json!(b);
    }
    run_cmd(s, "solid.extrude", extrude)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::Session;

    fn run(s: &mut Session, id: &str, p: Value) -> Value {
        match s.execute(id, &p) {
            Ok(v) => v,
            Err(e) => panic!("{id} {p}: {e}"),
        }
    }

    fn volume(s: &mut Session) -> f64 {
        run(s, "inspect.measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap_or(f64::NAN)
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() <= 1e-6 * b.abs().max(1.0), "{a} != {b}");
    }

    #[test]
    fn a_straight_wall_has_length_times_thickness_times_height() {
        let mut s = Session::default();
        run(
            &mut s,
            "arch.wall",
            json!({"a": [0, 0], "b": [4000, 0], "height": 2700, "thickness": 150, "side": "center", "name": "South", "body_name": "South"}),
        );
        close(volume(&mut s), 4000.0 * 150.0 * 2700.0);
        assert_eq!(run(&mut s, "inspect.measure", json!({}))["body_count"], 1);
    }

    #[test]
    fn an_opening_cuts_width_by_depth_by_height_from_a_wall() {
        let mut s = Session::default();
        run(
            &mut s,
            "arch.wall",
            json!({"a": [0, 0], "b": [4000, 0], "height": 2700, "thickness": 150, "side": "center", "name": "South"}),
        );
        let before = volume(&mut s);
        run(
            &mut s,
            "arch.opening",
            json!({
                "a": [0, 0],
                "b": [4000, 0],
                "offset_from_a": 1000,
                "width": 900,
                "height": 2100,
                "sill_height": 0,
                "cut_depth": 400,
                "name": "FrontDoor"
            }),
        );
        // Cut volume is the opening prism (width × cut_depth × height), fully through the wall thickness.
        close(volume(&mut s), before - 900.0 * 150.0 * 2100.0);
    }

    #[test]
    fn a_zero_length_wall_is_refused() {
        let mut s = Session::default();
        let e = s.execute("arch.wall", &json!({"a": [1, 1], "b": [1, 1], "height": 100, "thickness": 10}));
        assert!(e.is_err(), "{e:?}");
        assert_eq!(s.doc.features.len(), 0);
    }

    #[test]
    fn an_opening_past_the_wall_is_refused() {
        let mut s = Session::default();
        run(&mut s, "arch.wall", json!({"a": [0, 0], "b": [1000, 0], "height": 100, "thickness": 10}));
        let e = s.execute(
            "arch.opening",
            &json!({"a": [0, 0], "b": [1000, 0], "offset_from_a": 800, "width": 300, "height": 50}),
        );
        assert!(e.is_err(), "{e:?}");
    }
}
