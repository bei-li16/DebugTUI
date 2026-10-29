# R52 GIC audited-baseline regression fixture

`r52-gic-audited-baseline.json` freezes the four GIC fragments from published
commit `bbdb12d39fdf91c92aa0a58b4e33bdd634d72efd` (v1.2.4). It checks all 1,519
views and 17,752 field layouts, permissions, offsets, resets, conditions and
portable declaration origins without depending on the contents of `artifacts/`.
Repeated field layouts are stored once; column names are in the JSON metadata.

The old runtime test loaded an ignored historical audit report which is absent
from a clean checkout and was removed by the requested build/test retention
cleanup. This fixture is an explicit published-baseline regression snapshot,
not a reconstruction of that independently authored report and not a new
primary-source or hardware verification. Original source-review records remain
in `reference/system-registers-reaudit-r52-f429.json` (R52-161). Document and
baseline fragment SHA256 values are recorded in the fixture.

Independent capacity formulas, owner/route guards and raw-value decoding tests
remain in `register_reaudit_runtime.rs`. Do not regenerate this fixture from the
working catalogue merely to accept a change: review the primary evidence and
record any intentional baseline revision explicitly.
