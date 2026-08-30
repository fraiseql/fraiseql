FROM python:3.11-slim

WORKDIR /app

# No pip install: this service runs on the standard library, so the image needs
# no package index at build time and no dependency resolution at start.
RUN groupadd -r app && useradd -r -g app app
COPY --chown=app:app server.py .
USER app

EXPOSE 4000

# The healthcheck is a stdlib probe. `curl -f` was the previous spelling and
# python:3.11-slim ships neither curl nor wget, so it could never report a
# failure (#1193).
HEALTHCHECK --interval=5s --timeout=3s --retries=10 \
  CMD ["python3", "-c", "import sys,urllib.request; sys.exit(0 if urllib.request.urlopen('http://localhost:4000/health', timeout=3).status == 200 else 1)"]

CMD ["python3", "server.py"]
