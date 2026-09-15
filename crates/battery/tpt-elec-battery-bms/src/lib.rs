// SPDX-License-Identifier: MIT OR Apache-2.0

//! State-of-charge estimation.
//!
//! [`SocEstimator`] variants: coulomb counting, an extended Kalman filter
//! over the 1-RC ECM ([`KalmanFilterEstimator`]), and scaffold slots for
//! UKF / neural estimators. The KF state is `[SOC, V_rc]` with the ECM
//! terminal voltage as the measurement, giving the classic sensor-fusion
//! behavior: drift-free under load thanks to voltage corrections.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_battery_core::EquivalentCircuitModel;

/// SOC estimator family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocEstimator {
    /// Pure coulomb counting (needs an accurate initial SOC).
    CoulombCounting,
    /// Extended Kalman filter over the ECM.
    KalmanFilter,
    /// Unscented Kalman filter (scaffolded).
    UnscentedKalmanFilter,
    /// Neural estimator (scaffolded).
    NeuralNetwork,
}

/// Kalman-filter SOC estimator over state `[SOC, V_rc]`.
pub struct KalmanFilterEstimator {
    /// The cell model.
    pub model: EquivalentCircuitModel,
    /// Cell capacity [Ah].
    pub capacity_ah: f64,
    /// State: [SOC, RC-branch voltage].
    pub state: [f64; 2],
    /// 2×2 covariance (row-major).
    pub covariance: [[f64; 2]; 2],
    /// Process noise on SOC [1/step²].
    pub process_noise_soc: f64,
    /// Process noise on the RC state [V²].
    pub process_noise_rc: f64,
    /// Measurement noise [V²].
    pub measurement_noise: f64,
}

impl KalmanFilterEstimator {
    /// Initializes at a guessed SOC with fresh covariance.
    pub fn new(model: EquivalentCircuitModel, capacity_ah: f64, initial_soc: f64) -> Self {
        Self {
            model,
            capacity_ah,
            state: [initial_soc, 0.0],
            covariance: [[0.05, 0.0], [0.0, 1e-4]],
            process_noise_soc: 1e-8,
            process_noise_rc: 1e-6,
            measurement_noise: 1e-4, // (20 mV)²
        }
    }

    /// Pure coulomb-counting prediction step for constant `current` over
    /// `dt` seconds, propagating covariance with simple process noise.
    pub fn predict(&mut self, current: f64, dt: f64) {
        let d_soc = -current * dt / (self.capacity_ah * 3600.0);
        self.state[0] = (self.state[0] + d_soc).clamp(0.0, 1.0);

        // RC state prediction (exact, same as the ECM)
        if let Some(pair) = self.model.rc_pairs.first() {
            let tau = pair.tau().max(1e-9);
            let target = current * pair.r;
            self.state[1] = target + (self.state[1] - target) * (-dt / tau).exp();
        }

        // Covariance propagation (diagonal + process noise)
        let f_soc = 1.0; // dSOC/dSOC = 1
        let f_rc = (-dt / self.model.rc_pairs[0].tau().max(1e-9)).exp();
        self.covariance[0][0] = f_soc * f_soc * self.covariance[0][0] + self.process_noise_soc;
        self.covariance[1][1] = f_rc * f_rc * self.covariance[1][1] + self.process_noise_rc;
    }

    /// Measurement update with a sensed terminal `voltage` under the load
    /// current used by the last predict step.
    pub fn update(&mut self, voltage: f64, current: f64) {
        // h(x): V = OCV(soc) − I·R0 − V_rc
        let ocv = self.model.ocv.voltage_at_soc(self.state[0], 25.0);
        let predicted_v = ocv - current * self.model.r0 - self.state[1];

        // dV/dSOC ≈ (OCV(soc+δ) − OCV(soc−δ))/(2δ)
        let delta = 0.01;
        let dh_dsoc = (self
            .model
            .ocv
            .voltage_at_soc((self.state[0] + delta).min(1.0), 25.0)
            - self
                .model
                .ocv
                .voltage_at_soc((self.state[0] - delta).max(0.0), 25.0))
            / (2.0 * delta)
            / 1.0;
        // H = [dV/dSOC, dV/dVrc] = [dh_dsoc, −1]
        let h = [dh_dsoc, -1.0];

        // S = H P Hᵀ + R
        let s = h[0] * (self.covariance[0][0] * h[0] + self.covariance[1][0] * h[1])
            + h[1] * (self.covariance[0][1] * h[0] + self.covariance[1][1] * h[1])
            + self.measurement_noise;

        // K = P Hᵀ / S
        let k0 = (self.covariance[0][0] * h[0] + self.covariance[0][1] * h[1]) / s;
        let k1 = (self.covariance[1][0] * h[0] + self.covariance[1][1] * h[1]) / s;

        let innovation = voltage - predicted_v;
        self.state[0] = (self.state[0] + k0 * innovation).clamp(0.0, 1.0);
        self.state[1] += k1 * innovation;

        // Joseph-lite covariance update
        let p00 = (1.0 - k0 * h[0]) * self.covariance[0][0] - k0 * h[1] * self.covariance[1][0];
        let p11 = (1.0 - k1 * h[1]) * self.covariance[1][1] - k1 * h[0] * self.covariance[0][1];
        self.covariance[0][0] = p00.max(1e-12);
        self.covariance[1][1] = p11.max(1e-12);
    }

