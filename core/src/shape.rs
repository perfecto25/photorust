//! Shapes — the geometry behind the shape tools.
//!
//! Photoshop's six shape tools differ in exactly one way: the outline a drag
//! produces. What happens to that outline afterwards — a shape layer, a path,
//! or pixels — is the same for all of them, and is [`ShapeMode`]'s business.
//! So the whole group is one function, [`outline`], returning points.
//!
//! The points are also what the canvas previews while the drag is live, so the
//! dashed outline under the cursor and the thing that lands on release cannot
//! disagree: they are the same call.
//!
//! Curves are flattened here rather than kept as Béziers. A dragged shape is
//! committed to pixels or to a polygonal path immediately, so carrying exact
//! curve maths through the pipeline would buy nothing; the segment counts below
//! are chosen so the flattening is invisible at the sizes a shape is drawn at.

use crate::buffer::Rect;

/// What a dragged shape becomes. CS6's Mode menu, in its order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ShapeMode {
    /// A layer of its own, filled with the colour and cut to the shape. CS6's
    /// default.
    #[default]
    Shape = 0,
    /// A path, added to the work path and drawn by nothing until the Paths
    /// panel is told to fill or stroke it.
    Path = 1,
    /// Pixels, painted straight onto the active layer.
    Pixels = 2,
}

impl ShapeMode {
    pub fn from_i32(v: i32) -> ShapeMode {
        match v {
            1 => ShapeMode::Path,
            2 => ShapeMode::Pixels,
            _ => ShapeMode::Shape,
        }
    }
}

/// Which shape tool is in hand — CS6's flyout, in its order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ShapeKind {
    #[default]
    Rectangle = 0,
    RoundedRectangle = 1,
    Ellipse = 2,
    Polygon = 3,
    Line = 4,
    CustomShape = 5,
}

impl ShapeKind {
    pub fn from_i32(v: i32) -> ShapeKind {
        match v {
            1 => ShapeKind::RoundedRectangle,
            2 => ShapeKind::Ellipse,
            3 => ShapeKind::Polygon,
            4 => ShapeKind::Line,
            5 => ShapeKind::CustomShape,
            _ => ShapeKind::Rectangle,
        }
    }

    /// What a layer made from this shape is called, following Photoshop's
    /// habit of naming a shape layer after the tool that drew it.
    pub fn layer_name(self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "Rectangle",
            ShapeKind::RoundedRectangle => "Rounded Rectangle",
            ShapeKind::Ellipse => "Ellipse",
            ShapeKind::Polygon => "Polygon",
            ShapeKind::Line => "Line",
            ShapeKind::CustomShape => "Shape",
        }
    }
}

/// The settings the shape tools' options bar holds. Each tool reads the one
/// that belongs to it and ignores the rest.
#[derive(Clone, Copy, Debug)]
pub struct ShapeOptions {
    pub kind: ShapeKind,
    /// Rounded Rectangle's Radius, in pixels.
    pub corner_radius: f32,
    /// Polygon's Sides.
    pub sides: u32,
    /// Line's Weight, in pixels.
    pub line_weight: f32,
    /// Which entry of [`CUSTOM_SHAPE_NAMES`] the Custom Shape tool draws.
    pub custom: usize,
}

impl Default for ShapeOptions {
    fn default() -> Self {
        Self {
            kind: ShapeKind::Rectangle,
            // Photoshop's own defaults.
            corner_radius: 10.0,
            sides: 5,
            line_weight: 1.0,
            custom: 0,
        }
    }
}

/// The outline a drag from `from` to `to` marks out, in document space.
///
/// `shift` and `alt` are CS6's modifiers, and mean different things to
/// different tools — a rectangle squares off and grows from its centre, a line
/// snaps its angle, a polygon is centred on the start whatever is held. Each
/// tool's reading of them is in its own function below.
///
/// The result is a closed outline: the last point joins back to the first, and
/// is not repeated.
pub fn outline(options: ShapeOptions, from: (f32, f32), to: (f32, f32), shift: bool, alt: bool)
    -> Vec<(f32, f32)>
{
    match options.kind {
        ShapeKind::Rectangle => rectangle_points(drag_rect(from, to, shift, alt)),
        ShapeKind::RoundedRectangle => {
            rounded_rectangle_points(drag_rect(from, to, shift, alt), options.corner_radius)
        }
        ShapeKind::Ellipse => ellipse_points(drag_rect(from, to, shift, alt)),
        ShapeKind::Polygon => polygon_points(from, to, options.sides, shift),
        ShapeKind::Line => line_points(from, to, options.line_weight, shift),
        ShapeKind::CustomShape => {
            custom_shape_points(options.custom, drag_rect(from, to, shift, alt))
        }
    }
}

