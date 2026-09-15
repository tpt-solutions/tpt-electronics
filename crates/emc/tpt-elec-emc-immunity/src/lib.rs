// SPDX-License-Identifier: MIT OR Apache-2.0

//! Immunity test modeling against IEC 61000-4 severities.
//!
//! [`ImmunityTest`] checks whether a port's designed withstand level meets
//! the required test level for ESD/surge/EFT/radiated stresses using the
//! standard severity tables.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_emc_core::{EmcStandard, ImmunityLevel};

/// The kind of immunity stress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImmunityKind {
    /// Radiated RF field (IEC 61000-4-3).
    RadiatedRf,
    /// Conducted RF (IEC 61000-4-6).
    ConductedRf,
    /// ESD (IEC 61000-4-2).
    Esd,
    /// Surge (IEC 61000-4-5).
    Surge,
    /// EFT burst (IEC 61000-4-4).
    Eft,
}

/// Result of an immunity evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImmunityResult {
    /// Whether the design withstands the required level.
    pub passed: bool,
    /// Required severity (in the kind's unit).
    pub required: f64,
    /// Designed/withstand severity.
    pub designed: f64,
    /// Headroom (designed − required).
    pub headroom: f64,
}

/// An immunity test plan item.
#[derive(Clone, Debug)]
pub struct ImmunityTest {
    /// Applicable standard.
    pub standard: EmcStandard,
    /// Stress kind.
    pub kind: ImmunityKind,
    /// Required severity level index (1..4).
    pub required_level: u8,
    /// Designed withstand in the same unit as the kind's severity.
    pub designed_withstand: f64,
}

impl ImmunityTest {
    /// Evaluates the test.
    pub fn evaluate(&self) -> ImmunityResult {
        let required = match self.kind {
            ImmunityKind::RadiatedRf | ImmunityKind::ConductedRf => {
                ImmunityLevel::radiated(self.required_level).severity
            }
            ImmunityKind::Esd => ImmunityLevel::esd(self.required_level).severity,
            ImmunityKind::Surge => ImmunityLevel::surge(self.required_level).severity,
            ImmunityKind::Eft => ImmunityLevel::eft(self.required_level).severity,
        };
        ImmunityResult {
            passed: self.designed_withstand >= required,
            required,
            designed: self.designed_withstand,
            headroom: self.designed_withstand - required,
        }
    }
}

/// A port-level immunity plan: all tests must pass.
#[derive(Clone, Debug, Default)]
pub struct ImmunityPlan {
    /// Tests for the port.
    pub tests: Vec<ImmunityTest>,
}

impl ImmunityPlan {
    /// Adds a test.
    pub fn push(&mut self, test: ImmunityTest) {
        self.tests.push(test);
    }

    /// Whether every test passes.
    pub fn compliant(&self) -> bool {
        self.tests.iter().all(|t| t.evaluate().passed)
    }

    /// The weakest headroom across tests (native units).
    pub fn worst_headroom(&self) -> f64 {
        self.tests
            .iter()
            .map(|t| t.evaluate().headroom)
            .fold(f64::INFINITY, f64::min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esd_withstand_evaluates() {
        let t = ImmunityTest {
            standard: EmcStandard::Iec61000 { part: "4-2".into() },
            kind: ImmunityKind::Esd,
            required_level: 3, // 8 kV air
            designed_withstand: 15.0,
        };
        let r = t.evaluate();
        assert!(r.passed);
        assert!((r.headroom - 7.0).abs() < 1e-9);
    }

    #[test]
    fn failing_esd_detected() {
        let t = ImmunityTest {
            standard: EmcStandard::Iec61000 { part: "4-2".into() },
            kind: ImmunityKind::Esd,
            required_level: 4, // 15 kV
            designed_withstand: 8.0,
        };
        assert!(!t.evaluate().passed);
    }

    #[test]
    fn radiated_level_3() {
        let t = ImmunityTest {
            standard: EmcStandard::Iec61000 { part: "4-3".into() },
            kind: ImmunityKind::RadiatedRf,
            required_level: 3,
            designed_withstand: 10.0,
        };
        assert!(t.evaluate().passed);
        assert!((t.evaluate().required - 10.0).abs() < 1e-9);
    }

    #[test]
    fn plan_requires_all_passes() {
        let mut plan = ImmunityPlan::default();
        plan.push(ImmunityTest {
            standard: EmcStandard::Iec61000 { part: "4-2".into() },
            kind: ImmunityKind::Esd,
            required_level: 2,
            designed_withstand: 8.0,
        });
        plan.push(ImmunityTest {
            standard: EmcStandard::Iec61000 { part: "4-5".into() },
            kind: ImmunityKind::Surge,
            required_level: 3,
            designed_withstand: 1.0, // needs 2 kV
        });
        assert!(!plan.compliant());
        assert!((plan.worst_headroom() + 1.0).abs() < 1e-9);
    }
}
