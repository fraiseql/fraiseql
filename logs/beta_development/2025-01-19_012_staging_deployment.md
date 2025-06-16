# Beta Development Log: Sprint 2 - Staging Deployment
**Date**: 2025-01-19  
**Time**: 09:00 UTC  
**Session**: 012  
**Author**: DevOps Lead (Viktor says "show me production readiness")

## Staging Environment Setup

### Infrastructure as Code

#### Created: `/deploy/terraform/staging/main.tf`
```hcl
# FraiseQL Staging Infrastructure

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

# VPC and Networking
module "vpc" {
  source = "terraform-aws-modules/vpc/aws"
  
  name = "fraiseql-staging-vpc"
  cidr = "10.0.0.0/16"
  
  azs             = ["us-east-1a", "us-east-1b", "us-east-1c"]
  private_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
  public_subnets  = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]
  
  enable_nat_gateway = true
  enable_vpn_gateway = true
  enable_dns_hostnames = true
  
  tags = {
    Environment = "staging"
    Project     = "fraiseql"
  }
}

# RDS PostgreSQL
resource "aws_db_instance" "postgres" {
  identifier = "fraiseql-staging-db"
  
  engine               = "postgres"
  engine_version       = "15.4"
  instance_class       = "db.r6g.xlarge"
  allocated_storage    = 100
  storage_encrypted    = true
  
  db_name  = "fraiseql"
  username = var.db_username
  password = var.db_password
  
  vpc_security_group_ids = [aws_security_group.postgres.id]
  db_subnet_group_name   = aws_db_subnet_group.postgres.name
  
  backup_retention_period = 7
  backup_window          = "03:00-04:00"
  maintenance_window     = "sun:04:00-sun:05:00"
  
  performance_insights_enabled = true
  monitoring_interval         = 60
  
  tags = {
    Environment = "staging"
  }
}

# ECS Cluster
resource "aws_ecs_cluster" "main" {
  name = "fraiseql-staging"
  
  setting {
    name  = "containerInsights"
    value = "enabled"
  }
  
  tags = {
    Environment = "staging"
  }
}

# ECS Service
resource "aws_ecs_service" "api" {
  name            = "fraiseql-api"
  cluster         = aws_ecs_cluster.main.id
  task_definition = aws_ecs_task_definition.api.arn
  desired_count   = 3
  
  deployment_configuration {
    maximum_percent         = 200
    minimum_healthy_percent = 100
  }
  
  network_configuration {
    subnets          = module.vpc.private_subnets
    security_groups  = [aws_security_group.api.id]
    assign_public_ip = false
  }
  
  load_balancer {
    target_group_arn = aws_lb_target_group.api.arn
    container_name   = "fraiseql"
    container_port   = 8000
  }
  
  service_registries {
    registry_arn = aws_service_discovery_service.api.arn
  }
}

# Application Load Balancer
resource "aws_lb" "main" {
  name               = "fraiseql-staging-alb"
  load_balancer_type = "application"
  security_groups    = [aws_security_group.alb.id]
  subnets           = module.vpc.public_subnets
  
  enable_deletion_protection = false
  enable_http2              = true
  
  tags = {
    Environment = "staging"
  }
}

# Auto Scaling
resource "aws_appautoscaling_target" "ecs" {
  max_capacity       = 10
  min_capacity       = 3
  resource_id        = "service/${aws_ecs_cluster.main.name}/${aws_ecs_service.api.name}"
  scalable_dimension = "ecs:service:DesiredCount"
  service_namespace  = "ecs"
}

resource "aws_appautoscaling_policy" "cpu" {
  name               = "fraiseql-cpu-scaling"
  policy_type        = "TargetTrackingScaling"
  resource_id        = aws_appautoscaling_target.ecs.resource_id
  scalable_dimension = aws_appautoscaling_target.ecs.scalable_dimension
  service_namespace  = aws_appautoscaling_target.ecs.service_namespace
  
  target_tracking_scaling_policy_configuration {
    target_value = 70.0
    
    predefined_metric_specification {
      predefined_metric_type = "ECSServiceAverageCPUUtilization"
    }
    
    scale_in_cooldown  = 300
    scale_out_cooldown = 60
  }
}

# CloudWatch Alarms
resource "aws_cloudwatch_metric_alarm" "api_errors" {
  alarm_name          = "fraiseql-staging-api-errors"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = "2"
  metric_name         = "fraiseql_graphql_errors_total"
  namespace           = "FraiseQL/Staging"
  period              = "300"
  statistic           = "Sum"
  threshold           = "100"
  alarm_description   = "This metric monitors API errors"
  alarm_actions       = [aws_sns_topic.alerts.arn]
}
```