/// The rectangle a drag marks out, with Shift squaring it off and Alt growing
/// it from the point the drag started at.
fn drag_rect(from: (f32, f32), to: (f32, f32), shift: bool, alt: bool) -> (f32, f32, f32, f32) {
    let (mut dx, mut dy) = (to.0 - from.0, to.1 - from.1);

    if shift {
        // The longer side wins, so squaring off never pulls the shape back
        // from where the pointer is.
        let side = dx.abs().max(dy.abs());
        dx = if dx < 0.0 { -side } else { side };
        dy = if dy < 0.0 { -side } else { side };
    }

    if alt {
        // Grown from the centre: the drag becomes a half-diagonal.
        (from.0 - dx, from.1 - dy, dx.abs() * 2.0, dy.abs() * 2.0)
    } else {
        (from.0.min(from.0 + dx), from.1.min(from.1 + dy), dx.abs(), dy.abs())
    }
}

/// The four corners of a rectangle, clockwise from the top-left.
pub fn rectangle_points(rect: (f32, f32, f32, f32)) -> Vec<(f32, f32)> {
    let (x, y, w, h) = rect;
    vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
}

/// The same rectangle with its corners cut to quarter-circles.
///
/// The radius is clamped to half the shorter side: asking for more than that
/// would make opposite corners overlap, and Photoshop simply stops there too —
/// a 200px radius on a 40px-tall box gives a stadium, not a knot.
pub fn rounded_rectangle_points(rect: (f32, f32, f32, f32), radius: f32) -> Vec<(f32, f32)> {
    let (x, y, w, h) = rect;
    let r = radius.max(0.0).min(w.min(h) / 2.0);
    if r <= 0.0 {
        return rectangle_points(rect);
    }

    const PER_CORNER: usize = 8;
    let mut points = Vec::with_capacity(PER_CORNER * 4);
    // Centre of each corner's arc, and the angle its quarter starts at, going
    // clockwise from the top-left.
    let corners = [
        ((x + r, y + r), std::f32::consts::PI),
        ((x + w - r, y + r), 1.5 * std::f32::consts::PI),
        ((x + w - r, y + h - r), 0.0),
        ((x + r, y + h - r), 0.5 * std::f32::consts::PI),
    ];
    for ((cx, cy), start) in corners {
        for i in 0..PER_CORNER {
            let t = i as f32 / (PER_CORNER - 1) as f32;
            let angle = start + t * 0.5 * std::f32::consts::PI;
            points.push((cx + r * angle.cos(), cy + r * angle.sin()));
        }
    }
    points
}

/// An ellipse inscribed in the rectangle.
pub fn ellipse_points(rect: (f32, f32, f32, f32)) -> Vec<(f32, f32)> {
    let (x, y, w, h) = rect;
    let (rx, ry) = (w / 2.0, h / 2.0);
    let (cx, cy) = (x + rx, y + ry);

    // Enough segments that the flattening stays under about a third of a pixel
    // at the size being drawn, and never fewer than a smooth small circle needs.
    let steps = ((rx.max(ry) * 1.6) as usize).clamp(24, 180);
    (0..steps)
        .map(|i| {
            let angle = (i as f32 / steps as f32) * std::f32::consts::TAU;
            (cx + rx * angle.cos(), cy + ry * angle.sin())
        })
        .collect()
}

