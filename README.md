# Personal academic website — Gustavo Hoffmann Moreira

Built with [Quarto](https://quarto.org), deployed to GitHub Pages.

## Local preview

```sh
quarto preview        # live-reloading local server
quarto render         # build into _site/
```

Quarto is installed at `~/.local/opt/quarto-1.9.38` with a symlink in `~/.local/bin`.
If `quarto` is not found, add to your `~/.zshrc`: `export PATH="$HOME/.local/bin:$PATH"`.

## Publishing (first time)

1. Create a GitHub repository — for the URL `https://USERNAME.github.io`, name it
   `USERNAME.github.io`; any other name gives `https://USERNAME.github.io/REPO`.
2. Push this folder to it (branch `main`).
3. Run once locally: `quarto publish gh-pages` (creates the `gh-pages` branch).
4. In the repo settings → Pages, confirm the source is the `gh-pages` branch.
5. Set `site-url` in `_quarto.yml` to the final URL.

After that, every push to `main` republishes automatically via GitHub Actions
(`.github/workflows/publish.yml`).

## Adding content

- **Working paper / preprint**: copy `research/working-papers/_template.qmd` to a new
  file (no leading underscore), fill in title/abstract, drop the PDF next to it.
  The Research page updates automatically.
- **Publication**: add an entry to `research/publications.yml` (point `path` at the
  DOI or journal page).
- **Other output** (translations, talks): add to `research/other.yml`.
- **Photo**: replace `profile.svg` with a real photo (e.g. `profile.jpg`) and update
  the `image:` field in `index.qmd`.
