//! Filters and adjustments.
//!
//! Two distinct things live here:
//!
//! * [`Adjustment`] — a cheap, per-pixel colour transform. Adjustments are
//!   evaluated by the compositor on the fly, which is what makes adjustment
//!   layers non-destructive.
//! * [`Filter`] — a neighbourhood operation (convolution and friends) applied
//!   destructively to a [`Pixmap`].

pub mod adjust;
pub mod artistic;
pub mod brush_strokes;
pub mod convolve;
pub mod distort;
pub mod frame;
pub mod pixelate;
pub mod render;
pub mod segment;
pub mod sketch;
pub mod stylize;
pub mod texture;

pub use adjust::Adjustment;
pub use convolve::{custom_default, gaussian_blur, sharpen, sharpen_edges, sharpen_more,
                   unsharp_mask, Kernel, RadialQuality, SharpenRemove, CUSTOM_SIZE,
                   CUSTOM_WEIGHTS};
pub use distort::{EdgeMode, RippleSize, SpherizeMode, WaveKind, ZigZagStyle, SHEAR_POINTS};
pub use pixelate::{MezzotintType, ScreenAngles, DEFAULT_SCREEN_ANGLES};
pub use artistic::DaubBrush;
pub use stylize::{ContourEdge, DiffuseMode, ExtrudeOptions, ExtrudeType, TileFill, TileOptions,
                  WindMethod};
pub use render::{FlameOptions, FlameShape, FlameStyle, FlameType, LensType, Lighting,
                 LightType, TextureChannel};

use crate::buffer::Pixmap;