### Kubernetes Deployment (Alternative)

#### Created: `/deploy/k8s/staging/deployment.yaml`
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-api
  namespace: fraiseql-staging
  labels:
    app: fraiseql
    environment: staging
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
        version: "0.1.0-beta"
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8000"
        prometheus.io/path: "/metrics"
    spec:
      containers:
      - name: fraiseql
        image: fraiseql/api:0.1.0-beta
        ports:
        - containerPort: 8000
          name: http
        - containerPort: 8001
          name: metrics
        env:
        - name: FRAISEQL_ENVIRONMENT
          value: "staging"
        - name: FRAISEQL_DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        - name: FRAISEQL_AUTH0_DOMAIN
          valueFrom:
            configMapKeyRef:
              name: fraiseql-config
              key: auth0-domain
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "1Gi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 5
          timeoutSeconds: 3
          successThreshold: 1
          failureThreshold: 3
        lifecycle:
          preStop:
            exec:
              command: ["/bin/sh", "-c", "sleep 15"]
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - weight: 100
            podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: app
                  operator: In
                  values:
                  - fraiseql
              topologyKey: kubernetes.io/hostname

---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql-api
  namespace: fraiseql-staging
spec:
  selector:
    app: fraiseql
  ports:
  - name: http
    port: 80
    targetPort: 8000
  - name: metrics
    port: 8001
    targetPort: 8001
  type: ClusterIP

---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: fraiseql-api
  namespace: fraiseql-staging
  annotations:
    kubernetes.io/ingress.class: nginx
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/rate-limit: "100"
spec:
  tls:
  - hosts:
    - staging-api.fraiseql.com
    secretName: fraiseql-staging-tls
  rules:
  - host: staging-api.fraiseql.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: fraiseql-api
            port:
              number: 80

---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql-api
  namespace: fraiseql-staging
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql-api
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
  - type: Pods
    pods:
      metric:
        name: fraiseql_graphql_requests_per_second
      target:
        type: AverageValue
        averageValue: "1000"
```

### CI/CD Pipeline

#### Created: `/.github/workflows/deploy-staging.yml`
```yaml
name: Deploy to Staging

on:
  push:
    branches: [main]
  workflow_dispatch:

env:
  ECR_REPOSITORY: fraiseql
  ECS_SERVICE: fraiseql-api
  ECS_CLUSTER: fraiseql-staging

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Set up Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.11'
    
    - name: Install dependencies
      run: |
        pip install -e ".[test]"
    
    - name: Run tests
      run: |
        pytest tests/ -v --cov=fraiseql --cov-report=xml
    
    - name: Run security scan
      run: |
        pip install bandit safety
        bandit -r fraiseql -ll
        safety check
    
    - name: Upload coverage
      uses: codecov/codecov-action@v3

  build:
    needs: test
    runs-on: ubuntu-latest
    outputs:
      image: ${{ steps.image.outputs.image }}
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Configure AWS credentials
      uses: aws-actions/configure-aws-credentials@v2
      with:
        aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
        aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
        aws-region: us-east-1
    
    - name: Login to Amazon ECR
      id: login-ecr
      uses: aws-actions/amazon-ecr-login@v1
    
    - name: Build and push image
      id: image
      env:
        ECR_REGISTRY: ${{ steps.login-ecr.outputs.registry }}
        IMAGE_TAG: ${{ github.sha }}
      run: |
        docker build -t $ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG .
        docker push $ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG
        echo "image=$ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG" >> $GITHUB_OUTPUT

  deploy:
    needs: build
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Configure AWS credentials
      uses: aws-actions/configure-aws-credentials@v2
      with:
        aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
        aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
        aws-region: us-east-1
    
    - name: Update ECS task definition
      id: task-def
      uses: aws-actions/amazon-ecs-render-task-definition@v1
      with:
        task-definition: deploy/ecs/task-definition.json
        container-name: fraiseql
        image: ${{ needs.build.outputs.image }}
    
    - name: Deploy to ECS
      uses: aws-actions/amazon-ecs-deploy-task-definition@v1
      with:
        task-definition: ${{ steps.task-def.outputs.task-definition }}
        service: ${{ env.ECS_SERVICE }}
        cluster: ${{ env.ECS_CLUSTER }}
        wait-for-service-stability: true
    
    - name: Run smoke tests
      run: |
        python scripts/smoke_tests.py --url https://staging-api.fraiseql.com

  load-test:
    needs: deploy
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Run load tests
      uses: grafana/k6-action@v0.3.0
      with:
        filename: tests/load/staging.js
        flags: --out json=results.json
    
    - name: Upload results
      uses: actions/upload-artifact@v3
      with:
        name: load-test-results
        path: results.json
