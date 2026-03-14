# Multi-Cloud Federation Example

Deploy FraiseQL federation across AWS, GCP, and Azure with data locality and no vendor lock-in.

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│              Apollo Router/Federation Gateway              │
│                                                             │
└────────┬──────────────────────┬─────────────┬──────────────┘
         │                      │             │
    ┌────▼────┐          ┌──────▼──┐    ┌─────▼─────┐
    │  AWS    │          │   GCP   │    │   Azure   │
    │us-east  │          │eu-west  │    │southeast  │
    └────┬────┘          └──────┬──┘    └─────┬─────┘
         │                      │             │
    ┌────▼────────────┐   ┌─────▼──────┐   ┌─▼──────────┐
    │Users Service    │   │Orders      │   │Products    │
    │PostgreSQL       │   │Service     │   │Service     │
    │                 │   │PostgreSQL  │   │SQL Server  │
    │(Port 4001)      │   │(Port 4002) │   │(Port 4003) │
    └─────────────────┘   └────────────┘   └────────────┘
```

## Key Features

- **Data Locality**: Users stay in US, Orders in EU, Products in APAC
- **No Vendor Lock-in**: Replace any cloud provider without schema changes
- **Cost Transparency**: Pay each cloud provider directly
- **Single Schema Definition**: One GraphQL schema, deployed to three clouds
- **Performance Optimized**: Latency depends on federation paths, not cloud hops

## Components

### AWS us-east (Users Service)

- **Provider**: AWS
- **Database**: RDS PostgreSQL 16
- **Instance**: db.t3.micro (dev) or db.t3.small (prod)
- **Entity**: User (owns, not extended)
- **Storage**: US region only
- **Cost**: ~$10-50/month

### GCP eu-west (Orders Service)

- **Provider**: Google Cloud Platform
- **Database**: Cloud SQL PostgreSQL 16
- **Instance**: db-f1-micro (dev) or db-custom (prod)
- **Entity**: Order (owns, extends User)
- **Storage**: EU region only
- **Cost**: ~$12-60/month

### Azure southeast-asia (Products Service)

- **Provider**: Microsoft Azure
- **Database**: Azure Database for PostgreSQL or SQL Server
- **Instance**: B_Gen5_1 (dev) or B_Gen5_2 (prod)
- **Entity**: Product (owns)
- **Storage**: APAC region only
- **Cost**: ~$15-70/month

## Single Cloud Development

For local development without cloud accounts, use Docker Compose:

```bash
cd docker-local
docker-compose up -d

# Should see 3 services running locally
docker-compose ps
```

This creates:

- 3 local PostgreSQL databases
- 3 FraiseQL instances on ports 4001-4003
- Apollo Router on port 4000

## Multi-Cloud Deployment

### Prerequisites

You'll need accounts and CLIs installed:

```bash
# AWS
aws --version                  # AWS CLI v2+
aws configure

# GCP
gcloud --version              # Google Cloud SDK
gcloud auth login
gcloud config set project PROJECT_ID

# Azure
az --version                  # Azure CLI 2.40+
az login
```

### Step 1: Deploy to AWS

```bash
cd deployment/aws
./deploy.sh users-service us-east-1
```

**What this does:**

1. Creates RDS PostgreSQL instance
2. Initializes users schema
3. Builds Docker image
4. Pushes to ECR
5. Deploys to ECS
6. Returns endpoint URL

**Expected output:**

```
✅ RDS instance created: users-db-xxx.us-east-1.rds.amazonaws.com
✅ Docker image pushed: 123456789.dkr.ecr.us-east-1.amazonaws.com/fraiseql-users:latest
✅ Service deployed: http://users-service.us-east-1.elb.amazonaws.com
```

**Save the endpoint** for configuration.

### Step 2: Deploy to GCP

```bash
cd deployment/gcp
./deploy.sh orders-service europe-west1
```

**What this does:**

1. Creates Cloud SQL PostgreSQL instance
2. Initializes orders schema
3. Builds Docker image
4. Pushes to Artifact Registry
5. Deploys to Cloud Run
6. Returns endpoint URL

**Expected output:**

```
✅ Cloud SQL instance created: orders-db-xxx.c.PROJECT_ID.cloudsql.iam.gserviceaccount.com
✅ Docker image pushed: europe-west1-docker.pkg.dev/PROJECT/fraiseql/orders-service:latest
✅ Service deployed: https://orders-service-xxx-ew.a.run.app
```

### Step 3: Deploy to Azure

```bash
cd deployment/azure
./deploy.sh products-service southeastasia
```

**What this does:**

1. Creates Azure Database for PostgreSQL server
2. Initializes products schema
3. Builds Docker image
4. Pushes to Container Registry
5. Deploys to Container Instances
6. Returns endpoint URL

**Expected output:**

```
✅ PostgreSQL server created: products-db-xxx.postgres.database.azure.com
✅ Docker image pushed: fraiseqlregistry.azurecr.io/fraiseql-products:latest
✅ Service deployed: http://fraiseql-products.southeastasia.azurecontainer.io
```

### Step 4: Deploy Federation Gateway

```bash
# Use Apollo Router or federation gateway
./deployment/gateway/deploy.sh

