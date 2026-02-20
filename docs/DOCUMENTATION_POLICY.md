# Documentation Policy

## Language Policy

- Public-facing documentation in this repository must be written in English.
- This includes `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, and docs entry pages.
- Legacy non-English content should be translated when touched.

## Metrics Consistency Policy

- Mutable quality metrics (test counts, lint/build pass state) must be maintained only in [STATUS.md](./STATUS.md).
- Other documents must reference `docs/STATUS.md` instead of duplicating live numbers.

## Why

- Prevent contradictory numbers across docs.
- Reduce maintenance overhead.
- Keep onboarding and release communication consistent.
