//! BGRA to NV12 (BT.709, limited range): the input hardware video encoders take. Pure CPU, split
//! across a few threads so a 1080p frame costs a couple of milliseconds.

/// Bytes an NV12 picture of `w`×`h` (both even) takes: a full Y plane, then half-size UV pairs.
pub fn nv12_len(w: u32, h: u32) -> usize {
    w as usize * h as usize * 3 / 2
}

#[inline]
fn luma(b: i32, g: i32, r: i32) -> u8 {
    ((47 * r + 157 * g + 16 * b + 128) >> 8).clamp(0, 219) as u8 + 16
}

/// Converts tightly packed BGRA into `out` (`nv12_len` bytes). `w` and `h` must be even.
pub fn bgra_to_nv12(bgra: &[u8], w: u32, h: u32, out: &mut [u8]) {
    let (w, h) = (w as usize, h as usize);
    assert!(w % 2 == 0 && h % 2 == 0, "NV12 needs an even size");
    assert!(bgra.len() >= w * h * 4 && out.len() >= w * h * 3 / 2);
    let (y_plane, uv_plane) = out.split_at_mut(w * h);
    // Bands of row pairs, one per thread.
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 4);
    let pairs = h / 2;
    let per = pairs.div_ceil(threads);
    std::thread::scope(|s| {
        let mut ys = y_plane;
        let mut uvs = uv_plane;
        let mut pair = 0;
        while pair < pairs {
            let n = per.min(pairs - pair);
            let (y_band, y_rest) = ys.split_at_mut(n * 2 * w);
            let (uv_band, uv_rest) = uvs.split_at_mut(n * w);
            ys = y_rest;
            uvs = uv_rest;
            let src = &bgra[pair * 2 * w * 4..(pair + n) * 2 * w * 4];
            s.spawn(move || convert_band(src, w, n, y_band, uv_band));
            pair += n;
        }
    });
}

fn convert_band(src: &[u8], w: usize, pairs: usize, y: &mut [u8], uv: &mut [u8]) {
    for p in 0..pairs {
        let r0 = &src[p * 2 * w * 4..(p * 2 + 1) * w * 4];
        let r1 = &src[(p * 2 + 1) * w * 4..(p * 2 + 2) * w * 4];
        for x in (0..w).step_by(2) {
            let mut sum = [0i32; 3];
            for (row, dy) in [(r0, 0), (r1, 1)] {
                for dx in 0..2 {
                    let o = (x + dx) * 4;
                    let (b, g, r) = (row[o] as i32, row[o + 1] as i32, row[o + 2] as i32);
                    y[(p * 2 + dy) * w + x + dx] = luma(b, g, r);
                    sum[0] += b;
                    sum[1] += g;
                    sum[2] += r;
                }
            }
            let (b, g, r) = ((sum[0] + 2) >> 2, (sum[1] + 2) >> 2, (sum[2] + 2) >> 2);
            let u = ((-26 * r - 86 * g + 112 * b + 128) >> 8) + 128;
            let v = ((112 * r - 102 * g - 10 * b + 128) >> 8) + 128;
            uv[p * w + x] = u.clamp(16, 240) as u8;
            uv[p * w + x + 1] = v.clamp(16, 240) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, bgr: [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|_| [bgr[0], bgr[1], bgr[2], 255]).collect()
    }

    #[test]
    fn black_white_and_grey_map_to_limited_range() {
        let mut out = vec![0; nv12_len(4, 2)];
        bgra_to_nv12(&solid(4, 2, [0, 0, 0]), 4, 2, &mut out);
        assert!(out[..8].iter().all(|&y| y == 16) && out[8..].iter().all(|&c| c == 128), "{out:?}");
        bgra_to_nv12(&solid(4, 2, [255, 255, 255]), 4, 2, &mut out);
        assert!(out[..8].iter().all(|&y| y == 235) && out[8..].iter().all(|&c| c == 128), "{out:?}");
    }

    #[test]
    fn pure_colours_land_where_bt709_puts_them() {
        let mut out = vec![0; nv12_len(2, 2)];
        // Red: Y 63, U 102, V 240.
        bgra_to_nv12(&solid(2, 2, [0, 0, 255]), 2, 2, &mut out);
        assert!((out[0] as i32 - 63).abs() <= 1 && (out[4] as i32 - 102).abs() <= 1 && (out[5] as i32 - 240).abs() <= 1, "{out:?}");
        // Blue: Y 32, U 240, V 118.
        bgra_to_nv12(&solid(2, 2, [255, 0, 0]), 2, 2, &mut out);
        assert!((out[0] as i32 - 32).abs() <= 1 && (out[4] as i32 - 240).abs() <= 1 && (out[5] as i32 - 118).abs() <= 1, "{out:?}");
    }

    #[test]
    fn threaded_bands_match_one_pass() {
        // A 1080p-shaped gradient: every band boundary must line up.
        let (w, h) = (64u32, 54u32);
        let src: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i % 239) as u8, (i % 7 * 30) as u8, 255]).collect();
        let mut threaded = vec![0; nv12_len(w, h)];
        bgra_to_nv12(&src, w, h, &mut threaded);
        let mut single = vec![0; nv12_len(w, h)];
        let (yp, uvp) = single.split_at_mut((w * h) as usize);
        convert_band(&src, w as usize, h as usize / 2, yp, uvp);
        assert_eq!(threaded, single);
    }
}
