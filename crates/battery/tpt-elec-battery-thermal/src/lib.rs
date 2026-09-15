// SPDX-License-Identifier: MIT OR Apache-2.0

//! Thermal runaway propagation modeling for battery packs.
//!
//! [`ThermalRunawayModel`] drives a cell-to-cell thermal network: cells
//! above the onset temperature generate heat (Arrhenius-like acceleration),
//! neighbors exchange heat through an adjacency matrix, and the simulation
//! flags newly-runaway cells over time — the classic propagation-risk study
//! for pack design (barriers, spacing, heat sinking).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Adjacency of cells in the pack (symmetric, weights = thermal conductance
/// between neighbors [W/K]).
#[derive(Clone, Debug)]
pub struct AdjacencyMatrix {
    /// Number of cells.
    pub n: usize,
    /// Conductance pairs (i, j, W/K), i < j.
    pub conductances: Vec<(usize, usize, f64)>,
}

impl AdjacencyMatrix {
    /// Linear chain: each cell coupled to its neighbors.
    pub fn chain(n: usize, conductance_w_per_k: f64) -> Self {
        Self {
            n,
            conductances: (0..n.saturating_sub(1))
                .map(|i| (i, i + 1, conductance_w_per_k))
                .collect(),
        }
    }

    /// Row of an adjacency matrix (sparse): neighbor → conductance.
    pub fn neighbors(&self, i: usize) -> impl Iterator<Item = (usize, f64)> + '_ {
        self.conductances.iter().filter_map(move |&(a, b, g)| {
            if a == i {
                Some((b, g))
            } else if b == i {
                Some((a, g))
            } else {
                None
            }
        })
    }
}

/// Thermal runaway model parameters.
#[derive(Clone, Debug)]
pub struct ThermalRunawayModel {
    /// Onset temperature [°C] where self-heating accelerates.
    pub onset_temperature: f64,
    /// Heat generation rate of one runaway cell [W].
    pub heat_generation_rate: f64,
    /// Trigger threshold for a neighbor [°C] (usually ≈ onset − 10…20 K).
    pub propagation_threshold: f64,
    /// Cell thermal mass [J/K].
    pub cell_heat_capacity: f64,
    /// Heat loss to ambient per cell [W/K].
    pub ambient_conductance: f64,
    /// Ambient temperature [°C].
    pub ambient_c: f64,
}

/// Outcome of a propagation simulation.
#[derive(Clone, Debug)]
pub struct PropagationRisk {
    /// Whether runaway spread beyond the initiating cell(s).
    pub will_propagate: bool,
    /// Cells in runaway at the end of the horizon.
    pub runaway_cells: Vec<usize>,
    /// Simulated horizon [s].
    pub horizon_s: f64,
    /// Final cell temperatures [°C].
    pub final_temperatures: Vec<f64>,
}

impl ThermalRunawayModel {
    /// Simulates the thermal network for `horizon_s` starting from
    /// `initial_temperatures` with the given initial runaway set.
    pub fn propagation_risk(
        &self,
        initial_temperatures: &[f64],
        initial_runaway: &[usize],
        adjacency: &AdjacencyMatrix,
        horizon_s: f64,
        dt: f64,
    ) -> PropagationRisk {
        let n = adjacency.n;
        let mut temps = initial_temperatures.to_vec();
        let mut runaway = vec![false; n];
        for &i in initial_runaway {
            runaway[i] = true;
        }
        let initial_count = runaway.iter().filter(|&&r| r).count();
        let mut t = 0.0;

        while t < horizon_s {
            let mut fluxes = vec![0.0f64; n];
            for i in 0..n {
                // Ambient cooling
                fluxes[i] -= self.ambient_conductance * (temps[i] - self.ambient_c);
                // Runaway heat generation
                if runaway[i] {
                    fluxes[i] += self.heat_generation_rate;
                }
                // Neighbor conduction
                for (j, g) in adjacency.neighbors(i) {
                    fluxes[i] -= g * (temps[i] - temps[j]);
                }
            }
            for i in 0..n {
                temps[i] += fluxes[i] * dt / self.cell_heat_capacity;
            }
            // Trigger new runaway
            for i in 0..n {
                if !runaway[i] && temps[i] >= self.propagation_threshold {
                    runaway[i] = true;
                }
            }
            t += dt;
        }

        let runaway_cells: Vec<usize> = (0..n).filter(|&i| runaway[i]).collect();
        let final_count = runaway_cells.len();
        PropagationRisk {
            will_propagate: final_count > initial_count,
            runaway_cells,
            horizon_s,
            final_temperatures: temps,
        }
    }

    /// Single-cell steady temperature with runaway heat and ambient cooling
    /// (checks whether one cell can even sustain runaway alone).
    pub fn single_cell_runaway_temperature(&self) -> f64 {
        self.ambient_c + self.heat_generation_rate / self.ambient_conductance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ThermalRunawayModel {
        ThermalRunawayModel {
            onset_temperature: 150.0,
            heat_generation_rate: 200.0, // W
            propagation_threshold: 140.0,
            cell_heat_capacity: 80.0,  // J/K (typical 21700-ish)
            ambient_conductance: 0.15, // W/K to 25 °C ambient (insulated pack)
            ambient_c: 25.0,
        }
    }

    #[test]
    fn isolated_cell_no_propagation() {
        // Single cell with nothing adjacent: stays a party of one.
        let m = model();
        let adj = AdjacencyMatrix::chain(3, 0.0); // no coupling
        let risk = m.propagation_risk(&[25.0, 25.0, 25.0], &[1], &adj, 100.0, 0.1);
        assert!(!risk.will_propagate);
        assert_eq!(risk.runaway_cells, vec![1]);
    }

    #[test]
    fn tight_chain_propagates() {
        // Strong cell-to-cell coupling: the runaway cooks its neighbors.
        let m = model();
        let adj = AdjacencyMatrix::chain(4, 2.0); // 2 W/K between neighbors
        let risk = m.propagation_risk(&[25.0; 4], &[0], &adj, 600.0, 0.05);
        assert!(risk.will_propagate, "cells: {:?}", risk.runaway_cells);
        assert!(risk.runaway_cells.len() >= 2);
    }

    #[test]
    fn barrier_stops_propagation() {
        // Same as above but with a thermal barrier (low conductance) between
        // cells 1 and 2.
        let m = model();
        let adj = AdjacencyMatrix {
            n: 4,
            conductances: vec![(0, 1, 2.0), (2, 3, 2.0)], // gap between 1 and 2
        };
        let risk = m.propagation_risk(&[25.0; 4], &[0], &adj, 600.0, 0.05);
        assert!(!risk.runaway_cells.contains(&2) && !risk.runaway_cells.contains(&3));
        assert_eq!(risk.runaway_cells, vec![0, 1]);
    }

    #[test]
    fn single_cell_self_heats_to_runaway_temperature() {
        let m = model();
        // 200 W / 0.5 W/K = 425 °C above ambient: runaway is self-sustaining
        assert!(m.single_cell_runaway_temperature() > m.onset_temperature + 200.0);
    }

    #[test]
    fn cooler_neighbor_never_triggers_when_far_below_threshold() {
        let mut m = model();
        m.heat_generation_rate = 5.0; // weak: stays under threshold
        m.ambient_conductance = 2.0;
        let adj = AdjacencyMatrix::chain(3, 0.5);
        let risk = m.propagation_risk(&[25.0; 3], &[0], &adj, 300.0, 0.1);
        assert!(!risk.will_propagate);
        // And the initiating cell itself stays below the propagation threshold
        assert!(risk.final_temperatures[0] < m.propagation_threshold);
    }
}
