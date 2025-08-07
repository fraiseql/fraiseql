# AWS Deployment Guide

## Overview

Deploy FraiseQL on Amazon Web Services using ECS Fargate for serverless containers, RDS for managed PostgreSQL, and Application Load Balancer for high availability.

## Architecture

```mermaid
graph TB
    subgraph "Route 53"
        DNS[DNS]
    end

    subgraph "CloudFront"
        CDN[CDN Distribution]
    end

    subgraph "VPC"
        subgraph "Public Subnets"
            ALB[Application Load Balancer]
        end

        subgraph "Private Subnets"
            subgraph "ECS Fargate"
                TASK1[Task 1]
                TASK2[Task 2]
                TASK3[Task N]
            end

            subgraph "Data Layer"
                RDS[(RDS PostgreSQL)]
                REDIS[(ElastiCache Redis)]
            end
        end
    end

    subgraph "Storage"
        S3[S3 Buckets]
        SECRETS[Secrets Manager]
    end

    subgraph "Monitoring"
        CW[CloudWatch]
        XRAY[X-Ray]
    end

    DNS --> CDN
    CDN --> ALB
    ALB --> TASK1
    ALB --> TASK2
    ALB --> TASK3

    TASK1 --> RDS
    TASK1 --> REDIS
    TASK2 --> RDS
    TASK2 --> REDIS
    TASK3 --> RDS
    TASK3 --> REDIS

    TASK1 --> SECRETS
    TASK2 --> SECRETS
    TASK3 --> SECRETS

    TASK1 --> CW
    TASK2 --> CW
    TASK3 --> CW
```

## Prerequisites

- AWS Account with appropriate permissions
- AWS CLI configured
- Docker installed locally
- Domain name (optional)

## Quick Start with AWS Copilot

```bash
# Install Copilot
curl -Lo copilot https://github.com/aws/copilot-cli/releases/latest/download/copilot-linux
chmod +x copilot
sudo mv copilot /usr/local/bin/copilot

# Initialize application
copilot app init fraiseql

# Deploy environment
copilot env init --name production
copilot env deploy --name production

# Deploy service
copilot svc init --name api
copilot svc deploy --name api --env production
```

## ECS Fargate Deployment

### Task Definition

```json
{
  "family": "fraiseql-task",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "1024",
  "memory": "2048",
  "taskRoleArn": "arn:aws:iam::ACCOUNT:role/fraiseql-task-role",
  "executionRoleArn": "arn:aws:iam::ACCOUNT:role/fraiseql-execution-role",
  "containerDefinitions": [
    {
      "name": "fraiseql",
      "image": "ACCOUNT.dkr.ecr.REGION.amazonaws.com/fraiseql:latest",
      "essential": true,
      "portMappings": [
        {
          "containerPort": 8000,
          "protocol": "tcp"
        }
      ],
      "environment": [
        {
          "name": "FRAISEQL_MODE",
          "value": "production"
        },
        {
          "name": "AWS_REGION",
          "value": "us-east-1"
        }
      ],
      "secrets": [
        {
          "name": "DATABASE_URL",
          "valueFrom": "arn:aws:secretsmanager:REGION:ACCOUNT:secret:fraiseql/db-XXXXX"
        },
        {
          "name": "SECRET_KEY",
          "valueFrom": "arn:aws:secretsmanager:REGION:ACCOUNT:secret:fraiseql/app-XXXXX"
        },
        {
          "name": "REDIS_URL",
          "valueFrom": "arn:aws:secretsmanager:REGION:ACCOUNT:secret:fraiseql/redis-XXXXX"
        }
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/fraiseql",
          "awslogs-region": "us-east-1",
          "awslogs-stream-prefix": "ecs"
        }
      },
      "healthCheck": {
        "command": ["CMD-SHELL", "curl -f http://localhost:8000/health || exit 1"],
        "interval": 30,
        "timeout": 5,
        "retries": 3,
        "startPeriod": 60
      }
    }
  ]
}
```