    /// Current SOC estimate.
    pub fn soc(&self) -> f64 {
        self.state[0]
    }
}

/// Runs a synthetic drive cycle through the ECM and lets the KF track SOC.
///
/// Returns the final |SOC error| — the classic estimator validation.
pub fn track_soc(
    model: &EquivalentCircuitModel,
    capacity_ah: f64,
    currents: &[f64],
    dt: f64,
    initial_soc_guess: f64,
) -> f64 {
    let mut kf = KalmanFilterEstimator::new(model.clone(), capacity_ah, initial_soc_guess);
    let mut true_soc = 0.8f64; // reality: starts at 80 %
    let mut rc_state = vec![0.0; model.rc_pairs.len()];

    for &i in currents {
        // Reality steps first (true SOC and RC state)
        true_soc = model.update_soc(true_soc, i, dt, capacity_ah);
        rc_state = model.step(i, dt, &rc_state);
        let v_true = model.terminal_voltage(true_soc, i, &rc_state);

        // Estimator: predict with the measured current, correct with the
        // sensed voltage.
        kf.predict(i, dt);
        kf.update(v_true, i);
    }

    (kf.soc() - true_soc).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coulomb_counting_integrates() {
        let model = EquivalentCircuitModel::one_rc(0.03, 0.02, 2000.0);
        let mut kf = KalmanFilterEstimator::new(model, 2.0, 0.5);
        // 100 s at 2 A on a 2 Ah cell: ΔSOC = 100·2/7200 = 0.0278
        kf.predict(2.0, 100.0);
        assert!((kf.soc() - (0.5 - 0.0278)).abs() < 1e-3);
    }

    #[test]
    fn voltage_correction_pulls_wrong_initial_guess_back() {
        // Estimator starts 20 % too high; the OCV at rest corrects it fast.
        let model = EquivalentCircuitModel::one_rc(0.03, 0.02, 2000.0);
        let capacity = 2.0;
        let true_soc = 0.5;
        let v_ocv = model.ocv.voltage_at_soc(true_soc, 25.0);

        let mut kf = KalmanFilterEstimator::new(model.clone(), capacity, 0.7);
        for _ in 0..10 {
            kf.predict(0.0, 1.0); // resting
            kf.update(v_ocv, 0.0);
        }
        assert!(
            (kf.soc() - true_soc).abs() < 0.01,
            "soc estimate {} vs {}",
            kf.soc(),
            true_soc
        );
    }

    #[test]
    fn kalman_tracks_true_soc_through_drive_cycle() {
        let model = EquivalentCircuitModel::one_rc(0.03, 0.02, 2000.0);
        // 2 A discharge pulses with rests, 10 s steps, 40 minutes.
        let mut currents = Vec::new();
        for _ in 0..60 {
            currents.extend_from_slice(&[2.0; 60]);
            currents.extend_from_slice(&[0.0; 180]);
        }
        let err = track_soc(&model, 2.0, &currents, 10.0, 0.85);
        // The filter should converge to within a couple of percent.
        assert!(err < 0.02, "SOC error {err}");
    }

    #[test]
    fn estimator_variants_enumerated() {
        let e = SocEstimator::UnscentedKalmanFilter;
        assert_ne!(e, SocEstimator::KalmanFilter);
        assert_eq!(SocEstimator::CoulombCounting, SocEstimator::CoulombCounting);
    }
}
