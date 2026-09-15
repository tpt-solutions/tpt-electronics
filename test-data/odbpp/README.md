# ODB++ fixture

`sample.zip` is a stored (uncompressed) ZIP with the classic ODB++
directory layout:

- `matrix/matrix` — layer list (NAME/TYPE rows)
- `steps/pcb/layers/<layer>/features` — feature lines:
  - `L x1 y1 x2 y2 width` — line (ODB units = µm / 10 in this fixture)
  - `P x y symbol` — pad
  - `A cx cy r start sweep width` — arc
- `steps/pcb/layers/drill/features` — `T x y diameter` tool hits

The reader in `tpt-elec-odbpp` accepts stored ZIP members only (a clear
error names compressed entries); this keeps the engine dependency-free.
