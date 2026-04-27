FROM python:3.13-slim

WORKDIR /app

# Install dependencies
RUN pip install --no-cache-dir flask graphene mysql-connector-python

# Create non-root user
RUN groupadd -r app && useradd -r -g app app

# Create simple GraphQL server
COPY --chown=app:app server.py .

# Switch to non-root user
USER app

EXPOSE 4000

CMD ["python", "server.py"]
