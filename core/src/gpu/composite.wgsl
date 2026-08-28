// Layer compositing — a port of `compositor::composite_row`.
//
// The CPU is the reference. Every function below mirrors one in
// `compositor.rs` or `blend.rs`, deliberately keeping the same shape and the
// same branch order so the two can be diffed by eye.
//
// Not handled here: adjustment layers. They need the whole `Adjustment` enum
// in shader form, which is Phase 3 — the host checks for them and takes the
// CPU path instead.

const KIND_RASTER: u32 = 0u;
const KIND_SOLID: u32 = 1u;
const NO_MASK: u32 = 0xffffffffu;

struct LayerMeta {
    pixel_offset: u32,
    width: u32,
    height: u32,
    off_x: i32,
    off_y: i32,
    // Index into `masks`, or NO_MASK. Mask origin is the layer's own offset.
    mask_offset: u32,
    mask_width: u32,
    mask_height: u32,
    alpha: f32,
    blend_mode: u32,
    kind: u32,
    clipping: u32,
    solid: u32,
    visible: u32,
    _pad0: u32,
    _pad1: u32,
};

struct Params {
    width: u32,
    height: u32,
    layer_count: u32,
    _pad: u32,
};

@group(0) @binding(0) var<storage, read> pixels: array<u32>;
// One entry per mask pixel, holding just the coverage byte.
@group(0) @binding(1) var<storage, read> masks: array<u32>;
@group(0) @binding(2) var<storage, read> layers: array<LayerMeta>;
@group(0) @binding(3) var<storage, read_write> out_pixels: array<u32>;
@group(0) @binding(4) var<uniform> params: Params;

fn unpack(v: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(v & 0xffu),
        f32((v >> 8u) & 0xffu),
        f32((v >> 16u) & 0xffu),
        f32((v >> 24u) & 0xffu),
    ) / 255.0;
}

// Matches `compositor::to_u8`: clamp, then add the half, then truncate.
fn to_u8(v: f32) -> u32 {
    return u32(clamp(v, 0.0, 1.0) * 255.0 + 0.5);
}

fn pack(rgb: vec3<f32>, a: f32) -> u32 {
    return to_u8(rgb.x) | (to_u8(rgb.y) << 8u) | (to_u8(rgb.z) << 16u) | (to_u8(a) << 24u);
}

// ------------------------------------------------------------ blend modes ---

fn clamp01(v: f32) -> f32 { return clamp(v, 0.0, 1.0); }
fn screen1(b: f32, s: f32) -> f32 { return b + s - b * s; }

fn hard_light1(b: f32, s: f32) -> f32 {
    if (s <= 0.5) { return b * (2.0 * s); }
    return screen1(b, 2.0 * s - 1.0);
}

fn color_burn1(b: f32, s: f32) -> f32 {
    if (b >= 1.0) { return 1.0; }
    if (s <= 0.0) { return 0.0; }
    return 1.0 - clamp01((1.0 - b) / s);
}

fn color_dodge1(b: f32, s: f32) -> f32 {
    if (b <= 0.0) { return 0.0; }
    if (s >= 1.0) { return 1.0; }
    return clamp01(b / (1.0 - s));
}

fn soft_light1(b: f32, s: f32) -> f32 {
    var d: f32;
    if (b <= 0.25) {
        d = ((16.0 * b - 12.0) * b + 4.0) * b;
    } else {
        d = sqrt(b);
    }
    if (s <= 0.5) { return b - (1.0 - 2.0 * s) * b * (1.0 - b); }
    return b + (2.0 * s - 1.0) * (d - b);
}

fn linear_light1(b: f32, s: f32) -> f32 { return clamp01(b + 2.0 * s - 1.0); }

fn vivid_light1(b: f32, s: f32) -> f32 {
    if (s <= 0.5) { return color_burn1(b, 2.0 * s); }
    return color_dodge1(b, 2.0 * s - 1.0);
}

fn pin_light1(b: f32, s: f32) -> f32 {
    if (s <= 0.5) { return min(b, 2.0 * s); }
    return max(b, 2.0 * s - 1.0);
}

