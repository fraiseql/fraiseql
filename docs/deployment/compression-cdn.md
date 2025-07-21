# Compression and CDN Integration

Comprehensive guide for implementing compression and Content Delivery Network (CDN) integration with FraiseQL to optimize performance and reduce bandwidth costs.

## Table of Contents
- [Overview](#overview)
- [Response Compression](#response-compression)
- [CDN Integration](#cdn-integration)
- [Caching Strategies](#caching-strategies)
- [Edge Computing](#edge-computing)
- [Performance Optimization](#performance-optimization)
- [Monitoring & Analytics](#monitoring--analytics)
- [Security Considerations](#security-considerations)

## Overview

### Performance Benefits

| Optimization | Bandwidth Reduction | Latency Improvement | Cost Savings |
|--------------|-------------------|-------------------|--------------|
| **GZIP Compression** | 60-80% | N/A | High |
| **Brotli Compression** | 70-85% | N/A | Very High |
| **CDN Caching** | 90%+ | 50-90% | Very High |
| **Edge Computing** | Variable | 70-95% | High |
| **Response Minification** | 10-30% | Minimal | Medium |

### Architecture Overview

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Client    │───▶│   CDN Edge  │───▶│ Load Balan. │───▶│  FraiseQL   │
│             │    │             │    │             │    │   Server    │
│ ┌─────────┐ │    │ ┌─────────┐ │    │ ┌─────────┐ │    │ ┌─────────┐ │
│ │ Browser │ │    │ │  Cache  │ │    │ │ Nginx   │ │    │ │ FastAPI │ │
│ │   JS    │ │    │ │ Engine  │ │    │ │Compress.│ │    │ │Compress.│ │
│ └─────────┘ │    │ └─────────┘ │    │ └─────────┘ │    │ └─────────┘ │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

## Response Compression

### 1. Application-Level Compression

```python
# compression.py - Application-level compression for FraiseQL
import gzip
import brotli
import json
import time
from typing import Dict, Any, Optional, Union
from fastapi import Request, Response
from fastapi.middleware.base import BaseHTTPMiddleware
from starlette.responses import StreamingResponse
import asyncio

class CompressionConfig:
    """Configuration for compression middleware."""

    def __init__(self):
        # Compression settings
        self.enable_gzip = True
        self.enable_brotli = True
        self.minimum_size = 1024  # Don't compress responses smaller than 1KB
        self.compression_level = 6  # Balance between speed and compression ratio

        # MIME types to compress
        self.compressible_types = {
            'application/json',
            'application/graphql',
            'text/plain',
            'text/html',
            'text/css',
            'text/javascript',
            'application/javascript',
            'application/xml',
            'text/xml'
        }

        # Client support detection
        self.brotli_quality = 11  # Higher quality for Brotli
        self.gzip_quality = 6     # Standard quality for GZIP

class SmartCompressionMiddleware(BaseHTTPMiddleware):
    """Smart compression middleware that chooses optimal algorithm."""

    def __init__(self, app, config: CompressionConfig = None):
        super().__init__(app)
        self.config = config or CompressionConfig()
        self._compression_stats = {
            'total_requests': 0,
            'compressed_requests': 0,
            'gzip_used': 0,
            'brotli_used': 0,
            'bytes_saved': 0,
            'compression_time': 0
        }

    async def dispatch(self, request: Request, call_next):
        """Process request and apply compression if appropriate."""
        self._compression_stats['total_requests'] += 1

        response = await call_next(request)

        # Check if compression should be applied
        if not self._should_compress(request, response):
            return response

        # Determine best compression algorithm
        compression_type = self._select_compression(request)

        if compression_type:
            response = await self._compress_response(response, compression_type)
            self._compression_stats['compressed_requests'] += 1

        return response

    def _should_compress(self, request: Request, response: Response) -> bool:
        """Determine if response should be compressed."""

        # Check if already compressed
        if response.headers.get('content-encoding'):
            return False

        # Check content type
        content_type = response.headers.get('content-type', '').split(';')[0]
        if content_type not in self.config.compressible_types:
            return False

        # Check content length
        content_length = response.headers.get('content-length')
        if content_length and int(content_length) < self.config.minimum_size:
            return False

        # Check if client accepts compression
        accept_encoding = request.headers.get('accept-encoding', '')
        if not any(encoding in accept_encoding for encoding in ['gzip', 'br']):
            return False

        return True

    def _select_compression(self, request: Request) -> Optional[str]:
        """Select the best compression algorithm based on client support."""
        accept_encoding = request.headers.get('accept-encoding', '').lower()

        # Prefer Brotli if supported (better compression)
        if self.config.enable_brotli and 'br' in accept_encoding:
            return 'br'

        # Fall back to GZIP
        if self.config.enable_gzip and 'gzip' in accept_encoding:
            return 'gzip'

        return None

    async def _compress_response(self, response: Response, compression_type: str) -> Response:
        """Compress response using specified algorithm."""
        start_time = time.time()

        # Get response content
        if hasattr(response, 'body'):
            content = response.body
        else:
            # For streaming responses, we need to read the content
            content = b''.join([chunk async for chunk in response.body_iterator])

        original_size = len(content)

        # Compress content
        if compression_type == 'br':
            compressed_content = brotli.compress(
                content,
                quality=self.config.brotli_quality
            )
            encoding = 'br'
            self._compression_stats['brotli_used'] += 1
        elif compression_type == 'gzip':
            compressed_content = gzip.compress(
                content,
                compresslevel=self.config.gzip_quality
            )
            encoding = 'gzip'
            self._compression_stats['gzip_used'] += 1
        else:
            return response

        # Update statistics
        compression_time = time.time() - start_time
        bytes_saved = original_size - len(compressed_content)
        self._compression_stats['bytes_saved'] += bytes_saved
        self._compression_stats['compression_time'] += compression_time

        # Create new response with compressed content
        new_response = Response(
            content=compressed_content,
            status_code=response.status_code,
            headers=dict(response.headers),
            media_type=response.media_type
        )

        # Update headers
        new_response.headers['content-encoding'] = encoding
        new_response.headers['content-length'] = str(len(compressed_content))
        new_response.headers['vary'] = 'Accept-Encoding'

        # Add compression metadata
        compression_ratio = (1 - len(compressed_content) / original_size) * 100
        new_response.headers['x-compression-ratio'] = f"{compression_ratio:.1f}%"
        new_response.headers['x-original-size'] = str(original_size)

        return new_response

    def get_stats(self) -> Dict[str, Any]:
        """Get compression statistics."""
        total = self._compression_stats['total_requests']
        compressed = self._compression_stats['compressed_requests']

        return {
            **self._compression_stats,
            'compression_rate': compressed / max(total, 1) * 100,
            'avg_bytes_saved': self._compression_stats['bytes_saved'] / max(compressed, 1),
            'avg_compression_time': self._compression_stats['compression_time'] / max(compressed, 1) * 1000  # ms
        }

# GraphQL-specific compression
class GraphQLCompressionOptimizer:
    """Optimize GraphQL responses for compression."""

    @staticmethod
    def optimize_graphql_response(response_data: Dict[str, Any]) -> Dict[str, Any]:
        """Optimize GraphQL response for better compression."""

        # Remove null values to reduce size
        def remove_nulls(obj):
            if isinstance(obj, dict):
                return {k: remove_nulls(v) for k, v in obj.items() if v is not None}
            elif isinstance(obj, list):
                return [remove_nulls(item) for item in obj if item is not None]
            return obj

        # Deduplicate repeated strings
        def deduplicate_strings(obj, string_pool=None):
            if string_pool is None:
                string_pool = {}

            if isinstance(obj, dict):
                return {k: deduplicate_strings(v, string_pool) for k, v in obj.items()}
            elif isinstance(obj, list):
                return [deduplicate_strings(item, string_pool) for item in obj]
            elif isinstance(obj, str) and len(obj) > 10:  # Only pool longer strings
                if obj in string_pool:
                    return f"__ref_{string_pool[obj]}"
                else:
                    ref_id = len(string_pool)
                    string_pool[obj] = ref_id
                    return obj
            return obj

        optimized = remove_nulls(response_data)
        return optimized

# Integration with FraiseQL
from fraiseql import create_fraiseql_app

def create_compressed_app():
    """Create FraiseQL app with compression."""
    app = create_fraiseql_app()

    # Add compression middleware
    compression_config = CompressionConfig()
    compression_middleware = SmartCompressionMiddleware(app, compression_config)
    app.add_middleware(SmartCompressionMiddleware, config=compression_config)

    return app
```

### 2. Load Balancer Compression

```nginx
# nginx-compression.conf - Advanced Nginx compression configuration
upstream fraiseql_backend {
    least_conn;
    server fraiseql-1:8000;
    server fraiseql-2:8000;
    server fraiseql-3:8000;
    keepalive 32;
}

server {
    listen 80;
    server_name api.company.com;

    # Gzip compression
    gzip on;
    gzip_vary on;
    gzip_min_length 1000;
    gzip_comp_level 6;
    gzip_types
        application/json
        application/graphql
        application/javascript
        text/css
        text/javascript
        text/plain
        text/xml
        application/xml
        application/xml+rss;

    # Brotli compression (requires nginx-module-brotli)
    brotli on;
    brotli_comp_level 6;
    brotli_min_length 1000;
    brotli_types
        application/json
        application/graphql
        application/javascript
        text/css
        text/javascript
        text/plain
        text/xml
        application/xml;

    # Response buffering for better compression
    proxy_buffering on;
    proxy_buffer_size 4k;
    proxy_buffers 8 4k;
    proxy_busy_buffers_size 8k;

    location /graphql {
        proxy_pass http://fraiseql_backend;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Compression headers
        proxy_set_header Accept-Encoding gzip,br;

        # Cache compressed responses
        proxy_cache graphql_cache;
        proxy_cache_key "$scheme$request_method$host$request_uri$is_args$args";
        proxy_cache_valid 200 5m;
        proxy_cache_valid 404 1m;
        proxy_cache_use_stale error timeout updating http_500 http_502 http_503 http_504;
        proxy_cache_background_update on;
        proxy_cache_lock on;

        # Add cache headers
        add_header X-Cache-Status $upstream_cache_status;
        add_header X-Compression-Level $sent_http_content_encoding;
    }

    # Health check (no compression needed)
    location /health {
        proxy_pass http://fraiseql_backend;
        gzip off;
        brotli off;
        access_log off;
    }

    # Metrics endpoint (compress for efficiency)
    location /metrics {
        proxy_pass http://fraiseql_backend;
        allow 10.0.0.0/8;
        deny all;
    }
}

# Proxy cache configuration
proxy_cache_path /var/cache/nginx/graphql
    levels=1:2
    keys_zone=graphql_cache:10m
    max_size=100m
    inactive=60m
    use_temp_path=off;
```

## CDN Integration

### 1. CloudFront Configuration

```yaml
# cloudfront-config.yaml - AWS CloudFront configuration for FraiseQL
AWSTemplateFormatVersion: '2010-09-09'
Description: 'CloudFront distribution for FraiseQL API'

Parameters:
  DomainName:
    Type: String
    Default: api.company.com
  OriginDomainName:
    Type: String
    Default: origin.company.com

Resources:
  CloudFrontDistribution:
    Type: AWS::CloudFront::Distribution
    Properties:
      DistributionConfig:
        Aliases:
          - !Ref DomainName

        Origins:
          - Id: FraiseQLOrigin
            DomainName: !Ref OriginDomainName
            CustomOriginConfig:
              HTTPPort: 80
              HTTPSPort: 443
              OriginProtocolPolicy: https-only
              OriginSSLProtocols:
                - TLSv1.2
                - TLSv1.3

        DefaultCacheBehavior:
          TargetOriginId: FraiseQLOrigin
          ViewerProtocolPolicy: redirect-to-https
          AllowedMethods:
            - GET
            - HEAD
            - OPTIONS
            - PUT
            - PATCH
            - POST
            - DELETE
          CachedMethods:
            - GET
            - HEAD
            - OPTIONS
          Compress: true

          # Cache based on headers for GraphQL
          ForwardedValues:
            QueryString: false
            Headers:
              - Authorization
              - Content-Type
              - Accept
              - Accept-Encoding
              - X-Requested-With
            Cookies:
              Forward: none

          # Cache TTL settings
          DefaultTTL: 0        # Don't cache by default
          MaxTTL: 86400       # 1 day maximum
          MinTTL: 0

          # Response headers policy
          ResponseHeadersPolicyId: !Ref ResponseHeadersPolicy

        CacheBehaviors:
          # Static schema introspection (can be cached)
          - PathPattern: /graphql/schema
            TargetOriginId: FraiseQLOrigin
            ViewerProtocolPolicy: redirect-to-https
            AllowedMethods:
              - GET
              - HEAD
            CachedMethods:
              - GET
              - HEAD
            Compress: true
            ForwardedValues:
              QueryString: false
              Headers:
                - Accept-Encoding
            DefaultTTL: 3600    # Cache for 1 hour
            MaxTTL: 86400      # 1 day maximum

          # Health checks (short cache)
          - PathPattern: /health
            TargetOriginId: FraiseQLOrigin
            ViewerProtocolPolicy: allow-all
            AllowedMethods:
              - GET
              - HEAD
            CachedMethods:
              - GET
              - HEAD
            Compress: true
            ForwardedValues:
              QueryString: false
            DefaultTTL: 60      # Cache for 1 minute
            MaxTTL: 300        # 5 minutes maximum

        Enabled: true
        HttpVersion: http2
        IPV6Enabled: true
        PriceClass: PriceClass_All

        # Logging
        Logging:
          Bucket: !GetAtt LoggingBucket.DomainName
          IncludeCookies: false
          Prefix: cloudfront-logs/

        # Error pages
        CustomErrorResponses:
          - ErrorCode: 500
            ResponseCode: 500
            ResponsePagePath: /error/500.html
            ErrorCachingMinTTL: 60
          - ErrorCode: 502
            ResponseCode: 502
            ResponsePagePath: /error/502.html
            ErrorCachingMinTTL: 60
          - ErrorCode: 503
            ResponseCode: 503
            ResponsePagePath: /error/503.html
            ErrorCachingMinTTL: 60
          - ErrorCode: 504
            ResponseCode: 504
            ResponsePagePath: /error/504.html
            ErrorCachingMinTTL: 60

  ResponseHeadersPolicy:
    Type: AWS::CloudFront::ResponseHeadersPolicy
    Properties:
      ResponseHeadersPolicyConfig:
        Name: FraiseQL-Headers
        Comment: Headers for FraiseQL API

        SecurityHeadersConfig:
          StrictTransportSecurity:
            AccessControlMaxAgeSec: 31536000
            IncludeSubdomains: true
          ContentTypeOptions:
            Override: true
          FrameOptions:
            FrameOption: DENY
            Override: true
          ReferrerPolicy:
            ReferrerPolicy: strict-origin-when-cross-origin
            Override: true

        CustomHeadersConfig:
          Items:
            - Header: X-API-Version
              Value: "1.0"
              Override: false
            - Header: X-Cache-Info
              Value: "CloudFront"
              Override: false

  LoggingBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Sub "${AWS::StackName}-cloudfront-logs"
      AccessControl: LogDeliveryWrite
      PublicAccessBlockConfiguration:
        BlockPublicAcls: true
        BlockPublicPolicy: true
        IgnorePublicAcls: true
        RestrictPublicBuckets: true

Outputs:
  DistributionDomainName:
    Description: CloudFront distribution domain name
    Value: !GetAtt CloudFrontDistribution.DomainName
  DistributionId:
    Description: CloudFront distribution ID
    Value: !Ref CloudFrontDistribution
```

### 2. Cache Invalidation Strategy

```python
# cdn_cache.py - CDN cache management for FraiseQL
import boto3
import asyncio
import hashlib
import json
from typing import List, Dict, Set, Optional
from dataclasses import dataclass
from datetime import datetime, timedelta

@dataclass
class CacheInvalidationRule:
    """Rule for cache invalidation."""
    mutation_types: Set[str]
    affected_paths: List[str]
    invalidation_delay: int = 0  # Seconds to wait before invalidation
    invalidation_priority: int = 1  # 1=high, 2=medium, 3=low

class CDNCacheManager:
    """Manage CDN cache invalidation for GraphQL mutations."""

    def __init__(self, distribution_id: str, aws_region: str = 'us-east-1'):
        self.distribution_id = distribution_id
        self.cloudfront = boto3.client('cloudfront', region_name=aws_region)

        # Define invalidation rules
        self.invalidation_rules = [
            CacheInvalidationRule(
                mutation_types={'createUser', 'updateUser', 'deleteUser'},
                affected_paths=['/graphql*'],
                invalidation_priority=1
            ),
            CacheInvalidationRule(
                mutation_types={'createPost', 'updatePost', 'deletePost'},
                affected_paths=['/graphql*'],
                invalidation_priority=2
            ),
            CacheInvalidationRule(
                mutation_types={'updateSchema'},
                affected_paths=['/graphql/schema', '/graphql*'],
                invalidation_priority=1
            )
        ]

        self._pending_invalidations: Dict[str, Set[str]] = {}
        self._invalidation_task: Optional[asyncio.Task] = None

    async def handle_mutation(self, mutation_name: str, affected_entities: List[str] = None):
        """Handle cache invalidation for a GraphQL mutation."""

        # Find applicable rules
        applicable_rules = [
            rule for rule in self.invalidation_rules
            if mutation_name in rule.mutation_types
        ]

        if not applicable_rules:
            return

        # Collect paths to invalidate
        paths_to_invalidate = set()
        max_priority = min(rule.invalidation_priority for rule in applicable_rules)

        for rule in applicable_rules:
            if rule.invalidation_priority == max_priority:
                paths_to_invalidate.update(rule.affected_paths)

        # Add entity-specific paths if provided
        if affected_entities:
            for entity_id in affected_entities:
                paths_to_invalidate.add(f'/graphql/entity/{entity_id}')

        # Queue invalidation
        await self._queue_invalidation(paths_to_invalidate, max_priority)

    async def _queue_invalidation(self, paths: Set[str], priority: int):
        """Queue paths for invalidation."""
        priority_key = str(priority)

        if priority_key not in self._pending_invalidations:
            self._pending_invalidations[priority_key] = set()

        self._pending_invalidations[priority_key].update(paths)

        # Start invalidation task if not running
        if not self._invalidation_task or self._invalidation_task.done():
            self._invalidation_task = asyncio.create_task(self._process_invalidations())

    async def _process_invalidations(self):
        """Process queued invalidations."""
        await asyncio.sleep(5)  # Wait a bit to batch invalidations

        for priority in sorted(self._pending_invalidations.keys()):
            paths = self._pending_invalidations[priority]

            if paths:
                await self._create_invalidation(list(paths))
                del self._pending_invalidations[priority]

    async def _create_invalidation(self, paths: List[str]):
        """Create CloudFront invalidation."""
        try:
            # CloudFront allows max 3000 paths per invalidation
            batch_size = 3000

            for i in range(0, len(paths), batch_size):
                batch_paths = paths[i:i + batch_size]

                caller_reference = f"fraiseql-{datetime.now().isoformat()}-{hashlib.md5(str(batch_paths).encode()).hexdigest()[:8]}"

                response = await asyncio.get_event_loop().run_in_executor(
                    None,
                    lambda: self.cloudfront.create_invalidation(
                        DistributionId=self.distribution_id,
                        InvalidationBatch={
                            'Paths': {
                                'Quantity': len(batch_paths),
                                'Items': batch_paths
                            },
                            'CallerReference': caller_reference
                        }
                    )
                )

                invalidation_id = response['Invalidation']['Id']
                print(f"Created invalidation {invalidation_id} for {len(batch_paths)} paths")

        except Exception as e:
            print(f"Failed to create invalidation: {e}")

    async def smart_cache_warming(self, popular_queries: List[Dict[str, any]]):
        """Pre-warm cache with popular queries."""
        import aiohttp

        async with aiohttp.ClientSession() as session:
            tasks = []

            for query_info in popular_queries:
                task = self._warm_query(session, query_info)
                tasks.append(task)

            # Warm up to 10 queries concurrently
            for i in range(0, len(tasks), 10):
                batch = tasks[i:i + 10]
                await asyncio.gather(*batch, return_exceptions=True)

    async def _warm_query(self, session: aiohttp.ClientSession, query_info: Dict[str, any]):
        """Warm a specific query in the cache."""
        try:
            url = f"https://{self.distribution_id}.cloudfront.net/graphql"

            async with session.post(
                url,
                json={
                    'query': query_info['query'],
                    'variables': query_info.get('variables', {})
                },
                headers={
                    'Content-Type': 'application/json',
                    'Cache-Control': 'no-cache'  # Force origin request
                }
            ) as response:
                await response.text()  # Consume response

        except Exception as e:
            print(f"Failed to warm query: {e}")

# Integration with FraiseQL mutations
cdn_cache_manager = CDNCacheManager(distribution_id=CLOUDFRONT_DISTRIBUTION_ID)

@mutation
async def create_user_with_cache_invalidation(info, input: CreateUserInput) -> UserSuccess | UserError:
    """Create user with automatic cache invalidation."""

    # Execute the mutation
    result = await create_user(info, input)

    # Handle cache invalidation for successful mutations
    if isinstance(result, UserSuccess):
        await cdn_cache_manager.handle_mutation(
            'createUser',
            affected_entities=[result.user.id]
        )

    return result
```

### 3. Edge Computing with Lambda@Edge

```python
# lambda_edge.py - Lambda@Edge functions for FraiseQL optimization
import json
import base64
import gzip
from typing import Dict, Any, Optional

def lambda_handler(event: Dict[str, Any], context) -> Dict[str, Any]:
    """Lambda@Edge function for request/response optimization."""

    request = event['Records'][0]['cf']['request']
    response = event['Records'][0]['cf'].get('response')

    # Determine the trigger type
    event_type = event['Records'][0]['cf']['config']['eventType']

    if event_type == 'viewer-request':
        return handle_viewer_request(request)
    elif event_type == 'origin-request':
        return handle_origin_request(request)
    elif event_type == 'origin-response':
        return handle_origin_response(request, response)
    elif event_type == 'viewer-response':
        return handle_viewer_response(request, response)

    return request if not response else response

def handle_viewer_request(request: Dict[str, Any]) -> Dict[str, Any]:
    """Process incoming viewer requests."""

    # Normalize GraphQL requests
    if request['uri'] == '/graphql' and request['method'] == 'POST':
        # Add cache key based on query hash
        body = request.get('body', {})
        if body.get('data'):
            # Decode body
            body_content = base64.b64decode(body['data']).decode('utf-8')

            try:
                graphql_request = json.loads(body_content)
                query_hash = hash(graphql_request.get('query', ''))

                # Add custom header for caching
                if 'headers' not in request:
                    request['headers'] = {}

                request['headers']['x-query-hash'] = [{
                    'key': 'X-Query-Hash',
                    'value': str(query_hash)
                }]

            except json.JSONDecodeError:
                pass

    # Add security headers
    if 'headers' not in request:
        request['headers'] = {}

    # Add origin header for CORS
    request['headers']['origin'] = [{
        'key': 'Origin',
        'value': 'https://api.company.com'
    }]

    return request

def handle_origin_request(request: Dict[str, Any]) -> Dict[str, Any]:
    """Process requests before sending to origin."""

    # Add custom headers for origin
    if 'headers' not in request:
        request['headers'] = {}

    # Add CloudFront request ID for tracing
    request['headers']['x-cloudfront-request-id'] = [{
        'key': 'X-CloudFront-Request-ID',
        'value': request.get('requestId', 'unknown')
    }]

    # Optimize GraphQL introspection queries
    if request['uri'] == '/graphql' and request['method'] == 'POST':
        body = request.get('body', {})
        if body.get('data'):
            try:
                body_content = base64.b64decode(body['data']).decode('utf-8')
                graphql_request = json.loads(body_content)

                # Check if it's an introspection query
                query = graphql_request.get('query', '')
                if '__schema' in query or '__type' in query:
                    # Add header to indicate this can be cached longer
                    request['headers']['x-introspection-query'] = [{
                        'key': 'X-Introspection-Query',
                        'value': 'true'
                    }]

            except (json.JSONDecodeError, UnicodeDecodeError):
                pass

    return request

def handle_origin_response(request: Dict[str, Any], response: Dict[str, Any]) -> Dict[str, Any]:
    """Process responses from origin."""

    # Initialize headers if not present
    if 'headers' not in response:
        response['headers'] = {}

    # Add performance headers
    response['headers']['x-edge-optimized'] = [{
        'key': 'X-Edge-Optimized',
        'value': 'true'
    }]

    # Optimize caching for different response types
    content_type = ''
    for header_name, header_values in response.get('headers', {}).items():
        if header_name.lower() == 'content-type':
            content_type = header_values[0]['value']
            break

    if 'application/json' in content_type:
        # Check if it's a GraphQL response
        body = response.get('body', {})
        if body.get('data'):
            try:
                # Decode response body
                if body.get('encoding') == 'base64':
                    body_content = base64.b64decode(body['data']).decode('utf-8')
                else:
                    body_content = body['data']

                graphql_response = json.loads(body_content)

                # Check if response contains errors
                if graphql_response.get('errors'):
                    # Don't cache error responses
                    response['headers']['cache-control'] = [{
                        'key': 'Cache-Control',
                        'value': 'no-cache, no-store, must-revalidate'
                    }]
                else:
                    # Check if it's an introspection response
                    introspection_header = request.get('headers', {}).get('x-introspection-query')
                    if introspection_header:
                        # Cache introspection responses for longer
                        response['headers']['cache-control'] = [{
                            'key': 'Cache-Control',
                            'value': 'public, max-age=3600'  # 1 hour
                        }]

            except (json.JSONDecodeError, UnicodeDecodeError):
                pass

    return response

def handle_viewer_response(request: Dict[str, Any], response: Dict[str, Any]) -> Dict[str, Any]:
    """Process responses before sending to viewer."""

    # Initialize headers if not present
    if 'headers' not in response:
        response['headers'] = {}

    # Add security headers
    security_headers = {
        'strict-transport-security': 'max-age=31536000; includeSubDomains',
        'content-security-policy': "default-src 'self'",
        'x-content-type-options': 'nosniff',
        'x-frame-options': 'DENY',
        'x-xss-protection': '1; mode=block',
        'referrer-policy': 'strict-origin-when-cross-origin'
    }

    for header_name, header_value in security_headers.items():
        response['headers'][header_name] = [{
            'key': header_name.title().replace('-', '-'),
            'value': header_value
        }]

    # Add CORS headers for GraphQL endpoints
    if request.get('uri') == '/graphql':
        response['headers']['access-control-allow-origin'] = [{
            'key': 'Access-Control-Allow-Origin',
            'value': '*'  # Configure based on your CORS policy
        }]
        response['headers']['access-control-allow-methods'] = [{
            'key': 'Access-Control-Allow-Methods',
            'value': 'POST, GET, OPTIONS'
        }]
        response['headers']['access-control-allow-headers'] = [{
            'key': 'Access-Control-Allow-Headers',
            'value': 'Content-Type, Authorization, Accept'
        }]

    # Optimize response for mobile clients
    user_agent = ''
    for header_name, header_values in request.get('headers', {}).items():
        if header_name.lower() == 'user-agent':
            user_agent = header_values[0]['value'].lower()
            break

    if 'mobile' in user_agent:
        # Add mobile-specific optimization headers
        response['headers']['x-mobile-optimized'] = [{
            'key': 'X-Mobile-Optimized',
            'value': 'true'
        }]

    return response

# Deployment configuration for Lambda@Edge
lambda_edge_config = {
    "viewer-request": {
        "function_name": "fraiseql-viewer-request",
        "runtime": "python3.9",
        "timeout": 5,
        "memory": 128
    },
    "origin-request": {
        "function_name": "fraiseql-origin-request",
        "runtime": "python3.9",
        "timeout": 30,
        "memory": 256
    },
    "origin-response": {
        "function_name": "fraiseql-origin-response",
        "runtime": "python3.9",
        "timeout": 30,
        "memory": 256
    },
    "viewer-response": {
        "function_name": "fraiseql-viewer-response",
        "runtime": "python3.9",
        "timeout": 5,
        "memory": 128
    }
}
```

## Caching Strategies

### 1. GraphQL-Specific Caching

```python
# graphql_cache.py - Advanced GraphQL caching strategies
import hashlib
import json
import time
from typing import Dict, Any, Optional, Set, List
from dataclasses import dataclass
from graphql import DocumentNode, OperationDefinitionNode

@dataclass
class CacheRule:
    """Rule for GraphQL response caching."""
    operation_types: Set[str]  # query, mutation, subscription
    operation_names: Set[str]  # specific operation names
    max_age: int              # cache TTL in seconds
    vary_by: List[str]        # headers/variables to vary cache by
    cache_level: str          # 'cdn', 'edge', 'application'

class GraphQLCacheManager:
    """Advanced GraphQL caching with intelligent cache key generation."""

    def __init__(self):
        self.cache_rules = [
            # Introspection queries - cache heavily
            CacheRule(
                operation_types={'query'},
                operation_names={'IntrospectionQuery', '__schema'},
                max_age=3600,  # 1 hour
                vary_by=[],
                cache_level='cdn'
            ),

            # Public data queries - moderate caching
            CacheRule(
                operation_types={'query'},
                operation_names={'getPublicPosts', 'getCategories'},
                max_age=300,   # 5 minutes
                vary_by=['accept-language'],
                cache_level='edge'
            ),

            # User-specific queries - short caching
            CacheRule(
                operation_types={'query'},
                operation_names={'getCurrentUser', 'getUserPosts'},
                max_age=60,    # 1 minute
                vary_by=['authorization'],
                cache_level='application'
            ),

            # Mutations - no caching
            CacheRule(
                operation_types={'mutation'},
                operation_names={'*'},
                max_age=0,
                vary_by=[],
                cache_level='none'
            )
        ]

        self._query_complexity_cache = {}
        self._field_usage_stats = {}

    def generate_cache_key(
        self,
        query: str,
        variables: Dict[str, Any] = None,
        headers: Dict[str, str] = None,
        operation_name: str = None
    ) -> str:
        """Generate a cache key for a GraphQL request."""

        # Normalize the query (remove whitespace, comments)
        normalized_query = self._normalize_query(query)

        # Create base key from query
        query_hash = hashlib.sha256(normalized_query.encode()).hexdigest()[:16]

        # Add variables if present
        variables_str = ""
        if variables:
            # Sort variables for consistent hashing
            sorted_vars = json.dumps(variables, sort_keys=True, separators=(',', ':'))
            variables_hash = hashlib.md5(sorted_vars.encode()).hexdigest()[:8]
            variables_str = f":{variables_hash}"

        # Add headers based on cache rules
        headers_str = ""
        if headers:
            cache_rule = self._find_cache_rule(query, operation_name)
            if cache_rule and cache_rule.vary_by:
                relevant_headers = {
                    key: headers.get(key, '')
                    for key in cache_rule.vary_by
                    if key in headers
                }
                if relevant_headers:
                    headers_json = json.dumps(relevant_headers, sort_keys=True)
                    headers_hash = hashlib.md5(headers_json.encode()).hexdigest()[:8]
                    headers_str = f":{headers_hash}"

        return f"gql:{query_hash}{variables_str}{headers_str}"

    def get_cache_config(
        self,
        query: str,
        operation_name: str = None
    ) -> Optional[Dict[str, Any]]:
        """Get cache configuration for a GraphQL operation."""

        cache_rule = self._find_cache_rule(query, operation_name)

        if not cache_rule or cache_rule.max_age == 0:
            return None

        return {
            'max_age': cache_rule.max_age,
            'cache_level': cache_rule.cache_level,
            'vary_by': cache_rule.vary_by,
            'cache_control': self._generate_cache_control_header(cache_rule)
        }

    def _find_cache_rule(self, query: str, operation_name: str = None) -> Optional[CacheRule]:
        """Find the applicable cache rule for a query."""

        # Parse operation type from query
        operation_type = self._extract_operation_type(query)

        # Find matching rules
        for rule in self.cache_rules:
            # Check operation type
            if operation_type not in rule.operation_types:
                continue

            # Check operation name
            if operation_name:
                if ('*' not in rule.operation_names and
                    operation_name not in rule.operation_names):
                    continue
            elif rule.operation_names != {'*'}:
                # If no operation name provided but rule is specific, skip
                continue

            return rule

        return None

    def _normalize_query(self, query: str) -> str:
        """Normalize GraphQL query for consistent caching."""
        # Remove comments
        lines = []
        for line in query.split('\n'):
            comment_pos = line.find('#')
            if comment_pos >= 0:
                line = line[:comment_pos]
            line = line.strip()
            if line:
                lines.append(line)

        # Join and remove extra whitespace
        normalized = ' '.join(lines)
        return ' '.join(normalized.split())

    def _extract_operation_type(self, query: str) -> str:
        """Extract operation type from GraphQL query."""
        normalized = query.strip().lower()
        if normalized.startswith('mutation'):
            return 'mutation'
        elif normalized.startswith('subscription'):
            return 'subscription'
        else:
            return 'query'  # Default to query

    def _generate_cache_control_header(self, cache_rule: CacheRule) -> str:
        """Generate Cache-Control header based on cache rule."""
        if cache_rule.cache_level == 'cdn':
            return f"public, max-age={cache_rule.max_age}, s-maxage={cache_rule.max_age}"
        elif cache_rule.cache_level == 'edge':
            return f"public, max-age={cache_rule.max_age // 2}, s-maxage={cache_rule.max_age}"
        elif cache_rule.cache_level == 'application':
            return f"private, max-age={cache_rule.max_age}"
        else:
            return "no-cache, no-store, must-revalidate"

    def track_query_usage(self, query: str, execution_time: float, result_size: int):
        """Track query usage for optimization."""
        query_hash = hashlib.sha256(self._normalize_query(query).encode()).hexdigest()[:16]

        if query_hash not in self._field_usage_stats:
            self._field_usage_stats[query_hash] = {
                'count': 0,
                'total_time': 0,
                'total_size': 0,
                'avg_time': 0,
                'avg_size': 0
            }

        stats = self._field_usage_stats[query_hash]
        stats['count'] += 1
        stats['total_time'] += execution_time
        stats['total_size'] += result_size
        stats['avg_time'] = stats['total_time'] / stats['count']
        stats['avg_size'] = stats['total_size'] / stats['count']

    def get_optimization_recommendations(self) -> List[Dict[str, Any]]:
        """Get recommendations for cache optimization."""
        recommendations = []

        for query_hash, stats in self._field_usage_stats.items():
            if stats['count'] > 100:  # High usage queries
                if stats['avg_time'] > 0.5:  # Slow queries
                    recommendations.append({
                        'type': 'increase_cache_ttl',
                        'query_hash': query_hash,
                        'reason': f"High usage ({stats['count']}) slow query ({stats['avg_time']:.3f}s)",
                        'current_avg_time': stats['avg_time'],
                        'usage_count': stats['count']
                    })

                if stats['avg_size'] > 100000:  # Large responses
                    recommendations.append({
                        'type': 'enable_compression',
                        'query_hash': query_hash,
                        'reason': f"Large response size ({stats['avg_size']} bytes)",
                        'avg_size': stats['avg_size'],
                        'usage_count': stats['count']
                    })

        return recommendations

# Integration with FastAPI middleware
from fastapi import Request, Response
from fastapi.middleware.base import BaseHTTPMiddleware

class GraphQLCacheMiddleware(BaseHTTPMiddleware):
    """Middleware to handle GraphQL caching."""

    def __init__(self, app, cache_manager: GraphQLCacheManager):
        super().__init__(app)
        self.cache_manager = cache_manager

    async def dispatch(self, request: Request, call_next):
        """Handle caching for GraphQL requests."""

        # Only process GraphQL requests
        if request.url.path != '/graphql' or request.method != 'POST':
            return await call_next(request)

        # Parse GraphQL request
        try:
            body = await request.body()
            graphql_request = json.loads(body)
            query = graphql_request.get('query', '')
            variables = graphql_request.get('variables', {})
            operation_name = graphql_request.get('operationName')
        except:
            return await call_next(request)

        # Get cache configuration
        cache_config = self.cache_manager.get_cache_config(query, operation_name)

        # Execute request
        start_time = time.time()
        response = await call_next(request)
        execution_time = time.time() - start_time

        # Add cache headers if applicable
        if cache_config:
            response.headers['Cache-Control'] = cache_config['cache_control']

            if cache_config['vary_by']:
                vary_headers = ', '.join(cache_config['vary_by'])
                response.headers['Vary'] = vary_headers

        # Track usage statistics
        if hasattr(response, 'body'):
            result_size = len(response.body)
        else:
            result_size = 0

        self.cache_manager.track_query_usage(query, execution_time, result_size)

        return response
```

## Performance Optimization

### 1. Response Optimization

```python
# response_optimization.py - Advanced response optimization for FraiseQL
import json
import gzip
import time
from typing import Dict, Any, List, Optional, Union
from dataclasses import dataclass

@dataclass
class OptimizationConfig:
    """Configuration for response optimization."""
    enable_minification: bool = True
    enable_field_filtering: bool = True
    enable_response_compression: bool = True
    max_response_size: int = 1024 * 1024  # 1MB
    compression_threshold: int = 1024      # 1KB

class ResponseOptimizer:
    """Optimize GraphQL responses for performance."""

    def __init__(self, config: OptimizationConfig = None):
        self.config = config or OptimizationConfig()
        self._optimization_stats = {
            'responses_optimized': 0,
            'bytes_saved': 0,
            'optimization_time': 0
        }

    def optimize_response(
        self,
        response_data: Dict[str, Any],
        query: str = None,
        requested_fields: List[str] = None
    ) -> Dict[str, Any]:
        """Optimize GraphQL response data."""

        start_time = time.time()
        original_size = len(json.dumps(response_data))

        optimized_data = response_data.copy()

        # Apply optimizations
        if self.config.enable_field_filtering and requested_fields:
            optimized_data = self._filter_unused_fields(optimized_data, requested_fields)

        if self.config.enable_minification:
            optimized_data = self._minify_response(optimized_data)

        # Update statistics
        optimized_size = len(json.dumps(optimized_data))
        optimization_time = time.time() - start_time

        self._optimization_stats['responses_optimized'] += 1
        self._optimization_stats['bytes_saved'] += (original_size - optimized_size)
        self._optimization_stats['optimization_time'] += optimization_time

        return optimized_data

    def _filter_unused_fields(
        self,
        data: Dict[str, Any],
        requested_fields: List[str]
    ) -> Dict[str, Any]:
        """Remove fields not requested in the GraphQL query."""

        if not isinstance(data, dict):
            return data

        # For GraphQL responses, filter the 'data' section
        if 'data' in data and isinstance(data['data'], dict):
            filtered_data = data.copy()
            filtered_data['data'] = self._filter_object_fields(
                data['data'],
                requested_fields
            )
            return filtered_data

        return data

    def _filter_object_fields(
        self,
        obj: Union[Dict, List, Any],
        allowed_fields: List[str]
    ) -> Union[Dict, List, Any]:
        """Recursively filter object fields."""

        if isinstance(obj, dict):
            filtered = {}
            for key, value in obj.items():
                if key in allowed_fields or key.startswith('__'):  # Keep meta fields
                    if isinstance(value, (dict, list)):
                        filtered[key] = self._filter_object_fields(value, allowed_fields)
                    else:
                        filtered[key] = value
            return filtered

        elif isinstance(obj, list):
            return [
                self._filter_object_fields(item, allowed_fields)
                for item in obj
            ]

        return obj

    def _minify_response(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """Minify response by removing unnecessary data."""

        # Remove null values
        def remove_nulls(obj):
            if isinstance(obj, dict):
                return {
                    k: remove_nulls(v)
                    for k, v in obj.items()
                    if v is not None
                }
            elif isinstance(obj, list):
                return [remove_nulls(item) for item in obj if item is not None]
            return obj

        # Remove empty arrays and objects
        def remove_empty(obj):
            if isinstance(obj, dict):
                filtered = {}
                for k, v in obj.items():
                    cleaned = remove_empty(v)
                    if cleaned or cleaned == 0 or cleaned == False:  # Keep falsy values except None and empty containers
                        filtered[k] = cleaned
                return filtered
            elif isinstance(obj, list):
                return [remove_empty(item) for item in obj]
            return obj

        minified = remove_nulls(data)
        minified = remove_empty(minified)

        return minified

    def suggest_optimizations(self, response_data: Dict[str, Any]) -> List[str]:
        """Suggest optimizations for the response."""
        suggestions = []

        response_size = len(json.dumps(response_data))

        if response_size > self.config.max_response_size:
            suggestions.append(f"Response size ({response_size} bytes) exceeds limit. Consider pagination.")

        # Check for deeply nested data
        max_depth = self._calculate_depth(response_data)
        if max_depth > 10:
            suggestions.append(f"Response depth ({max_depth}) is high. Consider flattening the structure.")

        # Check for repeated data
        repeated_data = self._find_repeated_data(response_data)
        if repeated_data:
            suggestions.append("Found repeated data patterns. Consider normalization.")

        return suggestions

    def _calculate_depth(self, obj: Any, current_depth: int = 0) -> int:
        """Calculate maximum depth of nested objects."""
        if isinstance(obj, dict):
            if not obj:
                return current_depth
            return max(
                self._calculate_depth(value, current_depth + 1)
                for value in obj.values()
            )
        elif isinstance(obj, list):
            if not obj:
                return current_depth
            return max(
                self._calculate_depth(item, current_depth + 1)
                for item in obj
            )
        return current_depth

    def _find_repeated_data(self, obj: Any) -> bool:
        """Find repeated data patterns that could be optimized."""
        seen_objects = set()

        def check_object(o):
            if isinstance(o, dict):
                obj_str = json.dumps(o, sort_keys=True)
                if len(obj_str) > 100 and obj_str in seen_objects:
                    return True
                seen_objects.add(obj_str)
                return any(check_object(v) for v in o.values())
            elif isinstance(o, list):
                return any(check_object(item) for item in o)
            return False

        return check_object(obj)

    def get_stats(self) -> Dict[str, Any]:
        """Get optimization statistics."""
        stats = self._optimization_stats.copy()

        if stats['responses_optimized'] > 0:
            stats['avg_bytes_saved'] = stats['bytes_saved'] / stats['responses_optimized']
            stats['avg_optimization_time'] = (
                stats['optimization_time'] / stats['responses_optimized'] * 1000  # ms
            )
        else:
            stats['avg_bytes_saved'] = 0
            stats['avg_optimization_time'] = 0

        return stats

# Integration with GraphQL execution
from graphql import execute

class OptimizedGraphQLExecutor:
    """GraphQL executor with response optimization."""

    def __init__(self, schema, optimizer: ResponseOptimizer):
        self.schema = schema
        self.optimizer = optimizer

    async def execute_optimized(
        self,
        query: str,
        variables: Dict[str, Any] = None,
        context: Dict[str, Any] = None
    ) -> Dict[str, Any]:
        """Execute GraphQL query with response optimization."""

        # Execute the query
        result = await execute(
            self.schema,
            query,
            variable_values=variables,
            context_value=context
        )

        # Convert execution result to dict
        response_data = {
            'data': result.data,
            'errors': [error.formatted for error in result.errors] if result.errors else None
        }

        # Remove None values
        response_data = {k: v for k, v in response_data.items() if v is not None}

        # Extract requested fields from query
        requested_fields = self._extract_fields_from_query(query)

        # Optimize response
        optimized_response = self.optimizer.optimize_response(
            response_data,
            query=query,
            requested_fields=requested_fields
        )

        return optimized_response

    def _extract_fields_from_query(self, query: str) -> List[str]:
        """Extract field names from GraphQL query."""
        # Simple field extraction (could be improved with proper AST parsing)
        fields = []

        # Remove query operation and brackets
        cleaned = query.replace('query', '').replace('mutation', '').replace('subscription', '')

        # Extract field names (simplified approach)
        import re
        field_pattern = r'\b([a-zA-Z_][a-zA-Z0-9_]*)\s*[{\(]?'
        matches = re.findall(field_pattern, cleaned)

        # Filter out GraphQL keywords
        keywords = {'fragment', 'on', 'if', 'true', 'false', 'null'}
        fields = [match for match in matches if match not in keywords]

        return fields
```

## Monitoring & Analytics

### 1. CDN Performance Monitoring

```python
# cdn_monitoring.py - CDN performance monitoring and analytics
import boto3
import asyncio
import json
from datetime import datetime, timedelta
from typing import Dict, List, Any, Optional
from dataclasses import dataclass

@dataclass
class CDNMetrics:
    """CDN performance metrics."""
    timestamp: datetime
    requests: int
    bytes_downloaded: int
    cache_hit_rate: float
    origin_latency: float
    edge_latency: float
    error_rate: float
    top_urls: List[Dict[str, Any]]

class CDNMonitor:
    """Monitor CDN performance and generate insights."""

    def __init__(self, distribution_id: str, aws_region: str = 'us-east-1'):
        self.distribution_id = distribution_id
        self.cloudwatch = boto3.client('cloudwatch', region_name=aws_region)
        self.cloudfront = boto3.client('cloudfront', region_name=aws_region)

    async def get_performance_metrics(
        self,
        start_time: datetime,
        end_time: datetime
    ) -> CDNMetrics:
        """Get CDN performance metrics for the specified time range."""

        # Get CloudWatch metrics
        metrics_data = await self._fetch_cloudwatch_metrics(start_time, end_time)

        # Get real user monitoring data
        rum_data = await self._fetch_rum_data(start_time, end_time)

        # Combine and calculate metrics
        return CDNMetrics(
            timestamp=datetime.now(),
            requests=metrics_data.get('requests', 0),
            bytes_downloaded=metrics_data.get('bytes_downloaded', 0),
            cache_hit_rate=metrics_data.get('cache_hit_rate', 0.0),
            origin_latency=metrics_data.get('origin_latency', 0.0),
            edge_latency=rum_data.get('edge_latency', 0.0),
            error_rate=metrics_data.get('error_rate', 0.0),
            top_urls=metrics_data.get('top_urls', [])
        )

    async def _fetch_cloudwatch_metrics(
        self,
        start_time: datetime,
        end_time: datetime
    ) -> Dict[str, Any]:
        """Fetch metrics from CloudWatch."""

        namespace = 'AWS/CloudFront'
        dimensions = [
            {
                'Name': 'DistributionId',
                'Value': self.distribution_id
            }
        ]

        # Define metrics to fetch
        metric_queries = [
            {
                'name': 'requests',
                'metric_name': 'Requests',
                'statistic': 'Sum'
            },
            {
                'name': 'bytes_downloaded',
                'metric_name': 'BytesDownloaded',
                'statistic': 'Sum'
            },
            {
                'name': 'cache_hit_rate',
                'metric_name': 'CacheHitRate',
                'statistic': 'Average'
            },
            {
                'name': 'origin_latency',
                'metric_name': 'OriginLatency',
                'statistic': 'Average'
            },
            {
                'name': 'error_rate',
                'metric_name': '4xxErrorRate',
                'statistic': 'Average'
            }
        ]

        results = {}

        for metric_query in metric_queries:
            try:
                response = await asyncio.get_event_loop().run_in_executor(
                    None,
                    lambda: self.cloudwatch.get_metric_statistics(
                        Namespace=namespace,
                        MetricName=metric_query['metric_name'],
                        Dimensions=dimensions,
                        StartTime=start_time,
                        EndTime=end_time,
                        Period=3600,  # 1 hour periods
                        Statistics=[metric_query['statistic']]
                    )
                )

                datapoints = response.get('Datapoints', [])
                if datapoints:
                    latest_datapoint = max(datapoints, key=lambda x: x['Timestamp'])
                    results[metric_query['name']] = latest_datapoint[metric_query['statistic']]
                else:
                    results[metric_query['name']] = 0

            except Exception as e:
                print(f"Error fetching {metric_query['name']}: {e}")
                results[metric_query['name']] = 0

        return results

    async def _fetch_rum_data(
        self,
        start_time: datetime,
        end_time: datetime
    ) -> Dict[str, Any]:
        """Fetch Real User Monitoring data."""
        # This would integrate with your RUM solution (e.g., CloudWatch RUM)
        # For now, return placeholder data
        return {
            'edge_latency': 50.0,  # ms
            'user_experience_score': 85.0
        }

    async def analyze_performance_trends(
        self,
        days: int = 7
    ) -> Dict[str, Any]:
        """Analyze performance trends over the specified number of days."""

        end_time = datetime.now()
        start_time = end_time - timedelta(days=days)

        # Get daily metrics
        daily_metrics = []
        for i in range(days):
            day_start = start_time + timedelta(days=i)
            day_end = day_start + timedelta(days=1)

            metrics = await self.get_performance_metrics(day_start, day_end)
            daily_metrics.append(metrics)

        # Calculate trends
        cache_hit_rates = [m.cache_hit_rate for m in daily_metrics]
        origin_latencies = [m.origin_latency for m in daily_metrics]
        error_rates = [m.error_rate for m in daily_metrics]

        return {
            'cache_hit_rate_trend': self._calculate_trend(cache_hit_rates),
            'origin_latency_trend': self._calculate_trend(origin_latencies),
            'error_rate_trend': self._calculate_trend(error_rates),
            'avg_cache_hit_rate': sum(cache_hit_rates) / len(cache_hit_rates),
            'avg_origin_latency': sum(origin_latencies) / len(origin_latencies),
            'avg_error_rate': sum(error_rates) / len(error_rates),
            'performance_score': self._calculate_performance_score(daily_metrics[-1])
        }

    def _calculate_trend(self, values: List[float]) -> str:
        """Calculate trend direction from a list of values."""
        if len(values) < 2:
            return 'stable'

        # Simple linear trend calculation
        first_half = sum(values[:len(values)//2]) / (len(values)//2)
        second_half = sum(values[len(values)//2:]) / (len(values) - len(values)//2)

        change_percentage = ((second_half - first_half) / first_half) * 100

        if change_percentage > 5:
            return 'improving'
        elif change_percentage < -5:
            return 'degrading'
        else:
            return 'stable'

    def _calculate_performance_score(self, metrics: CDNMetrics) -> float:
        """Calculate overall performance score (0-100)."""

        # Weight different metrics
        cache_score = min(metrics.cache_hit_rate, 100)  # Higher is better
        latency_score = max(0, 100 - (metrics.origin_latency / 10))  # Lower is better
        error_score = max(0, 100 - (metrics.error_rate * 10))  # Lower is better

        # Weighted average
        total_score = (
            cache_score * 0.4 +      # 40% weight on cache performance
            latency_score * 0.4 +    # 40% weight on latency
            error_score * 0.2        # 20% weight on error rate
        )

        return round(total_score, 1)

    async def generate_optimization_report(self) -> Dict[str, Any]:
        """Generate optimization recommendations based on performance analysis."""

        # Get recent performance data
        trends = await self.analyze_performance_trends(days=7)
        current_metrics = await self.get_performance_metrics(
            datetime.now() - timedelta(hours=1),
            datetime.now()
        )

        recommendations = []

        # Cache hit rate recommendations
        if current_metrics.cache_hit_rate < 80:
            recommendations.append({
                'type': 'cache_optimization',
                'priority': 'high',
                'title': 'Low Cache Hit Rate',
                'description': f'Cache hit rate is {current_metrics.cache_hit_rate:.1f}%, below optimal 80%',
                'actions': [
                    'Review cache headers for GraphQL responses',
                    'Implement query-based caching strategies',
                    'Consider increasing cache TTL for stable data'
                ]
            })

        # Origin latency recommendations
        if current_metrics.origin_latency > 500:  # 500ms
            recommendations.append({
                'type': 'origin_optimization',
                'priority': 'medium',
                'title': 'High Origin Latency',
                'description': f'Origin latency is {current_metrics.origin_latency:.0f}ms',
                'actions': [
                    'Optimize database queries',
                    'Implement connection pooling',
                    'Consider adding more origin servers'
                ]
            })

        # Error rate recommendations
        if current_metrics.error_rate > 1:  # 1%
            recommendations.append({
                'type': 'error_reduction',
                'priority': 'high',
                'title': 'High Error Rate',
                'description': f'Error rate is {current_metrics.error_rate:.1f}%',
                'actions': [
                    'Review error logs for common issues',
                    'Implement better error handling',
                    'Add health checks and monitoring'
                ]
            })

        return {
            'performance_score': trends['performance_score'],
            'trends': trends,
            'current_metrics': current_metrics,
            'recommendations': recommendations,
            'generated_at': datetime.now().isoformat()
        }
```

## Security Considerations

### 1. CDN Security Configuration

```yaml
# cdn-security.yaml - Security configuration for CDN
Resources:
  # Web Application Firewall
  WebACL:
    Type: AWS::WAFv2::WebACL
    Properties:
      Name: FraiseQL-WAF
      Scope: CLOUDFRONT
      DefaultAction:
        Allow: {}
      Rules:
        # Rate limiting
        - Name: RateLimitRule
          Priority: 1
          Statement:
            RateBasedStatement:
              Limit: 2000
              AggregateKeyType: IP
          Action:
            Block: {}
          VisibilityConfig:
            SampledRequestsEnabled: true
            CloudWatchMetricsEnabled: true
            MetricName: RateLimitRule

        # Block malicious IPs
        - Name: IPReputationRule
          Priority: 2
          Statement:
            ManagedRuleGroupStatement:
              VendorName: AWS
              Name: AWSManagedRulesAmazonIpReputationList
          Action:
            Block: {}
          VisibilityConfig:
            SampledRequestsEnabled: true
            CloudWatchMetricsEnabled: true
            MetricName: IPReputationRule

        # GraphQL specific protections
        - Name: GraphQLProtectionRule
          Priority: 3
          Statement:
            AndStatement:
              Statements:
                - ByteMatchStatement:
                    SearchString: "/graphql"
                    FieldToMatch:
                      UriPath: {}
                    TextTransformations:
                      - Priority: 0
                        Type: LOWERCASE
                    PositionalConstraint: CONTAINS
                - SizeConstraintStatement:
                    FieldToMatch:
                      Body: {}
                    ComparisonOperator: GT
                    Size: 10000  # Block very large queries
                    TextTransformations:
                      - Priority: 0
                        Type: NONE
          Action:
            Block: {}
          VisibilityConfig:
            SampledRequestsEnabled: true
            CloudWatchMetricsEnabled: true
            MetricName: GraphQLProtectionRule

  # Origin Request Policy
  OriginRequestPolicy:
    Type: AWS::CloudFront::OriginRequestPolicy
    Properties:
      OriginRequestPolicyConfig:
        Name: FraiseQL-OriginRequest
        Comment: Origin request policy for FraiseQL
        CookiesConfig:
          CookieBehavior: none
        HeadersConfig:
          HeaderBehavior: whitelist
          Headers:
            - Authorization
            - Content-Type
            - Accept
            - Accept-Encoding
            - User-Agent
            - X-Forwarded-For
            - X-Real-IP
        QueryStringsConfig:
          QueryStringBehavior: none
```

### 2. Content Security Policy

```python
# security_headers.py - Security headers for FraiseQL CDN
from typing import Dict, List

class SecurityHeadersConfig:
    """Configuration for security headers."""

    @staticmethod
    def get_security_headers() -> Dict[str, str]:
        """Get security headers for FraiseQL API."""

        return {
            # Content Security Policy
            'Content-Security-Policy': (
                "default-src 'self'; "
                "script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; "
                "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; "
                "font-src 'self' https://fonts.gstatic.com; "
                "img-src 'self' data: https:; "
                "connect-src 'self' https://api.company.com; "
                "frame-ancestors 'none'; "
                "base-uri 'self'; "
                "form-action 'self'"
            ),

            # Strict Transport Security
            'Strict-Transport-Security': 'max-age=31536000; includeSubDomains; preload',

            # Prevent MIME type sniffing
            'X-Content-Type-Options': 'nosniff',

            # Prevent clickjacking
            'X-Frame-Options': 'DENY',

            # XSS Protection
            'X-XSS-Protection': '1; mode=block',

            # Referrer Policy
            'Referrer-Policy': 'strict-origin-when-cross-origin',

            # Permissions Policy
            'Permissions-Policy': (
                'geolocation=(), '
                'microphone=(), '
                'camera=(), '
                'payment=(), '
                'usb=(), '
                'magnetometer=(), '
                'accelerometer=(), '
                'gyroscope=()'
            ),

            # Cross-Origin policies
            'Cross-Origin-Embedder-Policy': 'require-corp',
            'Cross-Origin-Opener-Policy': 'same-origin',
            'Cross-Origin-Resource-Policy': 'same-origin'
        }

    @staticmethod
    def get_graphql_cors_headers() -> Dict[str, str]:
        """Get CORS headers for GraphQL endpoints."""

        return {
            'Access-Control-Allow-Origin': '*',  # Configure based on your needs
            'Access-Control-Allow-Methods': 'POST, GET, OPTIONS',
            'Access-Control-Allow-Headers': (
                'Content-Type, Authorization, Accept, '
                'X-Requested-With, X-GraphQL-Query-Name'
            ),
            'Access-Control-Max-Age': '86400',  # 24 hours
            'Access-Control-Expose-Headers': (
                'X-RateLimit-Limit, X-RateLimit-Remaining, '
                'X-Response-Time, X-Cache-Status'
            )
        }
```

## Best Practices Summary

### 1. Compression Optimization
- **Algorithm Selection**: Use Brotli when supported, fallback to GZIP
- **Compression Levels**: Balance compression ratio with CPU usage
- **Size Thresholds**: Don't compress responses smaller than 1KB
- **Content Types**: Only compress text-based content types

### 2. CDN Configuration
- **Cache Rules**: Implement GraphQL-specific caching strategies
- **Invalidation**: Automate cache invalidation for mutations
- **Security**: Use WAF and security headers
- **Monitoring**: Track performance metrics and optimization opportunities

### 3. Performance Optimization
- **Response Minification**: Remove null values and unnecessary data
- **Field Filtering**: Only return requested fields
- **Query Complexity**: Limit complex queries that can't be cached
- **Edge Computing**: Use Lambda@Edge for request/response optimization

### 4. Monitoring & Analytics
- **Real-time Metrics**: Monitor cache hit rates, latency, and error rates
- **Trend Analysis**: Track performance trends over time
- **Optimization Reports**: Generate actionable recommendations
- **User Experience**: Monitor real user metrics for edge performance

## Next Steps

- [Disaster Recovery](./disaster-recovery.md) - Backup and recovery strategies
- [Performance Tuning](./performance-tuning.md) - High-scale optimization
- [Security Guide](./security.md) - Comprehensive security implementation