### ECS Service

```yaml
# ecs-service.yaml
apiVersion: v1
kind: Service
metadata:
  name: fraiseql-service
spec:
  cluster: fraiseql-cluster
  taskDefinition: fraiseql-task:latest
  desiredCount: 3
  launchType: FARGATE
  networkConfiguration:
    awsvpcConfiguration:
      subnets:
        - subnet-xxx
        - subnet-yyy
      securityGroups:
        - sg-fraiseql-app
      assignPublicIp: DISABLED
  loadBalancers:
    - targetGroupArn: arn:aws:elasticloadbalancing:REGION:ACCOUNT:targetgroup/fraiseql-tg
      containerName: fraiseql
      containerPort: 8000
  healthCheckGracePeriodSeconds: 60
  deploymentConfiguration:
    maximumPercent: 200
    minimumHealthyPercent: 100
    deploymentCircuitBreaker:
      enable: true
      rollback: true
```

## RDS PostgreSQL Setup

### CloudFormation Template

```yaml
# rds-postgres.yaml
AWSTemplateFormatVersion: '2010-09-09'
Description: RDS PostgreSQL for FraiseQL

Parameters:
  DBPassword:
    Type: String
    NoEcho: true
    Description: Database master password

Resources:
  DBSubnetGroup:
    Type: AWS::RDS::DBSubnetGroup
    Properties:
      DBSubnetGroupDescription: Subnet group for FraiseQL RDS
      SubnetIds:
        - !Ref PrivateSubnet1
        - !Ref PrivateSubnet2
      Tags:
        - Key: Name
          Value: fraiseql-db-subnet-group

  DBSecurityGroup:
    Type: AWS::EC2::SecurityGroup
    Properties:
      GroupDescription: Security group for FraiseQL RDS
      VpcId: !Ref VPC
      SecurityGroupIngress:
        - IpProtocol: tcp
          FromPort: 5432
          ToPort: 5432
          SourceSecurityGroupId: !Ref AppSecurityGroup
      Tags:
        - Key: Name
          Value: fraiseql-db-sg

  DBInstance:
    Type: AWS::RDS::DBInstance
    Properties:
      DBInstanceIdentifier: fraiseql-db
      DBName: fraiseql
      Engine: postgres
      EngineVersion: '15.4'
      DBInstanceClass: db.t3.medium
      AllocatedStorage: 100
      StorageType: gp3
      StorageEncrypted: true
      Iops: 3000
      MasterUsername: fraiseql_admin
      MasterUserPassword: !Ref DBPassword
      VPCSecurityGroups:
        - !Ref DBSecurityGroup
      DBSubnetGroupName: !Ref DBSubnetGroup
      BackupRetentionPeriod: 30
      PreferredBackupWindow: "03:00-04:00"
      PreferredMaintenanceWindow: "sun:04:00-sun:05:00"
      EnablePerformanceInsights: true
      PerformanceInsightsRetentionPeriod: 7
      MonitoringInterval: 60
      MonitoringRoleArn: !GetAtt DBMonitoringRole.Arn
      EnableCloudwatchLogsExports:
        - postgresql
      DeletionProtection: true
      Tags:
        - Key: Name
          Value: fraiseql-database
        - Key: Environment
          Value: production

  DBReadReplica:
    Type: AWS::RDS::DBInstance
    Properties:
      DBInstanceIdentifier: fraiseql-db-read
      SourceDBInstanceIdentifier: !Ref DBInstance
      DBInstanceClass: db.t3.small
      PubliclyAccessible: false
      Tags:
        - Key: Name
          Value: fraiseql-database-read
        - Key: Environment
          Value: production

Outputs:
  DBEndpoint:
    Description: Database endpoint
    Value: !GetAtt DBInstance.Endpoint.Address
    Export:
      Name: fraiseql-db-endpoint

  DBReadEndpoint:
    Description: Read replica endpoint
    Value: !GetAtt DBReadReplica.Endpoint.Address
    Export:
      Name: fraiseql-db-read-endpoint
```

