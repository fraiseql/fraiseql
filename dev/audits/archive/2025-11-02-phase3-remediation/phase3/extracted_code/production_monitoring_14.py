# Extracted from: docs/production/monitoring.md
# Block number: 14
import json
import logging
from datetime import datetime


class StructuredFormatter(logging.Formatter):
    """JSON structured logging formatter."""

    def format(self, record):
        log_data = {
            "timestamp": datetime.utcnow().isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
            "module": record.module,
            "function": record.funcName,
            "line": record.lineno,
        }

        # Add extra fields
        if hasattr(record, "user_id"):
            log_data["user_id"] = record.user_id
        if hasattr(record, "query_id"):
            log_data["query_id"] = record.query_id
        if hasattr(record, "duration"):
            log_data["duration_ms"] = record.duration

        # Add exception info
        if record.exc_info:
            log_data["exception"] = self.formatException(record.exc_info)

        return json.dumps(log_data)


# Configure logging
logging.basicConfig(level=logging.INFO, handlers=[logging.StreamHandler()])

# Set formatter
for handler in logging.root.handlers:
    handler.setFormatter(StructuredFormatter())

logger = logging.getLogger(__name__)

# Usage
logger.info(
    "GraphQL query executed",
    extra={"user_id": "user-123", "query_id": "query-456", "duration": 125.5, "complexity": 45},
)
