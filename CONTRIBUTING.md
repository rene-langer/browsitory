# Contributing to Browsitory

Thank you for your interest in contributing to Browsitory! We welcome contributions of all kinds, including bug reports, feature requests, documentation improvements, and code contributions.

## Code of Conduct

Be respectful and constructive in all interactions with other contributors.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally
3. **Create a feature branch** with a descriptive name
4. **Make your changes** and test them
5. **Commit your changes** with clear, descriptive messages
6. **Push to your fork** and **open a pull request**

## Development Setup

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for detailed setup instructions.

## Making Changes

### Before You Start
- Check if an issue exists for your change
- For large changes, open an issue first to discuss
- Follow the project's code style (ESLint + Prettier)

### While Developing
- Write clear, self-documenting code
- Add comments only for non-obvious logic
- Keep functions small and focused
- Use TypeScript types properly

### Testing Your Changes
```bash
npm run lint        # Check for linting errors
npm run format      # Format code
npm run type-check  # Type checking
npm run build       # Verify build works
```

## Pull Request Process

1. **Update documentation** if your changes require it
2. **Add/update tests** if applicable (when test infrastructure is added)
3. **Keep commits clean** - use clear commit messages following Conventional Commits
4. **Reference related issues** in your PR description
5. **Be responsive** to feedback during review

### PR Title Format
Use Conventional Commits format:
- `feat: add feature description`
- `fix: fix issue description`
- `docs: update documentation`
- `refactor: refactor component name`

### PR Description Template
```markdown
## Description
Brief description of what this PR does

## Changes
- Change 1
- Change 2
- Change 3

## Related Issue
Fixes #123

## Testing
How to test these changes

## Screenshots (if applicable)
Before/after screenshots for UI changes
```

## Feature Requests

When suggesting a new feature:
1. Use a clear, descriptive title
2. Provide a detailed description
3. Explain the use case and why it's needed
4. Include mockups or examples if helpful

## Bug Reports

When reporting a bug:
1. Use a clear, descriptive title
2. Describe the exact steps to reproduce
3. Describe the observed behavior
4. Explain the expected behavior
5. Include screenshots if applicable
6. Include your environment (OS, browser, Node version)

## License Compliance

- All contributions must be compatible with MIT License
- Ensure any libraries you use are MIT licensed or compatible
- Document any license of external code used
- Check `package.json` for existing solutions

## Architecture Guidelines

Before making architectural changes:
1. Read [ARCHITECTURE.md](docs/ARCHITECTURE.md)
2. Discuss major changes in an issue first
3. Consider backward compatibility
4. Update architecture docs if needed

## Code Style

### TypeScript
- Use explicit types, avoid `any`
- Follow naming conventions (camelCase for variables, PascalCase for components)
- Keep type definitions close to usage

### React
- Use functional components with hooks
- Keep components focused and reusable
- Use meaningful component/prop names
- Extract complex logic to custom hooks

### Styling
- Use Tailwind utility classes
- Follow the design system in `tailwind.config.js`
- Avoid inline styles
- Use semantic color names

### Git
- Create a new branch for each feature/fix
- Keep branches up-to-date with main
- Rebase before opening PR (no merge commits)
- Squash commits if they're related to the same change

## Performance Considerations

When contributing:
- Consider bundle size impact
- Use code splitting for large features
- Memoize expensive components
- Cache data appropriately (IndexedDB for offline)
- Profile before and after changes

## Documentation

Please update relevant documentation for your changes:
- README.md for user-facing changes
- DEVELOPMENT.md for development setup changes
- ARCHITECTURE.md for architectural changes
- Inline comments for complex logic

## Questions?

- Check existing issues and discussions
- Read the documentation first
- Open a GitHub discussion if unclear
- Reach out to maintainers if needed

## Recognition

Contributors will be recognized in:
- GitHub contributors page
- Project README (for significant contributions)
- Release notes

Thank you for making Browsitory better! 🚀