## ElastiCache Redis Setup

```yaml
# elasticache-redis.yaml
Resources:
  RedisSubnetGroup:
    Type: AWS::ElastiCache::SubnetGroup
    Properties:
      Description: Subnet group for FraiseQL Redis
      SubnetIds:
        - !Ref PrivateSubnet1
        - !Ref PrivateSubnet2

  RedisSecurityGroup:
    Type: AWS::EC2::SecurityGroup
    Properties:
      GroupDescription: Security group for FraiseQL Redis
      VpcId: !Ref VPC
      SecurityGroupIngress:
        - IpProtocol: tcp
          FromPort: 6379
          ToPort: 6379
          SourceSecurityGroupId: !Ref AppSecurityGroup

  RedisParameterGroup:
    Type: AWS::ElastiCache::ParameterGroup
    Properties:
      CacheParameterGroupFamily: redis7
      Description: FraiseQL Redis parameters
      Properties:
        timeout: "300"
        maxmemory-policy: "allkeys-lru"

  RedisCluster:
    Type: AWS::ElastiCache::ReplicationGroup
    Properties:
      ReplicationGroupId: fraiseql-redis
      ReplicationGroupDescription: FraiseQL Redis cluster
      Engine: redis
      EngineVersion: 7.0.7
      CacheNodeType: cache.t3.small
      NumCacheClusters: 2
      AutomaticFailoverEnabled: true
      MultiAZEnabled: true
      CacheSubnetGroupName: !Ref RedisSubnetGroup
      SecurityGroupIds:
        - !Ref RedisSecurityGroup
      CacheParameterGroupName: !Ref RedisParameterGroup
      AtRestEncryptionEnabled: true
      TransitEncryptionEnabled: true
      SnapshotRetentionLimit: 7
      SnapshotWindow: "03:00-05:00"
      Tags:
        - Key: Name
          Value: fraiseql-redis
        - Key: Environment
          Value: production
```

## Application Load Balancer

```yaml
# alb.yaml
Resources:
  ApplicationLoadBalancer:
    Type: AWS::ElasticLoadBalancingV2::LoadBalancer
    Properties:
      Name: fraiseql-alb
      Type: application
      Scheme: internet-facing
      IpAddressType: ipv4
      Subnets:
        - !Ref PublicSubnet1
        - !Ref PublicSubnet2
      SecurityGroups:
        - !Ref ALBSecurityGroup
      Tags:
        - Key: Name
          Value: fraiseql-alb

  TargetGroup:
    Type: AWS::ElasticLoadBalancingV2::TargetGroup
    Properties:
      Name: fraiseql-tg
      Port: 8000
      Protocol: HTTP
      VpcId: !Ref VPC
      TargetType: ip
      HealthCheckEnabled: true
      HealthCheckPath: /health
      HealthCheckProtocol: HTTP
      HealthCheckIntervalSeconds: 30
      HealthCheckTimeoutSeconds: 5
      HealthyThresholdCount: 2
      UnhealthyThresholdCount: 3
      Matcher:
        HttpCode: 200
      TargetGroupAttributes:
        - Key: deregistration_delay.timeout_seconds
          Value: 30
        - Key: stickiness.enabled
          Value: true
        - Key: stickiness.type
          Value: app_cookie
        - Key: stickiness.app_cookie.duration_seconds
          Value: 86400

  HTTPSListener:
    Type: AWS::ElasticLoadBalancingV2::Listener
    Properties:
      LoadBalancerArn: !Ref ApplicationLoadBalancer
      Port: 443
      Protocol: HTTPS
      Certificates:
        - CertificateArn: !Ref Certificate
      DefaultActions:
        - Type: forward
          TargetGroupArn: !Ref TargetGroup

  HTTPListener:
    Type: AWS::ElasticLoadBalancingV2::Listener
    Properties:
      LoadBalancerArn: !Ref ApplicationLoadBalancer
      Port: 80
      Protocol: HTTP
      DefaultActions:
        - Type: redirect
          RedirectConfig:
            Protocol: HTTPS
            Port: 443
            StatusCode: HTTP_301
```

