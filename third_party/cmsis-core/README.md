# Pinned CMSIS-Core input

These unmodified headers and LICENSE are from ARM-software/CMSIS_6 commit
`b0bbb0423b278ca632cfe1474eb227961d835fd2` (6.1.0).
Header paths upstream are `CMSIS/Core/Include/core_cm3.h`, `core_cm4.h`, and
`core_cm7.h`. The root LICENSE is Apache-2.0. Original copyright and SPDX notices
remain in each header. `source-lock.json` pins every input byte by SHA256.

`python scripts/generate-register-catalogues.py --check` verifies the input
hashes and compares offline generation with the shipped TOML. It does not fetch
upstream, execute header code, connect a probe, or infer reset values. Update the
lock and review generated diffs together when deliberately upgrading CMSIS.

CMSIS supplies addresses, masks, and declared access metadata. Architecture
rules absent from headers have separate manual citations in the generator;
header qualifiers alone cannot prove read side effects or hardware presence.
