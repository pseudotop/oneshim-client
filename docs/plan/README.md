[English](./README.md) | [한국어](./README.ko.md)

# Plan Index

This directory stores dated implementation plans and execution tracking notes.

## Naming Convention

- `YYYY-MM-DD-<topic>.md`
- `YYYY-MM-DD-<topic>.ko.md` for key plans that need Korean companion docs.

## Status Policy

- `Draft`: proposal under review
- `Active`: currently used as implementation baseline
- `Done`: completed without follow-up plan
- `Superseded`: replaced by a newer dated plan

## Active Plans

| Date | Status | Plan |
| --- | --- | --- |
| 2026-02-25 | Active | [ADR-002 GUI V2 Implementation Plan](./2026-02-25-adr-002-gui-v2-implementation-plan.md) |
| 2026-02-25 | Active | [ADR-002 Phase3 Delivery Plan](./2026-02-25-adr-002-phase3-delivery-plan.md) |

## Usage Rule

1. Create a new dated plan file when scope or execution strategy changes materially.
2. Update this index in the same commit as the plan change.
3. Mark older plans `Superseded` instead of deleting unless migration policy requires archival move.
