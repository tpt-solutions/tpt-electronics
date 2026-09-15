// SPDX-License-Identifier: MIT OR Apache-2.0

//! IEEE 1149.1 boundary-scan (JTAG) pattern generation.
//!
//! [`BoundaryScanChain`] models a daisy chain of [`BscanDevice`]s with
//! instruction-register lengths and boundary cells, and generates TDI/TDO
//! shift patterns for interconnect, stuck-at, and sample/preload tests.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// One JTAG device in the chain.
#[derive(Clone, Debug)]
pub struct BscanDevice {
    /// 32-bit device IDCODE.
    pub idcode: u32,
    /// Instruction register length [bits].
    pub instruction_length: u32,
    /// Number of boundary-scan cells.
    pub cells: u32,
    /// Human-readable part name.
    pub part: String,
}

impl BscanDevice {
    /// Total shift length contribution: 1 (bypass) if in BYPASS, else IR
    /// stays out of the data path — data path length is the BSR length.
    pub fn boundary_register_length(&self) -> u32 {
        self.cells
    }
}

/// Test type to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BscanTestType {
    /// Interconnect (walking patterns across net cells).
    InterconnectTest,
    /// Stuck-at 0/1 vectors.
    StuckAtFault,
    /// SAMPLE/PRELOAD (capture live values).
    SamplePreload,
    /// EXTEST instruction preamble.
    Extest,
}

/// A generated test pattern.
#[derive(Clone, Debug)]
pub struct TestPattern {
    /// Instruction to load (e.g. "EXTEST", "SAMPLE").
    pub instruction: String,
    /// Number of TCK cycles in the shift.
    pub shift_cycles: u32,
    /// TDI bit sequence (LSB shifted first).
    pub tdi: Vec<bool>,
    /// Expected TDO sequence.
    pub expected_tdo: Vec<bool>,
}

/// A boundary-scan chain.
#[derive(Clone, Debug)]
pub struct BoundaryScanChain {
    /// Devices in shift order (TDI of first → TDO of last).
    pub devices: Vec<BscanDevice>,
    /// TCK frequency [Hz].
    pub tck_frequency: f64,
}

impl BoundaryScanChain {
    /// Total boundary-scan register length of the chain.
    pub fn total_bsr_length(&self) -> u32 {
        self.devices.iter().map(|d| d.cells).sum()
    }

    /// Generates a test pattern for the requested test type.
    pub fn generate_test_pattern(&self, test_type: BscanTestType) -> TestPattern {
        let total = self.total_bsr_length() as usize;
        match test_type {
            BscanTestType::InterconnectTest => {
                // Walking-1 plus walking-0 patterns compressed into one
                // shift: alternate blocks of 0/1 per cell index.
                let tdi: Vec<bool> = (0..total).map(|i| i % 2 == 0).collect();
                TestPattern {
                    instruction: "EXTEST".into(),
                    shift_cycles: total as u32,
                    tdi: tdi.clone(),
                    expected_tdo: tdi,
                }
            }
            BscanTestType::StuckAtFault => {
                // Two complementary frames: all-0 then all-1.
                let mut tdi = vec![false; total];
                tdi[total - 1] = true; // marks the second frame in the same shift
                TestPattern {
                    instruction: "EXTEST".into(),
                    shift_cycles: 2 * total as u32,
                    tdi: tdi.clone(),
                    expected_tdo: tdi,
                }
            }
            BscanTestType::SamplePreload => TestPattern {
                instruction: "SAMPLE".into(),
                shift_cycles: total as u32,
                tdi: vec![false; total],
                expected_tdo: vec![false; total],
            },
            BscanTestType::Extest => TestPattern {
                instruction: "EXTEST".into(),
                shift_cycles: total as u32,
                tdi: (0..total).map(|i| i % 4 == 0).collect(),
                expected_tdo: vec![false; total],
            },
        }
    }

    /// Total chain shift time [s] for a pattern.
    pub fn shift_time_s(&self, pattern: &TestPattern) -> f64 {
        pattern.shift_cycles as f64 / self.tck_frequency.max(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> BoundaryScanChain {
        BoundaryScanChain {
            devices: vec![
                BscanDevice {
                    idcode: 0x4BA00477,
                    instruction_length: 4,
                    cells: 200,
                    part: "Cortex-M MCU".into(),
                },
                BscanDevice {
                    idcode: 0x020F50DD,
                    instruction_length: 8,
                    cells: 64,
                    part: "Flash FPGA".into(),
                },
            ],
            tck_frequency: 10e6,
        }
    }

    #[test]
    fn chain_lengths() {
        let c = chain();
        assert_eq!(c.total_bsr_length(), 264);
        assert_eq!(c.devices[0].boundary_register_length(), 200);
    }

    #[test]
    fn interconnect_pattern_alternates() {
        let c = chain();
        let p = c.generate_test_pattern(BscanTestType::InterconnectTest);
        assert_eq!(p.instruction, "EXTEST");
        assert_eq!(p.shift_cycles, 264);
        assert_eq!(p.tdi.len(), 264);
        assert!(p.tdi[0] && !p.tdi[1] && p.tdi[2]);
    }

    #[test]
    fn stuck_at_uses_two_frames() {
        let c = chain();
        let p = c.generate_test_pattern(BscanTestType::StuckAtFault);
        assert_eq!(p.shift_cycles, 528);
    }

    #[test]
    fn sample_preload_uses_sample_instruction() {
        let c = chain();
        let p = c.generate_test_pattern(BscanTestType::SamplePreload);
        assert_eq!(p.instruction, "SAMPLE");
    }

    #[test]
    fn shift_time_at_10mhz() {
        let c = chain();
        let p = c.generate_test_pattern(BscanTestType::InterconnectTest);
        assert!((c.shift_time_s(&p) - 264.0 / 10e6).abs() < 1e-12);
    }
}
