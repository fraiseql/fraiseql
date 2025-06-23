# FraiseQL Website

This is the marketing website for FraiseQL, showcasing its features and benefits.

## Structure

- `index.html` - Homepage with hero, features, and quick start
- `features/` - Feature pages
  - `index.html` - Features overview
  - `turborouter.html` - TurboRouter deep dive
- `docs/` - Redirects to ReadTheDocs
- `style.css` - All styles

## Key Features Highlighted

1. **TurboRouter** - Near-zero overhead query execution
2. **CQRS Architecture** - Clean separation of reads/writes
3. **Security First** - SQL injection protection built-in
4. **Code Generation** - Generate migrations and CRUD
5. **Performance** - 5-17x faster than traditional ORMs

## Deployment

The website is static HTML/CSS and can be deployed to any static hosting service:

### GitHub Pages
```bash
git add website/
git commit -m "Update website"
git push
```

### Netlify
Drop the `website/` folder into Netlify

### Vercel
```bash
cd website/
vercel --prod
```

### Local Development
```bash
cd website/
python -m http.server 8000
# Visit http://localhost:8000
```

## Updates

When adding new features:
1. Update the homepage features grid if it's a major feature
2. Add to features/index.html
3. Consider creating a dedicated feature page like turborouter.html
4. Update navigation if needed