/// A regular polygon, centred where the drag began.
///
/// Photoshop's Polygon tool works from the centre out — the drag is a radius,
/// not a corner — and the shape turns to follow the pointer, so the first
/// vertex sits under it. Shift snaps that rotation to 15°, as it does
/// everywhere else angles are dragged.
pub fn polygon_points(centre: (f32, f32), to: (f32, f32), sides: u32, shift: bool) -> Vec<(f32, f32)> {
    let sides = sides.clamp(3, 100);
    let (dx, dy) = (to.0 - centre.0, to.1 - centre.1);
    let radius = (dx * dx + dy * dy).sqrt();
    if radius <= 0.0 {
        return Vec::new();
    }

    let mut rotation = dy.atan2(dx);
    if shift {
        let step = std::f32::consts::PI / 12.0; // 15°
        rotation = (rotation / step).round() * step;
    }

    (0..sides)
        .map(|i| {
            let angle = rotation + (i as f32 / sides as f32) * std::f32::consts::TAU;
            (centre.0 + radius * angle.cos(), centre.1 + radius * angle.sin())
        })
        .collect()
}

/// A line of the given weight, as the rectangle it covers.
///
/// The Line tool draws a filled shape rather than a stroke — that is why it has
/// a Weight rather than a brush — so the outline is the quad swept by a
/// `weight`-wide segment. Shift snaps the angle to 45°.
pub fn line_points(from: (f32, f32), to: (f32, f32), weight: f32, shift: bool) -> Vec<(f32, f32)> {
    let (mut dx, mut dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.0 {
        return Vec::new();
    }

    if shift {
        let step = std::f32::consts::PI / 4.0; // 45°
        let angle = (dy.atan2(dx) / step).round() * step;
        dx = length * angle.cos();
        dy = length * angle.sin();
    }

    // Half the weight, at right angles to the line.
    let half = weight.max(1.0) / 2.0;
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy * half, ux * half);
    let end = (from.0 + dx, from.1 + dy);

    vec![
        (from.0 + nx, from.1 + ny),
        (end.0 + nx, end.1 + ny),
        (end.0 - nx, end.1 - ny),
        (from.0 - nx, from.1 - ny),
    ]
}

/// The custom shapes, in the order the picker lists them.
///
/// Photoshop ships its as artwork in a `.csh` library; ours are **generated**,
/// for the same reason the patterns are — the CS6 set is Adobe's artwork. Each
/// is defined in a unit square and scaled into whatever rectangle is dragged.
pub const CUSTOM_SHAPE_NAMES: [&str; 6] =
    ["Star", "Heart", "Arrow", "Cross", "Lightning", "Check"];

/// One custom shape's outline, scaled into `rect`.
///
/// Each shape is fitted to the rectangle rather than placed in it: whatever the
/// generator below produced is stretched so it touches all four sides, which is
/// what a user dragging a box out expects to get and saves every shape from
/// having to be authored to exactly the same extents.
pub fn custom_shape_points(index: usize, rect: (f32, f32, f32, f32)) -> Vec<(f32, f32)> {
    let (x, y, w, h) = rect;
    fit_to_unit_square(unit_shape(index))
        .into_iter()
        .map(|(ux, uy)| (x + ux * w, y + uy * h))
        .collect()
}

/// Stretch an outline so it exactly fills the unit square. A shape with no
/// extent on an axis — a straight line — is centred on it instead of being
/// divided by zero.
fn fit_to_unit_square(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return points;
    }
    let (mut x0, mut y0) = (f32::MAX, f32::MAX);
    let (mut x1, mut y1) = (f32::MIN, f32::MIN);
    for (x, y) in &points {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let (w, h) = (x1 - x0, y1 - y0);

    points
        .into_iter()
        .map(|(x, y)| {
            let nx = if w > 1e-6 { (x - x0) / w } else { 0.5 };
            let ny = if h > 1e-6 { (y - y0) / h } else { 0.5 };
            (nx, ny)
        })
        .collect()
}

