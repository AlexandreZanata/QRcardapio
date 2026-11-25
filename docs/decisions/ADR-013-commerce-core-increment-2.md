# ADR-013 — Commerce Core Increment 2

- **Status** Accepted
- **Date** 2026-11-25

## Service calendar

Service settings default to `America/Sao_Paulo`. The dashboard supports five embedded time zones covering Brazilian UTC offsets, including separate São Paulo and Cuiabá rules. Weekly windows may cross midnight and retain the date they started. A date exception replaces the recurring schedule for that local date and overrides any spillover from the day before. An empty exception closes the date. Service evaluation returns the opening shift date and a separate business date with the 04:00 cutoff specified by QR Delivery. Increment 3 will pass these results into checkout. Existing orders keep their recorded dates and numbers.

## Customer privacy

The owner can export or erase a customer while owners and managers can manage the profile and addresses. Erasure anonymizes the customer and retained delivery details, deletes addresses, and unlinks retained financial records without changing totals, items or events. Inactive customer retention policy stays open. The checkout increment will attach new orders to customer records and delivery details. QR Delivery will add zone association to addresses.

## Public demonstration

The highlighted landing action and the existing QR share the same destination. The demo's ingredients and extras belong to the bundled fictional catalog and render as optional read-only details. Dashboard configuration is scoped to each tenant's draft.
