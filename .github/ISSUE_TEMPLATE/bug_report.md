---
name: Bug report
about: Report something that simulates incorrectly or crashes
title: "[bug] "
labels: bug
assignees: ""
---

**Affected crate(s)** (e.g. `tpt-elec-thermal` 0.1.0):

**Description**
A clear description of what went wrong. If results are numerically wrong,
include the expected physical behavior.

**Minimal reproduction**

```rust
// smallest code that shows the problem
```

**Expected vs actual**

| Quantity | Expected | Actual |
|---|---|---|
| e.g. max temp | 85.0 °C | 120.3 °C |

**Environment**
- OS:
- Rust version (`rustc -V`):

**Additional context**
Reference standard / analytic solution used to validate (e.g. JEDEC JESD51,
IPC-2152 chart point), if applicable.
