# Alwaysdata DNS Configuration for fraiseql.dev

## DNS Records to Configure

Login to your Alwaysdata account and add these DNS records:

### 1. Root Domain (fraiseql.dev)
- **Type:** A
- **Name:** @ (or leave empty)
- **Value:** 82.66.42.150
- **TTL:** 300 (5 minutes for testing, increase to 3600 later)

### 2. Root Domain IPv6
- **Type:** AAAA
- **Name:** @ (or leave empty)
- **Value:** 2a01:e0a:98:8962::20
- **TTL:** 300

### 3. WWW Subdomain
- **Type:** A
- **Name:** www
- **Value:** 82.66.42.150
- **TTL:** 300

### 4. WWW Subdomain IPv6
- **Type:** AAAA
- **Name:** www
- **Value:** 2a01:e0a:98:8962::20
- **TTL:** 300

## Steps in Alwaysdata Interface

1. Go to https://admin.alwaysdata.com
2. Navigate to **Domains** → **DNS**
3. Select fraiseql.dev domain
4. Click **Add a record** for each entry above
5. Save changes

## Verify DNS Propagation

After configuring, run:
```bash
./deploy/check-dns.sh
```

DNS propagation typically takes 5-30 minutes. Once the DNS is pointing correctly, proceed with deployment.
