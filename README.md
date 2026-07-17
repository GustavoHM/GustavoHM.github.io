# Personal academic website — Gustavo Hoffmann Moreira

Bilingual site built with [Quarto](https://quarto.org), deployed to GitHub Pages:
English at `/`, Portuguese at `/pt/`. Each language has its own navbar
(configured in `_quarto-en.yml` / `_quarto-pt.yml`) with a link to switch.

## Local preview

```sh
./build.sh                        # render both languages into _site/
python3 -m http.server 4322 --directory _site   # browse the full bilingual site

quarto preview                    # live-reload preview (English version only)
```

Quarto is installed at `~/.local/opt/quarto-1.9.38` with a symlink in `~/.local/bin`.
If `quarto` is not found, add to your `~/.zshrc`: `export PATH="$HOME/.local/bin:$PATH"`.

## Publishing (first time)

1. Create a GitHub repository — for the URL `https://USERNAME.github.io`, name it
   `USERNAME.github.io`; any other name gives `https://USERNAME.github.io/REPO`.
2. Push this folder to it (branch `main`).
3. In the repo: Settings → Pages → Build and deployment → Source: **GitHub Actions**.
4. Set `site-url` in `_quarto.yml` to the final URL.

Every push to `main` builds both languages and deploys automatically
(`.github/workflows/publish.yml`).

## Adding content

Most content changes need a small edit in both languages:

- **Working paper / preprint**: copy `research/working-papers/_template.qmd` to a new
  file (no leading underscore), fill in title/abstract, drop the PDF next to it.
  Then add a matching entry in `pt/pesquisa/trabalhos.yml` pointing at the paper page.
- **Publication**: add an entry to `research/publications.yml` and
  `pt/pesquisa/publicacoes.yml` (point `path` at the DOI or journal page).
- **Other output** (translations, talks): `research/other.yml` and
  `pt/pesquisa/outros.yml`.
- **Photo**: replace `profile.svg` with a real photo (e.g. `profile.jpg`) and update
  the `image:` field in `index.qmd` and `pt/index.qmd`.

## Layout

```
_quarto.yml         shared config      build.sh              renders EN + PT into _site/
_quarto-en.yml      English profile    index/teaching/cv.qmd  English pages
_quarto-pt.yml      Portuguese profile pt/                    Portuguese pages
research/           EN research page + working-papers/ + publication lists
pt/pesquisa/        PT research page + translated lists
```
