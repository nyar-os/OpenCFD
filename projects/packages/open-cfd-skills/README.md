# @wae/open-cfd-skills

Agent skill documents for OpenCFD. The package ships text only; it contains no
solver, meshing or export code.

## When to use

Load `open-cfd` when a task needs a two-dimensional incompressible case:
creating or editing a `.ocfd` document, running a cavity or channel, adding
obstacles, porous regions or probes, configuring boundaries, scalar, thermal,
buoyancy or turbulence settings, or exporting CSV, VTK, SVG or histories.

## Invocation

Point your agent at the installed package and give it the task:

```text
Read the open-cfd skill, then run a 24×24 lid-driven cavity at Re=100 and
report the maximum velocity from the solve report.
```

The skill tells the agent to run `open-cfd capabilities` first, treat the
capability matrix as the only support claim, and report the exact command and
export path used.

## Contents

- [`skills/open-cfd/SKILL.md`](skills/open-cfd/SKILL.md) — routes, preflight,
  solver and export limits, hard rules.

See the [product README](../../README.md) for what OpenCFD supports and
[documentation/capabilities.md](../../documentation/capabilities.md) for the
capability matrix.