# Provide subgraph endpoints when prompted:
# Users endpoint: http://users-service.us-east-1.elb.amazonaws.com
# Orders endpoint: https://orders-service-xxx-ew.a.run.app
# Products endpoint: http://fraiseql-products.southeastasia.azurecontainer.io
```

The gateway will:

1. Discover each subgraph's schema
2. Compose a federated schema
3. Create a unified GraphQL endpoint
4. Route queries to appropriate services

## Test Queries

### Single Service (AWS)

```bash
curl -X POST https://users-service.us-east-1.elb.amazonaws.com/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users { id name } }"
  }'
```

### Cross-Service (AWS → GCP)

```bash
curl -X POST https://gateway.example.com/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users { id orders { id status } } }"
  }'
```

### Three-Service (AWS → GCP → Azure)

```bash
curl -X POST https://gateway.example.com/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { users { id name orders { id total products { id name } } } }"
  }'
```

## Expected Performance

| Scenario | Latency | Notes |
|----------|---------|-------|
| Single service (AWS) | <10ms | Local RDS query |
| Cross-cloud (AWS+GCP) | 50-150ms | Transatlantic latency |
| Three-cloud (all) | 100-300ms | 2x federation hops |
| Batch 1000 users | <50ms | Batched local query |

## Cost Estimation (Monthly)

| Cloud | Service | Component | Estimate |
|-------|---------|-----------|----------|
| AWS | Users | RDS db.t3.micro + ECS | $15-40 |
| GCP | Orders | Cloud SQL f1-micro + Cloud Run | $12-50 |
| Azure | Products | B_Gen5_1 server | $15-70 |
| **Total** | All | Infrastructure | **$42-160** |

**Plus egress:**

- AWS to GCP: $0.02/GB egress
- GCP to Azure: $0.12/GB egress (GCP Premium egress)
- Azure to AWS: $0.02/GB egress

For typical SaaS (1TB/month federation traffic):

- US-EU-APAC triangle: ~$50-100/month egress

## Troubleshooting

### Service won't start

**AWS:**

```bash
aws ecs describe-services --cluster fraiseql --services users-service
aws ecs describe-task-definition --task-definition fraiseql-users:1
aws logs tail /ecs/fraiseql-users --follow
```

**GCP:**

```bash
gcloud run services describe orders-service --region europe-west1
gcloud run services logs orders-service --region europe-west1 --tail
```

**Azure:**

```bash
az container logs --resource-group fraiseql --name products-service
az container show --resource-group fraiseql --name products-service
```

### Slow queries

Check inter-cloud latency:

```bash
# AWS to GCP
ping orders-service-xxx-ew.a.run.app

# GCP to Azure
ping fraiseql-products.southeastasia.azurecontainer.io

# Check federation planning
curl https://gateway.example.com/graphql -d '{"query": "{ __typename }"}'
```

### Database connectivity

Test connections from services:

```bash
# From AWS service
aws ecs execute-command --cluster fraiseql --task <task-id> \
  --container users-service \
  --command "psql postgres://user:pass@db.rds.amazonaws.com/users -c 'SELECT 1'"

# From GCP service
gcloud run services update orders-service --region europe-west1 \
  --command "psql postgres://user:pass@cloud-sql/orders -c 'SELECT 1'"

# From Azure service
az container exec --resource-group fraiseql --name products-service \
  --command-line "psql postgres://user:pass@db.postgres.azure.com/products"
```

## Cleanup

To avoid ongoing charges, delete all resources:

```bash
# AWS
aws ecs delete-service --cluster fraiseql --service users-service --force
aws ec2 delete-instances --instance-ids <instance-ids>
aws rds delete-db-instance --db-instance-identifier users-db --skip-final-snapshot

# GCP
gcloud run services delete orders-service --region europe-west1
gcloud sql instances delete orders-db
gcloud container images delete europe-west1-docker.pkg.dev/PROJECT/fraiseql/orders-service

# Azure
az container delete --resource-group fraiseql --name products-service
az postgres server delete --resource-group fraiseql --name products-db
az acr repository delete --name fraiseqlregistry --repository fraiseql/products
```

## Next Steps

1. **Test Locally First**: Use `docker-local/` Docker Compose setup
2. **Deploy Single Cloud**: Start with AWS, then add GCP, then Azure
3. **Monitor Performance**: Track latency per federation path
4. **Optimize Data**: Consider caching layers between clouds
5. **Scale Services**: Add more replicas as traffic grows

## Multi-Cloud Best Practices

- **Data Locality**: Keep data near users
- **Eventual Consistency**: Accept federation latency
- **Cost Monitoring**: Track per-cloud spending
- **Backup Strategy**: Backup each cloud independently
- **Disaster Recovery**: Plan for single cloud outage