/// A destructive image operation from the Filter menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Filter {
    /// Radius in pixels.
    GaussianBlur { radius: f32 },
    /// Box blur — cheaper, used for previews.
    BoxBlur { radius: u32 },
    Sharpen,
    /// CS6's other two fixed-strength sharpeners. `SharpenMore` is a heavier
    /// dose of `Sharpen`; `SharpenEdges` confines it to where there is an
    /// edge to sharpen. Neither takes a parameter, as in CS6.
    SharpenMore,
    SharpenEdges,
    /// CS6's Smart Sharpen: an unsharp mask against the kind of blur you say
    /// the softness came from, holding back detail below a noise floor.
    /// `angle` is only read for a motion streak.
    SmartSharpen {
        amount: f32,
        radius: f32,
        reduce_noise: f32,
        remove: SharpenRemove,
        angle: f32,
    },
    UnsharpMask {
        amount: f32,
        radius: f32,
        threshold: u8,
    },
    /// The detail a blur of `radius` would take out, over mid-grey — CS6's
    /// Filter ▸ Other ▸ High Pass.
    HighPass { radius: f32 },
    /// Add monochrome or colour noise. `amount` is a percentage, as CS6's
    /// slider is; `gaussian` picks its Gaussian distribution over the uniform
    /// one, which clumps rather than spreading evenly.
    Noise {
        amount: f32,
        monochromatic: bool,
        gaussian: bool,
    },
    /// The median of everything within `radius` — Filter ▸ Noise ▸ Median.
    Median { radius: u32 },
    /// The same, but only where a pixel is further than `threshold` from that
    /// median — Filter ▸ Noise ▸ Dust & Scratches.
    DustAndScratches { radius: u32, threshold: u32 },

    // Filter ▸ Distort. These move the picture about rather than mixing
    // neighbours together, so they live in their own module (`distort`) and
    // share one bilinear back end. Displace is not here: it takes a second
    // image, which no amount of numbers can carry, so it has its own path
    // through the bridge.
    /// Squeeze the middle in, or push it out. `amount` is -100 to 100.
    Pinch { amount: f32 },
    /// Wrap the picture into a disc, or unroll one back into a rectangle.
    PolarCoordinates { to_polar: bool },
    /// Ripple the picture as though seen through water.
    Ripple { amount: f32, size: RippleSize },
    /// Push each row sideways by an amount read off a curve.
    Shear {
        offsets: [f32; SHEAR_POINTS],
        wrap: bool,
    },
    /// Wrap the picture onto a ball, or press it into a bowl.
    Spherize { amount: f32, mode: SpherizeMode },
    /// Wind the picture into a spiral. `angle` in degrees.
    Twirl { angle: f32 },
    /// A sum of waves. `seed` is what CS6's Randomize button re-rolls.
    Wave {
        generators: u32,
        wavelength: (f32, f32),
        amplitude: (f32, f32),
        scale: (f32, f32),
        kind: WaveKind,
        wrap: bool,
        seed: u32,
    },
    /// Break the picture into irregular polygonal cells, each filled with the
    /// average of what was under it — Filter ▸ Pixelate ▸ Crystallize — and
    /// the same thing on a plain square grid, which is Mosaic.
    Crystallize { cell_size: u32 },
    Mosaic { cell_size: u32 },
    /// Rebuild the picture out of scattered dabs on a ground of the
    /// background colour — Filter ▸ Pixelate ▸ Pointillize. The colour is the
    /// document's rather than anything the dialog asks for, so the bridge
    /// fills it in; see `Engine::filter_for`.
    Pointillize {
        cell_size: u32,
        background: crate::buffer::Rgba8,
    },
    /// Clump neighbouring pixels of similar colour into flat patches, and
    /// four shifted copies averaged together. Neither takes a parameter, as
    /// in CS6 — they are meant to be repeated with Ctrl+F.
    Facet,
    Fragment,
    /// Throw every channel to one end or the other on a random pattern —
    /// Filter ▸ Pixelate ▸ Mezzotint.
    Mezzotint { kind: MezzotintType },
    /// Redraw the picture as a four-colour press would — Filter ▸ Pixelate ▸
    /// Color Halftone. `max_radius` is the size of a dot at full ink.
    ColorHalftone {
        max_radius: f32,
        angles: ScreenAngles,
    },
    /// Break the picture into rings that alternate the way they are pushed.
    ZigZag {
        amount: f32,
        ridges: u32,
        style: ZigZagStyle,
    },

    // The rest of CS6's Blur submenu. All of them are per-pixel work over a
    // neighbourhood and would suit the GPU (CLAUDE.md §7), but none is
    // enabled there: the backend's one shader is the Gaussian, and the
    // measurements in docs/gpu-migration.md are that a single filter which
    // uploads its input and reads the result straight back rarely pays for
    // the trip. They belong in a batch with the rest of the filter stack, if
    // that is ever done, rather than one at a time.
    /// One flat colour: the mean of everything in range.
    Average,
    /// CS6's fixed-strength blurs, which take no radius — a soft 3×3 and a
    /// stronger one. The numbers are what makes them "Blur" and "Blur More"
    /// rather than a Gaussian you have to dial in.
    Blur,
    BlurMore,
    /// `angle` in degrees, `distance` in pixels.
    MotionBlur { angle: f32, distance: f32 },
    /// `amount` as a percentage; `spin` turns about the centre rather than
    /// running in and out from it. `center` is in normalized 0..1 coordinates
    /// — CS6's Blur Center box, which is draggable and defaults to the middle
    /// — and `quality` is how densely the path each pixel travels is sampled.
    RadialBlur {
        amount: f32,
        spin: bool,
        center: (f32, f32),
        quality: RadialQuality,
    },
    /// Blurs within a region but not across its edges: a neighbour counts
    /// only if it is within `threshold` of the centre pixel.
    SurfaceBlur { radius: u32, threshold: u32 },
    /// Generate a grayscale cloud-like fractal noise pattern. `difference`
    /// blends into existing pixels rather than replacing them — CS6's
    /// Filter ▸ Render ▸ Clouds and Difference Clouds.
    Clouds { difference: bool },
    /// Long streaks drawn between the foreground and background colours —
    /// Filter ▸ Render ▸ Fibers. `variance` (CS6's 0–64) is how much the
    /// fibres swing and how fine they break up, `strength` (1–64) how far
    /// each streak runs, and `seed` is what the Randomize button re-rolls. The colours are the
    /// document's foreground and background — the dialog does not ask for
    /// them, so the bridge fills them in; see `Engine::filter_for`.
    Fibers {
        variance: f32,
        strength: f32,
        seed: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// A decorative border of vines, flowers and leaves, or a ruled or
    /// moulded frame — CS6's Picture Frame.
    PictureFrame { options: frame::FrameOptions },
    /// Light thrown into the picture by the lens it was taken with —
    /// Filter ▸ Render ▸ Lens Flare. `center` is where the sun is, in
    /// fractions of the width and height, which is what the dialog's
    /// draggable crosshair sets; `brightness` is CS6's 10–300%; `lens` is
    /// which of its four lenses threw it.
    LensFlare {
        center: (f32, f32),
        brightness: f32,
        lens: render::LensType,
    },
    /// Re-light the picture as though a lamp were shining on it — Filter ▸
    /// Render ▸ Lighting Effects. One lamp rather than CS6's panel of them;
    /// see [`render::Lighting`].
    Lighting { light: render::Lighting },
    /// Shuffle each pixel with one of its neighbours — Filter ▸ Stylize ▸
    /// Diffuse. The mode decides which neighbour wins, and Anisotropic is the
    /// odd one that smooths along edges instead of shuffling at all.
    Diffuse { mode: stylize::DiffuseMode },
    /// Stamp the picture into metal — Filter ▸ Stylize ▸ Emboss. `angle` is
    /// where the light comes from in degrees, `height` how thick the relief
    /// stands in pixels, and `amount` CS6's 1–500%.
    Emboss {
        angle: f32,
        height: f32,
        amount: f32,
    },
    /// Break the picture into towers standing out of the frame — Filter ▸
    /// Stylize ▸ Extrude.
    Extrude { options: stylize::ExtrudeOptions },
    /// Draw the picture's edges as dark lines on white — Filter ▸ Stylize ▸
    /// Find Edges. Takes no parameters, as in CS6.
    FindEdges,
    /// Fold the tonal range back on itself — Filter ▸ Stylize ▸ Solarize.
    /// Takes no parameters, as in CS6.
    Solarize,
    /// Cut the picture into squares and nudge each off where it was — Filter ▸
    /// Stylize ▸ Tiles. The two swatch colours in `options` come from the
    /// document rather than the dialog; the bridge fills them in.
    Tiles { options: stylize::TileOptions },
    /// Draw the line where each channel crosses a brightness — Filter ▸
    /// Stylize ▸ Trace Contour.
    TraceContour { level: u8, edge: stylize::ContourEdge },
    /// The convolution the user writes out by hand — Filter ▸ Other ▸ Custom.
    /// `weights` reads row by row from the top left of CS6's 5×5 grid,
    /// `scale` divides the total and `offset` is added to it.
    Custom {
        weights: [f32; CUSTOM_WEIGHTS],
        scale: f32,
        offset: f32,
    },
    /// The picture redrawn in pencil on paper — CS6's Colored Pencil, which
    /// it keeps in the Filter Gallery rather than in the Filter menu.
    ColoredPencil {
        width: u32,
        pressure: u32,
        paper: u32,
    },
    /// The picture rebuilt out of pieces of coloured paper — CS6's Cutout,
    /// another of the Filter Gallery's.
    Cutout {
        levels: u32,
        simplicity: u32,
        fidelity: u32,
    },
    /// The picture repainted with a stiff, half-dry brush — CS6's Dry Brush,
    /// another of the Filter Gallery's.
    DryBrush {
        size: u32,
        detail: u32,
        texture: u32,
    },
    /// The picture as a fast film would have taken it — CS6's Film Grain,
    /// another of the Filter Gallery's.
    FilmGrain {
        grain: u32,
        highlight_area: u32,
        intensity: u32,
    },
    /// The picture laid into wet plaster — CS6's Fresco, which shares Dry
    /// Brush's three sliders.
    Fresco {
        size: u32,
        detail: u32,
        texture: u32,
    },
    /// The picture repainted in daubs of one colour — CS6's Paint Daubs.
    PaintDaubs {
        size: u32,
        sharpness: u32,
        brush: artistic::DaubBrush,
    },
    /// The picture spread with a knife — CS6's Palette Knife.
    PaletteKnife {
        size: u32,
        detail: u32,
        softness: u32,
    },
    /// The picture drawn in chalk on a textured surface — CS6's Rough
    /// Pastels.
    RoughPastels {
        length: u32,
        detail: u32,
        texture: texture::Texture,
        scaling: u32,
        relief: u32,
        light: texture::Light,
        invert: bool,
    },
    /// The picture laid in broadly on a textured surface — CS6's
    /// Underpainting.
    Underpainting {
        size: u32,
        coverage: u32,
        texture: texture::Texture,
        scaling: u32,
        relief: u32,
        light: texture::Light,
        invert: bool,
    },
    /// The picture's boundaries drawn over it in light or dark — CS6's
    /// Accented Edges.
    AccentedEdges {
        width: u32,
        brightness: u32,
        smoothness: u32,
    },
    /// The picture as an airbrush would spatter it — CS6's Spatter.
    Spatter {
        radius: u32,
        smoothness: u32,
    },
    /// The picture repainted in angled, scattered strokes — CS6's Sprayed
    /// Strokes.
    SprayedStrokes {
        length: u32,
        radius: u32,
        direction: brush_strokes::StrokeDirection,
    },
    /// The picture carved in shallow relief and lit from one side, in the two
    /// swatches — CS6's Bas Relief. The colours are the document's, so the
    /// bridge fills them in; see `Engine::filter_for`.
    BasRelief {
        detail: u32,
        smoothness: u32,
        light: texture::Light,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture redrawn in charcoal and chalk over a mid-grey ground —
    /// CS6's Chalk & Charcoal. The colours are the document's, so the bridge
    /// fills them in; see `Engine::filter_for`.
    ChalkAndCharcoal {
        charcoal_area: u32,
        chalk_area: u32,
        pressure: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture redrawn as a charcoal sketch on bare paper — CS6's
    /// Charcoal. The colours are the document's, so the bridge fills them in;
    /// see `Engine::filter_for`.
    Charcoal {
        thickness: u32,
        detail: u32,
        balance: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture as a sheet of polished metal — CS6's Chrome. Grey
    /// whatever the swatches are, unlike the rest of the Sketch family.
    Chrome {
        detail: u32,
        smoothness: u32,
    },
    /// The picture drawn in a waxy stick on textured paper — CS6's Conté
    /// Crayon. The colours are the document's, so the bridge fills them in;
    /// see `Engine::filter_for`.
    ConteCrayon {
        foreground_level: u32,
        background_level: u32,
        texture: texture::Texture,
        scaling: u32,
        relief: u32,
        light: texture::Light,
        invert: bool,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture drawn in fine parallel ink lines — CS6's Graphic Pen. The
    /// colours are the document's, so the bridge fills them in; see
    /// `Engine::filter_for`.
    GraphicPen {
        stroke_length: u32,
        balance: u32,
        direction: brush_strokes::StrokeDirection,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture ruled into a screen of dots, rings or lines — CS6's
    /// Halftone Pattern. The colours are the document's, so the bridge fills
    /// them in; see `Engine::filter_for`.
    HalftonePattern {
        size: u32,
        contrast: u32,
        pattern: sketch::HalftonePattern,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture cut out of a sheet of handmade paper — CS6's Note Paper.
    /// The colours are the document's, so the bridge fills them in; see
    /// `Engine::filter_for`.
    NotePaper {
        balance: u32,
        graininess: u32,
        relief: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture as a photocopier sees it — CS6's Photocopy. The colours
    /// are the document's, so the bridge fills them in; see
    /// `Engine::filter_for`.
    Photocopy {
        detail: u32,
        darkness: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture poured in plaster and lit from one side — CS6's Plaster.
    /// The colours are the document's, so the bridge fills them in; see
    /// `Engine::filter_for`.
    Plaster {
        balance: u32,
        smoothness: u32,
        light: texture::Light,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture as film whose emulsion has clumped — CS6's Reticulation.
    /// The colours are the document's, so the bridge fills them in; see
    /// `Engine::filter_for`.
    Reticulation {
        density: u32,
        foreground_level: u32,
        background_level: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture cut as a rubber stamp — CS6's Stamp. The colours are the
    /// document's, so the bridge fills them in; see `Engine::filter_for`.
    Stamp {
        balance: u32,
        smoothness: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture torn out of paper — CS6's Torn Edges. The colours are the
    /// document's, so the bridge fills them in; see `Engine::filter_for`.
    TornEdges {
        balance: u32,
        smoothness: u32,
        contrast: u32,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture printed through grain — CS6's Grain. Sprinkles, Speckle
    /// and Stippled use the document's colours, so the bridge fills them in;
    /// see `Engine::filter_for`.
    Grain {
        intensity: u32,
        contrast: u32,
        kind: texture::GrainType,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture as raised squares of its average colours — CS6's
    /// Patchwork.
    Patchwork {
        square: u32,
        relief: u32,
    },
    /// The picture laid in tiles with grout between — CS6's Mosaic Tiles.
    MosaicTiles {
        size: u32,
        grout: u32,
        lighten: u32,
    },
    /// The picture printed on a surface of brick, burlap, canvas or
    /// sandstone — CS6's Texturizer.
    Texturizer {
        texture: texture::Texture,
        scaling: u32,
        relief: u32,
        light: texture::Light,
        invert: bool,
    },
    /// The picture remade as panes of flat colour held in lead — CS6's
    /// Stained Glass. The lead is the foreground colour; see
    /// `Engine::filter_for`.
    StainedGlass {
        cell_size: u32,
        border: u32,
        light: u32,
        foreground: crate::buffer::Rgba8,
    },
    /// The picture painted onto cracked plaster — CS6's Craquelure.
    Craquelure {
        spacing: u32,
        depth: u32,
        brightness: u32,
    },
    /// The picture daubed onto damp, fibrous paper — CS6's Water Paper. It
    /// keeps the picture's colours, so, like Chrome, it takes no swatches.
    WaterPaper {
        fiber: u32,
        brightness: u32,
        contrast: u32,
    },
    /// The picture painted in Japanese ink wash — CS6's Sumi-e.
    SumiE {
        width: u32,
        pressure: u32,
        contrast: u32,
    },
    /// The picture drawn over in fine pen lines — CS6's Ink Outlines.
    InkOutlines {
        length: u32,
        dark: u32,
        light: u32,
    },
    /// The picture stroked and driven to black and white — CS6's Dark
    /// Strokes.
    DarkStrokes {
        balance: u32,
        black: u32,
        white: u32,
    },
    /// The picture drawn over in strokes both ways — CS6's Crosshatch.
    Crosshatch {
        length: u32,
        sharpness: u32,
        strength: u32,
    },
    /// The picture repainted in diagonal strokes — CS6's Angled Strokes.
    AngledStrokes {
        balance: u32,
        length: u32,
        sharpness: u32,
    },
    /// The picture washed in with a wet brush — CS6's Watercolor.
    Watercolor {
        detail: u32,
        shadow: u32,
        texture: u32,
    },
    /// The picture dabbed on with a sponge — CS6's Sponge.
    Sponge {
        size: u32,
        definition: u32,
        smoothness: u32,
    },
    /// The picture's darks smudged and its lights brightened — CS6's Smudge
    /// Stick.
    SmudgeStick {
        length: u32,
        highlight: u32,
        intensity: u32,
    },
    /// The picture posterized with its edges inked — CS6's Poster Edges.
    PosterEdges {
        thickness: u32,
        intensity: u32,
        posterization: u32,
    },
    /// The picture shrink-wrapped in glossy plastic — CS6's Plastic Wrap.
    PlasticWrap {
        highlight: u32,
        detail: u32,
        smoothness: u32,
    },
    /// The picture lit by a tube of one colour — CS6's Neon Glow. Only `glow`
    /// comes from the dialog; the two the picture is rendered between are the
    /// document's swatches, which the bridge fills in.
    NeonGlow {
        size: i32,
        brightness: u32,
        glow: crate::buffer::Rgba8,
        foreground: crate::buffer::Rgba8,
        background: crate::buffer::Rgba8,
    },
    /// The picture's edges lit up on a black ground — CS6's Glowing Edges,
    /// which it keeps in the Filter Gallery rather than the Stylize submenu.
    GlowingEdges {
        width: u32,
        brightness: u32,
        smoothness: u32,
    },
    /// Blow the picture sideways off its edges — Filter ▸ Stylize ▸ Wind.
    /// `from_right` is CS6's Direction, naming the side the wind comes from.
    Wind {
        method: stylize::WindMethod,
        from_right: bool,
    },
}

impl Filter {
    pub fn name(&self) -> &'static str {
        match self {
            Filter::GaussianBlur { .. } => "Gaussian Blur",
            Filter::BoxBlur { .. } => "Box Blur",
            Filter::Sharpen => "Sharpen",
            Filter::SharpenMore => "Sharpen More",
            Filter::SharpenEdges => "Sharpen Edges",
            Filter::SmartSharpen { .. } => "Smart Sharpen",
            Filter::UnsharpMask { .. } => "Unsharp Mask",
            Filter::HighPass { .. } => "High Pass",
            Filter::Noise { .. } => "Add Noise",
            Filter::Median { .. } => "Median",
            Filter::DustAndScratches { .. } => "Dust & Scratches",
            Filter::Pinch { .. } => "Pinch",
            Filter::PolarCoordinates { .. } => "Polar Coordinates",
            Filter::Ripple { .. } => "Ripple",
            Filter::Shear { .. } => "Shear",
            Filter::Spherize { .. } => "Spherize",
            Filter::Twirl { .. } => "Twirl",
            Filter::Wave { .. } => "Wave",
            Filter::ZigZag { .. } => "ZigZag",
            Filter::ColorHalftone { .. } => "Color Halftone",
            Filter::Crystallize { .. } => "Crystallize",
            Filter::Mosaic { .. } => "Mosaic",
            Filter::Pointillize { .. } => "Pointillize",
            Filter::Facet => "Facet",
            Filter::Fragment => "Fragment",
            Filter::Mezzotint { .. } => "Mezzotint",
            Filter::Average => "Average",
            Filter::Blur => "Blur",
            Filter::BlurMore => "Blur More",
            Filter::MotionBlur { .. } => "Motion Blur",
            Filter::RadialBlur { .. } => "Radial Blur",
            Filter::SurfaceBlur { .. } => "Surface Blur",
            Filter::Clouds { difference: false } => "Clouds",
            Filter::Clouds { difference: true } => "Difference Clouds",
            Filter::Fibers { .. } => "Fibers",
            Filter::LensFlare { .. } => "Lens Flare",
            Filter::PictureFrame { .. } => "Picture Frame",
            Filter::Lighting { .. } => "Lighting Effects",
            Filter::Diffuse { .. } => "Diffuse",
            Filter::Emboss { .. } => "Emboss",
            Filter::Extrude { .. } => "Extrude",
            Filter::FindEdges => "Find Edges",
            Filter::Solarize => "Solarize",
            Filter::Tiles { .. } => "Tiles",
            Filter::TraceContour { .. } => "Trace Contour",
            Filter::Wind { .. } => "Wind",
            Filter::Custom { .. } => "Custom",
            Filter::GlowingEdges { .. } => "Glowing Edges",
            Filter::ColoredPencil { .. } => "Colored Pencil",
            Filter::Cutout { .. } => "Cutout",
            Filter::DryBrush { .. } => "Dry Brush",
            Filter::FilmGrain { .. } => "Film Grain",
            Filter::Fresco { .. } => "Fresco",
            Filter::NeonGlow { .. } => "Neon Glow",
            Filter::PaintDaubs { .. } => "Paint Daubs",
            Filter::PaletteKnife { .. } => "Palette Knife",
            Filter::PlasticWrap { .. } => "Plastic Wrap",
            Filter::PosterEdges { .. } => "Poster Edges",
            Filter::RoughPastels { .. } => "Rough Pastels",
            Filter::SmudgeStick { .. } => "Smudge Stick",
            Filter::Sponge { .. } => "Sponge",
            Filter::Watercolor { .. } => "Watercolor",
            Filter::AccentedEdges { .. } => "Accented Edges",
            Filter::AngledStrokes { .. } => "Angled Strokes",
            Filter::Crosshatch { .. } => "Crosshatch",
            Filter::DarkStrokes { .. } => "Dark Strokes",
            Filter::InkOutlines { .. } => "Ink Outlines",
            Filter::Spatter { .. } => "Spatter",
            Filter::SprayedStrokes { .. } => "Sprayed Strokes",
            Filter::SumiE { .. } => "Sumi-e",
            Filter::BasRelief { .. } => "Bas Relief",
            Filter::ChalkAndCharcoal { .. } => "Chalk & Charcoal",
            Filter::Charcoal { .. } => "Charcoal",
            Filter::Chrome { .. } => "Chrome",
            Filter::ConteCrayon { .. } => "Conte Crayon",
            Filter::GraphicPen { .. } => "Graphic Pen",
            Filter::HalftonePattern { .. } => "Halftone Pattern",
            Filter::NotePaper { .. } => "Note Paper",
            Filter::Photocopy { .. } => "Photocopy",
            Filter::Plaster { .. } => "Plaster",
            Filter::Reticulation { .. } => "Reticulation",
            Filter::Stamp { .. } => "Stamp",
            Filter::TornEdges { .. } => "Torn Edges",
            Filter::WaterPaper { .. } => "Water Paper",
            Filter::Craquelure { .. } => "Craquelure",
            Filter::Grain { .. } => "Grain",
            Filter::MosaicTiles { .. } => "Mosaic Tiles",
            Filter::Patchwork { .. } => "Patchwork",
            Filter::StainedGlass { .. } => "Stained Glass",
            Filter::Texturizer { .. } => "Texturizer",
            Filter::Underpainting { .. } => "Underpainting",
        }
    }

    /// Build a filter from the name the Filter menu uses, and the numbers its
    /// dialog collected.
    ///
    /// The shell speaks in menu names because that is what it has — a menu
    /// item and the values from its dialog — and this keeps the mapping in one
    /// place where both `applyFilter` and the preview can reach it. An
    /// unrecognised name is `None` rather than a guess.
    ///
    /// `p` is positional, in the order the dialog lists its controls, and a
    /// value the dialog did not offer reads as zero. Slice rather than a fixed
    /// array because the count is a property of the filter, not of the bridge:
    /// Blur wants one, Radial Blur five, Wave ten, and Shear a whole sampled
    /// curve.
    pub fn from_menu_name(name: &str, p: &[f32]) -> Option<Filter> {
        let at = |i: usize| p.get(i).copied().unwrap_or(0.0);
        let (p1, p2, p3, p4, p5) = (at(0), at(1), at(2), at(3), at(4));
        Some(match name {
            "Gaussian Blur" => Filter::GaussianBlur { radius: p1.max(0.0) },
            "Box Blur" => Filter::BoxBlur {
                radius: p1.max(0.0) as u32,
            },
            "Average" => Filter::Average,
            "Blur" => Filter::Blur,
            "Blur More" => Filter::BlurMore,
            "Motion Blur" => Filter::MotionBlur {
                angle: p1,
                distance: p2.max(0.0),
            },
            "Radial Blur" => Filter::RadialBlur {
                amount: p1.max(0.0),
                spin: p2 != 0.0,
                center: (p3, p4),
                quality: RadialQuality::from_i32(p5 as i32),
            },
            "Surface Blur" => Filter::SurfaceBlur {
                radius: p1.max(0.0) as u32,
                threshold: p2.max(0.0) as u32,
            },
            "Sharpen" => Filter::Sharpen,
            "Sharpen More" => Filter::SharpenMore,
            "Sharpen Edges" => Filter::SharpenEdges,
            "Smart Sharpen" => Filter::SmartSharpen {
                amount: p1.max(0.0),
                radius: p2.max(0.0),
                reduce_noise: p3.clamp(0.0, 100.0),
                remove: SharpenRemove::from_i32(p4 as i32),
                angle: p5,
            },
            "High Pass" => Filter::HighPass { radius: p1 },
            "Unsharp Mask" => Filter::UnsharpMask {
                amount: p1,
                radius: p2,
                threshold: 0,
            },
            "Add Noise" => Filter::Noise {
                amount: p1.max(0.0),
                gaussian: p2 != 0.0,
                monochromatic: p3 != 0.0,
            },
            "Median" => Filter::Median {
                radius: p1.max(0.0) as u32,
            },
            "Dust & Scratches" => Filter::DustAndScratches {
                radius: p1.max(0.0) as u32,
                threshold: p2.max(0.0) as u32,
            },
            "Pinch" => Filter::Pinch { amount: p1 },
            "Polar Coordinates" => Filter::PolarCoordinates {
                to_polar: p1 != 0.0,
            },
            "Ripple" => Filter::Ripple {
                amount: p1,
                size: RippleSize::from_i32(p2 as i32),
            },
            "Shear" => {
                // The curve first, then the Undefined Areas flag after it, so
                // that adding a point to the curve would not renumber it.
                let mut offsets = [0.0f32; SHEAR_POINTS];
                for (i, slot) in offsets.iter_mut().enumerate() {
                    *slot = at(i);
                }
                Filter::Shear {
                    offsets,
                    wrap: at(SHEAR_POINTS) != 0.0,
                }
            }
            "Spherize" => Filter::Spherize {
                amount: p1,
                mode: SpherizeMode::from_i32(p2 as i32),
            },
            "Twirl" => Filter::Twirl { angle: p1 },
            "Wave" => Filter::Wave {
                generators: p1.max(1.0) as u32,
                wavelength: (p2, p3),
                amplitude: (p4, p5),
                scale: (at(5), at(6)),
                kind: WaveKind::from_i32(at(7) as i32),
                wrap: at(8) != 0.0,
                seed: at(9) as u32,
            },
            "Facet" => Filter::Facet,
            "Fragment" => Filter::Fragment,
            "Mezzotint" => Filter::Mezzotint {
                kind: MezzotintType::from_i32(p1 as i32),
            },
            "Crystallize" => Filter::Crystallize {
                cell_size: p1.max(1.0) as u32,
            },
            "Mosaic" => Filter::Mosaic {
                cell_size: p1.max(1.0) as u32,
            },
            "Pointillize" => Filter::Pointillize {
                cell_size: p1.max(1.0) as u32,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Color Halftone" => Filter::ColorHalftone {
                max_radius: p1.max(1.0),
                // Falling back to the standard press angles rather than to
                // zero, which would stack all four screens on top of one
                // another and produce a plaid instead of a halftone.
                angles: [
                    p.get(1).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[0]),
                    p.get(2).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[1]),
                    p.get(3).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[2]),
                    p.get(4).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[3]),
                ],
            },
            "ZigZag" => Filter::ZigZag {
                amount: p1,
                ridges: p2.max(1.0) as u32,
                style: ZigZagStyle::from_i32(p3 as i32),
            },
            // In the order CS6's Properties panel reads down the page, with
            // the three the panel does not have — where the lamp is, how far
            // it reaches and which way it points, all handles on the canvas
            // there — on the end.
            "Lighting Effects" => {
                let fallback = render::Lighting::default();
                let at = |i: usize, or: f32| p.get(i).copied().unwrap_or(or);
                let colour = |i: usize| {
                    crate::buffer::Rgba8::new(
                        at(i, 255.0).clamp(0.0, 255.0) as u8,
                        at(i + 1, 255.0).clamp(0.0, 255.0) as u8,
                        at(i + 2, 255.0).clamp(0.0, 255.0) as u8,
                        255,
                    )
                };
                Filter::Lighting {
                    light: render::Lighting {
                        kind: render::LightType::from_i32(at(0, 0.0) as i32),
                        color: colour(1),
                        intensity: at(4, fallback.intensity).clamp(-100.0, 100.0),
                        hotspot: at(5, fallback.hotspot).clamp(-100.0, 100.0),
                        colorize: colour(6),
                        exposure: at(9, 0.0).clamp(-100.0, 100.0),
                        gloss: at(10, 0.0).clamp(-100.0, 100.0),
                        metallic: at(11, 0.0).clamp(-100.0, 100.0),
                        ambience: at(12, 0.0).clamp(-100.0, 100.0),
                        texture: render::TextureChannel::from_i32(at(13, 0.0) as i32),
                        height: at(14, fallback.height).clamp(0.0, 100.0),
                        // The dialog offers this as a percentage of the
                        // frame, which is a size a person can think in; the
                        // engine wants the fraction.
                        size: (at(15, fallback.size * 100.0) / 100.0).clamp(0.02, 3.0),
                        angle: at(16, fallback.angle),
                        center: (
                            at(17, 0.5).clamp(0.0, 1.0),
                            at(18, 0.5).clamp(0.0, 1.0),
                        ),
                    },
                }
            }
            "Diffuse" => Filter::Diffuse {
                mode: stylize::DiffuseMode::from_i32(p1 as i32),
            },
            "Find Edges" => Filter::FindEdges,
            "Solarize" => Filter::Solarize,
            // The grid row by row, then Scale and Offset — the order the
            // dialog reads down the page. A field left empty is zero, which
            // is what an absent parameter already reads as.
            "Custom" => {
                let mut weights = [0.0f32; CUSTOM_WEIGHTS];
                for (i, w) in weights.iter_mut().enumerate() {
                    *w = at(i);
                }
                Filter::Custom {
                    weights,
                    scale: at(CUSTOM_WEIGHTS),
                    offset: at(CUSTOM_WEIGHTS + 1),
                }
            }
            "Colored Pencil" => Filter::ColoredPencil {
                width: p1.max(0.0) as u32,
                pressure: p2.max(0.0) as u32,
                paper: p3.max(0.0) as u32,
            },
            "Cutout" => Filter::Cutout {
                levels: p1.max(0.0) as u32,
                simplicity: p2.max(0.0) as u32,
                fidelity: p3.max(0.0) as u32,
            },
            "Dry Brush" => Filter::DryBrush {
                size: p1.max(0.0) as u32,
                detail: p2.max(0.0) as u32,
                texture: p3.max(0.0) as u32,
            },
            "Film Grain" => Filter::FilmGrain {
                grain: p1.max(0.0) as u32,
                highlight_area: p2.max(0.0) as u32,
                intensity: p3.max(0.0) as u32,
            },
            "Fresco" => Filter::Fresco {
                size: p1.max(0.0) as u32,
                detail: p2.max(0.0) as u32,
                texture: p3.max(0.0) as u32,
            },
            "Palette Knife" => Filter::PaletteKnife {
                size: p1.max(0.0) as u32,
                detail: p2.max(0.0) as u32,
                softness: p3.max(0.0) as u32,
            },
            "Underpainting" => Filter::Underpainting {
                size: p1.max(0.0) as u32,
                coverage: p2.max(0.0) as u32,
                texture: texture::Texture::from_i32(p3 as i32),
                scaling: p4.max(0.0) as u32,
                relief: p5.max(0.0) as u32,
                light: texture::Light::from_i32(at(5) as i32),
                invert: at(6) != 0.0,
            },
            "Spatter" => Filter::Spatter {
                radius: p1.max(0.0) as u32,
                smoothness: p2.max(0.0) as u32,
            },
            "Sprayed Strokes" => Filter::SprayedStrokes {
                length: p1.max(0.0) as u32,
                radius: p2.max(0.0) as u32,
                direction: brush_strokes::StrokeDirection::from_i32(p3 as i32),
            },
            "Conte Crayon" => Filter::ConteCrayon {
                foreground_level: p1.max(0.0) as u32,
                background_level: p2.max(0.0) as u32,
                texture: texture::Texture::from_i32(p3 as i32),
                scaling: at(3).max(0.0) as u32,
                relief: at(4).max(0.0) as u32,
                light: texture::Light::from_i32(at(5) as i32),
                invert: at(6) != 0.0,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Graphic Pen" => Filter::GraphicPen {
                stroke_length: p1.max(0.0) as u32,
                balance: p2.max(0.0) as u32,
                direction: brush_strokes::StrokeDirection::from_i32(p3 as i32),
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Halftone Pattern" => Filter::HalftonePattern {
                size: p1.max(0.0) as u32,
                contrast: p2.max(0.0) as u32,
                pattern: sketch::HalftonePattern::from_i32(p3 as i32),
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Note Paper" => Filter::NotePaper {
                balance: p1.max(0.0) as u32,
                graininess: p2.max(0.0) as u32,
                relief: p3.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Photocopy" => Filter::Photocopy {
                detail: p1.max(0.0) as u32,
                darkness: p2.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Torn Edges" => Filter::TornEdges {
                balance: p1.max(0.0) as u32,
                smoothness: p2.max(0.0) as u32,
                contrast: p3.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Patchwork" => Filter::Patchwork {
                square: p1.max(0.0) as u32,
                relief: p2.max(0.0) as u32,
            },
            "Mosaic Tiles" => Filter::MosaicTiles {
                size: p1.max(0.0) as u32,
                grout: p2.max(0.0) as u32,
                lighten: p3.max(0.0) as u32,
            },
            "Grain" => Filter::Grain {
                intensity: p1.max(0.0) as u32,
                contrast: p2.max(0.0) as u32,
                kind: texture::GrainType::from_i32(p3 as i32),
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Texturizer" => Filter::Texturizer {
                texture: texture::Texture::from_i32(p1 as i32),
                scaling: p2.max(0.0) as u32,
                relief: p3.max(0.0) as u32,
                light: texture::Light::from_i32(p4 as i32),
                invert: p5 != 0.0,
            },
            "Stained Glass" => Filter::StainedGlass {
                cell_size: p1.max(0.0) as u32,
                border: p2.max(0.0) as u32,
                light: p3.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
            },
            "Craquelure" => Filter::Craquelure {
                spacing: p1.max(0.0) as u32,
                depth: p2.max(0.0) as u32,
                brightness: p3.max(0.0) as u32,
            },
            "Water Paper" => Filter::WaterPaper {
                fiber: p1.max(0.0) as u32,
                brightness: p2.max(0.0) as u32,
                contrast: p3.max(0.0) as u32,
            },
            "Stamp" => Filter::Stamp {
                balance: p1.max(0.0) as u32,
                smoothness: p2.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Reticulation" => Filter::Reticulation {
                density: p1.max(0.0) as u32,
                foreground_level: p2.max(0.0) as u32,
                background_level: p3.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Plaster" => Filter::Plaster {
                balance: p1.max(0.0) as u32,
                smoothness: p2.max(0.0) as u32,
                light: texture::Light::from_i32(p3 as i32),
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Chrome" => Filter::Chrome {
                detail: p1.max(0.0) as u32,
                smoothness: p2.max(0.0) as u32,
            },
            "Charcoal" => Filter::Charcoal {
                thickness: p1.max(0.0) as u32,
                detail: p2.max(0.0) as u32,
                balance: p3.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Chalk & Charcoal" => Filter::ChalkAndCharcoal {
                charcoal_area: p1.max(0.0) as u32,
                chalk_area: p2.max(0.0) as u32,
                pressure: p3.max(0.0) as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Bas Relief" => Filter::BasRelief {
                detail: p1.max(0.0) as u32,
                smoothness: p2.max(0.0) as u32,
                light: texture::Light::from_i32(p3 as i32),
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Sumi-e" => Filter::SumiE {
                width: p1.max(0.0) as u32,
                pressure: p2.max(0.0) as u32,
                contrast: p3.max(0.0) as u32,
            },
            "Ink Outlines" => Filter::InkOutlines {
                length: p1.max(0.0) as u32,
                dark: p2.max(0.0) as u32,
                light: p3.max(0.0) as u32,
            },
            "Dark Strokes" => Filter::DarkStrokes {
                balance: p1.max(0.0) as u32,
                black: p2.max(0.0) as u32,
                white: p3.max(0.0) as u32,
            },
            "Crosshatch" => Filter::Crosshatch {
                length: p1.max(0.0) as u32,
                sharpness: p2.max(0.0) as u32,
                strength: p3.max(0.0) as u32,
            },
            "Angled Strokes" => Filter::AngledStrokes {
                balance: p1.max(0.0) as u32,
                length: p2.max(0.0) as u32,
                sharpness: p3.max(0.0) as u32,
            },
            "Accented Edges" => Filter::AccentedEdges {
                width: p1.max(0.0) as u32,
                brightness: p2.max(0.0) as u32,
                smoothness: p3.max(0.0) as u32,
            },
            "Watercolor" => Filter::Watercolor {
                detail: p1.max(0.0) as u32,
                shadow: p2.max(0.0) as u32,
                texture: p3.max(0.0) as u32,
            },
            "Sponge" => Filter::Sponge {
                size: p1.max(0.0) as u32,
                definition: p2.max(0.0) as u32,
                smoothness: p3.max(0.0) as u32,
            },
            "Smudge Stick" => Filter::SmudgeStick {
                length: p1.max(0.0) as u32,
                highlight: p2.max(0.0) as u32,
                intensity: p3.max(0.0) as u32,
            },
            "Rough Pastels" => Filter::RoughPastels {
                length: p1.max(0.0) as u32,
                detail: p2.max(0.0) as u32,
                texture: texture::Texture::from_i32(p3 as i32),
                scaling: p4.max(0.0) as u32,
                relief: p5.max(0.0) as u32,
                light: texture::Light::from_i32(at(5) as i32),
                invert: at(6) != 0.0,
            },
            "Poster Edges" => Filter::PosterEdges {
                thickness: p1.max(0.0) as u32,
                intensity: p2.max(0.0) as u32,
                posterization: p3.max(0.0) as u32,
            },
            "Plastic Wrap" => Filter::PlasticWrap {
                highlight: p1.max(0.0) as u32,
                detail: p2.max(0.0) as u32,
                smoothness: p3.max(0.0) as u32,
            },
            "Paint Daubs" => Filter::PaintDaubs {
                size: p1.max(0.0) as u32,
                sharpness: p2.max(0.0) as u32,
                brush: artistic::DaubBrush::from_i32(p3 as i32),
            },
            // The swatch fills three slots after the two sliders, as every
            // colour button does. The two the picture is rendered between are
            // the document's, so they are left at black and white here and the
            // bridge puts the real ones in.
            "Neon Glow" => Filter::NeonGlow {
                size: p1 as i32,
                brightness: p2.max(0.0) as u32,
                glow: crate::buffer::Rgba8::new(
                    p3.clamp(0.0, 255.0) as u8,
                    p4.clamp(0.0, 255.0) as u8,
                    p5.clamp(0.0, 255.0) as u8,
                    255,
                ),
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Glowing Edges" => Filter::GlowingEdges {
                width: p1.max(0.0) as u32,
                brightness: p2.max(0.0) as u32,
                smoothness: p3.max(0.0) as u32,
            },
            "Wind" => Filter::Wind {
                method: stylize::WindMethod::from_i32(p1 as i32),
                from_right: p2 == 0.0,
            },
            "Trace Contour" => Filter::TraceContour {
                level: p1.clamp(0.0, 255.0) as u8,
                edge: stylize::ContourEdge::from_i32(p2 as i32),
            },
            // In the order CS6's dialog reads down: the two numbers, then the
            // fill box. The colours come from the document, not the dialog.
            "Tiles" => {
                let fallback = stylize::TileOptions::default();
                let at = |i: usize, or: f32| p.get(i).copied().unwrap_or(or);
                Filter::Tiles {
                    options: stylize::TileOptions {
                        count: at(0, fallback.count as f32).clamp(1.0, 99.0) as u32,
                        offset: at(1, fallback.offset as f32).clamp(1.0, 99.0) as u32,
                        fill: stylize::TileFill::from_i32(at(2, 0.0) as i32),
                        ..fallback
                    },
                }
            }
            "Emboss" => Filter::Emboss {
                angle: p1,
                height: if p.len() > 1 { p2.clamp(1.0, 100.0) } else { 3.0 },
                amount: if p.len() > 2 { p3.clamp(1.0, 500.0) } else { 100.0 },
            },
            // In the order CS6's dialog reads down the box: the shape, then
            // the grid's square, then how far the towers stand and what
            // decides it, then the two tick boxes.
            "Extrude" => {
                let fallback = stylize::ExtrudeOptions::default();
                let at = |i: usize, or: f32| p.get(i).copied().unwrap_or(or);
                Filter::Extrude {
                    options: stylize::ExtrudeOptions {
                        kind: stylize::ExtrudeType::from_i32(at(0, 0.0) as i32),
                        size: at(1, fallback.size as f32).clamp(2.0, 255.0) as u32,
                        depth: at(2, fallback.depth).clamp(1.0, 255.0),
                        level_based: at(3, 0.0) != 0.0,
                        solid_front: at(4, 0.0) != 0.0,
                        mask_incomplete: at(5, 0.0) != 0.0,
                    },
                }
            }
            "Clouds" => Filter::Clouds { difference: false },
            "Difference Clouds" => Filter::Clouds { difference: true },
            // The colours come from the document, not the dialog; the bridge
            // fills them in, like Pointillize's background above.
            "Fibers" => Filter::Fibers {
                variance: p1.clamp(0.0, 64.0),
                strength: p2.clamp(1.0, 64.0),
                seed: p3 as u32,
                foreground: crate::buffer::Rgba8::BLACK,
                background: crate::buffer::Rgba8::WHITE,
            },
            // The centre comes from the dialog's crosshair rather than a
            // slider, which is why it trails the two controls CS6 lists.
            "Picture Frame" => {
                let colour = |i: usize| {
                    let c = |v: f32| v.clamp(0.0, 255.0) as u8;
                    crate::buffer::Rgba8::new(c(at(i)), c(at(i + 1)), c(at(i + 2)), 255)
                };
                let whole = |i: usize| at(i).max(0.0) as u32;
                Filter::PictureFrame {
                    options: frame::FrameOptions {
                        frame: whole(0),
                        vine: colour(1),
                        margin: whole(4),
                        size: whole(5),
                        arrangement: whole(6),
                        flower: whole(7),
                        flower_colour: colour(8),
                        flower_size: whole(11),
                        leaf: whole(12),
                        leaf_colour: colour(13),
                        leaf_size: whole(16),
                        lines: whole(17),
                        thickness: whole(18),
                        angle: whole(19),
                        fade: whole(20),
                        invert: at(21) != 0.0,
                    },
                }
            }
            "Lens Flare" => Filter::LensFlare {
                brightness: p1.clamp(10.0, 300.0),
                lens: render::LensType::from_i32(p2 as i32),
                center: (
                    p.get(2).copied().unwrap_or(0.5).clamp(0.0, 1.0),
                    p.get(3).copied().unwrap_or(0.5).clamp(0.0, 1.0),
                ),
            },
            _ => return None,
        })
    }

    /// How far, in pixels, a result pixel reaches for its input.
    ///
    /// This is what lets a preview of one region be computed from a crop: pad
    /// the crop by the reach, filter it, and the middle is identical to what
    /// the whole image would have produced.
    ///
    /// `None` means the operation cannot be cropped at all. Average is the
    /// mean of every pixel there is, and Radial Blur sweeps about a centre
    /// given as a fraction of the whole image — neither has an answer that a
    /// region can be asked for on its own, so a preview of them has to filter
    /// the whole layer first.
    pub fn reach(&self) -> Option<u32> {
        match *self {
            // Three sigma covers a Gaussian to well under a level of 255.
            Filter::GaussianBlur { radius } => Some((radius * 3.0).ceil().max(1.0) as u32),
            Filter::Blur => Some(3),
            Filter::BlurMore => Some(6),
            Filter::BoxBlur { radius } => Some(radius),
            Filter::SurfaceBlur { radius, .. } => Some(radius),
            // The line is centred on the pixel, so it reaches half its length
            // in the worst direction.
            Filter::MotionBlur { distance, .. } => Some((distance / 2.0).ceil().max(1.0) as u32),
            Filter::UnsharpMask { radius, .. } => Some((radius * 3.0).ceil().max(1.0) as u32),
            // As far as the blur reaches, which is past the canvas at the top
            // of the range; the crop is clamped to the canvas either way.
            Filter::HighPass { radius } => Some((radius * 3.0).ceil().max(1.0) as u32),
            // All three are built on a radius-1 Gaussian, which reaches three
            // pixels; Sharpen Edges then looks one further for its gradient.
            Filter::Sharpen | Filter::SharpenMore => Some(3),
            Filter::SharpenEdges => Some(4),
            // Whichever blur it is sharpening against, three sigma of a
            // Gaussian is the widest of the three at a given radius.
            Filter::SmartSharpen { radius, .. } => Some((radius * 3.0).ceil().max(1.0) as u32),
            // Noise reads no neighbours at all, but it is seeded from each
            // pixel's coordinates so that undo/redo reproduces it. A crop
            // taken out of the layer would be at different coordinates and
            // would show a different grain from the one the user is about to
            // get, so it goes the whole-layer way with the other two.
            Filter::Noise { .. } => None,
            Filter::Median { radius } | Filter::DustAndScratches { radius, .. } => Some(radius),
            // A distortion can fetch a pixel from anywhere in the image, so
            // there is no crop that could answer for a region on its own.
            Filter::Pinch { .. }
            | Filter::PolarCoordinates { .. }
            | Filter::Ripple { .. }
            | Filter::Shear { .. }
            | Filter::Spherize { .. }
            | Filter::Twirl { .. }
            | Filter::Wave { .. }
            | Filter::ZigZag { .. } => None,
            // The screen is laid on the canvas, so a crop would land on a
            // different part of the lattice and its dots would not line up
            // with the ones either side of it.
            Filter::ColorHalftone { .. }
            | Filter::Crystallize { .. }
            | Filter::Mosaic { .. }
            | Filter::Pointillize { .. } => None,
            // The pattern is laid on the canvas, so a crop would land on a
            // different part of it and its grain would not line up with the
            // pixels either side.
            Filter::Mezzotint { .. } => None,
            // Both read only a small neighbourhood, so a padded crop answers
            // for a region exactly.
            Filter::Facet => Some(1),
            Filter::Fragment => Some(4),
            Filter::Average | Filter::RadialBlur { .. } => None,
            // Cloud noise is seeded from pixel coordinates, so a crop would
            // generate a different pattern from the whole image.
            Filter::Clouds { .. } => None,
            // The fibres are seeded over the whole canvas as well.
            Filter::Fibers { .. } => None,
            // The flare is placed as a fraction of the whole frame and sized
            // against its diagonal, so a crop has no answer of its own.
            Filter::LensFlare { .. } => None,
            // The frame is laid round the edge of the whole canvas.
            Filter::PictureFrame { .. } => None,
            // The lamp is placed the same way.
            Filter::Lighting { .. } => None,
            // A pixel reaches one step for the neighbour it swaps with.
            // Anisotropic reaches further, because it runs that step several
            // times over and each pass reads what the last one left — get
            // this wrong and a preview is right in the middle and wrong at
            // its edges, which is exactly where nobody looks.
            Filter::Diffuse { mode } => Some(match mode {
                stylize::DiffuseMode::Anisotropic => stylize::ANISOTROPIC_REACH,
                _ => 1,
            }),
            // Half the height each way, plus the softening pass that runs
            // before it, which is half the height again.
            Filter::Emboss { height, .. } => Some(height.clamp(1.0, 100.0).ceil() as u32 + 1),
            // A tower is thrown outwards by a fraction of how far it already
            // is from the middle of the frame, so what lands on a pixel can
            // have come from anywhere in the picture. No crop answers for it.
            Filter::Extrude { .. } => None,
            // A 3×3 Sobel reads one pixel out, so a crop padded by that much
            // answers for a region exactly.
            Filter::FindEdges => Some(1),
            // A pixel's new colour depends on nothing but its old one, so a
            // crop of any region answers for itself with no padding at all.
            Filter::Solarize => Some(0),
            // The grid is laid on the layer's own corner, so a crop would
            // start it somewhere else and its tiles would not line up with
            // the ones either side of the region.
            Filter::Tiles { .. } => None,
            // One step out for the neighbour it is compared against, so a
            // crop padded by that much answers for a region exactly.
            Filter::TraceContour { .. } => Some(1),
            // A streak's length is seeded from where it starts, so a crop
            // taken out of the layer would be at different coordinates and
            // would blow differently from the picture the user is about to
            // get. Whole layer, with Add Noise and the rest of the seeded
            // family.
            Filter::Wind { .. } => None,
            // Half of the 5×5 grid, so a crop padded by that much answers
            // for a region exactly.
            Filter::Custom { .. } => Some((CUSTOM_SIZE / 2) as u32),
            // Three sigma of the smoothing blur, one more for the Sobel on
            // top of it, and then however far the edge is widened.
            Filter::GlowingEdges {
                width, smoothness, ..
            } => Some(stylize::glow_reach(width, smoothness)),
            // The hatch is laid on the canvas, so a crop would land on a
            // different part of it and its strokes would not line up with
            // the ones either side.
            Filter::ColoredPencil { .. } => None,
            // A piece of paper is however far it reaches — a background can
            // run the whole width of the picture — and its colour is the mean
            // of all of it. A crop would cut the piece in two and paint the
            // halves differently.
            Filter::Cutout { .. } => None,
            // The brush itself reaches a bounded way — three times its half
            // width, for the load, the roughness under it and the roughness's
            // own averaging — but the canvas it paints on is laid over the
            // frame, so a crop would take its grain from somewhere else. With
            // the hatch and the seeded family.
            Filter::DryBrush { .. } => None,
            // Every pixel answers for itself and reaches nowhere — but the
            // grain is seeded from where the pixel is, so a crop taken out of
            // the layer would be at different coordinates and would come back
            // on different film. With Add Noise and the rest of the seeded
            // family.
            Filter::FilmGrain { .. } => None,
            // The dabs reach a bounded way, but the plaster's own surface is
            // laid over the frame — with the canvas under Dry Brush, and for
            // the same reason.
            Filter::Fresco { .. } => None,
            // The cells are laid on the canvas, so a crop would land on a
            // different part of the lattice and its joins would not line up
            // with the ones either side — with Crystallize, whose cells these
            // are.
            Filter::PaletteKnife { .. } => None,
            Filter::PlasticWrap { smoothness, .. } => Some(artistic::plastic_wrap_reach(smoothness)),
            Filter::PosterEdges { thickness, .. } => Some(artistic::poster_edges_reach(thickness)),
            // The strokes' grain and the texture are laid by where on the
            // canvas a pixel is, so a crop would get different ones.
            Filter::RoughPastels { .. } => None,
            // The stick's grain is laid by where on the canvas a pixel is.
            // The grain along the strokes is laid by where on the canvas a
            // pixel is.
            Filter::SmudgeStick { .. } => None,
            // The blotches are laid by where on the canvas a pixel is.
            Filter::Sponge { .. } => None,
            // The granulation is laid by where on the canvas a pixel is.
            Filter::Watercolor { .. } => None,
            // The drag along each stroke is laid by where on the canvas a
            // pixel is.
            Filter::AngledStrokes { .. } | Filter::Crosshatch { .. } | Filter::DarkStrokes { .. } => None,
            // The lines follow the picture's own contours, which a crop cuts.
            Filter::InkOutlines { .. } => None,
            // The spray is thrown by where on the canvas a pixel is.
            Filter::Spatter { .. } => None,
            // The comb that throws the spray is laid by where on the canvas a
            // pixel is.
            Filter::SprayedStrokes { .. } => None,
            // The ink creeps by where on the canvas a pixel is.
            Filter::SumiE { .. } => None,
            // The strokes each stick is dragged along are laid by where on
            // the canvas a pixel is.
            Filter::ChalkAndCharcoal { .. } => None,
            // The grain the stick is dragged across is laid by where on the
            // canvas a pixel is.
            Filter::Charcoal { .. } => None,
            // The paper's grain is laid by where on the canvas a pixel is.
            Filter::ConteCrayon { .. } => None,
            // The strokes are laid by where on the canvas a pixel is, so a
            // crop would land on a different part of the noise and its marks
            // would not line up with the ones either side.
            Filter::GraphicPen { .. } => None,
            // The screen is laid by where on the canvas a pixel is — and the
            // rings about the middle of the frame — so a crop would land on a
            // different part of it and its cells would not line up with the
            // ones either side.
            Filter::HalftonePattern { .. } => None,
            // The paper's grain is laid by where on the canvas a pixel is.
            Filter::NotePaper { .. } => None,
            // As far as the neighbourhood each pixel is compared with.
            Filter::Photocopy { detail, .. } => Some(sketch::photocopy_reach(detail)),
            // The ramp runs across the whole frame, so a crop would shade its
            // own little ramp from black to white.
            Filter::Plaster { .. } => None,
            // The grain is laid by where on the canvas a pixel is.
            Filter::Reticulation { .. } => None,
            // As far as the melt blurs.
            Filter::Stamp { smoothness, .. } => Some(sketch::stamp_reach(smoothness)),
            // The grain is laid by where on the canvas a pixel is.
            Filter::TornEdges { .. } => None,
            // The fibres are laid by where on the canvas a pixel is.
            Filter::WaterPaper { .. } => None,
            // The cracks are laid by where on the canvas a pixel is.
            Filter::Craquelure { .. } => None,
            // The grain is laid by where on the canvas a pixel is.
            Filter::Grain { .. } => None,
            // The tiles are laid by where on the canvas a pixel is.
            Filter::MosaicTiles { .. } => None,
            // The squares are laid by where on the canvas a pixel is.
            Filter::Patchwork { .. } => None,
            // The panes are laid by where on the canvas a pixel is, and the
            // glow is centred on the whole image.
            Filter::StainedGlass { .. } => None,
            // The surface is laid by where on the canvas a pixel is.
            Filter::Texturizer { .. } => None,
            // The melt reaches as far as Smoothness blurs, and the waveform
            // magnifies whatever that changed.
            Filter::Chrome { smoothness, .. } => {
                Some(((1.6 + smoothness.clamp(0, 10) as f32 * 1.1) * 3.0).ceil() as u32 + 2)
            }
            // Two taps either side, plus whatever Smoothness blurred over.
            Filter::BasRelief { smoothness, .. } => Some(
                ((smoothness.clamp(1, 15) as f32 * 0.38) * 3.0).ceil() as u32 + 2,
            ),
            Filter::AccentedEdges { width, smoothness, .. } => {
                Some(brush_strokes::accented_edges_reach(width, smoothness))
            }
            // The texture is laid by where on the canvas a pixel is.
            Filter::Underpainting { .. } => None,
            // The Rough brushes' tooth is laid by where on the canvas a pixel
            // is, so a crop would get different texture — with Fresco.
            Filter::PaintDaubs { brush, .. } if brush.is_rough() => None,
            // The daub, then what is blurred after it: Wide Blurry's softening
            // and the sharpening's own blur, three sigma each.
            Filter::PaintDaubs { size, brush, .. } => {
                let (across, down) = artistic::daub_reach(size, brush);
                Some(across + (down as f32 * 2.0 * artistic::WIDE_BLUR * 3.0).ceil() as u32 + 6)
            }
            // Three sigma of the blur that spreads the picture's own light.
            Filter::NeonGlow { size, .. } => Some((size.unsigned_abs() * 3).max(1)),
        }
    }

    /// Apply the filter to `pixmap` in place.
    pub fn apply(&self, pixmap: &mut Pixmap) {
        match *self {
            Filter::GaussianBlur { radius } => {
                convolve::gaussian_blur_accelerated(pixmap, radius)
            }
            Filter::BoxBlur { radius } => convolve::box_blur(pixmap, radius),
            Filter::Average => convolve::average(pixmap),
            // CS6's two fixed blurs are a gentle Gaussian and a stronger one;
            // the radii are chosen to match how far each visibly softens.
            Filter::Blur => convolve::gaussian_blur_accelerated(pixmap, 0.7),
            Filter::BlurMore => convolve::gaussian_blur_accelerated(pixmap, 2.0),
            Filter::MotionBlur { angle, distance } => {
                convolve::motion_blur(pixmap, angle, distance)
            }
            Filter::RadialBlur {
                amount,
                spin,
                center,
                quality,
            } => convolve::radial_blur(pixmap, amount, spin, center, quality),
            Filter::SurfaceBlur { radius, threshold } => {
                convolve::surface_blur(pixmap, radius, threshold)
            }
            Filter::Sharpen => sharpen(pixmap),
            Filter::SharpenMore => convolve::sharpen_more(pixmap),
            Filter::SharpenEdges => convolve::sharpen_edges(pixmap),
            Filter::SmartSharpen {
                amount,
                radius,
                reduce_noise,
                remove,
                angle,
            } => convolve::smart_sharpen(pixmap, amount, radius, reduce_noise, remove, angle),
            Filter::UnsharpMask {
                amount,
                radius,
                threshold,
            } => unsharp_mask(pixmap, amount, radius, threshold),
            Filter::HighPass { radius } => convolve::high_pass(pixmap, radius),
            Filter::Noise {
                amount,
                monochromatic,
                gaussian,
            } => add_noise(pixmap, amount, monochromatic, gaussian),
            Filter::Median { radius } => convolve::median_filter(pixmap, radius),
            Filter::DustAndScratches { radius, threshold } => {
                convolve::dust_and_scratches(pixmap, radius, threshold)
            }
            Filter::Pinch { amount } => distort::pinch(pixmap, amount),
            Filter::PolarCoordinates { to_polar } => {
                distort::polar_coordinates(pixmap, to_polar)
            }
            Filter::Ripple { amount, size } => distort::ripple(pixmap, amount, size),
            Filter::Shear { offsets, wrap } => distort::shear(pixmap, &offsets, wrap),
            Filter::Spherize { amount, mode } => distort::spherize(pixmap, amount, mode),
            Filter::Twirl { angle } => distort::twirl(pixmap, angle),
            Filter::Wave {
                generators,
                wavelength,
                amplitude,
                scale,
                kind,
                wrap,
                seed,
            } => distort::wave(pixmap, generators, wavelength, amplitude, scale, kind, wrap, seed),
            Filter::ZigZag {
                amount,
                ridges,
                style,
            } => distort::zigzag(pixmap, amount, ridges, style),
            Filter::ColorHalftone { max_radius, angles } => {
                pixelate::color_halftone(pixmap, max_radius, angles)
            }
            Filter::Crystallize { cell_size } => pixelate::crystallize(pixmap, cell_size),
            Filter::Mosaic { cell_size } => pixelate::mosaic(pixmap, cell_size),
            Filter::Pointillize {
                cell_size,
                background,
            } => pixelate::pointillize(pixmap, cell_size, background),
            Filter::Facet => pixelate::facet(pixmap),
            Filter::Fragment => pixelate::fragment(pixmap),
            Filter::Mezzotint { kind } => pixelate::mezzotint(pixmap, kind),
            Filter::Clouds { difference } => render::clouds(pixmap, difference),
            Filter::Fibers {
                variance,
                strength,
                seed,
                foreground,
                background,
            } => render::fibers(pixmap, variance, strength, seed, foreground, background),
            Filter::LensFlare {
                center,
                brightness,
                lens,
            } => render::lens_flare(pixmap, center, brightness, lens),
            Filter::PictureFrame { options } => frame::picture_frame(pixmap, &options),
            Filter::Lighting { light } => render::lighting_effects(pixmap, light),
            Filter::Diffuse { mode } => stylize::diffuse(pixmap, mode),
            Filter::Emboss {
                angle,
                height,
                amount,
            } => stylize::emboss(pixmap, angle, height, amount),
            Filter::Extrude { options } => stylize::extrude(pixmap, options),
            Filter::FindEdges => stylize::find_edges(pixmap),
            Filter::Solarize => stylize::solarize(pixmap),
            Filter::Tiles { options } => stylize::tiles(pixmap, options),
            Filter::TraceContour { level, edge } => stylize::trace_contour(pixmap, level, edge),
            Filter::Wind { method, from_right } => stylize::wind(pixmap, method, from_right),
            Filter::GlowingEdges {
                width,
                brightness,
                smoothness,
            } => stylize::glowing_edges(pixmap, width, brightness, smoothness),
            Filter::ColoredPencil {
                width,
                pressure,
                paper,
            } => artistic::colored_pencil(pixmap, width, pressure, paper),
            Filter::Cutout {
                levels,
                simplicity,
                fidelity,
            } => artistic::cutout(pixmap, levels, simplicity, fidelity),
            Filter::DryBrush {
                size,
                detail,
                texture,
            } => artistic::dry_brush(pixmap, size, detail, texture),
            Filter::FilmGrain {
                grain,
                highlight_area,
                intensity,
            } => artistic::film_grain(pixmap, grain, highlight_area, intensity),
            Filter::Fresco {
                size,
                detail,
                texture,
            } => artistic::fresco(pixmap, size, detail, texture),
            Filter::PaletteKnife {
                size,
                detail,
                softness,
            } => artistic::palette_knife(pixmap, size, detail, softness),
            Filter::PlasticWrap {
                highlight,
                detail,
                smoothness,
            } => artistic::plastic_wrap(pixmap, highlight, detail, smoothness),
            Filter::PosterEdges {
                thickness,
                intensity,
                posterization,
            } => artistic::poster_edges(pixmap, thickness, intensity, posterization),
            Filter::RoughPastels {
                length,
                detail,
                texture,
                scaling,
                relief,
                light,
                invert,
            } => artistic::rough_pastels(pixmap, length, detail, texture, scaling, relief, light, invert),
            Filter::SmudgeStick {
                length,
                highlight,
                intensity,
            } => artistic::smudge_stick(pixmap, length, highlight, intensity),
            Filter::Sponge {
                size,
                definition,
                smoothness,
            } => artistic::sponge(pixmap, size, definition, smoothness),
            Filter::Watercolor {
                detail,
                shadow,
                texture,
            } => artistic::watercolor(pixmap, detail, shadow, texture),
            Filter::AccentedEdges {
                width,
                brightness,
                smoothness,
            } => brush_strokes::accented_edges(pixmap, width, brightness, smoothness),
            Filter::AngledStrokes {
                balance,
                length,
                sharpness,
            } => brush_strokes::angled_strokes(pixmap, balance, length, sharpness),
            Filter::Crosshatch {
                length,
                sharpness,
                strength,
            } => brush_strokes::crosshatch(pixmap, length, sharpness, strength),
            Filter::DarkStrokes {
                balance,
                black,
                white,
            } => brush_strokes::dark_strokes(pixmap, balance, black, white),
            Filter::InkOutlines {
                length,
                dark,
                light,
            } => brush_strokes::ink_outlines(pixmap, length, dark, light),
            Filter::Spatter { radius, smoothness } => {
                brush_strokes::spatter(pixmap, radius, smoothness)
            }
            Filter::SprayedStrokes {
                length,
                radius,
                direction,
            } => brush_strokes::sprayed_strokes(pixmap, length, radius, direction),
            Filter::SumiE {
                width,
                pressure,
                contrast,
            } => brush_strokes::sumi_e(pixmap, width, pressure, contrast),
            Filter::BasRelief {
                detail,
                smoothness,
                light,
                foreground,
                background,
            } => sketch::bas_relief(pixmap, detail, smoothness, light, foreground, background),
            Filter::Chrome { detail, smoothness } => sketch::chrome(pixmap, detail, smoothness),
            Filter::ConteCrayon {
                foreground_level,
                background_level,
                texture,
                scaling,
                relief,
                light,
                invert,
                foreground,
                background,
            } => sketch::conte_crayon(
                pixmap,
                foreground_level,
                background_level,
                texture,
                scaling,
                relief,
                light,
                invert,
                foreground,
                background,
            ),
            Filter::GraphicPen {
                stroke_length,
                balance,
                direction,
                foreground,
                background,
            } => sketch::graphic_pen(
                pixmap,
                stroke_length,
                balance,
                direction,
                foreground,
                background,
            ),
            Filter::HalftonePattern {
                size,
                contrast,
                pattern,
                foreground,
                background,
            } => sketch::halftone_pattern(
                pixmap,
                size,
                contrast,
                pattern,
                foreground,
                background,
            ),
            Filter::NotePaper {
                balance,
                graininess,
                relief,
                foreground,
                background,
            } => sketch::note_paper(pixmap, balance, graininess, relief, foreground, background),
            Filter::Photocopy {
                detail,
                darkness,
                foreground,
                background,
            } => sketch::photocopy(pixmap, detail, darkness, foreground, background),
            Filter::Plaster {
                balance,
                smoothness,
                light,
                foreground,
                background,
            } => sketch::plaster(pixmap, balance, smoothness, light, foreground, background),
            Filter::Stamp {
                balance,
                smoothness,
                foreground,
                background,
            } => sketch::stamp(pixmap, balance, smoothness, foreground, background),
            Filter::TornEdges {
                balance,
                smoothness,
                contrast,
                foreground,
                background,
            } => sketch::torn_edges(pixmap, balance, smoothness, contrast, foreground, background),
            Filter::WaterPaper {
                fiber,
                brightness,
                contrast,
            } => sketch::water_paper(pixmap, fiber, brightness, contrast),
            Filter::Craquelure {
                spacing,
                depth,
                brightness,
            } => texture::craquelure(pixmap, spacing, depth, brightness),
            Filter::Grain {
                intensity,
                contrast,
                kind,
                foreground,
                background,
            } => texture::grain(pixmap, intensity, contrast, kind, foreground, background),
            Filter::Patchwork { square, relief } => texture::patchwork(pixmap, square, relief),
            Filter::StainedGlass {
                cell_size,
                border,
                light,
                foreground,
            } => texture::stained_glass(pixmap, cell_size, border, light, foreground),
            Filter::Texturizer {
                texture: surface,
                scaling,
                relief,
                light,
                invert,
            } => texture::texturizer(pixmap, surface, scaling, relief, light, invert),
            Filter::MosaicTiles { size, grout, lighten } => {
                texture::mosaic_tiles(pixmap, size, grout, lighten)
            }
            Filter::Reticulation {
                density,
                foreground_level,
                background_level,
                foreground,
                background,
            } => sketch::reticulation(
                pixmap,
                density,
                foreground_level,
                background_level,
                foreground,
                background,
            ),
            Filter::Charcoal {
                thickness,
                detail,
                balance,
                foreground,
                background,
            } => sketch::charcoal(pixmap, thickness, detail, balance, foreground, background),
            Filter::ChalkAndCharcoal {
                charcoal_area,
                chalk_area,
                pressure,
                foreground,
                background,
            } => sketch::chalk_and_charcoal(
                pixmap,
                charcoal_area,
                chalk_area,
                pressure,
                foreground,
                background,
            ),
            Filter::Underpainting {
                size,
                coverage,
                texture,
                scaling,
                relief,
                light,
                invert,
            } => artistic::underpainting(pixmap, size, coverage, texture, scaling, relief, light, invert),
            Filter::PaintDaubs {
                size,
                sharpness,
                brush,
            } => artistic::paint_daubs(pixmap, size, sharpness, brush),
            Filter::NeonGlow {
                size,
                brightness,
                glow,
                foreground,
                background,
            } => artistic::neon_glow(pixmap, size, brightness, glow, foreground, background),
            Filter::Custom {
                weights,
                scale,
                offset,
            } => convolve::custom(pixmap, &weights, scale, offset),
        }
    }
}

/// Deterministic value noise.
///
/// Seeded from pixel coordinates rather than a RNG so that re-running a filter
/// during an undo/redo replay reproduces the exact same image.
fn add_noise(pixmap: &mut Pixmap, amount: f32, monochromatic: bool, gaussian: bool) {
    // CS6's slider is a percentage of the tonal range, running to 400 where
    // the picture is entirely gone. Half of it either way, so 100% spans the
    // whole range from a mid grey.
    let magnitude = (amount.clamp(0.0, 400.0) / 100.0) * 127.5;
    if magnitude <= 0.0 {
        return;
    }
    let width = pixmap.width();

    for y in 0..pixmap.height() {
        for x in 0..width {
            let base = hash2(x, y);
            let jitter = |salt: u32| -> i32 {
                let h = hash2(base.wrapping_add(salt), salt);
                // A hash in [-1, 1].
                let u = (h % 2001) as f32 / 1000.0 - 1.0;
                if !gaussian {
                    return (u * magnitude) as i32;
                }
                // Gaussian noise clumps: most of it lands near zero and the
                // occasional sample goes a long way out, which is the grainier
                // look CS6's second option gives. Built by adding three
                // independent hashes rather than taking a logarithm — the sum
                // is near enough a bell for this, and it costs three
                // multiplications.
                let v = (hash2(h, salt.wrapping_add(101)) % 2001) as f32 / 1000.0 - 1.0;
                let w = (hash2(h, salt.wrapping_add(211)) % 2001) as f32 / 1000.0 - 1.0;
                ((u + v + w) / 3.0 * 1.732 * magnitude) as i32
            };

            let px = pixmap.get(x as i32, y as i32);
            if px.a == 0 {
                continue;
            }
            let (dr, dg, db) = if monochromatic {
                let d = jitter(0);
                (d, d, d)
            } else {
                (jitter(0), jitter(1), jitter(2))
            };
            pixmap.set(
                x as i32,
                y as i32,
                crate::buffer::Rgba8::new(
                    (px.r as i32 + dr).clamp(0, 255) as u8,
                    (px.g as i32 + dg).clamp(0, 255) as u8,
                    (px.b as i32 + db).clamp(0, 255) as u8,
                    px.a,
                ),
            );
        }
    }
}

/// Cheap integer hash used to derive reproducible per-pixel noise.
#[inline]
fn hash2(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E3779B1) ^ y.wrapping_mul(0x85EBCA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545F491);
    h ^= h >> 13;
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    #[test]
    fn noise_is_deterministic() {
        let make = || {
            let mut pm = Pixmap::filled(8, 8, Rgba8::new(128, 128, 128, 255));
            add_noise(&mut pm, 50.0, false, false);
            pm
        };
        assert_eq!(make().as_bytes(), make().as_bytes());
    }

    #[test]
    fn noise_leaves_transparent_pixels_alone() {
        let mut pm = Pixmap::new(4, 4);
        add_noise(&mut pm, 100.0, false, false);
        assert!(pm.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn zero_amount_noise_is_a_no_op() {
        let mut pm = Pixmap::filled(4, 4, Rgba8::new(100, 100, 100, 255));
        let before = pm.as_bytes().to_vec();
        add_noise(&mut pm, 0.0, false, false);
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn monochromatic_noise_keeps_channels_equal() {
        let mut pm = Pixmap::filled(8, 8, Rgba8::new(128, 128, 128, 255));
        add_noise(&mut pm, 30.0, true, false);
        for y in 0..8 {
            for x in 0..8 {
                let p = pm.get(x, y);
                assert_eq!(p.r, p.g);
                assert_eq!(p.g, p.b);
            }
        }
    }

    #[test]
    fn noise_preserves_alpha() {
        let mut pm = Pixmap::filled(4, 4, Rgba8::new(10, 10, 10, 200));
        add_noise(&mut pm, 100.0, false, false);
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(pm.get(x, y).a, 200);
            }
        }
    }

    #[test]
    fn filters_have_names() {
        let all = [
            Filter::GaussianBlur { radius: 1.0 },
            Filter::BoxBlur { radius: 1 },
            Filter::Sharpen,
            Filter::UnsharpMask {
                amount: 1.0,
                radius: 1.0,
                threshold: 0,
            },
            Filter::Noise {
                amount: 10.0,
                monochromatic: true,
                gaussian: false,
            },
            Filter::Median { radius: 2 },
            Filter::DustAndScratches {
                radius: 2,
                threshold: 15,
            },
        ];
        for f in all {
            assert!(!f.name().is_empty());
        }
    }

    /// The dialog hands over a flat list of numbers, and the order it puts
    /// them in is a contract between the two sides — Lens Flare's crosshair
    /// fills the last two slots, after the two controls CS6 lists.
    #[test]
    fn a_lens_flare_reads_its_dialog_in_order() {
        let flare = Filter::from_menu_name("Lens Flare", &[143.0, 2.0, 0.25, 0.75]);
        assert_eq!(
            flare,
            Some(Filter::LensFlare {
                brightness: 143.0,
                lens: LensType::Prime105,
                center: (0.25, 0.75),
            })
        );
        // What the dialog did not offer reads as the middle of the frame, not
        // as a flare jammed into the top-left corner.
        assert_eq!(
            Filter::from_menu_name("Lens Flare", &[100.0, 0.0]),
            Some(Filter::LensFlare {
                brightness: 100.0,
                lens: LensType::Zoom50To300,
                center: (0.5, 0.5),
            })
        );
    }

    /// Diffuse's four modes are the whole of its dialog, and an unknown
    /// number has to read as the one CS6 opens on rather than as nothing.
    #[test]
    fn diffuse_reads_the_mode_its_dialog_chose() {
        for (chosen, expected) in [
            (0.0, DiffuseMode::Normal),
            (1.0, DiffuseMode::DarkenOnly),
            (2.0, DiffuseMode::LightenOnly),
            (3.0, DiffuseMode::Anisotropic),
            (9.0, DiffuseMode::Normal),
        ] {
            assert_eq!(
                Filter::from_menu_name("Diffuse", &[chosen]),
                Some(Filter::Diffuse { mode: expected })
            );
        }
        assert_eq!(
            Filter::from_menu_name("Diffuse", &[]),
            Some(Filter::Diffuse {
                mode: DiffuseMode::Normal
            })
        );
    }

    /// Emboss's three numbers, and what an unasked-for one falls back to.
    /// CS6 opens on 135°, three pixels and 100%, which is also what a repeat
    /// with no dialog has to use.
    #[test]
    fn emboss_reads_angle_then_height_then_amount() {
        assert_eq!(
            Filter::from_menu_name("Emboss", &[45.0, 8.0, 250.0]),
            Some(Filter::Emboss {
                angle: 45.0,
                height: 8.0,
                amount: 250.0
            })
        );
        assert_eq!(
            Filter::from_menu_name("Emboss", &[]),
            Some(Filter::Emboss {
                angle: 0.0,
                height: 3.0,
                amount: 100.0
            })
        );
        // A taller relief reads further, so its preview has to crop wider.
        let reach = |height| {
            Filter::Emboss {
                angle: 0.0,
                height,
                amount: 100.0,
            }
            .reach()
        };
        assert!(reach(20.0) > reach(3.0));
    }

    /// Extrude's six, in the order its dialog reads down the box. Size and
    /// Depth are both plain numbers in overlapping ranges, so a swap between
    /// them produces a picture that still looks extruded and is simply not
    /// the one that was asked for.
    #[test]
    fn extrude_reads_its_dialog_in_order() {
        assert_eq!(
            Filter::from_menu_name("Extrude", &[1.0, 12.0, 200.0, 1.0, 1.0, 1.0]),
            Some(Filter::Extrude {
                options: ExtrudeOptions {
                    kind: ExtrudeType::Pyramids,
                    size: 12,
                    depth: 200.0,
                    level_based: true,
                    solid_front: true,
                    mask_incomplete: true,
                }
            })
        );
        // Nothing collected is CS6's opening state: blocks, 30 and 30, thrown
        // at random, neither box ticked.
        assert_eq!(
            Filter::from_menu_name("Extrude", &[]),
            Some(Filter::Extrude {
                options: ExtrudeOptions::default()
            })
        );
        // A tower is thrown by a fraction of how far it already is from the
        // middle, so no crop can answer for a region of it on its own.
        assert_eq!(
            Filter::Extrude {
                options: ExtrudeOptions::default()
            }
            .reach(),
            None
        );
    }

    /// Find Edges takes nothing, as in CS6, and still needs a name for the
    /// History panel and a reach for a cropped preview.
    #[test]
    fn find_edges_takes_no_parameters() {
        assert_eq!(
            Filter::from_menu_name("Find Edges", &[]),
            Some(Filter::FindEdges)
        );
        assert_eq!(Filter::FindEdges.name(), "Find Edges");
        assert_eq!(Filter::FindEdges.reach(), Some(1));
    }

    /// Solarize takes nothing either, and reads no neighbours, so a region
    /// needs no padding round it.
    #[test]
    fn solarize_takes_no_parameters() {
        assert_eq!(
            Filter::from_menu_name("Solarize", &[]),
            Some(Filter::Solarize)
        );
        assert_eq!(Filter::Solarize.name(), "Solarize");
        assert_eq!(Filter::Solarize.reach(), Some(0));
    }

    /// Anisotropic runs its pass several times over, so it reads further than
    /// the one step the other three do. A preview crops to the reach, so a
    /// reach that is too small shows a seam a few pixels inside the
    /// thumbnail's edge — and nothing at all wrong in the middle.
    #[test]
    fn anisotropic_reaches_further_than_the_other_modes() {
        let reach = |mode| Filter::Diffuse { mode }.reach();
        assert_eq!(reach(DiffuseMode::Normal), Some(1));
        assert_eq!(reach(DiffuseMode::DarkenOnly), Some(1));
        assert_eq!(
            reach(DiffuseMode::Anisotropic),
            Some(crate::filters::stylize::ANISOTROPIC_REACH)
        );
        assert!(crate::filters::stylize::ANISOTROPIC_REACH > 1);
    }

    /// Nineteen numbers in one flat list, read back into the panel CS6 shows.
    /// Two colours in the middle of it fill three slots each, which is where
    /// a miscount would put the exposure into the blue channel and never be
    /// noticed by anything but this.
    #[test]
    fn lighting_effects_reads_its_panel_in_order() {
        let panel = [
            1.0, // Light Type: Point
            255.0, 200.0, 100.0, // Color
            35.0,  // Intensity
            -22.0, // Hotspot
            10.0, 20.0, 30.0, // Colorize
            15.0,  // Exposure
            -13.0, // Gloss
            41.0,  // Metallic
            44.0,  // Ambience
            2.0,   // Texture: Green
            80.0,  // Height
            60.0,  // Size, as a percentage of the frame
            120.0, // Angle
            0.25, 0.75, // Where the lamp stands
        ];
        assert_eq!(
            Filter::from_menu_name("Lighting Effects", &panel),
            Some(Filter::Lighting {
                light: Lighting {
                    kind: LightType::Point,
                    color: Rgba8::new(255, 200, 100, 255),
                    intensity: 35.0,
                    hotspot: -22.0,
                    colorize: Rgba8::new(10, 20, 30, 255),
                    exposure: 15.0,
                    gloss: -13.0,
                    metallic: 41.0,
                    ambience: 44.0,
                    texture: TextureChannel::Green,
                    height: 80.0,
                    size: 0.6,
                    angle: 120.0,
                    center: (0.25, 0.75),
                },
            })
        );
        // An empty list is CS6's opening spot light rather than a lamp with
        // no colour, no size and nothing to shine on.
        assert_eq!(
            Filter::from_menu_name("Lighting Effects", &[]),
            Some(Filter::Lighting {
                light: Lighting::default()
            })
        );
    }
}
