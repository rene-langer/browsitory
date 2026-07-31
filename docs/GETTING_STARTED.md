# Getting Started with Browsitory

Welcome to Browsitory! This guide will help you set up and understand the project.

## What is Browsitory?

Browsitory is a Progressive Web App (PWA) for Git repository management. It provides a modern, intuitive interface for:
- Viewing commit history
- Managing branches
- Staging and committing changes
- Viewing diffs
- Push/pull operations

The unique aspect is that it's a **Progressive Web App** - it can run:
- As an installable app on your desktop or mobile device
- As a web app in your browser
- **Offline** with cached repository data

## Quick Start (5 minutes)

### 1. Install Dependencies
```bash
cd browsitory
npm install
```

### 2. Start Development Server
```bash
npm run dev
```

### 3. Open in Browser
Visit `http://localhost:5173` and explore the app.

### 4. Install as App (Optional)
- **Desktop**: Click "Install app" in the address bar (Chrome/Edge)
- **Mobile**: Tap menu → "Install app" (Android Chrome)
- **iOS**: Tap Share → "Add to Home Screen"

## Project Structure

Quick reference for important files:

```
browsitory/
├── docs/                    # Documentation
│   ├── FEATURES.md         # Complete feature list
│   ├── ARCHITECTURE.md     # Tech stack & design
│   ├── DEVELOPMENT.md      # Development guide
│   └── LICENSE_COMPLIANCE  # License verification
├── src/
│   ├── components/         # Reusable React components
│   ├── pages/             # Page-level components (Dashboard, Repository)
│   ├── hooks/             # Custom React hooks
│   ├── store/             # Zustand state management
│   ├── lib/               # Utility functions
│   ├── services/          # Git service & API integration
│   └── App.tsx            # Root component
├── public/                # Static files (icons, etc.)
├── README.md              # Project overview
├── package.json           # Dependencies
└── vite.config.ts         # Build configuration
```

## Tech Stack at a Glance

### Frontend
- **React** - UI framework
- **TypeScript** - Type safety
- **Vite** - Fast build tool
- **Tailwind CSS** - Styling
- **isomorphic-git** - Git operations (JavaScript)

### Backend (Optional)
- **Go/Node.js** - Can add for server deployment
- **PostgreSQL/SQLite** - Database (for server mode)

## Key Concepts

### 1. Progressive Web App (PWA)
- Works offline thanks to Service Worker
- Can be installed as an app
- Installable on desktop and mobile
- Works without requiring separate app download

### 2. Git Operations
- Uses **isomorphic-git** - a pure JavaScript git implementation
- Works in browser without needing Git CLI
- Can handle file I/O via browser File API

### 3. Local-First Architecture
- Initial MVP stores data in browser (IndexedDB, localStorage)
- No backend required to get started
- Can add optional backend for server deployment later

## Common Tasks

### View Commit History
1. Open a Git repository
2. Repository status shows in main view
3. Commit list displays automatically
4. Click a commit to see details

### Stage Changes
1. In the Changes panel, select files to stage
2. Click "Stage" to add to staging area
3. Unstaged changes show in separate section

### Create a Commit
1. Stage files you want to commit
2. Write commit message
3. Click "Commit"

### Switch Branches
1. View branch list in sidebar
2. Click branch name to switch
3. Working directory updates instantly

## Development Workflow

### Making Changes

1. **Create a branch**
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Make your changes** - files auto-reload

3. **Check code quality**
   ```bash
   npm run lint        # Check code style
   npm run format      # Format code
   npm run type-check  # Type checking
   ```

4. **Build and test**
   ```bash
   npm run build       # Production build
   npm run preview     # Preview build
   ```

5. **Commit and push**
   ```bash
   git add .
   git commit -m "feat: add new feature"
   git push origin feature/my-feature
   ```

### Project Standards

- **Code Style**: ESLint + Prettier (run `npm run format`)
- **Type Safety**: Strict TypeScript (fix type errors)
- **Commit Messages**: Conventional Commits format (feat:, fix:, docs:, etc.)
- **Components**: React functional components with hooks
- **Styling**: Tailwind CSS utility classes

## API Reference (for Backend Integration)

When implementing server backend, these endpoints are needed:

```
GET    /api/repositories              - List repositories
POST   /api/repositories              - Add new repository  
GET    /api/repositories/:id/commits  - Get commit history
GET    /api/repositories/:id/status   - Get repo status
POST   /api/repositories/:id/commit   - Create commit
POST   /api/repositories/:id/push     - Push to remote
POST   /api/repositories/:id/pull     - Pull from remote
GET    /api/repositories/:id/diff     - Get diff
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed API design.

## Next Steps

### For Users
1. Open a local Git repository
2. Explore the commit history
3. Try staging changes
4. Install as an app

### For Contributors
1. Read [DEVELOPMENT.md](DEVELOPMENT.md)
2. Read [CONTRIBUTING.md](../CONTRIBUTING.md)
3. Pick an issue to work on
4. Submit a pull request

### For Deploying
1. See [ARCHITECTURE.md](ARCHITECTURE.md) for deployment options
2. Deployment guide coming soon

## Troubleshooting

### Port Already in Use?
```bash
lsof -ti:5173 | xargs kill -9
npm run dev -- --port 3000
```

### Module Not Found?
```bash
rm -rf node_modules package-lock.json
npm install
```

### TypeScript Errors?
```bash
npm run type-check  # See all errors
npm run format      # Auto-fix some issues
```

### Git Operations Failing?
- Check browser console (F12) for error details
- Ensure repository path is correct
- Verify `.git` directory exists

## Resources

- **[Documentation Index](../README.md)** - All documentation
- **[Features](FEATURES.md)** - Complete feature specifications
- **[Architecture](ARCHITECTURE.md)** - System design decisions
- **[Development Guide](DEVELOPMENT.md)** - Detailed dev setup
- **[Contributing](../CONTRIBUTING.md)** - How to contribute

## Questions?

- Check documentation first
- Look at issues and discussions on GitHub
- Open a new issue if stuck
- Ask in pull requests if uncertain

## License

Browsitory is licensed under MIT License - see [LICENSE](../LICENSE) for details.

---

**Ready to get started?** Run `npm install && npm run dev` and open http://localhost:5173! 🚀
