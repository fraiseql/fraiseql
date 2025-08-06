FROM python:3.11-alpine

# Install PostgreSQL and required packages
RUN apk add --no-cache \
    postgresql15 \
    postgresql15-client \
    gcc \
    musl-dev \
    curl \
    supervisor \
    libpq-dev \
    python3-dev

# Set up PostgreSQL environment
ENV PGDATA=/var/lib/postgresql/data
ENV POSTGRES_USER=postgres
ENV POSTGRES_DB=benchmark_db

# Create PostgreSQL directories
RUN mkdir -p /var/run/postgresql && \
    chown -R postgres:postgres /var/run/postgresql && \
    mkdir -p $PGDATA && \
    chown -R postgres:postgres $PGDATA

# Set up Python environment
WORKDIR /app

# Copy and install FraiseQL requirements
COPY fraiseql/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy FraiseQL application code
COPY fraiseql/*.py ./

# Copy database initialization scripts
COPY shared/database/schema.sql /docker-entrypoint-initdb.d/01-schema.sql
COPY shared/database/seed-data.sql /docker-entrypoint-initdb.d/02-seed-data.sql

# Create supervisor configuration
RUN echo "[supervisord]\n\
nodaemon=true\n\
logfile=/var/log/supervisord.log\n\
\n\
[program:postgresql]\n\
command=/usr/lib/postgresql/15/bin/postgres -D /var/lib/postgresql/data -c unix_socket_directories='/var/run/postgresql'\n\
user=postgres\n\
autostart=true\n\
autorestart=true\n\
stdout_logfile=/var/log/postgresql.log\n\
stderr_logfile=/var/log/postgresql.err.log\n\
priority=1\n\
\n\
[program:fraiseql]\n\
command=python /app/app_socket.py\n\
directory=/app\n\
autostart=true\n\
autorestart=true\n\
startsecs=10\n\
startretries=3\n\
stdout_logfile=/var/log/fraiseql.log\n\
stderr_logfile=/var/log/fraiseql.err.log\n\
priority=2\n\
environment=DATABASE_URL=\"postgresql:///benchmark_db?host=/var/run/postgresql\",PYTHONUNBUFFERED=\"1\"" > /etc/supervisor/conf.d/supervisord.conf

# Create the app_socket.py that uses Unix socket
RUN echo "#!/usr/bin/env python3\n\
import os\n\
import time\n\
import asyncio\n\
import psycopg\n\
from fraiseql import create_app_from_db_url\n\
from fraiseql.core.graphql_type import create_graphql_type_from_table\n\
from typing import List, Dict, Any\n\
import uvicorn\n\
\n\
# Set socket-based DATABASE_URL\n\
os.environ['DATABASE_URL'] = 'postgresql:///benchmark_db?host=/var/run/postgresql&user=postgres'\n\
\n\
async def wait_for_postgres():\n\
    \"\"\"Wait for PostgreSQL to be ready via Unix socket.\"\"\"\n\
    print('Waiting for PostgreSQL to be ready...')\n\
    \n\
    for i in range(60):\n\
        try:\n\
            # Try to connect via Unix socket\n\
            conn = await psycopg.AsyncConnection.connect(\n\
                'postgresql:///benchmark_db?host=/var/run/postgresql&user=postgres'\n\
            )\n\
            await conn.execute('SELECT 1')\n\
            await conn.close()\n\
            print('PostgreSQL is ready!')\n\
            return True\n\
        except Exception as e:\n\
            if i % 10 == 0:\n\
                print(f'Still waiting for PostgreSQL... ({i}s)')\n\
            await asyncio.sleep(1)\n\
    \n\
    return False\n\
\n\
async def create_views():\n\
    \"\"\"Create necessary views for FraiseQL.\"\"\"\n\
    print('Creating FraiseQL views...')\n\
    conn = await psycopg.AsyncConnection.connect(\n\
        'postgresql:///benchmark_db?host=/var/run/postgresql&user=postgres'\n\
    )\n\
    \n\
    # Create user view\n\
    await conn.execute('''\n\
        CREATE OR REPLACE VIEW benchmark.v_users AS\n\
        SELECT \n\
            u.id,\n\
            u.email,\n\
            u.username,\n\
            u.full_name,\n\
            u.created_at,\n\
            u.is_active\n\
        FROM benchmark.users u\n\
    ''')\n\
    \n\
    # Create product view\n\
    await conn.execute('''\n\
        CREATE OR REPLACE VIEW benchmark.v_products AS\n\
        SELECT \n\
            p.id,\n\
            p.sku,\n\
            p.name,\n\
            p.description,\n\
            p.price,\n\
            p.stock_quantity,\n\
            p.category_id,\n\
            p.is_active\n\
        FROM benchmark.products p\n\
    ''')\n\
    \n\
    await conn.commit()\n\
    await conn.close()\n\
    print('Views created successfully!')\n\
\n\
async def main():\n\
    if not await wait_for_postgres():\n\
        print('Failed to connect to PostgreSQL')\n\
        exit(1)\n\
    \n\
    await create_views()\n\
    \n\
    # Create the FraiseQL app\n\
    app = create_app_from_db_url(\n\
        'postgresql:///benchmark_db?host=/var/run/postgresql&user=postgres',\n\
        schema='benchmark'\n\
    )\n\
    \n\
    # Add health endpoint\n\
    @app.get('/health')\n\
    async def health():\n\
        return {'status': 'ok'}\n\
    \n\
    print('Starting FraiseQL application on Unix socket...')\n\
    config = uvicorn.Config(app, host='0.0.0.0', port=8000, log_level='info')\n\
    server = uvicorn.Server(config)\n\
    await server.serve()\n\
\n\
if __name__ == '__main__':\n\
    asyncio.run(main())" > /app/app_socket.py

# Create startup script
RUN echo "#!/bin/bash\n\
set -e\n\
\n\
# Initialize PostgreSQL if needed\n\
if [ ! -s \"$PGDATA/PG_VERSION\" ]; then\n\
    echo \"Initializing PostgreSQL...\"\n\
    su - postgres -c \"/usr/lib/postgresql/15/bin/initdb -D $PGDATA --auth-local trust --auth-host trust\"\n\
    \n\
    # Start PostgreSQL temporarily for setup\n\
    su - postgres -c \"/usr/lib/postgresql/15/bin/pg_ctl -D $PGDATA -o '-c unix_socket_directories=/var/run/postgresql' -l /tmp/pg_init.log start\"\n\
    \n\
    # Wait for PostgreSQL to start\n\
    sleep 5\n\
    \n\
    # Create database\n\
    su - postgres -c \"psql -h /var/run/postgresql -c 'CREATE DATABASE benchmark_db;'\"\n\
    \n\
    # Run initialization scripts\n\
    echo \"Loading schema...\"\n\
    su - postgres -c \"psql -h /var/run/postgresql -d benchmark_db -f /docker-entrypoint-initdb.d/01-schema.sql\"\n\
    \n\
    echo \"Loading seed data (this may take a few minutes)...\"\n\
    su - postgres -c \"psql -h /var/run/postgresql -d benchmark_db -f /docker-entrypoint-initdb.d/02-seed-data.sql\"\n\
    \n\
    # Stop PostgreSQL\n\
    su - postgres -c \"/usr/lib/postgresql/15/bin/pg_ctl -D $PGDATA stop\"\n\
    \n\
    echo \"PostgreSQL initialization complete!\"\n\
fi\n\
\n\
# Start supervisor\n\
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf" > /start.sh && \
    chmod +x /start.sh

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=10s --start-period=120s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

CMD ["/start.sh"]