/// A custom shape, in whatever coordinates suit it — `custom_shape_points`
/// fits the result to its box, so only the proportions here matter.
fn unit_shape(index: usize) -> Vec<(f32, f32)> {
    match CUSTOM_SHAPE_NAMES.get(index).copied().unwrap_or("Star") {
        "Heart" => heart(),
        "Arrow" => vec![
            (0.0, 0.32), (0.55, 0.32), (0.55, 0.06), (1.0, 0.5),
            (0.55, 0.94), (0.55, 0.68), (0.0, 0.68),
        ],
        "Cross" => vec![
            (0.34, 0.0), (0.66, 0.0), (0.66, 0.34), (1.0, 0.34), (1.0, 0.66),
            (0.66, 0.66), (0.66, 1.0), (0.34, 1.0), (0.34, 0.66), (0.0, 0.66),
            (0.0, 0.34), (0.34, 0.34),
        ],
        "Lightning" => vec![
            (0.58, 0.0), (0.9, 0.0), (0.62, 0.4), (0.86, 0.4),
            (0.3, 1.0), (0.44, 0.56), (0.16, 0.56), (0.34, 0.0),
        ],
        "Check" => vec![
            (0.9, 0.12), (1.0, 0.28), (0.42, 0.95), (0.0, 0.58),
            (0.12, 0.42), (0.44, 0.7),
        ],
        // Star: ten points alternating between two radii, first point up.
        _ => (0..10)
            .map(|i| {
                let radius = if i % 2 == 0 { 0.5 } else { 0.2 };
                let angle = -std::f32::consts::FRAC_PI_2
                    + (i as f32 / 10.0) * std::f32::consts::TAU;
                (0.5 + radius * angle.cos(), 0.5 + radius * angle.sin())
            })
            .collect(),
    }
}

/// The heart, from the usual parametric curve. Its y is negated because the
/// curve's axis points up and a raster's does not.
fn heart() -> Vec<(f32, f32)> {
    const STEPS: usize = 48;
    (0..STEPS)
        .map(|i| {
            let t = (i as f32 / STEPS as f32) * std::f32::consts::TAU;
            let x = 16.0 * t.sin().powi(3);
            let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos()
                - (4.0 * t).cos();
            (x, -y)
        })
        .collect()
}

/// A shape rendered on its own for the picker's swatch, `size` square.
///
/// Inset a little so the outline is not clipped by the swatch's edge.
pub fn custom_shape_preview_points(index: usize, size: u32) -> Vec<(f32, f32)> {
    let inset = (size as f32 * 0.1).max(1.0);
    let side = (size as f32 - inset * 2.0).max(1.0);
    custom_shape_points(index, (inset, inset, side, side))
}