## Auto Scaling

```yaml
# auto-scaling.yaml
Resources:
  ServiceScalingTarget:
    Type: AWS::ApplicationAutoScaling::ScalableTarget
    Properties:
      MaxCapacity: 10
      MinCapacity: 2
      ResourceId: !Sub service/${ClusterName}/${ServiceName}
      RoleARN: !Sub arn:aws:iam::${AWS::AccountId}:role/aws-service-role/ecs.application-autoscaling.amazonaws.com/AWSServiceRoleForApplicationAutoScaling_ECSService
      ScalableDimension: ecs:service:DesiredCount
      ServiceNamespace: ecs

  ServiceScalingPolicy:
    Type: AWS::ApplicationAutoScaling::ScalingPolicy
    Properties:
      PolicyName: fraiseql-scaling-policy
      PolicyType: TargetTrackingScaling
      ScalingTargetId: !Ref ServiceScalingTarget
      TargetTrackingScalingPolicyConfiguration:
        PredefinedMetricSpecification:
          PredefinedMetricType: ECSServiceAverageCPUUtilization
        TargetValue: 70.0
        ScaleInCooldown: 300
        ScaleOutCooldown: 60
```

## Secrets Manager

```bash
# Create secrets
aws secretsmanager create-secret \
  --name fraiseql/db \
  --description "Database connection string" \
  --secret-string '{"url":"postgresql://user:pass@host:5432/db"}'

aws secretsmanager create-secret \
  --name fraiseql/app \
  --description "Application secrets" \
  --secret-string '{"secret_key":"your-secret-key","jwt_secret":"your-jwt-secret"}'

# Grant access to ECS task
aws iam attach-role-policy \
  --role-name fraiseql-task-role \
  --policy-arn arn:aws:iam::aws:policy/SecretsManagerReadWrite
```

## CloudWatch Monitoring

### Log Groups

```bash
# Create log groups
aws logs create-log-group --log-group-name /ecs/fraiseql
aws logs create-log-group --log-group-name /aws/rds/instance/fraiseql-db/postgresql

# Set retention
aws logs put-retention-policy \
  --log-group-name /ecs/fraiseql \
  --retention-in-days 30
```

### CloudWatch Alarms

```yaml
# cloudwatch-alarms.yaml
Resources:
  HighCPUAlarm:
    Type: AWS::CloudWatch::Alarm
    Properties:
      AlarmName: fraiseql-high-cpu
      AlarmDescription: Triggers when CPU utilization is too high
      MetricName: CPUUtilization
      Namespace: AWS/ECS
      Statistic: Average
      Period: 300
      EvaluationPeriods: 2
      Threshold: 80
      ComparisonOperator: GreaterThanThreshold
      AlarmActions:
        - !Ref SNSTopic

  HighMemoryAlarm:
    Type: AWS::CloudWatch::Alarm
    Properties:
      AlarmName: fraiseql-high-memory
      AlarmDescription: Triggers when memory utilization is too high
      MetricName: MemoryUtilization
      Namespace: AWS/ECS
      Statistic: Average
      Period: 300
      EvaluationPeriods: 2
      Threshold: 80
      ComparisonOperator: GreaterThanThreshold
      AlarmActions:
        - !Ref SNSTopic

  DatabaseConnectionsAlarm:
    Type: AWS::CloudWatch::Alarm
    Properties:
      AlarmName: fraiseql-db-connections
      AlarmDescription: Triggers when database connections are too high
      MetricName: DatabaseConnections
      Namespace: AWS/RDS
      Statistic: Average
      Period: 300
      EvaluationPeriods: 1
      Threshold: 80
      ComparisonOperator: GreaterThanThreshold
      AlarmActions:
        - !Ref SNSTopic
```

