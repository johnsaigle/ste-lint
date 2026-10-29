# ste-lint

`ste-lint` is a fast, bounded command-line prose linter inspired by
[Simplified Technical English](https://en.wikipedia.org/wiki/Simplified_Technical_English).
It helps technical writers and LLM-assisted workflows find text that is harder
to read than necessary.

This project is an authoring aid, not an ASD-STE100 compliance checker. Full
compliance requires the official standard, its controlled dictionary, domain
knowledge, and human judgment.

## Checks

| Code | Check |
| --- | --- |
| `STE001` | Sentence exceeds 20 words in procedure mode or 25 in description mode |
| `STE002` | Paragraph has more than six sentences |
| `STE003` | Possible passive voice |
| `STE004` | Possible complex verb construction |
| `STE005` | `-ing` form that needs review |
| `STE006` | Imprecise, wordy, or common LLM-associated phrase |
| `STE007` | Semicolon that can combine instructions or topics |

The grammar checks are intentionally conservative heuristics. Treat each
warning as a prompt for review, not proof that the text is incorrect.

Sentence checks operate on sentences, not physical lines. Wrapped prose stays
in one sentence. Common abbreviations such as `e.g.` do not end a sentence,
and closing quotes or parentheses can follow sentence punctuation. Dotted
technical tokens such as `CONTRIBUTING.md` and `docs.rs` count as one word.
Markdown headings and list items establish structural boundaries.

Grammar checks skip technical tokens. Filenames, paths, URLs, imports, and
identifiers are written by tools and conventions rather than as prose, so
`src/logging/mod.rs`, `logging::init`, `logging_enabled`, `@scope/logging`, and
`build-logging.sh` are not reported even though they contain an `-ing` form.

The `-ing` check reports a word only when a closed-class word puts it in a verb
position. It reports `logging` in `is logging events`, `by logging the events`,
and `Testing the valve takes time.`, and it ignores the same spelling in a noun
position such as `the mapping table` or `a string is required`. A preposition
followed by an `-ing` word without a clear object (`by logging`) is ambiguous,
so the check leaves it alone. A copula followed by an `-ing` word and a bare
noun, as in `reads are blocking operations`, is also ambiguous in English, so
that form is reported for review.

## Install

```console
cargo install --path .
```

## Use

Lint supported prose files below the current directory:

```console
ste-lint .
```

Lint procedural text, where sentences have a 20-word limit:

```console
ste-lint docs/ --mode procedure
```

Read standard input and emit JSON:

```console
printf 'The valve has been opened.\n' | ste-lint - --format json
```

Text diagnostics include the source line below each warning:

```text
! guide.md
  Warning line 4 [STE003]: possible passive voice
  > The valve is opened by the system.
    help: Name the actor and use active voice when the actor is known.

=======================================
! Found 1 warning(s) in 1 file(s)
```

Use `ste-lint --help` for all limits and options. Exit status is `0` when no
warnings are found, `1` when warnings are found, and `2` for an operational or
configuration error.

## Supported Files

The default extensions are `md`, `markdown`, `mdown`, `mkdn`, `txt`, `rst`,
`adoc`, and `asciidoc`. Use `--extensions` to replace this list. Markdown code
fences, inline code, and link destinations are excluded from prose checks.

## Resource Bounds

All inputs are bounded. Defaults limit traversal to 10,000 files and 32 levels,
each file to 2 MiB, output to 1,000 diagnostics, and worker threads to at most
16. Processing occurs in fixed-size batches. The product of the worker count and
maximum file size cannot exceed 128 MiB. The CLI also enforces hard ceilings.
Symbolic links are not followed. Git ignore rules and hidden-file filtering are
enabled by default.

The implementation uses a bounded Rayon pool rather than an async runtime.
Local filesystem reads are blocking operations, and bounded data parallelism is
simpler and more appropriate for processing independent files.

## References

- [ASD-STE100 official site](https://asd-ste100.org/)
- [Simplified Technical English overview](https://en.wikipedia.org/wiki/Simplified_Technical_English)
