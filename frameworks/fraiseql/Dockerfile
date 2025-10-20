FROM python:3.13-slim

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Copy requirements
COPY requirements.txt .

# Install Python dependencies
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY . .

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=10s --timeout=3s \
  CMD curl -f http://localhost:8000/health || exit 1

# Start the application
CMD ["uvicorn", "benchmark_app:app", "--host", "0.0.0.0", "--port", "8000", "--workers", "4"]
