# //WHET·STONE — Marketing Site

Static, self-contained marketing page. Drop the folder anywhere and serve it.

## Files

```
whetstone-site/
├── index.html             entry point
├── page.css               page chrome (nav, sections, theme overrides)
├── tweaks-panel.jsx       in-page Tweaks shell
├── src/                   React components (loaded as Babel JSX)
│   ├── Nav.jsx
│   ├── Hero.jsx           three hero variants
│   ├── Modules.jsx
│   ├── InstallTerminal.jsx
│   ├── CompressionDemo.jsx
│   ├── Numbers.jsx
│   ├── Architecture.jsx
│   ├── Editors.jsx
│   ├── Releases.jsx
│   ├── FAQ.jsx
│   ├── DocsLinks.jsx
│   ├── Footer.jsx
│   └── App.jsx
└── ds/                    design-system assets (CSS, fonts, mark SVGs)
    ├── colors_and_type.css
    ├── whetstone.css
    ├── fonts/             woff2 — Space Grotesk + JetBrains Mono
    └── assets/            mark SVGs
```

## Run locally

Pick any static file server. Examples:

```bash
# python
python3 -m http.server 5173

# node
npx serve .

# bun
bun --port 5173 .
```

Then open <http://localhost:5173>.

> Opening `index.html` directly via `file://` will not work — the fonts and
> JSX modules require an HTTP origin.

## Deploy

Any static host works (Cloudflare Pages, Vercel, Netlify, GitHub Pages, S3 +
CloudFront, etc). Upload the whole folder as-is. There is no build step.

## Theme

- Default follows the visitor's OS color scheme (`prefers-color-scheme`).
- The toggle in the top-right of the nav is a hard override and persists
  to `localStorage` under the key `ws-theme` (`light` | `dark`).
- The OS-preference listener only follows the system theme until the user
  picks a side; after that, the explicit choice wins.

## Tweaks panel

Hidden by default. When the page is opened in an environment that supports
the Tweaks protocol (e.g. embedded in a design tool) a panel appears with:

- Mode (light / dark)
- Hero variant: display headline / big number / architecture-first
- Headline accent: acid / magenta / royal
- Marquee on/off
- Architecture diagram on/off

For a public production deploy you can ignore this panel — visitors won't
see it.

## Production note

JSX is transpiled in-browser via `@babel/standalone`. That's fine for a
small marketing site, but adds ~400 ms to first paint on a cold cache. If
you want a faster cold start, pre-compile `src/*.jsx` with esbuild or vite
and swap the script tags from `type="text/babel"` to plain `text/javascript`.
