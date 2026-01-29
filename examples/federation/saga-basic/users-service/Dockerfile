FROM python:3.11-slim

WORKDIR /app

# Install dependencies
RUN pip install --no-cache-dir flask graphene psycopg2-binary

# Create simple GraphQL server
COPY server.py .

EXPOSE 4000

CMD ["python", "server.py"]