fn blend_channel(mode: u32, b: f32, s: f32) -> f32 {
    switch (mode) {
        case 2u: { return min(b, s); }              // Darken
        case 3u: { return b * s; }                  // Multiply
        case 4u: { return color_burn1(b, s); }
        case 5u: { return clamp01(b + s - 1.0); }   // LinearBurn
        case 7u: { return max(b, s); }              // Lighten
        case 8u: { return screen1(b, s); }
        case 9u: { return color_dodge1(b, s); }
        case 10u: { return clamp01(b + s); }        // LinearDodge
        case 12u: { return hard_light1(s, b); }     // Overlay — operands swapped
        case 13u: { return soft_light1(b, s); }
        case 14u: { return hard_light1(b, s); }
        case 15u: { return vivid_light1(b, s); }
        case 16u: { return linear_light1(b, s); }
        case 17u: { return pin_light1(b, s); }
        case 18u: {                                 // HardMix
            if (linear_light1(b, s) < 0.5) { return 0.0; }
            return 1.0;
        }
        case 19u: { return abs(b - s); }            // Difference
        case 20u: { return b + s - 2.0 * b * s; }   // Exclusion
        case 21u: { return clamp01(b - s); }        // Subtract
        case 22u: {                                 // Divide
            if (s <= 0.0) { return 1.0; }
            return clamp01(b / s);
        }
        // Normal, Dissolve, and every non-separable mode return the source;
        // the latter are handled in `blend_rgb`.
        default: { return s; }
    }
}

fn lum3(c: vec3<f32>) -> f32 { return 0.3 * c.x + 0.59 * c.y + 0.11 * c.z; }

fn clip_color(c_in: vec3<f32>) -> vec3<f32> {
    var c = c_in;
    let l = lum3(c);
    let n = min(c.x, min(c.y, c.z));
    let x = max(c.x, max(c.y, c.z));
    // f32::EPSILON, matching the CPU's guard against dividing by ~0.
    let eps = 1.1920929e-7;

    if (n < 0.0) {
        let d = l - n;
        if (d > eps) {
            c = vec3<f32>(l) + (c - vec3<f32>(l)) * l / d;
        } else {
            c = vec3<f32>(l);
        }
    }
    if (x > 1.0) {
        let d = x - l;
        if (d > eps) {
            c = vec3<f32>(l) + (c - vec3<f32>(l)) * (1.0 - l) / d;
        } else {
            c = vec3<f32>(l);
        }
    }
    return c;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    let d = l - lum3(c);
    return clip_color(c + vec3<f32>(d));
}

fn sat3(c: vec3<f32>) -> f32 {
    return max(c.x, max(c.y, c.z)) - min(c.x, min(c.y, c.z));
}

// Rescale to saturation `s`, keeping channel ordering — and therefore hue.
fn set_sat(c_in: vec3<f32>, s: f32) -> vec3<f32> {
    var c = array<f32, 3>(c_in.x, c_in.y, c_in.z);
    var imin = 0;
    var imid = 1;
    var imax = 2;
    if (c[imin] > c[imid]) { let t = imin; imin = imid; imid = t; }
    if (c[imin] > c[imax]) { let t = imin; imin = imax; imax = t; }
    if (c[imid] > c[imax]) { let t = imid; imid = imax; imax = t; }

    var out = array<f32, 3>(0.0, 0.0, 0.0);
    let range = c[imax] - c[imin];
    if (range > 1.1920929e-7) {
        out[imid] = (c[imid] - c[imin]) * s / range;
        out[imax] = s;
    }
    // out[imin] stays 0 — a fully desaturated channel.
    return vec3<f32>(out[0], out[1], out[2]);
}

fn blend_rgb(mode: u32, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    switch (mode) {
        case 23u: { return set_lum(set_sat(s, sat3(b)), lum3(b)); }  // Hue
        case 24u: { return set_lum(set_sat(b, sat3(s)), lum3(b)); }  // Saturation
        case 25u: { return set_lum(s, lum3(b)); }                    // Color
        case 26u: { return set_lum(b, lum3(s)); }                    // Luminosity
        case 6u: {                                                   // DarkerColor
            if (lum3(s) < lum3(b)) { return s; }
            return b;
        }
        case 11u: {                                                  // LighterColor
            if (lum3(s) > lum3(b)) { return s; }
            return b;
        }
        default: {
            return vec3<f32>(
                blend_channel(mode, b.x, s.x),
                blend_channel(mode, b.y, s.y),
                blend_channel(mode, b.z, s.z),
            );
        }
    }
}