## CI/CD with CodePipeline

```yaml
# codepipeline.yaml
Resources:
  Pipeline:
    Type: AWS::CodePipeline::Pipeline
    Properties:
      Name: fraiseql-pipeline
      RoleArn: !GetAtt CodePipelineRole.Arn
      ArtifactStore:
        Type: S3
        Location: !Ref ArtifactBucket
      Stages:
        - Name: Source
          Actions:
            - Name: Source
              ActionTypeId:
                Category: Source
                Owner: ThirdParty
                Provider: GitHub
                Version: 1
              Configuration:
                Owner: your-org
                Repo: fraiseql
                Branch: main
                OAuthToken: !Ref GitHubToken
              OutputArtifacts:
                - Name: SourceOutput

        - Name: Build
          Actions:
            - Name: Build
              ActionTypeId:
                Category: Build
                Owner: AWS
                Provider: CodeBuild
                Version: 1
              Configuration:
                ProjectName: !Ref BuildProject
              InputArtifacts:
                - Name: SourceOutput
              OutputArtifacts:
                - Name: BuildOutput

        - Name: Deploy
          Actions:
            - Name: Deploy
              ActionTypeId:
                Category: Deploy
                Owner: AWS
                Provider: ECS
                Version: 1
              Configuration:
                ClusterName: fraiseql-cluster
                ServiceName: fraiseql-service
                FileName: imagedefinitions.json
              InputArtifacts:
                - Name: BuildOutput
```

## CodeBuild Project

```yaml
# buildspec.yml
version: 0.2

phases:
  pre_build:
    commands:
      - echo Logging in to Amazon ECR...
      - aws ecr get-login-password --region $AWS_DEFAULT_REGION | docker login --username AWS --password-stdin $AWS_ACCOUNT_ID.dkr.ecr.$AWS_DEFAULT_REGION.amazonaws.com
      - REPOSITORY_URI=$AWS_ACCOUNT_ID.dkr.ecr.$AWS_DEFAULT_REGION.amazonaws.com/fraiseql
      - COMMIT_HASH=$(echo $CODEBUILD_RESOLVED_SOURCE_VERSION | cut -c 1-7)
      - IMAGE_TAG=${COMMIT_HASH:=latest}

  build:
    commands:
      - echo Build started on `date`
      - echo Building the Docker image...
      - docker build -t $REPOSITORY_URI:latest .
      - docker tag $REPOSITORY_URI:latest $REPOSITORY_URI:$IMAGE_TAG

  post_build:
    commands:
      - echo Build completed on `date`
      - echo Pushing the Docker images...
      - docker push $REPOSITORY_URI:latest
      - docker push $REPOSITORY_URI:$IMAGE_TAG
      - echo Writing image definitions file...
      - printf '[{"name":"fraiseql","imageUri":"%s"}]' $REPOSITORY_URI:$IMAGE_TAG > imagedefinitions.json

artifacts:
  files: imagedefinitions.json
```

## Terraform Complete Example

