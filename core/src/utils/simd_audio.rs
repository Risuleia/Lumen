#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{
    _mm256_add_ps, _mm256_fmadd_ps, _mm256_fnmadd_ps, _mm256_loadu_ps, _mm256_mul_ps,
    _mm256_permute2f128_ps, _mm256_set1_ps, _mm256_setzero_ps, _mm256_shuffle_ps, _mm256_sqrt_ps,
    _mm256_storeu_ps,
};

#[cfg(target_arch = "x86_64")]
use rustfft::num_complex::Complex;

use crate::services::audio::KineticBand;

#[inline(always)]
#[cfg(target_arch = "x86_64")]
pub unsafe fn simd_window_and_cast(
    samples_source: &[f32],
    window_coeffs: &[f32],
    fft_input_buffer: &mut [Complex<f32>],
) {
    if is_x86_feature_detected!("avx2") {
        unsafe {
            let mut idx = 0;
            let src_ptr = samples_source.as_ptr();
            let win_ptr = window_coeffs.as_ptr();
            let dest_ptr = fft_input_buffer.as_mut_ptr() as *mut f32;

            while idx < 2048 {
                let a0 = _mm256_loadu_ps(src_ptr.add(idx));
                let w0 = _mm256_loadu_ps(win_ptr.add(idx));
                let a1 = _mm256_loadu_ps(src_ptr.add(idx + 8));
                let w1 = _mm256_loadu_ps(win_ptr.add(idx + 8));

                let r0 = _mm256_mul_ps(a0, w0);
                let r1 = _mm256_mul_ps(a1, w1);

                let zeros = _mm256_setzero_ps();

                let shuf0 = _mm256_shuffle_ps::<0b01_00_01_00>(r0, zeros);
                let shuf1 = _mm256_shuffle_ps::<0b11_10_11_10>(r0, zeros);
                let shuf2 = _mm256_shuffle_ps::<0b01_00_01_00>(r1, zeros);
                let shuf3 = _mm256_shuffle_ps::<0b11_10_11_10>(r1, zeros);

                let out0 = _mm256_permute2f128_ps::<0b0010_0000>(shuf0, shuf1);
                let out1 = _mm256_permute2f128_ps::<0b0011_0001>(shuf0, shuf1);
                let out2 = _mm256_permute2f128_ps::<0b0010_0000>(shuf2, shuf3);
                let out3 = _mm256_permute2f128_ps::<0b0011_0001>(shuf2, shuf3);

                _mm256_storeu_ps(dest_ptr.add(idx * 2), out0);
                _mm256_storeu_ps(dest_ptr.add(idx * 2 + 8), out1);
                _mm256_storeu_ps(dest_ptr.add(idx * 2 + 16), out2);
                _mm256_storeu_ps(dest_ptr.add(idx * 2 + 24), out3);

                idx += 16;
            }
        }
    } else {
        for idx in 0..2048 {
            fft_input_buffer[idx] =
                Complex { re: samples_source[idx] * window_coeffs[idx], im: 0.0 };
        }
    }
}

#[inline(always)]
#[cfg(target_arch = "x86_64")]
pub unsafe fn simd_extract_magnitudes(fft_output: &[Complex<f32>], magnitude_bins: &mut [f32]) {
    if is_x86_feature_detected!("avx2") {
        unsafe {
            let source_ptr = fft_output.as_ptr() as *const f32;
            let mut idx = 0;

            while idx < 1024 {
                let chunk0 = _mm256_loadu_ps(source_ptr.add(idx * 2));
                let chunk1 = _mm256_loadu_ps(source_ptr.add(idx * 2 + 8));

                let r_shuf0 = _mm256_shuffle_ps::<0b10_10_00_00>(chunk0, chunk0);
                let r_shuf1 = _mm256_shuffle_ps::<0b10_10_00_00>(chunk1, chunk1);

                let i_shuf0 = _mm256_shuffle_ps::<0b11_11_01_01>(chunk0, chunk0);
                let i_shuf1 = _mm256_shuffle_ps::<0b11_11_01_01>(chunk1, chunk1);

                let reals = _mm256_permute2f128_ps::<0b0010_0000>(r_shuf0, r_shuf1);
                let imags = _mm256_permute2f128_ps::<0b0010_0000>(i_shuf0, i_shuf1);

                let r_squared = _mm256_mul_ps(reals, reals);
                let i_squared = _mm256_mul_ps(imags, imags);
                let sum_squares = _mm256_add_ps(r_squared, i_squared);

                let computed_magnitudes = _mm256_sqrt_ps(sum_squares);

                _mm256_storeu_ps(magnitude_bins.as_mut_ptr().add(idx), computed_magnitudes);

                idx += 8;
            }
        }
    } else {
        for idx in 0..1024 {
            magnitude_bins[idx] = fft_output[idx].norm();
        }
    }
}

