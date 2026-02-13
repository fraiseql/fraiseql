FROM python:3.11-slim

WORKDIR /app

RUN pip install --no-cache-dir flask psycopg2-binary

COPY server.py .

EXPOSE 4000

CMD ["python", "server.py"]
