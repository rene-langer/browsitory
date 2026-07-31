# Browsitory - Project Setup Summary

## Project Overview

**Browsitory** is a Progressive Web App for Git repository management. It provides a modern interface for viewing and managing Git repositories from a web browser, with full offline support.

### Key Features
- 📱 Progressive Web App - installable on desktop/mobile
- 🌐 Browser-based Git repository manager
- 🔍 Visual commit history and diff viewer
- 📝 Stage, commit, and push changes
- 🌿 Branch management
- 💾 Full offline support via Service Worker
- ⚡ Fast performance with Vite

### Name: **Browsitory**
"Browser" + "Repository" = A repository manager in your browser

---

## Project Structure Created

```
browsitory/
├── docs/
│   ├── FEATURES.md              # 📋 Complete feature specifications
│   ├── ARCHITECTURE.md          # 🏗️ Tech stack & system design
│   ├── DEVELOPMENT.md           # 👨‍💻 Development guide & patterns
│   ├── GETTING_STARTED.md       # 🚀 Quick start guide
│   ├── LICENSE_COMPLIANCE.md    # 📜 License verification
│   └── PROJECT_SETUP.md         # 📄 This file
│
├── src/
│   ├── components/              # React UI components
│   │   └── Layout.tsx           # Main layout shell
│   ├── pages/                   # Page-level components
│   │   ├── Dashboard.tsx        # Home/repository list
│   │   ├── Repository.tsx       # Repository details view
│   │   └── NotFound.tsx         # 404 page
│   ├── hooks/                   # Custom React hooks
│   ├── store/                   # Zustand state stores
│   ├── lib/                     # Utility functions & helpers
│   ├── services/                # Git & API services
│   ├── styles/
│   │   └── globals.css          # Global Tailwind styles
│   ├── App.tsx                  # Root React component
│   └── main.tsx                 # Application entry point
│
├── public/                      # Static assets (icons will go here)
│
├── Configuration Files
│   ├── package.json             # Dependencies & scripts
│   ├── tsconfig.json            # TypeScript configuration
│   ├── tsconfig.node.json       # TypeScript build config
│   ├── vite.config.ts           # Vite build configuration
│   ├── tailwind.config.js       # Tailwind CSS configuration
│   ├── postcss.config.js        # PostCSS configuration
│   ├── .eslintrc.json           # ESLint rules
│   ├── .prettierrc.json         # Prettier formatting rules
│   └── .gitignore               # Git ignore patterns
│
├── Documentation
│   ├── README.md                # Project overview & quick start
│   ├── LICENSE                  # MIT License
│   └── CONTRIBUTING.md          # Contribution guidelines
│
└── Build Output
    └── dist/                    # (Generated on build)
```

---

## Technology Stack

### Frontend (Chosen)
| Layer | Technology | License | Purpose |
|-------|-----------|---------|---------|
| **Framework** | React 18 | MIT | UI framework with hooks |
| **Language** | TypeScript | Apache 2.0 | Type safety |
| **Build Tool** | Vite | MIT | Fast bundler & dev server |
| **Styling** | Tailwind CSS | MIT | Utility-first CSS |
| **Routing** | React Router v6 | MIT | Client-side routing |
| **State** | Zustand | MIT | Lightweight state management |
| **Git Ops** | isomorphic-git | MIT | Pure JS Git implementation |
| **Icons** | Lucide React | ISC | SVG icons |
| **PWA** | Workbox | Apache 2.0 | Service worker management |
| **Dev Tools** | ESLint + Prettier | MIT | Code quality & formatting |

### Backend (Optional - for server deployment)
Choose one option:

**Option A: Go** (Recommended for performance)
- Framework: Gin (MIT)
- Git Lib: go-git (Apache 2.0)
- Database: PostgreSQL (MIT)

**Option B: Node.js** (Recommended for dev speed)
- Framework: Express (MIT)
- Git Lib: nodegit (MIT)
- Database: PostgreSQL (MIT)

**Option C: Python** (Recommended for simplicity)
- Framework: Flask (BSD)
- Git Lib: GitPython (BSD)
- Database: SQLite3 (Public Domain)

---

## npm Scripts

```bash
npm run dev          # Start development server (http://localhost:5173)
npm run build        # Production build
npm run preview      # Preview production build
npm run lint         # Check code style
npm run format       # Auto-format code
npm run type-check   # TypeScript type checking
```

