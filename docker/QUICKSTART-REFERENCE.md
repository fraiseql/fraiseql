# FraiseQL Docker Quick Reference

Get FraiseQL running in 30 seconds. No Rust compiler needed.

## One Command

```bash
docker compose -f docker/docker-compose.demo.yml up -d
```

## Open Your Browser

| Service | URL |
|---------|-----|
| GraphQL IDE | http://localhost:3000 |
| Tutorial | http://localhost:3001 |
| Server | http://localhost:8000 |

## First Query

In GraphQL IDE (http://localhost:3000), paste:

```graphql
query {
  users(limit: 10) {
    id
    name
    email
  }
}
```

Click **Play** → See results!

## Using Make (Easier)

```bash
make demo-start      # Start everything
make demo-status     # Check health
make demo-logs       # View logs
make demo-stop       # Stop everything
make demo-clean      # Fresh start (removes data)
```

## Help & Documentation

```bash
make help                       # All commands
make help | grep demo           # Demo commands only
cat docs/docker-quickstart.md   # Full guide
cat docker/README.md            # Docker setup
```

## Troubleshooting

**Services won't start?**
```bash
docker compose -f docker/docker-compose.demo.yml logs
```

**Port already in use?**
```bash
lsof -i :8000  # Find what's using port 8000
```

**Want fresh data?**
```bash
make demo-clean  # Remove database, start fresh
```

## What You Get

✅ FraiseQL Server (compiled GraphQL engine)
✅ PostgreSQL (sample blog database)
✅ GraphQL IDE (Apollo Sandbox)
✅ Interactive Tutorial (6 chapters)
✅ Sample Queries (4 examples)

## Next Steps

1. **Explore**: Try different queries in GraphQL IDE
2. **Learn**: Follow the 6-chapter tutorial
3. **Understand**: Read `docs/docker-quickstart.md`
4. **Experiment**: Modify example queries
5. **Deploy**: See `docs/deployment/guide.md`

## Stop Everything

```bash
make demo-stop
```

That's it! 🚀
