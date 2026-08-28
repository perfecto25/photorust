// Separable Gaussian blur — one axis per dispatch.
//
// Deliberately a byte-for-byte port of `filters::convolve::blur_pass`, because
// the CPU result is the reference and any divergence is a bug. In particular:
//
//   * samples are clamped to the edge, not wrapped or zeroed;
//   * the accumulator works in 0..255 units, not 0..1, so the rounding below
//     matches the CPU's `(acc.clamp(0.0, 255.0) + 0.5) as u8` exactly;
//   * pixels arrive already premultiplied. The premultiply and its inverse
//     stay on the CPU, where they are integer-exact and cheap.

struct Params {
    width: u32,
    height: u32,
    taps: i32,
    // 1 = horizontal, 0 = vertical.
    horizontal: u32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<storage, read> weights: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

// RGBA8 packed one pixel per u32. The host uploads raw bytes, so this assumes
// the little-endian layout of both target platforms (x86-64 and arm64): byte 0
// is red and lands in the low bits.
fn unpack(v: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(v & 0xffu),
        f32((v >> 8u) & 0xffu),
        f32((v >> 16u) & 0xffu),
        f32((v >> 24u) & 0xffu),
    );
}

fn pack(c: vec4<f32>) -> u32 {
    // Clamp first, then add the half — the same order as the CPU, so values at
    // the top of the range round identically.
    let q = clamp(c, vec4<f32>(0.0), vec4<f32>(255.0)) + vec4<f32>(0.5);
    return u32(q.x) | (u32(q.y) << 8u) | (u32(q.z) << 16u) | (u32(q.w) << 24u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = params.width;
    let h = params.height;
    if (gid.x >= w || gid.y >= h) {
        return;
    }

    let x = i32(gid.x);
    let y = i32(gid.y);
    let taps = params.taps;
    let last_x = i32(w) - 1;
    let last_y = i32(h) - 1;

    var acc = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    let count = taps * 2 + 1;
    for (var i = 0; i < count; i = i + 1) {
        let d = i - taps;
        var sx = x;
        var sy = y;
        if (params.horizontal == 1u) {
            sx = clamp(x + d, 0, last_x);
        } else {
            sy = clamp(y + d, 0, last_y);
        }
        let idx = u32(sy) * w + u32(sx);
        acc = acc + unpack(src[idx]) * weights[i];
    }

    dst[u32(y) * w + u32(x)] = pack(acc);
}
