# License Compliance Report

Browsitory is licensed under the MIT License. All dependencies must comply with MIT or compatible licenses.

## Frontend Dependencies

### Core
- **React** - MIT License
- **React DOM** - MIT License
- **TypeScript** - Apache 2.0 License (compatible)
- **Vite** - MIT License

### Routing & State
- **React Router** - MIT License
- **Zustand** - MIT License

### UI & Styling
- **Tailwind CSS** - MIT License
- **Autoprefixer** - MIT License
- **PostCSS** - MIT License
- **Lucide React** - ISC License (compatible)
- **clsx** - MIT License
- **tailwind-merge** - MIT License

### Git Operations
- **isomorphic-git** - MIT License + Apache 2.0 License (dual-licensed, MIT compatible)
- **idb-keyval** - Apache 2.0 License (compatible; persists repository handles/metadata in IndexedDB)

Phase 1's File System Access ↔ isomorphic-git bridge (`src/services/fsaGitFs.ts`) is
hand-written rather than a dependency: the closest off-the-shelf option, ZenFS's
`@zenfs/dom` `WebAccess` backend, is **LGPL-3.0** and was rejected on license grounds.
`@isomorphic-git/lightning-fs` (MIT) remains a documented fallback for a possible future
cross-browser "virtual clone" mode, but is not a current dependency.

### Diff Viewer
- **react-diff-viewer-continued** - MIT License

### Build Tools
- **@vitejs/plugin-react** - MIT License
- **@vitejs/plugin-react-swc** - MIT License
- **Vite PWA Plugin** - MIT License
- **Workbox** - Apache 2.0 License (compatible)

### Development Tools
- **ESLint** - MIT License
- **TypeScript ESLint** - MIT License + BSD License (compatible)
- **Prettier** - MIT License
- **@types/react** - MIT License
- **@types/react-dom** - MIT License
- **@types/node** - MIT License
- **@types/wicg-file-system-access** - MIT License (DefinitelyTyped)

### Testing
- **Vitest** - MIT License
- **@vitest/coverage-v8** - MIT License
- **jsdom** - MIT License
- **@testing-library/react** - MIT License
- **@testing-library/jest-dom** - MIT License
- **@testing-library/user-event** - MIT License

## Backend Dependencies (Optional, for server mode)

### Node.js Stack
- **Express.js** - MIT License
- **Node.js** - MIT License

### Go Stack
- **Gin** - MIT License
- **go-git** - Apache 2.0 License (compatible)

### Database
- **SQLite3** - Public Domain
- **PostgreSQL** - PostgreSQL License (compatible with MIT)
- **Prisma** - Apache 2.0 License (compatible)

## License Summary

✅ **All core dependencies are MIT licensed or have compatible licenses**

### Licenses Used in Project
- **MIT License** - Most permissive, used by majority of dependencies
- **Apache 2.0 License** - Compatible with MIT
- **ISC License** - Compatible with MIT
- **Public Domain** - SQLite3

### Compatibility Notes
1. Apache 2.0 is compatible with MIT (more permissive MIT)
2. ISC is compatible with MIT (similar permissiveness)
3. Public domain (SQLite3) is fully compatible
4. PostgreSQL License is compatible with MIT

## Compliance Verification Process

To verify license compliance:

```bash
# Check for license issues
npm list

# For detailed license information
npx license-checker

# View specific package license
npm view <package-name> license
```

## Adding New Dependencies

When adding new dependencies:

1. **Verify the license**
   ```bash
   npm view <package-name> license
   ```

2. **Check compatibility** - Only MIT, Apache 2.0, ISC, MIT-0, or similar permissive licenses

3. **Update this document** with the new dependency and its license

4. **Run license checker**
   ```bash
   npx license-checker
   ```

## Excluded Licenses

❌ Do NOT add dependencies with these licenses:
- GPL (any version) - Requires derivative works to be open source
- AGPL - Network version of GPL
- SSPL - Server Side Public License
- Commercial/Proprietary - Requires purchase
- Elastic License - Restrictive commercial license

## Attribution

All licenses of dependencies are properly attributed through:
- `package.json` - Lists all dependencies and versions
- `node_modules/<package>/LICENSE` - Each package includes its license
- This document - Provides compliance overview

## Questions About Licenses?

If you're unsure about a license:
1. Check [Open Source Initiative](https://opensource.org/licenses)
2. Review [SPDX License List](https://spdx.org/licenses/)
3. Open an issue to discuss before adding dependency

## Version Updates

When updating dependency versions, verify the license hasn't changed:

```bash
# Before updating
npm outdated

# Check license of new version
npm view <package>@latest license

# Update if license is compatible
npm update <package>
```

---

Last Updated: 2024
