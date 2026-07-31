# Development Guide

## Getting Started

### Prerequisites
- Node.js >= 18.0.0
- npm >= 9.0.0
- Git

### Setup

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
   - App will hot-reload on file changes

## Project Structure

```
browsitory/
├── src/
│   ├── components/     # Reusable React components
│   ├── pages/         # Page-level components
│   ├── hooks/         # Custom React hooks
│   ├── store/         # Zustand state stores
│   ├── lib/           # Utilities and helper functions
│   ├── services/      # External service integrations (Git, API)
│   ├── styles/        # Global styles
│   ├── App.tsx        # Root component
│   └── main.tsx       # Application entry point
├── public/            # Static assets
├── docs/              # Documentation
├── package.json
└── vite.config.ts     # Build configuration
```

## Development Workflow

### Code Style

We use ESLint and Prettier for code quality. Run these commands:

```bash
# Check for linting issues
npm run lint

# Format code
npm run format

# Type checking
npm run type-check
```

### Git Workflow

1. Create a feature branch: `git checkout -b feature/feature-name`
2. Make your changes
3. Run linting and tests: `npm run lint && npm run type-check`
4. Format code: `npm run format`
5. Commit with descriptive message: `git commit -m "feat: add feature"`
6. Push to your fork and open a pull request

### Commit Message Convention

Follow Conventional Commits:
- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation
- `style:` for code style changes
- `refactor:` for code refactoring
- `test:` for test additions/changes
- `chore:` for maintenance tasks

Example: `feat: add diff viewer component`

## Key Libraries & Patterns

### Git Operations

We use `isomorphic-git` for all Git operations. It's a pure JavaScript implementation that works in browsers.

Example usage:
```typescript
import * as fs from 'isomorphic-git'

// Read commit history
const commits = await fs.log({ fs, dir: '/path/to/repo' })

// Get repository status
const status = await fs.statusMatrix({ fs, dir: '/path/to/repo' })
```

### State Management

We use Zustand for state management. Create stores in `src/store/`:

```typescript
import { create } from 'zustand'

interface RepositoryStore {
  repositories: Repository[]
  currentRepo: Repository | null
  addRepository: (repo: Repository) => void
  selectRepository: (id: string) => void
}

export const useRepositoryStore = create<RepositoryStore>((set) => ({
  repositories: [],
  currentRepo: null,
  addRepository: (repo) => set((state) => ({
    repositories: [...state.repositories, repo],
  })),
  selectRepository: (id) => set((state) => ({
    currentRepo: state.repositories.find(r => r.id === id) || null,
  })),
}))
```

### React Hooks

Custom hooks go in `src/hooks/`. Common patterns:

```typescript
// Hook for git operations
export function useGitOperation(dir: string) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<Error | null>(null)

  const execute = useCallback(async (operation: () => Promise<any>) => {
    try {
      setLoading(true)
      setError(null)
      return await operation()
    } catch (err) {
      setError(err instanceof Error ? err : new Error(String(err)))
      throw err
    } finally {
      setLoading(false)
    }
  }, [])

  return { execute, loading, error }
}
```

### Styling

We use Tailwind CSS for styling. Some guidelines:

- Use existing Tailwind classes, avoid custom CSS when possible
- Follow the design system defined in `tailwind.config.js`
- Use semantic color classes: `bg-primary`, `text-muted-foreground`, etc.
- For responsive design, use Tailwind's responsive prefixes: `md:`, `lg:`, etc.

Example:
```tsx
<button className="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:opacity-90 transition">
  Click me
</button>
```

## PWA Features

The app is a Progressive Web App enabled by Vite PWA plugin. To test PWA features:

### Service Worker
- Check DevTools → Application → Service Workers
- See caching behavior in Network tab
- Check offline functionality

### Installation
- Desktop: Look for "Install app" in address bar
- Mobile: Menu → "Install app"
- Test adding to home screen

### Icons
PWA icons should be placed in `public/`:
- `icon-192.png` - 192x192 PWA icon
- `icon-512.png` - 512x512 PWA icon
- `icon-192-maskable.png` - 192x192 maskable icon
- `icon-512-maskable.png` - 512x512 maskable icon

## Testing (Future)

```bash
# Run tests
npm run test

# Run tests in watch mode
npm run test:watch

# Generate coverage report
npm run test:coverage
```

## Building for Production

```bash
# Build the app
npm run build

# Preview production build locally
npm run preview

# Build output is in `dist/` directory
```

## Deployment

See [DEPLOYMENT.md](DEPLOYMENT.md) for deployment instructions.

## Troubleshooting

### Port already in use
```bash
# Kill process on port 5173
lsof -ti:5173 | xargs kill -9

# Or use different port
npm run dev -- --port 3000
```

### Git operations not working
- Ensure repository path is correct
- Check browser console for errors
- Verify repository has proper `.git` directory

### PWA not installing
- Must be served over HTTPS (except localhost)
- Manifest.json must be valid
- Check DevTools for PWA requirements

### Module not found errors
- Check import paths use `@/` aliases
- Ensure files exist in expected locations
- Run `npm install` after adding new dependencies

## Performance Tips

1. **Code Splitting**: Use React.lazy() for route-based splitting
2. **Memoization**: Use React.memo() for expensive components
3. **Virtual Lists**: Use virtualization for large lists (1000+ items)
4. **IndexedDB**: Cache commit history for offline access
5. **Service Worker**: Precache critical assets

## Resources

- [React Documentation](https://react.dev)
- [TypeScript Handbook](https://www.typescriptlang.org/docs/)
- [Tailwind CSS](https://tailwindcss.com/docs)
- [isomorphic-git](https://isomorphic-git.org/)
- [Zustand](https://github.com/pmndrs/zustand)
- [Vite Documentation](https://vitejs.dev/)
