# API documentation

`nash docs [PATH]` compiles a project and generates HTML API documentation in
`docs/`. Package projects document their configured exposed modules; applications
document all their modules. Dependencies are checked but not included in output.
Compilation errors stop generation. Missing declaration comments and invalid
`@docs` references produce Nash warnings without suppressing documentation.

```sh
nash docs --out target/api
nash docs --format markdown --out target/api-markdown
nash docs --base --out target/base-docs
```

`--base` documents the source bundled with the compiler, plus the separate
`Builtin` and `Primitive` catalogs. It needs no project or Base installation.
A site uses one path per module, such as `Cardano/Tx.html`; projects with duplicate
module names must be documented separately.

Module overviews and declaration comments use `{-| ... -}`. An overview can place
public declarations between prose sections with `@docs name, otherName`.
Remaining declarations appear in source order. Private declarations and hidden
constructors are omitted. Signatures come from solved types; type declarations
also show their inferred kinds. Macro declarations are not yet supported by the
source parser, so there are no generated macro entries.

HTML includes local CSS, a module index, and a JSON search index. Search matches
names, modules, signatures and summaries. Serve the output over HTTP for search;
module pages and navigation also work when opened directly. Markdown comments
support tables, links and fenced code blocks. Nash code blocks use Nash's literal
parsers and keyword/operator catalogs for highlighting. Raw HTML is displayed as
text rather than executed.

The CLI performs asynchronous filesystem writes with Tokio. Extraction and
rendering have library unit snapshots whose descriptions contain Nash input.
No tests invoke the CLI binary.

`.github/workflows/docs.yml` generates a downloadable site on pull requests and
publishes Base documentation through GitHub Pages on `main`. The repository's
Pages source must be configured as **GitHub Actions**. Local generation does not
publish anything.
