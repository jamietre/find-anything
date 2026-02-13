# Claude Code Instructions for find-anything

This file contains project-specific instructions for Claude Code when working on this codebase.

## Planning and Documentation

### Feature Planning

For each substantive new feature:
1. Create a numbered and named plan file in `docs/plans/`
2. Use the naming format: `NNN-feature-name.md` (e.g., `001-pdf-extraction.md`)
3. Include in the plan:
   - Overview of the feature
   - Design decisions and trade-offs
   - Implementation approach
   - Files that will be modified or created
   - Testing strategy
   - Any breaking changes or migration steps

Example plan structure:
```markdown
# Feature Name

## Overview
Brief description of what this feature does and why it's needed.

## Design Decisions
Key architectural choices and their rationale.

## Implementation
Step-by-step approach to implementing the feature.

## Files Changed
- `path/to/file.rs` - what changes
- `path/to/other.rs` - what changes

## Testing
How to test and validate the feature.

## Breaking Changes
Any breaking changes and migration guide if applicable.
```

### Existing Plans

Current plan files are stored in `docs/plans/`:
- `PLAN.md` - Original architecture and implementation plan (now historical)

---

## Project Conventions

(Additional rules will be added here as needed)
