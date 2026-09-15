// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eye diagram generation and analysis.
//!
//! [`EyeDiagram::from_waveform`] folds a waveform into a unit-interval
//! window, extracts the eye opening (height/width), and separates jitter
//! into deterministic (data-dependent, from the crossing histogram width)
//! and random components (Gaussian equivalent of the residual).
//!
//! [`EyeMask`] carries normalized mask outlines for common standards
//! (PCIe Gen1–6, DDR4/5, USB3/4, 10GbE); masks are normalized to one UI and
//! the signal amplitude so they apply to any interface scaling.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Jitter decomposition metrics (seconds).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JitterMetrics {
    /// Total jitter, peak-to-peak across all crossings.
    pub total_jitter: f64,
    /// Random jitter estimate (Gaussian σ).
    pub random_jitter: f64,
    /// Deterministic (data-dependent) jitter estimate, peak-to-peak.
    pub deterministic_jitter: f64,
    /// Raw crossing peak-to-peak.
    pub peak_to_peak: f64,
}

/// Standard compliance masks (normalized: time in UI, voltage in fractions
/// of the full amplitude swing).
#[derive(Clone, Debug, PartialEq)]
pub enum EyeMask {
    /// PCIe Gen 1 (2.5 GT/s).
    PcieGen1,
    /// PCIe Gen 2 (5 GT/s).
    PcieGen2,
    /// PCIe Gen 3 (8 GT/s).
    PcieGen3,
    /// PCIe Gen 4 (16 GT/s).
    PcieGen4,
    /// PCIe Gen 5 (32 GT/s).
    PcieGen5,
    /// PCIe Gen 6 (64 GT/s, PAM4 treated as NRZ-equivalent scaffold).
    PcieGen6,
    /// DDR4 (per-pin, normalized).
    Ddr4,
    /// DDR5.
    Ddr5,
    /// USB 3.x.
    Usb3,
    /// USB4.
    Usb4,
    /// 10GBASE-KR.
    Ethernet10G,
    /// Custom mask: forbidden rectangle `(t_center_half_ui, v_half_amplitude)`.
    Custom {
        /// Half-width of the forbidden time window [UI].
        half_ui: f64,
        /// Half-height of the forbidden voltage window [fraction of swing].
        half_amplitude: f64,
    },
}

impl EyeMask {
    /// Nominal bit rate [bits/s] of the standard (0 for custom).
    pub fn bit_rate(&self) -> f64 {
        match self {
            EyeMask::PcieGen1 => 2.5e9,
            EyeMask::PcieGen2 => 5.0e9,
            EyeMask::PcieGen3 => 8.0e9,
            EyeMask::PcieGen4 => 16.0e9,
            EyeMask::PcieGen5 => 32.0e9,
            EyeMask::PcieGen6 => 64.0e9,
            EyeMask::Ddr4 => 3.2e9,
            EyeMask::Ddr5 => 6.4e9,
            EyeMask::Usb3 => 10.0e9,
            EyeMask::Usb4 => 20.0e9,
            EyeMask::Ethernet10G => 10.3125e9,
            EyeMask::Custom { .. } => 0.0,
        }
    }

    /// The forbidden center region: `(half_ui, half_amplitude)` normalized.
    ///
    /// Values are simplified but representative of the standard's mask
    /// intent: a fraction of the UI and of the amplitude is reserved.
    pub fn forbidden_region(&self) -> (f64, f64) {
        match self {
            EyeMask::PcieGen1 => (0.25, 0.25),
            EyeMask::PcieGen2 => (0.25, 0.30),
            EyeMask::PcieGen3 => (0.22, 0.35),
            EyeMask::PcieGen4 => (0.20, 0.35),
            EyeMask::PcieGen5 => (0.18, 0.35),
            EyeMask::PcieGen6 => (0.16, 0.35),
            EyeMask::Ddr4 => (0.30, 0.30),
            EyeMask::Ddr5 => (0.28, 0.30),
            EyeMask::Usb3 => (0.25, 0.30),
            EyeMask::Usb4 => (0.22, 0.33),
            EyeMask::Ethernet10G => (0.24, 0.30),
            EyeMask::Custom {
                half_ui,
                half_amplitude,
            } => (*half_ui, *half_amplitude),
        }
    }
}

