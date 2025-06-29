# Deployment Instructions for fraiseql.dev

## Website Structure

The FraiseQL website consists of two parts:

1. **Marketing Website** (`/website/`) - Static HTML/CSS/JS
2. **Documentation** (`/docs/` source, `/site/` built) - MkDocs

## Deployment Steps

### 1. Build Documentation
```bash
# From project root
pip install -e ".[docs]"
mkdocs build
```

### 2. Deploy Marketing Website
Upload the contents of `/website/` to your web server root:
```
/website/
├── index.html
├── style.css
├── robots.txt
├── sitemap.xml
├── assets/
├── features/
├── use-cases/
├── getting-started.html
└── status.html
```

### 3. Deploy Documentation
Upload the contents of `/site/` (built by MkDocs) to `/docs/` on your server:
```
/docs/ (on server) ← /site/ (built locally)
```

### 4. Server Configuration

For nginx:
```nginx
server {
    server_name fraiseql.dev;
    root /var/www/fraiseql;

    location / {
        try_files $uri $uri/ /index.html;
    }

    location /docs {
        alias /var/www/fraiseql/docs;
        try_files $uri $uri/ /docs/index.html;
    }
}
```

For Apache:
```apache
<VirtualHost *:80>
    ServerName fraiseql.dev
    DocumentRoot /var/www/fraiseql

    <Directory /var/www/fraiseql>
        Options Indexes FollowSymLinks
        AllowOverride All
        Require all granted
    </Directory>

    Alias /docs /var/www/fraiseql/docs
</VirtualHost>
```

## Updates

To update the website:
1. Make changes to files in `/website/` or `/docs/`
2. If docs changed, rebuild with `mkdocs build`
3. Upload changed files to your server

## Assets

The website includes these visual assets:
- `/assets/architecture-diagram.svg` - System architecture
- `/assets/performance-chart.svg` - Performance comparison
- `/assets/query-flow.svg` - Query translation flow
- `/assets/og-image.svg` - Social media preview

## SEO Files

- `robots.txt` - Search engine instructions
- `sitemap.xml` - Site structure for search engines
- Meta tags and structured data in HTML files