#[inline(always)]
#[cfg(target_arch = "x86_64")]
pub unsafe fn simd_apply_spatial_filter(
    kinetic_bands: &[KineticBand; 24],
    band_smoothing_cache: &mut [f32; 24],
) {
    if is_x86_feature_detected!("avx2") {
        unsafe {
            let mut local_heights = [0.0f32; 24];
            for i in 0..24 {
                local_heights[i] = kinetic_bands[i].current_height;
            }

            let factor_v = _mm256_set1_ps(0.88);
            let ones_v = _mm256_set1_ps(1.0);

            let cache_ptr = band_smoothing_cache.as_mut_ptr();
            let heights_ptr = local_heights.as_ptr();

            for i in (0..24).step_by(8) {
                let mut norm_dist = [0.0f32; 8];
                for lane in 0..8 {
                    norm_dist[lane] = ((i + lane) as f32 - 11.5).abs() / 11.5;
                }

                let dist_v = _mm256_loadu_ps(norm_dist.as_ptr());
                let weight_v = _mm256_fnmadd_ps(dist_v, _mm256_mul_ps(dist_v, factor_v), ones_v);
                let h_v = _mm256_loadu_ps(heights_ptr.add(i));

                _mm256_storeu_ps(cache_ptr.add(i), _mm256_mul_ps(h_v, weight_v));
            }

            let w0_v = _mm256_set1_ps(0.02);
            let w1_v = _mm256_set1_ps(0.13);
            let w2_v = _mm256_set1_ps(0.70);

            let edge_l = *cache_ptr;
            let edge_r = *cache_ptr.add(23);

            for chunk in 0..3 {
                let idx = chunk * 8;
                let c_v = _mm256_loadu_ps(cache_ptr.add(idx));

                let mut lm2_arr = [0.0f32; 8];
                let mut lm1_arr = [0.0f32; 8];
                let mut lp1_arr = [0.0f32; 8];
                let mut lp2_arr = [0.0f32; 8];

                for lane in 0..8 {
                    let i = idx + lane;
                    lm2_arr[lane] = if i > 1 { *cache_ptr.add(i - 2) } else { edge_l };
                    lm1_arr[lane] = if i > 0 { *cache_ptr.add(i - 1) } else { edge_l };
                    lp1_arr[lane] = if i < 23 { *cache_ptr.add(i + 1) } else { edge_r };
                    lp2_arr[lane] = if i < 22 { *cache_ptr.add(i + 2) } else { edge_r };
                }

                let lm2_v = _mm256_loadu_ps(lm2_arr.as_ptr());
                let lm1_v = _mm256_loadu_ps(lm1_arr.as_ptr());
                let lp1_v = _mm256_loadu_ps(lp1_arr.as_ptr());
                let lp2_v = _mm256_loadu_ps(lp2_arr.as_ptr());

                let mut acc = _mm256_mul_ps(c_v, w2_v);
                acc = _mm256_fmadd_ps(lm1_v, w1_v, acc);
                acc = _mm256_fmadd_ps(lp1_v, w1_v, acc);
                acc = _mm256_fmadd_ps(lm2_v, w0_v, acc);
                acc = _mm256_fmadd_ps(lp2_v, w0_v, acc);

                _mm256_storeu_ps(cache_ptr.add(idx), acc);
            }
        }
    } else {
        for i in 0..24 {
            let normalized_distance = (i as f32 - 11.5).abs() / 11.5;
            let weight = 1.0 - (normalized_distance * normalized_distance) * 0.88;
            band_smoothing_cache[i] = kinetic_bands[i].current_height * weight;
        }

        let ref_v = *band_smoothing_cache;

        for i in 0..24 {
            let lm2 = if i > 1 { ref_v[i - 2] } else { ref_v[0] };
            let lm1 = if i > 0 { ref_v[i - 1] } else { ref_v[0] };
            let c = ref_v[i];
            let lp1 = if i < 23 { ref_v[i + 1] } else { ref_v[23] };
            let lp2 = if i < 22 { ref_v[i + 2] } else { ref_v[23] };

            band_smoothing_cache[i] =
                (lm2 * 0.02) + (lm1 * 0.13) + (c * 0.70) + (lp1 * 0.13) + (lp2 * 0.02);
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn simd_window_and_cast(s: &[f32], w: &[f32], b: &mut [Complex<f32>]) {
    for idx in 0..2048 {
        b[idx] = Complex { re: s[idx] * w[idx], im: 0.0 };
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn simd_extract_magnitudes(f: &[Complex<f32>], m: &mut [f32]) {
    for idx in 0..1024 {
        m[idx] = f[idx].norm();
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn apply_spatial_filter_avx2(
    kinetic_bands: &[KineticBand; 24],
    band_smoothing_cache: &mut [f32; 24],
) {
    for i in 0..24 {
        let normalized_distance = (i as f32 - 11.5).abs() / 11.5;
        let weight = 1.0 - (normalized_distance * normalized_distance) * 0.88;
        band_smoothing_cache[i] = kinetic_bands[i].current_height * weight;
    }
    let ref_v = *band_smoothing_cache;
    for i in 0..24 {
        let lm2 = if i > 1 { ref_v[i - 2] } else { ref_v[0] };
        let lm1 = if i > 0 { ref_v[i - 1] } else { ref_v[0] };
        let c = ref_v[i];
        let lp1 = if i < 23 { ref_v[i + 1] } else { ref_v[23] };
        let lp2 = if i < 22 { ref_v[i + 2] } else { ref_v[23] };
        band_smoothing_cache[i] =
            (lm2 * 0.02) + (lm1 * 0.13) + (c * 0.70) + (lp1 * 0.13) + (lp2 * 0.02);
    }
}
