# Fraisier Monitoring

This directory contains monitoring and observability resources for Fraisier deployments.

## Prometheus Metrics

### Starting the Metrics Exporter

The Fraisier CLI includes a built-in Prometheus metrics exporter:

```bash
# Start metrics server on default port (localhost:8001)
fraisier metrics

# Use custom port
fraisier metrics --port 8080

# Listen on all interfaces
fraisier metrics --address 0.0.0.0
```

Metrics will be available at `http://<address>:<port>/metrics`

### Available Metrics

#### Counters (monotonically increasing)

- `fraisier_deployments_total` - Total deployments by provider, status, and fraise type
- `fraisier_deployment_errors_total` - Total deployment errors by provider and error type
- `fraisier_rollbacks_total` - Total rollbacks by provider and reason
- `fraisier_health_checks_total` - Total health checks by provider, type, and status

#### Histograms (distribution of values)

- `fraisier_deployment_duration_seconds` - Deployment duration distribution by provider and status
- `fraisier_health_check_duration_seconds` - Health check duration distribution by provider and type
- `fraisier_rollback_duration_seconds` - Rollback duration distribution by provider

#### Gauges (point-in-time values)

- `fraisier_active_deployments` - Currently active deployments by provider
- `fraisier_deployment_lock_wait_seconds` - Time waiting for deployment lock
- `fraisier_provider_availability` - Provider availability (1=available, 0=unavailable)

## Grafana Dashboards

### Installing the Dashboard

1. Open Grafana in your browser (e.g., http://localhost:3000)
2. Go to **Dashboards** → **Import**
3. Upload the `grafana-dashboard.json` file from this directory
4. Select your Prometheus data source
5. Click **Import**

### Dashboard Panels

The included dashboard provides comprehensive visibility into Fraisier deployments:

- **Deployment Rate** - Deployments per 5 minutes by provider and status
- **Deployment Success Rate** - Success rate percentage over 1 hour
- **Deployment Duration Percentiles** - 95th and 99th percentile deployment times
- **Deployment Errors by Type** - Error frequency by provider and error type
- **Active Deployments** - Real-time count of deployments in progress
- **Health Checks by Type** - Health check frequency by type and status
- **Health Check Duration** - 95th percentile health check times
- **Rollbacks by Reason** - Rollback frequency by reason

## Setup Guide

### Prerequisites

- Prometheus server (to scrape metrics)
- Grafana server (to visualize metrics)
- Fraisier with prometheus_client installed

### Installation Steps

#### 1. Install Prometheus Client

```bash
pip install prometheus-client
```

#### 2. Start Fraisier Metrics Exporter

```bash
fraisier metrics --address 0.0.0.0 --port 8001
```

#### 3. Configure Prometheus

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'fraisier'
    static_configs:
      - targets: ['localhost:8001']
```

#### 4. Restart Prometheus

```bash
systemctl restart prometheus
```

#### 5. Import Grafana Dashboard

Use the steps above to import `grafana-dashboard.json` into Grafana.

## Integration with Deployment Code

Fraisier automatically records metrics during deployments:

```python
from fraisier.metrics import get_metrics_recorder

# Metrics are automatically recorded by deployers
# No additional code needed in your fraises
```

## Alerting

### Recommended Alert Rules

#### High Error Rate

```yaml
alert: HighDeploymentErrorRate
expr: rate(fraisier_deployment_errors_total[5m]) > 0.1
for: 5m
```

#### Slow Deployments

```yaml
alert: SlowDeployment
expr: histogram_quantile(0.95, deployment_duration_seconds) > 300
for: 5m
```

#### Many Active Deployments

```yaml
alert: ManyActiveDeployments
expr: fraisier_active_deployments > 5
for: 5m
```

#### Deployment Failures

```yaml
alert: DeploymentFailure
expr: increase(fraisier_deployments_total{status="failed"}[5m]) > 0
for: 1m
```

## Troubleshooting

### Metrics Not Appearing

1. Verify Prometheus is scraping the metrics endpoint:
   - Check Prometheus targets: http://localhost:9090/targets
   - Should see `fraisier` job in "UP" status

2. Verify Fraisier metrics exporter is running:

   ```bash
   curl http://localhost:8001/metrics | head -20
   ```

3. Check for Prometheus client installation:

   ```bash
   python -c "import prometheus_client; print('OK')"
   ```

### Dashboard Not Showing Data

1. Verify data source is configured correctly:
   - Grafana → Configuration → Data Sources → Prometheus
   - Test data source connection

2. Verify Prometheus has scraped data:
   - Grafana → Explore → Query
   - Try: `fraisier_deployments_total`

3. Check time range:
   - Ensure time range in dashboard covers data collection period
   - Default is last 6 hours

## Performance Considerations

- Metrics collection has minimal overhead (<1% CPU)
- Prometheus memory usage scales with cardinality (number of label combinations)
- Recommend keeping 15 days of metrics retention
- Grafana dashboard loads in <1 second with typical data volumes

## References

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [Prometheus Client Python](https://github.com/prometheus/client_python)