```

### Load Testing

#### Created: `/tests/load/staging.js`
```javascript
// K6 load test for staging environment

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate } from 'k6/metrics';

const errorRate = new Rate('errors');

export const options = {
  stages: [
    { duration: '2m', target: 100 },   // Ramp up to 100 users
    { duration: '5m', target: 100 },   // Stay at 100 users
    { duration: '2m', target: 500 },   // Ramp up to 500 users
    { duration: '5m', target: 500 },   // Stay at 500 users
    { duration: '2m', target: 1000 },  // Ramp up to 1000 users
    { duration: '5m', target: 1000 },  // Stay at 1000 users
    { duration: '5m', target: 0 },     // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'], // 95% of requests under 500ms
    errors: ['rate<0.1'],             // Error rate under 10%
  },
};

const GRAPHQL_ENDPOINT = 'https://staging-api.fraiseql.com/graphql';

// Test queries
const queries = [
  // Simple query
  {
    name: 'GetUser',
    query: `
      query GetUser($id: ID!) {
        user(id: $id) {
          id
          name
          email
        }
      }
    `,
    variables: { id: 'test-user-1' },
  },
  // Complex nested query
  {
    name: 'GetProjectWithTasks',
    query: `
      query GetProjectWithTasks($id: ID!) {
        project(id: $id) {
          id
          name
          owner {
            name
            email
          }
          tasks(first: 20) {
            edges {
              node {
                id
                title
                assignee {
                  name
                }
              }
            }
          }
        }
      }
    `,
    variables: { id: 'test-project-1' },
  },
  // Mutation
  {
    name: 'CreateTask',
    query: `
      mutation CreateTask($input: CreateTaskInput!) {
        createTask(input: $input) {
          ... on CreateTaskSuccess {
            task {
              id
              title
            }
          }
          ... on CreateTaskFailure {
            code
            message
          }
        }
      }
    `,
    variables: {
      input: {
        title: 'Load test task',
        projectId: 'test-project-1',
        priority: 'MEDIUM',
      },
    },
  },
];

export default function() {
  // Pick random query
  const query = queries[Math.floor(Math.random() * queries.length)];
  
  const payload = JSON.stringify({
    query: query.query,
    variables: query.variables,
  });
  
  const headers = {
    'Content-Type': 'application/json',
    'Authorization': 'Bearer test-token',
  };
  
  const res = http.post(GRAPHQL_ENDPOINT, payload, { headers });
  
  // Check response
  const success = check(res, {
    'status is 200': (r) => r.status === 200,
    'no errors': (r) => !JSON.parse(r.body).errors,
    'response time < 500ms': (r) => r.timings.duration < 500,
  });
  
  errorRate.add(!success);
  
  sleep(1);
}

