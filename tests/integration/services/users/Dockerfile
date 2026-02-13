FROM python:3.12-slim

WORKDIR /app

# Install dependencies
RUN pip install --no-cache-dir \
    fraiseql \
    psycopg2-binary \
    pydantic \
    fastapi \
    uvicorn

# Copy schema
COPY schema.py /app/schema.py

# Copy startup script
COPY start.sh /app/start.sh
RUN chmod +x /app/start.sh

# Health check
HEALTHCHECK --interval=5s --timeout=3s --retries=5 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:4001/graphql')"

EXPOSE 4001

CMD ["/app/start.sh"]
