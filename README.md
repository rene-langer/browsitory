# Browsitory

A Progressive Web App for Git repository management with a fast, visual Git client experience. Manage your Git repositories locally as an installable app or deploy on a server to manage repositories from anywhere.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **Status: Phase 1 (MVP)**. Implemented today: opening a local repository, commit history,
> diff viewing, staging/unstaging, and committing. Everything else in the feature list below
> is on the [roadmap](#roadmap). Repository access uses the browser's File System Access API,
> so a Chromium-based browser (Chrome, Edge, Opera) is required for now — see
> [Limitations](docs/PROJECT_SETUP.md#phase-1-limitations).

## Features

- 📱 **Progressive Web App** - Install as a native app or use in browser
- 🌐 **Offline Support** - Full functionality without internet connection
- 🔍 **Visual Diff Viewer** - Side-by-side or unified diff comparison
- 📊 **Commit Graph** - Visualize repository history as a DAG
- 🌿 **Branch Management** - Create, switch, merge, and rebase branches
- 📝 **Staging Area** - Stage/unstage changes with hunk-level control
- 🚀 **Push/Pull** - Synchronize with remote repositories
- 💾 **Stash Management** - Save work in progress
- 🔎 **Search & Filter** - Find commits by message, author, or hash
- ⚡ **Fast & Responsive** - Lightning-fast performance on desktop and tablet
- 🎨 **Dark/Light Theme** - Beautiful UI with theme support
- ⌨️ **Keyboard Shortcuts** - Customizable keybindings for power users

## Quick Start

### Local PWA (Recommended for Getting Started)

```bash
# Install dependencies
npm install

# Development server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview
```

Then open http://localhost:5173 in your browser and add the app to your home screen.

### Server Deployment (Coming Soon)

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for server deployment options.

## Tech Stack

### Frontend
- **React 18** - UI framework (MIT)
- **TypeScript** - Type safety
- **Vite** - Build tool (MIT)
- **Tailwind CSS** - Styling (MIT)
- **isomorphic-git** - Git operations (MIT)
- **idb-keyval** - Persists repository handles across reloads (Apache 2.0)
- **react-diff-viewer-continued** - Diff rendering (MIT)
- **Zustand** - State management (MIT)
- **Workbox** - Service worker (MIT)

### Testing
- **Vitest** + **@testing-library/react** - Unit and component tests (MIT)

### Backend (Optional)
- **Go + Gin** OR **Node.js + Express** (MIT)
- **PostgreSQL / SQLite3** (MIT)
- **go-git / nodegit** (MIT / Apache 2.0)

## Project Structure

```
browsitory/
├── docs/
│   ├── FEATURES.md          # Detailed feature list
│   ├── ARCHITECTURE.md      # System architecture & tech decisions
│   └── DEPLOYMENT.md        # Deployment guide (WIP)
├── src/
│   ├── components/          # React components
│   ├── pages/              # Page components
│   ├── hooks/              # Custom React hooks
│   ├── store/              # Zustand stores
│   ├── lib/                # Utilities and helpers
│   ├── services/           # Git operations service
│   ├── styles/             # Global styles
│   └── App.tsx
├── public/
│   ├── manifest.json       # PWA manifest
│   └── icons/              # PWA icons
├── package.json
├── tsconfig.json
├── vite.config.ts
└── README.md
```

## Getting Started with Development

1. **Clone the repository**
   ```bash
   git clone <repository-url>
   cd browsitory
   ```

2. **Install dependencies**
   ```bash
   npm install
   ```

3. **Start development server**
   ```bash
   npm run dev
   ```

4. **Open in browser**
   - Visit http://localhost:5173
   - Use Chrome DevTools to simulate PWA installation
   - Or use "Install app" option in address bar (Chrome/Edge)

## Installation as App

### Desktop
1. Visit the app in Chrome, Edge, or other Chromium-based browser
2. Look for "Install app" button in address bar (or menu)
3. Click to install
4. App appears in your applications menu

### Mobile (Android)
1. Visit the app in Chrome
2. Tap menu → "Install app"
3. Confirm installation
4. App added to home screen

### iOS/Safari
1. Visit the app in Safari
2. Tap Share → "Add to Home Screen"
3. Name the shortcut
4. App added to home screen (limited PWA support)

## Documentation

- **[Features](docs/FEATURES.md)** - Complete feature specifications
- **[Architecture](docs/ARCHITECTURE.md)** - System design and technology choices
- **[Development Guide](docs/DEVELOPMENT.md)** - Development setup and conventions (WIP)
- **[Deployment Guide](docs/DEPLOYMENT.md)** - Deployment and hosting options (WIP)

## Usage Examples

### Opening a Repository
1. Click "Open Repository" or drag & drop a folder
2. Browser will prompt for folder access permissions
3. Repository appears in your repository list

### Viewing Commit History
1. Select a repository from the list
2. Commit history displays in main view
3. Click a commit to see details and diff
4. Use search to filter commits

### Creating a Commit
1. Open a repository
2. Select files to stage in the "Changes" panel
3. Write commit message
4. Click "Commit"

### Pushing Changes
Not yet implemented — planned for a later phase alongside fetch/pull. See the
[roadmap](#roadmap).

## Contributing

Contributions are welcome! Please read our contributing guidelines (WIP) before submitting PRs.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [isomorphic-git](https://isomorphic-git.org/) for pure JavaScript git operations

## Roadmap

- [x] Initial architecture and feature planning
- [x] Phase 1: MVP with local PWA support
  - [x] Repository management (File System Access API)
  - [x] Commit history viewer
  - [x] Diff viewer
  - [x] Staging/unstaging + commit creation
  - [x] Unit/component test tooling (Vitest) + CI
  - [ ] Deeper offline caching of repository data (app-shell offline support is in place)
- [ ] Phase 2: Enhanced features
  - [ ] Interactive rebase
  - [ ] Conflict resolution
  - [ ] Blame viewer
  - [ ] Graph visualization
- [ ] Phase 3: Server backend
  - [ ] Backend API
  - [ ] User authentication
  - [ ] Multi-user support
- [ ] Phase 4: Advanced features
  - [ ] PR integration
  - [ ] Code review tools
  - [ ] Collaboration features

## Support

For issues, feature requests, or questions, please open an issue on GitHub.

---

**Status**: Early development - breaking changes expected. Not ready for production use yet.