---

## License & Compliance

✅ **All MIT Licensed or Compatible**

- **MIT** (Most dependencies) - Most permissive open source
- **Apache 2.0** (Compatible) - Vite, Workbox
- **Public Domain** (Compatible) - SQLite3
- **ISC** (Compatible) - Lucide React

See [LICENSE_COMPLIANCE.md](LICENSE_COMPLIANCE.md) for detailed breakdown.

**Key Rule**: All new dependencies must be MIT or compatible (Apache 2.0, ISC, MIT-0). NO GPL/AGPL/Commercial licenses.

---

## Development Roadmap

### Phase 1: MVP (Local PWA) - Current
- [x] Project structure & tooling setup
- [x] Basic UI shell (Layout, Dashboard, Repository page)
- [x] PWA configuration
- [x] Repository opening/management (File System Access API, Chromium-only — see Limitations)
- [x] Commit history viewing
- [x] Diff viewer
- [x] File staging/unstaging
- [x] Commit creation
- [x] Unit/component test tooling (Vitest) + tests
- [x] CI workflow (GitHub Actions)

#### Phase 1 Limitations
- Repository access requires the File System Access API (`showDirectoryPicker`), supported
  in Chrome/Edge/Opera only — Firefox and Safari are not supported yet. A LightningFS-based
  "virtual clone" mode is a possible future fallback for those browsers.
- No symlink support in the filesystem adapter (`src/services/fsaGitFs.ts`).
- Push/pull to a remote remains out of scope (planned for Phase 3 alongside the server
  backend). Branch management, merge, rebase, and stash are implemented — see Phase 2 below.