```hcl
# main.tf
terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }

  backend "s3" {
    bucket = "fraiseql-terraform-state"
    key    = "prod/terraform.tfstate"
    region = "us-east-1"
  }
}

provider "aws" {
  region = var.aws_region
}

module "network" {
  source = "./modules/network"

  vpc_cidr = "10.0.0.0/16"
  availability_zones = ["us-east-1a", "us-east-1b"]
  public_subnet_cidrs = ["10.0.1.0/24", "10.0.2.0/24"]
  private_subnet_cidrs = ["10.0.10.0/24", "10.0.11.0/24"]
}

module "ecs" {
  source = "./modules/ecs"

  cluster_name = "fraiseql"
  vpc_id = module.network.vpc_id
  private_subnet_ids = module.network.private_subnet_ids
  alb_target_group_arn = module.alb.target_group_arn

  container_image = "${aws_ecr_repository.fraiseql.repository_url}:latest"
  container_cpu = 1024
  container_memory = 2048
  desired_count = 3

  database_url = module.rds.connection_string
  redis_url = module.elasticache.connection_string
}

module "rds" {
  source = "./modules/rds"

  vpc_id = module.network.vpc_id
  subnet_ids = module.network.private_subnet_ids
  security_group_id = module.ecs.app_security_group_id

  instance_class = "db.t3.medium"
  allocated_storage = 100
  database_name = "fraiseql"
  master_username = "fraiseql_admin"
  master_password = var.db_password
}

module "elasticache" {
  source = "./modules/elasticache"

  vpc_id = module.network.vpc_id
  subnet_ids = module.network.private_subnet_ids
  security_group_id = module.ecs.app_security_group_id

  node_type = "cache.t3.small"
  num_cache_nodes = 2
}

module "alb" {
  source = "./modules/alb"

  vpc_id = module.network.vpc_id
  public_subnet_ids = module.network.public_subnet_ids
  certificate_arn = aws_acm_certificate.main.arn
}
```

## Cost Optimization

### Estimated Monthly Costs

| Service | Configuration | Cost/Month |
|---------|--------------|------------|
| ECS Fargate | 3 tasks × 1 vCPU × 2GB | ~$90 |
| RDS PostgreSQL | db.t3.medium, 100GB | ~$100 |
| ElastiCache Redis | cache.t3.small × 2 | ~$50 |
| Application Load Balancer | 1 ALB | ~$25 |
| Data Transfer | 100GB/month | ~$10 |
| CloudWatch | Logs + Metrics | ~$20 |
| Secrets Manager | 5 secrets | ~$2 |
| **Total** | | **~$297** |

### Cost Saving Tips

1. **Use Spot Instances** for non-critical workloads
2. **Reserved Instances** for predictable workloads (up to 72% savings)
3. **Auto-scaling** to match demand
4. **S3 Lifecycle Policies** for log storage
5. **CloudWatch Log Insights** instead of third-party tools
6. **AWS Compute Savings Plans** for consistent usage

## Security Best Practices

1. **VPC Configuration**
   - Private subnets for application and database
   - Public subnets only for load balancer
   - NAT Gateway for outbound internet access

2. **IAM Roles**
   - Least privilege principle
   - Separate roles for task and execution
   - No hardcoded credentials

3. **Secrets Management**
   - Use AWS Secrets Manager
   - Rotate secrets regularly
   - Encrypt at rest and in transit

4. **Network Security**
   - Security groups with minimal access
   - Network ACLs for additional protection
   - VPC Flow Logs enabled

5. **Compliance**
   - Enable AWS Config
   - Use AWS Security Hub
   - Regular security audits

## Troubleshooting

### Common Issues

#### ECS Task Fails to Start
```bash
# Check task status
aws ecs describe-tasks \
  --cluster fraiseql-cluster \
  --tasks <task-arn>

# View CloudWatch logs
aws logs tail /ecs/fraiseql --follow
```

#### Database Connection Issues
```bash
# Test from ECS task
aws ecs execute-command \
  --cluster fraiseql-cluster \
  --task <task-arn> \
  --container fraiseql \
  --interactive \
  --command "/bin/bash"

# Inside container
psql $DATABASE_URL -c "SELECT 1"
```

#### High Latency
- Check CloudWatch metrics
- Review X-Ray traces
- Analyze RDS Performance Insights
- Check ElastiCache metrics

## Next Steps

1. Set up [monitoring](./monitoring.md) with CloudWatch
2. Configure [auto-scaling](./scaling.md) policies
3. Implement [disaster recovery](./production-checklist.md)
4. Review [security checklist](./production-checklist.md)
