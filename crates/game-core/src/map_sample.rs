//! Sample terrain for the strategic grid from an azimuthal-equidistant
//! "flat-earth" projection image (N-pole centre, Antarctica rim).
//!
//! Shared between the seed tool (uses the `image` crate to decode a PNG) and
//! the frontend WASM module (passes pixels decoded in the browser). Both
//! paths end up calling [`sample_grid_from_map`] — guarantees that what the
//! server stores in `hex_map` matches what the client renders.
//!
//! Image format: interleaved RGB bytes, row-major, length = iw * ih * 3.

use crate::terrain::Terrain;

/// Sample terrain + elevation for every (q, r) in a `width × height` grid.
///
/// Uses the same polar projection math as the frontend renderer
/// (`theta = lon01 * 2π`, `rho = lat01 * radius`), so the terrain underneath
/// a hex on the polar disc matches the pixel colour at that spot of the PNG.
///
/// Returns a flat `Vec` of `(Terrain, elevation)` indexed by `r * width + q`.
pub fn sample_grid_from_map(
    pixels: &[u8],
    iw: u32,
    ih: u32,
    width: i32,
    height: i32,
) -> Vec<(Terrain, i32)> {
    let iw_f = iw as f32;
    let ih_f = ih as f32;
    let cx = iw_f / 2.0;
    let cy = ih_f / 2.0;
    let radius = iw_f.min(ih_f) / 2.0;

    let mut out = Vec::with_capacity((width * height) as usize);
    for r in 0..height {
        let lat01 = r as f32 / height as f32;
        let rho = lat01 * radius;
        for q in 0..width {
            let lon01 = q as f32 / width as f32;
            let theta = lon01 * 2.0 * std::f32::consts::PI;
            let px = (cx + rho * theta.sin()).clamp(0.0, iw_f - 1.0) as i32;
            let py = (cy - rho * theta.cos()).clamp(0.0, ih_f - 1.0) as i32;

            let (rc, gc, bc) = sample_avg_3x3(pixels, iw, ih, px, py);
            let terrain = classify_pixel(rc, gc, bc);
            out.push((terrain, terrain_elevation(terrain)));
        }
    }
    out
}

/// 3x3 mean of the RGB values around `(cx, cy)` in a packed RGB byte buffer.
/// Smooths the PNG's meridian/parallel gridlines so a single noisy pixel
/// doesn't flip a whole hex's classification.
pub fn sample_avg_3x3(pixels: &[u8], iw: u32, ih: u32, cx: i32, cy: i32) -> (u8, u8, u8) {
    let mut rs: u32 = 0;
    let mut gs: u32 = 0;
    let mut bs: u32 = 0;
    let mut n: u32 = 0;
    let iw_i = iw as i32;
    let ih_i = ih as i32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let x = cx + dx;
            let y = cy + dy;
            if x < 0 || y < 0 || x >= iw_i || y >= ih_i {
                continue;
            }
            let idx = ((y as u32 * iw + x as u32) * 3) as usize;
            if idx + 2 >= pixels.len() {
                continue;
            }
            rs += pixels[idx] as u32;
            gs += pixels[idx + 1] as u32;
            bs += pixels[idx + 2] as u32;
            n += 1;
        }
    }
    if n == 0 {
        (0, 0, 0)
    } else {
        ((rs / n) as u8, (gs / n) as u8, (bs / n) as u8)
    }
}

/// Classify an averaged RGB sample from the flat-earth PNG into a Terrain.
///
/// Source palette (rough): deep blue for open ocean, paler blue for shelf
/// seas, olive/khaki for vegetated land, tan for desert, grey-brown for
/// mountains, near-white for polar ice. Gridlines are grey — handled by the
/// 3x3 averaging upstream.
pub fn classify_pixel(r: u8, g: u8, b: u8) -> Terrain {
    let ri = r as i32;
    let gi = g as i32;
    let bi = b as i32;
    let brightness = ri + gi + bi;
    let max_ch = ri.max(gi).max(bi);
    let min_ch = ri.min(gi).min(bi);

    // Near-white with no colour cast → polar ice.
    if brightness > 660 && max_ch - min_ch < 30 {
        return Terrain::Ice;
    }

    // Blue-dominant → water. Darker = deeper.
    if bi > gi + 10 && bi > ri + 15 {
        if brightness < 400 {
            return Terrain::DeepOcean;
        }
        if brightness > 620 {
            return Terrain::Coast;
        }
        return Terrain::Ocean;
    }

    // Olive / yellow-green dominant → vegetated land.
    // Palette: (108, 124, 88), (133, 130, 95), (111, 124, 81).
    if gi >= ri.saturating_sub(5) && gi > bi + 5 {
        if brightness < 330 {
            return Terrain::Forest;
        }
        if ri > 150 && gi > 150 {
            return Terrain::Plains;
        }
        return Terrain::Forest;
    }

    // Bright warm tone → desert.
    if ri > 170 && gi > 130 && bi < 170 && ri >= gi && ri >= bi {
        return Terrain::Desert;
    }

    // Muted warm tone → hills/mountains.
    if ri >= bi && ri < 180 && gi < 140 {
        if ri < 110 {
            return Terrain::Mountain;
        }
        return Terrain::Hills;
    }

    // Light neutral grey → tundra.
    if brightness > 500 && (ri - gi).abs() < 30 && (gi - bi).abs() < 30 {
        return Terrain::Tundra;
    }

    Terrain::Plains
}

/// Map a Terrain back to a representative elevation (metres). Used when the
/// source image doesn't carry a height channel.
pub fn terrain_elevation(t: Terrain) -> i32 {
    match t {
        Terrain::DeepOcean => -3000,
        Terrain::Ocean => -800,
        Terrain::Coast => -30,
        Terrain::Plains => 120,
        Terrain::Forest => 220,
        Terrain::Hills => 600,
        Terrain::Mountain => 2200,
        Terrain::Desert => 300,
        Terrain::Tundra => 180,
        Terrain::Urban => 130,
        Terrain::Ice => 500,
    }
}