#### Phase 1 review follow-up (closed)
A review pass found the PWA manifest referenced icon and screenshot assets that didn't
exist under `public/` (404s), the favicon `<link>` in `index.html` pointed at the
Vite-template `/vite.svg` (also missing), `package-lock.json` had drifted from a clean
`npm install`, and `src/services/.gitkeep` / `src/store/.gitkeep` were stale (both
directories already contain real files). All were fixed:
- Added real placeholder PWA icons (`public/icon-192.png`, `icon-512.png`,
  `icon-192-maskable.png`, `icon-512-maskable.png`) — solid brand-purple (`#7C3AED`,
  matching `--primary` in `src/styles/globals.css`) with a white blocky "B" monogram,
  generated by the dependency-free `scripts/generate-icons.mjs` (hand-rolled PNG
  encoder using only Node's built-in `zlib`; no canvas/sharp dependency added). Rerun
  `node scripts/generate-icons.mjs` any time the placeholder needs regenerating.
- Removed the manifest's `screenshots` array in `vite.config.ts` — no real app
  screenshots exist yet, and screenshots are optional for installability, so the array
  was dropped rather than filled with fake images.
- Fixed `index.html`'s favicon to reference the shipped `/icon-192.png` instead of the
  dead `/vite.svg`.
- Deleted the stale `src/services/.gitkeep` and `src/store/.gitkeep` (left
  `src/hooks/.gitkeep` and `src/lib/.gitkeep` in place — those directories are still
  genuinely empty and unreferenced, kept for the `@hooks`/`@lib` path aliases).
- Reinstalled `node_modules` from scratch to normalize `package-lock.json` drift (a
  spurious `extraneous: true` entry for a nested `tailwindcss`-transitive `yaml`
  package — no actual dependency version changes).

**Verification**: `npm run type-check`, `npm run lint`, `npm test` (153/153 passing),
and `npm run build` all pass. Post-build, `dist/manifest.webmanifest` and
`dist/index.html` were checked statically and confirmed to reference only files that
actually exist in `dist/` (all four icon PNGs present, no more `screenshots` entries,
favicon resolves). **Not verified**: an actual browser load of the built app — the
`claude-in-chrome` browser-automation tool was unavailable in the environment this fix
was made in, so manifest installability, real icon rendering, and console/network
behavior (e.g. confirming zero 404s in the Network tab) have only been checked
statically, not in a live browser. The `showDirectoryPicker` repository-opening flow
also remains manually-verified-only, as before, since it requires a native OS file
dialog that automation can't drive.

### Phase 2: Enhanced Features
- [x] Branch management (create/delete/rename/switch — all native isomorphic-git support)
- [x] Interactive rebase (pick/drop only; no reword/squash — see scope cuts below)
- [x] Merge & conflict resolution (native `merge()`, hand-rolled ours/theirs conflict diff)
- [x] Blame viewer (hand-rolled — isomorphic-git has no native `blame`)
- [x] Stash management (push/apply/pop/drop/list — all native isomorphic-git support)
- [x] Graph visualization (multi-branch commit graph, Dagre layout + custom SVG rendering)

#### Phase 2 scope cuts (deliberate)
- **Rebase**: only `pick`/`drop` actions — no `reword` or `squash`. `onto` must be an
  ancestor of the branch being rebased (no rebasing onto a diverged branch). Must be on a
  named branch with a clean working tree to start. No in-app conflict editor — the app has
  no file-editing capability at all yet, so conflict resolution means viewing which files
  conflict plus a read-only ours/theirs diff, editing the real on-disk files with your own
  OS editor (this works since it's the File System Access API on real files), then marking
  resolved and continuing in-app.
- **Blame**: no rename-following (matches plain `git blame`'s default, not `--follow`);
  blame is against a ref's last-committed content, not live working-tree edits.
- **Merge**: no 3-way merge editor beyond the ours/theirs diff view — same "edit the real
  file yourself" model as rebase conflicts.

#### Phase 2 review follow-up
A coverage pass after merging all three workstreams found `BlameViewer.tsx` and
`GraphView.tsx` had zero test coverage (present in the implementation but no test file was
ever written for either) — both now have RTL component tests. The build also crossed
Vite's default 500 kB chunk-size warning threshold; splitting the bundle (e.g. lazy-loading
the graph/blame/rebase panels) is a reasonable follow-up but wasn't done here since it's a
performance nice-to-have, not a correctness issue.

#### First real-browser test pass (closed)
The app had never actually been exercised against a real Chromium browser + a real,
previously-cloned repository until this pass — every prior "verification" was automated
tests plus a clean build. That surfaced five real bugs no test could have caught, since the
fake directory handle used in tests has neither isomorphic-git's exact fs-binding
expectations at stake nor any meaningful async latency to expose a performance bug:

- Four correctness bugs in `fsaGitFs.ts` that made every operation against a real repo fail
  outright: a missing `readlink`/`symlink` pair crashing isomorphic-git's internal `bindFs()`
  on every call; a missing `ctimeMs` crashing `git.add()`; an unhandled `.` path segment
  (isomorphic-git's own "current directory" convention) causing `ENOENT` on every
  status/diff; and a missing global `Buffer` polyfill (Vite doesn't provide one) breaking
  any repo with real pack files, which a fresh `git.init`-created test repo never has.
- A severe performance bug in the same file: no directory-handle caching meant every read
  re-walked the full path from root, turning an O(files) tree walk into O(files × depth)
  FileSystemDirectoryHandle round-trips — ~19.5s to load one small file's diff on a real
  ~50-commit repo, fixed to ~7ms with a per-adapter directory cache.
- `DiffViewer.tsx` defaulted to character-level diffing (the library's own default),
  producing noisy, hard-to-read highlighting on any line with more than a one-character
  edit — switched to word-level diffing (`DiffMethod.WORDS_WITH_SPACE`), matching
  GitHub/GitLab-style diff rendering.

All fixed and verified live against a real repository (branch, status, stashes, commit
history, commit diffs, blame, and the multi-branch graph view). Regression tests were added
for the fsaGitFs bugs and the caching behavior; see `src/services/fsaGitFs.test.ts`.

### Phase 3: Server Backend
- [ ] Backend API (Go/Node.js)
- [ ] Database setup (PostgreSQL)
- [ ] User authentication
- [ ] Multi-user support
- [ ] Remote repository access

### Phase 4: Advanced Features
- [ ] GitHub/GitLab PR integration
- [ ] Code review tools
- [ ] Advanced search
- [ ] Collaboration features

---

## Getting Started Checklist

- [x] ✅ Project structure created
- [x] ✅ Configuration files set up
- [x] ✅ Package.json with all dependencies
- [x] ✅ TypeScript configuration
- [x] ✅ Vite + PWA setup
- [x] ✅ Tailwind CSS configured
- [x] ✅ ESLint + Prettier configured
- [x] ✅ Basic React app structure
- [x] ✅ Route setup with React Router
- [x] ✅ Documentation created
- [ ] Next: `npm install` to install dependencies
- [ ] Next: `npm run dev` to start development

---

## Quick Start Commands

```bash
# 1. Navigate to project
cd browsitory

# 2. Install dependencies
npm install

# 3. Start development server
npm run dev

# 4. Open in browser
# Visit http://localhost:5173

# 5. Install as app (optional)
# Click "Install app" in address bar (Chrome/Edge)
```

---

## File Organization Best Practices

### Components
- Place reusable components in `src/components/`
- Page-level components go in `src/pages/`
- Keep components focused and single-purpose
- Use TypeScript interfaces for props

### State Management
- Global state in Zustand stores (`src/store/`)
- Local component state with useState
- Separate stores by domain (repositories, commits, ui, etc.)

### Services
- Git operations in `src/services/git.ts`
- API calls in `src/services/api.ts`
- External integrations in appropriate service files

### Utilities
- Helper functions in `src/lib/`
- Create subdirectories for related utilities
- Export constants from `src/lib/constants.ts`

### Styling
- Use Tailwind CSS utility classes
- Global styles in `src/styles/globals.css`
- Component-specific styles via Tailwind, not CSS files

---

## IDE/Editor Setup Recommendations

### VS Code Extensions
- **ES7+ React/Redux/React-Native snippets** - dsznajder.es7-react-js-snippets
- **ESLint** - dbaeumer.vscode-eslint
- **Prettier** - esbenp.prettier-vscode
- **Tailwind CSS IntelliSense** - bradlc.vscode-tailwindcss
- **TypeScript Vue Plugin** - Vue.volar

### VS Code Settings
```json
{
  "editor.defaultFormatter": "esbenp.prettier-vscode",
  "editor.formatOnSave": true,
  "editor.codeActionsOnSave": {
    "source.fixAll.eslint": true
  }
}
```

---

## Documentation Files

| Document | Purpose |
|----------|---------|
| [README.md](../README.md) | Project overview & quick start |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Tech decisions & system design |
| [FEATURES.md](FEATURES.md) | Detailed feature specifications |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Development guide & patterns |
| [GETTING_STARTED.md](GETTING_STARTED.md) | Quick start for developers |
| [LICENSE_COMPLIANCE.md](LICENSE_COMPLIANCE.md) | License verification |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | How to contribute |

---

## Key Decisions & Rationale

### Why React?
- Large ecosystem for PWA development
- Excellent developer experience
- Strong component model
- Great for building complex UIs

### Why TypeScript?
- Type safety prevents bugs
- Better IDE support
- Self-documenting code
- Popular in modern projects

### Why Vite?
- Lightning-fast dev server (ES modules)
- Fast production builds
- Hot module replacement
- Modern build configuration

### Why Tailwind CSS?
- Utility-first approach is faster
- Great for PWA styling
- Smaller bundle size than component libraries
- Easy theming support

### Why isomorphic-git?
- Pure JavaScript implementation
- Works in browser (no Git CLI needed)
- Perfect for PWA use case
- Good offline support

### Why Local PWA First?
- No backend needed to start
- Faster time-to-MVP
- Lower deployment complexity
- Can add server backend later

---

## Next Steps

1. **Install dependencies**
   ```bash
   npm install
   ```

2. **Start developing**
   ```bash
   npm run dev
   ```

3. **Read development guide**
   - See [DEVELOPMENT.md](DEVELOPMENT.md) for patterns & conventions

4. **Start implementing features**
   - Pick a feature from [FEATURES.md](FEATURES.md)
   - Create UI components
   - Integrate Git operations
   - Add state management as needed

5. **For deployment**
   - See [ARCHITECTURE.md](ARCHITECTURE.md) deployment section
   - Choose local PWA or server-based deployment

---

## Resources

- **[React Docs](https://react.dev)** - React fundamentals
- **[TypeScript Handbook](https://www.typescriptlang.org/docs/)** - TypeScript guide
- **[Tailwind CSS](https://tailwindcss.com/)** - CSS utilities
- **[Vite Guide](https://vitejs.dev/)** - Build tool
- **[isomorphic-git](https://isomorphic-git.org/)** - Git operations
- **[React Router](https://reactrouter.com/)** - Routing
- **[Zustand](https://github.com/pmndrs/zustand)** - State management

---

**Status**: 🟢 Project structure ready for development  
**Last Updated**: 2024-07-31  
**License**: MIT  

Ready to build? Run `npm install && npm run dev`! 🚀
