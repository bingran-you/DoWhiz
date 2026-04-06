# DoWhiz Website

<p align="center">
  <img src="public/assets/DoWhiz.svg" alt="Do icon" width="96" />
</p>

Oliver-first product shell for DoWhiz, built with Vite + React.
The web app handles the public landing story, auth onboarding, Oliver setup dashboard, and supporting internal analytics.

## Prerequisites
- Node.js 18+ (20+ recommended).
- npm (ships with Node).

## Quick start (from repo root)
```
cd website
npm install
npm run dev
```

Open the local URL shown in the terminal (defaults to http://localhost:5173).

## Common commands (from website/)
- Dev server: `npm run dev`
- Lint: `npm run lint`
- Production build: `npm run build`
- Preview production build: `npm run preview`
- Responsive audit: `npm run test:responsive`
- SEO artifact generation: `npm run seo:build`
- SEO keyword report: `npm run seo:report`
- New SEO blog scaffold: `npm run seo:new-blog -- --slug my-post --headline "My headline" --description "Meta description" --owner Oliver`
- SEO crawl report: `npm run seo:crawl`

Build output goes to `website/dist/`.
SEO crawl reports are written to `website/reports/` as dated `.md` + `.json` files.
The SEO keyword report can run with Search Console data only, but keyword volume is still recommended for prioritization.

## VM Deployment Workflow

If you host the website on a VM (instead of Vercel), build the static assets and serve `website/dist` from Nginx.

```bash
cd website
npm install
npm run build
```

Nginx example:
```nginx
server {
    listen 80;
    server_name dowhiz.com www.dowhiz.com;

    return 301 https://dowhiz.com$request_uri;
}

server {
    listen 443 ssl;
    server_name dowhiz.com www.dowhiz.com;

    # SSL config omitted; use certbot or your provider's recommendations.
    root /home/azureuser/DoWhiz/website/dist;
    try_files $uri /index.html;
}
```

If you are using a dedicated API subdomain (example: `api.dowhiz.com`) for the Rust service, you can keep the website hosted on Vercel and only run the API on the VM.

## Project structure
- `website/src/app/`: app-level router and shared clients.
- `website/src/pages/`: page-level route components (`LandingPage`, `StartupIntakePage`, `WorkspaceHomePage`, internal dashboard wrapper).
- `website/src/domain/`: canonical startup workspace models (`workspaceBlueprint`, `resourceModel`, `workspaceHomeModel`, provider runtime overlay logic).
- `website/src/components/`: reusable UI blocks for landing/workspace sections.
- `website/src/styles/`: modular style layers (`tokens`, `base`, `landing`, `layout`, `responsive`).
- `website/public/`: static pages and assets copied as-is.
- `website/seo/`: SEO source-of-truth data, templates, fixtures, and generated exports.
- `website/vercel.json`: hosting redirects/rewrites for Vercel deployment.

## Core route map
- `/`: Oliver-first landing page with a clear onboarding path.
- `/start`: Legacy startup intake route retained for backward compatibility.
- `/workspace`: Legacy workspace route that is no longer part of the primary product journey.
- `/dashboard`: Internal analytics dashboard (supporting page, not the primary product home).
- `/auth/index.html`: Oliver setup dashboard (connections, tasks, memory, settings).
- `/cn`: Localized landing path.

## Routing and hosting notes
- SPA routing uses `BrowserRouter`, so direct deep links require host rewrites to `/index.html`.
- On Vercel, rewrite coverage is defined in `website/vercel.json`, including `/start`, `/workspace`, and `/dashboard`.
- On VM/Nginx hosting, `try_files $uri /index.html;` is required for SPA routes.

## Environment variables
No website `.env` file is required for local development at the moment.
Provider-runtime overlays gracefully degrade when the user is not authenticated.

## Troubleshooting
- Port 5173 in use: run `npm run dev -- --port 5174` or stop the other process.
- `/auth/index.html` shows `Failed to fetch`: make sure MongoDB is reachable at `MONGODB_URI`, then start `./DoWhiz_service/scripts/run_employee.sh little_bear 9001 --skip-hook --skip-ngrok`. The auth page treats loopback hosts such as `localhost`, `127.0.0.1`, `0.0.0.0`, and `::1` as local development URLs.
- Clean install: remove `website/node_modules/` and run `npm install` again.
