# Documentation Policy

## Language Policy

- Public-facing documentation in this repository is **English-primary**.
- Korean documentation is also required for key public guides as companion docs.
- This includes `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, and docs entry pages.
- Companion Korean docs should be kept semantically aligned with English primary docs.

## Metrics Consistency Policy

- Mutable quality metrics (test counts, lint/build pass state) must be maintained only in [STATUS.md](./STATUS.md).
- Other documents must reference `docs/STATUS.md` instead of duplicating live numbers.

## Why

- Prevent contradictory numbers across docs.
- Reduce maintenance overhead.
- Keep onboarding and release communication consistent.

## Companion Docs

- Korean companion: [DOCUMENTATION_POLICY.ko.md](./DOCUMENTATION_POLICY.ko.md)
- Status companion: [STATUS.ko.md](./STATUS.ko.md)
