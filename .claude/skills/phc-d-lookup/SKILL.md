---
name: phc-d-lookup
description: Look up a PHC design decision by D-### identifier. Returns the locked spec text, status, date, and any cross-references / amendments. Faster than re-reading spec/design-decisions.md. Invoke with `/phc-d-lookup <D-###>`.
---

# PHC Design Decision Lookup

## Input

`$ARGUMENTS` is a D-### identifier (e.g. `D-027`, `D-005a`). If missing, ask.

## Steps

1. `grep -n "^### $ARGUMENTS " spec/design-decisions.md` — locate the entry. If 0 matches: report "not found", suggest `grep "^### D-" spec/design-decisions.md` for full list.

2. Read the entry block (from matched line through the next `^### D-` or `^---` marker).

3. Also grep for cross-references:
   - `grep -n "$ARGUMENTS" spec/design-decisions.md` — every mention (amendments, subsumes, see-also).
   - `grep -rn "$ARGUMENTS" spec/ CLAUDE.md AGENTS.md` — references outside design-decisions.

4. Report in this shape:

```
## $ARGUMENTS — <title>

**Status**: <locked|provisional> (Date: YYYY-MM-DD)
**Decision**: <one-line summary>

<full block text>

### Cross-refs
- spec/design-decisions.md:<line> — <context snippet>
- CLAUDE.md:<line> — <context snippet>
- ...
```

5. If `Status: provisional` or `Amended by D-NNNa`: highlight at top.

## Rules

- Quote the spec exactly. Do not paraphrase the **Decision** line.
- Do not invent cross-refs. Only list what `grep` returned.
- If user asks "what does D-### say about X" and X is not in the entry: say "not in D-###, try `/phc-d-lookup <other>`" instead of guessing.
