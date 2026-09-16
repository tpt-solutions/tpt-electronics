# tpt-elec-battery-bms

> SOC estimation: coulomb counting, EKF over [SOC, V_rc], and sensor-fusion correction

Part of [tpt-electronics](https://github.com/tpt-solutions/tpt-electronics) — a fully open-source, MIT-licensed multiphysics simulation engine.

`bms` · `soc` · `kalman-filter` · `coulomb-counting` · `state-estimation`

## Status

✅ Stable · Part of the v0.1.0 release

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
tpt-elec-battery-bms = "0.1.0"
```

See the crate-level documentation for a tested quick-start example.

## Testing

```console
$ cargo test -p tpt-elec-battery-bms
```

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.

Contributions require [DCO sign-off](../../CONTRIBUTING.md) (no CLA).