/// Mask compliance outcome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaskResult {
    /// Whether the eye clears the mask.
    pub passed: bool,
    /// Time margin beyond the forbidden window [UI].
    pub margin_ui: f64,
    /// Voltage margin beyond the forbidden window [fraction of swing].
    pub margin_amplitude: f64,
}

/// Largest gap between consecutive sorted samples (internal gaps only).
fn largest_internal_gap(sorted: &[f64]) -> Option<f64> {
    if sorted.len() < 3 {
        return None;
    }
    sorted
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(None, |acc: Option<f64>, g| {
            Some(match acc {
                Some(best) => best.max(g),
                None => g,
            })
        })
}

/// An analyzed eye diagram.
#[derive(Clone, Debug)]
pub struct EyeDiagram {
    /// Bit rate of the waveform [bits/s].
    pub bit_rate: f64,
    /// Unit interval [s].
    pub ui: f64,
    /// Vertical eye opening [V].
    pub eye_height: f64,
    /// Horizontal eye opening [s].
    pub eye_width: f64,
    /// Jitter decomposition.
    pub jitter: JitterMetrics,
    /// Crossing offsets within a UI [s], for plotting/histograms.
    pub crossings: Vec<f64>,
}

impl EyeDiagram {
    /// Analyzes a waveform by folding it into a unit-interval window.
    ///
    /// * `waveform` — uniformly sampled voltage samples
    /// * `bit_rate` — bits/s (UI = 1/bit_rate)
    /// * `samples_per_ui` — samples per unit interval
    pub fn from_waveform(waveform: &[f64], bit_rate: f64, samples_per_ui: u32) -> Self {
        let spu = samples_per_ui.max(2) as usize;
        let ui = 1.0 / bit_rate.max(1e-300);
        let dt = ui / spu as f64;

        if waveform.len() < 2 * spu + 2 {
            return Self {
                bit_rate,
                ui,
                eye_height: 0.0,
                eye_width: 0.0,
                jitter: JitterMetrics::default(),
                crossings: Vec::new(),
            };
        }

        // Amplitude and threshold
        let vmax = waveform.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let vmin = waveform.iter().cloned().fold(f64::INFINITY, f64::min);
        let threshold = (vmax + vmin) / 2.0;
        let _swing = (vmax - vmin).max(1e-300);

        // Find crossings and their UI-relative offsets.
        let mut crossings = Vec::new();
        let mut eye_width_min = f64::INFINITY;
        let _ = &mut eye_width_min;
        let mut offsets: Vec<f64> = Vec::new();
        for i in 1..waveform.len() {
            let a = waveform[i - 1];
            let b = waveform[i];
            if (a < threshold) != (b < threshold) {
                // linear interpolation of the crossing instant
                let t = (i as f64 - 1.0) * dt + (threshold - a) / (b - a).max(1e-300) * dt;
                // offset from the nearest UI boundary
                let nearest = (t / ui).round() * ui;
                let off = t - nearest;
                offsets.push(off);
                crossings.push(t);
            }
        }
        offsets.sort_by(|a, b| a.total_cmp(b));
        let pp = offsets
            .last()
            .zip(offsets.first())
            .map(|(hi, lo)| hi - lo)
            .unwrap_or(0.0);
        // First-order separation (B6): a bimodal crossing histogram (two
        // edge populations from even/odd transitions) splits at the largest
        // internal gap — that gap is the deterministic (data-dependent)
        // jitter. Unimodal crossings → DJ ≈ 0, RJ absorbs the spread.
        let dj = largest_internal_gap(&offsets).unwrap_or(0.0);
        let dj = if dj > 0.15 * pp { dj } else { 0.0 };
        let rj = ((pp - dj) / 6.0).max(0.0);
        let jitter = JitterMetrics {
            total_jitter: pp,
            random_jitter: rj,
            deterministic_jitter: dj,
            peak_to_peak: pp,
        };

        // Eye width: UI minus the worst crossing spread (bounds the valid
        // sampling window).
        let eye_width = (ui - pp).max(0.0);

        // Eye height: at the center of each UI, the gap between the lowest
        // "high" sample and the highest "low" sample.
        let mut highs = Vec::new();
        let mut lows = Vec::new();
        let mut k = spu / 2;
        while k < waveform.len() {
            let v = waveform[k];
            if v >= threshold {
                highs.push(v);
            } else {
                lows.push(v);
            }
            k += spu;
        }
        let eye_height = match (
            highs.iter().min_by(|a, b| a.total_cmp(b)),
            lows.iter().max_by(|a, b| a.total_cmp(b)),
        ) {
            (Some(h), Some(l)) => (h - l).max(0.0),
            _ => 0.0,
        };

        Self {
            bit_rate,
            ui,
            eye_height,
            eye_width,
            jitter,
            crossings,
        }
    }

