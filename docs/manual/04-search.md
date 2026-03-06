# Search

[← Manual home](README.md)

---

## Search modes

The search mode controls how your query is interpreted. Select it from the dropdown in the web UI, or pass `--mode` on the CLI.

| Mode | What it does | Use when |
|---|---|---|
| **Fuzzy** | Ranks results by relevance; tolerates partial matches and minor variations | General-purpose searching |
| **Document** | Full-text match across whole documents; better recall for prose content | Searching notes, articles, long docs |
| **Exact** | Literal substring match (case-insensitive) | Finding a specific function name, error string, etc. |
| **Regex** | Regular expression match | Pattern-based searches; query passed through unchanged |

**Fuzzy** mode uses FTS5 with BM25 ranking and a fuzzy prefix step. It matches words that start with each of your query terms, so `artifac` will match `artifactory`. It is the best default for most searches.

**Document** mode treats all terms as required, weighted by their position in the document. It works better than fuzzy for longer prose queries where you want all terms to appear somewhere in the file.

**Exact** mode performs a case-insensitive literal search. Useful for finding specific identifiers, error messages, or any string that must appear verbatim.

**Regex** mode passes your query directly to the FTS5 regex engine. No stop-word stripping or date extraction is applied. Use standard RE2 syntax.

---

## Natural language date queries

You can embed date constraints directly in your search query using plain English. The date phrase is recognized, extracted from the query, and converted to a date range filter — you don't need to touch the Advanced filters panel at all.

**Examples:**

```
artifactory last month
report in the last two days
contracts between january and february
notes in january 2025
config files last week
```

The detected date phrase is highlighted in green in the search box, and a chip appears below the search bar showing the active date range. Click ✕ on the chip to remove the date filter without changing your search terms.

### How phrases are interpreted

The interpretation of "last X" depends on whether it is preceded by a rolling-window prefix ("in the", "within the"):

| Phrase | Interpretation | Result |
|---|---|---|
| `last month` | Calendar month | Feb 1 – Feb 28 |
| `in the last month` | Rolling window | 30 days ago → now |
| `last year` | Calendar year | Jan 1, 2025 – Dec 31, 2025 |
| `in the last year` | Rolling window | 1 year ago → now |
| `last week` | Calendar Mon–Sun | Feb 23 – Mar 1 |
| `in the last week` | Rolling window | 7 days ago → now |
| `last weekend` | Calendar Sat+Sun | Feb 28 – Mar 1 |
| `yesterday` | Complete day | Mar 5 00:00 – Mar 5 23:59 |
| `last Monday` | Complete day | Mar 2 00:00 – Mar 2 23:59 |
| `last two days` | Rolling window | 2 days ago → now |
| `last 6 months` | Rolling window | 6 months ago → now |
| `in january` | Named calendar month | Jan 1 – Jan 31 |
| `in january 2025` | Named month + year | Jan 1, 2025 – Jan 31, 2025 |

### Range syntax

```
report between january and february
contracts from last monday to friday
notes jan to march
```

Two date expressions connected by "and", "to", or "−" are treated as an explicit range.

### Direction modifiers

```
changes since last monday    → from last Monday until now
files after march 1          → from March 1 until now
documents before christmas   → until Dec 25
```

### Stop word stripping

In **Fuzzy** and **Document** modes, common articles and conjunctions (`a`, `an`, `the`, `and`) are stripped from the query before it is sent to the search engine. This is invisible — your original query text is preserved in the search box.

```
"the quick brown fox"  →  sends: "quick brown fox"
"artifactory and token last week"  →  sends: "artifactory token" + date filter
```

Words inside double quotes are **always preserved verbatim** and are never stripped:
```
"the quick brown" fox  →  sends: "the quick brown" fox  (quoted part untouched)
```

The operators `or` and `not` are also never stripped (they are meaningful FTS5 boolean operators in Exact and Document modes).

Stop word stripping does **not** apply in Regex or Exact mode.

---

## Date range quick reference

| You type | Date from | Date to |
|---|---|---|
| `last week` | Monday of prior week | Sunday of prior week |
| `last month` | 1st of prior month | Last day of prior month |
| `last year` | Jan 1 of prior year | Dec 31 of prior year |
| `last weekend` | Saturday of prior weekend | Sunday of prior weekend |
| `yesterday` | Yesterday 00:00 | Yesterday 23:59 |
| `last Monday` | Last Monday 00:00 | Last Monday 23:59 |
| `last two days` | 48 hours ago | Now |
| `last 6 months` | 6 months ago | Now |
| `in the last week` | 7 days ago | Now |
| `in the last month` | 30 days ago | Now |
| `in the last year` | 365 days ago | Now |
| `in january` | Jan 1 (current year) | Jan 31 |
| `in january 2025` | Jan 1, 2025 | Jan 31, 2025 |
| `since last monday` | Last Monday | Now |
| `before march` | — | Feb 28 (end of month before March) |

---

## Advanced filters

The **Advanced search** panel (accessible from the search bar area) lets you filter by:

- **Sources** — restrict results to one or more named sources
- **Date from / Date to** — manually set a date range using date pickers

Manual date filters always take precedence over natural language date phrases. When both are active simultaneously, the NLP chip is shown with a strikethrough and a red `!` icon — hover over `!` to see an explanation. Clear the manual date range (or remove the date phrase from your query) to resolve the conflict.

The result count line updates to reflect the active filter:
```
390 results between 2/1/2026 and 2/28/2026
200 results after 9/1/2025
```

---

## Filtering by source

If you have multiple machines indexed, each appears as a separate source in the Advanced panel. You can select one or more sources to restrict your search.

When a source is selected in the file tree sidebar, it is also highlighted in the results.

---

## CLI search

```sh
find-anything <PATTERN> [OPTIONS]
```

| Option | Description |
|---|---|
| `--mode <MODE>` | `fuzzy` (default), `exact`, `document`, `regex` |
| `--source <NAME>` | Restrict to this source (repeatable) |
| `--limit <N>` | Maximum results (default: 50) |
| `--offset <N>` | Skip first N results (for pagination) |
| `-C, --context <N>` | Lines of context around each match |
| `--no-color` | Disable ANSI colour output |
| `--config <PATH>` | Client config file |

**Examples:**

```sh
# Basic fuzzy search
find-anything "password strength"

# Exact match with context
find-anything --mode exact -C 2 "fn process_file"

# Restrict to a source
find-anything --source code --mode regex "TODO|FIXME"

# Paginate
find-anything --limit 20 --offset 40 terraform
```

Output format:
```
[kind] path/to/file.ext:line_number   matched line content
```

---

[← Indexing](03-indexing.md) | [Next: Web UI →](05-web-ui.md)
