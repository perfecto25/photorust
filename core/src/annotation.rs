//! Annotations — the marks the eyedropper group leaves on a document.
//!
//! None of these touch pixels. They are overlays the user places to *read* an
//! image rather than change it, and they belong to the document the same way
//! slices do:
//!
//! * **Colour samplers** — persistent probes whose values stay on screen while
//!   you work, read out in the Info panel.
//! * **Notes** — text pinned to a point.
//! * **Count markers** — numbered tallies, for counting things in an image.
//! * **The ruler** — a single measuring line giving distance and angle.
//!
//! Photoshop caps samplers at [`MAX_COLOR_SAMPLERS`] and numbers every marker
//! from 1, both of which are modelled here.

/// How many colour samplers Photoshop allows on one document.
///
/// Ten. Older versions capped this at four; CS4 raised it, and the CS6 Info
/// panel will happily list `#1` through `#10`.
pub const MAX_COLOR_SAMPLERS: usize = 10;

/// The three point-marker kinds, shared with the shell as plain integers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum MarkerKind {
    ColorSampler = 0,
    Note = 1,
    Count = 2,
}

impl MarkerKind {
    pub fn from_i32(v: i32) -> Option<MarkerKind> {
        match v {
            0 => Some(MarkerKind::ColorSampler),
            1 => Some(MarkerKind::Note),
            2 => Some(MarkerKind::Count),
            _ => None,
        }
    }
}

/// A point marker in document coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marker {
    pub x: i32,
    pub y: i32,
    /// Note text. Empty for samplers and count markers.
    pub text: String,
}

impl Marker {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y, text: String::new() }
    }
}

/// A measuring line, and optionally the second arm that makes it a protractor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ruler {
    pub ax: f32,
    pub ay: f32,
    pub bx: f32,
    pub by: f32,
}

/// What the options bar reads out for a ruler: Photoshop's X, Y, W, H, A, D1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Degrees from horizontal, in `-180..=180`, positive **anticlockwise** —
    /// Photoshop reports the angle in screen terms, where y runs downward, so
    /// a line going down-right reads as a negative angle.
    pub angle: f32,
    pub distance: f32,
}

impl Ruler {
    pub fn new(ax: f32, ay: f32, bx: f32, by: f32) -> Self {
        Self { ax, ay, bx, by }
    }

    pub fn measure(&self) -> Measurement {
        let dx = self.bx - self.ax;
        let dy = self.by - self.ay;
        Measurement {
            x: self.ax,
            y: self.ay,
            width: dx,
            height: dy,
            // Negated because document y grows downward while the reported
            // angle is the everyday anticlockwise-from-east one.
            angle: (-dy).atan2(dx).to_degrees(),
            distance: dx.hypot(dy),
        }
    }
}

/// Every annotation on a document.
#[derive(Clone, Debug, Default)]
pub struct Annotations {
    samplers: Vec<Marker>,
    notes: Vec<Marker>,
    counts: Vec<Marker>,
    ruler: Option<Ruler>,
}

impl Annotations {
    pub fn new() -> Self {
        Self::default()
    }

    fn list(&self, kind: MarkerKind) -> &Vec<Marker> {
        match kind {
            MarkerKind::ColorSampler => &self.samplers,
            MarkerKind::Note => &self.notes,
            MarkerKind::Count => &self.counts,
        }
    }

    fn list_mut(&mut self, kind: MarkerKind) -> &mut Vec<Marker> {
        match kind {
            MarkerKind::ColorSampler => &mut self.samplers,
            MarkerKind::Note => &mut self.notes,
            MarkerKind::Count => &mut self.counts,
        }
    }

    pub fn markers(&self, kind: MarkerKind) -> &[Marker] {
        self.list(kind)
    }

    pub fn count(&self, kind: MarkerKind) -> usize {
        self.list(kind).len()
    }

    pub fn marker(&self, kind: MarkerKind, index: usize) -> Option<&Marker> {
        self.list(kind).get(index)
    }

