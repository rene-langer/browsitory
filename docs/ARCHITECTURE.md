# Browsitory - Architecture & Tech Stack

## Overview

Browsitory is a Progressive Web App (PWA) for Git repository management, providing a fast, visual Git client experience in the browser. It supports both local deployment (installable app) and server-based deployment for remote repository access.

## Architecture Layers

### 1. Frontend Layer (Browser)

**Framework**: React 18+ with TypeScript
- **Why React**: Large ecosystem, excellent PWA support, strong component model
- **License**: MIT
- **State Management**: Zustand (lightweight, MIT) or Redux Toolkit (MIT)
- **UI Components**: shadcn/ui (MIT) or Mantine (MIT)
- **Routing**: React Router v6 (MIT)
- **Git Operations**: isomorphic-git (MIT) - pure JavaScript git implementation
- **Diff Viewer**: react-diff-viewer-continued (MIT) or custom implementation
- **Graph Visualization**: Dagre (MIT) for DAG rendering, React Flow (MIT) for interactive layouts
- **Styling**: Tailwind CSS (MIT) or CSS modules
- **Icons**: Lucide React (MIT)

**Build Tool**: Vite (MIT)
- Fast development server
- Optimized production builds
- Native ESM support

**PWA Support**:
- Workbox (MIT) for service worker management
- Manifest.json for installability
- Service Worker for offline support

### 2. Backend Layer (Optional/Server Deployment)

**Language Options**:

**Option A: Node.js + Express (Recommended)**
- **Runtime**: Node.js (MIT)
- **Framework**: Express.js (MIT)
- **License**: MIT
- **Why**: Same language as frontend reduces context switching, large git library ecosystem
- **Git Operations**: nodegit (MIT) or isomorphic-git
- **Database**: SQLite3 (MIT) for local, PostgreSQL (MIT) for server
- **ORM**: Prisma (Apache 2.0, compatible) or Typeorm (MIT)

**Option B: Python + Flask**
- **Framework**: Flask (BSD, compatible)
- **Git Operations**: GitPython (BSD, compatible)
- **Database**: SQLite3 (MIT) for local
- **Why**: Simple, excellent git integration

**Option C: Go**
- **Framework**: Gin (MIT)
- **Git Operations**: go-git (Apache 2.0, compatible)
- **Why**: High performance, small binary, excellent for server deployments

**Recommendation**: Go-based backend for production deployments, simple Node.js Express for development and local setups.

### 3. Data Layer

**Local Storage**:
- **Preferences**: localStorage (browser standard)
- **Repository Metadata**: IndexedDB (browser standard) for larger datasets
- **Commit Cache**: IndexedDB for offline access to commit history
- **Database**: SQLite3 (MIT) when running locally

**Server Storage** (optional):
- **Database**: PostgreSQL (MIT) with connection pooling
- **Cache**: Redis (MIT) for session management and commit cache
- **File Storage**: Direct filesystem access to git repositories

### 4. Git Integration

**Frontend Git Operations**:
- **Library**: isomorphic-git (MIT)
- **Capabilities**: Read-only operations in browser (status, log, diff, blame)
- **Limitations**: File system access via File API when needed

**Backend Git Operations** (server mode):
- **Library**: go-git (Apache 2.0) or nodegit (MIT)
- **Capabilities**: Full git operations (push, pull, rebase, merge)
- **Persistence**: Direct access to repository directories

## System Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Browser / Desktop                     │
├─────────────────────────────────────────────────────────┤
│  React App + TypeScript                                 │
│  ├─ UI Components (shadcn/ui)                          │
│  ├─ State Management (Zustand)                         │
│  ├─ Git Operations (isomorphic-git)                    │
│  ├─ Diff Viewer                                        │
│  └─ Service Worker (Workbox)                           │
├─────────────────────────────────────────────────────────┤
│  IndexedDB / localStorage                              │
│  (Offline cache, preferences)                          │
└─────────────────────────────────────────────────────────┘
           ↓ (Optional API calls)
┌─────────────────────────────────────────────────────────┐
│              Backend Server (Optional)                   │
├─────────────────────────────────────────────────────────┤
│  Go + Gin Framework  OR  Node.js + Express             │
│  ├─ REST/GraphQL API                                   │
│  ├─ Git Operations (go-git / nodegit)                  │
│  ├─ Authentication                                      │
│  └─ Repository Management                              │
├─────────────────────────────────────────────────────────┤
│  PostgreSQL / SQLite3                                  │
│  (User data, repository metadata, settings)            │
└─────────────────────────────────────────────────────────┘
           ↓
┌─────────────────────────────────────────────────────────┐
│            Git Repositories (File System)                │
└─────────────────────────────────────────────────────────┘
```

## Deployment Modes

### Mode 1: Local PWA (Recommended for Initial MVP)
- React app bundled with Vite
- isomorphic-git for all git operations
- Data stored in IndexedDB and localStorage
- Installable via "Add to Home Screen"
- Works offline
- Direct file system access via File/Directory APIs
- Zero backend required

### Mode 2: Server-Based (Production)
- Same React frontend deployed to web server
- Backend API server (Go/Node.js)
- Repositories stored on server
- User authentication
- Multi-user support
- Can manage repositories from anywhere

### Mode 3: Hybrid
- Local PWA for local repositories
- Optional backend for remote repository access
- Sync preferences across devices

## API Design (for Server Mode)

**REST Endpoints**:
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

Or GraphQL alternative for more flexible querying.

## Performance Considerations

1. **Code Splitting**: Route-based and library-based splitting
2. **Lazy Loading**: Load commit history on demand
3. **Virtual Scrolling**: For large commit lists
4. **IndexedDB Caching**: Cache frequently accessed data
5. **Service Worker**: Precache critical assets
6. **Image Optimization**: SVG for icons, WebP for images
7. **Bundle Size**: Target < 500KB gzipped for core app

## Security Considerations

1. **SSH Key Storage**: Store SSH keys securely in browser (IndexedDB with encryption) or require passphrase entry per operation
2. **Git Credentials**: Never store plaintext passwords, use credential managers or token-based auth
3. **CORS**: Properly configure CORS if backend is separate
4. **CSP**: Content Security Policy headers
5. **Input Validation**: Sanitize git input to prevent injection attacks
6. **File Access**: Validate file paths to prevent directory traversal

## Data Persistence Strategy

### Local Mode
- IndexedDB: Commit history, diffs, blame info
- localStorage: User preferences, selected repository
- filesystem: Direct repository access via File API

### Server Mode
- PostgreSQL: Persistent data (users, repositories, settings)
- Redis: Session cache, rate limiting
- Filesystem: Git repository directories
- CDN: Static assets (optional)

## Development Roadmap Phases

### Phase 1: MVP (Local PWA)
- Basic repository management
- Commit history viewing
- Diff viewer
- Staging/unstaging
- Commit creation
- Branch switching
- Service worker & offline support

### Phase 2: Enhanced Features
- Interactive rebase
- Merge conflict resolution
- Blame view
- Stash management
- Graph visualization

### Phase 3: Server Backend
- Backend API implementation
- User authentication
- Multi-user support
- Remote repository access

### Phase 4: Advanced Features
- PR integration (GitHub/GitLab)
- Code review tools
- Advanced search
- Collaboration features