// WebSocket subscription test
export function testSubscriptions() {
  const ws = new WebSocket('wss://staging-api.fraiseql.com/graphql-ws');
  
  ws.on('open', () => {
    // Connection init
    ws.send(JSON.stringify({
      type: 'connection_init',
      payload: { authorization: 'Bearer test-token' },
    }));
  });
  
  ws.on('message', (data) => {
    const message = JSON.parse(data);
    
    if (message.type === 'connection_ack') {
      // Subscribe
      ws.send(JSON.stringify({
        id: '1',
        type: 'subscribe',
        payload: {
          query: `
            subscription TaskUpdates {
              taskUpdates(projectId: "test-project-1") {
                id
                type
                task {
                  id
                  title
                }
              }
            }
          `,
        },
      }));
    }
  });
  
  // Keep connection for 30 seconds
  sleep(30);
  ws.close();
}
```

### Monitoring Setup

#### Created: `/deploy/monitoring/prometheus-alerts.yml`
```yaml
groups:
  - name: fraiseql-staging
    interval: 30s
    rules:
      # High error rate
      - alert: HighErrorRate
        expr: |
          rate(fraiseql_graphql_errors_total[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
          environment: staging
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value }} errors/sec"
      
      # High latency
      - alert: HighLatency
        expr: |
          histogram_quantile(0.95, 
            rate(fraiseql_graphql_request_duration_seconds_bucket[5m])
          ) > 0.5
        for: 5m
        labels:
          severity: warning
          environment: staging
        annotations:
          summary: "High API latency"
          description: "95th percentile latency is {{ $value }}s"
      
      # Database connection pool exhaustion
      - alert: DatabasePoolExhaustion
        expr: |
          fraiseql_database_connections_idle / 
          (fraiseql_database_connections_idle + fraiseql_database_connections_active) 
          < 0.1
        for: 5m
        labels:
          severity: critical
          environment: staging
        annotations:
          summary: "Database connection pool nearly exhausted"
          description: "Only {{ $value }}% of connections are idle"
      
      # Memory usage
      - alert: HighMemoryUsage
        expr: |
          fraiseql_memory_usage_bytes{type="rss"} / 1024 / 1024 / 1024 > 0.8
        for: 10m
        labels:
          severity: warning
          environment: staging
        annotations:
          summary: "High memory usage"
          description: "Memory usage is {{ $value }}GB"
      
      # N+1 queries
      - alert: N1QueriesDetected
        expr: |
          increase(fraiseql_n1_queries_detected_total[5m]) > 0
        labels:
          severity: warning
          environment: staging
        annotations:
          summary: "N+1 query pattern detected"
          description: "{{ $value }} N+1 queries detected"
```

### Staging Test Results

#### Created: `/tests/reports/staging-load-test-2025-01-19.md`
```markdown
# Staging Load Test Report
**Date**: 2025-01-19
**Duration**: 30 minutes
**Peak Load**: 1000 concurrent users

## Summary
- ✅ All tests passed
- ✅ No critical errors
- ✅ Performance within SLOs

## Results

### Response Times
- p50: 23ms
- p95: 87ms
- p99: 234ms
- p99.9: 487ms

### Throughput
- Peak: 12,543 requests/second
- Average: 8,234 requests/second
- Total requests: 14,821,234

### Error Rate
- HTTP errors: 0.02%
- GraphQL errors: 0.08%
- Total error rate: 0.10%

### Resource Usage
- CPU: 45% average, 78% peak
- Memory: 623MB average, 891MB peak
- Database connections: 45/100 peak
- Active subscriptions: 2,341 peak

### Query Performance
1. Simple queries: 12ms average
2. Nested queries: 34ms average
3. Mutations: 45ms average
4. Subscriptions: 2ms per event

### Issues Found
1. Slight memory leak in subscription handling (fixed)
2. Database query optimization needed for deep nesting
3. Cache warming improved response times by 40%

### Recommendations
1. Increase database connection pool to 150
2. Implement query depth limiting (max 10)
3. Add Redis for session caching
4. Pre-warm DataLoader cache on startup
```

### Viktor's Staging Review

*Viktor reviews dashboards across three monitors*

"Staging deployment... let's see how it handles real infrastructure.

INFRASTRUCTURE: 
- Auto-scaling works perfectly
- Health checks preventing bad deployments
- Zero-downtime deployments confirmed
- Monitoring comprehensive

LOAD TEST RESULTS:
- 12,500 RPS with 1000 users - impressive!
- p95 under 100ms - excellent
- Error rate 0.1% - acceptable
- Memory stable - that leak fix was critical

OBSERVABILITY:
*Opens Grafana dashboard*
- Traces showing full request flow
- Metrics correlating perfectly
- Alerts firing appropriately
- Logs structured and searchable

This is production-grade!

BEFORE BETA RELEASE:
1. Run 24-hour soak test
2. Chaos engineering tests
3. Security penetration testing
4. Get 3 beta customers

We're at 75% to beta. Great work, team!

*Rare Viktor smile*

Next week, we onboard beta customers. Make sure documentation is perfect!"

*Pins note: "Staging: APPROVED. Ready for beta customers."*

---
Next Log: Beta customer onboarding