/// The bounding box of an outline, for callers that need to know what it
/// covers without walking it themselves.
pub fn bounds(points: &[(f32, f32)]) -> Rect {
    if points.is_empty() {
        return Rect::default();
    }
    let (mut x0, mut y0) = (f32::MAX, f32::MAX);
    let (mut x1, mut y1) = (f32::MIN, f32::MIN);
    for (x, y) in points {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    Rect::new(
        x0.floor() as i32,
        y0.floor() as i32,
        (x1.ceil() - x0.floor()).max(0.0) as u32,
        (y1.ceil() - y0.floor()).max(0.0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(kind: ShapeKind) -> ShapeOptions {
        ShapeOptions { kind, ..ShapeOptions::default() }
    }

    #[test]
    fn a_rectangle_has_four_corners_in_order() {
        let points = outline(opts(ShapeKind::Rectangle), (10.0, 20.0), (40.0, 60.0), false, false);
        assert_eq!(points, vec![(10.0, 20.0), (40.0, 20.0), (40.0, 60.0), (10.0, 60.0)]);
    }

    #[test]
    fn shift_squares_a_rectangle_off_by_its_longer_side() {
        let points = outline(opts(ShapeKind::Rectangle), (0.0, 0.0), (40.0, 10.0), true, false);
        let b = bounds(&points);
        assert_eq!((b.width, b.height), (40, 40));
    }

    #[test]
    fn alt_grows_a_rectangle_from_where_the_drag_began() {
        let points = outline(opts(ShapeKind::Rectangle), (50.0, 50.0), (60.0, 70.0), false, true);
        let b = bounds(&points);
        assert_eq!((b.x, b.y), (40, 30), "the shape did not centre on the start");
        assert_eq!((b.width, b.height), (20, 40));
    }

    #[test]
    fn a_rounded_rectangle_stays_inside_its_box_and_cuts_its_corners() {
        let rect = (0.0, 0.0, 100.0, 60.0);
        let points = rounded_rectangle_points(rect, 15.0);
        let b = bounds(&points);
        assert_eq!((b.width, b.height), (100, 60), "the rounding changed the size");
        // The very corner of the box is outside a rounded rectangle.
        assert!(!points.iter().any(|(x, y)| *x < 1.0 && *y < 1.0), "a corner was left square");
    }

    #[test]
    fn a_huge_radius_stops_at_half_the_shorter_side() {
        // Otherwise opposite corners would cross and the outline would knot.
        let points = rounded_rectangle_points((0.0, 0.0, 100.0, 40.0), 500.0);
        let b = bounds(&points);
        assert_eq!((b.width, b.height), (100, 40));
    }

    #[test]
    fn an_ellipse_fills_its_box_and_is_round() {
        let points = ellipse_points((0.0, 0.0, 80.0, 40.0));
        let b = bounds(&points);
        assert!((b.width as i32 - 80).abs() <= 1 && (b.height as i32 - 40).abs() <= 1);
        // Every point sits on the ellipse: (x/rx)² + (y/ry)² == 1.
        for (x, y) in &points {
            let (nx, ny) = ((x - 40.0) / 40.0, (y - 20.0) / 20.0);
            assert!((nx * nx + ny * ny - 1.0).abs() < 1e-3, "point ({x}, {y}) is off the ellipse");
        }
    }

    #[test]
    fn a_polygon_has_its_sides_and_centres_on_the_drag_start() {
        let points = polygon_points((50.0, 50.0), (50.0, 20.0), 6, false);
        assert_eq!(points.len(), 6);
        for (x, y) in &points {
            let (dx, dy) = (x - 50.0, y - 50.0);
            assert!(((dx * dx + dy * dy).sqrt() - 30.0).abs() < 1e-3, "a vertex left the circle");
        }
    }

    #[test]
    fn a_polygon_turns_to_follow_the_drag() {
        let up = polygon_points((0.0, 0.0), (0.0, -10.0), 3, false);
        let right = polygon_points((0.0, 0.0), (10.0, 0.0), 3, false);
        assert_ne!(up[0], right[0], "the polygon did not rotate with the drag");
    }

    #[test]
    fn a_line_is_a_quad_of_its_weight() {
        let points = line_points((10.0, 10.0), (10.0, 50.0), 6.0, false);
        assert_eq!(points.len(), 4);
        let b = bounds(&points);
        assert_eq!(b.width, 6, "the line was not its weight across");
        assert_eq!(b.height, 40);
    }

    #[test]
    fn shift_snaps_a_line_to_45_degrees() {
        // A drag a few degrees off horizontal comes out exactly horizontal.
        let points = line_points((0.0, 0.0), (100.0, 8.0), 2.0, true);
        let b = bounds(&points);
        assert_eq!(b.height, 2, "the line did not snap flat");
    }

    #[test]
    fn every_custom_shape_is_a_usable_outline_inside_its_box() {
        for index in 0..CUSTOM_SHAPE_NAMES.len() {
            let points = custom_shape_points(index, (0.0, 0.0, 100.0, 100.0));
            assert!(points.len() >= 3, "{} has no area", CUSTOM_SHAPE_NAMES[index]);
            for (x, y) in &points {
                assert!(
                    (-0.01..=100.01).contains(x) && (-0.01..=100.01).contains(y),
                    "{} escapes its box at ({x}, {y})",
                    CUSTOM_SHAPE_NAMES[index]
                );
            }
            // And it fills that box rather than huddling in a corner.
            let b = bounds(&points);
            assert!(b.width >= 90 && b.height >= 90, "{} does not fill its box", CUSTOM_SHAPE_NAMES[index]);
        }
    }

    #[test]
    fn a_drag_that_went_nowhere_makes_no_shape() {
        assert!(polygon_points((5.0, 5.0), (5.0, 5.0), 5, false).is_empty());
        assert!(line_points((5.0, 5.0), (5.0, 5.0), 4.0, false).is_empty());
    }

    #[test]
    fn modes_and_kinds_come_back_from_their_codes() {
        assert_eq!(ShapeMode::from_i32(2), ShapeMode::Pixels);
        assert_eq!(ShapeMode::from_i32(99), ShapeMode::Shape, "an unknown mode is the default");
        assert_eq!(ShapeKind::from_i32(4), ShapeKind::Line);
        assert_eq!(ShapeKind::from_i32(-1), ShapeKind::Rectangle);
    }
}
