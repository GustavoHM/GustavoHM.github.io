# Personal academic website — Gustavo Hoffmann Moreira

Multilingual site built with [Quarto](https://quarto.org), deployed to GitHub Pages:
English at `/`, Portuguese at `/pt/`, Japanese at `/ja/` (languages are listed in `_idiomas.toml`). Each
language has its own profile and navbar (`_quarto-<code>.yml`) with links to switch.

## Local preview

```sh
./build.sh                        # render every language profile into _site/
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
  Then add a matching entry in `pt/pesquisa/trabalhos.yml` and `ja/research/working-papers.yml`
  pointing at the paper page.
- **Publication**: add an entry to `research/publications.yml` and
  `pt/pesquisa/publicacoes.yml` / `ja/research/articles.yml` (point `path` at the DOI or journal page).
- **Other output** (translations, talks): `research/other.yml` and
  `pt/pesquisa/outros.yml` / `ja/research/other.yml`.
- **Photo**: replace `profile.svg` with a real photo (e.g. `profile.jpg`) and update
  the `image:` field in `index.qmd` and `pt/index.qmd`.

## Layout

```
_quarto.yml         shared config      build.sh              renders every profile into _site/
_quarto-en.yml      English profile    index/teaching/cv.qmd  English pages
_quarto-pt.yml      Portuguese profile pt/                    Portuguese pages
_quarto-ja.yml      Japanese profile   ja/                    Japanese pages
research/           EN research page + working-papers/ + publication lists
pt/pesquisa/        PT research page + translated lists
ja/research/        JA research page + translated lists
```

## Dashboard (`painel/`)

Native macOS app (Rust + egui) for editing the site in every language listed in
`_idiomas.toml`. Build and install it into `~/Applications/Painel do Site.app` with:

```sh
painel/criar-app.sh
```

- **Postagens**: per-language posts (each language with a `postagens` folder).
  Saving/autosave keeps `draft: true` on the computer; **Publicar no site** sets
  `draft: false`, test-renders, commits only that post's folder and pushes `main`.
- **Páginas**: About, Research intro, posts listing page, Teaching, CV — any two
  languages side by side. Only `title` and the text are edited; `:::{#id}` listing
  slots are protected.
- **Pesquisa**: form editor for the research lists, one file per language, items
  matched by position; date and link are shared, title/authors/description per language.
- **Papers**: working-paper pages (root language) with their entry in every other
  language's `lista_papers` kept in sync; PDFs go in `working-papers/files/<slug>/`.
- **Site › Menu**: navbar items of every language in one table — labels per language,
  reorder, show/hide. **Site › Configurações**: site title and description, footer per
  language, profile photo and the About-page links (icon and URL shared, text per language).
- **Nova página / Remover página**: creates a page in every language (`<slug>.qmd`,
  `<code>/<slug>.qmd`), registers it in `_idiomas.toml` and adds it to the menu;
  removal undoes all three and sends it. About, Research and Posts are protected.
- **Several sites**: the site picker at the top keeps a list of Quarto sites
  (`~/Library/Application Support/Painel do Site/sites.txt`). A site without
  `_idiomas.toml` gets a starter one built from its pages; single-language sites without
  `_quarto-<code>.yml` profiles use `_quarto.yml` for the menu and footer.
- **Línguas…**: adds a language by copying the structure of an existing one
  (e.g. `pt/` → `ja/`), creating `_quarto-<code>.yml`, adding the language link to
  every navbar and registering it in `_idiomas.toml`. Nothing is published until
  you click **Enviar configuração ao site**.

Every send commits only the files of what is open. Changes to `painel/` alone don't
trigger a site rebuild (`paths-ignore` in the workflow). Tests (`cd painel && cargo test`)
run against a throwaway copy of the site with a local git remote.