// ------------------------------------------------------------- compositing ---

fn sample_layer(li: u32, x: i32, y: i32) -> vec4<f32> {
    let L = layers[li];
    let lx = x - L.off_x;
    let ly = y - L.off_y;
    // Outside the layer reads as transparent, matching `Pixmap::get`.
    if (lx < 0 || ly < 0 || lx >= i32(L.width) || ly >= i32(L.height)) {
        return vec4<f32>(0.0);
    }
    return unpack(pixels[L.pixel_offset + u32(ly) * L.width + u32(lx)]);
}

fn mask_at(li: u32, x: i32, y: i32) -> f32 {
    let L = layers[li];
    if (L.mask_offset == NO_MASK) { return 1.0; }
    let lx = x - L.off_x;
    let ly = y - L.off_y;
    // Outside the mask reads as fully masked *out*, as Photoshop does.
    if (lx < 0 || ly < 0 || lx >= i32(L.mask_width) || ly >= i32(L.mask_height)) {
        return 0.0;
    }
    return f32(masks[L.mask_offset + u32(ly) * L.mask_width + u32(lx)]) / 255.0;
}

// Alpha of the clipping base: scan down past other clipping layers to the
// first ordinary one.
fn clip_coverage(index: u32, x: i32, y: i32) -> f32 {
    var i = i32(index) - 1;
    loop {
        if (i < 0) { break; }
        let L = layers[u32(i)];
        if (L.clipping == 1u) { i = i - 1; continue; }
        if (L.visible == 0u) { return 0.0; }
        let px = sample_layer(u32(i), x, y);
        return px.a * mask_at(u32(i), x, y);
    }
    // No base beneath it — nothing to clip to, so it is fully hidden.
    return 0.0;
}

// Stable pseudo-random threshold for Dissolve. `bitcast` rather than a value
// conversion, so negative coordinates wrap exactly as Rust's `as u32` does.
fn dissolve_threshold(x: i32, y: i32) -> f32 {
    var h = bitcast<u32>(x) * 0x9E3779B1u ^ bitcast<u32>(y) * 0x85EBCA77u;
    h = h ^ (h >> 15u);
    h = h * 0x2545F491u;
    h = h ^ (h >> 13u);
    return f32(h % 10000u) / 10000.0;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let x = i32(gid.x);
    let y = i32(gid.y);

    var back_rgb = vec3<f32>(0.0);
    var back_a = 0.0;

    for (var li = 0u; li < params.layer_count; li = li + 1u) {
        let L = layers[li];
        if (L.visible == 0u) { continue; }

        var clip = 1.0;
        if (L.clipping == 1u) { clip = clip_coverage(li, x, y); }
        if (clip <= 0.0) { continue; }

        var src_rgb: vec3<f32>;
        var src_a: f32;

        if (L.kind == KIND_SOLID) {
            let c = unpack(L.solid);
            src_rgb = c.rgb;
            src_a = c.a * L.alpha * mask_at(li, x, y) * clip;
        } else {
            let px = sample_layer(li, x, y);
            if (px.a <= 0.0) { continue; }
            src_rgb = px.rgb;
            src_a = px.a * L.alpha * mask_at(li, x, y) * clip;
            if (L.blend_mode == 1u) {
                // Dissolve turns partial alpha into an all-or-nothing choice,
                // hashed from position so the pattern does not shimmer.
                if (dissolve_threshold(x, y) < src_a) { src_a = 1.0; } else { src_a = 0.0; }
            }
        }

        if (src_a <= 0.0) { continue; }

        // `blend_over` from compositor.rs, inlined.
        let ab = back_a;
        let ar = src_a + ab * (1.0 - src_a);
        if (ar <= 0.0) {
            back_rgb = vec3<f32>(0.0);
            back_a = 0.0;
            continue;
        }
        let blended = blend_rgb(L.blend_mode, back_rgb, src_rgb);
        let ratio = src_a / ar;
        let mixed = (1.0 - ab) * src_rgb + ab * blended;
        back_rgb = (1.0 - ratio) * back_rgb + ratio * mixed;
        back_a = ar;
    }

    out_pixels[gid.y * params.width + gid.x] = pack(back_rgb, back_a);
}
