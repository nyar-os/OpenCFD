# @wae/open-cfd-skills

Agent skills for OpenCFD. The package ships skill documents only; it is not a
runtime library and contains no meshing, solver or export algorithms.

## Status

`skills/open-cfd/SKILL.md` records the routing and evidence rules for create,
edit, validate, solve and export against the implemented session core. The CLI
and SDK behind it create and reopen a `.ocfd` case, set mesh, fluid, time,
boundary, obstacle, porous region, scalar, turbulence, wall function and thermal
parameters, validate the case, solve it and export CSV, VTK or SVG. Turbulence is
limited to the constant and mixing-length eddy-viscosity models; compressible
flow, three-dimensional flow, coupled heat transfer, adaptive meshing and
external format import are not implemented and report
`capability/not-implemented`.

## Layout

```text
packages/open-cfd-skills/
├── package.json
├── README.md
└── skills/
    └── open-cfd/
        └── SKILL.md
```

The skill must never be shipped claiming methods, commands or flags that the
installed SDK/CLI does not expose. The capability matrix remains the only support
claim; unsupported features reject with `capability/not-implemented`.
