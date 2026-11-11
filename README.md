# ste-lint

`ste-lint` is a fast command-line prose linter inspired by
[Simplified Technical English](https://en.wikipedia.org/wiki/Simplified_Technical_English).
It helps technical writers and LLM-assisted workflows find text that is harder
to read than necessary.

This project helps authors. It does not check compliance with ASD-STE100. Full
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

Every warning has a high, medium, or low confidence level. The text output shows the level for `medium` and `low` warnings, and
the JSON output has a `confidence` field.

| Code | Confidence |
| --- | --- |
| `STE001`, `STE002`, `STE006` | High |
| `STE003` | Depends on the actor and the mode |
| `STE004`, `STE007` | Medium |
| `STE005` | Medium, or low when a bare noun follows a copula |

For `STE003`, the level is high when a `by` phrase names the actor. Without
an actor, the level is medium in procedure mode and low in description mode. ASD-STE100 permits passive voice in descriptive text when
the reader does not need to know the actor. Use `--min-confidence medium` to hide the
low-confidence warnings.

Sentence checks operate on sentences, not physical lines. Wrapped prose stays
in one sentence. Common abbreviations such as `e.g.` do not end a sentence,
and closing quotes or parentheses can follow sentence punctuation. The word
count treats a dotted token, for example `CONTRIBUTING.md` or `docs.rs`, as
one word.
Markdown headings and list items establish structural boundaries.

Grammar checks skip technical tokens. Tools and conventions, not prose rules,
control the spelling of filenames, paths, URLs, imports, and identifiers. Thus, they
skip `src/logging/mod.rs`, `logging::init`, `logging_enabled`,
`@scope/logging`, and `build-logging.sh`, although each one has an `-ing` form.

The `-ing` check reports a word only when a closed-class word puts it in a verb
position. It reports `logging` in `is logging events`, `by logging the events`,
and `Testing the valve takes time.`, and it ignores the same spelling in a noun
position such as `the mapping table` or `a string is required`. A preposition
followed by an `-ing` word without a clear object (`by logging`) is ambiguous,
so the check leaves it alone. A copula followed by an `-ing` word and a bare
noun, as in `reads are blocking operations`, is also ambiguous in English. The
check reports that form for review with low confidence.

The passive check (`STE003`) looks for a form of `be` and a past participle.
Only adverbs and a few other words, for example `not`, `then`, and `all`, can
come between them. Thus, the check reports `is not used` and `is then
returned`, but not `is a hidden field` or `is the token`.

The check knows common irregular participles such as `set`, `built`, `run`,
`made`, and `written`. It skips words that end in `-en` but are not participles, such as `open`,
`even`, and `between`. It also skips forms that usually describe a state,
such as `deprecated`, `unchanged`, and `based`. The complex verb check
(`STE004`) uses the same participle test. It does not report a participle
that is also a noun, as in `has read access`, unless an object follows it,
as in `has read the file`.

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

Use `ste-lint --help` for all limits and options. The exit status is `0` when
the linter finds no warnings, `1` when it finds warnings, and `2` for an
operational or configuration error.

## Select Rules

Use `--select` to run only some rules, and `--ignore` to skip rules:

```console
ste-lint docs/ --ignore STE007
ste-lint docs/ --select STE001,STE003 --min-confidence medium
```

### Inline Directives

Use HTML comments to turn off rules for part of a file. A directive without
codes applies to all rules.

```markdown
<!-- ste-lint-disable-next-line STE003 -->
The gasket is replaced by the technician.

<!-- ste-lint-disable STE001 STE003 -->
Text that a license or a legal team controls.
<!-- ste-lint-enable STE001 STE003 -->
```

`ste-lint-disable` applies until the end of the file or until a matching
`ste-lint-enable`. An `ste-lint-enable` without codes ends all open regions.
An `ste-lint-enable` with codes ends only a region that has the same codes.
Directives also work in plain-text files. A directive in a code fence or a
code span has no effect, so you can show directives in examples.

### Config File

The linter reads `ste-lint.toml` from the current directory, or from the
nearest parent directory that has one. Use `--config <path>` to name a file, or
`--no-config` to ignore config files. Command-line options override the file.
`--select` replaces the `select` value from the file, and `--ignore` adds to
the `ignore` value.

```toml
mode = "description"
max-sentence-words = 25
max-paragraph-sentences = 6
select = ["ALL"]
ignore = ["STE007"]
min-confidence = "low"

[per-path-ignores]
"CHANGELOG.md" = ["STE001", "STE002"]
"legal/**" = ["ALL"]
```

Globs in `per-path-ignores` are relative to the directory of the config file.
A glob without a `/` matches at any depth, as in `.gitignore`.

## Supported Files

The default extensions are `md`, `markdown`, `mdown`, `mkdn`, `txt`, `rst`,
`adoc`, and `asciidoc`. Use `--extensions` to replace this list. The linter does
not follow symbolic links. By default, it obeys Git ignore rules and skips
hidden files.

In Markdown files, these parts are not prose, and the checks skip them:

- Code fences and inline code
- Link destinations, link reference definitions, and reference labels
- Images and badges
- HTML comments, HTML tags, autolinks, and HTML entities such as `&mdash;`
- YAML and TOML front matter
- Table delimiter rows

Each table cell is a separate paragraph, so a row is not one long sentence.

The semicolon check (`STE007`) reports only a semicolon between two words,
with a space after it. It does not report `TL;DR`, HTML entities, or a
semicolon at the end of a list item or a line of code. The phrase check does
not report `a number of` before a unit, as in `a number of seconds`.

## Accuracy

`tests/corpus` holds labeled sample documents. Each `.expected` file lists
the true positives. A test compares the linter output with these labels and
fails if the precision or recall of a rule is less than 0.85. Run this
command to see the table:

```console
cargo test corpus -- --nocapture
```

When you change a rule, add an example to the corpus. The corpus also
records known limits. For example, the linter reports `is closed` as passive
when it describes a state, and it does not detect `get` passives.

## References

- [ASD-STE100 official site](https://asd-ste100.org/)
- [Simplified Technical English overview](https://en.wikipedia.org/wiki/Simplified_Technical_English)
