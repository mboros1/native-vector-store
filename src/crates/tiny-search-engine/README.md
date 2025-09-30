Tiny Search Engine

Small utilities to build and prepare local search data bundles.

Included tools

- `vsx-scrape`: Scrape Open VSX (VS Code extension marketplace mirror) and emit a JSON dump of extensions suitable for
  client-side indexing.

Quick start

- Everything:
  `cargo run -p tiny-search-engine --bin vsx-scrape --release -- --all --query "*" --page-size 100 --concurrency 6 --pretty > vsx.json`
- Paged sample:
  `cargo run -p tiny-search-engine --bin vsx-scrape --release -- --pages 3 --page-size 100 --concurrency 4 --pretty > vsx.json`

Flags

- `--pages` number of search pages to pull (default 3)
- `--page-size` results per page (default 100)
- `--concurrency` concurrent detail requests per page (default 4)
- `--all` keep paging until no results (ignores `--pages`)
- `--delay-ms` delay between requests in milliseconds (default 0)
- `--output` optional file path to write JSON (defaults to stdout)
- `--pretty` pretty-print JSON

The `--query` flag controls the search term (defaults to `*` for breadth).
