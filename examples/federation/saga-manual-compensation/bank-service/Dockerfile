FROM python:3.11-slim

WORKDIR /app

RUN pip install --no-cache-dir flask psycopg2-binary

# Create non-root user
RUN groupadd -r app && useradd -r -g app app

COPY --chown=app:app server.py .

# Switch to non-root user
USER app

EXPOSE 4000

CMD ["python", "server.py"]
