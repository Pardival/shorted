## Ticket

Closes #<issue> — <title>

## What changed

<!-- Short description. Screenshots / screen recordings for UI changes. -->

## Acceptance criteria

<!-- Copy the ticket's criteria and show how each is met (test name, recording, measurement). -->
- [ ] …

## Definition of done

- [ ] Tests added at the right level; CI green
- [ ] No new clippy / analyze warnings; formatted
- [ ] Strings in ARB (en + fr/es/pt)
- [ ] Performance budget of the touched area not regressed (numbers below if relevant)
- [ ] Docs / ADR updated if behaviour, schema or architecture changed

## Security checklist

- [ ] New input surface → limits, validation, fuzz target if it parses bytes
- [ ] New dependency → licence compatible, maintained, cargo-deny passes
- [ ] New network call → HTTPS, timeout
- [ ] No secrets or user content can reach logs

## Measurements

<!-- Device / platform, fps, frame times, export time… -->