    /// Place a marker, returning its index.
    ///
    /// `None` when the kind is full — only colour samplers have a limit, and
    /// Photoshop simply refuses another rather than evicting one.
    pub fn add(&mut self, kind: MarkerKind, x: i32, y: i32) -> Option<usize> {
        if kind == MarkerKind::ColorSampler && self.samplers.len() >= MAX_COLOR_SAMPLERS {
            return None;
        }
        let list = self.list_mut(kind);
        list.push(Marker::new(x, y));
        Some(list.len() - 1)
    }

    pub fn move_marker(&mut self, kind: MarkerKind, index: usize, x: i32, y: i32) -> bool {
        match self.list_mut(kind).get_mut(index) {
            Some(marker) => {
                marker.x = x;
                marker.y = y;
                true
            }
            None => false,
        }
    }

    pub fn set_text(&mut self, kind: MarkerKind, index: usize, text: impl Into<String>) -> bool {
        match self.list_mut(kind).get_mut(index) {
            Some(marker) => {
                marker.text = text.into();
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, kind: MarkerKind, index: usize) -> bool {
        let list = self.list_mut(kind);
        if index >= list.len() {
            return false;
        }
        list.remove(index);
        true
    }

    pub fn clear(&mut self, kind: MarkerKind) {
        self.list_mut(kind).clear();
    }

    /// Index of the marker within `radius` of a point, nearest first.
    ///
    /// The shell hit-tests in document space, so `radius` is scaled by zoom on
    /// its side to keep the grab area a constant size on screen.
    pub fn marker_at(&self, kind: MarkerKind, x: i32, y: i32, radius: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, marker) in self.list(kind).iter().enumerate() {
            let d = ((marker.x - x) as f32).hypot((marker.y - y) as f32);
            if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    pub fn ruler(&self) -> Option<Ruler> {
        self.ruler
    }

    pub fn set_ruler(&mut self, ruler: Ruler) {
        self.ruler = Some(ruler);
    }

    pub fn clear_ruler(&mut self) {
        self.ruler = None;
    }

    /// True when the document carries nothing at all.
    pub fn is_empty(&self) -> bool {
        self.samplers.is_empty()
            && self.notes.is_empty()
            && self.counts.is_empty()
            && self.ruler.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn markers_of_different_kinds_are_independent() {
        let mut a = Annotations::new();
        a.add(MarkerKind::ColorSampler, 1, 1);
        a.add(MarkerKind::Note, 2, 2);
        a.add(MarkerKind::Count, 3, 3);
        a.add(MarkerKind::Count, 4, 4);

        assert_eq!(a.count(MarkerKind::ColorSampler), 1);
        assert_eq!(a.count(MarkerKind::Note), 1);
        assert_eq!(a.count(MarkerKind::Count), 2);

        a.clear(MarkerKind::Count);
        assert_eq!(a.count(MarkerKind::Count), 0);
        assert_eq!(a.count(MarkerKind::Note), 1, "clearing counts hit the notes");
    }

    #[test]
    fn colour_samplers_stop_at_the_limit() {
        let mut a = Annotations::new();
        for i in 0..MAX_COLOR_SAMPLERS {
            assert_eq!(a.add(MarkerKind::ColorSampler, i as i32, 0), Some(i));
        }
        assert_eq!(a.add(MarkerKind::ColorSampler, 9, 9), None, "one too many was accepted");
        assert_eq!(a.count(MarkerKind::ColorSampler), MAX_COLOR_SAMPLERS);

        // Removing one makes room again.
        a.remove(MarkerKind::ColorSampler, 0);
        assert_eq!(
            a.add(MarkerKind::ColorSampler, 9, 9),
            Some(MAX_COLOR_SAMPLERS - 1)
        );
    }

    #[test]
    fn other_marker_kinds_are_unlimited() {
        let mut a = Annotations::new();
        for i in 0..50 {
            assert!(a.add(MarkerKind::Count, i, 0).is_some());
        }
        assert_eq!(a.count(MarkerKind::Count), 50);
    }

    #[test]
    fn hit_testing_picks_the_nearest_within_range() {
        let mut a = Annotations::new();
        a.add(MarkerKind::Count, 10, 10);
        a.add(MarkerKind::Count, 14, 10);

        assert_eq!(a.marker_at(MarkerKind::Count, 11, 10, 5.0), Some(0));
        assert_eq!(a.marker_at(MarkerKind::Count, 13, 10, 5.0), Some(1));
        assert_eq!(a.marker_at(MarkerKind::Count, 40, 40, 5.0), None, "matched far away");
        // Nothing of that kind here, even though a Note is.
        a.add(MarkerKind::Note, 40, 40);
        assert_eq!(a.marker_at(MarkerKind::Count, 40, 40, 5.0), None);
    }

    #[test]
    fn removing_shifts_the_later_indices_down() {
        let mut a = Annotations::new();
        for i in 0..3 {
            a.add(MarkerKind::Count, i * 10, 0);
        }
        assert!(a.remove(MarkerKind::Count, 0));
        assert_eq!(a.marker(MarkerKind::Count, 0).unwrap().x, 10);
        assert_eq!(a.count(MarkerKind::Count), 2);
        assert!(!a.remove(MarkerKind::Count, 9), "out-of-range removal reported success");
    }

    #[test]
    fn note_text_round_trips() {
        let mut a = Annotations::new();
        let i = a.add(MarkerKind::Note, 5, 5).unwrap();
        assert!(a.set_text(MarkerKind::Note, i, "check this edge"));
        assert_eq!(a.marker(MarkerKind::Note, i).unwrap().text, "check this edge");
        assert!(!a.set_text(MarkerKind::Note, 9, "nope"));
    }

    #[test]
    fn moving_a_marker_relocates_it() {
        let mut a = Annotations::new();
        a.add(MarkerKind::ColorSampler, 1, 1);
        assert!(a.move_marker(MarkerKind::ColorSampler, 0, 30, 40));
        let m = a.marker(MarkerKind::ColorSampler, 0).unwrap();
        assert_eq!((m.x, m.y), (30, 40));
        assert!(!a.move_marker(MarkerKind::ColorSampler, 5, 0, 0));
    }

    #[test]
    fn a_horizontal_ruler_reads_zero_degrees() {
        let m = Ruler::new(10.0, 20.0, 60.0, 20.0).measure();
        assert!(close(m.x, 10.0) && close(m.y, 20.0));
        assert!(close(m.width, 50.0));
        assert!(close(m.height, 0.0));
        assert!(close(m.angle, 0.0), "angle was {}", m.angle);
        assert!(close(m.distance, 50.0));
    }

    #[test]
    fn a_line_going_down_reads_as_a_negative_angle() {
        // Down-right on screen. Photoshop reports this as negative, because
        // the angle is anticlockwise-from-east while y grows downward.
        let m = Ruler::new(0.0, 0.0, 10.0, 10.0).measure();
        assert!(close(m.angle, -45.0), "angle was {}", m.angle);

        let up = Ruler::new(0.0, 10.0, 10.0, 0.0).measure();
        assert!(close(up.angle, 45.0), "angle was {}", up.angle);
    }

    #[test]
    fn distance_is_the_hypotenuse() {
        let m = Ruler::new(0.0, 0.0, 3.0, 4.0).measure();
        assert!(close(m.distance, 5.0));
        assert!(close(m.width, 3.0) && close(m.height, 4.0));
    }

    #[test]
    fn the_ruler_is_optional_and_clearable() {
        let mut a = Annotations::new();
        assert!(a.ruler().is_none());
        assert!(a.is_empty());

        a.set_ruler(Ruler::new(0.0, 0.0, 5.0, 5.0));
        assert!(a.ruler().is_some());
        assert!(!a.is_empty());

        a.clear_ruler();
        assert!(a.ruler().is_none());
        assert!(a.is_empty());
    }

    #[test]
    fn marker_kind_round_trips_through_its_integer() {
        for (v, kind) in [
            (0, MarkerKind::ColorSampler),
            (1, MarkerKind::Note),
            (2, MarkerKind::Count),
        ] {
            assert_eq!(MarkerKind::from_i32(v), Some(kind));
            assert_eq!(kind as i32, v);
        }
        assert_eq!(MarkerKind::from_i32(7), None);
    }
}