    /// Checks compliance against a standard mask.
    ///
    /// The mask's forbidden region is normalized (UI, amplitude fraction);
    /// `amplitude_v` is the actual signal swing used to scale the voltage.
    pub fn check_mask_compliance(&self, mask: &EyeMask, amplitude_v: f64) -> MaskResult {
        let (half_ui, half_amp) = mask.forbidden_region();
        let t_margin_ui = self.eye_width / self.ui - 2.0 * half_ui;
        let v_margin = self.eye_height - 2.0 * half_amp * amplitude_v;
        MaskResult {
            passed: t_margin_ui >= 0.0 && v_margin >= 0.0,
            margin_ui: t_margin_ui,
            margin_amplitude: v_margin,
        }
    }
}

/// Deterministic pseudo-random binary sequence generator (PRBS-7), useful
/// for building test waveforms without external dependencies.
pub struct Prbs {
    state: u8,
}

impl Prbs {
    /// A PRBS-7 generator (x⁷ + x⁶ + 1 LFSR); nonzero seeds only.
    pub fn prbs7(seed: u8) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed & 0x7f },
        }
    }

    /// Returns the next bit (0 or 1). Full period is 127 bits.
    pub fn next_bit(&mut self) -> u8 {
        let out = self.state & 1;
        let feedback = ((self.state >> 6) ^ (self.state >> 5)) & 1;
        self.state = ((self.state << 1) | feedback) & 0x7f;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a clean NRZ waveform: level ±0.5 with first-order smoothing.
    fn make_waveform(bits: &[u8], samples_per_ui: u32, tau_s: f64) -> Vec<f64> {
        let ui = 12.5e-9; // 80 Mb/s
        let dt = ui / samples_per_ui as f64;
        let alpha = dt / (tau_s + dt);
        let mut out = Vec::new();
        let mut v = -0.5f64;
        for &bit in bits {
            let target = if bit == 1 { 0.5 } else { -0.5 };
            for _ in 0..samples_per_ui {
                v += alpha * (target - v);
                out.push(v);
            }
        }
        out
    }

    #[test]
    fn clean_eye_is_wide_open() {
        let bits = [1u8, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1];
        let wf = make_waveform(&bits, 32, 0.5e-9); // τ ≪ UI
        let eye = EyeDiagram::from_waveform(&wf, 1.0 / 12.5e-9, 32);
        // Nearly full swing opening (0.9 V of the 1.0 V swing)
        assert!(eye.eye_height > 0.85, "height {}", eye.eye_height);
        assert!(eye.eye_width > 0.8 * eye.ui, "width {}", eye.eye_width);
        assert!(eye.jitter.total_jitter < 0.1 * eye.ui);
    }

    #[test]
    fn degraded_channel_closes_eye() {
        let bits = [1u8, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1];
        let clean = make_waveform(&bits, 32, 0.5e-9);
        let slow = make_waveform(&bits, 32, 8e-9); // τ ≈ 0.64 UI
        let eye_clean = EyeDiagram::from_waveform(&clean, 80e6, 32);
        let eye_slow = EyeDiagram::from_waveform(&slow, 80e6, 32);
        assert!(
            eye_slow.eye_height < eye_clean.eye_height,
            "degraded {} vs clean {}",
            eye_slow.eye_height,
            eye_clean.eye_height
        );
        assert!(eye_slow.jitter.total_jitter > eye_clean.jitter.total_jitter);
    }

    #[test]
    fn mask_compliance_pass_and_fail() {
        let bits = [1u8, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1];
        let wf = make_waveform(&bits, 32, 0.5e-9);
        let eye = EyeDiagram::from_waveform(&wf, 80e6, 32);
        let ok = eye.check_mask_compliance(
            &EyeMask::Custom {
                half_ui: 0.1,
                half_amplitude: 0.2,
            },
            1.0,
        );
        assert!(ok.passed);
        let fail = eye.check_mask_compliance(
            &EyeMask::Custom {
                half_ui: 0.52,
                half_amplitude: 0.51,
            },
            1.0,
        );
        assert!(!fail.passed);
        assert!(fail.margin_ui < 0.0);
    }

    #[test]
    fn standard_masks_have_expected_bit_rates() {
        assert_eq!(EyeMask::PcieGen3.bit_rate(), 8.0e9);
        assert_eq!(EyeMask::PcieGen4.bit_rate(), 16.0e9);
        assert_eq!(EyeMask::Ethernet10G.bit_rate(), 10.3125e9);
        // Mask center shrinks as data rates rise
        let (g1,) = (EyeMask::PcieGen1.forbidden_region().0,);
        let (g6,) = (EyeMask::PcieGen6.forbidden_region().0,);
        assert!(g6 < g1);
    }

    #[test]
    fn prbs7_sequence_properties() {
        let mut p = Prbs::prbs7(0x41);
        let ones = (0..127).map(|_| p.next_bit()).sum::<u8>();
        // PRBS-7 has 127 bits, 64 ones and 63 zeros (balanced)
        assert_eq!(ones, 64);
    }

    #[test]
    fn short_waveform_returns_closed_eye() {
        let eye = EyeDiagram::from_waveform(&[0.0, 1.0], 1e9, 32);
        assert_eq!(eye.eye_height, 0.0);
        assert_eq!(eye.eye_width, 0.0);
    }

    #[test]
    fn golden_pcie_gen3_eye() {
        // Golden reference: test-data/golden/si/pcie_gen3_eye.json
        #[derive(serde::Deserialize)]
        struct Channel {
            tau_ui: f64,
            eye_height_v: f64,
            eye_width_ps: f64,
            total_jitter_ps: f64,
            mask_passed: bool,
        }
        #[derive(serde::Deserialize)]
        struct Golden {
            amplitude_v: f64,
            clean_channel: Channel,
            degraded_channel: Channel,
            tolerance_rel: f64,
        }
        let raw = include_str!("../../../../test-data/golden/si/pcie_gen3_eye.json");
        let golden: Golden = serde_json::from_str(raw).unwrap();

        let bit_rate = 8.0e9;
        let ui = 1.0 / bit_rate;
        let spu = 32u32;
        let dt = ui / spu as f64;

        for case in [&golden.clean_channel, &golden.degraded_channel] {
            let tau = case.tau_ui * ui;
            let alpha = dt / (tau + dt);
            let mut prbs = Prbs::prbs7(0x41);
            let mut v = -golden.amplitude_v / 2.0;
            let mut wf = Vec::new();
            for _ in 0..8 {
                let bit = prbs.next_bit();
                let target = if bit == 1 {
                    golden.amplitude_v / 2.0
                } else {
                    -golden.amplitude_v / 2.0
                };
                for _ in 0..spu {
                    v += alpha * (target - v);
                    wf.push(v);
                }
            }
            let eye = EyeDiagram::from_waveform(&wf, bit_rate, spu);
            let tol = golden.tolerance_rel;
            assert!(
                (eye.eye_height - case.eye_height_v).abs() / case.eye_height_v < tol,
                "height {} vs {}",
                eye.eye_height,
                case.eye_height_v
            );
            assert!(
                (eye.eye_width * 1e12 - case.eye_width_ps).abs() / case.eye_width_ps < tol,
                "width {} ps vs {} ps",
                eye.eye_width * 1e12,
                case.eye_width_ps
            );
            assert!(
                (eye.jitter.total_jitter * 1e12 - case.total_jitter_ps).abs()
                    / case.total_jitter_ps
                    < tol * 3.0,
                "tj {} ps vs {} ps",
                eye.jitter.total_jitter * 1e12,
                case.total_jitter_ps
            );
            let mask = eye.check_mask_compliance(&EyeMask::PcieGen3, golden.amplitude_v);
            assert_eq!(mask.passed, case.mask_passed);
        }
    }
}
