# Examples

Runnable demos of tpt-electronics. Each is a workspace member.

| Example | Demonstrates | Run |
|---|---|---|
| `4-layer-fr4` | Multi-layer stackup thermal solve | `cargo run -p example-4-layer-fr4` |
| `bga-thermal-cycle` | Transient thermal cycling of a BGA | `cargo run -p example-bga-thermal-cycle` |
| `buck-converter` | Automated buck converter design (power-core + control) | `cargo run -p example-buck-converter` |
| `high-current-trace` | IPC-2152 current capacity / electromigration | `cargo run -p example-high-current-trace` |
| `pcie-eye-diagram` | PCIe Gen3 eye diagram + mask compliance | `cargo run -p example-pcie-eye-diagram` |
| `simple-led-board` | Minimal LED board thermal solve | `cargo run -p example-simple-led-board` |
| `wifi-antenna` | WiFi 6E matching network synthesis (RF) | `cargo run -p example-wifi-antenna` |

For a narrated walkthrough of the full pipeline (KiCad → export → thermal → report), see the [tutorial](../docs/book/src/tutorial.md).

For a fresh analysis project scaffold, use `cargo generate tpt-solutions/tpt-electronics-template`.
